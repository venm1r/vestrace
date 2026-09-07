//! Runtime-role schema evidence for the delivery ResultPrepared authority.
//!
//! Production-shaped Task 14D evidence for the guarded ResultPrepared
//! authority. It uses the Task 14C output fixture only to establish the prior
//! accepted-key boundary, then proves this task's runtime-role commit, replay,
//! refusal and immutable-result behavior without direct result-table DML.

mod common;

use std::{fs, path::Path};

use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, PgPool, Row, migrate::Migrator};
use uuid::Uuid;
use vestrace_application::{
    AcceptDeliveryOutputs, AcceptEmbeddingJob, DeliveryOutputIdentity,
    EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyMode, EmbeddingJobRepository, EmbeddingOutputKeyProgress,
    EmbeddingOutputKeyRepository, EmbeddingOutputKeyService, IdempotencyRecord, OutboxMessage,
    PreDispatchTerminalState, PreDispatchTerminationEvidence, RequestEmbeddingOutputRetirement,
    TerminateEmbeddingJobPreDispatch,
};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, ContentMaterialId, DataDestination,
    EmbeddingSpaceId, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId,
    ModelRequestEvidenceId, PolicyDecision, PolicyDecisionReason, PolicyDecisionResult,
    PolicyInputState, ResourceScope, RiskCategory, Sensitivity,
    embedding::EmbeddingJobKind,
    id::{AuditEventId, OutboxId, PolicyDecisionId},
    trust::{KeyPurpose, KeyReference, SecretResolutionRequest},
};
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
use vestrace_infrastructure::postgres::{
    PgEmbeddingDataPolicyDecisionRepository, PgEmbeddingJobRepository,
    PgEmbeddingOutputKeyRepository, PgStore,
};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const PROVISIONER: &str = include_str!("../../../docker/postgres/init-runtime-role.sh");

const RESULT_TABLES: [&str; 6] = [
    "embedding_space_corpus_states",
    "embedding_index_generation_guards",
    "embedding_job_result_preparations",
    "embedding_projection_entries",
    "embedding_job_result_prepared_attachments",
    "embedding_projection_source_dependencies",
];

type CredentialResultMarker = (
    String,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    String,
    String,
    String,
    String,
);

fn provisioner_sql_from(marker: &str) -> &'static str {
    let start = PROVISIONER
        .find(marker)
        .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
    PROVISIONER[start..]
        .rsplit_once("\nSQL\n")
        .map(|(sql, _)| sql)
        .expect("the provisioner must contain the SQL heredoc terminator")
}

async fn install_extensions_from_real_provisioner(pool: &PgPool) {
    let statements = PROVISIONER
        .lines()
        .filter(|line| line.starts_with("CREATE EXTENSION IF NOT EXISTS "))
        .collect::<Vec<_>>()
        .join("\n");
    sqlx::raw_sql(&statements).execute(pool).await.unwrap();
}

async fn hand_database_to_runtime(pool: &PgPool) {
    sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I OWNER TO vestrace', current_database()); END $$",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = false)]
async fn result_preparation_schema_is_guarded_and_runtime_dml_is_refused(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the real provisioner must install the 0194 ownership bridge");
    let runtime = common::runtime_pool(&pool).await;
    MIGRATOR
        .run(&runtime)
        .await
        .expect("the restricted runtime must apply 0194 through the real bridge");

    for table in RESULT_TABLES {
        let authority: (String, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(relowner), relrowsecurity, relforcerowsecurity, \
                    has_table_privilege('vestrace', $1, 'SELECT'), \
                    has_table_privilege('vestrace', $1, 'INSERT'), \
                    has_table_privilege('vestrace', $1, 'UPDATE') \
               FROM pg_class WHERE oid=('public.' || $1)::regclass",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            authority,
            (
                "vestrace_guarded_owner".into(),
                true,
                true,
                true,
                false,
                false
            ),
            "{table} must expose no direct runtime write path"
        );
    }

    let policy_lock_acl: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT has_table_privilege('vestrace_guarded_owner', \
                    'public.embedding_data_policy_decisions', 'SELECT'), \
                has_table_privilege('vestrace_guarded_owner', \
                    'public.embedding_data_policy_decisions', 'UPDATE'), \
                EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.embedding_data_policy_decisions'::regclass \
                       AND tgname='tr_embedding_data_policy_decisions_no_update'), \
                EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.embedding_data_policy_decisions'::regclass \
                       AND tgname='tr_embedding_data_policy_decisions_no_delete')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        policy_lock_acl,
        (true, true, true, true),
        "the guarded command may lock its immutable policy decision without granting runtime a result-table write path"
    );

    let receipt_authority: (bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT has_column_privilege('vestrace_guarded_owner', \
                    'public.external_effect_receipts', 'id', 'INSERT'), \
                has_column_privilege('vestrace_guarded_owner', \
                    'public.external_effect_lifecycle_transitions', 'effect_id', 'INSERT'), \
                has_sequence_privilege('vestrace_guarded_owner', \
                    'public.external_effect_lifecycle_transitions_ordinal_seq', 'USAGE'), \
                has_table_privilege('vestrace', 'public.external_effect_receipts', 'INSERT'), \
                has_table_privilege('vestrace', 'public.external_effect_lifecycle_transitions', 'INSERT'), \
                has_sequence_privilege('vestrace', \
                    'public.external_effect_lifecycle_transitions_ordinal_seq', 'USAGE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipt_authority,
        (true, true, true, true, true, true),
        "the guarded result command has its exact insert columns and the legacy runtime receipt posture is unchanged"
    );

    let functions: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT procedure.proname, pg_get_userbyid(procedure.proowner), \
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                has_function_privilege('public',procedure.oid,'EXECUTE') \
           FROM pg_proc procedure WHERE procedure.oid IN ( \
             'public.vestrace_validate_embedding_result_preparation()'::regprocedure, \
             'public.vestrace_validate_embedding_projection_dependency()'::regprocedure, \
             'public.vestrace_create_embedding_result_space_guards()'::regprocedure, \
             'public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::regprocedure, \
             'public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'::regprocedure, \
             'public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'::regprocedure, \
             'public.vestrace_reject_result_prepared_pre_dispatch_terminalization()'::regprocedure, \
             'public.vestrace_assign_embedding_delivery_source_intent()'::regprocedure \
           ) ORDER BY procedure.proname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(functions.len(), 8);
    for (name, owner, runtime_execute, public_execute) in functions {
        assert_eq!(owner, "vestrace_guarded_owner", "{name}");
        assert!(!public_execute, "{name}");
        assert_eq!(
            runtime_execute,
            matches!(
                name.as_str(),
                "vestrace_lock_embedding_result_completion_authority"
                    | "vestrace_load_embedding_result_eligibility"
                    | "vestrace_commit_embedding_result_preparation"
            ),
            "{name} runtime ACL"
        );
    }

    let constraints: Vec<String> = sqlx::query_scalar(
        "SELECT conname FROM pg_constraint WHERE conname = ANY($1) ORDER BY conname",
    )
    .bind(vec![
        "embedding_projection_response_index_is_input_ordinal",
        "embedding_delivery_source_memberships_result_exact_key",
        "embedding_space_registrations_result_model_dimension_key",
        "embedding_result_preparation_policy_fkey",
        "embedding_projection_policy_fkey",
        "embedding_delivery_source_memberships_source_intent_exact_fkey",
    ])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(constraints.len(), 6, "0194 identity constraints must exist");

    let fresh_guard_trigger: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.embedding_space_registrations'::regclass \
           AND tgname='embedding_result_space_guard_on_registration' \
           AND tgfoid='public.vestrace_create_embedding_result_space_guards()'::regprocedure)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        fresh_guard_trigger,
        "a post-0194 registration must create both result guards"
    );

    let source_identity_trigger: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.embedding_delivery_source_memberships'::regclass \
           AND tgname='embedding_delivery_source_memberships_source_intent_assign' \
           AND tgfoid='public.vestrace_assign_embedding_delivery_source_intent()'::regprocedure)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        source_identity_trigger,
        "each membership must persist its source material's exact source intent separately from its output intent"
    );

    let completion_arguments: String = sqlx::query_scalar(
        "SELECT pg_get_function_arguments('public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::regprocedure)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        completion_arguments,
        "target_workspace uuid, target_job uuid, target_effect uuid, target_connection uuid, target_connection_revision uuid, target_dispatch_transition uuid, target_lease uuid"
    );

    let error = sqlx::query(
        "INSERT INTO embedding_job_result_preparations(\
           id,workspace_id,job_id,external_effect_id,space_registration_id,model_binding_snapshot_id,\
           model_request_evidence_id,expected_job_version,adapter,response_model,output_count,\
           data_policy_decision_id,receipt_id) VALUES(\
           gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),\
           gen_random_uuid(),gen_random_uuid(),1,'forbidden','forbidden',1,gen_random_uuid(),gen_random_uuid())",
    )
    .execute(&runtime)
    .await
    .unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );

    runtime.close().await;
}

