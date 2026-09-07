//! Recovery after a lost embedding dispatch.
//!
//! Spec line 245: "Loss after `Dispatching` remains deadline-gated; one recovery
//! winner leaves the effect `Unknown` and finalizes the job
//! `InconclusiveUnknown`. Neither workers nor recovery automatically retry a job
//! after `Dispatching`."
//!
//! The property that matters is that recovery *decides what already happened*
//! and never makes something happen again. This suite proves that against the
//! database rather than against a counter in a double: a second provider call
//! would need a second effect and a second admission, and those are rows.

use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::{fs, path::Path, sync::Arc};
use tempfile::TempDir;
use uuid::Uuid;
use vestrace_application::{
    AcceptDeliveryOutputs, AcceptEmbeddingJob, ApplicationError, ConfiguredCapabilityPolicyEngine,
    DeliveryOutputIdentity, EmbeddingJobAttemptRecovery, EmbeddingJobRepository,
    EmbeddingJobTerminationService, EmbeddingOutputKeyProgress, EmbeddingOutputKeyRepository,
    EmbeddingOutputKeyService, ExternalEffectRepository, IdempotencyRecord, OutboxMessage,
    PolicyDecisionEngine, PreDispatchTerminalState, PreDispatchTerminationEvidence,
    ProviderDispatchRepository, RequestContext, RequestEmbeddingOutputRetirement,
    TerminateEmbeddingJobPreDispatch,
};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, ContentMaterialId, EmbeddingJobId,
    EmbeddingSpaceId, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId,
    ModelRequestEvidenceId, PolicyDecision, PolicyDecisionReason, PolicyDecisionResult,
    PolicyInputState, PrincipalId, ResourceScope, RiskCategory, WorkspaceId,
    embedding::EmbeddingJobKind,
    id::{AuditEventId, OutboxId, PolicyDecisionId},
};
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
use vestrace_infrastructure::{
    PgEmbeddingJobRepository, PgEmbeddingOutputKeyRepository, PgExternalEffectRepository, PgStore,
};

#[path = "common/mod.rs"]
mod common;

use common::*;

const OUTPUT_BOOTSTRAP_KEY_ID: &str = "recovery-output-key-bootstrap";
const OUTPUT_BOOTSTRAP_SCOPE: &str = "recovery-output-key-bootstrap";
const OUTPUT_BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

/// Keeps the same mounted host-vault shape as the delivery-output service. The
/// recovery fixture uses it only to obtain the real provisional receipts that
/// the existing pre-dispatch gate requires; it does not manufacture receipt
/// rows through an owner connection.
struct OutputVaultFixture {
    bootstrap: TempDir,
    vault: TempDir,
    reference: KeyReference,
}

impl OutputVaultFixture {
    fn new() -> Self {
        let bootstrap = TempDir::new().expect("recovery output bootstrap");
        write_output_bootstrap(bootstrap.path());
        Self {
            vault: TempDir::new().expect("recovery output vault"),
            reference: KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                OUTPUT_BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                OUTPUT_BOOTSTRAP_SCOPE,
                OUTPUT_BOOTSTRAP_ALGORITHM,
            )
            .expect("recovery output bootstrap reference"),
            bootstrap,
        }
    }

    fn vault_for_workspace(&self, workspace_id: WorkspaceId) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault.path(),
            self.bootstrap.path(),
            self.reference.clone(),
            SecretResolutionRequest::new(
                workspace_id,
                OUTPUT_BOOTSTRAP_SCOPE,
                "test://embedding-effect-recovery-output",
            ),
        )
        .expect("recovery output host vault")
    }
}

fn write_output_bootstrap(root: &Path) {
    let key = root.join(OUTPUT_BOOTSTRAP_KEY_ID);
    let version = key.join("v1");
    fs::create_dir_all(&version).expect("recovery output key directory");
    fs::write(key.join("scope"), OUTPUT_BOOTSTRAP_SCOPE).expect("recovery output scope");
    fs::write(key.join("purpose"), "storage").expect("recovery output purpose");
    fs::write(key.join("algorithm"), OUTPUT_BOOTSTRAP_ALGORITHM)
        .expect("recovery output algorithm");
    fs::write(version.join("state"), "active").expect("recovery output state");
    fs::write(version.join("private.pkcs8"), [0x5A; 32]).expect("recovery output key");
}

fn delivery_outputs(count: usize) -> Vec<DeliveryOutputIdentity> {
    (0..count)
        .map(|ordinal| DeliveryOutputIdentity {
            output_ordinal: ordinal as u64,
            intent_id: MaterialKeyCreationIntentId::new(),
            material_id: ContentMaterialId::new(),
            key_id: MaterialKeyId::new(),
            nonce: IntentNonce::new(),
        })
        .collect()
}

fn delivery_output_acceptance(
    fixture: &AcceptedJob,
    receipt_id: Uuid,
    outputs: Vec<DeliveryOutputIdentity>,
) -> AcceptDeliveryOutputs {
    let at = chrono::DateTime::<Utc>::from_timestamp(1_800_000_000, 0)
        .expect("fixed delivery acceptance timestamp");
    AcceptDeliveryOutputs {
        receipt_id,
        idempotency_key: format!("recovery-delivery-output-{receipt_id}"),
        acceptance: AcceptEmbeddingJob {
            job_id: fixture.job_id,
            space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
            kind: EmbeddingJobKind::Delivery,
            model_binding_snapshot_id: fixture.snapshot_id,
            intent: fixture.intent.clone(),
            model_request_evidence_id: ModelRequestEvidenceId::from_uuid(fixture.evidence_id),
            retries_unknown_embedding_job_id: None,
            expected_predecessor_version: None,
            idempotency: Some(IdempotencyRecord {
                idempotency_key: format!("recovery-delivery-output-{receipt_id}"),
                workspace_id: fixture.context.workspace_id,
                request_hash: format!("recovery-delivery-output:{receipt_id}"),
                response_payload: None,
                status: "completed".to_owned(),
                created_at: at,
                expires_at: at + Duration::hours(24),
            }),
            outbox: vec![OutboxMessage {
                id: OutboxId::from_uuid(receipt_id),
                workspace_id: fixture.context.workspace_id,
                topic: "embedding.job.delivery_accepted".to_owned(),
                payload: serde_json::json!({"receipt_id": receipt_id}),
                created_at: at,
                attempts: 0,
            }],
            audit: AuditEvent::new(
                AuditEventId::from_uuid(receipt_id),
                fixture.context.workspace_id,
                fixture.context.principal_id,
                "embedding.job.delivery_accepted",
                "embedding_job",
                fixture.job_id.as_uuid(),
                serde_json::json!({"receipt_id": receipt_id}),
                at,
            )
            .expect("delivery output acceptance audit"),
        },
        outputs,
    }
}

