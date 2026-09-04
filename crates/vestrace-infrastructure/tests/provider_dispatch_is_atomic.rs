use std::{
    alloc::{GlobalAlloc, Layout, System},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ConfiguredCapabilityPolicyEngine, ConfiguredProviderDispatchPolicyEvaluator,
    ConnectionAuth, ConnectionKind, CredentialDispatchLease, CredentialDispatchLeaseRepository,
    CredentialDispatchLeaseRequest, CredentialMaterialPreparationRequest,
    CredentialMaterialPreparer, EffectiveChatEvidence, EffectiveChatFinishReason,
    EffectiveChatMessage, EffectiveChatResult, EffectiveModelRequest, EffectiveRequestLimits,
    EffectiveSampling, ExternalEffectRepository, FinalizeProviderResult, IdempotencyRecord,
    InstallationMutationPermit, ModelDataPolicyDecisionRecord, ModelDataPolicyMode,
    ModelDataPolicySettings, ModelRequestEvidenceRepository, ModelRequestReconstruction,
    OperatorCredential, OutboxMessage, PermitMode, PrepareProviderResult, ProviderDispatchCause,
    ProviderDispatchFaultInjector, ProviderDispatchFaultPoint, ProviderDispatchOutcome,
    ProviderDispatchPolicyEvaluation, ProviderDispatchPolicyEvaluator, ProviderDispatchRepository,
    ProviderDispatchRequest, ProviderDispatchTarget, ProviderLostDispatchRecovery,
    ProviderPostNetworkCompletion, ProviderResultFaultInjector, ProviderResultFaultPoint,
    ProviderResultIdentities, ProviderResultRepository, ProviderUsage, Q1ChatMessage,
    Q1ModelsListProbeResult, Q1ProbeFailure, Q1ProbeRequest, Q1ProbeResponse,
    QualificationJobRepository, QualificationJobRequest, QualificationJobService,
    QualificationProbeCompletion, RequestContext, RunStepAttemptRecovery,
    SharedProviderDispatchRepository, UnitOfWork,
};
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{
    AgentRunId, ArtifactId, ArtifactRevisionId, AuditEvent, AuthorizationRequest, Capability,
    ConnectionId, ConnectionRevisionId, ContentMaterialId, CredentialKeyCreationIntentId,
    CredentialPreparedAttachmentId, CredentialRevisionId, CredentialSlotId, DataDestination,
    DeliverySemantics, EffectPrecondition, EffectReversibility, ExternalEffectIntent,
    ExternalEffectReceipt, ExternalEffectReceiptId, IdempotencyProfile, IntentNonce,
    MaterialKeyBindingReceipt, MaterialKeyCreationIntentId, MaterialKeyId, ModelExecutionId,
    ModelRequestEvidenceId, NoAuthBindingRevisionId, PolicyDecision, PolicyDecisionReason,
    PolicyDecisionResult, PolicyInputState, PreparedMaterialAttachmentId, PrincipalId,
    QualificationJobId, QualificationProbeResult, RiskCategory, RunStepId, Sensitivity, WorkItemId,
    WorkerId, WorkspaceId, ZeroizingDek,
    id::{AuditEventId, OutboxId, PolicyDecisionId},
};
use vestrace_infrastructure::crypto::ContentMaterialCodec;
use vestrace_infrastructure::postgres::{
    PgCredentialDispatchLeaseRepository, PgCredentialMaterialPreparer, PgExternalEffectRepository,
    PgGovernedMutationRepository, PgInstallationMutationPermit,
    PgModelDataPolicyDecisionRepository, PgModelRequestEvidenceRepository,
    PgProviderDispatchRepository, PgProviderResultRepository, PgQualificationJobRepository,
    PgQualificationProbeRunner, PgStore, PostgresRunStore, QualificationQ1Adapter,
};

struct ObservingAllocator;

static TRACKED_CONTENT_POINTER: AtomicUsize = AtomicUsize::new(0);
static TRACKED_DEALLOCATION_OBSERVED: AtomicBool = AtomicBool::new(false);
static TRACKED_DEALLOCATION_ZERO: AtomicBool = AtomicBool::new(false);
static TRACKED_CREDENTIAL_POINTER: AtomicUsize = AtomicUsize::new(0);
static TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED: AtomicBool = AtomicBool::new(false);
static TRACKED_CREDENTIAL_DEALLOCATION_ZERO: AtomicBool = AtomicBool::new(false);
static TRACKING_TEST_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const TRACKED_CONTENT_BYTES: &[u8] = b"dispatch secret sentinel";
const TRACKED_CREDENTIAL_BYTES: &[u8] = b"real-provider-credential-sentinel";

// SAFETY: every operation is forwarded to the process allocator unchanged.
// The observer only reads the exact tracked allocation before forwarding its
// deallocation and performs atomic writes; it allocates and logs nothing.
unsafe impl GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if TRACKED_CONTENT_POINTER.load(Ordering::SeqCst) == pointer as usize {
            let mut all_zero = true;
            for offset in 0..TRACKED_CONTENT_BYTES.len() {
                if unsafe { pointer.add(offset).read() } != 0 {
                    all_zero = false;
                    break;
                }
            }
            TRACKED_DEALLOCATION_ZERO.store(all_zero, Ordering::SeqCst);
            TRACKED_DEALLOCATION_OBSERVED.store(true, Ordering::SeqCst);
            TRACKED_CONTENT_POINTER.store(0, Ordering::SeqCst);
        }
        if TRACKED_CREDENTIAL_POINTER.load(Ordering::SeqCst) == pointer as usize {
            let mut all_zero = true;
            for offset in 0..TRACKED_CREDENTIAL_BYTES.len() {
                if unsafe { pointer.add(offset).read() } != 0 {
                    all_zero = false;
                    break;
                }
            }
            TRACKED_CREDENTIAL_DEALLOCATION_ZERO.store(all_zero, Ordering::SeqCst);
            TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.store(true, Ordering::SeqCst);
            TRACKED_CREDENTIAL_POINTER.store(0, Ordering::SeqCst);
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: ObservingAllocator = ObservingAllocator;

fn track_content_deallocation(pointer: *const u8) {
    TRACKED_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
    TRACKED_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
    TRACKED_CONTENT_POINTER.store(pointer as usize, Ordering::SeqCst);
}

fn track_credential_deallocation(pointer: *const u8) {
    TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
    TRACKED_CREDENTIAL_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
    TRACKED_CREDENTIAL_POINTER.store(pointer as usize, Ordering::SeqCst);
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url).unwrap();
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("runtime must connect to the SQLx test database")
}

async fn runtime_pool_single(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url).unwrap();
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("single-session runtime pool must connect to the SQLx test database")
}

async fn wait_for_blocker(pool: &PgPool, waiting_pid: i32, expected_blocker: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blockers: Vec<i32> = sqlx::query_scalar("SELECT unnest(pg_blocking_pids($1))")
                .bind(waiting_pid)
                .fetch_all(pool)
                .await
                .unwrap();
            if blockers.contains(&expected_blocker) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("session never reached the required observed lock wait");
}

#[derive(Clone, Copy)]
struct Ids {
    workspace: WorkspaceId,
    principal: PrincipalId,
    run: AgentRunId,
    step: RunStepId,
    connection: ConnectionId,
    revision: ConnectionRevisionId,
    snapshot: Uuid,
    evidence: ModelRequestEvidenceId,
}

/// The exact cause and pinned target a direct policy evaluation must be given
/// when it is called outside `prepare_dispatch`, which normally supplies both.
fn run_step_cause(fixture: &Fixture) -> ProviderDispatchCause {
    ProviderDispatchCause::RunStep {
        run_id: fixture.ids.run,
        step_id: fixture.ids.step,
        snapshot_id: fixture.ids.snapshot,
    }
}

fn pinned_target() -> ProviderDispatchTarget {
    ProviderDispatchTarget {
        kind: vestrace_domain::ConnectionKind::LMStudioLocal,
        runtime_base_url: "http://127.0.0.1:1234/v1".to_owned(),
        destination: vestrace_domain::DataDestination::LocalModel,
    }
}

struct Fixture {
    ids: Ids,
    intent: ExternalEffectIntent,
    credential: Option<CredentialFixture>,
    chat_model_revision_id: Uuid,
    embedding_model_revision_id: Uuid,
}

#[derive(Clone, Copy)]
struct CredentialFixture {
    slot: CredentialSlotId,
    revision: Uuid,
    activation_guard: Uuid,
    material_key: Uuid,
    creation_intent: Uuid,
    nonce: Uuid,
    occupancy: Uuid,
    prepared_attachment: Uuid,
    preparation_creates: usize,
    preparation_unwraps: usize,
}

fn new_intent(ids: Ids) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        format!("run://{}", ids.run),
        ids.workspace,
        ids.principal,
        "openai-compatible",
        "chat",
        "https://provider.atomic.test/v1/chat/completions",
        "sha256:arguments",
        "produce a governed answer",
        vec![EffectPrecondition::new("model-snapshot", ids.snapshot.to_string()).unwrap()],
        "sha256:preconditions",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        chrono::Utc::now(),
    )
    .unwrap()
}

async fn set_context(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, ids: Ids) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace.to_string())
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(ids.principal.to_string())
        .fetch_one(&mut **tx)
        .await
        .unwrap();
}

async fn reserve_run_step_execution_attempt(runtime: &PgPool, fixture: &Fixture) -> Uuid {
    let attempt_id = Uuid::now_v7();
    let input_identity = [
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    ];
    let mut reserve = runtime.begin().await.unwrap();
    set_context(&mut reserve, fixture.ids).await;
    let reserved: Uuid = sqlx::query_scalar(
        "SELECT vestrace_reserve_run_step_execution_attempt(\
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(attempt_id)
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(fixture.ids.snapshot)
    .bind(input_identity[0])
    .bind(input_identity[1])
    .bind(input_identity[2])
    .bind(input_identity[3])
    .bind(input_identity[4])
    .bind(fixture.intent.id().as_uuid())
    .bind(fixture.ids.evidence.as_uuid())
    .fetch_one(&mut *reserve)
    .await
    .unwrap();
    assert_eq!(reserved, attempt_id);
    reserve.commit().await.unwrap();
    attempt_id
}

async fn reserve_run_step_execution_attempt_with_identity(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ids: Ids,
    step_id: RunStepId,
    attempt_id: Uuid,
    identities: [Uuid; 7],
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT vestrace_reserve_run_step_execution_attempt(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(attempt_id)
    .bind(ids.workspace.as_uuid())
    .bind(ids.run.as_uuid())
    .bind(step_id.as_uuid())
    .bind(ids.snapshot)
    .bind(identities[0])
    .bind(identities[1])
    .bind(identities[2])
    .bind(identities[3])
    .bind(identities[4])
    .bind(identities[5])
    .bind(identities[6])
    .fetch_one(&mut **transaction)
    .await
}

async fn run_step_execution_attempt_id(pool: &PgPool, fixture: &Fixture) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM run_step_execution_attempts
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn setup(owner: &PgPool, runtime: &PgPool) -> Fixture {
    setup_run_branch(owner, runtime, false).await
}

async fn setup_run_branch(owner: &PgPool, runtime: &PgPool, credential_backed: bool) -> Fixture {
    let fixture = setup_branch(owner, runtime, credential_backed).await;
    reserve_run_step_execution_attempt(runtime, &fixture).await;
    fixture
}

async fn setup_branch(owner: &PgPool, runtime: &PgPool, credential_backed: bool) -> Fixture {
    let ids = Ids {
        workspace: WorkspaceId::new(),
        principal: PrincipalId::new(),
        run: AgentRunId::new(),
        step: RunStepId::new(),
        connection: ConnectionId::new(),
        revision: ConnectionRevisionId::new(),
        snapshot: Uuid::now_v7(),
        evidence: ModelRequestEvidenceId::new(),
    };
    let connector = Uuid::now_v7();
    let guard = Uuid::now_v7();
    let no_auth = Uuid::now_v7();
    let provider = Uuid::now_v7();
    let model = Uuid::now_v7();
    let embedding_model = Uuid::now_v7();
    let model_revision = Uuid::now_v7();
    let embedding_model_revision = Uuid::now_v7();
    let job = QualificationJobId::new();
    let connection_qualification = Uuid::now_v7();
    let model_qualification = Uuid::now_v7();
    let policy = Uuid::now_v7();
    let shape = Uuid::now_v7();
    let sampling = Uuid::now_v7();
    let limits = Uuid::now_v7();
    let input_material = ContentMaterialId::new();
    let input_key = MaterialKeyId::new();
    let input_intent = Uuid::now_v7();
    let mut credential = credential_backed.then(|| CredentialFixture {
        slot: CredentialSlotId::new(),
        revision: Uuid::now_v7(),
        activation_guard: Uuid::now_v7(),
        material_key: Uuid::now_v7(),
        creation_intent: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        occupancy: Uuid::now_v7(),
        prepared_attachment: Uuid::now_v7(),
        preparation_creates: 0,
        preparation_unwraps: 0,
    });

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(ids.workspace.as_uuid())
        .bind(format!("dispatch-{}", ids.workspace))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(ids.principal.as_uuid())
        .bind(ids.workspace.as_uuid())
        .bind(format!("dispatch-{}", ids.principal))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector)
    .bind(ids.workspace.as_uuid())
    .bind(format!("dispatch-{connector}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,$5,'active')")
        .bind(ids.connection.as_uuid()).bind(connector).bind(ids.workspace.as_uuid()).bind(ids.principal.as_uuid())
        .bind(format!("dispatch-{}", ids.connection)).execute(owner).await.unwrap();
    sqlx::query("INSERT INTO agent_runs(id,workspace_id,principal_id,title,objective,coordinator_snapshot_id,status,run_version) VALUES($1,$2,$3,'provider','provider',$4,'running',1)")
        .bind(ids.run.as_uuid()).bind(ids.workspace.as_uuid()).bind(ids.principal.as_uuid()).bind(ids.snapshot).execute(owner).await.unwrap();
    sqlx::query("INSERT INTO run_steps(id,run_id,workspace_id,step_number,title,status,assigned_actor) VALUES($1,$2,$3,1,'provider','running','\"system\"'::jsonb)")
        .bind(ids.step.as_uuid()).bind(ids.run.as_uuid()).bind(ids.workspace.as_uuid()).execute(owner).await.unwrap();
    sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'provider','remote')",
    )
    .bind(provider)
    .bind(ids.workspace.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    for (model_id, model_name) in [(model, "model"), (embedding_model, "embedding-model")] {
        sqlx::query("INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,$4,4096,0,0)")
            .bind(model_id).bind(provider).bind(ids.workspace.as_uuid()).bind(model_name).execute(owner).await.unwrap();
    }

    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, ids).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard)
        .bind(ids.workspace.as_uuid())
        .bind(ids.connection.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut credential_preparation_counts = None;
    if let Some(credential) = credential {
        sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider','primary')")
            .bind(credential.slot.as_uuid())
            .bind(ids.workspace.as_uuid())
            .bind(ids.connection.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
            .bind(credential.activation_guard)
            .bind(ids.workspace.as_uuid())
            .bind(ids.connection.as_uuid())
            .bind(credential.slot.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1,$2,$3,$4)")
            .bind(credential.occupancy)
            .bind(ids.workspace.as_uuid())
            .bind(ids.connection.as_uuid())
            .bind(credential.slot.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(
                $1,$2,$3,$4,$5,$6,$7,$8,'credential_v2')",
        )
        .bind(credential.creation_intent)
        .bind(ids.workspace.as_uuid())
        .bind(ids.connection.as_uuid())
        .bind(credential.slot.as_uuid())
        .bind(credential.occupancy)
        .bind(credential.revision)
        .bind(credential.material_key)
        .bind(credential.nonce)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let preparation_vault = Arc::new(PreparationCountingVault {
            creates: AtomicUsize::new(0),
            unwraps: AtomicUsize::new(0),
        });
        let prepared = PgCredentialMaterialPreparer::new(
            Arc::new(PgInstallationMutationPermit::new(PgStore::from_pool(
                runtime.clone(),
            ))),
            preparation_vault.clone(),
        )
        .prepare(
            &RequestContext::new(ids.workspace, ids.principal),
            CredentialMaterialPreparationRequest {
                connection_id: ids.connection,
                credential_slot_id: credential.slot,
                credential_revision_id: CredentialRevisionId::from_uuid(credential.revision),
                material_key_id: MaterialKeyId::from_uuid(credential.material_key),
                intent_id: CredentialKeyCreationIntentId::from_uuid(credential.creation_intent),
                intent_nonce: IntentNonce::from_uuid(credential.nonce),
                prepared_attachment_id: CredentialPreparedAttachmentId::from_uuid(
                    credential.prepared_attachment,
                ),
                credential: OperatorCredential::new("real-provider-credential-sentinel"),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            prepared.intent_id.as_uuid(),
            credential.creation_intent,
            "preparer returned another credential intent"
        );
        let preparation_counts = preparation_vault.counts();

        let mut candidate = runtime.begin().await.unwrap();
        set_context(&mut candidate, ids).await;
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1,$2)")
            .bind(credential.creation_intent)
            .bind(Uuid::now_v7())
            .execute(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(credential.creation_intent)
            .execute(&mut *candidate)
            .await
            .unwrap();
        candidate.commit().await.unwrap();

        let mut credential_tx = owner.begin().await.unwrap();
        set_context(&mut credential_tx, ids).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *credential_tx)
            .await
            .unwrap();
        sqlx::query("UPDATE credential_guard_occupancies SET state='activated' WHERE id=$1")
            .bind(credential.occupancy)
            .execute(&mut *credential_tx)
            .await
            .unwrap();
        sqlx::query("UPDATE credential_key_creation_intents SET state='active' WHERE id=$1")
            .bind(credential.creation_intent)
            .execute(&mut *credential_tx)
            .await
            .unwrap();
        sqlx::query("UPDATE credential_slots SET current_revision_id=$2,current_revision_version=1 WHERE id=$1")
            .bind(credential.slot.as_uuid()).bind(credential.revision).execute(&mut *credential_tx).await.unwrap();
        credential_tx.commit().await.unwrap();
        credential_preparation_counts = Some(preparation_counts);
        tx = runtime.begin().await.unwrap();
        set_context(&mut tx, ids).await;
    }
    if let (Some(credential), Some((creates, unwraps))) =
        (credential.as_mut(), credential_preparation_counts)
    {
        credential.preparation_creates = creates;
        credential.preparation_unwraps = unwraps;
    }
    let (kind, logical_url, runtime_url, transport, auth_mode, slot) =
        if let Some(credential) = credential {
            (
                "open_ai_chat_completions_v1",
                "https://api.example.test/v1",
                "https://api.example.test/v1",
                "remote_https",
                "bearer",
                Some(credential.slot.as_uuid()),
            )
        } else {
            (
                "lm_studio_local",
                "http://127.0.0.1:1234/v1",
                "http://127.0.0.1:1234/v1",
                "loopback_only",
                "none",
                None,
            )
        };
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,'q1',$8,$9,$10,0)")
        .bind(ids.revision.as_uuid()).bind(ids.workspace.as_uuid()).bind(ids.connection.as_uuid()).bind(guard)
        .bind(kind).bind(logical_url).bind(runtime_url).bind(transport).bind(auth_mode).bind(slot)
        .fetch_one(&mut *tx).await.unwrap();
    if credential.is_none() {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)",
        )
        .bind(no_auth)
        .bind(ids.workspace.as_uuid())
        .bind(ids.connection.as_uuid())
        .bind(ids.revision.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,'model','chat',NULL,NULL,NULL,NULL,NULL,NULL,0)")
        .bind(model_revision).bind(ids.workspace.as_uuid()).bind(model).bind(ids.connection.as_uuid()).bind(guard)
        .bind(ids.revision.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,'embedding-model','embedding',NULL,NULL,NULL,NULL,NULL,NULL,0)")
        .bind(embedding_model_revision).bind(ids.workspace.as_uuid()).bind(embedding_model).bind(ids.connection.as_uuid()).bind(guard)
        .bind(ids.revision.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    let frame = ContentMaterialCodec::new()
        .seal(
            ids.workspace,
            input_material,
            input_key,
            &ZeroizingDek::new([0x4B; 32]),
            b"dispatch secret sentinel",
        )
        .unwrap();
    let mut semantic = runtime.begin().await.unwrap();
    set_context(&mut semantic, ids).await;
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_request_shape_revision($1,$2,1,'chat_completions',false,ARRAY['user']::TEXT[])")
        .bind(shape).bind(ids.workspace.as_uuid()).fetch_one(&mut *semantic).await.unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_sampling_revision($1,$2,1,0.2,1.0)",
    )
    .bind(sampling)
    .bind(ids.workspace.as_uuid())
    .fetch_one(&mut *semantic)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_limits_revision($1,$2,1,64,1,1024)",
    )
    .bind(limits)
    .bind(ids.workspace.as_uuid())
    .fetch_one(&mut *semantic)
    .await
    .unwrap();
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)")
        .bind(input_intent).bind(ids.workspace.as_uuid()).bind(input_material.as_uuid()).bind(input_key.as_uuid())
        .bind(Uuid::now_v7()).bind(ids.step.as_uuid()).execute(&mut *semantic).await.unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(input_intent)
        .execute(&mut *semantic)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(input_intent)
        .bind(Uuid::now_v7())
        .execute(&mut *semantic)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,$4)")
        .bind(input_intent)
        .bind(Uuid::now_v7())
        .bind(&frame)
        .bind(i64::try_from(frame.len()).unwrap())
        .execute(&mut *semantic)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(input_intent)
        .bind(Uuid::now_v7())
        .execute(&mut *semantic)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(input_intent)
        .execute(&mut *semantic)
        .await
        .unwrap();
    semantic.commit().await.unwrap();

    let intent = new_intent(ids);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&RequestContext::new(ids.workspace, ids.principal), &intent)
        .await
        .unwrap();

    let check = Uuid::now_v7();
    let mut tx = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace.to_string())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())")
        .bind(job.as_uuid()).bind(ids.workspace.as_uuid()).bind(ids.revision.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,valid_until,capabilities) VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',ARRAY['chat']::TEXT[])")
        .bind(connection_qualification).bind(ids.workspace.as_uuid()).bind(ids.revision.as_uuid()).bind(job.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) VALUES($1,$2,$3,$4,$5,$6,ARRAY['chat']::TEXT[],NOW()+INTERVAL '1 hour')")
        .bind(model_qualification).bind(ids.workspace.as_uuid()).bind(model_revision).bind(ids.revision.as_uuid())
        .bind(connection_qualification).bind(job.as_uuid()).execute(&mut *tx).await.unwrap();
    if let Some(credential) = credential {
        sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,expected_slot_version) VALUES($1,$2,$3,$4,$5,$6,$7,'credential',$8,$9,$10,1)")
            .bind(ids.snapshot).bind(ids.workspace.as_uuid()).bind(ids.connection.as_uuid()).bind(ids.revision.as_uuid())
            .bind(connection_qualification).bind(model_revision).bind(model_qualification).bind(credential.revision).bind(credential.slot.as_uuid()).bind(credential.activation_guard).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
            .bind(ids.workspace.as_uuid()).bind(ids.snapshot).execute(&mut *tx).await.unwrap();
    } else {
        sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)")
            .bind(ids.snapshot).bind(ids.workspace.as_uuid()).bind(ids.connection.as_uuid()).bind(ids.revision.as_uuid())
            .bind(connection_qualification).bind(model_revision).bind(model_qualification).bind(no_auth).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
            .bind(ids.workspace.as_uuid()).bind(ids.snapshot).execute(&mut *tx).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO run_model_binding_snapshots(workspace_id,run_id,snapshot_id) VALUES($1,$2,$3)",
    )
    .bind(ids.workspace.as_uuid())
    .bind(ids.run.as_uuid())
    .bind(ids.snapshot)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_revisions(id,workspace_id,connection_id,version,max_in_flight,requests_per_60_seconds,queue_wait_timeout_seconds,provider_throttle_cap_seconds) VALUES($1,$2,$3,1,1,60000,30,900)")
        .bind(policy).bind(ids.workspace.as_uuid()).bind(ids.connection.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_heads(workspace_id,connection_id,current_policy_revision_id,version) VALUES($1,$2,$3,1)")
        .bind(ids.workspace.as_uuid()).bind(ids.connection.as_uuid()).bind(policy).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) VALUES($1,$2,$3,'chat_completions',$4,'run_step',$5)")
        .bind(ids.evidence.as_uuid()).bind(ids.workspace.as_uuid()).bind(intent.id().as_uuid()).bind(ids.snapshot).bind(ids.step.as_uuid()).execute(&mut *tx).await.unwrap();
    let kinds = [
        "external_effect",
        "binding_snapshot",
        "connection_revision",
        "connection_qualification_revision",
        "model_revision",
        "model_qualification_revision",
        "governed_input_material",
        "request_shape_revision",
        "sampling_revision",
        "limits_revision",
    ];
    let refs = [
        intent.id().as_uuid(),
        ids.snapshot,
        ids.revision.as_uuid(),
        connection_qualification,
        model_revision,
        model_qualification,
        input_material.as_uuid(),
        shape,
        sampling,
        limits,
    ];
    for (ordinal, (kind, reference)) in kinds.iter().zip(refs).enumerate() {
        let version = matches!(
            *kind,
            "request_shape_revision" | "sampling_revision" | "limits_revision"
        )
        .then_some(1_i64);
        let safe_ordinal = (*kind == "governed_input_material").then_some("0");
        sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,reference_version,safe_ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(Uuid::now_v7()).bind(ids.workspace.as_uuid()).bind(ids.evidence.as_uuid()).bind(ordinal as i32)
            .bind(kind).bind(reference).bind(version).bind(safe_ordinal).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,missing_reference_count) VALUES($1,$2,$3,'complete',0)")
        .bind(check).bind(ids.workspace.as_uuid()).bind(ids.evidence.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    Fixture {
        ids,
        intent,
        credential,
        chat_model_revision_id: model_revision,
        embedding_model_revision_id: embedding_model_revision,
    }
}