const RESULT_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
const BOOTSTRAP_KEY_ID: &str = "result-preparation-output-key-bootstrap";
const BOOTSTRAP_SCOPE: &str = "result-preparation-output-key-bootstrap";
const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

struct ResultFixture {
    runtime: PgPool,
    accepted: common::AcceptedJob,
    sources: Vec<ContentMaterialId>,
    outputs: Vec<DeliveryOutputIdentity>,
    policy_cause: Uuid,
}

#[derive(Clone, Copy, Debug)]
enum DeliveryPolicyCase {
    ExactAllowed,
    WrongCause,
    WrongAttempt,
    WrongInputCount,
    Denied,
}

#[derive(Clone, Debug)]
struct CommitAttempt {
    job_id: Uuid,
    effect_id: Uuid,
    expected_version_delta: i64,
    response_model: String,
    output_count: usize,
    dimensions: Vec<i32>,
}

#[derive(Debug, Eq, FromRow, PartialEq)]
struct RefusedPreparationState {
    markers: i64,
    receipts: i64,
    attachments: i64,
    ciphertexts: i64,
    materials: i64,
    projections: i64,
    dependencies: i64,
    lease_released: bool,
    job_state: String,
    job_version: i64,
    corpus_revision: i64,
    generation_epoch: i64,
}

#[derive(Debug, Eq, FromRow, PartialEq)]
struct ResultPreparedFenceState {
    markers: i64,
    receipts: i64,
    attachments: i64,
    ciphertexts: i64,
    projections: i64,
    dependencies: i64,
    lease_released: bool,
    job_state: String,
    source_blockers_nonterminal: i64,
    retirement_requests: i64,
    terminal_authorities: i64,
    no_auth_marker: bool,
    live_output_materials: i64,
    ordinary_output_references: i64,
    corpus_revision: i64,
    generation_epoch: i64,
}

struct OutputVaultFixture {
    bootstrap: tempfile::TempDir,
    vault: tempfile::TempDir,
}

impl OutputVaultFixture {
    fn new() -> Self {
        let bootstrap = tempfile::TempDir::new().unwrap();
        write_bootstrap(bootstrap.path());
        Self {
            bootstrap,
            vault: tempfile::TempDir::new().unwrap(),
        }
    }

    fn vault(&self, workspace: vestrace_domain::WorkspaceId) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault.path(),
            self.bootstrap.path(),
            KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                BOOTSTRAP_SCOPE,
                BOOTSTRAP_ALGORITHM,
            )
            .unwrap(),
            SecretResolutionRequest::new(
                workspace,
                BOOTSTRAP_SCOPE,
                "test://result-preparation-output-key",
            ),
        )
        .unwrap()
    }
}

fn write_bootstrap(root: &Path) {
    let key = root.join(BOOTSTRAP_KEY_ID);
    let version = key.join("v1");
    fs::create_dir_all(&version).unwrap();
    fs::write(key.join("scope"), BOOTSTRAP_SCOPE).unwrap();
    fs::write(key.join("purpose"), "storage").unwrap();
    fs::write(key.join("algorithm"), BOOTSTRAP_ALGORITHM).unwrap();
    fs::write(version.join("state"), "active").unwrap();
    fs::write(version.join("private.pkcs8"), [0x5A; 32]).unwrap();
}

#[derive(Debug, FromRow)]
struct ResultPreparedState {
    markers: i64,
    receipts: i64,
    attachments: i64,
    projections: i64,
    dependencies: i64,
    prepared_attachments: i64,
    ciphertexts: i64,
    lease_released: bool,
    live_materials: i64,
    ordinary_references: i64,
    memory_embeddings: i64,
    corpus_revision: i64,
    generation_epoch: i64,
    sensitivity: String,
    classification_labels: Vec<String>,
    has_unclassified: bool,
}

async fn scoped(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, workspace: Uuid) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
}

async fn live_source(runtime: &PgPool, accepted: &common::AcceptedJob) -> ContentMaterialId {
    let material_id = ContentMaterialId::new();
    let intent_id = MaterialKeyCreationIntentId::new();
    let key_id = MaterialKeyId::new();
    let mut framed_ciphertext = vec![0x51_u8; 4096];
    framed_ciphertext[..5].copy_from_slice(b"VMRF\x01");
    let mut transaction = runtime.begin().await.unwrap();
    scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)")
        .bind(intent_id.as_uuid()).bind(accepted.context.workspace_id.as_uuid())
        .bind(material_id.as_uuid()).bind(key_id.as_uuid()).bind(Uuid::now_v7())
        .bind(accepted.job_id.as_uuid()).execute(&mut *transaction).await.unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(framed_ciphertext)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    material_id
}

async fn attach_source_to_evidence(
    owner: &PgPool,
    accepted: &common::AcceptedJob,
    source: ContentMaterialId,
    ordinal: i64,
) {
    let mut transaction = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,$4,'governed_input_material',$5)")
        .bind(Uuid::now_v7()).bind(accepted.context.workspace_id.as_uuid()).bind(accepted.evidence_id)
        .bind(ordinal).bind(source.as_uuid()).execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

fn outputs() -> Vec<DeliveryOutputIdentity> {
    (0..2)
        .map(|output_ordinal| DeliveryOutputIdentity {
            output_ordinal,
            intent_id: MaterialKeyCreationIntentId::new(),
            material_id: ContentMaterialId::new(),
            key_id: MaterialKeyId::new(),
            nonce: IntentNonce::new(),
        })
        .collect()
}

fn acceptance_command(
    accepted: &common::AcceptedJob,
    receipt_id: Uuid,
    output_set: Vec<DeliveryOutputIdentity>,
) -> AcceptDeliveryOutputs {
    let at = DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap();
    AcceptDeliveryOutputs {
        receipt_id,
        idempotency_key: format!("result-preparation-{receipt_id}"),
        acceptance: AcceptEmbeddingJob {
            job_id: accepted.job_id,
            space_registration_id: EmbeddingSpaceId::from_uuid(accepted.space_registration_id),
            kind: EmbeddingJobKind::Delivery,
            model_binding_snapshot_id: accepted.snapshot_id,
            intent: accepted.intent.clone(),
            model_request_evidence_id: ModelRequestEvidenceId::from_uuid(accepted.evidence_id),
            retries_unknown_embedding_job_id: None,
            expected_predecessor_version: None,
            idempotency: Some(IdempotencyRecord {
                idempotency_key: format!("result-preparation-{receipt_id}"),
                workspace_id: accepted.context.workspace_id,
                request_hash: format!("result-preparation:{receipt_id}"),
                response_payload: None,
                status: "completed".into(),
                created_at: at,
                expires_at: at + Duration::hours(24),
            }),
            outbox: vec![OutboxMessage {
                id: OutboxId::from_uuid(receipt_id),
                workspace_id: accepted.context.workspace_id,
                topic: "embedding.job.delivery_accepted".into(),
                payload: serde_json::json!({"receipt_id": receipt_id}),
                created_at: at,
                attempts: 0,
            }],
            audit: AuditEvent::new(
                AuditEventId::from_uuid(receipt_id),
                accepted.context.workspace_id,
                accepted.context.principal_id,
                "embedding.job.delivery_accepted",
                "embedding_job",
                accepted.job_id.as_uuid(),
                serde_json::json!({"receipt_id": receipt_id}),
                at,
            )
            .unwrap(),
        },
        outputs: output_set,
    }
}