async fn prepare_complete_delivery_outputs(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &AcceptedJob,
    vault: &OutputVaultFixture,
) {
    let outputs = delivery_outputs(2);
    let repository = PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()));
    repository
        .accept_delivery_outputs(
            &fixture.context,
            delivery_output_acceptance(fixture, Uuid::now_v7(), outputs.clone()),
        )
        .await
        .expect("the recovery fixture must accept its exact output set");

    let service = EmbeddingOutputKeyService::new(
        Arc::new(repository),
        Arc::new(vault.vault_for_workspace(fixture.context.workspace_id)),
    );
    assert_eq!(
        service
            .reconcile_one(&fixture.context)
            .await
            .expect("the first recovery output reconciliation must persist one receipt"),
        Some(EmbeddingOutputKeyProgress::WaitingForResultKeys),
        "one output receipt cannot make a two-output delivery dispatchable"
    );
    assert!(matches!(
        service
            .reconcile_one(&fixture.context)
            .await
            .expect("the second recovery output reconciliation must complete the set"),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));
    let receipt_identities: Vec<(i64, Uuid)> = sqlx::query_as(
        "SELECT output_ordinal,intent_id FROM embedding_output_key_receipts \
          WHERE workspace_id=$1 AND job_id=$2 ORDER BY output_ordinal,intent_id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_all(owner)
    .await
    .expect("the owner observer may read immutable output receipts");
    let expected_identities: Vec<(i64, Uuid)> = outputs
        .iter()
        .map(|output| {
            (
                i64::try_from(output.output_ordinal).expect("output ordinal"),
                output.intent_id.as_uuid(),
            )
        })
        .collect();
    assert_eq!(
        receipt_identities, expected_identities,
        "both exact accepted outputs must be prepared and receipted before dispatch"
    );
}

async fn live_source(runtime: &PgPool, fixture: &AcceptedJob) -> ContentMaterialId {
    let material_id = ContentMaterialId::new();
    let intent_id = MaterialKeyCreationIntentId::new();
    let key_id = MaterialKeyId::new();
    let mut framed_ciphertext = vec![0x51_u8; 4096];
    framed_ciphertext[..5].copy_from_slice(b"VMRF\x01");
    let mut transaction = runtime.begin().await.expect("live source transaction");
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .expect("scope live source");
    sqlx::query(
        "SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)",
    )
    .bind(intent_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(material_id.as_uuid())
    .bind(key_id.as_uuid())
    .bind(Uuid::now_v7())
    .bind(fixture.job_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .expect("reserve live source intent");
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .expect("create live source key");
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .expect("receipt live source key");
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(framed_ciphertext)
        .execute(&mut *transaction)
        .await
        .expect("prepare live source material");
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .expect("bind live source key");
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .expect("finalize live source material");
    transaction
        .commit()
        .await
        .expect("commit live source material");
    material_id
}

async fn attach_live_source_evidence(
    owner: &PgPool,
    fixture: &AcceptedJob,
    source: ContentMaterialId,
) {
    let mut evidence = owner.begin().await.expect("source evidence transaction");
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *evidence)
        .await
        .expect("guarded source evidence role");
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *evidence)
        .await
        .expect("scope source evidence");
    sqlx::query(
        "INSERT INTO model_request_evidence_nodes(\
         id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) \
         VALUES($1,$2,$3,8,'governed_input_material',$4)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.evidence_id)
    .bind(source.as_uuid())
    .execute(&mut *evidence)
    .await
    .expect("attach live source evidence node");
    evidence
        .commit()
        .await
        .expect("commit live source evidence");
}

/// Task 14C accepts the delivery job and its exact output identities in one
/// guarded command. Output keys must therefore be accepted and reconciled from
/// the pre-acceptance boundary, never backfilled into an existing job.
async fn accepted_with_receipted_outputs(pool: &PgPool, runtime: &PgPool) -> AcceptedJob {
    accepted_with_receipted_outputs_and_vault(pool, runtime)
        .await
        .0
}

async fn accepted_with_receipted_outputs_and_vault(
    pool: &PgPool,
    runtime: &PgPool,
) -> (AcceptedJob, OutputVaultFixture) {
    let fixture = prepare_delivery_embedding_job(pool, runtime).await;
    let source = live_source(runtime, &fixture).await;
    make_dispatchable(pool, runtime, &fixture).await;
    attach_live_source_evidence(pool, &fixture, source).await;
    let vault = OutputVaultFixture::new();
    prepare_complete_delivery_outputs(pool, runtime, &fixture, &vault).await;
    (fixture, vault)
}

fn cancellation_termination(
    context: &RequestContext,
    fixture: &AcceptedJob,
    expected_version: u64,
    receipt_id: Uuid,
    idempotency_key: &str,
) -> TerminateEmbeddingJobPreDispatch {
    let request = AuthorizationRequest::new(
        Capability::ExecutionWrite,
        "embedding.job.cancel",
        ResourceScope::workspace().to_string(),
        RiskCategory::Low,
    );
    TerminateEmbeddingJobPreDispatch {
        receipt_id,
        job_id: fixture.job_id,
        expected_version,
        idempotency_key: idempotency_key.to_owned(),
        terminal_state: PreDispatchTerminalState::Cancelled,
        evidence: PreDispatchTerminationEvidence::CancellationAuthorization(Box::new(
            PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "embedding-cancellation-v1".to_owned(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: Capability::ExecutionWrite,
                operation: "embedding.job.cancel".to_owned(),
                resource_scope: ResourceScope::workspace().to_string(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &request,
                ),
                matched_grant_id: None,
                decided_at: Utc::now(),
            },
        )),
    }
}

async fn retire_receipted_outputs(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    vault: &OutputVaultFixture,
    termination: TerminateEmbeddingJobPreDispatch,
) {
    let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
        runtime.clone(),
    )));
    let service = EmbeddingOutputKeyService::new(
        repository,
        Arc::new(vault.vault_for_workspace(fixture.context.workspace_id)),
    );
    service
        .request_retirement(
            &fixture.context,
            RequestEmbeddingOutputRetirement {
                termination: termination.clone(),
            },
        )
        .await
        .expect("the terminal authority must request retirement for every exact output");
    for _ in 0..2 {
        assert!(matches!(
            service
                .reconcile_one(&fixture.context)
                .await
                .expect("the output key worker must record its retirement receipt"),
            Some(EmbeddingOutputKeyProgress::Retired { .. })
        ));
    }
}