struct QualificationFixture {
    fixture: Fixture,
    job: QualificationJobId,
    target: Uuid,
}

async fn setup_qualification_branch(
    owner: &PgPool,
    runtime: &PgPool,
    credential_backed: bool,
) -> QualificationFixture {
    // Ordinal 00 is a local static validation.  A fixture that reaches the
    // governed provider-dispatch path must be one of the network ordinals.
    setup_qualification_branch_for_ordinal(owner, runtime, credential_backed, "10").await
}

async fn setup_qualification_branch_for_ordinal(
    owner: &PgPool,
    runtime: &PgPool,
    credential_backed: bool,
    probe_ordinal: &str,
) -> QualificationFixture {
    let base = setup_branch(owner, runtime, credential_backed).await;
    let job = QualificationJobId::new();
    let target = Uuid::now_v7();
    let shape = Uuid::now_v7();
    let limits = Uuid::now_v7();
    let evidence = ModelRequestEvidenceId::new();
    let mut ids = base.ids;
    ids.evidence = evidence;
    let intent = new_intent(ids);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&RequestContext::new(ids.workspace, ids.principal), &intent)
        .await
        .unwrap();

    let mut semantic = runtime.begin().await.unwrap();
    set_context(&mut semantic, ids).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(\
         $1,$2,1,'models_list',false,ARRAY[]::TEXT[])",
    )
    .bind(shape)
    .bind(ids.workspace.as_uuid())
    .fetch_one(&mut *semantic)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,1,1,1)")
        .bind(limits)
        .bind(ids.workspace.as_uuid())
        .fetch_one(&mut *semantic)
        .await
        .unwrap();
    semantic.commit().await.unwrap();

    let mut authority = owner.begin().await.unwrap();
    set_context(&mut authority, ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *authority)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) VALUES($1,$2,$3,'q1','running')")
        .bind(job.as_uuid()).bind(ids.workspace.as_uuid()).bind(ids.revision.as_uuid())
        .execute(&mut *authority).await.unwrap();
    if let Some(credential) = base.credential {
        sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,expected_slot_version,chat_model_revision_id,embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'credential',$6,$7,$8,1,$9,$10)")
            .bind(target).bind(ids.workspace.as_uuid()).bind(job.as_uuid())
            .bind(ids.connection.as_uuid()).bind(ids.revision.as_uuid())
            .bind(credential.revision).bind(credential.slot.as_uuid()).bind(credential.activation_guard)
            .bind(base.chat_model_revision_id).bind(base.embedding_model_revision_id)
            .execute(&mut *authority).await.unwrap();
    } else {
        let no_auth: Uuid = sqlx::query_scalar(
            "SELECT no_auth_binding_revision_id FROM model_binding_snapshots WHERE id=$1",
        )
        .bind(ids.snapshot)
        .fetch_one(&mut *authority)
        .await
        .unwrap();
        sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id,chat_model_revision_id,embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7,$8)")
            .bind(target).bind(ids.workspace.as_uuid()).bind(job.as_uuid())
            .bind(ids.connection.as_uuid()).bind(ids.revision.as_uuid()).bind(no_auth)
            .bind(base.chat_model_revision_id).bind(base.embedding_model_revision_id)
            .execute(&mut *authority).await.unwrap();
    }
    authority.commit().await.unwrap();

    let kinds = vec![
        "external_effect",
        "connection_revision",
        "qualification_target",
        "qualification_probe",
        "request_shape_revision",
        "limits_revision",
    ];
    let references = vec![
        intent.id().as_uuid(),
        ids.revision.as_uuid(),
        target,
        job.as_uuid(),
        shape,
        limits,
    ];
    let versions = vec![None, None, None, None, Some(1_i64), Some(1_i64)];
    let ordinals = vec![None, None, None, Some(probe_ordinal), None, None];
    let request_kind = match probe_ordinal {
        "10" => "models_list",
        "90" => "embeddings",
        "20" | "30" | "35" | "40" | "50" | "60" | "70" | "80" => "chat_completions",
        _ => panic!("unknown q1 network ordinal {probe_ordinal}"),
    };
    let mut evidence_tx = runtime.begin().await.unwrap();
    set_context(&mut evidence_tx, ids).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(\
         $1,$2,$3,$4,NULL,$5,'qualification_probe',$6,$7,$8,$9,$10)",
    )
    .bind(evidence.as_uuid())
    .bind(ids.workspace.as_uuid())
    .bind(intent.id().as_uuid())
    .bind(request_kind)
    .bind(target)
    .bind(job.as_uuid())
    .bind(&kinds)
    .bind(&references)
    .bind(&versions)
    .bind(&ordinals)
    .fetch_one(&mut *evidence_tx)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(\
         $1,$2,$3,'complete',ARRAY[]::TEXT[],ARRAY[]::UUID[],ARRAY[]::UUID[],ARRAY[]::UUID[])",
    )
    .bind(Uuid::now_v7())
    .bind(ids.workspace.as_uuid())
    .bind(evidence.as_uuid())
    .fetch_one(&mut *evidence_tx)
    .await
    .unwrap();
    let (
        message_layout,
        tool_choice,
        parallel_tool_calls,
        response_format,
        stream,
        include_usage,
        replay,
    ) = q1_source_fields(probe_ordinal);
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_qualification_q1_mre_source(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(evidence.as_uuid())
    .bind(ids.workspace.as_uuid())
    .bind(probe_ordinal)
    .bind(message_layout)
    .bind(tool_choice)
    .bind(parallel_tool_calls)
    .bind(response_format)
    .bind(stream)
    .bind(include_usage)
    .bind(replay)
    .fetch_one(&mut *evidence_tx)
    .await
    .unwrap();
    evidence_tx.commit().await.unwrap();

    QualificationFixture {
        fixture: Fixture {
            ids,
            intent,
            credential: base.credential,
            chat_model_revision_id: base.chat_model_revision_id,
            embedding_model_revision_id: base.embedding_model_revision_id,
        },
        job,
        target,
    }
}

fn q1_source_fields(
    ordinal: &str,
) -> (
    &'static str,
    &'static str,
    bool,
    &'static str,
    bool,
    bool,
    Option<&'static str>,
) {
    match ordinal {
        "10" | "20" | "90" => ("plain_text", "none", false, "none", false, false, None),
        "30" => ("plain_text", "none", false, "none", true, false, None),
        "35" => ("plain_text", "none", false, "none", true, true, None),
        "40" => (
            "plain_text",
            "named_probe",
            false,
            "none",
            false,
            false,
            None,
        ),
        "50" => (
            "assistant_tool_call_replay",
            "none",
            false,
            "none",
            false,
            false,
            Some("call_q1"),
        ),
        "60" => ("plain_text", "required", true, "none", false, false, None),
        "70" => (
            "plain_text",
            "none",
            false,
            "strict_nonce_json_schema",
            false,
            false,
            None,
        ),
        "80" => (
            "multipart_image_marker",
            "none",
            false,
            "none",
            false,
            false,
            None,
        ),
        _ => panic!("unknown q1 network ordinal {ordinal}"),
    }
}

async fn insert_finalizable_q1_network_probe(
    pool: &PgPool,
    qualification: &QualificationFixture,
    ordinal: &str,
) -> (Uuid, Uuid) {
    let effect_id = Uuid::now_v7();
    let evidence_id = Uuid::now_v7();
    let evidence_check_id = Uuid::now_v7();
    let receipt_id = Uuid::now_v7();
    let request_kind = match ordinal {
        "10" => "models_list",
        "90" => "embeddings",
        "20" | "30" | "35" | "40" | "50" | "60" | "70" | "80" => "chat_completions",
        _ => panic!("not a q1 network ordinal: {ordinal}"),
    };
    let (
        message_layout,
        tool_choice,
        parallel_tool_calls,
        response_format,
        stream,
        include_usage,
        replay,
    ) = q1_source_fields(ordinal);

    let mut transaction = pool.begin().await.unwrap();
    set_context(&mut transaction, qualification.fixture.ids).await;
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'openai-compatible','{}'::jsonb)",
    )
    .bind(effect_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_request_evidence_roots(
             id,workspace_id,external_effect_id,request_kind,
             qualification_target_binding_id,cause_kind,cause_id
         ) VALUES($1,$2,$3,$4,$5,'qualification_probe',$6)",
    )
    .bind(evidence_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(effect_id)
    .bind(request_kind)
    .bind(qualification.target)
    .bind(qualification.job.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    for (node_ordinal, kind, reference, safe_ordinal) in [
        (0_i32, "external_effect", effect_id, None),
        (1_i32, "qualification_target", qualification.target, None),
        (
            2_i32,
            "qualification_probe",
            qualification.job.as_uuid(),
            Some(ordinal),
        ),
    ] {
        sqlx::query(
            "INSERT INTO model_request_evidence_nodes(
                 id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,safe_ordinal
             ) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(Uuid::now_v7())
        .bind(qualification.fixture.ids.workspace.as_uuid())
        .bind(evidence_id)
        .bind(node_ordinal)
        .bind(kind)
        .bind(reference)
        .bind(safe_ordinal)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(
             id,workspace_id,evidence_root_id,status,missing_reference_count
         ) VALUES($1,$2,$3,'complete',0)",
    )
    .bind(evidence_check_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(evidence_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_qualification_q1_mre_source(
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(evidence_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(ordinal)
    .bind(message_layout)
    .bind(tool_choice)
    .bind(parallel_tool_calls)
    .bind(response_format)
    .bind(stream)
    .bind(include_usage)
    .bind(replay)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO provider_dispatch_causes(
             external_effect_id,workspace_id,model_request_evidence_id,model_request_evidence_check_id,
             cause_kind,qualification_job_id,qualification_target_binding_id,qualification_probe_ordinal
         ) VALUES($1,$2,$3,$4,'qualification_probe',$5,$6,$7)",
    )
    .bind(effect_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(evidence_id)
    .bind(evidence_check_id)
    .bind(qualification.job.as_uuid())
    .bind(qualification.target)
    .bind(ordinal)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("RESET ROLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_receipts(
             id,effect_id,workspace_id,outcome_status,payload
         ) VALUES($1,$2,$3,'acknowledged','{}'::jsonb)",
    )
    .bind(receipt_id)
    .bind(effect_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(
             id,effect_id,workspace_id,status,cause,cause_ref,recorded_at
         ) VALUES($1,$2,$3,'acknowledged','receipt_recorded',$4,NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(effect_id)
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(receipt_id.to_string())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (effect_id, evidence_id)
}

async fn setup_sibling(owner: &PgPool, runtime: &PgPool, base: &Fixture) -> Fixture {
    let ids = Ids {
        workspace: base.ids.workspace,
        principal: base.ids.principal,
        run: AgentRunId::new(),
        step: RunStepId::new(),
        connection: base.ids.connection,
        revision: base.ids.revision,
        snapshot: Uuid::now_v7(),
        evidence: ModelRequestEvidenceId::new(),
    };
    let intent = new_intent(ids);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&RequestContext::new(ids.workspace, ids.principal), &intent)
        .await
        .unwrap();

    sqlx::query("INSERT INTO agent_runs(id,workspace_id,principal_id,title,objective,status,run_version) VALUES($1,$2,$3,'provider sibling','provider','running',1)")
        .bind(ids.run.as_uuid()).bind(ids.workspace.as_uuid()).bind(ids.principal.as_uuid()).execute(owner).await.unwrap();
    sqlx::query("INSERT INTO run_steps(id,run_id,workspace_id,step_number,title,status) VALUES($1,$2,$3,1,'provider sibling','running')")
        .bind(ids.step.as_uuid()).bind(ids.run.as_uuid()).bind(ids.workspace.as_uuid()).execute(owner).await.unwrap();

    let mut tx = owner.begin().await.unwrap();
    set_context(&mut tx, ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_binding_snapshots(
            id,workspace_id,connection_id,connection_revision_id,
            connection_qualification_revision_id,model_revision_id,
            model_qualification_revision_id,branch,credential_revision_id,
            credential_slot_id,credential_activation_guard_id,
            expected_slot_version,no_auth_binding_revision_id)
         SELECT $1,workspace_id,connection_id,connection_revision_id,
            connection_qualification_revision_id,model_revision_id,
            model_qualification_revision_id,branch,credential_revision_id,
            credential_slot_id,credential_activation_guard_id,
            expected_slot_version,no_auth_binding_revision_id
           FROM model_binding_snapshots WHERE workspace_id=$2 AND id=$3",
    )
    .bind(ids.snapshot)
    .bind(ids.workspace.as_uuid())
    .bind(base.ids.snapshot)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
        .bind(ids.workspace.as_uuid())
        .bind(ids.snapshot)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO run_model_binding_snapshots(workspace_id,run_id,snapshot_id) VALUES($1,$2,$3)",
    )
    .bind(ids.workspace.as_uuid())
    .bind(ids.run.as_uuid())
    .bind(ids.snapshot)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) VALUES($1,$2,$3,'chat_completions',$4,'run_step',$5)")
        .bind(ids.evidence.as_uuid()).bind(ids.workspace.as_uuid()).bind(intent.id().as_uuid()).bind(ids.snapshot).bind(ids.step.as_uuid()).execute(&mut *tx).await.unwrap();
    let source_nodes: Vec<(String, Uuid, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT reference_kind, reference_id, reference_version, safe_ordinal FROM model_request_evidence_nodes
          WHERE workspace_id=$1 AND evidence_root_id=$2 ORDER BY ordinal",
    )
    .bind(ids.workspace.as_uuid()).bind(base.ids.evidence.as_uuid())
    .fetch_all(&mut *tx).await.unwrap();
    for (ordinal, (kind, mut reference, version, safe_ordinal)) in
        source_nodes.into_iter().enumerate()
    {
        if kind == "external_effect" {
            reference = intent.id().as_uuid();
        } else if kind == "binding_snapshot" {
            reference = ids.snapshot;
        }
        sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,reference_version,safe_ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(Uuid::now_v7()).bind(ids.workspace.as_uuid()).bind(ids.evidence.as_uuid())
            .bind(i32::try_from(ordinal).unwrap()).bind(kind).bind(reference).bind(version).bind(safe_ordinal)
            .execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,missing_reference_count) VALUES($1,$2,$3,'complete',0)")
        .bind(Uuid::now_v7()).bind(ids.workspace.as_uuid()).bind(ids.evidence.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let sibling = Fixture {
        ids,
        intent,
        credential: base.credential,
        chat_model_revision_id: base.chat_model_revision_id,
        embedding_model_revision_id: base.embedding_model_revision_id,
    };
    reserve_run_step_execution_attempt(runtime, &sibling).await;
    sibling
}

fn effective_request() -> EffectiveModelRequest {
    EffectiveModelRequest::chat_completions(
        "model",
        vec![EffectiveChatMessage::user("dispatch secret sentinel")],
        EffectiveSampling::new(0.2, 1.0).unwrap(),
        EffectiveRequestLimits::new(64, 1, 1024).unwrap(),
        Vec::new(),
        false,
    )
    .unwrap()
}

struct FixedEvidence;
#[async_trait]
impl ModelRequestEvidenceRepository for FixedEvidence {
    async fn create_in(
        &self,
        _uow: &mut dyn UnitOfWork,
        creation: &vestrace_application::CreateModelRequestEvidence,
    ) -> Result<ModelRequestEvidenceId, ApplicationError> {
        Ok(ModelRequestEvidenceId::from_uuid(creation.root_id()))
    }
    async fn reconstruct_current_in(
        &self,
        _uow: &mut dyn UnitOfWork,
        _workspace: WorkspaceId,
        _evidence: ModelRequestEvidenceId,
    ) -> Result<ModelRequestReconstruction, ApplicationError> {
        Ok(ModelRequestReconstruction::Complete(effective_request()))
    }
}

struct AllowPolicy {
    ids: Ids,
    content_pointer: Arc<AtomicUsize>,
    track_deallocation: bool,
}

struct MatrixPolicy {
    ids: Ids,
    model_allowed: bool,
    mode: ModelDataPolicyMode,
    authorization_result: PolicyDecisionResult,
}

struct QualificationAllowPolicy;

#[async_trait]
impl ProviderDispatchPolicyEvaluator for QualificationAllowPolicy {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        _cause: &ProviderDispatchCause,
        _target: &ProviderDispatchTarget,
        _request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
        let auth_request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation(),
            intent.target(),
            intent.risk(),
        );
        Ok(ProviderDispatchPolicyEvaluation {
            authorization: PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "provider-dispatch-v1".into(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: intent.required_capability(),
                operation: intent.operation().into(),
                resource_scope: intent.target().into(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &auth_request,
                ),
                matched_grant_id: None,
                decided_at: chrono::Utc::now(),
            },
            model_data_policy: None,
        })
    }
}

#[derive(Default)]
struct CountingQ1Adapter {
    calls: AtomicUsize,
}

#[async_trait]
impl QualificationQ1Adapter for CountingQ1Adapter {
    async fn execute(
        &self,
        _kind: ConnectionKind,
        _runtime_base_url: &str,
        _auth: ConnectionAuth,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(matches!(request, Q1ProbeRequest::ModelsList));
        Ok(Q1ProbeResponse::ModelsList(
            Q1ModelsListProbeResult::new(1).unwrap(),
        ))
    }
}

#[derive(Default)]
struct OptionalUnsupportedQ1Adapter {
    calls: AtomicUsize,
}

#[async_trait]
impl QualificationQ1Adapter for OptionalUnsupportedQ1Adapter {
    async fn execute(
        &self,
        _kind: ConnectionKind,
        _runtime_base_url: &str,
        _auth: ConnectionAuth,
        _request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(Q1ProbeFailure::HttpStatus { status: 404 })
    }
}

#[async_trait]
impl ProviderDispatchPolicyEvaluator for MatrixPolicy {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        _cause: &ProviderDispatchCause,
        _target: &ProviderDispatchTarget,
        _request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
        let auth_request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation(),
            intent.target(),
            intent.risk(),
        );
        Ok(ProviderDispatchPolicyEvaluation {
            authorization: PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "provider-dispatch-v1".into(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: intent.required_capability(),
                operation: intent.operation().into(),
                resource_scope: intent.target().into(),
                result: self.authorization_result,
                reason: if self.authorization_result == PolicyDecisionResult::Allow {
                    PolicyDecisionReason::ConfiguredAllowance
                } else {
                    PolicyDecisionReason::DefaultDeny
                },
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &auth_request,
                ),
                matched_grant_id: None,
                decided_at: chrono::Utc::now(),
            },
            model_data_policy: Some(ModelDataPolicyDecisionRecord {
                id: Uuid::now_v7(),
                run_id: self.ids.run,
                step_id: self.ids.step,
                destination: DataDestination::RemoteProvider,
                classification: Sensitivity::Confidential,
                allowed: self.model_allowed,
                reason: "matrix policy decision".into(),
                policy_version: "model-data-v1".into(),
                mode: self.mode,
                decided_at: chrono::Utc::now(),
            }),
        })
    }
}
#[async_trait]
impl ProviderDispatchPolicyEvaluator for AllowPolicy {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        _cause: &ProviderDispatchCause,
        _target: &ProviderDispatchTarget,
        request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
        let EffectiveModelRequest::ChatCompletions(chat) = request else {
            return Err(ApplicationError::Policy("expected chat request".into()));
        };
        self.content_pointer.store(
            chat.messages()[0].content().as_ptr() as usize,
            Ordering::SeqCst,
        );
        if self.track_deallocation {
            assert_eq!(
                chat.messages()[0].content().as_bytes(),
                TRACKED_CONTENT_BYTES
            );
            track_content_deallocation(chat.messages()[0].content().as_ptr());
        }
        let auth_request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation(),
            intent.target(),
            intent.risk(),
        );
        Ok(ProviderDispatchPolicyEvaluation {
            authorization: PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "provider-dispatch-v1".into(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: intent.required_capability(),
                operation: intent.operation().into(),
                resource_scope: intent.target().into(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &auth_request,
                ),
                matched_grant_id: None,
                decided_at: chrono::Utc::now(),
            },
            model_data_policy: Some(ModelDataPolicyDecisionRecord {
                id: Uuid::now_v7(),
                run_id: self.ids.run,
                step_id: self.ids.step,
                destination: DataDestination::RemoteProvider,
                classification: Sensitivity::Confidential,
                allowed: true,
                reason: "approved for the pinned remote provider".into(),
                policy_version: "model-data-v1".into(),
                mode: ModelDataPolicyMode::Enforce,
                decided_at: chrono::Utc::now(),
            }),
        })
    }
}

