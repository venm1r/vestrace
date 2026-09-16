#[path = "model_binding_snapshot.rs"]
mod binding_fixture;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::Barrier;
use uuid::Uuid;
use vestrace_application::run::{CommitRun, RunSnapshot, RunStorePort};
use vestrace_application::{
    ApplicationError, EffectiveModelRequest, GovernedMutation, GovernedMutationReceipt,
    GovernedMutationRepository, GovernedRunStepInputAuthority, GovernedRunStepInputReservation,
    IdempotencyRecord, OutboxMessage, PrepareGovernedRunStepInput, ProviderDispatchCause,
    ProviderDispatchGovernedApply, ProviderDispatchPolicyEvaluation,
    ProviderDispatchPolicyEvaluator, ProviderDispatchRepository, ProviderDispatchTarget,
    RequestContext, RunCommandExecutor, RunStepExecutionAttempt, TransactionManager, UnitOfWork,
    VaultError,
};
use vestrace_domain::{
    AgentRunId, AgentRuntimeSnapshotId, AuditEvent, ContentMaterialId, ExternalEffectIntent,
    IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId, ModelRequestEvidenceId,
    PreparedMaterialAttachmentId, RunStepId, ZeroizingDek,
    id::{AuditEventId, OutboxId},
    run::{RunActorRef, RunVersion},
};
use vestrace_infrastructure::crypto::ContentMaterialCodec;
use vestrace_infrastructure::{
    PgCredentialDispatchLeaseRepository, PgExternalEffectRepository, PgGovernedMutationRepository,
    PgInstallationMutationPermit, PgMaterialIntentRepository, PgModelDataPolicyDecisionRepository,
    PgModelRequestEvidenceRepository, PgProviderDispatchRepository, PgStore, PgTransactionManager,
    PostgresRunStore,
};

struct UnusedVault;

impl vestrace_application::MaterialKeyVault for UnusedVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, VaultError> {
        unreachable!("no vault may be called while the caller-owned transaction is open")
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        unreachable!("no vault may be called while the caller-owned transaction is open")
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        unreachable!("no vault may be called while the caller-owned transaction is open")
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<vestrace_domain::ErasureReceipt, VaultError> {
        unreachable!("no vault may be called while the caller-owned transaction is open")
    }
}

struct RecordingVault {
    create_count: AtomicUsize,
    unwrap_count: AtomicUsize,
}

impl RecordingVault {
    fn new() -> Self {
        Self {
            create_count: AtomicUsize::new(0),
            unwrap_count: AtomicUsize::new(0),
        }
    }
}

impl vestrace_application::MaterialKeyVault for RecordingVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, VaultError> {
        self.create_count.fetch_add(1, Ordering::SeqCst);
        Ok(vestrace_domain::VaultReceipt::from_uuid(Uuid::from_u128(
            41,
        )))
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        self.unwrap_count.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([23_u8; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        unreachable!("governed input preparation must not erase its live key")
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<vestrace_domain::ErasureReceipt, VaultError> {
        unreachable!("governed input preparation must not erase its live key")
    }
}

struct UnusedDispatchPolicy;

#[async_trait]
impl ProviderDispatchPolicyEvaluator for UnusedDispatchPolicy {
    async fn evaluate(
        &self,
        _context: &vestrace_application::RequestContext,
        _intent: &ExternalEffectIntent,
        _cause: &ProviderDispatchCause,
        _target: &ProviderDispatchTarget,
        _request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "provider dispatch is outside the governed input reservation fixture".to_owned(),
        ))
    }
}

fn governed_input_authority_with<S, G, V>(
    pool: &PgPool,
    runs: S,
    governed: G,
    vault: Arc<V>,
) -> GovernedRunStepInputReservation<
    PgTransactionManager,
    S,
    PgMaterialIntentRepository,
    PgProviderDispatchRepository,
    G,
    PgExternalEffectRepository,
    Arc<V>,
    ContentMaterialCodec,
    PgModelRequestEvidenceRepository,
>
where
    V: vestrace_application::MaterialKeyVault + 'static,
{
    let store = PgStore::from_pool(pool.clone());
    let vault_for_reservation = vault.clone();
    let evidence_vault = vault.clone();
    let dispatch = PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PgModelRequestEvidenceRepository::new(vault.clone())),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgCredentialDispatchLeaseRepository::new(
            store.clone(),
            vault,
        )),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store.clone())),
        Arc::new(UnusedDispatchPolicy),
    );
    GovernedRunStepInputReservation::new(
        PgTransactionManager::new(store.clone()),
        runs,
        PgMaterialIntentRepository::new(store.clone()),
        dispatch,
        governed,
        PgExternalEffectRepository::new(store.clone()),
        vault_for_reservation,
        ContentMaterialCodec::new(),
        PgModelRequestEvidenceRepository::new(evidence_vault),
    )
}

/// The same dispatch repository `governed_input_authority` is built from, so a
/// test reads through the exact production adapter rather than a second graph.
fn governed_dispatch_repository(pool: &PgPool) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(pool.clone());
    let vault = Arc::new(UnusedVault);
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PgModelRequestEvidenceRepository::new(vault.clone())),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgCredentialDispatchLeaseRepository::new(
            store.clone(),
            vault,
        )),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(UnusedDispatchPolicy),
    )
}

fn governed_input_authority(
    pool: &PgPool,
) -> GovernedRunStepInputReservation<
    PgTransactionManager,
    PostgresRunStore,
    PgMaterialIntentRepository,
    PgProviderDispatchRepository,
    PgGovernedMutationRepository,
    PgExternalEffectRepository,
    Arc<RecordingVault>,
    ContentMaterialCodec,
    PgModelRequestEvidenceRepository,
> {
    let store = PgStore::from_pool(pool.clone());
    governed_input_authority_with(
        pool,
        PostgresRunStore::new(&store),
        PgGovernedMutationRepository::new(store),
        Arc::new(RecordingVault::new()),
    )
}

fn governed_material_authority(
    pool: &PgPool,
    vault: Arc<RecordingVault>,
) -> GovernedRunStepInputReservation<
    PgTransactionManager,
    PostgresRunStore,
    PgMaterialIntentRepository,
    PgProviderDispatchRepository,
    PgGovernedMutationRepository,
    PgExternalEffectRepository,
    Arc<RecordingVault>,
    ContentMaterialCodec,
    PgModelRequestEvidenceRepository,
> {
    let store = PgStore::from_pool(pool.clone());
    let dispatch = PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(PgModelRequestEvidenceRepository::new(vault.clone())),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgCredentialDispatchLeaseRepository::new(
            store.clone(),
            vault.clone(),
        )),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store.clone())),
        Arc::new(UnusedDispatchPolicy),
    );
    GovernedRunStepInputReservation::new(
        PgTransactionManager::new(store.clone()),
        PostgresRunStore::new(&store),
        PgMaterialIntentRepository::new(store.clone()),
        dispatch,
        PgGovernedMutationRepository::new(store.clone()),
        PgExternalEffectRepository::new(store.clone()),
        vault.clone(),
        ContentMaterialCodec::new(),
        PgModelRequestEvidenceRepository::new(vault),
    )
}

#[derive(Clone)]
struct FailAfterRunCommit {
    inner: PostgresRunStore,
}

#[async_trait]
impl RunStorePort for FailAfterRunCommit {
    async fn load(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        self.inner.load(context, run_id).await
    }

    async fn create(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.inner.create(context, commit).await
    }

    async fn commit(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.inner.commit(context, commit).await
    }

    async fn commit_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.inner.commit_in(context, unit_of_work, commit).await?;
        Err(ApplicationError::Unavailable(
            "injected fault after governed Run-step commit".to_owned(),
        ))
    }
}

struct FailAfterInputReservations;