/// Everything a second provider call would have to leave behind.
///
/// One effect, one admission, one concurrency lease, one `dispatching`
/// transition. Recovery may add an `unknown` outcome; it may not add any of
/// these.
async fn dispatch_footprint(pool: &PgPool, fixture: &AcceptedJob) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM provider_concurrency_leases WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
              WHERE effect_id=$2 AND status='dispatching')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Brings one job to a committed `Dispatching` with no receipt, which is the
/// state a worker that died mid-call leaves behind.
async fn dispatched(pool: &PgPool, runtime: &PgPool) -> AcceptedJob {
    let fixture = accepted_with_receipted_outputs(pool, runtime).await;
    dispatching_repository(runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await
        .expect("the fixture must reach a committed Dispatching");
    fixture
}

async fn configured_cancellation_service(runtime: &PgPool) -> EmbeddingJobTerminationService {
    let repository: vestrace_application::SharedEmbeddingJobRepository = Arc::new(
        PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone())),
    );
    let policy = Arc::new(
        ConfiguredCapabilityPolicyEngine::new(
            "embedding-cancellation-v1",
            [Capability::ExecutionWrite],
            RiskCategory::Low,
        )
        .unwrap(),
    );
    EmbeddingJobTerminationService::new(repository, policy)
}

async fn admit_embedding_fixture(runtime: &PgPool, fixture: &AcceptedJob, wait_id: Uuid) -> String {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let decision = sqlx::query_scalar::<_, String>(
        "SELECT decision FROM vestrace_try_admit_provider_dispatch(\
          $1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)",
    )
    .bind(Uuid::now_v7())
    .bind(wait_id)
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.evidence_id)
    .bind(fixture.snapshot_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    decision
}

async fn administratively_mark_embedding_job_running(pool: &PgPool, fixture: &AcceptedJob) {
    // There is no Task 14B worker executor.  This fixture supplies the existing
    // Running state under the guarded owner; the operation under test remains
    // the public runtime cancellation producer.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE embedding_jobs AS job SET state='running',version=job.version+1 \
          WHERE job.workspace_id=$1 AND job.id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();
}

/// The shared effect repository writes `Dispatching` directly.  That writer is
/// deliberately not an embedding repository, so an embedding-only cancellation
/// fence must live at the durable lifecycle boundary itself.  The legacy schema
/// accepted this exact committed contradiction: the job said Cancelled while the
/// effect newly said Dispatching.  Keeping the adversarial writer as raw runtime
/// SQL prevents the regression test from accidentally exercising only a friendly
/// wrapper that learnt about cancellation later.
#[sqlx::test(migrations = "../../migrations")]
async fn a_cancelled_embedding_job_refuses_the_direct_lifecycle_dispatch_writer(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    make_dispatchable(&pool, &runtime, &fixture).await;

    let policy_revision_id: Uuid = sqlx::query_scalar(
        "SELECT current_policy_revision_id FROM connection_admission_policy_heads \
          WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let expires_at = Utc::now() + Duration::minutes(5);
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query("UPDATE embedding_jobs SET state='cancelled', version=version+1 WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connection_dispatch_admissions(\
           id,workspace_id,connection_id,connection_revision_id,policy_revision_id,\
           external_effect_id,model_binding_snapshot_id,decision,admitted_at,\
           requested_wait_id,requested_concurrency_lease_id,requested_dispatch_ttl_seconds,\
           retry_after_seconds,dispatch_expires_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,'admitted',NOW(),$8,$9,60,NULL,$10)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(policy_revision_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.snapshot_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(expires_at)
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO provider_concurrency_leases(\
           id,workspace_id,connection_id,external_effect_id,slot_ordinal,issued_at,expires_at) \
         VALUES($1,$2,$3,$4,0,NOW(),$5)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.external_effect_id)
    .bind(expires_at)
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let mut direct_writer = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *direct_writer)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
           effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,$5)",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id.to_string())
    .bind(Uuid::now_v7())
    .bind(expires_at)
    .execute(&mut *direct_writer)
    .await;
    // A successful statement is not yet the forbidden fact.  Commit it, then
    // use the owner pool below as an independent persisted-state observer.  The
    // pre-0192 schema reaches that branch and makes this test RED with a count
    // of one; the 0192 BEFORE trigger instead takes the refusal branch.
    match result {
        Ok(_) => direct_writer
            .commit()
            .await
            .expect("the legacy direct lifecycle write must be committed for the RED observation"),
        Err(error) => {
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("23514"),
                "the durable embedding terminal fence must be a check refusal"
            );
            assert_eq!(
                error.as_database_error().map(|database| database.message()),
                Some("embedding job is terminal before provider dispatch"),
            );
            direct_writer.rollback().await.unwrap();
        }
    }

    let dispatches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
          WHERE workspace_id=$1 AND effect_id=$2 AND status='dispatching'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatches, 0,
        "the refused writer must leave no dispatch evidence"
    );
    runtime.close().await;
}

