//! Runtime-role schema evidence for the delivery ResultPrepared authority.
//!
//! Production-shaped Task 14D evidence for the guarded ResultPrepared
//! authority. It uses the Task 14C output fixture only to establish the prior
//! accepted-key boundary, then proves this task's runtime-role commit, replay,
//! refusal and immutable-result behavior without direct result-table DML.

mod common;
use common::result_preparation_fixture::*;

use chrono::Utc;
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;
use vestrace_application::{
    EmbeddingJobRepository, EmbeddingOutputKeyRepository, PreDispatchTerminalState,
    PreDispatchTerminationEvidence, RequestEmbeddingOutputRetirement,
    TerminateEmbeddingJobPreDispatch,
};
use vestrace_domain::{
    AuthorizationRequest, Capability, PolicyDecision, PolicyDecisionReason, PolicyDecisionResult,
    PolicyInputState, ResourceScope, RiskCategory, id::PolicyDecisionId,
};
use vestrace_infrastructure::postgres::{
    PgEmbeddingJobRepository, PgEmbeddingOutputKeyRepository, PgStore,
};

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
    let (expected_intent, historical_blocker, intent_state, blocker_kind, blocker_state): (
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
          AND blocker.id=$3",
    )
    .bind(fixture.accepted.context.workspace_id.as_uuid())
    .bind(fixture.accepted.snapshot_id)
    .bind(pinned.completion_blocker_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(intent_state, "active");
    assert_eq!(historical_blocker, pinned.completion_blocker_id);
    assert_eq!(blocker_kind, "effect");
    assert_eq!(blocker_state, "nonterminal");

    let preparation = Uuid::now_v7();
    assert_eq!(
        commit_result(&fixture, preparation, Uuid::now_v7()).await,
        preparation,
        "the guarded result command must accept the active credential branch"
    );
    let expected_blocker: Uuid = sqlx::query_scalar(
        "SELECT blocker_id FROM embedding_job_credential_completion_blockers WHERE workspace_id=$1 AND job_id=$2 AND external_effect_id=$3 AND model_binding_snapshot_id=$4 AND credential_revision_id=$5 AND credential_intent_id=$6",
    ).bind(fixture.accepted.context.workspace_id.as_uuid())
     .bind(fixture.accepted.job_id.as_uuid()).bind(fixture.accepted.external_effect_id)
     .bind(fixture.accepted.snapshot_id).bind(pinned.revision_id).bind(expected_intent)
     .fetch_one(&pool).await.unwrap();
    assert_ne!(
        expected_blocker, historical_blocker,
        "new preparation owns fresh protection without adopting an unrelated blocker"
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
    // Corruption injection bypasses both historical and forward write guards;
    // the unchanged runtime loader assertion below remains the independent oracle.
    sqlx::query("ALTER TABLE embedding_projection_source_dependencies DISABLE TRIGGER embedding_projection_source_dependencies_finalization_complete").execute(&mut *owner).await.unwrap();
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
    sqlx::query("ALTER TABLE embedding_projection_source_dependencies ENABLE TRIGGER embedding_projection_source_dependencies_finalization_complete").execute(&mut *owner).await.unwrap();
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
    // Corruption injection bypasses both historical and forward write guards;
    // the unchanged runtime loader assertion below remains the independent oracle.
    sqlx::query("ALTER TABLE embedding_projection_entries DISABLE TRIGGER embedding_projection_entries_finalization_complete").execute(&mut *owner).await.unwrap();
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
    owner.commit().await.unwrap();

    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture.accepted.context.workspace_id.as_uuid()).await;
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
    sqlx::query("ALTER TABLE embedding_projection_entries ENABLE TRIGGER embedding_projection_entries_finalization_complete").execute(&mut *owner).await.unwrap();
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