struct NoCredentialLease;
#[async_trait]
impl CredentialDispatchLeaseRepository for NoCredentialLease {
    async fn issue(
        &self,
        _context: &RequestContext,
        _request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        Err(ApplicationError::Internal(
            "unexpected credential issue".into(),
        ))
    }
    async fn issue_in(
        &self,
        _context: &RequestContext,
        _uow: &mut dyn UnitOfWork,
        _request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        Err(ApplicationError::Internal(
            "unexpected credential issue".into(),
        ))
    }
    async fn consume_for_dispatch(
        &self,
        _context: &RequestContext,
        _uow: &mut dyn UnitOfWork,
        _lease: &CredentialDispatchLease,
    ) -> Result<vestrace_application::ConnectionAuth, ApplicationError> {
        Err(ApplicationError::Internal(
            "unexpected credential consume".into(),
        ))
    }
}

struct CountingVault(AtomicUsize);

impl CountingVault {
    fn calls(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy, Debug)]
enum LeaseProjectionMutation {
    Authorization,
    ActivationGuard,
    Destination,
    ValidAuthMode,
    UnsupportedAuthMode,
}

struct MutatingCredentialLeaseRepository {
    inner: PgCredentialDispatchLeaseRepository<CountingVault>,
    mutation: LeaseProjectionMutation,
}

impl MutatingCredentialLeaseRepository {
    fn mutate(&self, lease: &mut CredentialDispatchLease) {
        match self.mutation {
            LeaseProjectionMutation::Authorization => lease.authorization_id = Uuid::now_v7(),
            LeaseProjectionMutation::ActivationGuard => {
                lease.credential_activation_guard_id = Uuid::now_v7()
            }
            LeaseProjectionMutation::Destination => {
                lease.destination_authority = "other.example.test".to_owned()
            }
            LeaseProjectionMutation::ValidAuthMode => lease.auth_mode = "api_key".to_owned(),
            LeaseProjectionMutation::UnsupportedAuthMode => {
                lease.auth_mode = "unsupported".to_owned()
            }
        }
    }
}

#[async_trait]
impl CredentialDispatchLeaseRepository for MutatingCredentialLeaseRepository {
    async fn issue(
        &self,
        context: &RequestContext,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        let mut lease = self.inner.issue(context, request).await?;
        self.mutate(&mut lease);
        Ok(lease)
    }

    async fn issue_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        let mut lease = self.inner.issue_in(context, unit_of_work, request).await?;
        self.mutate(&mut lease);
        Ok(lease)
    }

    async fn consume_for_dispatch(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        lease: &CredentialDispatchLease,
    ) -> Result<vestrace_application::ConnectionAuth, ApplicationError> {
        self.inner
            .consume_for_dispatch(context, unit_of_work, lease)
            .await
    }
}

struct PreparationCountingVault {
    creates: AtomicUsize,
    unwraps: AtomicUsize,
}

impl PreparationCountingVault {
    fn counts(&self) -> (usize, usize) {
        (
            self.creates.load(Ordering::SeqCst),
            self.unwraps.load(Ordering::SeqCst),
        )
    }
}

impl vestrace_application::MaterialKeyVault for PreparationCountingVault {
    fn create_if_absent(
        &self,
        _: MaterialKeyId,
        _: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(vestrace_domain::VaultReceipt::new())
    }

    fn unwrap(
        &self,
        _: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        self.unwraps.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([7; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn erase(
        &self,
        _: MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

impl vestrace_application::MaterialKeyVault for CountingVault {
    fn create_if_absent(
        &self,
        _: vestrace_domain::MaterialKeyId,
        _: vestrace_domain::IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn unwrap(
        &self,
        _: vestrace_domain::MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([7; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn erase(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

struct EvidenceVault {
    unwraps: AtomicUsize,
}

impl EvidenceVault {
    fn unwraps(&self) -> usize {
        self.unwraps.load(Ordering::SeqCst)
    }
}

impl vestrace_application::MaterialKeyVault for EvidenceVault {
    fn create_if_absent(
        &self,
        _: vestrace_domain::MaterialKeyId,
        _: vestrace_domain::IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn unwrap(
        &self,
        _: vestrace_domain::MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        self.unwraps.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([0x4B; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn erase(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

struct FailAt(ProviderDispatchFaultPoint);
impl ProviderDispatchFaultInjector for FailAt {
    fn check(&self, point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        if point == self.0 {
            Err(ApplicationError::Unavailable(format!(
                "injected fault at {point:?}"
            )))
        } else {
            Ok(())
        }
    }

    fn observe_auth(&self, auth: &vestrace_application::ConnectionAuth) {
        if matches!(
            self.0,
            ProviderDispatchFaultPoint::AfterCredentialConsume
                | ProviderDispatchFaultPoint::BeforeDispatching
                | ProviderDispatchFaultPoint::AfterDispatching
                | ProviderDispatchFaultPoint::BeforeGovernedCommit
                | ProviderDispatchFaultPoint::AfterGovernedCommit
        ) {
            let vestrace_application::ConnectionAuth::Bearer(credential) = auth else {
                panic!("credential dispatch did not produce bearer auth")
            };
            assert_eq!(credential.expose().as_bytes(), TRACKED_CREDENTIAL_BYTES);
            track_credential_deallocation(credential.expose().as_ptr());
        }
    }
}

struct ObserveCredentialDeallocation;

impl ProviderDispatchFaultInjector for ObserveCredentialDeallocation {
    fn check(&self, _point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        Ok(())
    }

    fn observe_auth(&self, auth: &vestrace_application::ConnectionAuth) {
        let vestrace_application::ConnectionAuth::Bearer(credential) = auth else {
            panic!("credential dispatch did not produce bearer auth")
        };
        assert_eq!(credential.expose().as_bytes(), TRACKED_CREDENTIAL_BYTES);
        track_credential_deallocation(credential.expose().as_ptr());
    }
}

struct DeferredCommitErrorPermit {
    store: PgStore,
}

#[async_trait]
impl InstallationMutationPermit for DeferredCommitErrorPermit {
    async fn acquire(
        &self,
        mode: PermitMode,
        context: &RequestContext,
    ) -> Result<vestrace_application::PermitHandle, ApplicationError> {
        assert_eq!(mode, PermitMode::Shared);
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtext($1))")
            .bind("vestrace-installation-mutation-permit-v1")
            .execute(transaction.connection())
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        sqlx::query(
            "CREATE TEMP TABLE task10c_deferred_commit_failure (
                 id INTEGER PRIMARY KEY,
                 parent_id INTEGER REFERENCES task10c_deferred_commit_failure(id)
                     DEFERRABLE INITIALLY DEFERRED
             ) ON COMMIT DROP",
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        sqlx::query("INSERT INTO task10c_deferred_commit_failure(id,parent_id) VALUES (1,2)")
            .execute(transaction.connection())
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(vestrace_application::PermitHandle::new(Box::new(
            transaction,
        )))
    }
}

fn dispatch_request(fixture: &Fixture) -> ProviderDispatchRequest {
    let now = chrono::Utc::now();
    ProviderDispatchRequest {
        context: RequestContext::new(fixture.ids.workspace, fixture.ids.principal),
        intent: fixture.intent.clone(),
        model_request_evidence_id: fixture.ids.evidence,
        connection_id: fixture.ids.connection,
        connection_revision_id: fixture.ids.revision,
        cause: ProviderDispatchCause::RunStep {
            run_id: fixture.ids.run,
            step_id: fixture.ids.step,
            snapshot_id: fixture.ids.snapshot,
        },
        admission_id: Uuid::now_v7(),
        wait_id: Uuid::now_v7(),
        concurrency_lease_id: Uuid::now_v7(),
        dispatch_ttl_seconds: 60,
        worker_id: WorkerId::new(),
        credential: None,
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.ids.workspace,
            fixture.ids.principal,
            "provider.dispatch.prepared",
            "external_effect",
            fixture.intent.id().as_uuid(),
            serde_json::json!({"phase":"pre_dispatch"}),
            now,
        )
        .unwrap(),
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("provider-dispatch:{}", fixture.intent.id()),
            workspace_id: fixture.ids.workspace,
            request_hash: "safe-provider-dispatch-tuple-v1".into(),
            response_payload: None,
            status: "completed".into(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
        }),
        outbox: vec![OutboxMessage {
            id: OutboxId::new(),
            workspace_id: fixture.ids.workspace,
            topic: "provider.dispatch.prepared".into(),
            payload: serde_json::json!({"effect_id": fixture.intent.id()}),
            created_at: now,
            attempts: 0,
        }],
    }
}

/// Establishes exactly the committed pre-network authority that the recovery
/// seam calls `ResumeAdmitted`: the same guarded admission, immutable allow
/// decision, and evidence-bound attempt phase as production, but deliberately
/// no `dispatch_started` lifecycle transition.
async fn admit_run_step_attempt_without_dispatch(runtime: &PgPool, fixture: &Fixture) {
    let request = dispatch_request(fixture);
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);
    let mut admission = runtime.begin().await.unwrap();
    set_context(&mut admission, fixture.ids).await;
    let decision: String = sqlx::query_scalar(
        "SELECT decision FROM vestrace_try_admit_provider_dispatch(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(request.admission_id)
    .bind(request.wait_id)
    .bind(request.concurrency_lease_id)
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.connection.as_uuid())
    .bind(fixture.ids.revision.as_uuid())
    .bind(fixture.intent.id().as_uuid())
    .bind(fixture.ids.evidence.as_uuid())
    .bind("run_step")
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(fixture.ids.snapshot)
    .bind(Option::<Uuid>::None)
    .bind(Option::<Uuid>::None)
    .bind(Option::<String>::None)
    .bind(i32::from(request.dispatch_ttl_seconds))
    .fetch_one(&mut *admission)
    .await
    .expect("the fixture must establish its canonical guarded admission");
    assert_eq!(decision, "admitted");
    admission.commit().await.unwrap();

    let evaluation = AllowPolicy {
        ids: fixture.ids,
        content_pointer: Arc::new(AtomicUsize::new(0)),
        track_deallocation: false,
    }
    .evaluate(
        &context,
        &fixture.intent,
        &run_step_cause(fixture),
        &pinned_target(),
        &effective_request(),
    )
    .await
    .expect("the test policy must issue an exact allow decision");
    let effects = PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()));
    effects
        .insert_intent(&context, &fixture.intent)
        .await
        .expect("the admitted fixture must retain the original immutable effect intent");
    effects
        .record_authorization(&context, fixture.intent.id(), &evaluation.authorization)
        .await
        .expect("the guarded admitted tuple must retain its immutable authorization");

    let mut phase = runtime.begin().await.unwrap();
    set_context(&mut phase, fixture.ids).await;
    let attempt_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM run_step_execution_attempts
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *phase)
    .await
    .expect("the fresh attempt must remain visible inside its scoped phase transaction");
    let advanced: Uuid = sqlx::query_scalar(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'admitted')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *phase)
    .await
    .expect("only the complete canonical admitted tuple can advance the attempt");
    assert_eq!(advanced, attempt_id);
    phase.commit().await.unwrap();
}

fn credential_request(fixture: &Fixture) -> vestrace_application::ProviderDispatchCredential {
    let credential = fixture.credential.expect("credential-backed fixture");
    vestrace_application::ProviderDispatchCredential {
        lease_id: Uuid::now_v7(),
        credential_slot_id: credential.slot,
        credential_revision_id: credential.revision,
        credential_activation_guard_id: credential.activation_guard,
        destination_authority: "api.example.test".into(),
        auth_mode: "bearer".into(),
    }
}

fn repository(
    runtime: &PgPool,
    fixture: &Fixture,
    pointer: Arc<AtomicUsize>,
    fault: Option<ProviderDispatchFaultPoint>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let permit = Arc::new(PgInstallationMutationPermit::new(store.clone()));
    let evidence = Arc::new(FixedEvidence);
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let credential = Arc::new(NoCredentialLease);
    let data_policy = Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone()));
    let governed = Arc::new(PgGovernedMutationRepository::new(store));
    let policy = Arc::new(AllowPolicy {
        ids: fixture.ids,
        content_pointer: pointer,
        track_deallocation: fault.is_some(),
    });
    if let Some(point) = fault {
        PgProviderDispatchRepository::with_faults(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
            Arc::new(FailAt(point)),
        )
    } else {
        PgProviderDispatchRepository::new(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
        )
    }
}

async fn seed_provider_result_output(pool: &PgPool, fixture: &Fixture) -> ProviderResultIdentities {
    let artifact_id = ArtifactId::new();
    let artifact_revision_id = ArtifactRevisionId::new();
    let model_execution_id = ModelExecutionId::new();
    let model_id: Uuid =
        sqlx::query_scalar("SELECT model_id FROM model_revisions WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.ids.workspace.as_uuid())
            .bind(fixture.chat_model_revision_id)
            .fetch_one(pool)
            .await
            .expect("Run dispatch fixture must retain the pinned chat model revision");

    sqlx::query(
        "INSERT INTO model_executions \
         (id,workspace_id,model_id,prompt_tokens,completion_tokens,latency_ms,status) \
         VALUES($1,$2,$3,1,1,1,'succeeded')",
    )
    .bind(model_execution_id.as_uuid())
    .bind(fixture.ids.workspace.as_uuid())
    .bind(model_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,$3)")
        .bind(artifact_id.as_uuid())
        .bind(fixture.ids.workspace.as_uuid())
        .bind(format!("provider-result-{}", artifact_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO artifact_revisions \
         (id,artifact_id,workspace_id,revision_number,media_type,content_hash,byte_size) \
         VALUES($1,$2,$3,1,'text/plain','dispatch-result-fixture',4096)",
    )
    .bind(artifact_revision_id.as_uuid())
    .bind(artifact_id.as_uuid())
    .bind(fixture.ids.workspace.as_uuid())
    .execute(pool)
    .await
    .unwrap();

    ProviderResultIdentities {
        material_intent_id: MaterialKeyCreationIntentId::new(),
        content_material_id: ContentMaterialId::new(),
        material_key_id: MaterialKeyId::new(),
        intent_nonce: IntentNonce::new(),
        prepared_attachment_id: PreparedMaterialAttachmentId::new(),
        receipt_id: ExternalEffectReceiptId::new(),
        artifact_id,
        artifact_revision_id,
        model_execution_id,
        advance_work_item_id: WorkItemId::new(),
    }
}

fn provider_result_repository_after_live_dispatch(
    runtime: &PgPool,
    dispatch: SharedProviderDispatchRepository,
) -> PgProviderResultRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderResultRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PreparationCountingVault {
            creates: AtomicUsize::new(0),
            unwraps: AtomicUsize::new(0),
        }),
        Arc::new(ContentMaterialCodec::new()),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PostgresRunStore::new(&store)),
    )
    .with_provider_dispatch(dispatch)
}

struct ResultFailAt(ProviderResultFaultPoint);

impl ProviderResultFaultInjector for ResultFailAt {
    fn check(&self, point: ProviderResultFaultPoint) -> Result<(), ApplicationError> {
        if point == self.0 {
            Err(ApplicationError::Internal(format!(
                "injected provider-result fault at {point:?}"
            )))
        } else {
            Ok(())
        }
    }
}

fn faulting_provider_result_repository_after_live_dispatch(
    runtime: &PgPool,
    dispatch: SharedProviderDispatchRepository,
    fault: ProviderResultFaultPoint,
) -> PgProviderResultRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderResultRepository::with_faults(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PreparationCountingVault {
            creates: AtomicUsize::new(0),
            unwraps: AtomicUsize::new(0),
        }),
        Arc::new(ContentMaterialCodec::new()),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PostgresRunStore::new(&store)),
        Arc::new(ResultFailAt(fault)),
    )
    .with_provider_dispatch(dispatch)
}

fn repository_with_policy(
    runtime: &PgPool,
    policy: Arc<dyn ProviderDispatchPolicyEvaluator>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(FixedEvidence),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(NoCredentialLease),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        policy,
    )
}

fn credential_repository(
    runtime: &PgPool,
    fixture: &Fixture,
    vault: Arc<CountingVault>,
    fault: Option<ProviderDispatchFaultPoint>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let policy: Arc<dyn ProviderDispatchPolicyEvaluator> = Arc::new(AllowPolicy {
        ids: fixture.ids,
        content_pointer: Arc::new(AtomicUsize::new(0)),
        track_deallocation: fault.is_some(),
    });
    let credential = Arc::new(PgCredentialDispatchLeaseRepository::new(
        store.clone(),
        vault,
    ));
    if let Some(point) = fault {
        PgProviderDispatchRepository::with_faults(
            Arc::new(PgInstallationMutationPermit::new(store.clone())),
            Arc::new(FixedEvidence),
            Arc::new(PgExternalEffectRepository::new(store.clone())),
            credential,
            Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
            Arc::new(PgGovernedMutationRepository::new(store)),
            policy,
            Arc::new(FailAt(point)),
        )
    } else {
        PgProviderDispatchRepository::new(
            Arc::new(PgInstallationMutationPermit::new(store.clone())),
            Arc::new(FixedEvidence),
            Arc::new(PgExternalEffectRepository::new(store.clone())),
            credential,
            Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
            Arc::new(PgGovernedMutationRepository::new(store)),
            policy,
        )
    }
}

fn credential_repository_with_projection_mutation(
    runtime: &PgPool,
    fixture: &Fixture,
    vault: Arc<CountingVault>,
    mutation: LeaseProjectionMutation,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let credential: Arc<dyn CredentialDispatchLeaseRepository> =
        Arc::new(MutatingCredentialLeaseRepository {
            inner: PgCredentialDispatchLeaseRepository::new(store.clone(), vault),
            mutation,
        });
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(FixedEvidence),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        credential,
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(AllowPolicy {
            ids: fixture.ids,
            content_pointer: Arc::new(AtomicUsize::new(0)),
            track_deallocation: false,
        }),
    )
}