/// The legacy lifecycle contract validated an admitted expiry but did not make
/// embedding history immutable.  A runtime UPDATE from Prepared to Dispatching
/// therefore bypassed the INSERT-only Task 14B fence after a lawful
/// cancellation.  Commit the raw writer before observing it from the owner
/// pool: this is a semantic RED against the old migration, not an uncommitted
/// statement error.
#[sqlx::test(migrations = "../../migrations")]
async fn a_cancelled_embedding_job_refuses_lifecycle_update_to_dispatching(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let (fixture, vault) = accepted_with_receipted_outputs_and_vault(&pool, &runtime).await;
    assert_eq!(
        admit_embedding_fixture(&runtime, &fixture, Uuid::now_v7()).await,
        "admitted"
    );
    let termination = cancellation_termination(
        &fixture.context,
        &fixture,
        1,
        Uuid::now_v7(),
        "cancel-before-lifecycle-update",
    );
    retire_receipted_outputs(&runtime, &fixture, &vault, termination.clone()).await;
    PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()))
        .terminate_pre_dispatch(fixture.context.clone(), termination)
        .await
        .expect("the real output retirement receipts authorize cancellation");
    let expiry: chrono::DateTime<Utc> = sqlx::query_scalar(
        "SELECT dispatch_expires_at FROM connection_dispatch_admissions \
          WHERE workspace_id=$1 AND external_effect_id=$2 AND decision='admitted'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut writer = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    let update = sqlx::query(
        "UPDATE external_effect_lifecycle_transitions AS lifecycle \
            SET status='dispatching',cause='dispatch_started',cause_ref=$3, \
                dispatch_owner=$4,dispatch_expires_at=$5 \
          WHERE lifecycle.workspace_id=$1 AND lifecycle.effect_id=$2 AND lifecycle.status='prepared'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.external_effect_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(expiry)
    .execute(&mut *writer)
    .await;
    match update {
        Ok(result) => {
            assert_eq!(result.rows_affected(), 1);
            writer
                .commit()
                .await
                .expect("the old UPDATE bypass must be committed for its RED observation");
        }
        Err(error) => {
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("23514")
            );
            writer.rollback().await.unwrap();
        }
    }
    let dispatches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
          WHERE workspace_id=$1 AND effect_id=$2 AND status='dispatching'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatches, 0,
        "a terminal embedding job owns immutable lifecycle history"
    );
    runtime.close().await;
}

/// Deleting a durable Dispatching transition used to erase the only evidence
/// that blocks cancellation.  `dispatched` reaches the fact through the real
/// shared admission and lifecycle path; the only adversarial operation here is
/// the raw runtime DELETE that must leave its persisted evidence intact.
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_embedding_dispatch_history_does_not_restore_cancellation(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;

    let mut eraser = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *eraser)
        .await
        .unwrap();
    let deletion = sqlx::query(
        "DELETE FROM external_effect_lifecycle_transitions \
          WHERE workspace_id=$1 AND effect_id=$2 AND status='dispatching'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .execute(&mut *eraser)
    .await;
    match deletion {
        Ok(result) => {
            assert_eq!(result.rows_affected(), 1);
            eraser
                .commit()
                .await
                .expect("the old DELETE bypass must commit for its RED observation");
        }
        Err(error) => {
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("23514")
            );
            eraser.rollback().await.unwrap();
        }
    }
    let dispatches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
          WHERE workspace_id=$1 AND effect_id=$2 AND status='dispatching'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatches, 1,
        "the durable Dispatching fact cannot be erased"
    );

    let service = configured_cancellation_service(&runtime).await;
    let cancellation = service
        .cancel(
            fixture.context.clone(),
            fixture.job_id,
            1,
            "cancel-after-history-erasure".to_owned(),
        )
        .await;
    assert!(
        cancellation.is_err(),
        "erasing a Dispatching fact must not restore cancellation authority"
    );
    runtime.close().await;
}

/// A configured policy decision is reissued on every retry, so the decision id
/// changes.  The durable command key must still converge on the first terminal
/// receipt and audit exactly once; only that receipt makes the recovery and
/// dispatch fences lawful.
#[sqlx::test(migrations = "../../migrations")]
async fn configured_cancellation_converges_on_one_terminal_receipt_and_blocks_dispatch(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let repository: vestrace_application::SharedEmbeddingJobRepository = Arc::new(
        PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone())),
    );
    let policy = Arc::new(
        ConfiguredCapabilityPolicyEngine::new(
            "embedding-cancellation-v1",
            [Capability::ExecutionWrite],
            RiskCategory::Low,
        )
        .unwrap(),
    );
    let service = EmbeddingJobTerminationService::new(repository, policy);

    let first = service
        .cancel(
            fixture.context.clone(),
            fixture.job_id,
            1,
            "cancel-converges-once".to_owned(),
        )
        .await
        .expect("the configured execution.write policy may cancel a requested job");
    let replay = service
        .cancel(
            fixture.context.clone(),
            fixture.job_id,
            1,
            "cancel-converges-once".to_owned(),
        )
        .await
        .expect("a fresh allowed decision must converge on the original receipt");
    assert_eq!(
        replay, first,
        "the receipt, including its first decision id, is canonical"
    );

    // Exercise the shared admission authority after cancellation.  The fixture
    // adds the ordinary policy/evidence prerequisites only now, so the fence
    // rather than a missing prerequisite is what refuses the attempt.
    make_dispatchable(&pool, &runtime, &fixture).await;
    let mut attempted_admission = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *attempted_admission)
        .await
        .unwrap();
    let admission = sqlx::query_scalar::<_, String>(
        "SELECT decision FROM vestrace_try_admit_provider_dispatch(\
          $1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.evidence_id)
    .bind(fixture.snapshot_id)
    .fetch_one(&mut *attempted_admission)
    .await;
    let refusal = admission.expect_err("cancellation must win before shared admission persists");
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
        Some("embedding job is terminal before provider dispatch")
    );
    attempted_admission.rollback().await.unwrap();

    let state: (String, i64) =
        sqlx::query_as("SELECT state,version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("cancelled".to_owned(), 2));
    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2), \
           (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.cancelled'), \
           (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE workspace_id=$1 AND external_effect_id=$3), \
           (SELECT COUNT(*) FROM external_effect_lifecycle_transitions WHERE workspace_id=$1 AND effect_id=$3 AND status='dispatching')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        evidence,
        (1, 1, 0, 0),
        "replay and refused admission must leave no dispatch evidence"
    );

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("a terminal receipt is an explicit recovery outcome");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::Cancelled);
    runtime.close().await;
}