async fn reconcile_output_receipts(
    runtime: &PgPool,
    accepted: &common::AcceptedJob,
    output_set: &[DeliveryOutputIdentity],
) {
    let vault_fixture = OutputVaultFixture::new();
    let service = EmbeddingOutputKeyService::new(
        std::sync::Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
            runtime.clone(),
        ))),
        std::sync::Arc::new(vault_fixture.vault(accepted.context.workspace_id)),
    );
    let mut reconciled = 0usize;
    let mut aggregate_prepared = false;
    for _ in 0..=output_set.len() {
        match service.reconcile_one(&accepted.context).await.unwrap() {
            Some(EmbeddingOutputKeyProgress::WaitingForResultKeys) => reconciled += 1,
            Some(EmbeddingOutputKeyProgress::Prepared { .. }) => {
                reconciled += 1;
                aggregate_prepared = true;
            }
            None => break,
            other => panic!("output-key fixture did not prepare its exact output: {other:?}"),
        }
    }
    assert_eq!(reconciled, output_set.len());
    assert!(
        aggregate_prepared,
        "the exact output aggregate was not prepared"
    );
}

async fn dispatch_through_embedding_fence(runtime: &PgPool, accepted: &common::AcceptedJob) {
    let mut transaction = runtime.begin().await.unwrap();
    scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT * FROM vestrace_try_admit_provider_dispatch($1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)")
        .bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7())
        .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.connection_id)
        .bind(accepted.connection_revision_id).bind(accepted.external_effect_id).bind(accepted.evidence_id)
        .bind(accepted.snapshot_id).execute(&mut *transaction).await.unwrap();
    let authorization_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_authorizations(id,effect_id,workspace_id,policy_id,policy_version,subject_id,capability,operation,resource_scope,result,reason,input_state,matched_grant_id,decided_at,payload) SELECT $1,id,workspace_id,NULL,'result-preparation-v1',$2,'export.read','produce a governed embedding','http://127.0.0.1:1234/v1/embeddings','allow','configured_allowance','{}'::jsonb,NULL,NOW(),'{}'::jsonb FROM external_effect_intents WHERE id=$3 AND workspace_id=$4")
        .bind(authorization_id).bind(accepted.context.principal_id.as_uuid()).bind(accepted.external_effect_id)
        .bind(accepted.context.workspace_id.as_uuid()).execute(&mut *transaction).await.unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'authorized','authorization_recorded',$3,NOW())")
        .bind(accepted.external_effect_id).bind(accepted.context.workspace_id.as_uuid()).bind(authorization_id.to_string())
        .execute(&mut *transaction).await.unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),'result-preparation-test',NOW()+INTERVAL '60 seconds')")
        .bind(accepted.external_effect_id).bind(accepted.context.workspace_id.as_uuid()).bind(accepted.external_effect_id.to_string())
        .execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn record_delivery_policy(runtime: &PgPool, cause: Uuid, policy: DeliveryPolicyCase) {
    let causal_reference_id = if matches!(policy, DeliveryPolicyCase::WrongCause) {
        Uuid::now_v7()
    } else {
        cause
    };
    let delivery_attempt = if matches!(policy, DeliveryPolicyCase::WrongAttempt) {
        Some(2)
    } else {
        Some(1)
    };
    let input_count = if matches!(policy, DeliveryPolicyCase::WrongInputCount) {
        1
    } else {
        2
    };
    let allowed = !matches!(policy, DeliveryPolicyCase::Denied);
    PgEmbeddingDataPolicyDecisionRepository::new(PgStore::from_pool(runtime.clone()))
        .record(&EmbeddingDataPolicyDecisionRecord {
            id: Uuid::now_v7(),
            purpose: vestrace_application::EmbeddingPurpose::Delivery,
            causal_reference_id,
            delivery_attempt,
            batch_ordinal: None,
            destination: DataDestination::LocalModel,
            classification: Sensitivity::Confidential,
            classification_labels: vec!["alpha".into(), "beta".into()],
            unclassified_count: 1,
            input_count,
            classification_allowed: allowed,
            destination_allowed: true,
            allowed,
            reason: "test allowed delivery".into(),
            policy_version: "result-preparation-v1".into(),
            mode: EmbeddingDataPolicyMode::Enforce,
            decided_at: Utc::now(),
        })
        .await
        .unwrap();
}

async fn result_fixture_with_policy_and_auth(
    pool: &PgPool,
    policy: DeliveryPolicyCase,
    credential_backed: bool,
) -> ResultFixture {
    let runtime = common::runtime_pool(pool).await;
    let accepted = if credential_backed {
        common::prepare_delivery_embedding_job_with_pinned_credential(pool, &runtime).await
    } else {
        common::prepare_delivery_embedding_job(pool, &runtime).await
    };
    let sources = vec![
        live_source(&runtime, &accepted).await,
        live_source(&runtime, &accepted).await,
        live_source(&runtime, &accepted).await,
    ];
    common::make_dispatchable(pool, &runtime, &accepted).await;
    for (offset, source) in sources.iter().copied().enumerate() {
        attach_source_to_evidence(pool, &accepted, source, 8 + offset as i64).await;
    }
    let output_set = outputs();
    let acceptance_receipt = Uuid::now_v7();
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &accepted.context,
            acceptance_command(&accepted, acceptance_receipt, output_set.clone()),
        )
        .await
        .unwrap();
    reconcile_output_receipts(&runtime, &accepted, &output_set).await;
    record_delivery_policy(&runtime, acceptance_receipt, policy).await;
    dispatch_through_embedding_fence(&runtime, &accepted).await;
    ResultFixture {
        runtime,
        accepted,
        sources,
        outputs: output_set,
        policy_cause: acceptance_receipt,
    }
}

async fn result_fixture_with_policy(pool: &PgPool, policy: DeliveryPolicyCase) -> ResultFixture {
    result_fixture_with_policy_and_auth(pool, policy, false).await
}

async fn result_fixture(pool: &PgPool) -> ResultFixture {
    result_fixture_with_policy(pool, DeliveryPolicyCase::ExactAllowed).await
}

async fn result_fixture_with_pinned_credential(pool: &PgPool) -> ResultFixture {
    result_fixture_with_policy_and_auth(pool, DeliveryPolicyCase::ExactAllowed, true).await
}