fn credential_repository_with_commit_error(
    runtime: &PgPool,
    fixture: &Fixture,
    vault: Arc<CountingVault>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderDispatchRepository::with_faults(
        Arc::new(DeferredCommitErrorPermit {
            store: store.clone(),
        }),
        Arc::new(FixedEvidence),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgCredentialDispatchLeaseRepository::new(
            store.clone(),
            vault,
        )),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(AllowPolicy {
            ids: fixture.ids,
            content_pointer: Arc::new(AtomicUsize::new(0)),
            track_deallocation: true,
        }),
        Arc::new(ObserveCredentialDeallocation),
    )
}

fn qualification_repository(
    runtime: &PgPool,
    credential_vault: Option<Arc<CountingVault>>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let credential: Arc<dyn CredentialDispatchLeaseRepository> = match credential_vault {
        Some(vault) => Arc::new(PgCredentialDispatchLeaseRepository::new(
            store.clone(),
            vault,
        )),
        None => Arc::new(NoCredentialLease),
    };
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PgModelRequestEvidenceRepository::new(Arc::new(
            EvidenceVault {
                unwraps: AtomicUsize::new(0),
            },
        ))),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        credential,
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(QualificationAllowPolicy),
    )
}

fn qualification_request(fixture: &QualificationFixture) -> ProviderDispatchRequest {
    let mut request = dispatch_request(&fixture.fixture);
    request.cause = ProviderDispatchCause::QualificationProbe {
        qualification_job_id: fixture.job,
        qualification_target_id: fixture.target,
        probe_ordinal: "10".to_owned(),
    };
    request
}

fn real_evidence_repository(
    runtime: &PgPool,
    fixture: &Fixture,
    vault: Arc<EvidenceVault>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PgModelRequestEvidenceRepository::new(vault)),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(NoCredentialLease),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(MatrixPolicy {
            ids: fixture.ids,
            model_allowed: true,
            mode: ModelDataPolicyMode::Enforce,
            authorization_result: PolicyDecisionResult::Allow,
        }),
    )
}

fn consume_prepared(outcome: ProviderDispatchOutcome, calls: &AtomicUsize) {
    let ProviderDispatchOutcome::Prepared {
        authority,
        request,
        auth,
        ..
    } = outcome
    else {
        panic!("adapter probe received a non-prepared outcome")
    };
    match &auth {
        vestrace_application::ConnectionAuth::None => {}
        vestrace_application::ConnectionAuth::Bearer(credential) => {
            assert_eq!(credential.expose(), "real-provider-credential-sentinel")
        }
        other => panic!("unexpected prepared auth mode: {other:?}"),
    }
    calls.fetch_add(1, Ordering::SeqCst);
    drop((authority, request, auth));
}

#[derive(Debug, PartialEq, Eq)]
enum AdapterHarnessOutcome {
    Prepared,
    Conflict { retry_after_seconds: Option<u32> },
    Denied,
}

fn drive_prepared_adapter_only(
    outcome: ProviderDispatchOutcome,
    calls: &AtomicUsize,
) -> AdapterHarnessOutcome {
    match outcome {
        prepared @ ProviderDispatchOutcome::Prepared { .. } => {
            consume_prepared(prepared, calls);
            AdapterHarnessOutcome::Prepared
        }
        ProviderDispatchOutcome::Conflict {
            retry_after_seconds,
            ..
        } => AdapterHarnessOutcome::Conflict {
            retry_after_seconds,
        },
        ProviderDispatchOutcome::Denied { .. } => AdapterHarnessOutcome::Denied,
    }
}

async fn atomic_counts(
    pool: &PgPool,
    fixture: &Fixture,
) -> (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM model_data_policy_decisions WHERE run_id=$1 AND step_id=$2),
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$3 AND workspace_id=$4),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$3),
            (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$3),
            (SELECT COUNT(*) FROM external_effect_authorizations WHERE effect_id=$3),
            (SELECT COUNT(*) FROM credential_dispatch_leases WHERE external_effect_id=$3),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions WHERE effect_id=$3 AND status='dispatching'),
            (SELECT COUNT(*) FROM audit_events WHERE resource_id=$3),
            (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id=$4 AND idempotency_key=$5),
            (SELECT COUNT(*) FROM outbox WHERE workspace_id=$4 AND topic='provider.dispatch.prepared')",
    )
    .bind(fixture.ids.run.as_uuid()).bind(fixture.ids.step.as_uuid()).bind(fixture.intent.id().as_uuid())
    .bind(fixture.ids.workspace.as_uuid()).bind(format!("provider-dispatch:{}", fixture.intent.id()))
    .fetch_one(pool).await.unwrap()
}

async fn audit_evidence(pool: &PgPool, audit_id: AuditEventId) -> (i64, i64) {
    sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM governed_mutation_audit_marks WHERE audit_event_id=$1),
            (SELECT COUNT(*) FROM installation_mutation_watermark_advances AS advance
              JOIN governed_mutation_audit_marks AS mark ON mark.id=advance.governed_mutation_mark_id
             WHERE mark.audit_event_id=$1)",
    ).bind(audit_id.as_uuid()).fetch_one(pool).await.unwrap()
}