/// A Running job is still pre-dispatch when it has not accumulated provider
/// evidence.  Cancellation therefore owns its active admission and lease as
/// well as the requested case; a stale command, a reused key with different
/// semantics, and a different principal cannot manufacture another receipt.
#[sqlx::test(migrations = "../../migrations")]
async fn running_cancellation_releases_its_admission_and_conflicting_commands_refuse(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let (fixture, vault) = accepted_with_receipted_outputs_and_vault(&pool, &runtime).await;
    administratively_mark_embedding_job_running(&pool, &fixture).await;
    assert_eq!(
        admit_embedding_fixture(&runtime, &fixture, Uuid::now_v7()).await,
        "admitted",
        "the cancellation case must own a real, still-live shared admission"
    );

    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let termination = cancellation_termination(
        &fixture.context,
        &fixture,
        2,
        Uuid::now_v7(),
        "running-cancellation",
    );
    retire_receipted_outputs(&runtime, &fixture, &vault, termination.clone()).await;
    let receipt = repository
        .terminate_pre_dispatch(fixture.context.clone(), termination)
        .await
        .expect("an undispatched Running job may be cancelled");
    assert_eq!(receipt.terminal_state, PreDispatchTerminalState::Cancelled);
    assert_eq!(receipt.version, 3);

    let same_key_semantic_mismatch = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            cancellation_termination(
                &fixture.context,
                &fixture,
                3,
                Uuid::now_v7(),
                "running-cancellation",
            ),
        )
        .await
        .expect_err("reusing a key with a different expected version is a conflict");
    assert!(matches!(
        same_key_semantic_mismatch,
        ApplicationError::Conflict(ref code) if code == "EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_CONFLICT"
    ));
    let stale_different_key = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            cancellation_termination(
                &fixture.context,
                &fixture,
                2,
                Uuid::now_v7(),
                "running-cancellation-stale",
            ),
        )
        .await
        .expect_err("a different stale command cannot overwrite the terminal receipt");
    assert!(matches!(
        stale_different_key,
        ApplicationError::Conflict(ref code) if code == "EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_CONFLICT"
    ));
    let other_principal = RequestContext::new(fixture.context.workspace_id, PrincipalId::new());
    let principal_conflict = repository
        .terminate_pre_dispatch(
            other_principal.clone(),
            cancellation_termination(
                &other_principal,
                &fixture,
                2,
                Uuid::now_v7(),
                "running-cancellation",
            ),
        )
        .await
        .expect_err("only the principal differs from the canonical command");
    assert!(
        matches!(
            principal_conflict,
            ApplicationError::Conflict(ref code) if code == "EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_CONFLICT"
        ),
        "the cross-principal command must retain its terminal conflict meaning: {principal_conflict:?}"
    );

    let state: (String, i64) =
        sqlx::query_as("SELECT state,version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("cancelled".to_owned(), 3));
    let cleanup: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2), \
           (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.cancelled'), \
           (SELECT COUNT(*) FROM provider_concurrency_leases WHERE workspace_id=$1 AND external_effect_id=$3 AND released_at IS NULL), \
           (SELECT COUNT(*) FROM connection_admission_states AS state \
             WHERE state.workspace_id=$1 AND state.connection_id=$4 AND state.active_lease_count <> 0)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cleanup,
        (1, 1, 0, 0),
        "the canonical receipt releases the live lease and its connection accounting exactly once"
    );
    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("the exact terminal receipt permits an explicit recovery outcome");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::Cancelled);
    runtime.close().await;
}

/// Terminal job state alone is never a recovery authority.  This legacy
/// adversarial fixture writes Cancelled as the guarded owner without the new
/// receipt; the recovery gate and repository must refuse that contradiction.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_refuses_a_terminal_embedding_job_without_its_exact_receipt(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE embedding_jobs AS job SET state='cancelled',version=job.version+1 \
          WHERE job.workspace_id=$1 AND job.id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect_err(
            "terminal state without the exact effect/version receipt is not recovery proof",
        );
    assert!(
        matches!(recovery, ApplicationError::Conflict(_)),
        "unexpected recovery refusal mapping: {recovery:?}"
    );
    runtime.close().await;
}