#[async_trait]
impl GovernedMutationRepository<ProviderDispatchGovernedApply> for FailAfterInputReservations {
    async fn commit(
        &self,
        _mutation: GovernedMutation<ProviderDispatchGovernedApply>,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "injected fault after governed Run-step input reservations".to_owned(),
        ))
    }

    async fn commit_in(
        &self,
        _unit_of_work: &mut dyn UnitOfWork,
        _mutation: GovernedMutation<ProviderDispatchGovernedApply>,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "injected fault after governed Run-step input reservations".to_owned(),
        ))
    }
}

struct GovernedInputFixture {
    context: vestrace_application::RequestContext,
    run_id: AgentRunId,
    expected_run_version: RunVersion,
    step_id: RunStepId,
    assigned_actor: RunActorRef,
    intent: ExternalEffectIntent,
    connection_revision_id: vestrace_domain::ConnectionRevisionId,
    connection_qualification_revision_id: Uuid,
    model_revision_id: vestrace_domain::ModelRevisionId,
    model_qualification_revision_id: Uuid,
    attempt: RunStepExecutionAttempt,
    audit: AuditEvent,
    idempotency: IdempotencyRecord,
    outbox: OutboxMessage,
}

impl GovernedInputFixture {
    fn command(&self) -> PrepareGovernedRunStepInput {
        PrepareGovernedRunStepInput {
            run_id: self.run_id,
            expected_run_version: self.expected_run_version,
            step_id: self.step_id,
            assigned_actor: self.assigned_actor.clone(),
            plan_step_reference: Some("governed-input-step".to_owned()),
            input: vestrace_application::run::ConfidentialRunInput::parse(
                "atomic governed input sentinel".to_owned(),
            )
            .unwrap(),
            intent: self.intent.clone(),
            connection_revision_id: self.connection_revision_id,
            connection_qualification_revision_id: self.connection_qualification_revision_id,
            model_revision_id: self.model_revision_id,
            model_qualification_revision_id: self.model_qualification_revision_id,
            sampling: vestrace_application::EffectiveSampling::new(0.2, 0.9).unwrap(),
            limits: vestrace_application::EffectiveRequestLimits::new(256, 4, 32_768).unwrap(),
            identities: self.attempt.clone(),
            audit: self.audit.clone(),
            idempotency: self.idempotency.clone(),
            outbox: vec![self.outbox.clone()],
        }
    }
}

async fn governed_input_fixture(pool: &PgPool) -> GovernedInputFixture {
    let (binding, _, model_revision_uuid) = binding_fixture::prepare_no_auth_binding(
        pool,
        binding_fixture::QualificationRefreshMode::Valid,
    )
    .await;
    let context = binding.context.clone();
    let connection_revision_id = binding.connection_revision_id;
    let model_revision_id = vestrace_domain::ModelRevisionId::from_uuid(model_revision_uuid);
    let run_id = AgentRunId::new();
    binding_fixture::run_service(pool)
        .execute(
            &context,
            binding_fixture::legacy_create_command(&context, run_id),
        )
        .await
        .unwrap();
    binding_fixture::run_service(pool)
        .execute(
            &context,
            binding_fixture::legacy_prepare_command(&context, run_id),
        )
        .await
        .unwrap();
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT snapshot_id FROM run_model_binding_snapshots
          WHERE workspace_id = $1 AND run_id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let (connection_qualification_revision_id, model_qualification_revision_id): (Uuid, Uuid) =
        sqlx::query_as(
            "SELECT connection_qualification_revision_id, model_qualification_revision_id
               FROM model_binding_snapshots
              WHERE workspace_id = $1 AND id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(snapshot_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let step_id = RunStepId::new();
    let assigned_actor = RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new());
    // The boundary builds the intent, then the attempt adopts its id: the
    // effect row has to exist before the MRE root can reference it, and
    // `ExternalEffectIntent::new` keeps its single allocation site.
    let intent = ExternalEffectIntent::new(
        format!("run://{run_id}"),
        context.workspace_id,
        context.principal_id,
        "openai-compatible",
        "chat",
        "https://provider.governed-input.test/v1/chat/completions",
        "sha256:governed-run-step-arguments",
        "produce a governed answer",
        vec![
            vestrace_domain::EffectPrecondition::new("model-snapshot", snapshot_id.to_string())
                .unwrap(),
        ],
        "sha256:governed-run-step-preconditions",
        vestrace_domain::RiskCategory::Medium,
        vestrace_domain::EffectReversibility::Unknown,
        vestrace_domain::IdempotencyProfile::ProviderKey,
        vestrace_domain::DeliverySemantics::AtLeastOnce,
        vestrace_domain::Capability::ExportRead,
        None::<String>,
        None::<String>,
        Utc::now(),
    )
    .unwrap();
    let attempt = RunStepExecutionAttempt {
        id: Uuid::now_v7(),
        workspace_id: context.workspace_id,
        run_id,
        step_id,
        model_binding_snapshot_id: snapshot_id,
        input_material_intent_id: MaterialKeyCreationIntentId::new(),
        input_content_material_id: ContentMaterialId::new(),
        input_material_key_id: MaterialKeyId::new(),
        input_intent_nonce: IntentNonce::new(),
        input_prepared_attachment_id: PreparedMaterialAttachmentId::new(),
        external_effect_id: intent.id(),
        model_request_evidence_id: ModelRequestEvidenceId::new(),
    };
    let now = Utc::now();
    let audit = AuditEvent::new(
        AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        "run.step.input.reserved",
        "run_step",
        step_id.as_uuid(),
        serde_json::json!({"attempt_id": attempt.id}),
        now,
    )
    .unwrap();
    let idempotency = IdempotencyRecord {
        idempotency_key: format!("governed-run-step-input:{}", step_id.as_uuid()),
        workspace_id: context.workspace_id,
        request_hash: "opaque-governed-run-step-input-v1".to_owned(),
        response_payload: None,
        status: "completed".to_owned(),
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
    };
    let outbox = OutboxMessage {
        id: OutboxId::new(),
        workspace_id: context.workspace_id,
        topic: "run.step.input.reserved".to_owned(),
        payload: serde_json::json!({"run_id": run_id, "step_id": step_id}),
        created_at: now,
        attempts: 0,
    };
    GovernedInputFixture {
        context,
        run_id,
        expected_run_version: RunVersion::new(2).unwrap(),
        step_id,
        assigned_actor,
        intent,
        connection_revision_id,
        connection_qualification_revision_id,
        model_revision_id,
        model_qualification_revision_id,
        attempt,
        audit,
        idempotency,
        outbox,
    }
}

async fn assert_governed_input_transaction_absent(pool: &PgPool, fixture: &GovernedInputFixture) {
    let state: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT run_version FROM agent_runs
              WHERE workspace_id = $1 AND id = $2),
            (SELECT COUNT(*) FROM run_events
              WHERE workspace_id = $1 AND run_id = $2 AND run_version = $3),
            (SELECT COUNT(*) FROM run_steps
              WHERE workspace_id = $1 AND run_id = $2 AND id = $4),
            (SELECT COUNT(*) FROM run_step_execution_attempts
              WHERE workspace_id = $1 AND run_id = $2 AND step_id = $4),
            (SELECT COUNT(*) FROM material_key_creation_intents WHERE id = $5),
            (SELECT COUNT(*) FROM idempotency_keys
              WHERE workspace_id = $1 AND idempotency_key = $6),
            (SELECT COUNT(*) FROM audit_events WHERE id = $7),
            (SELECT COUNT(*) FROM outbox WHERE id = $8),
            (SELECT COUNT(*) FROM run_work_items
              WHERE workspace_id = $1 AND run_id = $2 AND step_id = $4
                AND kind = 'execute_step')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.run_id.as_uuid())
    .bind(fixture.expected_run_version.next().unwrap().value() as i64)
    .bind(fixture.step_id.as_uuid())
    .bind(fixture.attempt.input_material_intent_id.as_uuid())
    .bind(&fixture.idempotency.idempotency_key)
    .bind(fixture.audit.id.as_uuid())
    .bind(fixture.outbox.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        state,
        (
            fixture.expected_run_version.value() as i64,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ),
        "an injected transaction failure must leave the Run, attempt, material, evidence, and queue unchanged",
    );
}