async fn watermark(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT watermark FROM installation_mutation_watermark WHERE singleton=true")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn atomic_count_authority_observes_a_current_effect_intent(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    PgExternalEffectRepository::new(PgStore::from_pool(runtime))
        .insert_intent(
            &RequestContext::new(fixture.ids.workspace, fixture.ids.principal),
            &fixture.intent,
        )
        .await
        .unwrap();

    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn admitted_dispatch_commits_all_legs_and_returns_the_same_request(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let pointer = Arc::new(AtomicUsize::new(0));
    let adapter_calls = AtomicUsize::new(0);
    let request = dispatch_request(&fixture);
    let audit_id = request.audit.id;
    let before_watermark = watermark(&pool).await;

    let outcome = repository(&runtime, &fixture, pointer.clone(), None)
        .prepare_dispatch(request)
        .await
        .unwrap();
    let ProviderDispatchOutcome::Prepared {
        authority,
        target,
        request,
        ..
    } = &outcome
    else {
        panic!("provider dispatch did not admit")
    };
    let EffectiveModelRequest::ChatCompletions(chat) = request else {
        panic!("provider dispatch returned another request kind")
    };
    assert_eq!(target.kind, ConnectionKind::LMStudioLocal);
    assert_eq!(target.runtime_base_url, "http://127.0.0.1:1234/v1");
    assert_eq!(
        pointer.load(Ordering::SeqCst),
        chat.messages()[0].content().as_ptr() as usize,
        "policy and caller did not observe the same zeroizing semantic allocation"
    );
    assert_eq!(authority.effect_id, fixture.intent.id());
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 1, 1, 1, 0, 1, 1, 1, 1)
    );
    assert_eq!(audit_evidence(&pool, audit_id).await, (1, 1));
    assert_eq!(watermark(&pool).await, before_watermark + 1);
    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn injected_write_boundaries_roll_every_dispatch_leg_back(pool: PgPool) {
    let _tracking_guard = TRACKING_TEST_GUARD.lock().await;
    let runtime = runtime_pool(&pool).await;
    let points = [
        ProviderDispatchFaultPoint::BeforeAdmission,
        ProviderDispatchFaultPoint::AfterAdmission,
        ProviderDispatchFaultPoint::BeforePolicyRecord,
        ProviderDispatchFaultPoint::AfterPolicyRecord,
        ProviderDispatchFaultPoint::BeforeIntent,
        ProviderDispatchFaultPoint::AfterIntent,
        ProviderDispatchFaultPoint::BeforeAuthorization,
        ProviderDispatchFaultPoint::AfterAuthorization,
        ProviderDispatchFaultPoint::BeforeDispatching,
        ProviderDispatchFaultPoint::AfterDispatching,
        ProviderDispatchFaultPoint::BeforeGovernedCommit,
        ProviderDispatchFaultPoint::AfterGovernedCommit,
    ];
    for point in points {
        let fixture = setup(&pool, &runtime).await;
        let adapter_calls = AtomicUsize::new(0);
        let request = dispatch_request(&fixture);
        let audit_id = request.audit.id;
        let before_watermark = watermark(&pool).await;
        let result = repository(
            &runtime,
            &fixture,
            Arc::new(AtomicUsize::new(0)),
            Some(point),
        )
        .prepare_dispatch(request)
        .await;
        let error = match result {
            Ok(outcome) => {
                consume_prepared(outcome, &adapter_calls);
                panic!("{point:?} unexpectedly committed")
            }
            Err(error) => error,
        };
        assert!(
            matches!(error, ApplicationError::Unavailable(_)),
            "{point:?}: {error}"
        );
        assert_eq!(
            atomic_counts(&pool, &fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "{point:?}"
        );
        assert_eq!(audit_evidence(&pool, audit_id).await, (0, 0), "{point:?}");
        assert_eq!(watermark(&pool).await, before_watermark, "{point:?}");
        assert_eq!(adapter_calls.load(Ordering::SeqCst), 0, "{point:?}");
        assert!(
            TRACKED_DEALLOCATION_OBSERVED.load(Ordering::SeqCst),
            "{point:?}: exact reconstructed content allocation was not deallocated"
        );
        assert!(
            TRACKED_DEALLOCATION_ZERO.load(Ordering::SeqCst),
            "{point:?}: reconstructed content allocation was not zero before deallocation"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn observe_model_denial_with_allowed_authorization_may_prepare(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let adapter_calls = AtomicUsize::new(0);
    let outcome = repository_with_policy(
        &runtime,
        Arc::new(MatrixPolicy {
            ids: fixture.ids,
            model_allowed: false,
            mode: ModelDataPolicyMode::Observe,
            authorization_result: PolicyDecisionResult::Allow,
        }),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    .expect("observe-only model denial must not replace allowed authorization");
    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 1, 1, 1, 0, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn denied_authorization_wins_even_when_model_policy_allows(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let adapter_calls = AtomicUsize::new(0);
    let outcome = repository_with_policy(
        &runtime,
        Arc::new(MatrixPolicy {
            ids: fixture.ids,
            model_allowed: true,
            mode: ModelDataPolicyMode::Enforce,
            authorization_result: PolicyDecisionResult::Deny,
        }),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    .expect("external authorization denial must take the denied path");
    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Denied
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 0, 0, 1, 0, 0, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn enforce_model_denial_cannot_pair_with_allowed_authorization(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let result = repository_with_policy(
        &runtime,
        Arc::new(MatrixPolicy {
            ids: fixture.ids,
            model_allowed: false,
            mode: ModelDataPolicyMode::Enforce,
            authorization_result: PolicyDecisionResult::Allow,
        }),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await;
    assert!(matches!(result, Err(ApplicationError::Policy(_))));
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );
}

/// Builds the one production evaluator over the production configured-capability
/// engine, so these two tests exercise no policy double at all.
fn configured_policy(
    admitted: DataDestination,
    mode: ModelDataPolicyMode,
) -> Arc<ConfiguredProviderDispatchPolicyEvaluator> {
    let engine = Arc::new(
        ConfiguredCapabilityPolicyEngine::new(
            "provider-dispatch-configured-v1",
            [Capability::ExportRead],
            RiskCategory::High,
        )
        .expect("the configured capability engine must accept this deployment"),
    );
    Arc::new(ConfiguredProviderDispatchPolicyEvaluator::new(
        engine,
        ModelDataPolicySettings {
            policy: DataPolicy::new(
                vestrace_domain::DataPolicyId::new(),
                "data-policy-v1",
                Sensitivity::Confidential,
                std::collections::BTreeSet::from([admitted]),
                None,
            )
            .expect("the data policy must accept one admitted destination"),
            classification: Sensitivity::Confidential,
            mode,
        },
    ))
}

#[sqlx::test(migrations = "../../migrations")]
async fn configured_policy_prepares_when_the_pinned_destination_is_admitted(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let adapter_calls = AtomicUsize::new(0);

    // The pinned revision is LM Studio on loopback, so the routing transaction
    // classifies it LocalModel and the data policy admits exactly that.
    let outcome = repository_with_policy(
        &runtime,
        configured_policy(DataDestination::LocalModel, ModelDataPolicyMode::Enforce),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    .expect("the configured evaluator must admit an allowed capability and destination");

    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 1, 1, 1, 0, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn configured_policy_denies_a_refused_destination_without_touching_admission(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let adapter_calls = AtomicUsize::new(0);

    // Capability is granted and the destination is refused under Enforce. A
    // naive evaluator would return an allowed authorization beside a denied
    // enforced record, which `enforce_model_denial_cannot_pair_with_allowed_authorization`
    // proves the repository rejects outright. The production evaluator must
    // instead deny the authorization, so this reaches the durable denied path.
    let outcome = repository_with_policy(
        &runtime,
        configured_policy(
            DataDestination::RemoteProvider,
            ModelDataPolicyMode::Enforce,
        ),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    .expect("an enforced destination refusal takes the denied path, not an error");

    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Denied
    );
    assert_eq!(
        adapter_calls.load(Ordering::SeqCst),
        0,
        "a refused destination must reach no adapter"
    );
    // Policy decision and intent are durable; admission, concurrency lease and
    // the dispatching transition are not.
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 0, 0, 1, 0, 0, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn configured_policy_proceeds_under_observe_and_keeps_the_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let adapter_calls = AtomicUsize::new(0);

    let outcome = repository_with_policy(
        &runtime,
        configured_policy(
            DataDestination::RemoteProvider,
            ModelDataPolicyMode::Observe,
        ),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    .expect("observe mode must not deny the dispatch");

    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);
    let (verdict, mode): (String, String) = sqlx::query_as(
        "SELECT verdict, mode FROM model_data_policy_decisions WHERE run_id=$1 AND step_id=$2",
    )
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("observe mode still commits its decision");
    assert_eq!(
        (verdict.as_str(), mode.as_str()),
        ("denied", "observe"),
        "the destination refusal must survive as a durable denied verdict recorded in observe mode"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn pinned_credential_and_request_auth_branches_must_match(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;

    let credential_fixture = setup_run_branch(&pool, &runtime, true).await;
    let missing = repository(
        &runtime,
        &credential_fixture,
        Arc::new(AtomicUsize::new(0)),
        None,
    )
    .prepare_dispatch(dispatch_request(&credential_fixture))
    .await;
    if !matches!(missing, Err(ApplicationError::Policy(_))) {
        match missing {
            Err(error) => panic!("credential omission returned {error}"),
            Ok(_) => panic!("credential omission was accepted"),
        }
    }
    assert_eq!(
        atomic_counts(&pool, &credential_fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );

    let no_auth_fixture = setup(&pool, &runtime).await;
    let mut supplied = dispatch_request(&no_auth_fixture);
    supplied.credential = Some(vestrace_application::ProviderDispatchCredential {
        lease_id: Uuid::now_v7(),
        credential_slot_id: CredentialSlotId::new(),
        credential_revision_id: Uuid::now_v7(),
        credential_activation_guard_id: Uuid::now_v7(),
        destination_authority: "api.example.test".into(),
        auth_mode: "bearer".into(),
    });
    let unexpected = repository(
        &runtime,
        &no_auth_fixture,
        Arc::new(AtomicUsize::new(0)),
        None,
    )
    .prepare_dispatch(supplied)
    .await;
    assert!(matches!(unexpected, Err(ApplicationError::Policy(_))));
    assert_eq!(
        atomic_counts(&pool, &no_auth_fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_metadata_must_match_the_pinned_authority(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for mutation in ["revision", "slot", "guard", "auth", "destination"] {
        let fixture = setup_run_branch(&pool, &runtime, true).await;
        let mut request = dispatch_request(&fixture);
        let mut credential = credential_request(&fixture);
        match mutation {
            "revision" => credential.credential_revision_id = Uuid::now_v7(),
            "slot" => credential.credential_slot_id = CredentialSlotId::new(),
            "guard" => credential.credential_activation_guard_id = Uuid::now_v7(),
            "auth" => credential.auth_mode = "api_key".into(),
            "destination" => credential.destination_authority = "other.example.test".into(),
            _ => unreachable!(),
        }
        request.credential = Some(credential);
        let result = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None)
            .prepare_dispatch(request)
            .await;
        assert!(
            matches!(result, Err(ApplicationError::Policy(_))),
            "{mutation}"
        );
        assert_eq!(
            atomic_counts(&pool, &fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "{mutation}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn caller_mutated_issued_lease_projection_rolls_back_every_dispatch_leg(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for mutation in [
        LeaseProjectionMutation::Authorization,
        LeaseProjectionMutation::ActivationGuard,
        LeaseProjectionMutation::Destination,
        LeaseProjectionMutation::ValidAuthMode,
        LeaseProjectionMutation::UnsupportedAuthMode,
    ] {
        let fixture = setup_run_branch(&pool, &runtime, true).await;
        let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
        let adapter_calls = AtomicUsize::new(0);
        let mut request = dispatch_request(&fixture);
        let audit_id = request.audit.id;
        let before_watermark = watermark(&pool).await;
        request.credential = Some(credential_request(&fixture));
        let result = credential_repository_with_projection_mutation(
            &runtime,
            &fixture,
            Arc::clone(&vault),
            mutation,
        )
        .prepare_dispatch(request)
        .await;
        match result {
            Err(ApplicationError::Policy(_)) => {}
            Err(error) => panic!("{mutation:?} returned {error}"),
            Ok(outcome) => {
                consume_prepared(outcome, &adapter_calls);
                panic!("{mutation:?} unexpectedly prepared")
            }
        }
        assert_eq!(vault.calls(), 0, "{mutation:?}");
        assert_eq!(adapter_calls.load(Ordering::SeqCst), 0, "{mutation:?}");
        assert_eq!(
            atomic_counts(&pool, &fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "{mutation:?}"
        );
        assert_eq!(audit_evidence(&pool, audit_id).await, (0, 0));
        assert_eq!(watermark(&pool).await, before_watermark);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn pinned_credential_must_remain_current_and_usable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for mutation in ["stale_version", "revoked"] {
        let fixture = setup_run_branch(&pool, &runtime, true).await;
        let credential = fixture.credential.expect("credential-backed fixture");
        let mut mutation_tx = pool.begin().await.unwrap();
        set_context(&mut mutation_tx, fixture.ids).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *mutation_tx)
            .await
            .unwrap();
        match mutation {
            "stale_version" => {
                sqlx::query(
                    "UPDATE credential_slots
                        SET current_revision_version=current_revision_version+1
                      WHERE id=$1 AND workspace_id=$2",
                )
                .bind(credential.slot.as_uuid())
                .bind(fixture.ids.workspace.as_uuid())
                .execute(&mut *mutation_tx)
                .await
                .unwrap();
            }
            "revoked" => {
                sqlx::query(
                    "UPDATE credential_slots
                        SET current_revision_id=NULL,
                            current_revision_version=current_revision_version+1,
                            tombstone_version=current_revision_version+1,
                            tombstoned_at=NOW()
                      WHERE id=$1 AND workspace_id=$2",
                )
                .bind(credential.slot.as_uuid())
                .bind(fixture.ids.workspace.as_uuid())
                .execute(&mut *mutation_tx)
                .await
                .unwrap();
                sqlx::query(
                    "UPDATE credential_key_creation_intents
                        SET state='retired', updated_at=NOW()
                      WHERE workspace_id=$1
                        AND credential_slot_id=$2
                        AND credential_revision_id=$3",
                )
                .bind(fixture.ids.workspace.as_uuid())
                .bind(credential.slot.as_uuid())
                .bind(credential.revision)
                .execute(&mut *mutation_tx)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        mutation_tx.commit().await.unwrap();

        let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
        let mut request = dispatch_request(&fixture);
        request.credential = Some(credential_request(&fixture));
        let result = credential_repository(&runtime, &fixture, vault.clone(), None)
            .prepare_dispatch(request)
            .await;
        assert!(
            matches!(result, Err(ApplicationError::Policy(_))),
            "{mutation}"
        );
        assert_eq!(vault.calls(), 0, "{mutation}");
        assert_eq!(
            atomic_counts(&pool, &fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "{mutation}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn preparer_frame_flows_through_dispatch_without_a_second_resolution(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup_run_branch(&pool, &runtime, true).await;
    let credential = fixture.credential.unwrap();
    assert_eq!(
        (
            credential.preparation_creates,
            credential.preparation_unwraps
        ),
        (1, 1),
        "the dispatch fixture must be produced through CredentialMaterialPreparer"
    );

    let persisted: (i64, bool, bool) = sqlx::query_as(
        "SELECT COUNT(*),
                bool_and(material.workspace_id=$1
                    AND intent.connection_id=$2
                    AND intent.credential_slot_id=$3
                    AND intent.credential_revision_id=$4
                    AND revision.material_key_id=$5
                    AND intent.id=$6
                    AND intent.nonce=$7),
                bool_and(position(convert_to('real-provider-credential-sentinel','UTF8')
                    in material.ciphertext)=0)
           FROM credential_prepared_materials AS material
           JOIN credential_key_creation_intents AS intent
             ON intent.workspace_id=material.workspace_id
            AND intent.id=material.intent_id
            AND intent.credential_revision_id=material.credential_revision_id
           JOIN credential_revisions AS revision
             ON revision.workspace_id=intent.workspace_id
            AND revision.id=intent.credential_revision_id
          WHERE material.intent_id=$6",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.connection.as_uuid())
    .bind(credential.slot.as_uuid())
    .bind(credential.revision)
    .bind(credential.material_key)
    .bind(credential.creation_intent)
    .bind(credential.nonce)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (1, true, true));

    let dispatch_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let adapter_calls = AtomicUsize::new(0);
    let mut request = dispatch_request(&fixture);
    request.credential = Some(credential_request(&fixture));
    let outcome = credential_repository(&runtime, &fixture, dispatch_vault.clone(), None)
        .prepare_dispatch(request)
        .await
        .unwrap();
    assert_eq!(dispatch_vault.calls(), 1);
    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_dispatch_success_and_faults_are_atomic_around_unwrap(pool: PgPool) {
    let _tracking_guard = TRACKING_TEST_GUARD.lock().await;
    let runtime = runtime_pool(&pool).await;

    let success = setup_run_branch(&pool, &runtime, true).await;
    let success_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let adapter_calls = AtomicUsize::new(0);
    let mut request = dispatch_request(&success);
    request.credential = Some(credential_request(&success));
    let outcome = credential_repository(&runtime, &success, success_vault.clone(), None)
        .prepare_dispatch(request)
        .await
        .unwrap();
    assert_eq!(success_vault.calls(), 1);
    assert_eq!(
        atomic_counts(&pool, &success).await,
        (1, 1, 1, 1, 1, 1, 1, 1, 1, 1)
    );
    let lease: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE consumed_at IS NOT NULL)
           FROM credential_dispatch_leases WHERE external_effect_id=$1",
    )
    .bind(success.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lease, (1, 1));
    assert_eq!(
        drive_prepared_adapter_only(outcome, &adapter_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 1);

    for (point, expected_unwraps, auth_allocated) in [
        (ProviderDispatchFaultPoint::BeforeCredentialIssue, 0, false),
        (ProviderDispatchFaultPoint::AfterCredentialIssue, 0, false),
        (
            ProviderDispatchFaultPoint::BeforeCredentialConsume,
            0,
            false,
        ),
        (ProviderDispatchFaultPoint::AfterCredentialConsume, 1, true),
        (ProviderDispatchFaultPoint::BeforeDispatching, 1, true),
        (ProviderDispatchFaultPoint::AfterDispatching, 1, true),
        (ProviderDispatchFaultPoint::BeforeGovernedCommit, 1, true),
        (ProviderDispatchFaultPoint::AfterGovernedCommit, 1, true),
    ] {
        TRACKED_CONTENT_POINTER.store(0, Ordering::SeqCst);
        TRACKED_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
        TRACKED_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
        TRACKED_CREDENTIAL_POINTER.store(0, Ordering::SeqCst);
        TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
        TRACKED_CREDENTIAL_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
        let fixture = setup_run_branch(&pool, &runtime, true).await;
        let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
        let adapter_calls = AtomicUsize::new(0);
        let mut request = dispatch_request(&fixture);
        let audit_id = request.audit.id;
        let before_watermark = watermark(&pool).await;
        request.credential = Some(credential_request(&fixture));
        let result = credential_repository(&runtime, &fixture, vault.clone(), Some(point))
            .prepare_dispatch(request)
            .await;
        let error = match result {
            Ok(outcome) => {
                consume_prepared(outcome, &adapter_calls);
                panic!("{point:?} unexpectedly committed")
            }
            Err(error) => error,
        };
        assert!(
            matches!(error, ApplicationError::Unavailable(_)),
            "{point:?}"
        );
        assert_eq!(vault.calls(), expected_unwraps, "{point:?}");
        assert_eq!(adapter_calls.load(Ordering::SeqCst), 0, "{point:?}");
        assert!(
            TRACKED_DEALLOCATION_OBSERVED.load(Ordering::SeqCst),
            "{point:?}: request allocation was not deallocated after rollback"
        );
        assert!(
            TRACKED_DEALLOCATION_ZERO.load(Ordering::SeqCst),
            "{point:?}: request allocation was not zero before deallocation"
        );
        if auth_allocated {
            assert!(
                TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.load(Ordering::SeqCst),
                "{point:?}: credential auth allocation was not deallocated after rollback"
            );
            assert!(
                TRACKED_CREDENTIAL_DEALLOCATION_ZERO.load(Ordering::SeqCst),
                "{point:?}: credential auth allocation was not zero before deallocation"
            );
        } else {
            assert_eq!(TRACKED_CREDENTIAL_POINTER.load(Ordering::SeqCst), 0);
        }
        assert_eq!(
            atomic_counts(&pool, &fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "{point:?}"
        );
        assert_eq!(audit_evidence(&pool, audit_id).await, (0, 0), "{point:?}");
        assert_eq!(watermark(&pool).await, before_watermark, "{point:?}");
    }

    TRACKED_CONTENT_POINTER.store(0, Ordering::SeqCst);
    TRACKED_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
    TRACKED_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
    TRACKED_CREDENTIAL_POINTER.store(0, Ordering::SeqCst);
    TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
    TRACKED_CREDENTIAL_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
    let fixture = setup_run_branch(&pool, &runtime, true).await;
    let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let adapter_calls = AtomicUsize::new(0);
    let mut request = dispatch_request(&fixture);
    let audit_id = request.audit.id;
    let before_watermark = watermark(&pool).await;
    request.credential = Some(credential_request(&fixture));
    let result = credential_repository_with_commit_error(&runtime, &fixture, vault.clone())
        .prepare_dispatch(request)
        .await;
    let error = match result {
        Ok(outcome) => {
            consume_prepared(outcome, &adapter_calls);
            panic!("deferred database commit error unexpectedly committed")
        }
        Err(error) => error,
    };
    assert!(matches!(error, ApplicationError::Storage(_)), "{error}");
    assert_eq!(vault.calls(), 1);
    assert_eq!(adapter_calls.load(Ordering::SeqCst), 0);
    assert!(TRACKED_DEALLOCATION_OBSERVED.load(Ordering::SeqCst));
    assert!(TRACKED_DEALLOCATION_ZERO.load(Ordering::SeqCst));
    assert!(TRACKED_CREDENTIAL_DEALLOCATION_OBSERVED.load(Ordering::SeqCst));
    assert!(TRACKED_CREDENTIAL_DEALLOCATION_ZERO.load(Ordering::SeqCst));
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );
    assert_eq!(audit_evidence(&pool, audit_id).await, (0, 0));
    assert_eq!(watermark(&pool).await, before_watermark);
}

#[sqlx::test(migrations = "../../migrations")]
async fn saturated_and_throttled_dispatches_commit_only_terminal_admission_evidence(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;

    let base = setup(&pool, &runtime).await;
    let sibling = setup_sibling(&pool, &runtime, &base).await;
    let first = repository(&runtime, &base, Arc::new(AtomicUsize::new(0)), None)
        .prepare_dispatch(dispatch_request(&base))
        .await
        .unwrap();
    let first_adapter = AtomicUsize::new(0);
    assert_eq!(
        drive_prepared_adapter_only(first, &first_adapter),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(first_adapter.load(Ordering::SeqCst), 1);
    let conflict_adapter = AtomicUsize::new(0);
    let conflict = repository(&runtime, &sibling, Arc::new(AtomicUsize::new(0)), None)
        .prepare_dispatch(dispatch_request(&sibling))
        .await
        .unwrap();
    assert_eq!(
        drive_prepared_adapter_only(conflict, &conflict_adapter),
        AdapterHarnessOutcome::Conflict {
            retry_after_seconds: None
        }
    );
    assert_eq!(conflict_adapter.load(Ordering::SeqCst), 0);
    assert_eq!(
        atomic_counts(&pool, &sibling).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 1)
    );
    let conflict_terminal: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1)",
    )
    .bind(sibling.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conflict_terminal, (0, 1));

    let throttled_base = setup(&pool, &runtime).await;
    let throttled_sibling = setup_sibling(&pool, &runtime, &throttled_base).await;
    let first = repository(
        &runtime,
        &throttled_base,
        Arc::new(AtomicUsize::new(0)),
        None,
    )
    .prepare_dispatch(dispatch_request(&throttled_base))
    .await
    .unwrap();
    let first_lease_id = match &first {
        ProviderDispatchOutcome::Prepared { authority, .. } => authority.concurrency_lease_id,
        _ => panic!("first dispatch was not prepared"),
    };
    let first_adapter = AtomicUsize::new(0);
    assert_eq!(
        drive_prepared_adapter_only(first, &first_adapter),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(first_adapter.load(Ordering::SeqCst), 1);
    let receipt_id = Uuid::now_v7();
    let mut throttle = runtime.begin().await.unwrap();
    set_context(&mut throttle, throttled_base.ids).await;
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id).bind(throttled_base.intent.id().as_uuid()).bind(throttled_base.ids.workspace.as_uuid())
        .bind(serde_json::json!({"response_class":"http_429","evidence_refs":["provider:http_429"]}))
        .execute(&mut *throttle).await.unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(throttled_base.intent.id().as_uuid()).bind(throttled_base.ids.workspace.as_uuid()).bind(receipt_id.to_string())
        .execute(&mut *throttle).await.unwrap();
    let released: Uuid = sqlx::query_scalar("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
        .bind(throttled_base.ids.workspace.as_uuid())
        .bind(throttled_base.intent.id().as_uuid())
        .bind(receipt_id)
        .fetch_one(&mut *throttle)
        .await
        .unwrap();
    assert_eq!(released, first_lease_id);
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_record_provider_throttle($1,$2,$3,$4,120)")
        .bind(Uuid::now_v7())
        .bind(throttled_base.ids.workspace.as_uuid())
        .bind(throttled_base.intent.id().as_uuid())
        .bind(receipt_id)
        .fetch_one(&mut *throttle)
        .await
        .unwrap();
    throttle.commit().await.unwrap();

    let throttle_adapter = AtomicUsize::new(0);
    let outcome = repository(
        &runtime,
        &throttled_sibling,
        Arc::new(AtomicUsize::new(0)),
        None,
    )
    .prepare_dispatch(dispatch_request(&throttled_sibling))
    .await
    .unwrap();
    let AdapterHarnessOutcome::Conflict {
        retry_after_seconds,
    } = drive_prepared_adapter_only(outcome, &throttle_adapter)
    else {
        panic!("active throttle did not return pre-dispatch conflict")
    };
    assert!(retry_after_seconds.is_some());
    assert_eq!(throttle_adapter.load(Ordering::SeqCst), 0);
    assert_eq!(
        atomic_counts(&pool, &throttled_sibling).await,
        (0, 1, 1, 0, 0, 0, 0, 0, 0, 1)
    );
    let throttle_terminal: (String, i64) = sqlx::query_as(
        "SELECT admission.decision,
                (SELECT COUNT(*) FROM provider_admission_waits
                  WHERE external_effect_id=admission.external_effect_id)
           FROM connection_dispatch_admissions AS admission
          WHERE admission.external_effect_id=$1",
    )
    .bind(throttled_sibling.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(throttle_terminal, ("throttled".into(), 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_probe_dispatches_with_exact_no_auth_or_credential_binding(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;

    let no_auth = setup_qualification_branch(&pool, &runtime, false).await;
    let no_auth_calls = AtomicUsize::new(0);
    let no_auth_outcome = qualification_repository(&runtime, None)
        .prepare_dispatch(qualification_request(&no_auth))
        .await
        .expect("exact no-auth qualification probe must prepare");
    assert_eq!(
        drive_prepared_adapter_only(no_auth_outcome, &no_auth_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(no_auth_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        atomic_counts(&pool, &no_auth.fixture).await,
        (0, 1, 1, 1, 1, 0, 1, 1, 1, 1)
    );

    let credential = setup_qualification_branch(&pool, &runtime, true).await;
    let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let credential_calls = AtomicUsize::new(0);
    let mut request = qualification_request(&credential);
    request.credential = Some(credential_request(&credential.fixture));
    let credential_outcome = qualification_repository(&runtime, Some(vault.clone()))
        .prepare_dispatch(request)
        .await
        .expect("exact credential qualification probe must prepare");
    assert_eq!(
        drive_prepared_adapter_only(credential_outcome, &credential_calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(credential_calls.load(Ordering::SeqCst), 1);
    assert_eq!(vault.calls(), 1);
    assert_eq!(
        atomic_counts(&pool, &credential.fixture).await,
        (0, 1, 1, 1, 1, 1, 1, 1, 1, 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_dispatch_returns_the_exact_complete_q1_request_for_a_qualification_probe(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch_for_ordinal(&pool, &runtime, false, "10").await;
    let mut request = qualification_request(&qualification);
    request.cause = ProviderDispatchCause::QualificationProbe {
        qualification_job_id: qualification.job,
        qualification_target_id: qualification.target,
        probe_ordinal: "10".to_owned(),
    };

    let outcome = qualification_repository(&runtime, None)
        .prepare_dispatch(request)
        .await
        .expect("a Complete q1 MRE must be reconstructed inside shared dispatch");
    let ProviderDispatchOutcome::Prepared {
        request: EffectiveModelRequest::ModelsList,
        q1_request: Some(request),
        ..
    } = outcome
    else {
        panic!("qualification dispatch did not return its closed Q1 request");
    };
    assert!(matches!(*request, Q1ProbeRequest::ModelsList));
}

#[sqlx::test(migrations = "../../migrations")]
async fn static_q1_probes_create_neither_effect_nor_mre(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()));
    let before_00: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE qualification_job_id=$2)",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();

    let state = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "00".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: None,
                model_request_evidence_id: None,
            },
        )
        .await
        .expect("static q1 ordinal 00 must persist without provider artifacts");
    assert_eq!(state, vestrace_domain::QualificationJobState::Running);
    let after_00: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE qualification_job_id=$2)",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_00, before_00);

    let prepared = qualification_repository(&runtime, None)
        .prepare_dispatch(qualification_request(&qualification))
        .await
        .expect("the intervening q1 models-list probe must be the one network dispatch");
    let ProviderDispatchOutcome::Prepared { authority, .. } = prepared else {
        panic!("q1 ordinal 10 did not prepare a network dispatch")
    };
    let state = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "10".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(authority.effect_id.as_uuid()),
                model_request_evidence_id: Some(qualification.fixture.ids.evidence.as_uuid()),
            },
        )
        .await
        .expect("the exact prepared q1 models-list cause must complete ordinal 10");
    assert_eq!(state, vestrace_domain::QualificationJobState::Running);

    let before_15: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE qualification_job_id=$2)",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let state = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "15".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: None,
                model_request_evidence_id: None,
            },
        )
        .await
        .expect("static q1 ordinal 15 must persist without provider artifacts");
    assert_eq!(state, vestrace_domain::QualificationJobState::Running);

    let after: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE workspace_id=$1),
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE qualification_job_id=$2),
            (SELECT COUNT(*) FROM qualification_probe_results
              WHERE qualification_job_id=$2 AND probe_ordinal IN ('00','15')
                AND external_effect_id IS NULL AND model_request_evidence_id IS NULL),
            (SELECT COUNT(*) FROM qualification_probe_results WHERE qualification_job_id=$2)",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((after.0, after.1, after.2), before_15);
    assert_eq!((after.3, after.4), (2, 3));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_prerequisite_dependents_cannot_pass_after_an_unsupported_predecessor(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime));
    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(&pool, &qualification, ordinal).await,
        );
    }

    for (ordinal, result) in [
        ("00", QualificationProbeResult::Pass),
        ("10", QualificationProbeResult::Pass),
        ("15", QualificationProbeResult::Pass),
        ("20", QualificationProbeResult::Pass),
        ("30", QualificationProbeResult::UnsupportedDefinite),
    ] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        repository
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the exact predecessor chain must persist");
    }

    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM qualification_probe_results WHERE qualification_job_id=$1",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let (effect_id, evidence_id) = network["35"];
    let refusal = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "35".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(effect_id),
                model_request_evidence_id: Some(evidence_id),
            },
        )
        .await
        .expect_err("ordinal 35 must not pass after unsupported stream predecessor 30");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("q1 probe requires its exact passing predecessor")
    ));
    let after: (i64, String) = sqlx::query_as(
        "SELECT COUNT(*), (SELECT state FROM qualification_jobs WHERE id=$1) \
         FROM qualification_probe_results WHERE qualification_job_id=$1",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (before, "running".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_tool_dependents_cannot_pass_after_an_unsupported_tool_predecessor(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime));
    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35", "40", "50", "60"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(&pool, &qualification, ordinal).await,
        );
    }
    for (ordinal, result) in [
        ("00", QualificationProbeResult::Pass),
        ("10", QualificationProbeResult::Pass),
        ("15", QualificationProbeResult::Pass),
        ("20", QualificationProbeResult::Pass),
        ("30", QualificationProbeResult::Pass),
        ("35", QualificationProbeResult::Pass),
        ("40", QualificationProbeResult::UnsupportedDefinite),
    ] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        repository
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the exact predecessor chain must persist");
    }
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM qualification_probe_results WHERE qualification_job_id=$1",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let (effect_id, evidence_id) = network["50"];
    let refusal = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "50".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(effect_id),
                model_request_evidence_id: Some(evidence_id),
            },
        )
        .await
        .expect_err("ordinal 50 must not pass after unsupported ordinal 40");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("q1 probe requires its exact passing predecessor")
    ));
    repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "50".to_owned(),
                result: QualificationProbeResult::SkippedPrerequisite,
                external_effect_id: Some(effect_id),
                model_request_evidence_id: Some(evidence_id),
            },
        )
        .await
        .expect("ordinal 50 must record the exact permitted prerequisite skip");
    let (effect_id, evidence_id) = network["60"];
    let refusal = repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "60".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(effect_id),
                model_request_evidence_id: Some(evidence_id),
            },
        )
        .await
        .expect_err("ordinal 60 must not pass after unsupported ordinal 40");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("q1 probe requires its exact passing predecessor")
    ));
    let after: (i64, String) = sqlx::query_as(
        "SELECT COUNT(*), (SELECT state FROM qualification_jobs WHERE id=$1) \
         FROM qualification_probe_results WHERE qualification_job_id=$1",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (before + 1, "running".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_request_refuses_a_stale_credential_slot_version_before_binding(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup_run_branch(&pool, &runtime, true).await;
    let credential = fixture.credential.expect("credential-backed fixture");
    let job_id = QualificationJobId::new();
    let binding_id = Uuid::now_v7();
    let mut transaction = runtime.begin().await.unwrap();
    set_context(&mut transaction, fixture.ids).await;
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_request_qualification_job(\
         $1,$2,$3,$4,$5,'openai-chat-completions-v1/q1','credential',$6,$7,$8,$9,NULL,$10,$11)",
    )
    .bind(job_id.as_uuid())
    .bind(binding_id)
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.connection.as_uuid())
    .bind(fixture.ids.revision.as_uuid())
    .bind(credential.revision)
    .bind(credential.slot.as_uuid())
    .bind(credential.activation_guard)
    .bind(0_i64)
    .bind(fixture.chat_model_revision_id)
    .bind(fixture.embedding_model_revision_id)
    .fetch_one(&mut *transaction)
    .await
    .expect_err("stale credential slot version must be refused before a target is persisted");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    let persisted: (i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT COUNT(*) FROM qualification_jobs WHERE id=$1),
             (SELECT COUNT(*) FROM qualification_target_bindings WHERE id=$2)",
    )
    .bind(job_id.as_uuid())
    .bind(binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (0, 0));
}