/// A durable denied effect authorization is sufficient to prove the provider
/// call cannot have begun.  Failure takes that exact authorization identity,
/// produces one terminal receipt and turns recovery into a terminal outcome.
#[sqlx::test(migrations = "../../migrations")]
async fn denied_effect_authorization_finalizes_failed_definite_once(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let deny_policy = ConfiguredCapabilityPolicyEngine::new(
        "embedding-definite-failure-v1",
        std::iter::empty::<Capability>(),
        RiskCategory::High,
    )
    .unwrap();
    let authorization_request = AuthorizationRequest::new(
        fixture.intent.required_capability(),
        fixture.intent.operation().to_owned(),
        fixture.intent.target().to_owned(),
        fixture.intent.risk(),
    );
    let denial = deny_policy
        .decide(&fixture.context, authorization_request)
        .await
        .expect("the configured deny engine returns a durable decision");
    assert!(!denial.is_allowed());
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .record_authorization(&fixture.context, fixture.intent.id(), &denial)
        .await
        .expect("the exact denied authorization is durable evidence");

    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let unknown_source = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-denial-unknown-source".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                    authorization_id: Uuid::now_v7(),
                },
            },
        )
        .await
        .expect_err("a caller-supplied authorization id is not durable denial evidence");
    assert!(matches!(unknown_source, ApplicationError::Policy(_)));
    let cross_workspace = repository
        .terminate_pre_dispatch(
            RequestContext::new(WorkspaceId::new(), PrincipalId::new()),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-denial-cross-workspace".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                    authorization_id: denial.id.as_uuid(),
                },
            },
        )
        .await
        .expect_err("a foreign workspace cannot address the embedding job");
    assert!(matches!(cross_workspace, ApplicationError::Policy(_)));
    let before_terminal: (String, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before_terminal, ("requested".to_owned(), 1, 0));
    let receipt = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-denial".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                    authorization_id: denial.id.as_uuid(),
                },
            },
        )
        .await
        .expect("the exact durable denial finalizes pre-dispatch failure");
    assert_eq!(
        receipt.terminal_state,
        PreDispatchTerminalState::FailedDefinite
    );
    assert_eq!(receipt.evidence_id, denial.id.as_uuid());
    let facts: (String, i64, String, i64) = sqlx::query_as(
        "SELECT \
           (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT evidence_kind FROM embedding_job_termination_receipts WHERE id=$3), \
           (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.failed_definite')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(receipt.receipt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        facts,
        (
            "failed_definite".to_owned(),
            2,
            "external_effect_denied".to_owned(),
            1
        )
    );
    let replay = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-denial".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                    authorization_id: denial.id.as_uuid(),
                },
            },
        )
        .await
        .expect("the same durable denial and command key converge on the receipt");
    assert_eq!(replay, receipt);
    let second_denial = deny_policy
        .decide(
            &fixture.context,
            AuthorizationRequest::new(
                fixture.intent.required_capability(),
                fixture.intent.operation().to_owned(),
                fixture.intent.target().to_owned(),
                fixture.intent.risk(),
            ),
        )
        .await
        .unwrap();
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .record_authorization(&fixture.context, fixture.intent.id(), &second_denial)
        .await
        .unwrap();
    let source_conflict = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-denial".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                    authorization_id: second_denial.id.as_uuid(),
                },
            },
        )
        .await
        .expect_err("a different durable source cannot reuse the command key");
    assert!(matches!(
        source_conflict,
        ApplicationError::Conflict(ref code) if code == "EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_CONFLICT"
    ));
    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("definite failure has an explicit terminal recovery outcome");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::FailedDefinite);
    runtime.close().await;
}

/// The wait is created by the shared admission authority, then administratively
/// aged without sleeping.  The termination function validates the exact
/// workspace/effect/connection/deadline witness and closes it as Timeout in
/// the same transaction as the FailedDefinite receipt.
#[sqlx::test(migrations = "../../migrations")]
async fn expired_admission_wait_finalizes_failed_definite_and_closes_exact_wait(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let (fixture, vault) = accepted_with_receipted_outputs_and_vault(&pool, &runtime).await;

    let mut policy = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *policy)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_connection_admission_policy($1,$2,$3,1::BIGINT,1::SMALLINT,1,30,900)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .fetch_one(&mut *policy)
    .await
    .unwrap();
    policy.commit().await.unwrap();

    // The policy itself is legal (one request per minute).  Occupy its one
    // slot with a distinct, valid fixture effect and lease, then let the
    // shared runtime function create the target wait without a wall-clock
    // sleep.  Only the target wait is the definite-failure witness.
    let occupancy_intent = workspace_scoped_intent(&fixture.context, fixture.snapshot_id);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&fixture.context, &occupancy_intent)
        .await
        .unwrap();
    let mut occupancy = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *occupancy)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *occupancy)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO provider_concurrency_leases(\
           id,workspace_id,connection_id,external_effect_id,slot_ordinal,issued_at,expires_at) \
         VALUES($1,$2,$3,$4,0,clock_timestamp(),clock_timestamp()+INTERVAL '5 minutes')",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(occupancy_intent.id().as_uuid())
    .execute(&mut *occupancy)
    .await
    .unwrap();
    occupancy.commit().await.unwrap();

    let wait_id = Uuid::now_v7();
    assert_eq!(
        admit_embedding_fixture(&runtime, &fixture, wait_id).await,
        "conflict",
        "the shared policy creates the durable queue wait"
    );
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let before_deadline = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-expired-wait-before-deadline".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::AdmissionTimeout { wait_id },
            },
        )
        .await
        .expect_err("an unexpired wait is not failure evidence");
    assert!(matches!(before_deadline, ApplicationError::Policy(_)));
    let wrong_wait = repository
        .terminate_pre_dispatch(
            fixture.context.clone(),
            TerminateEmbeddingJobPreDispatch {
                receipt_id: Uuid::now_v7(),
                job_id: fixture.job_id,
                expected_version: 1,
                idempotency_key: "definite-expired-wait-wrong-witness".to_owned(),
                terminal_state: PreDispatchTerminalState::FailedDefinite,
                evidence: PreDispatchTerminationEvidence::AdmissionTimeout {
                    wait_id: Uuid::now_v7(),
                },
            },
        )
        .await
        .expect_err("a wait id from no exact effect/connection witness is refused");
    assert!(matches!(wrong_wait, ApplicationError::Policy(_)));
    let before_age: (String, i64, Option<String>) = sqlx::query_as(
        "SELECT \
           (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT terminal_reason FROM provider_admission_waits WHERE workspace_id=$1 AND id=$3)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(wait_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before_age, ("requested".to_owned(), 1, None));
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE provider_admission_waits AS wait \
            SET deadline_at=clock_timestamp()-INTERVAL '1 second' \
          WHERE wait.workspace_id=$1 AND wait.id=$2 AND wait.external_effect_id=$3 \
            AND wait.terminal_reason IS NULL",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(wait_id)
    .bind(fixture.external_effect_id)
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let termination = TerminateEmbeddingJobPreDispatch {
        receipt_id: Uuid::now_v7(),
        job_id: fixture.job_id,
        expected_version: 1,
        idempotency_key: "definite-expired-wait".to_owned(),
        terminal_state: PreDispatchTerminalState::FailedDefinite,
        evidence: PreDispatchTerminationEvidence::AdmissionTimeout { wait_id },
    };
    retire_receipted_outputs(&runtime, &fixture, &vault, termination.clone()).await;
    let receipt = repository
        .terminate_pre_dispatch(fixture.context.clone(), termination)
        .await
        .expect("the exact expired shared wait finalizes definite failure");
    let facts: (String, String, Option<chrono::DateTime<Utc>>, i64) = sqlx::query_as(
        "SELECT \
           (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
           (SELECT terminal_reason FROM provider_admission_waits WHERE workspace_id=$1 AND id=$3), \
           (SELECT terminal_at FROM provider_admission_waits WHERE workspace_id=$1 AND id=$3), \
           (SELECT COUNT(*) FROM provider_concurrency_leases WHERE workspace_id=$1 AND external_effect_id=$4 AND released_at IS NULL)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(wait_id)
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts.0, "failed_definite");
    assert_eq!(facts.1, "timeout");
    assert!(facts.2.is_some(), "the timeout closure is durable");
    assert_eq!(facts.3, 0);
    assert_eq!(receipt.evidence_id, wait_id);
    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("timeout failure has an explicit terminal recovery outcome");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::FailedDefinite);
    runtime.close().await;
}