async fn provision_result_behavior_database(pool: &PgPool) {
    install_extensions_from_real_provisioner(pool).await;
    hand_database_to_runtime(pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(pool)
    .await
    .expect("the real provisioner must install the runtime migration bridge");
    let runtime = common::runtime_pool(pool).await;
    MIGRATOR
        .run(&runtime)
        .await
        .expect("the restricted runtime must apply the result-preparation migration");
    runtime.close().await;
}

fn exact_attempt(fixture: &ResultFixture) -> CommitAttempt {
    CommitAttempt {
        job_id: fixture.accepted.job_id.as_uuid(),
        effect_id: fixture.accepted.external_effect_id,
        expected_version_delta: 0,
        response_model: RESULT_MODEL.into(),
        output_count: fixture.outputs.len(),
        dimensions: vec![768; fixture.outputs.len()],
    }
}

async fn try_commit_result(
    fixture: &ResultFixture,
    preparation: Uuid,
    receipt: Uuid,
    attempt: &CommitAttempt,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut transaction,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let mut framed = vec![0x51_u8; 4096];
    framed[..5].copy_from_slice(b"VMRF\x01");
    let result = async {
        let (lease, dispatch): (Uuid, Uuid) = sqlx::query_as(
            "SELECT lease.id,lifecycle.id FROM provider_concurrency_leases lease \
             JOIN external_effect_lifecycle_transitions lifecycle ON lifecycle.workspace_id=lease.workspace_id AND lifecycle.effect_id=lease.external_effect_id \
             WHERE lease.workspace_id=$1 AND lease.external_effect_id=$2 AND lifecycle.status='dispatching' \
             ORDER BY lifecycle.ordinal DESC LIMIT 1",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_lock_embedding_result_completion_authority($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(fixture.accepted.connection_id)
        .bind(fixture.accepted.connection_revision_id)
        .bind(dispatch)
        .bind(lease)
        .fetch_one(&mut *transaction)
        .await?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar(
            "SELECT vestrace_commit_embedding_result_preparation($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(preparation)
        .bind(receipt)
        .bind(version + attempt.expected_version_delta)
        .bind(&attempt.response_model)
        .bind((0..attempt.output_count).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
        .bind((0..attempt.output_count).map(|_| framed.clone()).collect::<Vec<_>>())
        .bind(&attempt.dimensions)
        .fetch_one(&mut *transaction)
        .await
    }
    .await;
    match result {
        Ok(prepared) => {
            transaction.commit().await?;
            Ok(prepared)
        }
        Err(error) => {
            transaction.rollback().await.unwrap();
            Err(error)
        }
    }
}

async fn commit_result(fixture: &ResultFixture, preparation: Uuid, receipt: Uuid) -> Uuid {
    try_commit_result(fixture, preparation, receipt, &exact_attempt(fixture))
        .await
        .unwrap()
}

async fn refused_state(pool: &PgPool, fixture: &ResultFixture) -> RefusedPreparationState {
    sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND job_id=$2) AS markers, \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND effect_id=$3) AS receipts, \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments attachment JOIN embedding_job_material_intents member ON member.workspace_id=attachment.workspace_id AND member.intent_id=attachment.intent_id WHERE member.workspace_id=$1 AND member.job_id=$2) AS attachments, \
          (SELECT count(*) FROM content_material_bytes bytes JOIN embedding_job_material_intents member ON member.workspace_id=bytes.workspace_id AND member.intent_id=bytes.intent_id WHERE member.workspace_id=$1 AND member.job_id=$2) AS ciphertexts, \
          (SELECT count(*) FROM content_materials material JOIN embedding_job_material_intents member ON member.workspace_id=material.workspace_id AND member.intent_id=material.intent_id WHERE member.workspace_id=$1 AND member.job_id=$2) AS materials, \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND job_id=$2) AS projections, \
          (SELECT count(*) FROM embedding_projection_source_dependencies WHERE workspace_id=$1 AND job_id=$2) AS dependencies, \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE workspace_id=$1 AND external_effect_id=$3) AS lease_released, \
          (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2) AS job_state, \
          (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2) AS job_version, \
          (SELECT corpus_revision FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$4) AS corpus_revision, \
          (SELECT generation_epoch FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$4) AS generation_epoch",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(fixture.accepted.external_effect_id)
    .bind(fixture.accepted.space_registration_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn assert_refused_without_durable_result(pool: &PgPool, fixture: &ResultFixture) {
    assert_eq!(
        refused_state(pool, fixture).await,
        RefusedPreparationState {
            markers: 0,
            receipts: 0,
            attachments: 0,
            ciphertexts: 0,
            materials: 0,
            projections: 0,
            dependencies: 0,
            lease_released: false,
            job_state: "running".into(),
            job_version: 2,
            corpus_revision: 0,
            generation_epoch: 0,
        },
        "a refused result attempt must not leave any partial durable result or advance lifecycle state"
    );
}

fn cancellation_termination(
    fixture: &ResultFixture,
    idempotency_key: &str,
) -> TerminateEmbeddingJobPreDispatch {
    let request = AuthorizationRequest::new(
        Capability::ExecutionWrite,
        "embedding.job.cancel",
        ResourceScope::workspace().to_string(),
        RiskCategory::Low,
    );
    TerminateEmbeddingJobPreDispatch {
        receipt_id: Uuid::now_v7(),
        job_id: fixture.accepted.job_id,
        expected_version: 2,
        idempotency_key: idempotency_key.into(),
        terminal_state: PreDispatchTerminalState::Cancelled,
        evidence: PreDispatchTerminationEvidence::CancellationAuthorization(Box::new(
            PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "result-preparation-terminal-fence-v1".into(),
                workspace_id: fixture.accepted.context.workspace_id,
                subject_id: fixture.accepted.context.principal_id,
                capability: Capability::ExecutionWrite,
                operation: "embedding.job.cancel".into(),
                resource_scope: ResourceScope::workspace().to_string(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    fixture.accepted.context.workspace_id,
                    fixture.accepted.context.principal_id,
                    &request,
                ),
                matched_grant_id: None,
                decided_at: Utc::now(),
            },
        )),
    }
}

async fn result_prepared_fence_state(
    pool: &PgPool,
    fixture: &ResultFixture,
) -> ResultPreparedFenceState {
    sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND job_id=$2) AS markers, \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND effect_id=$3) AS receipts, \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments attachment JOIN embedding_job_result_preparations marker ON marker.workspace_id=attachment.workspace_id AND marker.id=attachment.preparation_id WHERE marker.workspace_id=$1 AND marker.job_id=$2) AS attachments, \
          (SELECT count(*) FROM content_material_bytes bytes JOIN embedding_job_material_intents member ON member.workspace_id=bytes.workspace_id AND member.intent_id=bytes.intent_id WHERE member.workspace_id=$1 AND member.job_id=$2) AS ciphertexts, \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND job_id=$2) AS projections, \
          (SELECT count(*) FROM embedding_projection_source_dependencies WHERE workspace_id=$1 AND job_id=$2) AS dependencies, \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE workspace_id=$1 AND external_effect_id=$3) AS lease_released, \
          (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2) AS job_state, \
          (SELECT count(*) FROM embedding_delivery_source_memberships membership JOIN material_erasure_blockers blocker ON blocker.workspace_id=membership.workspace_id AND blocker.id=membership.blocker_id WHERE membership.workspace_id=$1 AND membership.job_id=$2 AND blocker.state='nonterminal') AS source_blockers_nonterminal, \
          (SELECT count(*) FROM embedding_output_key_retirement_requests WHERE workspace_id=$1 AND job_id=$2) AS retirement_requests, \
          (SELECT count(*) FROM embedding_job_pre_dispatch_retirement_authorities WHERE workspace_id=$1 AND job_id=$2) AS terminal_authorities, \
          (SELECT auth_branch='no_auth' AND credential_revision_id IS NULL AND credential_intent_id IS NULL AND credential_erasure_blocker_id IS NULL FROM embedding_job_result_preparations WHERE workspace_id=$1 AND job_id=$2) AS no_auth_marker, \
          (SELECT count(*) FROM content_materials material JOIN embedding_projection_entries projection ON projection.workspace_id=material.workspace_id AND projection.material_id=material.id WHERE projection.workspace_id=$1 AND projection.job_id=$2 AND material.state='live') AS live_output_materials, \
          (SELECT count(*) FROM content_material_ordinary_references reference JOIN embedding_projection_entries projection ON projection.workspace_id=reference.workspace_id AND projection.material_id=reference.material_id WHERE projection.workspace_id=$1 AND projection.job_id=$2) AS ordinary_output_references, \
          (SELECT corpus_revision FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$4) AS corpus_revision, \
          (SELECT generation_epoch FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$4) AS generation_epoch",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(fixture.accepted.external_effect_id)
    .bind(fixture.accepted.space_registration_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn result_preparation_commits_a_complete_non_live_two_output_tuple(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    let receipt = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, receipt).await,
        preparation
    );
    let state: ResultPreparedState = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND id=$2) AS markers, \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND id=$3 AND effect_id=$4) AS receipts, \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$2) AS attachments, \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2) AS projections, \
          (SELECT count(*) FROM embedding_projection_source_dependencies dependency JOIN embedding_projection_entries projection ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id WHERE dependency.workspace_id=$1 AND projection.preparation_id=$2) AS dependencies, \
          (SELECT count(*) FROM prepared_material_attachments attachment JOIN embedding_job_result_prepared_attachments result ON result.prepared_attachment_id=attachment.id WHERE result.workspace_id=$1 AND result.preparation_id=$2) AS prepared_attachments, \
          (SELECT count(*) FROM content_material_bytes bytes JOIN embedding_projection_entries projection ON projection.workspace_id=bytes.workspace_id AND projection.intent_id=bytes.intent_id WHERE projection.workspace_id=$1 AND projection.preparation_id=$2) AS ciphertexts, \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE workspace_id=$1 AND external_effect_id=$4) AS lease_released, \
          (SELECT count(*) FROM content_materials material JOIN embedding_projection_entries projection ON projection.workspace_id=material.workspace_id AND projection.material_id=material.id WHERE projection.workspace_id=$1 AND projection.preparation_id=$2 AND material.state='live') AS live_materials, \
          (SELECT count(*) FROM content_material_ordinary_references reference JOIN embedding_projection_entries projection ON projection.workspace_id=reference.workspace_id AND projection.material_id=reference.material_id WHERE projection.workspace_id=$1 AND projection.preparation_id=$2) AS ordinary_references, \
          (SELECT count(*) FROM memory_embeddings) AS memory_embeddings, \
          (SELECT corpus_revision FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$5) AS corpus_revision, \
          (SELECT generation_epoch FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$5) AS generation_epoch, \
          (SELECT sensitivity::TEXT FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2 ORDER BY output_ordinal LIMIT 1) AS sensitivity, \
          (SELECT classification_labels FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2 ORDER BY output_ordinal LIMIT 1) AS classification_labels, \
          (SELECT has_unclassified FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2 ORDER BY output_ordinal LIMIT 1) AS has_unclassified",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid()).bind(preparation).bind(receipt)
    .bind(fixture.accepted.external_effect_id).bind(fixture.accepted.space_registration_id)
    .fetch_one(&pool).await.unwrap();
    assert_eq!(state.markers, 1);
    assert_eq!(state.receipts, 1);
    assert_eq!(state.attachments, 2);
    assert_eq!(state.projections, 2);
    assert_eq!(state.dependencies, 6);
    assert_eq!(state.prepared_attachments, 2);
    assert_eq!(state.ciphertexts, 2);
    assert!(state.lease_released);
    assert_eq!(state.live_materials, 0);
    assert_eq!(state.ordinary_references, 0);
    assert_eq!(state.memory_embeddings, 0);
    assert_eq!(state.corpus_revision, 0);
    assert_eq!(state.generation_epoch, 0);
    assert_eq!(state.sensitivity, "confidential");
    assert_eq!(state.classification_labels, vec!["alpha", "beta"]);
    assert!(state.has_unclassified);
    assert_eq!(fixture.policy_cause, sqlx::query_scalar::<_, Uuid>("SELECT causal_reference_id FROM embedding_data_policy_decisions decision JOIN embedding_job_result_preparations marker ON marker.data_policy_decision_id=decision.id WHERE marker.workspace_id=$1 AND marker.id=$2").bind(fixture.accepted.context.workspace_id.as_uuid()).bind(preparation).fetch_one(&pool).await.unwrap());
    let projection_policy: Vec<(i64, String, Vec<String>, bool)> = sqlx::query_as(
        "SELECT output_ordinal,sensitivity::TEXT,classification_labels,has_unclassified \
           FROM embedding_projection_entries \
          WHERE workspace_id=$1 AND preparation_id=$2 ORDER BY output_ordinal",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        projection_policy,
        vec![
            (
                0,
                "confidential".into(),
                vec!["alpha".into(), "beta".into()],
                true
            ),
            (
                1,
                "confidential".into(),
                vec!["alpha".into(), "beta".into()],
                true
            ),
        ],
        "every output must repeat the full sorted distinct policy label set and unclassified fact"
    );
    type SourceDependency = (i64, i32, Uuid, Uuid, Uuid, Uuid);
    let expected_dependencies: Vec<SourceDependency> = sqlx::query_as(
        "SELECT output_ordinal,source_ordinal,source_material_id,intent_id,source_intent_id,blocker_id \
           FROM embedding_delivery_source_memberships \
          WHERE workspace_id=$1 AND job_id=$2 ORDER BY output_ordinal,source_ordinal",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    let persisted_dependencies: Vec<SourceDependency> = sqlx::query_as(
        "SELECT projection.output_ordinal,dependency.source_ordinal,dependency.source_material_id, \
                dependency.output_intent_id,dependency.source_intent_id,dependency.erasure_blocker_id \
           FROM embedding_projection_source_dependencies dependency \
           JOIN embedding_projection_entries projection \
             ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id \
          WHERE projection.workspace_id=$1 AND projection.preparation_id=$2 \
          ORDER BY projection.output_ordinal,dependency.source_ordinal",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        expected_dependencies.len(),
        6,
        "three sources must be retained for each of two outputs"
    );
    assert_eq!(
        persisted_dependencies, expected_dependencies,
        "every projection must preserve the complete ordered 0193 source identity tuple"
    );
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_binds_its_active_credential_snapshot_tuple(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture_with_pinned_credential(&pool).await;
    let pinned = fixture
        .accepted
        .credential
        .as_ref()
        .expect("the credential fixture must expose its guarded pinned revision");
    let (expected_intent, expected_blocker, intent_state, blocker_kind, blocker_state): (
        Uuid,
        Uuid,
        String,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT intent.id,blocker.id,intent.state,blocker.blocker_kind,blocker.state \
           FROM model_binding_snapshots snapshot \
           JOIN credential_key_creation_intents intent \
             ON intent.workspace_id=snapshot.workspace_id \
            AND intent.connection_id=snapshot.connection_id \
            AND intent.credential_slot_id=snapshot.credential_slot_id \
            AND intent.credential_revision_id=snapshot.credential_revision_id \
           JOIN material_erasure_blockers blocker \
             ON blocker.workspace_id=intent.workspace_id \
            AND blocker.credential_intent_id=intent.id \
          WHERE snapshot.workspace_id=$1 AND snapshot.id=$2 \
            AND snapshot.branch='credential' \
            AND intent.state='active' \
            AND blocker.target_kind='credential' AND blocker.state='nonterminal' \
          ORDER BY blocker.id LIMIT 1",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.snapshot_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(intent_state, "active");
    assert_eq!(expected_blocker, pinned.completion_blocker_id);
    assert_eq!(blocker_kind, "effect");
    assert_eq!(blocker_state, "nonterminal");

    let preparation = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, Uuid::now_v7()).await,
        preparation,
        "the guarded result command must accept the active credential branch"
    );
    let marker: CredentialResultMarker = sqlx::query_as(
        "SELECT marker.auth_branch,marker.credential_revision_id,marker.credential_intent_id, \
                marker.credential_erasure_blocker_id,intent.state,blocker.target_kind, \
                blocker.blocker_kind,blocker.state \
           FROM embedding_job_result_preparations marker \
           JOIN credential_key_creation_intents intent \
             ON intent.workspace_id=marker.workspace_id AND intent.id=marker.credential_intent_id \
           JOIN material_erasure_blockers blocker \
             ON blocker.workspace_id=marker.workspace_id AND blocker.id=marker.credential_erasure_blocker_id \
          WHERE marker.workspace_id=$1 AND marker.id=$2",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        marker,
        (
            "credential".into(),
            Some(pinned.revision_id),
            Some(expected_intent),
            Some(expected_blocker),
            "active".into(),
            "credential".into(),
            "effect".into(),
            "nonterminal".into(),
        ),
        "ResultPrepared must retain the exact active credential revision, intent and completion blocker"
    );
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_marker_first_replay_and_concurrent_callers_converge(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let winner = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, winner, Uuid::now_v7()).await,
        winner
    );
    assert_eq!(
        commit_result(&fixture, Uuid::now_v7(), Uuid::now_v7()).await,
        winner,
        "a matching replay must converge on the immutable marker even with new random identities"
    );
    let replay_counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND effect_id=$3), \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$4), \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$4)",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(fixture.accepted.external_effect_id)
    .bind(winner)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(replay_counts, (1, 1, 2, 2));
    fixture.runtime.close().await;

    let concurrent = result_fixture(&pool).await;
    let left_id = Uuid::now_v7();
    let right_id = Uuid::now_v7();
    let (left, right) = tokio::join!(
        commit_result(&concurrent, left_id, Uuid::now_v7()),
        commit_result(&concurrent, right_id, Uuid::now_v7()),
    );
    assert!(
        left == left_id || right == right_id,
        "one concurrent caller must author its marker"
    );
    assert_eq!(
        left, right,
        "the concurrent loser must converge on the winner"
    );
    let concurrent_counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND effect_id=$3), \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1), \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1)",
    )
    .bind(concurrent.accepted.context.workspace_id.as_uuid())
    .bind(concurrent.accepted.job_id.as_uuid())
    .bind(concurrent.accepted.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(concurrent_counts, (1, 1, 2, 2));
    concurrent.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_loader_refuses_a_corrupted_partial_marker(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    let receipt = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, receipt).await,
        preparation,
        "the corruption probe must start with a production-shaped guarded commit"
    );

    // This is the sole owner mutation in the probe.  The fixture itself is
    // built by the runtime guarded authorities; the temporary removal models a
    // damaged persisted relation which normal DML and immutable triggers forbid.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("ALTER TABLE embedding_job_result_prepared_attachments DISABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM embedding_job_result_prepared_attachments \
         WHERE workspace_id=$1 AND preparation_id=$2 AND output_ordinal=0",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE embedding_job_result_prepared_attachments ENABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    owner.commit().await.unwrap();

    let durable_counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND id=$2), \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND id=$3), \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$2), \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2)",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .bind(receipt)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(durable_counts, (1, 1, 1, 2));

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let error = sqlx::query("SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)")
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_all(&mut *runtime)
        .await
        .unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "runtime eligibility must never converge on a merely present marker"
    );
    runtime.rollback().await.unwrap();
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_loader_refuses_a_marker_with_an_unsafe_source_chain(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, Uuid::now_v7()).await,
        preparation
    );

    // Task 14D exposes no source-terminalization authority after dispatch. The
    // existing runtime erasure command is the closest lifecycle route, and it
    // correctly refuses this exact accepted source while its 0193 blocker is
    // nonterminal.  Therefore the owner-only mutation below is corruption
    // evidence, not a way to construct an otherwise-valid fixture.
    let mut erasure = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut erasure,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let unavailable = sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
        .bind(fixture.sources[0].as_uuid())
        .execute(&mut *erasure)
        .await
        .unwrap_err();
    assert_eq!(
        unavailable
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "the existing erasure authority must not terminalize an accepted source blocker"
    );
    erasure.rollback().await.unwrap();

    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("ALTER TABLE content_materials DISABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE content_materials SET state='erasure_prepared' WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.sources[0].as_uuid())
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE content_materials ENABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    owner.commit().await.unwrap();

    let unsafe_chain: (String, String, String) = sqlx::query_as(
        "SELECT material.state,intent.state,blocker.state \
           FROM embedding_delivery_source_memberships membership \
           JOIN content_materials material \
             ON material.workspace_id=membership.workspace_id AND material.id=membership.source_material_id \
           JOIN material_key_creation_intents intent \
             ON intent.workspace_id=material.workspace_id AND intent.id=material.intent_id \
           JOIN material_erasure_blockers blocker \
             ON blocker.workspace_id=membership.workspace_id AND blocker.id=membership.blocker_id \
          WHERE membership.workspace_id=$1 AND membership.job_id=$2 AND membership.source_material_id=$3 \
          ORDER BY membership.output_ordinal LIMIT 1",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(fixture.sources[0].as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unsafe_chain,
        (
            "erasure_prepared".into(),
            "live".into(),
            "nonterminal".into()
        )
    );

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let error = sqlx::query("SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)")
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_all(&mut *runtime)
        .await
        .unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "runtime must not converge on a marker whose exact source chain is unsafe"
    );
    runtime.rollback().await.unwrap();
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_loader_refuses_a_marker_missing_an_exact_source_dependency(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    let receipt = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, receipt).await,
        preparation,
        "the dependency-corruption probe must begin with a complete runtime ResultPrepared tuple"
    );

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let existing_rows =
        sqlx::query("SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)")
            .bind(fixture.accepted.context.workspace_id.as_uuid())
            .bind(fixture.accepted.job_id.as_uuid())
            .bind(fixture.accepted.external_effect_id)
            .fetch_all(&mut *runtime)
            .await
            .unwrap();
    assert_eq!(
        existing_rows.len(),
        2,
        "a complete marker must load both outputs"
    );
    for (ordinal, row) in existing_rows.iter().enumerate() {
        assert_eq!(
            row.try_get::<Option<Uuid>, _>("preparation_id").unwrap(),
            Some(preparation),
            "Existing must retain the authored marker"
        );
        assert_eq!(
            row.try_get::<i64, _>("output_ordinal").unwrap(),
            ordinal as i64,
            "Existing rows must preserve the contiguous output order"
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("preparation_input_ordinal")
                .unwrap(),
            Some(ordinal as i64),
            "Existing must preserve its input ordinal"
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("preparation_response_index")
                .unwrap(),
            Some(ordinal as i64),
            "Existing must preserve its response index"
        );
    }
    runtime.commit().await.unwrap();

    // This models a damaged persisted dependency relation only after the
    // complete marker came from runtime guarded authorities.  The immutable
    // and deferred dependency triggers are disabled solely for this owner-only
    // corruption probe, then restored before committing the damaged state.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    let dependencies: Vec<(Uuid, i32, Uuid, Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT dependency.projection_id,dependency.source_ordinal,dependency.source_material_id, \
                dependency.output_intent_id,dependency.source_intent_id,dependency.erasure_blocker_id \
           FROM embedding_projection_source_dependencies dependency \
           JOIN embedding_projection_entries projection \
             ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id \
          WHERE projection.workspace_id=$1 AND projection.preparation_id=$2 \
          ORDER BY projection.output_ordinal,dependency.source_ordinal,dependency.source_material_id",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_all(&mut *owner)
    .await
    .unwrap();
    assert_eq!(
        dependencies.len(),
        6,
        "the valid fixture has all six dependencies"
    );
    let (
        projection_id,
        source_ordinal,
        source_material_id,
        output_intent_id,
        source_intent_id,
        erasure_blocker_id,
    ) = dependencies[0];
    sqlx::query(
        "ALTER TABLE embedding_projection_source_dependencies \
         DISABLE TRIGGER embedding_projection_source_dependencies_immutable",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE embedding_projection_source_dependencies \
         DISABLE TRIGGER embedding_projection_dependency_complete",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    let deleted = sqlx::query(
        "DELETE FROM embedding_projection_source_dependencies \
          WHERE workspace_id=$1 AND projection_id=$2 AND source_ordinal=$3 \
            AND source_material_id=$4 AND output_intent_id=$5 \
            AND source_intent_id=$6 AND erasure_blocker_id=$7",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(projection_id)
    .bind(source_ordinal)
    .bind(source_material_id)
    .bind(output_intent_id)
    .bind(source_intent_id)
    .bind(erasure_blocker_id)
    .execute(&mut *owner)
    .await
    .unwrap();
    assert_eq!(deleted.rows_affected(), 1);
    sqlx::query(
        "ALTER TABLE embedding_projection_source_dependencies \
         ENABLE TRIGGER embedding_projection_dependency_complete",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE embedding_projection_source_dependencies \
         ENABLE TRIGGER embedding_projection_source_dependencies_immutable",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let dependencies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_projection_source_dependencies dependency \
          JOIN embedding_projection_entries projection \
            ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id \
         WHERE projection.workspace_id=$1 AND projection.preparation_id=$2",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dependencies, 5,
        "one of six exact source dependencies is absent"
    );

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let load = sqlx::query("SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)")
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_all(&mut *runtime)
        .await;
    let error = match load {
        Err(error) => {
            runtime.rollback().await.unwrap();
            error
        }
        Ok(_) => {
            runtime.rollback().await.unwrap();
            let persisted: (i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT \
                  (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND id=$2), \
                  (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND id=$3), \
                  (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$2), \
                  (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2), \
                  (SELECT count(*) FROM embedding_projection_source_dependencies dependency \
                    JOIN embedding_projection_entries projection \
                      ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id \
                   WHERE projection.workspace_id=$1 AND projection.preparation_id=$2)",
            )
            .bind(fixture.accepted.context.workspace_id.as_uuid())
            .bind(preparation)
            .bind(receipt)
            .fetch_one(&pool)
            .await
            .unwrap();
            panic!(
                "runtime converged on a marker missing an exact source dependency: \
                 markers={}, receipts={}, attachments={}, projections={}, dependencies={}",
                persisted.0, persisted.1, persisted.2, persisted.3, persisted.4
            );
        }
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "runtime eligibility must reject a marker with a missing exact 0193 dependency"
    );
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_recovery_refuses_a_marker_with_mismatched_exact_acknowledgement(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    let receipt = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, receipt).await,
        preparation,
        "the recovery corruption probe must begin with a complete runtime ResultPrepared tuple"
    );

    // The fixture's marker and receipt come only from the runtime guarded
    // command.  This owner-only damage models a broken receipt witness after
    // that commit; it never grants runtime a receipt mutation path.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    let receipt_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) FROM pg_class \
          WHERE oid='public.external_effect_receipts'::regclass",
    )
    .fetch_one(&mut *owner)
    .await
    .unwrap();
    assert_eq!(
        receipt_owner, "vestrace",
        "only the actual external receipt table owner may conduct this corruption probe"
    );
    sqlx::query(
        "ALTER TABLE external_effect_receipts \
         DISABLE TRIGGER external_effect_receipts_task10_dispatch_contract",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    let updated = sqlx::query(
        "UPDATE external_effect_receipts \
            SET payload=jsonb_build_object('evidence_refs',jsonb_build_array('corrupted:receipt-witness')) \
          WHERE workspace_id=$1 AND id=$2 AND effect_id=$3",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(receipt)
    .bind(fixture.accepted.external_effect_id)
    .execute(&mut *owner)
    .await
    .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    sqlx::query(
        "ALTER TABLE external_effect_receipts \
         ENABLE TRIGGER external_effect_receipts_task10_dispatch_contract",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let persisted: (i64, i64, i64, i64, i64, bool) = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM embedding_job_result_preparations WHERE workspace_id=$1 AND id=$2), \
          (SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND id=$3 AND effect_id=$4), \
          (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$2), \
          (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2), \
          (SELECT count(*) FROM embedding_projection_source_dependencies dependency \
             JOIN embedding_projection_entries projection \
               ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id \
            WHERE dependency.workspace_id=$1 AND projection.preparation_id=$2), \
          COALESCE((SELECT outcome_status='acknowledged' \
                       AND payload @> jsonb_build_object('evidence_refs',jsonb_build_array(format('embedding_job_result_preparation:%s',$2))) \
                      FROM external_effect_receipts \
                     WHERE workspace_id=$1 AND id=$3 AND effect_id=$4),FALSE)",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .bind(receipt)
    .bind(fixture.accepted.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted,
        (1, 1, 2, 2, 6, false),
        "the owner corruption must leave the complete tuple durable but break only its exact acknowledgement witness"
    );

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let recovery =
        sqlx::query("SELECT phase FROM vestrace_lock_embedding_job_recovery_authority($1,$2)")
            .bind(fixture.accepted.context.workspace_id.as_uuid())
            .bind(fixture.accepted.job_id.as_uuid())
            .fetch_all(&mut *runtime)
            .await;
    match recovery {
        Err(error) => {
            runtime.rollback().await.unwrap();
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("23514"),
                "recovery must refuse a ResultPrepared marker without its exact acknowledgement"
            );
        }
        Ok(rows) => {
            let phases = rows
                .iter()
                .map(|row| row.try_get::<String, _>("phase").unwrap())
                .collect::<Vec<_>>();
            runtime.rollback().await.unwrap();
            panic!(
                "recovery bypassed the mismatched exact acknowledgement: phases={phases:?}, markers={}, receipts={}, attachments={}, projections={}, dependencies={}, receipt_exact={}",
                persisted.0, persisted.1, persisted.2, persisted.3, persisted.4, persisted.5
            );
        }
    }
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_loader_refuses_projection_with_mismatched_policy_labels(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    let receipt = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, receipt).await,
        preparation,
        "the classification corruption probe must begin with a complete runtime ResultPrepared tuple"
    );

    // Runtime creates the policy decision and ResultPrepared tuple. This
    // owner-only update models a damaged immutable projection after that fact;
    // it does not create a result or grant runtime projection-write authority.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query(
        "ALTER TABLE embedding_projection_entries \
         DISABLE TRIGGER embedding_projection_entry_dependency_complete",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE embedding_projection_entries \
         DISABLE TRIGGER embedding_projection_entries_immutable",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    let updated = sqlx::query(
        "UPDATE embedding_projection_entries \
            SET classification_labels=ARRAY['corrupted-label']::TEXT[] \
          WHERE workspace_id=$1 AND preparation_id=$2 AND output_ordinal=0",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .execute(&mut *owner)
    .await
    .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    sqlx::query(
        "ALTER TABLE embedding_projection_entries \
         ENABLE TRIGGER embedding_projection_entries_immutable",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE embedding_projection_entries \
         ENABLE TRIGGER embedding_projection_entry_dependency_complete",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let persisted: (i64, Vec<String>, Vec<String>, String, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM embedding_projection_entries \
                  WHERE workspace_id=$1 AND preparation_id=$2 AND output_ordinal=0), \
                projection.classification_labels,decision.classification_labels, \
                projection.sensitivity::TEXT,decision.classification::TEXT \
           FROM embedding_projection_entries projection \
           JOIN embedding_data_policy_decisions decision \
             ON decision.id=projection.data_policy_decision_id \
          WHERE projection.workspace_id=$1 AND projection.preparation_id=$2 \
            AND projection.output_ordinal=0",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(preparation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted,
        (
            1,
            vec!["corrupted-label".into()],
            vec!["alpha".into(), "beta".into()],
            "confidential".into(),
            "confidential".into(),
        ),
        "the persisted projection must differ only in its immutable policy-label tuple"
    );

    let mut runtime = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut runtime,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let load = sqlx::query("SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)")
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_all(&mut *runtime)
        .await;
    match load {
        Err(error) => {
            runtime.rollback().await.unwrap();
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("23514"),
                "runtime eligibility must fail closed when an immutable projection loses exact policy labels"
            );
        }
        Ok(rows) => {
            runtime.rollback().await.unwrap();
            panic!(
                "runtime eligibility accepted corrupted classification labels: rows={}, marker=1, projection_labels={:?}, policy_labels={:?}, projection_sensitivity={}, policy_sensitivity={}",
                rows.len(),
                persisted.1,
                persisted.2,
                persisted.3,
                persisted.4
            );
        }
    }
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_prepared_fences_late_terminal_retirement_and_generic_abandonment(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, Uuid::now_v7()).await,
        preparation
    );
    let before = result_prepared_fence_state(&pool, &fixture).await;
    assert_eq!(
        before,
        ResultPreparedFenceState {
            markers: 1,
            receipts: 1,
            attachments: 2,
            ciphertexts: 2,
            projections: 2,
            dependencies: 6,
            lease_released: true,
            job_state: "running".into(),
            source_blockers_nonterminal: 6,
            retirement_requests: 0,
            terminal_authorities: 0,
            no_auth_marker: true,
            live_output_materials: 0,
            ordinary_output_references: 0,
            corpus_revision: 0,
            generation_epoch: 0,
        }
    );

    let termination = cancellation_termination(&fixture, "result-prepared-late-cancellation");
    let cancellation = PgEmbeddingJobRepository::new(PgStore::from_pool(fixture.runtime.clone()))
        .terminate_pre_dispatch(fixture.accepted.context.clone(), termination.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(
            cancellation,
            vestrace_application::ApplicationError::Policy(_)
                | vestrace_application::ApplicationError::Conflict(_)
        ),
        "late pre-dispatch cancellation must be refused after ResultPrepared: {cancellation:?}"
    );
    assert_eq!(result_prepared_fence_state(&pool, &fixture).await, before);

    let retirement =
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(fixture.runtime.clone()))
            .request_retirement(
                &fixture.accepted.context,
                RequestEmbeddingOutputRetirement { termination },
            )
            .await
            .unwrap_err();
    assert!(
        matches!(
            retirement,
            vestrace_application::ApplicationError::Policy(_)
                | vestrace_application::ApplicationError::Conflict(_)
        ),
        "late output retirement must be refused after ResultPrepared: {retirement:?}"
    );
    assert_eq!(result_prepared_fence_state(&pool, &fixture).await, before);

    // The generic path is directly callable by runtime, but 0193 first
    // requires an exact retirement authority. ResultPrepared has no such
    // authority and its output already has result material, so generic
    // abandonment cannot start an erasure chain or replace the marker.
    let mut generic = fixture.runtime.begin().await.unwrap();
    scoped(
        &mut generic,
        fixture.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let abandonment = sqlx::query("SELECT vestrace_prepare_pre_prepared_material_abandon($1)")
        .bind(fixture.outputs[0].intent_id.as_uuid())
        .execute(&mut *generic)
        .await
        .unwrap_err();
    assert_eq!(
        abandonment
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    generic.rollback().await.unwrap();
    assert_eq!(result_prepared_fence_state(&pool, &fixture).await, before);
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_prepared_terminal_fence_rejects_a_guarded_owner_bypass(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    let preparation = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, Uuid::now_v7()).await,
        preparation,
        "the owner-bypass probe must begin with a complete runtime ResultPrepared tuple"
    );
    let before = result_prepared_fence_state(&pool, &fixture).await;

    // This models a buggy guarded internal writer, after the valid tuple has
    // already been authored solely by runtime authorities.  The values satisfy
    // this table's complete structural shape; the 0194 marker fence must still
    // make the insert impossible before any 0193 terminal workflow can use it.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    let insertion = sqlx::query(
        "INSERT INTO embedding_job_pre_dispatch_retirement_authorities(\
           id,workspace_id,principal_id,job_id,expected_version,idempotency_key,terminal_state,\
           evidence_kind,evidence_id,authorization_policy_version,authorization_capability,\
           authorization_operation,authorization_resource_scope,authorization_risk) \
         VALUES($1,$2,$3,$4,2,$5,'cancelled','cancellation_authorization',$6,\
                'result-prepared-owner-probe-v1','execution.write','embedding.job.cancel','workspace','low')",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.context.principal_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(format!("result-prepared-owner-bypass-{}", Uuid::now_v7()))
    .bind(Uuid::now_v7())
    .execute(&mut *owner)
    .await;
    let error = match insertion {
        Err(error) => {
            owner.rollback().await.unwrap();
            error
        }
        Ok(_) => {
            owner.commit().await.unwrap();
            let persisted: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM embedding_job_pre_dispatch_retirement_authorities \
                  WHERE workspace_id=$1 AND job_id=$2",
            )
            .bind(fixture.accepted.context.workspace_id.as_uuid())
            .bind(fixture.accepted.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            panic!(
                "ResultPrepared marker fence allowed a persisted guarded-owner terminal authority: {persisted}"
            );
        }
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "the ResultPrepared trigger must refuse even a guarded-owner authority insert"
    );

    assert_eq!(
        result_prepared_fence_state(&pool, &fixture).await,
        before,
        "the owner-bypass refusal must persist neither a terminal authority nor any partial mutation"
    );
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_refuses_a_missing_exact_output_key_receipt(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let fixture = result_fixture(&pool).await;
    assert_eq!(fixture.outputs.len(), 2);
    assert_eq!(fixture.outputs[1].output_ordinal, 1);

    // The fixture itself authored both output receipts through the runtime
    // reconciler.  This isolated owner-only deletion models persisted receipt
    // corruption; it never manufactures a valid result tuple by owner DML.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query(
        "ALTER TABLE embedding_output_key_receipts \
         DISABLE TRIGGER embedding_output_key_receipts_guarded",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    let deleted = sqlx::query(
        "DELETE FROM embedding_output_key_receipts \
          WHERE workspace_id=$1 AND job_id=$2 AND output_ordinal=1 AND intent_id=$3",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .bind(fixture.outputs[1].intent_id.as_uuid())
    .execute(&mut *owner)
    .await
    .unwrap();
    assert_eq!(deleted.rows_affected(), 1);
    sqlx::query(
        "ALTER TABLE embedding_output_key_receipts \
         ENABLE TRIGGER embedding_output_key_receipts_guarded",
    )
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_output_key_receipts \
          WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipt_count, 1,
        "exactly one accepted output receipt is absent"
    );

    let commit = try_commit_result(
        &fixture,
        Uuid::now_v7(),
        Uuid::now_v7(),
        &exact_attempt(&fixture),
    )
    .await;
    let error = match commit {
        Err(error) => error,
        Ok(preparation) => {
            let persisted = refused_state(&pool, &fixture).await;
            let remaining_receipts: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM embedding_output_key_receipts \
                  WHERE workspace_id=$1 AND job_id=$2",
            )
            .bind(fixture.accepted.context.workspace_id.as_uuid())
            .bind(fixture.accepted.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            panic!(
                "missing 0193 output receipt authored ResultPrepared {preparation}: \
                 result={persisted:?}, output_receipts={remaining_receipts}"
            );
        }
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514"),
        "the guarded result command must require every exact 0193 output receipt"
    );
    assert_refused_without_durable_result(&pool, &fixture).await;
    fixture.runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_preparation_refuses_malformed_caller_tuples_without_partial_state(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    for (name, mutate) in [
        (
            "partial attachment/ciphertext/dimension arrays",
            Box::new(|attempt: &mut CommitAttempt| {
                attempt.output_count = 1;
                attempt.dimensions = vec![768];
            }) as Box<dyn Fn(&mut CommitAttempt)>,
        ),
        (
            "wrong response model",
            Box::new(|attempt: &mut CommitAttempt| {
                attempt.response_model = "text-embedding-wrong".into();
            }) as Box<dyn Fn(&mut CommitAttempt)>,
        ),
        (
            "wrong dimensions",
            Box::new(|attempt: &mut CommitAttempt| {
                attempt.dimensions = vec![767, 767];
            }) as Box<dyn Fn(&mut CommitAttempt)>,
        ),
        (
            "wrong expected job version",
            Box::new(|attempt: &mut CommitAttempt| {
                attempt.expected_version_delta = 1;
            }) as Box<dyn Fn(&mut CommitAttempt)>,
        ),
    ] {
        let fixture = result_fixture(&pool).await;
        let before = refused_state(&pool, &fixture).await;
        assert_refused_without_durable_result(&pool, &fixture).await;
        let mut attempt = exact_attempt(&fixture);
        mutate(&mut attempt);
        let error = try_commit_result(&fixture, Uuid::now_v7(), Uuid::now_v7(), &attempt)
            .await
            .unwrap_err();
        assert!(
            matches!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("22023" | "23514")
            ),
            "{name} must be refused by the guarded command: {error}"
        );
        assert_eq!(
            refused_state(&pool, &fixture).await,
            before,
            "{name} must leave every durable result and lifecycle fact unchanged"
        );
        assert_refused_without_durable_result(&pool, &fixture).await;
        fixture.runtime.close().await;
    }
}

#[sqlx::test(migrations = false)]
async fn result_preparation_requires_its_exact_allowed_delivery_policy(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    for (name, policy) in [
        (
            "missing exact cause with a policy row for another cause",
            DeliveryPolicyCase::WrongCause,
        ),
        ("wrong delivery attempt", DeliveryPolicyCase::WrongAttempt),
        (
            "wrong response input count",
            DeliveryPolicyCase::WrongInputCount,
        ),
        ("denied delivery verdict", DeliveryPolicyCase::Denied),
    ] {
        let fixture = result_fixture_with_policy(&pool, policy).await;
        let before = refused_state(&pool, &fixture).await;
        assert_refused_without_durable_result(&pool, &fixture).await;
        let error = try_commit_result(
            &fixture,
            Uuid::now_v7(),
            Uuid::now_v7(),
            &exact_attempt(&fixture),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error
                .as_database_error()
                .and_then(|database| database.code())
                .as_deref(),
            Some("23514"),
            "{name} must not authorize a delivery result"
        );
        assert_eq!(
            refused_state(&pool, &fixture).await,
            before,
            "{name} must leave no partial result and no lifecycle or space advancement"
        );
        assert_refused_without_durable_result(&pool, &fixture).await;
        fixture.runtime.close().await;
    }
}