async fn independent_pool(source: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(source.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn configure(transaction: &mut sqlx::PgConnection, workspace_id: Uuid, principal_id: Uuid) {
    for (name, value) in [
        ("vestrace.workspace_id", workspace_id.to_string()),
        ("vestrace.principal_id", principal_id.to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(name)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
}

async fn seed_candidate_qualification(
    pool: &PgPool,
    credential: &binding_fixture::CredentialBindingFixture,
    model_revision_id: Uuid,
) -> (Uuid, Uuid, Uuid, Uuid) {
    let workspace_id = credential.fixture.context.workspace_id.as_uuid();
    let old_connection_qualification_id: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM connection_qualification_heads
          WHERE workspace_id = $1 AND connection_revision_id = $2",
    )
    .bind(workspace_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let old_model_qualification_id: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM model_qualification_heads
          WHERE workspace_id = $1 AND model_revision_id = $2",
    )
    .bind(workspace_id)
    .bind(model_revision_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let new_job_id = Uuid::now_v7();
    let new_connection_qualification_id = Uuid::now_v7();
    let new_model_qualification_id = Uuid::now_v7();
    let mut transaction = pool.begin().await.unwrap();
    configure(
        &mut transaction,
        workspace_id,
        credential.fixture.context.principal_id.as_uuid(),
    )
    .await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs
         (id, workspace_id, connection_revision_id, profile_revision, state, completed_at)
         VALUES ($1, $2, $3, 'race/candidate', 'succeeded', NOW())",
    )
    .bind(new_job_id)
    .bind(workspace_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_target_bindings
         (id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch,
          credential_revision_id, credential_slot_id, credential_activation_guard_id,
          expected_slot_version)
         VALUES ($1, $2, $3, $4, $5, 'credential', $6, $7, $8, 2)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(new_job_id)
    .bind(credential.fixture.connection_id.as_uuid())
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .bind(credential.candidate_credential_revision_id)
    .bind(credential.slot_id)
    .bind(credential.activation_guard_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions
         (id, workspace_id, connection_revision_id, qualification_job_id, profile_revision,
          valid_until, capabilities)
         VALUES ($1, $2, $3, $4, 'race/candidate', NOW() + INTERVAL '1 hour', ARRAY['chat'])",
    )
    .bind(new_connection_qualification_id)
    .bind(workspace_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .bind(new_job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions
         (id, workspace_id, model_revision_id, connection_revision_id,
          connection_qualification_revision_id, qualification_job_id, capabilities, valid_until)
         VALUES ($1, $2, $3, $4, $5, $6, ARRAY['chat'], NOW() + INTERVAL '1 hour')",
    )
    .bind(new_model_qualification_id)
    .bind(workspace_id)
    .bind(model_revision_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .bind(new_connection_qualification_id)
    .bind(new_job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (
        old_connection_qualification_id,
        old_model_qualification_id,
        new_connection_qualification_id,
        new_model_qualification_id,
    )
}

async fn rotate_and_publish_candidate_qualification(
    pool: PgPool,
    credential: binding_fixture::CredentialBindingFixture,
    model_revision_id: Uuid,
    candidate_connection_qualification_id: Uuid,
    candidate_model_qualification_id: Uuid,
    barrier: Arc<Barrier>,
) {
    let workspace_id = credential.fixture.context.workspace_id.as_uuid();
    let prior_connection_qualification_id: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM connection_qualification_heads
          WHERE workspace_id = $1 AND connection_revision_id = $2",
    )
    .bind(workspace_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let audit_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events
         (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at)
         VALUES ($1, $2, $3, 'credential.rotation.race', 'credential', $4, '{}'::jsonb, NOW())",
    )
    .bind(audit_id)
    .bind(workspace_id)
    .bind(credential.fixture.context.principal_id.as_uuid())
    .bind(credential.candidate_credential_revision_id)
    .execute(&pool)
    .await
    .unwrap();
    barrier.wait().await;
    let mut transaction = pool.begin().await.unwrap();
    configure(
        &mut transaction,
        workspace_id,
        credential.fixture.context.principal_id.as_uuid(),
    )
    .await;
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_rotate_credential($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(workspace_id)
    .bind(credential.fixture.connection_id.as_uuid())
    .bind(credential.fixture.execution_guard_id)
    .bind(credential.activation_guard_id)
    .bind(credential.slot_id)
    .bind(credential.credential_revision_id)
    .bind(credential.candidate_credential_revision_id)
    .bind(credential.candidate_credential_intent_id)
    .bind(prior_connection_qualification_id)
    .bind(audit_id)
    .bind(1_i64)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE connection_qualification_heads
            SET current_qualification_revision_id = $3, version = version + 1
          WHERE workspace_id = $1 AND connection_revision_id = $2",
    )
    .bind(workspace_id)
    .bind(credential.fixture.connection_revision_id.as_uuid())
    .bind(candidate_connection_qualification_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE model_qualification_heads
            SET current_qualification_revision_id = $3, version = version + 1
          WHERE workspace_id = $1 AND model_revision_id = $2",
    )
    .bind(workspace_id)
    .bind(model_revision_id)
    .bind(candidate_model_qualification_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

#[derive(Clone, Copy)]
struct NextNoAuthTuple {
    connection_revision_id: Uuid,
    model_revision_id: Uuid,
    connection_qualification_id: Uuid,
    model_qualification_id: Uuid,
}

async fn advance_connection_head_and_publish_new_tuple(
    pool: PgPool,
    fixture: binding_fixture::Fixture,
    model_id: vestrace_domain::ModelId,
    next: NextNoAuthTuple,
    barrier: Arc<Barrier>,
) {
    let workspace_id = fixture.context.workspace_id.as_uuid();
    barrier.wait().await;
    let mut transaction = pool.begin().await.unwrap();
    configure(
        &mut transaction,
        workspace_id,
        fixture.context.principal_id.as_uuid(),
    )
    .await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(next.connection_revision_id)
    .bind(workspace_id)
    .bind(fixture.connection_id.as_uuid())
    .bind(fixture.execution_guard_id)
    .bind("lm_studio_local")
    .bind("http://127.0.0.1:1234/v1")
    .bind("http://127.0.0.1:1234/v1")
    .bind("lm-studio-local/race-v2")
    .bind("loopback_only")
    .bind("none")
    .bind(Option::<Uuid>::None)
    .bind(1_i64)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(next.model_revision_id)
    .bind(workspace_id)
    .bind(model_id.as_uuid())
    .bind(fixture.connection_id.as_uuid())
    .bind(fixture.execution_guard_id)
    .bind(next.connection_revision_id)
    .bind("race-chat")
    .bind("chat")
    .bind(Option::<i32>::None)
    .bind(Option::<Uuid>::None)
    .bind(Option::<String>::None)
    .bind(Option::<i32>::None)
    .bind(Option::<Uuid>::None)
    .bind(Option::<String>::None)
    .bind(1_i64)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_no_auth_binding_revision($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(fixture.connection_id.as_uuid())
    .bind(next.connection_revision_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    let no_auth_binding_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM no_auth_binding_revisions
          WHERE workspace_id = $1 AND connection_id = $2 AND connection_revision_id = $3",
    )
    .bind(workspace_id)
    .bind(fixture.connection_id.as_uuid())
    .bind(next.connection_revision_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    let job_id = Uuid::now_v7();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs
         (id, workspace_id, connection_revision_id, profile_revision, state, completed_at)
         VALUES ($1, $2, $3, 'race/head-v2', 'succeeded', NOW())",
    )
    .bind(job_id)
    .bind(workspace_id)
    .bind(next.connection_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_target_bindings
         (id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch,
          no_auth_binding_revision_id)
         VALUES ($1, $2, $3, $4, $5, 'no_auth', $6)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(job_id)
    .bind(fixture.connection_id.as_uuid())
    .bind(next.connection_revision_id)
    .bind(no_auth_binding_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions
         (id, workspace_id, connection_revision_id, qualification_job_id, profile_revision,
          valid_until, capabilities)
         VALUES ($1, $2, $3, $4, 'race/head-v2', NOW() + INTERVAL '1 hour', ARRAY['chat'])",
    )
    .bind(next.connection_qualification_id)
    .bind(workspace_id)
    .bind(next.connection_revision_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions
         (id, workspace_id, model_revision_id, connection_revision_id,
          connection_qualification_revision_id, qualification_job_id, capabilities, valid_until)
         VALUES ($1, $2, $3, $4, $5, $6, ARRAY['chat'], NOW() + INTERVAL '1 hour')",
    )
    .bind(next.model_qualification_id)
    .bind(workspace_id)
    .bind(next.model_revision_id)
    .bind(next.connection_revision_id)
    .bind(next.connection_qualification_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_heads
         (workspace_id, connection_revision_id, current_qualification_revision_id, version)
         VALUES ($1, $2, $3, 1)",
    )
    .bind(workspace_id)
    .bind(next.connection_revision_id)
    .bind(next.connection_qualification_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_heads
         (workspace_id, model_revision_id, current_qualification_revision_id, version)
         VALUES ($1, $2, $3, 1)",
    )
    .bind(workspace_id)
    .bind(next.model_revision_id)
    .bind(next.model_qualification_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn run_acceptance_races_credential_rotation_on_two_independent_pools(pool: PgPool) {
    let (credential, model_revision_id) = binding_fixture::prepare_credential_binding(&pool).await;
    let (
        old_connection_qualification_id,
        old_model_qualification_id,
        new_connection_qualification_id,
        new_model_qualification_id,
    ) = seed_candidate_qualification(&pool, &credential, model_revision_id).await;
    let acceptance_pool = independent_pool(&pool).await;
    let rotation_pool = independent_pool(&pool).await;
    let barrier = Arc::new(Barrier::new(2));
    let acceptance_barrier = barrier.clone();
    let run_id = AgentRunId::new();
    let context = credential.fixture.context.clone();
    binding_fixture::run_service(&pool)
        .execute(
            &context,
            binding_fixture::legacy_create_command(&context, run_id),
        )
        .await
        .unwrap();
    let acceptance = async {
        acceptance_barrier.wait().await;
        binding_fixture::run_service(&acceptance_pool)
            .execute(
                &context,
                binding_fixture::legacy_prepare_command(&context, run_id),
            )
            .await
            .unwrap();
    };
    let rotation = rotate_and_publish_candidate_qualification(
        rotation_pool,
        credential.clone(),
        model_revision_id,
        new_connection_qualification_id,
        new_model_qualification_id,
        barrier,
    );
    tokio::join!(acceptance, rotation);

    let row: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT snapshot.credential_revision_id, snapshot.connection_qualification_revision_id,
                snapshot.model_qualification_revision_id
           FROM run_model_binding_snapshots AS link
           JOIN model_binding_snapshots AS snapshot ON snapshot.id = link.snapshot_id
          WHERE link.run_id = $1",
    )
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        row == (
            credential.credential_revision_id,
            old_connection_qualification_id,
            old_model_qualification_id
        ) || row
            == (
                credential.candidate_credential_revision_id,
                new_connection_qualification_id,
                new_model_qualification_id
            ),
        "the snapshot must contain one complete old or new credential tuple, never a mixture: {row:?}"
    );
    let (accepted_at, rotated_at): (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "SELECT snapshot.accepted_at, rotation.recorded_at
           FROM run_model_binding_snapshots AS link
           JOIN model_binding_snapshots AS snapshot ON snapshot.id = link.snapshot_id
           JOIN credential_rotation_events AS rotation
             ON rotation.activated_credential_revision_id = $1
          WHERE link.run_id = $2",
    )
    .bind(credential.candidate_credential_revision_id)
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    if row.0 == credential.credential_revision_id {
        assert!(
            accepted_at <= rotated_at,
            "an old credential may be pinned only before the rotation retires it"
        );
    } else {
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM credential_key_creation_intents WHERE id = $1",
            )
            .bind(credential.candidate_credential_intent_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            "active",
            "a new snapshot may name only the activated successor"
        );
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn run_acceptance_races_connection_head_advance_on_two_independent_pools(pool: PgPool) {
    let (fixture, model_id, old_model_revision_id) = binding_fixture::prepare_no_auth_binding(
        &pool,
        binding_fixture::QualificationRefreshMode::Valid,
    )
    .await;
    let old_connection_qualification_id: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM connection_qualification_heads
          WHERE workspace_id = $1 AND connection_revision_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let old_model_qualification_id: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM model_qualification_heads
          WHERE workspace_id = $1 AND model_revision_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(old_model_revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let next = NextNoAuthTuple {
        connection_revision_id: Uuid::now_v7(),
        model_revision_id: Uuid::now_v7(),
        connection_qualification_id: Uuid::now_v7(),
        model_qualification_id: Uuid::now_v7(),
    };
    let acceptance_pool = independent_pool(&pool).await;
    let advance_pool = independent_pool(&pool).await;
    let barrier = Arc::new(Barrier::new(2));
    let acceptance_barrier = barrier.clone();
    let context = fixture.context.clone();
    let run_id = AgentRunId::new();
    binding_fixture::run_service(&pool)
        .execute(
            &context,
            binding_fixture::legacy_create_command(&context, run_id),
        )
        .await
        .unwrap();
    let acceptance = async {
        acceptance_barrier.wait().await;
        binding_fixture::run_service(&acceptance_pool)
            .execute(
                &context,
                binding_fixture::legacy_prepare_command(&context, run_id),
            )
            .await
            .unwrap();
    };
    let advance = advance_connection_head_and_publish_new_tuple(
        advance_pool,
        fixture.clone(),
        model_id,
        next,
        barrier,
    );
    tokio::join!(acceptance, advance);
    let row: (Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT connection_revision_id, model_revision_id,
                connection_qualification_revision_id, model_qualification_revision_id
           FROM model_binding_snapshots AS snapshot
           JOIN run_model_binding_snapshots AS link ON link.snapshot_id = snapshot.id
          WHERE link.run_id = $1",
    )
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        row == (
            fixture.connection_revision_id.as_uuid(),
            old_model_revision_id,
            old_connection_qualification_id,
            old_model_qualification_id,
        ) || row
            == (
                next.connection_revision_id,
                next.model_revision_id,
                next.connection_qualification_id,
                next.model_qualification_id,
            ),
        "the snapshot must contain one complete old or new connection tuple, never a mixture: {row:?}"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_run_step_input_commits_exact_atomic_tuple_and_schedules_one_execute_step(
    pool: PgPool,
) {
    let fixture = governed_input_fixture(&pool).await;
    let outcome = governed_input_authority(&pool)
        .accept(&fixture.context, fixture.command())
        .await
        .expect("the governed Run-step input tuple must commit through one transaction");

    assert_eq!(
        outcome.run.run.version,
        fixture.expected_run_version.next().unwrap()
    );
    assert_eq!(outcome.attempt, fixture.attempt);
    assert_eq!(outcome.run.steps.len(), 1);
    assert_eq!(outcome.run.steps[0].id, fixture.step_id);
    assert_eq!(outcome.run.steps[0].assigned_actor, fixture.assigned_actor);

    let event_actor: serde_json::Value = sqlx::query_scalar(
        "SELECT actor FROM run_events
          WHERE workspace_id = $1 AND run_id = $2 AND run_version = $3",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.run_id.as_uuid())
    .bind(outcome.run.run.version.value() as i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_value::<RunActorRef>(event_actor).unwrap(),
        RunActorRef::Principal(fixture.context.principal_id),
        "the assigned agent owns step execution, not authorship of the caller's Run event",
    );

    let attempt: (
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        String,
    ) = sqlx::query_as(
        "SELECT id, run_id, step_id, model_binding_snapshot_id,
                input_material_intent_id, input_content_material_id, input_material_key_id,
                input_intent_nonce, input_prepared_attachment_id, external_effect_id,
                model_request_evidence_id, phase
           FROM run_step_execution_attempts
          WHERE workspace_id = $1 AND run_id = $2 AND step_id = $3",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.run_id.as_uuid())
    .bind(fixture.step_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        attempt,
        (
            fixture.attempt.id,
            fixture.run_id.as_uuid(),
            fixture.step_id.as_uuid(),
            fixture.attempt.model_binding_snapshot_id,
            fixture.attempt.input_material_intent_id.as_uuid(),
            fixture.attempt.input_content_material_id.as_uuid(),
            fixture.attempt.input_material_key_id.as_uuid(),
            fixture.attempt.input_intent_nonce.as_uuid(),
            fixture.attempt.input_prepared_attachment_id.as_uuid(),
            fixture.attempt.external_effect_id.as_uuid(),
            fixture.attempt.model_request_evidence_id.as_uuid(),
            "reserved".to_owned(),
        ),
        "the attempt must retain every request-allocated identity and exact Run binding",
    );

    let material: (Uuid, Uuid, Uuid, Uuid, String, Uuid, i64, String) = sqlx::query_as(
        "SELECT id, material_id, material_key_id, nonce,
                owner_kind, owner_id, output_ordinal, state
           FROM material_key_creation_intents WHERE id = $1",
    )
    .bind(fixture.attempt.input_material_intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        material,
        (
            fixture.attempt.input_material_intent_id.as_uuid(),
            fixture.attempt.input_content_material_id.as_uuid(),
            fixture.attempt.input_material_key_id.as_uuid(),
            fixture.attempt.input_intent_nonce.as_uuid(),
            "model_request_input".to_owned(),
            fixture.step_id.as_uuid(),
            0,
            // The owner tuple is what this test pins. The lifecycle reaches
            // `live` because acceptance completes the material after its
            // transaction commits; the intermediate `reserved` state is what
            // the two fault-injection tests prove, by aborting before it.
            "live".to_owned(),
        ),
    );

    let evidence_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM idempotency_keys
              WHERE workspace_id = $1 AND idempotency_key = $2),
            (SELECT COUNT(*) FROM audit_events WHERE id = $3),
            (SELECT COUNT(*) FROM outbox WHERE id = $4)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(&fixture.idempotency.idempotency_key)
    .bind(fixture.audit.id.as_uuid())
    .bind(fixture.outbox.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence_counts, (1, 1, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_work_items
              WHERE workspace_id = $1 AND run_id = $2 AND step_id = $3
                AND kind = 'execute_step'",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "acceptance schedules exactly one ExecuteStep once material and evidence are durable",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_run_step_input_prepares_live_material_and_complete_exact_mre_then_schedules(
    pool: PgPool,
) {
    let fixture = governed_input_fixture(&pool).await;
    let vault = Arc::new(RecordingVault::new());

    governed_material_authority(&pool, vault.clone())
        .accept(&fixture.context, fixture.command())
        .await
        .expect("governed input acceptance must finish its material and MRE lifecycle");

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM external_effect_intents
              WHERE workspace_id = $1 AND id = $2",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.attempt.external_effect_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "the exact effect identity must exist before the MRE root can reference it",
    );
    let material_state: String =
        sqlx::query_scalar("SELECT state FROM material_key_creation_intents WHERE id = $1")
            .bind(fixture.attempt.input_material_intent_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(material_state, "live");
    assert_eq!(vault.create_count.load(Ordering::SeqCst), 1);
    assert_eq!(vault.unwrap_count.load(Ordering::SeqCst), 2);

    let root: (Uuid, Uuid, Uuid, String, Uuid) = sqlx::query_as(
        "SELECT external_effect_id, binding_snapshot_id, cause_id, cause_kind, id
           FROM model_request_evidence_roots
          WHERE workspace_id = $1 AND id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.attempt.model_request_evidence_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        root,
        (
            fixture.attempt.external_effect_id.as_uuid(),
            fixture.attempt.model_binding_snapshot_id,
            fixture.step_id.as_uuid(),
            "run_step".to_owned(),
            fixture.attempt.model_request_evidence_id.as_uuid(),
        )
    );
    let governed_nodes: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT reference_id, safe_ordinal
           FROM model_request_evidence_nodes
          WHERE evidence_root_id = $1 AND reference_kind = 'governed_input_material'
          ORDER BY ordinal",
    )
    .bind(fixture.attempt.model_request_evidence_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        governed_nodes,
        vec![(
            fixture.attempt.input_content_material_id.as_uuid(),
            Some("0".to_owned()),
        )]
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM model_request_evidence_checks
              WHERE evidence_root_id = $1 ORDER BY checked_at DESC, id DESC LIMIT 1",
        )
        .bind(fixture.attempt.model_request_evidence_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        "complete"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_work_items
              WHERE workspace_id = $1 AND run_id = $2 AND step_id = $3
                AND kind = 'execute_step'",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "the guarded enqueue runs last, so exactly one ExecuteStep exists once          the input material is Live and its exact MRE is Complete",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_run_step_input_fault_after_run_commit_rolls_back_every_leg(pool: PgPool) {
    let fixture = governed_input_fixture(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let authority = governed_input_authority_with(
        &pool,
        FailAfterRunCommit {
            inner: PostgresRunStore::new(&store),
        },
        PgGovernedMutationRepository::new(store),
        Arc::new(UnusedVault),
    );

    let error = match authority.accept(&fixture.context, fixture.command()).await {
        Ok(_) => panic!("the injected post-Run fault must abort the caller-owned transaction"),
        Err(error) => error,
    };
    assert!(
        matches!(error, ApplicationError::Unavailable(ref message)
            if message == "injected fault after governed Run-step commit"),
        "the test must observe its exact injected fault, got {error:?}",
    );
    assert_governed_input_transaction_absent(&pool, &fixture).await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_run_step_input_fault_after_attempt_and_material_rolls_back_every_leg(
    pool: PgPool,
) {
    let fixture = governed_input_fixture(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let authority = governed_input_authority_with(
        &pool,
        PostgresRunStore::new(&store),
        FailAfterInputReservations,
        Arc::new(UnusedVault),
    );

    let error = match authority.accept(&fixture.context, fixture.command()).await {
        Ok(_) => {
            panic!("the injected post-reservation fault must abort the caller-owned transaction")
        }
        Err(error) => error,
    };
    assert!(
        matches!(error, ApplicationError::Unavailable(ref message)
            if message == "injected fault after governed Run-step input reservations"),
        "the test must observe its exact injected fault, got {error:?}",
    );
    assert_governed_input_transaction_absent(&pool, &fixture).await;
}

/// Acceptance must be able to recognise a step it has already reserved before
/// it authors anything for it. This read is what will let a same-Request-Id
/// replay reuse the original `external_effect_id` instead of allocating a
/// second intent and colliding with the guarded reserver's identity check.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_reserved_run_step_attempt_is_readable_by_its_exact_run_and_step(pool: PgPool) {
    let fixture = governed_input_fixture(&pool).await;
    governed_input_authority(&pool)
        .accept(&fixture.context, fixture.command())
        .await
        .expect("the governed Run-step input tuple must commit");

    let transactions = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let dispatch = governed_dispatch_repository(&pool);
    let mut unit_of_work = transactions.begin(&fixture.context).await.unwrap();

    let found = dispatch
        .load_run_step_attempt_in(
            &fixture.context,
            unit_of_work.as_mut(),
            fixture.run_id,
            fixture.step_id,
        )
        .await
        .expect("a reserved attempt is readable");

    assert_eq!(
        found.as_ref(),
        Some(&fixture.attempt),
        "the lookup must return the exact reserved tuple, not a rebuilt one"
    );

    // A step with no reservation answers None rather than erroring, so
    // acceptance can distinguish "already reserved" from "first write".
    let absent = dispatch
        .load_run_step_attempt_in(
            &fixture.context,
            unit_of_work.as_mut(),
            fixture.run_id,
            RunStepId::new(),
        )
        .await
        .expect("an unreserved step is a lawful absence, not a failure");
    assert!(absent.is_none());

    unit_of_work.rollback().await.unwrap();
}

/// The worker is handed a step id and nothing else, so everything it dispatches
/// has to come back out of the durable record. This proves the plan is that
/// record: the exact reserved attempt, the intent already persisted under its
/// effect id, and the Connection revision the Run pinned — with the auth branch
/// read from the snapshot rather than defaulted.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_governed_run_step_dispatch_plan_is_read_back_from_the_pinned_binding(pool: PgPool) {
    let fixture = governed_input_fixture(&pool).await;
    governed_input_authority(&pool)
        .accept(&fixture.context, fixture.command())
        .await
        .expect("the governed Run-step input tuple must commit");

    let (connection_id, branch): (Uuid, String) = sqlx::query_as(
        "SELECT snapshot.connection_id, snapshot.branch
           FROM model_binding_snapshots AS snapshot
          WHERE snapshot.workspace_id = $1 AND snapshot.id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.attempt.model_binding_snapshot_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(branch, "no_auth");

    let dispatch = governed_dispatch_repository(&pool);
    let plan = dispatch
        .load_run_step_dispatch_plan(&fixture.context, fixture.run_id, fixture.step_id)
        .await
        .expect("a reserved attempt must yield its durable dispatch plan");

    assert_eq!(plan.attempt, fixture.attempt);
    // Dispatch takes its effect id from the intent while everything downstream
    // takes it from the attempt. The plan is where the two are proved to be the
    // same effect rather than two that merely travel together.
    assert_eq!(plan.intent.id(), fixture.attempt.external_effect_id);
    assert_eq!(plan.intent.workspace_id(), fixture.context.workspace_id);
    assert_eq!(plan.intent.adapter(), fixture.intent.adapter());
    assert_eq!(plan.intent.target(), fixture.intent.target());
    assert_eq!(plan.connection_revision_id, fixture.connection_revision_id);
    assert_eq!(plan.connection_id.as_uuid(), connection_id);
    // The pinned branch is no_auth, so there is no credential to lease at all.
    // An implementation that defaulted a branch would manufacture one here.
    assert!(plan.credential.is_none());

    // A step with no reservation is a conflict, not an empty plan. Answering
    // absence would let a worker conclude there is nothing to dispatch for a
    // step whose provider call may already be in flight.
    let error = dispatch
        .load_run_step_dispatch_plan(&fixture.context, fixture.run_id, RunStepId::new())
        .await
        .expect_err("an unreserved step has no dispatch plan to hand out");
    assert!(matches!(error, ApplicationError::Conflict(_)), "{error:?}");
}

/// End-to-end governed execution over the real durable state that acceptance
/// leaves behind.
///
/// Everything here is the production graph except the vault, the policy verdict
/// and the socket: the dispatch transaction, the MRE reconstruction that
/// decrypts the accepted input, the result preparation and the publication are
/// the real PostgreSQL authorities. What is under test is that the executor
/// reaches the provider exactly once and never reaches it again afterwards.
mod governed_execution {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use sqlx::PgPool;
    use uuid::Uuid;
    use vestrace_application::run::RunStorePort;
    use vestrace_application::run::ports::{RunLease, WorkItem, WorkItemKind};
    use vestrace_application::run::{
        ExecuteStepHandler, GovernedModelAdapter, GovernedProviderStepExecutor, RunWorkHandler,
        RunWorkOutcome, StepModelExecutor, StepModelOutcome, StepModelRequest, SystemClock,
    };
    use vestrace_application::{
        ApplicationError, ConnectionAuth, EffectiveChatEvidence, EffectiveChatFinishReason,
        EffectiveChatResult, EffectiveModelRequest, EffectiveModelResponse,
        GovernedRunStepInputAuthority, ModelDataPolicyDecisionRecord, ModelDataPolicyMode,
        ProviderDispatchCause, ProviderDispatchPolicyEvaluation, ProviderDispatchPolicyEvaluator,
        ProviderDispatchTarget, ProviderError, ProviderUsage, RequestContext,
        SharedProviderDispatchRepository,
    };
    use vestrace_domain::{
        AuthorizationRequest, ConnectionKind, ExternalEffectIntent, PolicyDecision,
        PolicyDecisionId, PolicyDecisionReason, PolicyDecisionResult, PolicyInputState, WorkerId,
    };
    use vestrace_infrastructure::crypto::ContentMaterialCodec;
    use vestrace_infrastructure::{
        PgCredentialDispatchLeaseRepository, PgExternalEffectRepository,
        PgGovernedMutationRepository, PgInstallationMutationPermit,
        PgModelDataPolicyDecisionRepository, PgModelRequestEvidenceRepository,
        PgProviderDispatchRepository, PgProviderResultRepository, PgStore, PostgresRunStore,
    };

    /// Allows the exact effect and records the Run decision the dispatch
    /// transaction requires. The production evaluator has its own tests; what
    /// this one must not do is let an unbound decision through.
    struct AllowExactRunStep;

    #[async_trait]
    impl ProviderDispatchPolicyEvaluator for AllowExactRunStep {
        async fn evaluate(
            &self,
            context: &RequestContext,
            intent: &ExternalEffectIntent,
            cause: &ProviderDispatchCause,
            target: &ProviderDispatchTarget,
            _request: &EffectiveModelRequest,
        ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
            let auth_request = AuthorizationRequest::new(
                intent.required_capability(),
                intent.operation(),
                intent.target(),
                intent.risk(),
            );
            let ProviderDispatchCause::RunStep {
                run_id, step_id, ..
            } = cause
            else {
                return Err(ApplicationError::Policy(
                    "this fixture evaluates Run steps only".to_owned(),
                ));
            };
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
                    run_id: *run_id,
                    step_id: *step_id,
                    destination: target.destination,
                    classification: vestrace_domain::Sensitivity::Internal,
                    allowed: true,
                    reason: "fixture allowance".to_owned(),
                    policy_version: "provider-dispatch-v1".to_owned(),
                    mode: ModelDataPolicyMode::Enforce,
                    decided_at: chrono::Utc::now(),
                }),
            })
        }
    }

    /// Counts calls and answers one bounded completion. A second call would be
    /// a second external effect, so the count is the assertion that matters.
    #[derive(Default)]
    struct CountingModelAdapter {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl GovernedModelAdapter for CountingModelAdapter {
        async fn execute(
            &self,
            _kind: ConnectionKind,
            _runtime_base_url: &str,
            _auth: ConnectionAuth,
            _request: EffectiveModelRequest,
        ) -> Result<EffectiveModelResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(EffectiveModelResponse::ChatCompletions(
                EffectiveChatResult::new(
                    zeroize::Zeroizing::new("the governed completion".to_owned()),
                    EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                    ProviderUsage::Known {
                        prompt_tokens: 3,
                        completion_tokens: 5,
                    },
                )
                .unwrap(),
            ))
        }
    }

    /// A provider that dies mid-call, leaving exactly what a killed worker
    /// leaves: the dispatch transaction committed, no receipt, and no answer.
    struct PanickingModelAdapter;

    #[async_trait]
    impl GovernedModelAdapter for PanickingModelAdapter {
        async fn execute(
            &self,
            _kind: ConnectionKind,
            _runtime_base_url: &str,
            _auth: ConnectionAuth,
            _request: EffectiveModelRequest,
        ) -> Result<EffectiveModelResponse, ProviderError> {
            panic!("the worker died before the provider answered");
        }
    }

    /// No production path publishes a Connection admission policy yet, and the
    /// guarded admission function refuses without one. Both executor tests seed
    /// it directly so they stay about execution rather than about that gap.
    async fn seed_admission_policy(
        pool: &PgPool,
        workspace: Uuid,
        principal: Uuid,
        snapshot_id: Uuid,
    ) -> Uuid {
        let connection_id: Uuid = sqlx::query_scalar(
            "SELECT connection_id FROM model_binding_snapshots
              WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace)
        .bind(snapshot_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let policy_revision_id = Uuid::now_v7();
        let mut guarded = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *guarded)
            .await
            .unwrap();
        super::configure(&mut guarded, workspace, principal).await;
        sqlx::query(
            "INSERT INTO connection_admission_policy_revisions(
                 id, workspace_id, connection_id, version, max_in_flight,
                 requests_per_60_seconds, queue_wait_timeout_seconds,
                 provider_throttle_cap_seconds)
             VALUES($1,$2,$3,1,1,60000,30,900)",
        )
        .bind(policy_revision_id)
        .bind(workspace)
        .bind(connection_id)
        .execute(&mut *guarded)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO connection_admission_policy_heads(
                 workspace_id, connection_id, current_policy_revision_id, version)
             VALUES($1,$2,$3,1)",
        )
        .bind(workspace)
        .bind(connection_id)
        .bind(policy_revision_id)
        .execute(&mut *guarded)
        .await
        .unwrap();
        guarded.commit().await.unwrap();
        connection_id
    }

    fn executing_dispatch_repository(
        pool: &PgPool,
        vault: Arc<super::RecordingVault>,
    ) -> PgProviderDispatchRepository {
        let store = PgStore::from_pool(pool.clone());
        PgProviderDispatchRepository::new(
            Arc::new(PgInstallationMutationPermit::new(store.clone())),
            Arc::new(PgModelRequestEvidenceRepository::new(vault.clone())),
            Arc::new(PgExternalEffectRepository::new(store.clone())),
            Arc::new(PgCredentialDispatchLeaseRepository::new(
                store.clone(),
                vault,
            )),
            Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
            Arc::new(PgGovernedMutationRepository::new(store)),
            Arc::new(AllowExactRunStep),
        )
    }

    #[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
    async fn the_governed_executor_dispatches_an_accepted_step_once_and_never_again(pool: PgPool) {
        let vault = Arc::new(super::RecordingVault::new());
        let fixture = super::governed_input_fixture(&pool).await;
        super::governed_material_authority(&pool, vault.clone())
            .accept(&fixture.context, fixture.command())
            .await
            .expect("the governed Run-step input tuple must commit");

        let _connection_id = seed_admission_policy(
            &pool,
            fixture.context.workspace_id.as_uuid(),
            fixture.context.principal_id.as_uuid(),
            fixture.attempt.model_binding_snapshot_id,
        )
        .await;

        let store = PgStore::from_pool(pool.clone());
        let dispatch: SharedProviderDispatchRepository =
            Arc::new(executing_dispatch_repository(&pool, vault.clone()));
        let results = Arc::new(
            PgProviderResultRepository::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                vault,
                Arc::new(ContentMaterialCodec::new()),
                Arc::new(PgExternalEffectRepository::new(store.clone())),
                Arc::new(PostgresRunStore::new(&store)),
            )
            .with_provider_dispatch(dispatch.clone()),
        );
        let adapter = Arc::new(CountingModelAdapter::default());
        let executor =
            GovernedProviderStepExecutor::new(dispatch, results, adapter.clone(), WorkerId::new());

        // Driven through the real handler rather than around it. Result
        // preparation requires the step to be Running, and moving it there is
        // the handler's job; a test that set the status itself would prove the
        // executor works in a state production never produces.
        let runs = Arc::new(PostgresRunStore::new(&store));
        let handler = ExecuteStepHandler::new(runs.clone(), Arc::new(SystemClock))
            .with_model_executor(Arc::new(executor));
        // Result preparation requires a Running Run, which `AdvanceRunHandler`
        // establishes in production before an ExecuteStep item is ever leased.
        // This fixture stops short of the coordinator, so it states that
        // precondition directly rather than pretending the executor works on a
        // Run that was never started.
        sqlx::query("UPDATE agent_runs SET status='running' WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.run_id.as_uuid())
            .execute(&pool)
            .await
            .unwrap();

        let snapshot = runs
            .load(&fixture.context, fixture.run_id)
            .await
            .unwrap()
            .expect("the accepted run must be readable");
        let item = WorkItem {
            id: vestrace_domain::id::WorkItemId::new(),
            run_id: fixture.run_id,
            kind: WorkItemKind::ExecuteStep {
                step_id: fixture.step_id,
            },
            expected_run_version: snapshot.run.version,
            available_at: chrono::Utc::now(),
            idempotency_key: format!("step:{}", fixture.step_id.as_uuid()),
            attempt: 1,
        };
        let lease = RunLease {
            run_id: fixture.run_id,
            worker_id: WorkerId::new(),
            generation: 1,
            acquired_at: chrono::Utc::now(),
            heartbeat_at: chrono::Utc::now(),
            lease_until: chrono::Utc::now() + chrono::Duration::minutes(5),
        };

        let outcome = handler
            .handle(&fixture.context, &snapshot, &item, &lease)
            .await
            .expect("an accepted step must execute through the governed path");
        assert!(matches!(outcome, RunWorkOutcome::Completed));
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);

        let phase: String = sqlx::query_scalar(
            "SELECT phase FROM run_step_execution_attempts
              WHERE workspace_id = $1 AND run_id = $2 AND step_id = $3",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(phase, "published");
        let step_status: String = sqlx::query_scalar(
            "SELECT status FROM run_steps
              WHERE workspace_id = $1 AND run_id = $2 AND id = $3",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        // Published by the result finalizer, not by the handler: the handler
        // returned Completed without writing a success of its own.
        assert_eq!(step_status, "succeeded");

        // The sentinel the acceptance command sealed must not have surfaced in
        // any safe column on the way through.
        let leaked: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM run_events
                      WHERE workspace_id = $1 AND payload::text LIKE '%governed input sentinel%')
                  + (SELECT COUNT(*) FROM audit_events
                      WHERE workspace_id = $1 AND payload::text LIKE '%governed input sentinel%')
                  + (SELECT COUNT(*) FROM outbox
                      WHERE workspace_id = $1 AND payload::text LIKE '%governed input sentinel%')",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(leaked, 0, "the governed input must not reach a safe column");

        // A published attempt is finished. Running the work item again must
        // report that and must not produce a second external effect.
        let repeat_adapter = Arc::new(CountingModelAdapter::default());
        let repeat_vault = Arc::new(super::RecordingVault::new());
        let repeat_dispatch: SharedProviderDispatchRepository =
            Arc::new(executing_dispatch_repository(&pool, repeat_vault.clone()));
        let repeat_results = Arc::new(
            PgProviderResultRepository::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                repeat_vault,
                Arc::new(ContentMaterialCodec::new()),
                Arc::new(PgExternalEffectRepository::new(store.clone())),
                Arc::new(PostgresRunStore::new(&store)),
            )
            .with_provider_dispatch(repeat_dispatch.clone()),
        );
        let repeat = GovernedProviderStepExecutor::new(
            repeat_dispatch,
            repeat_results,
            repeat_adapter.clone(),
            WorkerId::new(),
        )
        .execute(
            &fixture.context,
            StepModelRequest {
                run_id: fixture.run_id,
                step_id: fixture.step_id,
            },
        )
        .await
        .expect("a published attempt is reported, not re-executed");
        assert_eq!(repeat, StepModelOutcome::Published);
        assert_eq!(repeat_adapter.calls.load(Ordering::SeqCst), 0);
    }
    /// A worker that dies between committing its dispatch and hearing from the
    /// provider must never call the provider again.
    ///
    /// The crash is performed rather than described. `prepare_dispatch` commits
    /// before the network call, so an adapter that panics leaves exactly what a
    /// killed process leaves: phase `dispatching`, a live deadline, one
    /// `dispatching` transition and no receipt.
    ///
    /// This covers the boundary inside the deadline, which is the one a test
    /// can reach honestly. The deadline cannot be brought forward: it is
    /// `connection_dispatch_admissions.dispatch_expires_at`, that table accepts
    /// guarded inserts only, and a deferred contract requires the lifecycle
    /// transition to carry the identical value — so nothing short of waiting
    /// out the sixty-second TTL can age it. That the deadline is unforgeable is
    /// the property, not an obstacle; the adopted-unknown side of the boundary
    /// is proved against the database in
    /// `provider_dispatch_is_atomic::run_step_attempt_recovery_adopts_only_the_expired_original_dispatch_as_unknown_without_re_admission`.
    ///
    /// Nothing here is an in-memory double: the repositories, the vault, the
    /// Run store and the handler are the production types over real
    /// PostgreSQL. The adapter is the process boundary the crash happens at.
    #[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
    async fn a_crash_between_dispatch_and_receipt_recovers_without_a_second_provider_call(
        pool: PgPool,
    ) {
        let vault = Arc::new(super::RecordingVault::new());
        let fixture = super::governed_input_fixture(&pool).await;
        super::governed_material_authority(&pool, vault.clone())
            .accept(&fixture.context, fixture.command())
            .await
            .expect("the governed Run-step input tuple must commit");
        seed_admission_policy(
            &pool,
            fixture.context.workspace_id.as_uuid(),
            fixture.context.principal_id.as_uuid(),
            fixture.attempt.model_binding_snapshot_id,
        )
        .await;
        sqlx::query("UPDATE agent_runs SET status='running' WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.run_id.as_uuid())
            .execute(&pool)
            .await
            .unwrap();

        let store = PgStore::from_pool(pool.clone());
        let crashing_dispatch: SharedProviderDispatchRepository =
            Arc::new(executing_dispatch_repository(&pool, vault.clone()));
        let crashing_results = Arc::new(
            PgProviderResultRepository::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                vault,
                Arc::new(ContentMaterialCodec::new()),
                Arc::new(PgExternalEffectRepository::new(store.clone())),
                Arc::new(PostgresRunStore::new(&store)),
            )
            .with_provider_dispatch(crashing_dispatch.clone()),
        );
        let crashing = GovernedProviderStepExecutor::new(
            crashing_dispatch,
            crashing_results,
            Arc::new(PanickingModelAdapter),
            WorkerId::new(),
        );
        let context = fixture.context.clone();
        let run_id = fixture.run_id;
        let step_id = fixture.step_id;
        let crash = tokio::spawn(async move {
            crashing
                .execute(&context, StepModelRequest { run_id, step_id })
                .await
        })
        .await;
        assert!(
            crash.is_err(),
            "the simulated crash must unwind out of the adapter"
        );

        // What the crash left behind, before anything recovers it.
        let (phase, dispatching, receipts): (String, i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT phase FROM run_step_execution_attempts
                  WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3),
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
                  WHERE workspace_id=$1 AND status='dispatching'),
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
                  WHERE workspace_id=$1 AND status IN ('succeeded','failed','unknown'))",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(phase, "dispatching");
        assert_eq!(dispatching, 1);
        assert_eq!(receipts, 0, "a crash leaves no receipt");

        let recovering_vault = Arc::new(super::RecordingVault::new());
        let recovering_dispatch: SharedProviderDispatchRepository = Arc::new(
            executing_dispatch_repository(&pool, recovering_vault.clone()),
        );
        let recovering_results = Arc::new(
            PgProviderResultRepository::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                recovering_vault,
                Arc::new(ContentMaterialCodec::new()),
                Arc::new(PgExternalEffectRepository::new(store.clone())),
                Arc::new(PostgresRunStore::new(&store)),
            )
            .with_provider_dispatch(recovering_dispatch.clone()),
        );
        let recovering_adapter = Arc::new(CountingModelAdapter::default());
        let runs = Arc::new(PostgresRunStore::new(&store));
        let handler = ExecuteStepHandler::new(runs.clone(), Arc::new(SystemClock))
            .with_model_executor(Arc::new(GovernedProviderStepExecutor::new(
                recovering_dispatch,
                recovering_results,
                recovering_adapter.clone(),
                WorkerId::new(),
            )));

        let snapshot = runs
            .load(&fixture.context, fixture.run_id)
            .await
            .unwrap()
            .expect("the accepted run must be readable");
        let item = WorkItem {
            id: vestrace_domain::id::WorkItemId::new(),
            run_id: fixture.run_id,
            kind: WorkItemKind::ExecuteStep {
                step_id: fixture.step_id,
            },
            expected_run_version: snapshot.run.version,
            available_at: chrono::Utc::now(),
            idempotency_key: format!("step:{}", fixture.step_id.as_uuid()),
            attempt: 2,
        };
        let lease = RunLease {
            run_id: fixture.run_id,
            worker_id: WorkerId::new(),
            generation: 1,
            acquired_at: chrono::Utc::now(),
            heartbeat_at: chrono::Utc::now(),
            lease_until: chrono::Utc::now() + chrono::Duration::minutes(5),
        };
        let outcome = handler
            .handle(&fixture.context, &snapshot, &item, &lease)
            .await
            .expect("recovery must report an outcome rather than error out");

        assert_eq!(
            recovering_adapter.calls.load(Ordering::SeqCst),
            0,
            "a dispatch that already left the process must never reach the provider again"
        );
        assert!(
            matches!(outcome, RunWorkOutcome::Retry { .. }),
            "a live dispatch owned by a dead worker is rescheduled, not failed and not repeated: {outcome:?}"
        );

        let (recovered_phase, dispatching_after, receipts_after): (String, i64, i64) =
            sqlx::query_as(
                "SELECT
                (SELECT phase FROM run_step_execution_attempts
                  WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3),
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
                  WHERE workspace_id=$1 AND status='dispatching'),
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions
                  WHERE workspace_id=$1 AND status IN ('succeeded','failed','unknown'))",
            )
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.run_id.as_uuid())
            .bind(fixture.step_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(recovered_phase, "dispatching");
        assert_eq!(
            dispatching_after, 1,
            "recovery must not open a second dispatch"
        );
        assert_eq!(
            receipts_after, 0,
            "recovery inside the deadline must invent no outcome for a call it did not make"
        );
        let step_status: String = sqlx::query_scalar(
            "SELECT status FROM run_steps WHERE workspace_id=$1 AND run_id=$2 AND id=$3",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.run_id.as_uuid())
        .bind(fixture.step_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(
            step_status, "failed",
            "a step whose dispatch is still inside its deadline is not a failed step"
        );
    }
}