fn acknowledgement_command(
    fixture: &AcceptedJob,
    successor_job_id: EmbeddingJobId,
    successor_mre_id: ModelRequestEvidenceId,
    expected_predecessor_version: u64,
    idempotency_key: &str,
) -> AcceptEmbeddingJob {
    let at = Utc::now();
    AcceptEmbeddingJob {
        job_id: successor_job_id,
        space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
        kind: EmbeddingJobKind::Delivery,
        model_binding_snapshot_id: fixture.snapshot_id,
        intent: embedding_intent(fixture, successor_job_id),
        model_request_evidence_id: successor_mre_id,
        retries_unknown_embedding_job_id: Some(fixture.job_id),
        expected_predecessor_version: Some(expected_predecessor_version),
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.to_owned(),
            workspace_id: fixture.context.workspace_id,
            request_hash: format!(
                "{}:{}:{}:{}:{}:{}",
                fixture.job_id,
                expected_predecessor_version,
                successor_job_id,
                successor_mre_id,
                fixture.space_registration_id,
                fixture.snapshot_id,
            ),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + Duration::hours(24),
        }),
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "embedding.job.unknown_acknowledged",
            "embedding_job",
            successor_job_id.as_uuid(),
            serde_json::json!({"predecessor_embedding_job_id": fixture.job_id}),
            at,
        )
        .unwrap(),
    }
}

async fn recover_unknown(owner: &PgPool, runtime: &PgPool, fixture: &AcceptedJob) {
    let recovery = dispatch_repository(runtime)
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AdoptedUnknown);
    let terminal: (String, i64, bool) = sqlx::query_as(
        "SELECT job.state,job.version,EXISTS( \
             SELECT 1 FROM embedding_job_termination_receipts receipt \
              WHERE receipt.workspace_id=job.workspace_id AND receipt.job_id=job.id \
         ) \
           FROM embedding_jobs job WHERE job.workspace_id=$1 AND job.id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(owner)
    .await
    .expect("unknown recovery terminal observation");
    assert_eq!(
        terminal,
        ("inconclusive_unknown".to_owned(), 3, false),
        "the dispatch-start Requested→Running bump and unknown finalization must leave the exact terminal version without a pre-dispatch termination receipt"
    );
}

/// A second successor identity races through the partial unique index. It is a
/// policy refusal, not an infrastructure failure: the operator may learn that
/// one successor already exists but must not see a storage-shaped 500.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_successor_identity_is_a_semantic_acceptance_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&pool, &runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));

    repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(
                &fixture,
                EmbeddingJobId::new(),
                ModelRequestEvidenceId::new(),
                3,
                "first",
            ),
        )
        .await
        .unwrap();
    let error = repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(
                &fixture,
                EmbeddingJobId::new(),
                ModelRequestEvidenceId::new(),
                3,
                "second",
            ),
        )
        .await
        .expect_err("the one-successor index must refuse a second identity");

    assert!(
        matches!(
            error,
            ApplicationError::Policy(ref message)
                if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "the partial unique index must retain its policy meaning: {error:?}"
    );
}