async fn record_complete_q1_matrix(
    owner: &PgPool,
    runtime: &PgPool,
    qualification: &QualificationFixture,
) {
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()));
    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35", "40", "50", "60", "70", "80", "90"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(owner, qualification, ordinal).await,
        );
    }
    for ordinal in [
        "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80", "90",
    ] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        repository
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result: QualificationProbeResult::Pass,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the complete q1 fixture must record every ordinal");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_finalizer_refuses_a_stale_no_auth_connection_head_before_publishing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    record_complete_q1_matrix(&pool, &runtime, &qualification).await;

    let mut stale_head = pool.begin().await.unwrap();
    set_context(&mut stale_head, qualification.fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *stale_head)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE connection_revision_heads SET state='disabled'
          WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.fixture.ids.connection.as_uuid())
    .execute(&mut *stale_head)
    .await
    .unwrap();
    stale_head.commit().await.unwrap();

    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let refusal = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()))
        .finalize_success(
            &context,
            vestrace_application::QualificationFinalization {
                job_id: qualification.job,
                connection_qualification_revision_id:
                    vestrace_domain::ConnectionQualificationRevisionId::new(),
                chat_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
                embedding_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
            },
        )
        .await
        .expect_err("a no-auth finalizer may not publish after its pinned head becomes stale");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("qualification finalization target is no longer current")
    ));
    let facts: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT state FROM qualification_jobs WHERE id=$1),
             (SELECT COUNT(*) FROM connection_qualification_revisions WHERE qualification_job_id=$1),
             (SELECT COUNT(*) FROM model_qualification_revisions WHERE qualification_job_id=$1),
             (SELECT COUNT(*) FROM connection_qualification_heads AS head
               JOIN qualification_target_bindings AS target
                 ON target.workspace_id=head.workspace_id
                AND target.connection_revision_id=head.connection_revision_id
              WHERE target.qualification_job_id=$1)",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts, ("running".to_owned(), 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_finalizer_refuses_a_stale_candidate_slot_before_publishing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, true).await;
    record_complete_q1_matrix(&pool, &runtime, &qualification).await;
    let credential = qualification
        .fixture
        .credential
        .expect("credential-backed qualification fixture");

    let mut stale_slot = pool.begin().await.unwrap();
    set_context(&mut stale_slot, qualification.fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *stale_slot)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE credential_slots SET current_revision_version=current_revision_version+1
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(credential.slot.as_uuid())
    .execute(&mut *stale_slot)
    .await
    .unwrap();
    stale_slot.commit().await.unwrap();

    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let refusal = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()))
        .finalize_success(
            &context,
            vestrace_application::QualificationFinalization {
                job_id: qualification.job,
                connection_qualification_revision_id:
                    vestrace_domain::ConnectionQualificationRevisionId::new(),
                chat_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
                embedding_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
            },
        )
        .await
        .expect_err("a Candidate finalizer may not publish after the slot CAS version changes");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("qualification finalization target is no longer current")
    ));
    let facts: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT state FROM qualification_jobs WHERE id=$1),
             (SELECT COUNT(*) FROM connection_qualification_revisions WHERE qualification_job_id=$1),
             (SELECT COUNT(*) FROM model_qualification_revisions WHERE qualification_job_id=$1),
             (SELECT COUNT(*) FROM connection_qualification_heads AS head
               JOIN qualification_target_bindings AS target
                 ON target.workspace_id=head.workspace_id
                AND target.connection_revision_id=head.connection_revision_id
              WHERE target.qualification_job_id=$1)",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts, ("running".to_owned(), 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_finalizer_locks_the_canonical_guard_before_publishing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    record_complete_q1_matrix(&pool, &runtime, &qualification).await;

    let mut blocker = pool.begin().await.unwrap();
    set_context(&mut blocker, qualification.fixture.ids).await;
    sqlx::query(
        "SELECT id FROM connection_execution_guards
          WHERE workspace_id=$1 AND connection_id=$2 FOR UPDATE",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.fixture.ids.connection.as_uuid())
    .execute(&mut *blocker)
    .await
    .unwrap();

    let mut finalizer = runtime.begin().await.unwrap();
    set_context(&mut finalizer, qualification.fixture.ids).await;
    sqlx::query("SET LOCAL lock_timeout='50ms'")
        .execute(&mut *finalizer)
        .await
        .unwrap();
    let refusal =
        sqlx::query_scalar::<_, ()>("SELECT vestrace_finalize_qualification_job($1,$2,$3,$4,$5)")
            .bind(qualification.fixture.ids.workspace.as_uuid())
            .bind(qualification.job.as_uuid())
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .fetch_one(&mut *finalizer)
            .await
            .expect_err(
                "the finalizer must acquire the permanent connection guard before it can publish",
            );
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55P03"),
        "the locked canonical guard must be the deterministic admission barrier",
    );
    finalizer.rollback().await.unwrap();
    blocker.rollback().await.unwrap();
    let facts: (String, i64) = sqlx::query_as(
        "SELECT
             (SELECT state FROM qualification_jobs WHERE id=$1),
             (SELECT COUNT(*) FROM connection_qualification_revisions WHERE qualification_job_id=$1)",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts, ("running".to_owned(), 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_finalizer_accepts_only_the_complete_twelve_probe_terminal_matrix(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime));
    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35", "40", "50", "60", "70", "80", "90"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(&pool, &qualification, ordinal).await,
        );
    }

    for ordinal in [
        "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80",
    ] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        let state = repository
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result: QualificationProbeResult::Pass,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the exact q1 matrix fixture must record in protocol order");
        assert_eq!(state, vestrace_domain::QualificationJobState::Running);
    }

    let finalization = vestrace_application::QualificationFinalization {
        job_id: qualification.job,
        connection_qualification_revision_id:
            vestrace_domain::ConnectionQualificationRevisionId::new(),
        chat_model_qualification_revision_id: vestrace_domain::ModelQualificationRevisionId::new(),
        embedding_model_qualification_revision_id:
            vestrace_domain::ModelQualificationRevisionId::new(),
    };
    let refusal = repository
        .finalize_success(&context, finalization)
        .await
        .expect_err("the finalizer must refuse an otherwise-complete matrix missing ordinal 90");
    assert!(matches!(
        refusal,
        ApplicationError::Storage(message)
            if message.contains("qualification finalization requires the exact q1 terminal matrix")
    ));
    let (effect_id, evidence_id) = network["90"];
    repository
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "90".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(effect_id),
                model_request_evidence_id: Some(evidence_id),
            },
        )
        .await
        .expect("the final required embedding probe must complete before finalization");
    repository
        .finalize_success(&context, finalization)
        .await
        .expect(
            "the complete q1 terminal matrix must publish exactly one pinned qualification tuple",
        );

    let observed: (String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT state FROM qualification_jobs WHERE id=$1),
            (SELECT COUNT(*) FROM qualification_probe_results WHERE qualification_job_id=$1),
            (SELECT COUNT(*) FROM connection_qualification_revisions WHERE qualification_job_id=$1),
            (SELECT COUNT(*) FROM model_qualification_revisions WHERE qualification_job_id=$1),
            (SELECT COUNT(*) FROM connection_qualification_heads AS head
              JOIN qualification_target_bindings AS target
                ON target.connection_revision_id=head.connection_revision_id
             WHERE target.qualification_job_id=$1)",
    )
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observed, ("succeeded".into(), 12, 1, 2, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn complete_q1_mre_reconstructs_the_exact_closed_adapter_request(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let (_, q1_evidence_id) =
        insert_finalizable_q1_network_probe(&pool, &qualification, "50").await;
    let mut inspection = runtime.begin().await.unwrap();
    set_context(&mut inspection, qualification.fixture.ids).await;
    let durable_tuple_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM model_request_evidence_roots AS root
           JOIN qualification_q1_mre_sources AS source
             ON source.workspace_id=root.workspace_id AND source.evidence_root_id=root.id
           JOIN qualification_target_bindings AS target
             ON target.workspace_id=root.workspace_id AND target.id=root.qualification_target_binding_id
           JOIN model_revisions AS chat
             ON chat.workspace_id=target.workspace_id AND chat.id=target.chat_model_revision_id
           JOIN model_revisions AS embedding
             ON embedding.workspace_id=target.workspace_id AND embedding.id=target.embedding_model_revision_id
          WHERE root.workspace_id=$1 AND root.id=$2",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(q1_evidence_id)
    .fetch_one(&mut *inspection)
    .await
    .unwrap();
    inspection.rollback().await.unwrap();
    assert_eq!(
        durable_tuple_count, 1,
        "fixture must contain every durable q1 source pin"
    );
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let permit_authority = PgInstallationMutationPermit::new(PgStore::from_pool(runtime));
    let mut permit = permit_authority
        .acquire(PermitMode::Shared, &context)
        .await
        .unwrap();
    let repository =
        PgModelRequestEvidenceRepository::new(Arc::new(CountingVault(AtomicUsize::new(0))));
    let request = repository
        .reconstruct_q1_in(
            permit.unit_of_work_mut(),
            qualification.fixture.ids.workspace,
            ModelRequestEvidenceId::from_uuid(q1_evidence_id),
        )
        .await
        .expect("a Complete q1 MRE must reconstruct its closed adapter request");
    let Q1ProbeRequest::Chat(chat) = request else {
        panic!("ordinal 50 must reconstruct a typed chat request");
    };
    assert_eq!(chat.ordinal(), "50");
    assert_eq!(chat.model(), "model");
    assert_eq!(chat.messages().len(), 3);
    assert!(matches!(
        chat.messages(),
        [
            Q1ChatMessage::AssistantToolCall { call_id, .. },
            Q1ChatMessage::ToolResult { call_id: tool_call_id, .. },
            Q1ChatMessage::UserText(_)
        ] if call_id == "call_q1" && tool_call_id == "call_q1"
    ));
    permit.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_probe_may_consume_only_its_pinned_candidate_credential(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let candidate = setup_qualification_branch(&pool, &runtime, true).await;
    let mut guarded = pool.begin().await.unwrap();
    set_context(&mut guarded, candidate.fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE credential_guard_occupancies
            SET state='candidate'
          WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(candidate.fixture.ids.workspace.as_uuid())
    .bind(candidate.fixture.ids.connection.as_uuid())
    .execute(&mut *guarded)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE credential_key_creation_intents
            SET state='candidate'
          WHERE workspace_id=$1 AND connection_id=$2 AND state='active'",
    )
    .bind(candidate.fixture.ids.workspace.as_uuid())
    .bind(candidate.fixture.ids.connection.as_uuid())
    .execute(&mut *guarded)
    .await
    .unwrap();
    guarded.commit().await.unwrap();

    let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let mut request = qualification_request(&candidate);
    request.credential = Some(credential_request(&candidate.fixture));
    let outcome = qualification_repository(&runtime, Some(vault.clone()))
        .prepare_dispatch(request)
        .await
        .expect("only the matching q1 Candidate target/cause may issue and consume a lease");
    let calls = AtomicUsize::new(0);
    assert_eq!(
        drive_prepared_adapter_only(outcome, &calls),
        AdapterHarnessOutcome::Prepared
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(vault.0.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_result_prepare_after_live_dispatch_commits_result_receipt_witness_and_release(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let dispatch = Arc::new(repository(
        &runtime,
        &fixture,
        Arc::new(AtomicUsize::new(0)),
        None,
    ));
    let dispatch_for_result: SharedProviderDispatchRepository = dispatch.clone();
    let outcome = dispatch
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("the real Run dispatch must create its live admission lease first");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("Run dispatch did not return a live authority")
    };
    let identities = seed_provider_result_output(&pool, &fixture).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    let results = provider_result_repository_after_live_dispatch(&runtime, dispatch_for_result);
    let prepared = results
        .prepare_after_dispatch(
            PrepareProviderResult {
                context: context.clone(),
                effect_id: fixture.intent.id(),
                run_id: fixture.ids.run,
                step_id: fixture.ids.step,
                identities,
                result: EffectiveChatResult::new(
                    zeroize::Zeroizing::new("live dispatch result".to_owned()),
                    EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                    ProviderUsage::Known {
                        prompt_tokens: 3,
                        completion_tokens: 5,
                    },
                )
                .unwrap(),
            },
            &authority,
        )
        .await
        .expect("one real provider result must commit its receipt witness and release together");
    sqlx::query("DELETE FROM artifact_revisions WHERE id=$1")
        .bind(identities.artifact_revision_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(identities.artifact_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_executions WHERE id=$1")
        .bind(identities.model_execution_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(prepared.effect_id, fixture.intent.id());
    assert_eq!(prepared.identities, identities);

    let observed: (i64, i64, i64, i64, Option<Uuid>, String, String) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM provider_result_preparations
              WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_receipts
              WHERE effect_id=$1 AND id=$2),
            (SELECT COUNT(*) FROM provider_result_preparations
              WHERE external_effect_id=$1 AND external_effect_receipt_id=$2
                AND receipt_witnessed_at IS NOT NULL),
            (SELECT COUNT(*) FROM provider_concurrency_leases
              WHERE external_effect_id=$1 AND released_receipt_id=$2
                AND released_at IS NOT NULL),
            (SELECT released_receipt_id FROM provider_concurrency_leases
              WHERE external_effect_id=$1),
            (SELECT status FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 ORDER BY ordinal DESC LIMIT 1),
            (SELECT phase FROM run_step_execution_attempts
              WHERE workspace_id=$3 AND run_id=$4 AND step_id=$5)",
    )
    .bind(fixture.intent.id().as_uuid())
    .bind(identities.receipt_id.as_uuid())
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observed,
        (
            1,
            1,
            1,
            1,
            Some(identities.receipt_id.as_uuid()),
            "acknowledged".to_owned(),
            "result_prepared".to_owned(),
        )
    );
    assert_eq!(
        dispatch
            .recover_run_step_attempt(&context, fixture.ids.run, fixture.ids.step, Utc::now())
            .await
            .expect("the durable result-prepared attempt must resume its original effect"),
        RunStepAttemptRecovery::ResumeResultPrepared {
            effect_id: fixture.intent.id(),
        }
    );

    let mut premature_publication = runtime.begin().await.unwrap();
    set_context(&mut premature_publication, fixture.ids).await;
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'published')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *premature_publication)
    .await
    .expect_err(
        "a caller-named published phase must be refused before exact publication and Run continuation evidence exist",
    );
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    assert_eq!(
        refusal
            .as_database_error()
            .map(|database| database.message()),
        Some("run step execution attempt publication is not exact")
    );
    premature_publication.rollback().await.unwrap();
    let phase_after_premature_publication: String = sqlx::query_scalar(
        "SELECT phase FROM run_step_execution_attempts \
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(phase_after_premature_publication, "result_prepared");

    let binding_receipt = MaterialKeyBindingReceipt::new();
    let publication = results
        .finalize(
            &context,
            FinalizeProviderResult {
                prepared: prepared.clone(),
                binding_receipt,
            },
        )
        .await
        .expect("the governed Run result finalizer must publish its original attempt");
    assert_eq!(publication.preparation_id, prepared.preparation_id);
    assert_eq!(publication.artifact_id, prepared.identities.artifact_id);
    assert_eq!(
        publication.artifact_revision_id,
        prepared.identities.artifact_revision_id
    );
    assert_eq!(
        publication.model_execution_id,
        prepared.identities.model_execution_id
    );

    let expected_run_version: i64 = sqlx::query_scalar(
        "SELECT expected_run_version FROM provider_result_preparations WHERE id=$1",
    )
    .bind(prepared.preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let finalized: (i64, i64, String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM provider_result_publications
              WHERE provider_result_preparation_id=$1),
            (SELECT COUNT(*) FROM artifact_revision_contents
              WHERE provider_result_preparation_id=$1
                AND workspace_id=$2 AND artifact_id=$3 AND artifact_revision_id=$4
                AND model_execution_id=$5),
            (SELECT status FROM run_steps
              WHERE workspace_id=$2 AND run_id=$6 AND id=$7),
            (SELECT run_version FROM agent_runs
              WHERE workspace_id=$2 AND id=$6),
            (SELECT current_version FROM run_streams
              WHERE workspace_id=$2 AND run_id=$6),
            (SELECT COUNT(*) FROM run_events
              WHERE workspace_id=$2 AND run_id=$6 AND run_version=$8
                AND event_type='run.step_status_changed'
                AND payload_kind='run.step_status_changed'
                AND payload=jsonb_build_object(
                    'type','step_status_changed','step_id',$7::uuid,
                    'from','running','to','succeeded','attempt',1)),
            (SELECT COUNT(*) FROM run_work_items
              WHERE id=$9 AND workspace_id=$2 AND run_id=$6 AND step_id=$7
                AND kind='advance_run' AND expected_run_version=$8
                AND idempotency_key=format('provider-result:%s:%s',$6,$4))",
    )
    .bind(prepared.preparation_id)
    .bind(fixture.ids.workspace.as_uuid())
    .bind(prepared.identities.artifact_id.as_uuid())
    .bind(prepared.identities.artifact_revision_id.as_uuid())
    .bind(prepared.identities.model_execution_id.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(expected_run_version + 1)
    .bind(prepared.identities.advance_work_item_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        finalized,
        (
            1,
            1,
            "succeeded".to_owned(),
            expected_run_version + 1,
            expected_run_version + 1,
            1,
            1
        )
    );

    let replay = results
        .finalize(
            &context,
            FinalizeProviderResult {
                prepared: prepared.clone(),
                binding_receipt,
            },
        )
        .await
        .expect(
            "the exact governed Run finalizer replay must converge without a second continuation",
        );
    assert_eq!(replay, publication);
    let replayed: (i64, i64, String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM provider_result_publications
              WHERE provider_result_preparation_id=$1),
            (SELECT COUNT(*) FROM artifact_revision_contents
              WHERE provider_result_preparation_id=$1
                AND workspace_id=$2 AND artifact_id=$3 AND artifact_revision_id=$4
                AND model_execution_id=$5),
            (SELECT status FROM run_steps
              WHERE workspace_id=$2 AND run_id=$6 AND id=$7),
            (SELECT run_version FROM agent_runs
              WHERE workspace_id=$2 AND id=$6),
            (SELECT current_version FROM run_streams
              WHERE workspace_id=$2 AND run_id=$6),
            (SELECT COUNT(*) FROM run_events
              WHERE workspace_id=$2 AND run_id=$6 AND run_version=$8
                AND event_type='run.step_status_changed'
                AND payload_kind='run.step_status_changed'
                AND payload=jsonb_build_object(
                    'type','step_status_changed','step_id',$7::uuid,
                    'from','running','to','succeeded','attempt',1)),
            (SELECT COUNT(*) FROM run_work_items
              WHERE id=$9 AND workspace_id=$2 AND run_id=$6 AND step_id=$7
                AND kind='advance_run' AND expected_run_version=$8
                AND idempotency_key=format('provider-result:%s:%s',$6,$4))",
    )
    .bind(prepared.preparation_id)
    .bind(fixture.ids.workspace.as_uuid())
    .bind(prepared.identities.artifact_id.as_uuid())
    .bind(prepared.identities.artifact_revision_id.as_uuid())
    .bind(prepared.identities.model_execution_id.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(expected_run_version + 1)
    .bind(prepared.identities.advance_work_item_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(replayed, finalized);
    let published_phase: String = sqlx::query_scalar(
        "SELECT phase FROM run_step_execution_attempts \
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(published_phase, "published");
    assert_eq!(
        dispatch
            .recover_run_step_attempt(&context, fixture.ids.run, fixture.ids.step, Utc::now())
            .await
            .expect("the published Run attempt must no-op on recovery"),
        RunStepAttemptRecovery::Published
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_result_finalize_after_live_dispatch_rolls_back_run_success_after_before_run_success_fault(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let dispatch = Arc::new(repository(
        &runtime,
        &fixture,
        Arc::new(AtomicUsize::new(0)),
        None,
    ));
    let dispatch_for_prepare: SharedProviderDispatchRepository = dispatch.clone();
    let dispatch_for_finalize: SharedProviderDispatchRepository = dispatch.clone();
    let outcome = dispatch
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("the real Run dispatch must create its live admission lease first");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("Run dispatch did not return a live authority")
    };
    let identities = seed_provider_result_output(&pool, &fixture).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);
    let prepared = provider_result_repository_after_live_dispatch(&runtime, dispatch_for_prepare)
        .prepare_after_dispatch(
            PrepareProviderResult {
                context: context.clone(),
                effect_id: fixture.intent.id(),
                run_id: fixture.ids.run,
                step_id: fixture.ids.step,
                identities,
                result: EffectiveChatResult::new(
                    zeroize::Zeroizing::new(
                        "live dispatch result before Run success fault".to_owned(),
                    ),
                    EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                    ProviderUsage::Known {
                        prompt_tokens: 3,
                        completion_tokens: 5,
                    },
                )
                .unwrap(),
            },
            &authority,
        )
        .await
        .expect("prepare-after-dispatch must commit before the finalizer fault");

    let baseline: (String, i64) = sqlx::query_as(
        "SELECT
            (SELECT state FROM material_key_creation_intents WHERE id=$1),
            (SELECT current_version FROM run_streams WHERE workspace_id=$2 AND run_id=$3)",
    )
    .bind(prepared.identities.material_intent_id.as_uuid())
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(baseline, ("result_prepared".to_owned(), 0));

    sqlx::query("DELETE FROM artifact_revisions WHERE id=$1")
        .bind(identities.artifact_revision_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(identities.artifact_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_executions WHERE id=$1")
        .bind(identities.model_execution_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();

    let error = faulting_provider_result_repository_after_live_dispatch(
        &runtime,
        dispatch_for_finalize,
        ProviderResultFaultPoint::BeforeRunSuccess,
    )
    .finalize(
        &context,
        FinalizeProviderResult {
            prepared: prepared.clone(),
            binding_receipt: MaterialKeyBindingReceipt::new(),
        },
    )
    .await
    .expect_err("the injected pre-Run-success fault must abort the governed finalizer");
    assert!(
        matches!(error, ApplicationError::Internal(message) if message == "injected provider-result fault at BeforeRunSuccess")
    );

    let observed: (String, String, String, String, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
                (SELECT phase FROM run_step_execution_attempts
                  WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3),
                (SELECT state FROM provider_result_preparations WHERE id=$4),
                (SELECT state FROM material_key_creation_intents WHERE id=$5),
                (SELECT status FROM run_steps
                  WHERE workspace_id=$1 AND run_id=$2 AND id=$3),
                (SELECT run_version FROM agent_runs WHERE workspace_id=$1 AND id=$2),
                (SELECT current_version FROM run_streams WHERE workspace_id=$1 AND run_id=$2),
                (SELECT COUNT(*) FROM provider_result_publications
                  WHERE provider_result_preparation_id=$4),
                (SELECT COUNT(*) FROM artifact_revision_contents
                  WHERE provider_result_preparation_id=$4),
                (SELECT COUNT(*) FROM run_events
                  WHERE workspace_id=$1 AND run_id=$2 AND run_version=2
                    AND event_type='run.step_status_changed'
                    AND payload_kind='run.step_status_changed'
                    AND payload=jsonb_build_object(
                        'type','step_status_changed','step_id',$3::uuid,
                        'from','running','to','succeeded','attempt',1)),
                (SELECT COUNT(*) FROM run_work_items
                  WHERE id=$6 AND workspace_id=$1 AND run_id=$2 AND step_id=$3
                    AND kind='advance_run' AND expected_run_version=2
                    AND idempotency_key=format('provider-result:%s:%s',$2,$7))",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(prepared.preparation_id)
    .bind(prepared.identities.material_intent_id.as_uuid())
    .bind(prepared.identities.advance_work_item_id.as_uuid())
    .bind(prepared.identities.artifact_revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observed,
        (
            "result_prepared".to_owned(),
            "result_prepared".to_owned(),
            baseline.0,
            "running".to_owned(),
            1,
            baseline.1,
            0,
            0,
            0,
            0,
        )
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_result_prepare_after_live_dispatch_rolls_back_receipt_witness_and_release_after_fault(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let dispatch = Arc::new(repository(
        &runtime,
        &fixture,
        Arc::new(AtomicUsize::new(0)),
        None,
    ));
    let dispatch_for_result: SharedProviderDispatchRepository = dispatch.clone();
    let outcome = dispatch
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("the real Run dispatch must create its live admission lease first");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("Run dispatch did not return a live authority")
    };
    let identities = seed_provider_result_output(&pool, &fixture).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    let error = faulting_provider_result_repository_after_live_dispatch(
        &runtime,
        dispatch_for_result,
        ProviderResultFaultPoint::AfterWitness,
    )
    .prepare_after_dispatch(
        PrepareProviderResult {
            context,
            effect_id: fixture.intent.id(),
            run_id: fixture.ids.run,
            step_id: fixture.ids.step,
            identities,
            result: EffectiveChatResult::new(
                zeroize::Zeroizing::new("live dispatch result fault".to_owned()),
                EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                ProviderUsage::Known {
                    prompt_tokens: 3,
                    completion_tokens: 5,
                },
            )
            .unwrap(),
        },
        &authority,
    )
    .await
    .expect_err("a post-witness failure must roll back every provider-result completion leg");
    assert!(
        matches!(error, ApplicationError::Internal(message) if message == "injected provider-result fault at AfterWitness")
    );

    let observed: (i64, i64, i64, i64, i64, String, String) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM material_key_creation_intents WHERE id=$1),
            (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$2),
            (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$2),
            (SELECT COUNT(*) FROM provider_concurrency_leases
              WHERE external_effect_id=$2 AND released_at IS NOT NULL),
            (SELECT COUNT(*) FROM provider_concurrency_leases
              WHERE external_effect_id=$2 AND released_at IS NULL),
            (SELECT status FROM external_effect_lifecycle_transitions
              WHERE effect_id=$2 ORDER BY ordinal DESC LIMIT 1),
            (SELECT phase FROM run_step_execution_attempts
              WHERE workspace_id=$3 AND run_id=$4 AND step_id=$5)",
    )
    .bind(identities.material_intent_id.as_uuid())
    .bind(fixture.intent.id().as_uuid())
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observed,
        (
            0,
            0,
            0,
            0,
            1,
            "dispatching".to_owned(),
            "dispatching".to_owned(),
        )
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn post_network_completion_is_one_receipt_release_and_replay(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let repository = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None);
    let outcome = repository
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("dispatch commits before its post-network completion");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("dispatch was not prepared");
    };
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        authority.effect_id,
        "loopback-provider",
        Utc::now(),
        vec!["loopback-eof-after-dispatch".to_owned()],
    )
    .unwrap();
    let completion = ProviderPostNetworkCompletion {
        authority: (*authority).clone(),
        receipt: receipt.clone(),
        throttle: None,
    };
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    repository
        .complete_post_network(&context, completion.clone())
        .await
        .expect("receipt, release, and completion commit together");
    repository
        .complete_post_network(&context, completion)
        .await
        .expect("the exact post-network completion replay is idempotent");

    let observed: (i64, i64, Option<Uuid>, String) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$1),
            (SELECT COUNT(*) FROM provider_concurrency_leases
              WHERE external_effect_id=$1 AND released_receipt_id=$2),
            (SELECT released_receipt_id FROM provider_concurrency_leases
              WHERE external_effect_id=$1),
            (SELECT status FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 ORDER BY ordinal DESC LIMIT 1)",
    )
    .bind(fixture.intent.id().as_uuid())
    .bind(receipt.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observed,
        (1, 1, Some(receipt.id().as_uuid()), "unknown".into())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_reservation_converges_exact_replay_and_refuses_cross_step_fixed_identity_alias(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let (
        attempt_id,
        input_material_intent_id,
        input_content_material_id,
        input_material_key_id,
        input_intent_nonce,
        input_prepared_attachment_id,
        external_effect_id,
        model_request_evidence_id,
    ): (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT id, input_material_intent_id, input_content_material_id,
                input_material_key_id, input_intent_nonce,
                input_prepared_attachment_id, external_effect_id,
                model_request_evidence_id
           FROM run_step_execution_attempts
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();

    let fixed_identities = [
        input_material_intent_id,
        input_content_material_id,
        input_material_key_id,
        input_intent_nonce,
        input_prepared_attachment_id,
        external_effect_id,
        model_request_evidence_id,
    ];
    let unique_identities: Vec<(String, Vec<String>)> = sqlx::query_as(
        "SELECT constraint_row.conname, constraint_row.columns
           FROM (
                SELECT constraint_entry.conname,
                       array_agg(attribute.attname ORDER BY key_column.ordinality)
                           AS columns
                  FROM pg_constraint AS constraint_entry
                  JOIN unnest(constraint_entry.conkey)
                         WITH ORDINALITY AS key_column(attnum, ordinality)
                    ON TRUE
                  JOIN pg_attribute AS attribute
                    ON attribute.attrelid = constraint_entry.conrelid
                   AND attribute.attnum = key_column.attnum
                 WHERE constraint_entry.conrelid =
                         'run_step_execution_attempts'::REGCLASS
                   AND constraint_entry.contype = 'u'
                   AND constraint_entry.conname IN (
                       'run_step_execution_attempts_unique_input_material_intent',
                       'run_step_execution_attempts_unique_input_content_material',
                       'run_step_execution_attempts_unique_input_material_key',
                       'run_step_execution_attempts_unique_input_intent_nonce',
                       'run_step_execution_attempts_unique_input_prepared_attachment',
                       'run_step_execution_attempts_unique_external_effect',
                       'run_step_execution_attempts_unique_model_request_evidence'
                   )
                 GROUP BY constraint_entry.conname
           ) AS constraint_row
          ORDER BY constraint_row.conname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        unique_identities,
        vec![
            (
                "run_step_execution_attempts_unique_external_effect".to_owned(),
                vec!["external_effect_id".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_input_content_material".to_owned(),
                vec!["input_content_material_id".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_input_intent_nonce".to_owned(),
                vec!["input_intent_nonce".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_input_material_intent".to_owned(),
                vec!["input_material_intent_id".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_input_material_key".to_owned(),
                vec!["input_material_key_id".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_input_prepared_attachment".to_owned(),
                vec!["input_prepared_attachment_id".to_owned()],
            ),
            (
                "run_step_execution_attempts_unique_model_request_evidence".to_owned(),
                vec!["model_request_evidence_id".to_owned()],
            ),
        ]
    );

    let mut replay = runtime.begin().await.unwrap();
    set_context(&mut replay, fixture.ids).await;
    let converged = reserve_run_step_execution_attempt_with_identity(
        &mut replay,
        fixture.ids,
        fixture.ids.step,
        attempt_id,
        fixed_identities,
    )
    .await
    .expect("the exact immutable reservation replay must converge");
    assert_eq!(converged, attempt_id);
    replay.commit().await.unwrap();

    let other_fixture = setup(&pool, &runtime).await;
    let sibling_step = RunStepId::new();
    sqlx::query(
        "INSERT INTO run_steps(id,run_id,workspace_id,step_number,title,status)
         VALUES($1,$2,$3,2,'cross-step-alias','pending')",
    )
    .bind(sibling_step.as_uuid())
    .bind(other_fixture.ids.run.as_uuid())
    .bind(other_fixture.ids.workspace.as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    let mut alias = runtime.begin().await.unwrap();
    set_context(&mut alias, other_fixture.ids).await;
    let refusal = reserve_run_step_execution_attempt_with_identity(
        &mut alias,
        other_fixture.ids,
        sibling_step,
        Uuid::now_v7(),
        fixed_identities,
    )
    .await
    .expect_err("a distinct workspace Run step must not alias a global fixed opaque identity");
    assert!(matches!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23505") | Some("23514")
    ));
    alias.rollback().await.unwrap();
    let second_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM run_step_execution_attempts
          WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(other_fixture.ids.workspace.as_uuid())
    .bind(other_fixture.ids.run.as_uuid())
    .bind(sibling_step.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(second_attempts, 0);

    let distinct_attempt_id = Uuid::now_v7();
    let mut distinct = runtime.begin().await.unwrap();
    set_context(&mut distinct, other_fixture.ids).await;
    let admitted = reserve_run_step_execution_attempt_with_identity(
        &mut distinct,
        other_fixture.ids,
        sibling_step,
        distinct_attempt_id,
        [
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
        ],
    )
    .await
    .expect("a distinct Run-step opaque identity must remain reservable");
    assert_eq!(admitted, distinct_attempt_id);
    distinct.commit().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_phases_are_evidence_bound_and_atomic_with_dispatch(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let attempt_id = run_step_execution_attempt_id(&pool, &fixture).await;

    let mut premature = runtime.begin().await.unwrap();
    set_context(&mut premature, fixture.ids).await;
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'admitted')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *premature)
    .await
    .expect_err("a named phase cannot advance without the canonical admitted dispatch tuple");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    premature.rollback().await.unwrap();

    let failed = match repository(
        &runtime,
        &fixture,
        Arc::new(AtomicUsize::new(0)),
        Some(ProviderDispatchFaultPoint::AfterDispatching),
    )
    .prepare_dispatch(dispatch_request(&fixture))
    .await
    {
        Err(error) => error,
        Ok(_) => {
            panic!("a failed transaction after dispatching unexpectedly prepared an adapter call")
        }
    };
    assert!(matches!(failed, ApplicationError::Unavailable(_)));
    let reserved_phase: String =
        sqlx::query_scalar("SELECT phase FROM run_step_execution_attempts WHERE id=$1")
            .bind(attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reserved_phase, "reserved");

    let first = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None)
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("the original Run dispatch must bind and atomically advance its attempt");
    assert!(matches!(first, ProviderDispatchOutcome::Prepared { .. }));
    let after_first: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT phase FROM run_step_execution_attempts WHERE id=$2),
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$1),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching')",
    )
    .bind(fixture.intent.id().as_uuid())
    .bind(attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_first, ("dispatching".to_owned(), 1, 1, 1));

    let mut premature_result = runtime.begin().await.unwrap();
    set_context(&mut premature_result, fixture.ids).await;
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'result_prepared')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *premature_result)
    .await
    .expect_err(
        "a caller-named result-prepared phase must be refused without the exact preparation, receipt, and release evidence",
    );
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    assert_eq!(
        refusal
            .as_database_error()
            .map(|database| database.message()),
        Some("run step execution attempt result preparation is not exact")
    );
    premature_result.rollback().await.unwrap();

    let phase_after_named_result_refusal: String =
        sqlx::query_scalar("SELECT phase FROM run_step_execution_attempts WHERE id=$1")
            .bind(attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(phase_after_named_result_refusal, "dispatching");

    let mut phase_replay = runtime.begin().await.unwrap();
    set_context(&mut phase_replay, fixture.ids).await;
    let replayed_attempt: Uuid = sqlx::query_scalar(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'dispatching')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .fetch_one(&mut *phase_replay)
    .await
    .expect("the exact canonical dispatch phase replay must converge");
    assert_eq!(replayed_attempt, attempt_id);
    phase_replay.commit().await.unwrap();

    let replay = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None)
        .prepare_dispatch(dispatch_request(&fixture))
        .await;
    match replay {
        Ok(ProviderDispatchOutcome::Conflict { .. }) => {}
        Ok(ProviderDispatchOutcome::Prepared { .. }) => {
            panic!("a live Run dispatch replay unexpectedly prepared a second adapter call")
        }
        Ok(ProviderDispatchOutcome::Denied { .. }) => {
            panic!("a live Run dispatch replay unexpectedly changed authorization outcome")
        }
        Err(ApplicationError::Conflict(code)) if code == "PROVIDER_ADMISSION_CONFLICT" => {}
        Err(error) => panic!("live Run dispatch replay returned {error}"),
    }
    let after_replay: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT phase FROM run_step_execution_attempts WHERE id=$2),
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$1),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching')",
    )
    .bind(fixture.intent.id().as_uuid())
    .bind(attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_replay, ("dispatching".to_owned(), 1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_recovery_classifies_a_fresh_reserved_attempt(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    let recovery = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None)
        .recover_run_step_attempt(&context, fixture.ids.run, fixture.ids.step, Utc::now())
        .await
        .expect("a fresh reserved attempt must not read an unassigned authority record");

    assert_eq!(recovery, RunStepAttemptRecovery::ResumeReserved);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_recovery_classifies_a_lawful_admitted_attempt_before_dispatch(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    admit_run_step_attempt_without_dispatch(&runtime, &fixture).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    let observed: (String, i64) = sqlx::query_as(
        "SELECT
            (SELECT phase FROM run_step_execution_attempts
              WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE workspace_id=$1 AND effect_id=$4 AND status='dispatching')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(fixture.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observed, ("admitted".to_owned(), 0));

    let recovery = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None)
        .recover_run_step_attempt(&context, fixture.ids.run, fixture.ids.step, Utc::now())
        .await
        .expect("the existing guarded admission must be resumable before any network dispatch");

    assert_eq!(recovery, RunStepAttemptRecovery::ResumeAdmitted);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_recovery_adopts_only_the_expired_original_dispatch_as_unknown_without_re_admission(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &runtime).await;
    let repository = repository(&runtime, &fixture, Arc::new(AtomicUsize::new(0)), None);
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);

    let attempt_id = run_step_execution_attempt_id(&pool, &fixture).await;

    let outcome = repository
        .prepare_dispatch(dispatch_request(&fixture))
        .await
        .expect("the real Run dispatch must create the original live admission before recovery");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("Run dispatch did not return its original authority");
    };

    let before: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$1),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching'),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='unknown')",
    )
    .bind(fixture.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, (1, 1, 1, 0));

    let before_deadline = repository
        .recover_run_step_attempt(
            &context,
            fixture.ids.run,
            fixture.ids.step,
            authority.dispatch_expires_at - chrono::Duration::seconds(1),
        )
        .await;
    assert_eq!(
        before_deadline.unwrap(),
        RunStepAttemptRecovery::AwaitDispatchDeadline
    );
    let still_dispatching: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching'),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='unknown')",
    )
    .bind(fixture.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(still_dispatching, (1, 0));

    let recovery = repository
        .recover_run_step_attempt(
            &context,
            fixture.ids.run,
            fixture.ids.step,
            authority.dispatch_expires_at + chrono::Duration::seconds(1),
        )
        .await;
    assert_eq!(recovery.unwrap(), RunStepAttemptRecovery::AdoptedUnknown);

    let after_recovery: (i64, i64, i64, i64, String, String) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$1),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching'),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='unknown'),
            (SELECT phase FROM run_step_execution_attempts WHERE id=$2),
            (SELECT status FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 ORDER BY ordinal DESC LIMIT 1)",
    )
    .bind(fixture.intent.id().as_uuid())
    .bind(attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after_recovery,
        (1, 1, 1, 1, "unknown".into(), "unknown".into())
    );

    let replay = repository
        .recover_run_step_attempt(
            &context,
            fixture.ids.run,
            fixture.ids.step,
            authority.dispatch_expires_at + chrono::Duration::seconds(2),
        )
        .await;
    assert_eq!(replay.unwrap(), RunStepAttemptRecovery::AlreadyUnknown);
    let after_replay: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM external_effect_intents WHERE id=$1),
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='dispatching'),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$1 AND status='unknown')",
    )
    .bind(fixture.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_replay, (1, 1, 1, 1));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn lost_q1_dispatch_becomes_inconclusive_without_another_adapter_attempt(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch_for_ordinal(&pool, &runtime, false, "10").await;
    let repository = qualification_repository(&runtime, None);
    let mut request = qualification_request(&qualification);
    request.cause = ProviderDispatchCause::QualificationProbe {
        qualification_job_id: qualification.job,
        qualification_target_id: qualification.target,
        probe_ordinal: "10".to_owned(),
    };
    let outcome = repository
        .prepare_dispatch(request)
        .await
        .expect("the one q1 network effect commits before the adapter runs");
    let ProviderDispatchOutcome::Prepared { authority, .. } = outcome else {
        panic!("q1 dispatch was not prepared");
    };
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let recovered = repository
        .recover_lost_post_network(&context, &authority, Utc::now())
        .await
        .expect("lost q1 dispatch is recovered through its original cause");
    assert_eq!(
        recovered,
        ProviderLostDispatchRecovery::QualificationInconclusive
    );
    let replay = repository
        .recover_lost_post_network(&context, &authority, Utc::now())
        .await
        .expect("lost q1 recovery is idempotent without a second adapter attempt");
    assert_eq!(
        replay,
        ProviderLostDispatchRecovery::QualificationInconclusive
    );

    let observed: (String, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT state FROM qualification_jobs WHERE id=$1),
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE qualification_job_id=$1),
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
              WHERE effect_id=$2 AND status='dispatching')",
    )
    .bind(qualification.job.as_uuid())
    .bind(qualification.fixture.intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observed, ("inconclusive_unknown".into(), 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_probe_refuses_wrong_job_target_ordinal_auth_or_current_credential(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;

    let credential_target = setup_qualification_branch(&pool, &runtime, true).await;
    let credential_target_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let missing_credential =
        qualification_repository(&runtime, Some(Arc::clone(&credential_target_vault)))
            .prepare_dispatch(qualification_request(&credential_target))
            .await;
    match missing_credential {
        Err(ApplicationError::Policy(_)) => {}
        Err(error) => panic!("credential omission returned {error}"),
        Ok(outcome) => {
            let adapter_calls = AtomicUsize::new(0);
            let observed = drive_prepared_adapter_only(outcome, &adapter_calls);
            panic!(
                "credential omission unexpectedly reached the adapter as {observed:?} (calls={})",
                adapter_calls.load(Ordering::SeqCst)
            );
        }
    }
    assert_eq!(credential_target_vault.calls(), 0);
    assert_eq!(
        atomic_counts(&pool, &credential_target.fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );

    let no_auth_target = setup_qualification_branch(&pool, &runtime, false).await;
    let no_auth_target_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let mut unexpected_credential = qualification_request(&no_auth_target);
    unexpected_credential.credential = Some(vestrace_application::ProviderDispatchCredential {
        lease_id: Uuid::now_v7(),
        credential_slot_id: CredentialSlotId::new(),
        credential_revision_id: Uuid::now_v7(),
        credential_activation_guard_id: Uuid::now_v7(),
        destination_authority: "api.example.test".into(),
        auth_mode: "bearer".into(),
    });
    let unexpected_credential =
        qualification_repository(&runtime, Some(Arc::clone(&no_auth_target_vault)))
            .prepare_dispatch(unexpected_credential)
            .await;
    match unexpected_credential {
        Err(ApplicationError::Policy(_)) => {}
        Err(error) => panic!("unexpected no-auth credential returned {error}"),
        Ok(outcome) => {
            let adapter_calls = AtomicUsize::new(0);
            let observed = drive_prepared_adapter_only(outcome, &adapter_calls);
            panic!(
                "unexpected no-auth credential reached the adapter as {observed:?} (calls={})",
                adapter_calls.load(Ordering::SeqCst)
            );
        }
    }
    assert_eq!(no_auth_target_vault.calls(), 0);
    assert_eq!(
        atomic_counts(&pool, &no_auth_target.fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );

    for mismatch in ["job", "target", "ordinal"] {
        let fixture = setup_qualification_branch(&pool, &runtime, false).await;
        let mut request = qualification_request(&fixture);
        let ProviderDispatchCause::QualificationProbe {
            qualification_job_id,
            qualification_target_id,
            probe_ordinal,
        } = &mut request.cause
        else {
            unreachable!()
        };
        match mismatch {
            "job" => *qualification_job_id = QualificationJobId::new(),
            "target" => *qualification_target_id = Uuid::now_v7(),
            "ordinal" => *probe_ordinal = "01".to_owned(),
            _ => unreachable!(),
        }
        let result = qualification_repository(&runtime, None)
            .prepare_dispatch(request)
            .await;
        assert!(
            matches!(result, Err(ApplicationError::Policy(_))),
            "qualification {mismatch} mismatch was not a policy refusal"
        );
        assert_eq!(
            atomic_counts(&pool, &fixture.fixture).await,
            (0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
            "qualification {mismatch} mismatch wrote dispatch state"
        );
    }

    let wrong_auth = setup_qualification_branch(&pool, &runtime, true).await;
    let auth_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let mut auth_request = qualification_request(&wrong_auth);
    let mut credential = credential_request(&wrong_auth.fixture);
    credential.auth_mode = "basic".to_owned();
    auth_request.credential = Some(credential);
    let auth_result = qualification_repository(&runtime, Some(auth_vault.clone()))
        .prepare_dispatch(auth_request)
        .await;
    assert!(matches!(auth_result, Err(ApplicationError::Policy(_))));
    assert_eq!(auth_vault.calls(), 0);
    assert_eq!(
        atomic_counts(&pool, &wrong_auth.fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );

    let stale = setup_qualification_branch(&pool, &runtime, true).await;
    let stale_vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let mut stale_tx = pool.begin().await.unwrap();
    set_context(&mut stale_tx, stale.fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *stale_tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE credential_slots SET current_revision_version=2
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(stale.fixture.ids.workspace.as_uuid())
    .bind(stale.fixture.credential.unwrap().slot.as_uuid())
    .execute(&mut *stale_tx)
    .await
    .unwrap();
    stale_tx.commit().await.unwrap();
    let mut stale_request = qualification_request(&stale);
    stale_request.credential = Some(credential_request(&stale.fixture));
    let stale_result = qualification_repository(&runtime, Some(stale_vault.clone()))
        .prepare_dispatch(stale_request)
        .await;
    assert!(matches!(stale_result, Err(ApplicationError::Policy(_))));
    assert_eq!(stale_vault.calls(), 0);
    assert_eq!(
        atomic_counts(&pool, &stale.fixture).await,
        (0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn three_session_dispatch_and_standalone_reconstruction_follow_root_first_order(
    pool: PgPool,
) {
    let setup_runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &setup_runtime).await;
    setup_runtime.close().await;

    let mut blocker = pool.begin().await.unwrap();
    set_context(&mut blocker, fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "SELECT id FROM model_binding_snapshots
          WHERE workspace_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.snapshot)
    .fetch_one(&mut *blocker)
    .await
    .unwrap();

    let dispatch_pool = runtime_pool_single(&pool).await;
    let reconstruction_pool = runtime_pool_single(&pool).await;
    let dispatch_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&dispatch_pool)
        .await
        .unwrap();
    let reconstruction_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&reconstruction_pool)
        .await
        .unwrap();
    let vault = Arc::new(EvidenceVault {
        unwraps: AtomicUsize::new(0),
    });

    let dispatch_repository = real_evidence_repository(&dispatch_pool, &fixture, vault.clone());
    let dispatch_request = dispatch_request(&fixture);
    let dispatch =
        tokio::spawn(async move { dispatch_repository.prepare_dispatch(dispatch_request).await });
    wait_for_blocker(&pool, dispatch_pid, blocker_pid).await;

    let evidence_repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let reconstruction_permit =
        PgInstallationMutationPermit::new(PgStore::from_pool(reconstruction_pool.clone()));
    let reconstruction_context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);
    let workspace = fixture.ids.workspace;
    let evidence = fixture.ids.evidence;
    let reconstruction = tokio::spawn(async move {
        let mut permit = reconstruction_permit
            .acquire(PermitMode::Shared, &reconstruction_context)
            .await?;
        let result = evidence_repository
            .reconstruct_current_in(permit.unit_of_work_mut(), workspace, evidence)
            .await;
        permit.rollback().await?;
        result
    });
    wait_for_blocker(&pool, reconstruction_pid, dispatch_pid).await;

    blocker.commit().await.unwrap();
    let (dispatch, reconstruction) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(dispatch, reconstruction)
    })
    .await
    .expect("canonical three-session race exceeded its bounded timeout");
    let dispatch = dispatch
        .expect("dispatch task panicked")
        .expect("canonical dispatch returned an error");
    let reconstruction = reconstruction
        .expect("reconstruction task panicked")
        .expect("canonical standalone reconstruction returned an error");
    assert!(matches!(dispatch, ProviderDispatchOutcome::Prepared { .. }));
    assert!(matches!(
        reconstruction,
        ModelRequestReconstruction::Complete(_)
    ));
    assert_eq!(
        atomic_counts(&pool, &fixture).await,
        (1, 1, 1, 1, 1, 0, 1, 1, 1, 1)
    );
    assert_eq!(vault.unwraps(), 2);
    dispatch_pool.close().await;
    reconstruction_pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn old_snapshot_before_root_mutation_reproduces_40p01_then_restores_green(pool: PgPool) {
    let setup_runtime = runtime_pool(&pool).await;
    let fixture = setup(&pool, &setup_runtime).await;
    setup_runtime.close().await;

    let signature = "vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)";
    let canonical: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::REGPROCEDURE)")
        .bind(signature)
        .fetch_one(&pool)
        .await
        .unwrap();
    let root_lock = "    PERFORM vestrace_lock_model_request_evidence_for_reconstruction(\n        target_workspace_id, target_evidence_id\n    );";
    let old_first_lock = "    IF target_cause_kind = 'run_step' THEN\n        PERFORM snapshot.id\n          FROM model_binding_snapshots AS snapshot\n         WHERE snapshot.workspace_id = target_workspace_id\n           AND snapshot.id = target_snapshot_id\n         FOR UPDATE OF snapshot;\n    END IF;\n\n";
    let mutated = canonical.replacen(root_lock, &format!("{old_first_lock}{root_lock}"), 1);
    assert_ne!(mutated, canonical, "routing mutation marker drifted");
    sqlx::query("GRANT CREATE ON SCHEMA public TO vestrace_guarded_owner")
        .execute(&pool)
        .await
        .unwrap();
    let mut mutation = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mutation)
        .await
        .unwrap();
    sqlx::query(&mutated).execute(&mut *mutation).await.unwrap();
    mutation.commit().await.unwrap();

    let mut blocker = pool.begin().await.unwrap();
    set_context(&mut blocker, fixture.ids).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "SELECT id FROM model_binding_snapshots
          WHERE workspace_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.snapshot)
    .fetch_one(&mut *blocker)
    .await
    .unwrap();

    let dispatch_pool = runtime_pool_single(&pool).await;
    let reconstruction_pool = runtime_pool_single(&pool).await;
    let dispatch_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&dispatch_pool)
        .await
        .unwrap();
    let reconstruction_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&reconstruction_pool)
        .await
        .unwrap();

    let dispatch_ids = fixture.ids;
    let dispatch_effect = fixture.intent.id().as_uuid();
    let dispatch = tokio::spawn(async move {
        let mut tx = dispatch_pool.begin().await.unwrap();
        set_context(&mut tx, dispatch_ids).await;
        let result = sqlx::query(
            "SELECT * FROM vestrace_lock_provider_dispatch_routing(
                $1,$2,$3,$4,$5,'run_step',$6,$7,$8,NULL,NULL)",
        )
        .bind(dispatch_ids.workspace.as_uuid())
        .bind(dispatch_ids.connection.as_uuid())
        .bind(dispatch_ids.revision.as_uuid())
        .bind(dispatch_effect)
        .bind(dispatch_ids.evidence.as_uuid())
        .bind(dispatch_ids.run.as_uuid())
        .bind(dispatch_ids.step.as_uuid())
        .bind(dispatch_ids.snapshot)
        .execute(&mut *tx)
        .await
        .map(|_| ());
        let _ = tx.rollback().await;
        result
    });
    wait_for_blocker(&pool, dispatch_pid, blocker_pid).await;

    let reconstruction_ids = fixture.ids;
    let reconstruction = tokio::spawn(async move {
        let mut tx = reconstruction_pool.begin().await.unwrap();
        set_context(&mut tx, reconstruction_ids).await;
        let result =
            sqlx::query("SELECT vestrace_lock_model_request_evidence_for_reconstruction($1,$2)")
                .bind(reconstruction_ids.workspace.as_uuid())
                .bind(reconstruction_ids.evidence.as_uuid())
                .execute(&mut *tx)
                .await
                .map(|_| ());
        let _ = tx.rollback().await;
        result
    });
    wait_for_blocker(&pool, reconstruction_pid, dispatch_pid).await;
    blocker.commit().await.unwrap();

    let (dispatch_result, reconstruction_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(dispatch, reconstruction)
        })
        .await
        .expect("old-order mutation did not resolve through deadlock detection");
    let dispatch_result = dispatch_result.expect("old-order dispatch task panicked");
    let reconstruction_result =
        reconstruction_result.expect("old-order reconstruction task panicked");
    let observed_codes: Vec<String> = [dispatch_result.as_ref(), reconstruction_result.as_ref()]
        .into_iter()
        .filter_map(|result| result.err())
        .filter_map(|error| error.as_database_error())
        .filter_map(|error| error.code())
        .map(|code| code.into_owned())
        .collect();

    let mut restoration = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *restoration)
        .await
        .unwrap();
    sqlx::query(&canonical)
        .execute(&mut *restoration)
        .await
        .unwrap();
    restoration.commit().await.unwrap();
    sqlx::query("REVOKE CREATE ON SCHEMA public FROM vestrace_guarded_owner")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        observed_codes.iter().any(|code| code == "40P01"),
        "old snapshot-before-root order did not produce exact 40P01: {observed_codes:?}"
    );

    let restored_pool = runtime_pool_single(&pool).await;
    let mut restored = restored_pool.begin().await.unwrap();
    set_context(&mut restored, fixture.ids).await;
    sqlx::query(
        "SELECT * FROM vestrace_lock_provider_dispatch_routing(
            $1,$2,$3,$4,$5,'run_step',$6,$7,$8,NULL,NULL)",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.connection.as_uuid())
    .bind(fixture.ids.revision.as_uuid())
    .bind(fixture.intent.id().as_uuid())
    .bind(fixture.ids.evidence.as_uuid())
    .bind(fixture.ids.run.as_uuid())
    .bind(fixture.ids.step.as_uuid())
    .bind(fixture.ids.snapshot)
    .execute(&mut *restored)
    .await
    .expect("restored root-first routing must be live");
    restored.rollback().await.unwrap();
    restored_pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn independent_real_reconstruction_dispatches_serialize_on_one_connection(pool: PgPool) {
    let setup_runtime = runtime_pool(&pool).await;
    let first_fixture = setup(&pool, &setup_runtime).await;
    let second_fixture = setup_sibling(&pool, &setup_runtime, &first_fixture).await;
    setup_runtime.close().await;

    let first_pool = runtime_pool(&pool).await;
    let second_pool = runtime_pool(&pool).await;
    let vault = Arc::new(EvidenceVault {
        unwraps: AtomicUsize::new(0),
    });
    let first_repository = real_evidence_repository(&first_pool, &first_fixture, vault.clone());
    let second_repository = real_evidence_repository(&second_pool, &second_fixture, vault.clone());
    let raced = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(
            first_repository.prepare_dispatch(dispatch_request(&first_fixture)),
            second_repository.prepare_dispatch(dispatch_request(&second_fixture)),
        )
    })
    .await
    .expect("full provider dispatch race exceeded its bounded timeout");
    let (first, second) = (
        raced
            .0
            .expect("first dispatch returned an unexpected database error"),
        raced
            .1
            .expect("second dispatch returned an unexpected database error"),
    );
    let outcomes = [first, second];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ProviderDispatchOutcome::Prepared { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ProviderDispatchOutcome::Conflict { .. }))
            .count(),
        1
    );
    for outcome in outcomes {
        if let ProviderDispatchOutcome::Prepared { authority, .. } = outcome {
            assert!(
                authority.effect_id == first_fixture.intent.id()
                    || authority.effect_id == second_fixture.intent.id()
            );
            assert_eq!(authority.connection_id, first_fixture.ids.connection);
            assert_eq!(authority.connection_revision_id, first_fixture.ids.revision);
        }
    }
    let active_leases: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM provider_concurrency_leases
          WHERE workspace_id=$1 AND connection_id=$2
            AND released_at IS NULL AND expires_at>NOW()",
    )
    .bind(first_fixture.ids.workspace.as_uuid())
    .bind(first_fixture.ids.connection.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_leases, 1, "race over-admitted one stable connection");
    assert_eq!(
        vault.unwraps(),
        2,
        "both real MRE reconstructions must complete"
    );
    first_pool.close().await;
    second_pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_runner_composes_the_original_q1_dispatch_once(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = setup_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(fixture.ids.workspace, fixture.ids.principal);
    let no_auth_binding_revision_id: Uuid = sqlx::query_scalar(
        "SELECT no_auth_binding_revision_id FROM model_binding_snapshots WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(fixture.ids.snapshot)
    .fetch_one(&pool)
    .await
    .unwrap();
    let job_id = QualificationJobId::new();
    let service = QualificationJobService::new(PgQualificationJobRepository::new(
        PgStore::from_pool(runtime.clone()),
    ));
    service
        .request(
            &context,
            QualificationJobRequest {
                job_id,
                target_binding_id: Uuid::now_v7(),
                connection_id: fixture.ids.connection,
                connection_revision_id: fixture.ids.revision,
                target: vestrace_domain::QualificationTargetBinding::NoAuth {
                    binding_revision_id: NoAuthBindingRevisionId::from_uuid(
                        no_auth_binding_revision_id,
                    ),
                },
                chat_model_revision_id: vestrace_domain::ModelRevisionId::from_uuid(
                    fixture.chat_model_revision_id,
                ),
                embedding_model_revision_id: vestrace_domain::ModelRevisionId::from_uuid(
                    fixture.embedding_model_revision_id,
                ),
            },
        )
        .await
        .unwrap();
    let adapter = Arc::new(CountingQ1Adapter::default());
    let runner = PgQualificationProbeRunner::with_adapter(
        PgStore::from_pool(runtime.clone()),
        Arc::new(qualification_repository(&runtime, None)),
        WorkerId::new(),
        adapter.clone(),
    );

    let static_state = service
        .run_next_probe(&context, job_id, "00", &runner)
        .await
        .unwrap();
    assert_eq!(
        static_state,
        vestrace_domain::QualificationJobState::Running
    );
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
    let static_artifacts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM provider_dispatch_causes
          WHERE workspace_id=$1 AND qualification_job_id=$2",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(static_artifacts, 0, "static 00 may not create q1 artifacts");

    let (first_network, second_network) = tokio::join!(
        service.run_next_probe(&context, job_id, "10", &runner),
        service.run_next_probe(&context, job_id, "10", &runner),
    );
    assert_eq!(
        [first_network.as_ref(), second_network.as_ref()]
            .into_iter()
            .filter(|result| result.is_ok())
            .count(),
        1,
        "the scoped job/ordinal reservation must admit exactly one runner"
    );
    assert!(
        [first_network, second_network]
            .into_iter()
            .any(|result| matches!(result, Ok(vestrace_domain::QualificationJobState::Running)))
    );
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
    let (causes, complete_mre, receipts): (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM provider_dispatch_causes
              WHERE workspace_id=$1 AND qualification_job_id=$2 AND qualification_probe_ordinal='10'),
            (SELECT COUNT(*) FROM model_request_evidence_checks AS checked
              JOIN model_request_evidence_roots AS root
                ON root.workspace_id=checked.workspace_id AND root.id=checked.evidence_root_id
              WHERE checked.workspace_id=$1 AND root.cause_kind='qualification_probe'
                AND root.cause_id=$2 AND checked.status='complete'),
            (SELECT COUNT(*) FROM external_effect_receipts AS receipt
              JOIN provider_dispatch_causes AS cause
                ON cause.workspace_id=receipt.workspace_id AND cause.external_effect_id=receipt.effect_id
              WHERE receipt.workspace_id=$1 AND cause.qualification_job_id=$2
                AND cause.qualification_probe_ordinal='10')",
    )
    .bind(fixture.ids.workspace.as_uuid())
    .bind(job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((causes, complete_mre, receipts), (1, 1, 1));

    let replay = service
        .run_next_probe(&context, job_id, "10", &runner)
        .await;
    assert!(
        replay.is_err(),
        "a completed original ordinal cannot be re-run"
    );
    assert_eq!(
        adapter.calls.load(Ordering::SeqCst),
        1,
        "replay must not call the adapter again"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn optional_definite_status_from_the_real_runner_is_acknowledged_and_finalizable(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let qualification = setup_qualification_branch(&pool, &runtime, false).await;
    let context = RequestContext::new(
        qualification.fixture.ids.workspace,
        qualification.fixture.ids.principal,
    );
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()));
    let service = QualificationJobService::new(repository.clone());

    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35", "40", "50", "60", "70", "90"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(&pool, &qualification, ordinal).await,
        );
    }
    for ordinal in ["00", "10", "15", "20", "30", "35", "40", "50", "60", "70"] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        service
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result: QualificationProbeResult::Pass,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the valid predecessor matrix must be present before ordinal 80");
    }

    let adapter = Arc::new(OptionalUnsupportedQ1Adapter::default());
    let runner = PgQualificationProbeRunner::with_adapter(
        PgStore::from_pool(runtime.clone()),
        Arc::new(qualification_repository(&runtime, None)),
        WorkerId::new(),
        adapter.clone(),
    );
    let state = service
        .run_next_probe(&context, qualification.job, "80", &runner)
        .await
        .expect("a manifest-valid optional status must complete through the real runner");
    assert_eq!(state, vestrace_domain::QualificationJobState::Running);
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);

    let runner_fact: (String, String) = sqlx::query_as(
        "SELECT receipt.outcome_status, result.result
           FROM qualification_probe_results AS result
           JOIN provider_dispatch_causes AS cause
             ON cause.workspace_id=result.workspace_id
            AND cause.external_effect_id=result.external_effect_id
            AND cause.model_request_evidence_id=result.model_request_evidence_id
           JOIN external_effect_receipts AS receipt
             ON receipt.workspace_id=cause.workspace_id
            AND receipt.effect_id=cause.external_effect_id
          WHERE result.workspace_id=$1
            AND result.qualification_job_id=$2
            AND result.probe_ordinal='80'",
    )
    .bind(qualification.fixture.ids.workspace.as_uuid())
    .bind(qualification.job.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        runner_fact,
        ("acknowledged".to_owned(), "unsupported_definite".to_owned()),
        "a definite provider answer is acknowledged even when the optional q1 capability is unsupported"
    );

    let (external_effect_id, model_request_evidence_id) = network["90"];
    service
        .record_probe_result(
            &context,
            QualificationProbeCompletion {
                probe_result_id: Uuid::now_v7(),
                job_id: qualification.job,
                ordinal: "90".to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(external_effect_id),
                model_request_evidence_id: Some(model_request_evidence_id),
            },
        )
        .await
        .expect("the required final probe must complete after optional ordinal 80");
    service
        .finalize_success(
            &context,
            vestrace_application::QualificationFinalization {
                job_id: qualification.job,
                connection_qualification_revision_id:
                    vestrace_domain::ConnectionQualificationRevisionId::new(),
                chat_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
                embedding_model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::new(),
            },
        )
        .await
        .expect("the complete matrix with a real optional unsupported runner result must finalize");
    let final_state: String =
        sqlx::query_scalar("SELECT state FROM qualification_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(qualification.fixture.ids.workspace.as_uuid())
            .bind(qualification.job.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(final_state, "succeeded");
}