/// A duplicate-charge acknowledgement is a successor of the same embedding
/// operation, not a way to reinterpret a recovered Delivery as another job
/// kind. The guarded predecessor lookup must reject the mismatch before the
/// intent it was paired with can survive the transaction.
#[sqlx::test(migrations = "../../migrations")]
async fn a_successor_with_a_kind_different_from_its_predecessor_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&pool, &runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));
    let before: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut command = acknowledgement_command(
        &fixture,
        EmbeddingJobId::new(),
        ModelRequestEvidenceId::new(),
        3,
        "kind-mismatch",
    );
    command.kind = EmbeddingJobKind::RetrievalQuery;

    let result = repository
        .accept_governed(fixture.context.clone(), command)
        .await;
    assert!(
        matches!(
            result,
            Err(ApplicationError::Policy(ref message))
                if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "a successor must keep its predecessor's kind: {result:?}"
    );
    let after: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after, before,
        "a refused kind mismatch leaves no job or effect"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn acknowledgement_creates_one_fresh_successor_and_preserves_the_unknown_head(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&pool, &runtime, &fixture).await;
    let before: (String, i64, Uuid) =
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    let command = acknowledgement_command(&fixture, successor_id, successor_mre_id, 3, "ack");
    let successor_effect_id = command.intent.id().as_uuid();

    PgEmbeddingJobRepository::new(PgStore::from_pool(runtime))
        .accept_governed(fixture.context.clone(), command)
        .await
        .unwrap();

    let successor: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT retries_unknown_embedding_job_id, external_effect_id, model_request_evidence_id \
         FROM embedding_jobs WHERE id=$1",
    )
    .bind(successor_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(successor.0, fixture.job_id.as_uuid());
    assert_eq!(successor.1, successor_effect_id);
    assert_eq!(successor.2, successor_mre_id.as_uuid());
    assert_ne!(successor.1, before.2, "the successor owns a fresh effect");
    let after: (String, i64, Uuid) =
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after, before,
        "acknowledgement never mutates the predecessor"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_identical_acknowledgement_replay_is_refused_without_another_job_or_effect(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&pool, &runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(&fixture, successor_id, successor_mre_id, 3, "replay"),
        )
        .await
        .unwrap();

    let replay = repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(&fixture, successor_id, successor_mre_id, 3, "replay"),
        )
        .await
        .expect_err("a fresh effect id makes the matching request a guarded refusal");
    assert!(matches!(
        replay,
        ApplicationError::Policy(ref message) if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
    ));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (2, 2),
        "both calls leave one predecessor and one successor only"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn acknowledgement_refuses_nonterminal_heads_without_creating_a_successor_or_effect(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let requested = accept_embedding_job(&pool, &runtime).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let before_requested: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(requested.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        repository
            .accept_governed(
                requested.context.clone(),
                acknowledgement_command(
                    &requested,
                    EmbeddingJobId::new(),
                    ModelRequestEvidenceId::new(),
                    1,
                    "requested"
                ),
            )
            .await
            .is_err()
    );
    let after_requested: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(requested.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_requested, before_requested);

    let dispatching = dispatched(&pool, &runtime).await;
    let before_dispatching: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(dispatching.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        repository
            .accept_governed(
                dispatching.context.clone(),
                acknowledgement_command(
                    &dispatching,
                    EmbeddingJobId::new(),
                    ModelRequestEvidenceId::new(),
                    1,
                    "dispatching"
                ),
            )
            .await
            .is_err()
    );
    let after_dispatching: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(dispatching.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_dispatching, before_dispatching);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stale_version_cannot_converge_on_an_existing_successor(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&pool, &runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    let command = acknowledgement_command(&fixture, successor_id, successor_mre_id, 3, "current");
    let successor_effect_id = command.intent.id().as_uuid();
    repository
        .accept_governed(fixture.context.clone(), command)
        .await
        .unwrap();
    let before: ((String, i64, Uuid), (String, i64, Uuid)) = (
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(successor_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
    );

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let stale = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,$7,$8)",
    )
    .bind(successor_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(fixture.snapshot_id)
    .bind(successor_effect_id)
    .bind(successor_mre_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(4_i64)
    .fetch_one(&mut *transaction)
    .await;
    assert_eq!(
        stale
            .unwrap_err()
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    let after: ((String, i64, Uuid), (String, i64, Uuid)) = (
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(successor_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    assert_eq!(after, before);
}

/// Before the deadline there is nothing to decide.
///
/// Line 245 makes loss after `Dispatching` deadline-gated. A worker that
/// adopted an in-flight dispatch the moment it noticed one would be racing the
/// provider it cannot see.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_waits_for_the_dispatch_deadline(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("a live dispatch classifies rather than refusing");

    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AwaitDispatchDeadline);
    let state: String = sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        state, "running",
        "waiting for a deadline must preserve the exact dispatched Running job"
    );
}

/// Past the deadline the effect becomes `Unknown` and the job terminal, and
/// nothing else is created.
///
/// The recovery time is passed in rather than waited for: the executor's
/// time-dependent branches cannot both be driven end to end without waiting out
/// the dispatch TTL, which P03's qualification established. What is proved here
/// against the database is the deadline branch; the pre-deadline branch above is
/// proved the same way, and neither is proved by sleeping.
#[sqlx::test(migrations = "../../migrations")]
async fn past_the_deadline_one_winner_adopts_and_finalizes(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let before = dispatch_footprint(&pool, &fixture).await;
    assert_eq!(before, (1, 1, 1, 1), "one dispatch leaves one of each");

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .expect("a dispatch past its deadline is adoptable");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AdoptedUnknown);

    let (state, version): (String, i64) =
        sqlx::query_as("SELECT state, version FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (state.as_str(), version),
        ("inconclusive_unknown", 3),
        "the job is terminal and its version advanced"
    );
    let unknown: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='unknown'",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unknown, 1, "the effect carries exactly one unknown outcome");

    // The decisive assertion. A second provider call would need a second effect
    // and a second admission; recovery created neither.
    assert_eq!(
        dispatch_footprint(&pool, &fixture).await,
        before,
        "recovery created a new dispatch footprint"
    );
}

/// A crash between the effect outcome and the job outcome is recovered by
/// running recovery again, and by nothing else.
///
/// Line 245 requires the job to be finalized from the immutable effect outcome
/// without another adapter call. The finalizer is idempotent, so the second run
/// reports `AlreadyUnknown` and leaves the version where the first run put it.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_recovery_adds_nothing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);
    let recovered_at = Utc::now() + Duration::hours(1);

    repository
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, recovered_at)
        .await
        .unwrap();
    let after_first = dispatch_footprint(&pool, &fixture).await;

    let second = repository
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, recovered_at)
        .await
        .expect("a terminal job still classifies");
    assert_eq!(second, EmbeddingJobAttemptRecovery::AlreadyUnknown);

    let (version, unknowns): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT version FROM embedding_jobs WHERE id=$1), \
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
                  WHERE effect_id=$2 AND status='unknown')",
    )
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (version, unknowns),
        (3, 1),
        "a second recovery must append no second outcome"
    );
    assert_eq!(dispatch_footprint(&pool, &fixture).await, after_first);
}

/// No scheduler advances a terminal ambiguity head.
///
/// Line 257 makes the authorized acknowledgement the sole successor path, so
/// rediscovering the plan of an `InconclusiveUnknown` job must not present it as
/// something to dispatch again.
#[sqlx::test(migrations = "../../migrations")]
async fn a_terminal_unknown_job_is_never_presented_as_dispatchable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);
    repository
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .unwrap();

    let footprint = dispatch_footprint(&pool, &fixture).await;
    let second_dispatch = dispatching_repository(&runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await;
    assert!(
        second_dispatch.is_err(),
        "a terminal unknown job must not dispatch again"
    );
    assert_eq!(
        dispatch_footprint(&pool, &fixture).await,
        footprint,
        "a refused second dispatch must leave no new footprint"
    );
}

/// Another workspace cannot recover this job into a terminal state.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_is_workspace_bound(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let intruder = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(intruder.workspace_id.as_uuid())
        .bind(format!("intruder-{}", Uuid::now_v7()))
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        dispatch_repository(&runtime)
            .recover_embedding_job_attempt(
                &intruder,
                fixture.job_id,
                Utc::now() + Duration::hours(1)
            )
            .await
            .is_err()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "running");
}
