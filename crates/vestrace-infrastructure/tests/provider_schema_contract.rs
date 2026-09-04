use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

/// Serializes the three tests that arm the process-global retained-result
/// observer.
///
/// `RESULT_CONTENT_POINTER` and its companions are one set of statics, and
/// libtest runs these tests on parallel threads. Whichever test stored the
/// pointer last was the only one the allocator hook could see, so the others
/// observed no deallocation and failed with "did not deallocate the exact
/// retained-result allocation" -- a different test each run. The zeroization
/// they prove is real; the evidence was being decided by a race, which made a
/// green run partly luck and a red one uninformative.
///
/// The lock is deliberately taken for the whole arm-run-assert region rather
/// than around each store. Poisoning is absorbed: a failing test must not turn
/// its siblings into misleading poison errors.
/// An async mutex, not a `std` one. The guard is held across every `.await` in
/// the test it serializes, and a blocking guard held that way makes the future
/// non-`Send` and can deadlock an executor that moves the task. `tokio`'s mutex
/// yields instead of blocking, and it cannot be poisoned, so a failing test
/// releases it cleanly rather than turning its sibling into a poison error.
static RETAINED_RESULT_OBSERVER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn observe_retained_result() -> tokio::sync::MutexGuard<'static, ()> {
    RETAINED_RESULT_OBSERVER.lock().await
}

struct ProviderResultObservingAllocator;

static RESULT_CONTENT_POINTER: AtomicUsize = AtomicUsize::new(0);
static RESULT_CONTENT_LEN: AtomicUsize = AtomicUsize::new(0);
static RESULT_CONTENT_OBSERVED: AtomicBool = AtomicBool::new(false);
static RESULT_CONTENT_ZERO: AtomicBool = AtomicBool::new(false);

// SAFETY: allocation/deallocation are forwarded unchanged. The observer reads
// only the exact retained-result allocation before forwarding its deallocation
// and performs fixed atomic writes without allocating or logging.
unsafe impl GlobalAlloc for ProviderResultObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if RESULT_CONTENT_POINTER.load(Ordering::SeqCst) == pointer as usize {
            let initialized_len = RESULT_CONTENT_LEN.load(Ordering::SeqCst);
            let mut all_zero = initialized_len <= layout.size();
            for offset in 0..initialized_len.min(layout.size()) {
                if unsafe { pointer.add(offset).read() } != 0 {
                    all_zero = false;
                    break;
                }
            }
            RESULT_CONTENT_ZERO.store(all_zero, Ordering::SeqCst);
            RESULT_CONTENT_OBSERVED.store(true, Ordering::SeqCst);
            RESULT_CONTENT_POINTER.store(0, Ordering::SeqCst);
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: ProviderResultObservingAllocator = ProviderResultObservingAllocator;

use chrono::Utc;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ArtifactContent, ArtifactRepository, ConnectionAuth, ConnectionKind,
    EffectiveChatEvidence, EffectiveChatFinishReason, EffectiveChatMessage, EffectiveChatResult,
    EffectiveModelRequest, EffectiveModelResponse, EffectiveRequestLimits, EffectiveSampling,
    ExternalEffectRepository, FinalizeProviderResult, InstallationMutationPermit, MaterialKeyVault,
    ModelDataPolicyDecisionRecord, ModelDataPolicyDecisionRepository, ModelDataPolicyMode,
    PermitMode, PrepareProviderResult, ProviderDispatchAuthority, ProviderResultFaultInjector,
    ProviderResultFaultPoint, ProviderResultIdentities, ProviderResultRepository, ProviderUsage,
    RequestContext, UnitOfWork, VaultError,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, ExternalEffectIntent,
    IdempotencyProfile,
};
use vestrace_domain::{
    AgentRunId, ArtifactId, ArtifactRevisionId, Capability, ConnectionId, ConnectionRevisionId,
    ContentMaterialId, DataDestination, ErasureReceipt, ExternalEffectId,
    ExternalEffectLifecycleTransitionId, ExternalEffectReceiptId, IntentNonce,
    MaterialKeyBindingReceipt, MaterialKeyCreationIntentId, MaterialKeyId, ModelExecutionId,
    PolicyDecisionId, PreparedMaterialAttachmentId, PrincipalId, RiskCategory, RunStepId,
    Sensitivity, VaultReceipt, WorkItemId, WorkerId, WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{
    OpenAiCompatibleClient, PgArtifactRepository, PgExternalEffectRepository,
    PgInstallationMutationPermit, PgModelDataPolicyDecisionRepository, PgProviderResultRepository,
    PgScopedTransaction, PgStore, PostgresRunStore, crypto::ContentMaterialCodec,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";
const GUARDED_OWNER: &str = "vestrace_guarded_owner";
// Keep the integration test binary tied to the complete migrations directory.

const P03_GUARDED_TABLES: [&str; 38] = [
    "connection_revision_heads",
    "connection_revisions",
    "no_auth_binding_revisions",
    "qualification_jobs",
    "qualification_target_bindings",
    "qualification_probe_results",
    "connection_qualification_revisions",
    "connection_qualification_heads",
    "model_revision_heads",
    "model_revisions",
    "model_qualification_revisions",
    "model_qualification_heads",
    "workspace_model_defaults",
    "model_binding_snapshots",
    "run_model_binding_snapshots",
    "model_request_evidence_roots",
    "model_request_evidence_nodes",
    "model_request_evidence_checks",
    "model_request_shape_revisions",
    "model_sampling_revisions",
    "model_limits_revisions",
    "model_tool_schema_revisions",
    "model_request_evidence_check_missing_references",
    "connection_admission_policy_heads",
    "connection_admission_policy_revisions",
    "connection_admission_states",
    "connection_dispatch_admissions",
    "provider_admission_waits",
    "provider_concurrency_leases",
    "provider_throttle_observations",
    "credential_dispatch_leases",
    "credential_activation_events",
    "credential_rotation_events",
    "provider_result_preparations",
    "provider_result_publications",
    "artifact_revision_contents",
    "provider_dispatch_causes",
    "run_step_execution_attempts",
];

const P03_RUNTIME_ENTRYPOINTS: [&str; 30] = [
    "vestrace_create_connection_revision_and_advance_head(uuid,uuid,uuid,uuid,text,text,text,text,text,text,uuid,bigint)",
    "vestrace_create_no_auth_binding_revision(uuid,uuid,uuid,uuid)",
    "vestrace_create_model_revision_and_advance_head(uuid,uuid,uuid,uuid,uuid,uuid,text,text,integer,uuid,text,integer,uuid,text,bigint)",
    "vestrace_set_workspace_model_default(uuid,uuid,text,uuid,text[],bigint)",
    "vestrace_create_run_model_binding_snapshot(uuid,uuid,uuid,text)",
    "vestrace_finalize_provider_result(uuid,bytea)",
    "vestrace_prepare_candidate_abandon_and_erasure(uuid,bigint)",
    "vestrace_prepare_provider_result(uuid,uuid,uuid,uuid,uuid,uuid,bytea,bigint)",
    "vestrace_witness_provider_result_receipt(uuid,uuid)",
    "vestrace_try_admit_provider_dispatch(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid,text,integer)",
    "vestrace_release_provider_dispatch(uuid,uuid,uuid)",
    "vestrace_record_provider_throttle(uuid,uuid,uuid,uuid,integer)",
    "vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)",
    "vestrace_lock_provider_dispatch_routing(uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid)",
    "vestrace_lock_model_request_evidence_for_reconstruction(uuid,uuid)",
    "vestrace_prepare_retired_or_revoked_credential_erasure(uuid)",
    "vestrace_activate_first_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_rotate_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_revoke_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_issue_credential_dispatch_lease(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,text,timestamp with time zone)",
    "vestrace_consume_credential_dispatch_lease(uuid,uuid,uuid,uuid)",
    "vestrace_create_model_request_shape_revision(uuid,uuid,bigint,text,boolean,text[])",
    "vestrace_create_model_sampling_revision(uuid,uuid,bigint,double precision,double precision)",
    "vestrace_create_model_limits_revision(uuid,uuid,bigint,integer,integer,integer)",
    "vestrace_create_model_tool_schema_revision(uuid,uuid,bigint,text,text,jsonb)",
    "vestrace_create_model_request_evidence(uuid,uuid,uuid,text,uuid,uuid,text,uuid,text[],uuid[],bigint[],text[])",
    "vestrace_append_model_request_evidence_check(uuid,uuid,uuid,text,text[],uuid[],uuid[],uuid[])",
    "vestrace_reserve_run_step_execution_attempt(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid)",
    "vestrace_transition_run_step_execution_attempt(uuid,uuid,uuid,text)",
    "vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)",
];

const P03_GUARDED_FUNCTIONS: [&str; 37] = [
    "vestrace_create_connection_revision_and_advance_head(uuid,uuid,uuid,uuid,text,text,text,text,text,text,uuid,bigint)",
    "vestrace_create_no_auth_binding_revision(uuid,uuid,uuid,uuid)",
    "vestrace_create_model_revision_and_advance_head(uuid,uuid,uuid,uuid,uuid,uuid,text,text,integer,uuid,text,integer,uuid,text,bigint)",
    "vestrace_set_workspace_model_default(uuid,uuid,text,uuid,text[],bigint)",
    "vestrace_create_run_model_binding_snapshot(uuid,uuid,uuid,text)",
    "vestrace_finalize_provider_result(uuid,bytea)",
    "vestrace_prepare_candidate_abandon_and_erasure(uuid,bigint)",
    "vestrace_prepare_provider_result(uuid,uuid,uuid,uuid,uuid,uuid,bytea,bigint)",
    "vestrace_witness_provider_result_receipt(uuid,uuid)",
    "vestrace_try_admit_provider_dispatch(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid,text,integer)",
    "vestrace_release_provider_dispatch(uuid,uuid,uuid)",
    "vestrace_record_provider_throttle(uuid,uuid,uuid,uuid,integer)",
    "vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)",
    "vestrace_lock_provider_dispatch_routing(uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid)",
    "vestrace_lock_model_request_evidence_for_reconstruction(uuid,uuid)",
    "vestrace_prepare_retired_or_revoked_credential_erasure(uuid)",
    "vestrace_activate_first_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_rotate_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_revoke_credential(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bigint)",
    "vestrace_issue_credential_dispatch_lease(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,text,timestamp with time zone)",
    "vestrace_consume_credential_dispatch_lease(uuid,uuid,uuid,uuid)",
    "vestrace_create_model_request_shape_revision(uuid,uuid,bigint,text,boolean,text[])",
    "vestrace_create_model_sampling_revision(uuid,uuid,bigint,double precision,double precision)",
    "vestrace_create_model_limits_revision(uuid,uuid,bigint,integer,integer,integer)",
    "vestrace_create_model_tool_schema_revision(uuid,uuid,bigint,text,text,jsonb)",
    "vestrace_create_model_request_evidence(uuid,uuid,uuid,text,uuid,uuid,text,uuid,text[],uuid[],bigint[],text[])",
    "vestrace_append_model_request_evidence_check(uuid,uuid,uuid,text,text[],uuid[],uuid[],uuid[])",
    "vestrace_reject_credential_dispatch_lease_state_rewrite()",
    "vestrace_reject_p03_immutable_mutation()",
    "vestrace_reject_raw_p03_mutation()",
    "vestrace_validate_connection_revision_identity()",
    "vestrace_validate_provider_result_completion()",
    "vestrace_validate_task10_deferred_contract()",
    "vestrace_validate_provider_result_material_live()",
    "vestrace_reserve_run_step_execution_attempt(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid)",
    "vestrace_transition_run_step_execution_attempt(uuid,uuid,uuid,text)",
    "vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)",
];

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let username = parsed.get_username().to_owned();
    assert_eq!(username, "vestrace");
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
                .username(&username)
                .password(password),
        )
        .await
        .expect("the restricted runtime role must connect to the SQLx test database")
}

fn model_policy_record(id: Uuid, reason: &str) -> ModelDataPolicyDecisionRecord {
    ModelDataPolicyDecisionRecord {
        id,
        run_id: AgentRunId::new(),
        step_id: RunStepId::new(),
        destination: DataDestination::RemoteProvider,
        classification: Sensitivity::Confidential,
        allowed: true,
        reason: reason.to_owned(),
        policy_version: "model-data-v1".to_owned(),
        mode: ModelDataPolicyMode::Enforce,
        decided_at: chrono::Utc::now(),
    }
}

async fn call_model_policy_guard(
    pool: &PgPool,
    record: &ModelDataPolicyDecisionRecord,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT public.vestrace_record_model_data_policy_decision(
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(record.id)
    .bind(record.run_id.as_uuid())
    .bind(record.step_id.as_uuid())
    .bind("remote_provider")
    .bind("confidential")
    .bind(if record.allowed { "allowed" } else { "denied" })
    .bind(&record.reason)
    .bind(&record.policy_version)
    .bind("enforce")
    .bind(record.decided_at)
    .fetch_one(pool)
    .await
}

fn valid_framed_ciphertext(size: usize, fill: u8) -> Vec<u8> {
    assert!((4096..=1_048_576).contains(&size));
    assert!(size.is_power_of_two());
    let mut ciphertext = vec![fill; size];
    ciphertext[..5].copy_from_slice(b"VMRF\x01");
    ciphertext
}

fn provider_result_receipt_payload(
    preparation_id: Uuid,
    advance_work_item_id: Uuid,
    finish_reason: &str,
    usage: Option<(i32, i32)>,
) -> serde_json::Value {
    let (usage_known, prompt_tokens, completion_tokens) = match usage {
        Some((prompt_tokens, completion_tokens)) => {
            (true, Some(prompt_tokens), Some(completion_tokens))
        }
        None => (false, None, None),
    };
    serde_json::json!({
        "response_class": "http_200",
        "evidence_refs": [format!("provider_result_preparation:{preparation_id}")],
        "provider_result_recovery": {
            "advance_work_item_id": advance_work_item_id,
            "finish_reason": finish_reason,
            "usage_known": usage_known,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
        }
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_p03_table_is_guarded_forced_rls_and_has_a_nonempty_acl(pool: PgPool) {
    let rows = sqlx::query(
        "SELECT c.relname, pg_get_userbyid(c.relowner) AS owner, \
                c.relrowsecurity, c.relforcerowsecurity, c.relacl IS NOT NULL AS has_acl \
           FROM pg_class AS c \
           JOIN pg_namespace AS n ON n.oid = c.relnamespace \
          WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname = ANY($1) \
          ORDER BY c.relname",
    )
    .bind(P03_GUARDED_TABLES.as_slice())
    .fetch_all(&pool)
    .await
    .expect("the P03 guarded table catalog must be queryable");

    assert_eq!(
        rows.len(),
        P03_GUARDED_TABLES.len(),
        "all declared P03 tables must exist; missing objects are a 42P01 product failure"
    );
    for row in rows {
        let table: String = row.get("relname");
        assert_eq!(row.get::<String, _>("owner"), GUARDED_OWNER, "{table}");
        assert!(
            row.get::<bool, _>("relrowsecurity"),
            "{table} must enable RLS"
        );
        assert!(
            row.get::<bool, _>("relforcerowsecurity"),
            "{table} must force RLS"
        );
        assert!(
            row.get::<bool, _>("has_acl"),
            "{table} must retain an explicit ACL"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn deployment_policy_decisions_have_exact_guarded_authority(pool: PgPool) {
    let row: (String, bool, Option<Vec<String>>, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(procedure.proowner), procedure.prosecdef, procedure.proconfig,
                position('EXECUTE' in upper(procedure.prosrc)) = 0,
                position('format(' in procedure.prosrc) = 0
           FROM pg_proc AS procedure
          WHERE procedure.oid = 'public.vestrace_record_model_data_policy_decision(
              uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone
          )'::regprocedure",
    )
    .fetch_one(&pool)
    .await
    .expect("the exact guarded model-data-policy entrypoint must exist");
    assert_eq!(row.0, GUARDED_OWNER);
    assert!(row.1, "the entrypoint must be SECURITY DEFINER");
    assert_eq!(row.2, Some(vec!["search_path=pg_catalog".to_owned()]));
    assert!(
        row.3 && row.4,
        "the guarded entrypoint must contain no dynamic SQL"
    );

    let table: (String, bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(relation.relowner), relation.relrowsecurity,
                relation.relforcerowsecurity,
                has_table_privilege('vestrace','public.model_data_policy_decisions','SELECT'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','REFERENCES'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','INSERT'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','UPDATE'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','DELETE')
           FROM pg_class AS relation
          WHERE relation.oid='public.model_data_policy_decisions'::regclass",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        table,
        (
            GUARDED_OWNER.to_owned(),
            false,
            false,
            true,
            false,
            false,
            false,
            false
        )
    );

    let function_acl: (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege('vestrace',
             'public.vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)',
             'EXECUTE'),
             has_function_privilege('public',
             'public.vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)',
             'EXECUTE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(function_acl, (true, false));
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_policy_repository_and_guard_replay_are_exact(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(runtime.clone());
    let repository = PgModelDataPolicyDecisionRepository::new(store.clone());
    let legacy = model_policy_record(Uuid::now_v7(), "legacy wrapper");
    repository.record(&legacy).await.unwrap();
    repository.record(&legacy).await.unwrap();

    let transaction_bound = model_policy_record(Uuid::now_v7(), "caller transaction");
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut transaction = store.begin_scoped(&context).await.unwrap();
    repository
        .record_in(&mut transaction, &transaction_bound)
        .await
        .unwrap();
    Box::new(transaction).commit().await.unwrap();

    let unequal = ModelDataPolicyDecisionRecord {
        reason: "unequal replay".to_owned(),
        ..legacy.clone()
    };
    let error = call_model_policy_guard(&runtime, &unequal)
        .await
        .expect_err("an unequal same-id replay must be refused");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,reason FROM model_data_policy_decisions WHERE id=ANY($1) ORDER BY id",
    )
    .bind(vec![legacy.id, transaction_bound.id])
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&(legacy.id, "legacy wrapper".to_owned())));
    assert!(rows.contains(&(transaction_bound.id, "caller transaction".to_owned())));
}

#[sqlx::test(migrations = "../../migrations")]
async fn model_policy_guard_same_id_races_are_serialized(pool: PgPool) {
    let first = runtime_pool(&pool).await;
    let second = runtime_pool(&pool).await;
    let identical = model_policy_record(Uuid::now_v7(), "identical race");
    let (left, right) = tokio::join!(
        call_model_policy_guard(&first, &identical),
        call_model_policy_guard(&second, &identical),
    );
    assert_eq!(left.unwrap(), identical.id);
    assert_eq!(right.unwrap(), identical.id);
    let identical_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM model_data_policy_decisions WHERE id=$1")
            .bind(identical.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identical_count, 1);

    let conflict_id = Uuid::now_v7();
    let alpha = model_policy_record(conflict_id, "alpha winner");
    let bravo = ModelDataPolicyDecisionRecord {
        reason: "bravo winner".to_owned(),
        ..alpha.clone()
    };
    let (alpha_result, bravo_result) = tokio::join!(
        call_model_policy_guard(&first, &alpha),
        call_model_policy_guard(&second, &bravo),
    );
    let outcomes = [alpha_result, bravo_result];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    let refusal = outcomes
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("one conflicting racer must be refused");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    let persisted: (i64, String) =
        sqlx::query_as("SELECT COUNT(*), MIN(reason) FROM model_data_policy_decisions WHERE id=$1")
            .bind(conflict_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted.0, 1);
    assert!(persisted.1 == "alpha winner" || persisted.1 == "bravo winner");
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_constraints_pin_auth_xor_and_cross_workspace_identity(pool: PgPool) {
    let definitions: Vec<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(con.oid) \
           FROM pg_constraint AS con \
          WHERE con.conrelid IN ( \
              'public.connection_revisions'::regclass, \
              'public.qualification_target_bindings'::regclass, \
              'public.model_binding_snapshots'::regclass \
          ) \
          ORDER BY con.conname",
    )
    .fetch_all(&pool)
    .await
    .expect("P03 identity constraints must exist");
    let normalized = definitions
        .join("\n")
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();

    assert!(
        normalized.contains("auth_mode")
            && normalized.contains("credential_slot_id")
            && normalized.contains("no_auth_binding_revision_id"),
        "database constraints must enforce the credential-versus-no-auth XOR"
    );
    assert!(
        normalized.matches("foreignkey(workspace_id").count() >= 5,
        "composite foreign keys must make workspace identity parity structural"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_schema_contains_only_ciphertext_and_safe_material_metadata(pool: PgPool) {
    let columns: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT table_name, column_name, data_type \
           FROM information_schema.columns \
          WHERE table_schema = 'public' \
            AND table_name = ANY($1) \
          ORDER BY table_name, ordinal_position",
    )
    .bind([
        "provider_result_preparations",
        "provider_result_publications",
        "artifact_revision_contents",
    ])
    .fetch_all(&pool)
    .await
    .expect("provider result columns must be inspectable");
    assert!(!columns.is_empty(), "provider result tables must exist");

    let forbidden = [
        "raw",
        "prompt",
        "response_body",
        "payload",
        "content_hash",
        "digest",
        "byte_size",
        "plaintext",
        "exact_length",
    ];
    for (table, column, data_type) in &columns {
        if table == "provider_result_preparations"
            && matches!(
                column.as_str(),
                "advance_work_item_id"
                    | "finish_reason"
                    | "usage_known"
                    | "prompt_tokens"
                    | "completion_tokens"
            )
        {
            assert_ne!(data_type, "bytea");
            assert_ne!(data_type, "jsonb");
            continue;
        }
        if (table == "provider_result_preparations" && column == "ciphertext")
            || (table == "artifact_revision_contents" && column == "erasure_bound_commitment")
        {
            assert_eq!(data_type, "bytea");
            continue;
        }
        assert_ne!(
            data_type, "bytea",
            "{table}.{column} is an undeclared byte store"
        );
        assert_ne!(
            data_type, "jsonb",
            "{table}.{column} is a forbidden generic payload"
        );
        assert!(
            forbidden.iter().all(|needle| !column.contains(needle)),
            "{table}.{column} exposes forbidden provider-result metadata"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn task10_dispatch_and_governed_artifact_contract_is_database_visible(pool: PgPool) {
    let dispatch_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
           WHERE table_schema='public' AND table_name='provider_dispatch_causes' \
           ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .expect("provider dispatch cause columns must be inspectable");
    assert_eq!(
        dispatch_columns,
        [
            "external_effect_id",
            "workspace_id",
            "model_request_evidence_id",
            "model_request_evidence_check_id",
            "cause_kind",
            "run_id",
            "step_id",
            "model_binding_snapshot_id",
            "qualification_job_id",
            "qualification_target_binding_id",
            "qualification_probe_ordinal",
            "created_at",
            // P04 widened this tuple by one column so that an embedding job can
            // be a dispatch cause instead of growing its own admission tables.
            // Spec line 219 makes it the third owner of an external effect and a
            // ModelRequestEvidence identity; the closed matrix constraint keeps
            // it exclusive with the Run and probe branches. The list stays exact
            // so a fourth column still has to be argued for here.
            "embedding_job_id",
        ],
        "0184 and 0187 together must expose only the closed normalized          dispatch-cause tuple"
    );

    let artifact_columns: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT column_name, is_nullable, column_default \
           FROM information_schema.columns \
          WHERE table_schema='public' AND table_name='artifact_revisions' \
            AND column_name IN ('storage_kind','content_hash','byte_size') \
          ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("artifact revision storage columns must be inspectable");
    assert_eq!(artifact_columns.len(), 3);
    assert!(artifact_columns.iter().any(|(name, nullable, default)| {
        name == "storage_kind"
            && nullable == "NO"
            && default.as_deref() == Some("'legacy_digest'::text")
    }));
    assert!(
        artifact_columns
            .iter()
            .any(|(name, nullable, _)| { name == "content_hash" && nullable == "YES" })
    );
    assert!(
        artifact_columns
            .iter()
            .any(|(name, nullable, _)| { name == "byte_size" && nullable == "YES" })
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn p03_entrypoints_are_exact_guarded_and_runtime_executable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let rows = sqlx::query(
        "SELECT p.oid::regprocedure::text AS signature, \
                pg_get_userbyid(p.proowner) AS owner, \
                p.prosecdef, \
                COALESCE(array_to_string(p.proconfig, ','), '') AS config, \
                has_function_privilege('vestrace', p.oid, 'EXECUTE') AS runtime_execute \
           FROM pg_proc AS p \
           JOIN pg_namespace AS n ON n.oid = p.pronamespace \
          WHERE n.nspname = 'public' AND p.oid::regprocedure::text = ANY($1) \
          ORDER BY p.oid::regprocedure::text",
    )
    .bind(P03_RUNTIME_ENTRYPOINTS.as_slice())
    .fetch_all(&pool)
    .await
    .expect("P03 function metadata must be inspectable");
    runtime.close().await;

    assert_eq!(rows.len(), P03_RUNTIME_ENTRYPOINTS.len());
    let actual: BTreeSet<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("signature"))
        .collect();
    assert_eq!(actual, P03_RUNTIME_ENTRYPOINTS.map(str::to_owned).into());
    for row in rows {
        let signature: String = row.get("signature");
        assert_eq!(row.get::<String, _>("owner"), GUARDED_OWNER, "{signature}");
        assert!(row.get::<bool, _>("prosecdef"), "{signature}");
        let expected_search_path =
            if signature.starts_with("vestrace_record_model_data_policy_decision(") {
                "search_path=pg_catalog"
            } else {
                "search_path=public, pg_temp"
            };
        assert_eq!(
            row.get::<String, _>("config"),
            expected_search_path,
            "{signature} must pin its search path"
        );
        assert!(row.get::<bool, _>("runtime_execute"), "{signature}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn reconstruction_lock_ownership_and_legacy_intent_acl_are_exact(pool: PgPool) {
    let locked_relations = [
        "model_request_evidence_roots",
        "model_binding_snapshots",
        "qualification_target_bindings",
        "model_request_evidence_nodes",
        "connection_revisions",
        "connection_qualification_revisions",
        "model_revisions",
        "model_qualification_revisions",
        "model_request_shape_revisions",
        "model_sampling_revisions",
        "model_limits_revisions",
        "model_tool_schema_revisions",
        "content_materials",
        "material_key_creation_intents",
        "content_material_bytes",
    ];
    for relation in locked_relations {
        let owner: String = sqlx::query_scalar(
            "SELECT pg_get_userbyid(relation.relowner)
               FROM pg_class AS relation
               JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
              WHERE namespace.nspname='public' AND relation.relname=$1",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner, GUARDED_OWNER, "{relation}");
    }
    let (intent_owner, migration_owner): (String, String) = sqlx::query_as(
        "SELECT pg_get_userbyid(relation.relowner), current_user::text
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
          WHERE namespace.nspname='public' AND relation.relname='external_effect_intents'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(intent_owner, migration_owner);
    assert_ne!(intent_owner, GUARDED_OWNER);
    let guarded_acl: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','SELECT'),
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','INSERT'),
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','UPDATE'),
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','DELETE'),
            has_column_privilege('vestrace_guarded_owner','public.external_effect_intents','id','UPDATE'),
            has_column_privilege('vestrace_guarded_owner','public.external_effect_intents','workspace_id','UPDATE'),
            has_column_privilege('vestrace_guarded_owner','public.external_effect_intents','payload','UPDATE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(guarded_acl, (true, false, false, false, true, false, false));

    let runtime_bytes_acl: (bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_table_privilege('vestrace','public.content_material_bytes','SELECT'),
            has_table_privilege('vestrace','public.content_material_bytes','REFERENCES'),
            has_table_privilege('vestrace','public.content_material_bytes','INSERT'),
            has_table_privilege('vestrace','public.content_material_bytes','UPDATE'),
            has_table_privilege('vestrace','public.content_material_bytes','DELETE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(runtime_bytes_acl, (true, false, false, false, false));

    let runtime_credential_frame_acl: (
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
    ) =
        sqlx::query_as(
        "SELECT
            has_column_privilege('vestrace','public.credential_prepared_materials','workspace_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','intent_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','credential_revision_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','ciphertext','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','created_at','SELECT'),
            has_column_privilege('vestrace','public.credential_revisions','associated_data_profile','SELECT'),
            has_column_privilege('vestrace','public.credential_key_creation_intents','nonce','SELECT'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','SELECT'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','UPDATE'),
            has_table_privilege('vestrace','public.credential_prepared_materials','UPDATE'),
            has_table_privilege('vestrace','public.credential_prepared_materials','DELETE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        runtime_credential_frame_acl,
        (
            true, true, true, true, false, false, true, true, true, false, false, false
        ),
        "runtime may read only the exact immutable credential frame tuple"
    );
    let runtime_credential_material_table_acl: (bool, bool, bool, bool, bool, bool, bool) =
        sqlx::query_as(
            "SELECT
                has_table_privilege('vestrace','public.credential_prepared_materials','SELECT'),
                has_table_privilege('vestrace','public.credential_prepared_materials','INSERT'),
                has_table_privilege('vestrace','public.credential_prepared_materials','UPDATE'),
                has_table_privilege('vestrace','public.credential_prepared_materials','DELETE'),
                has_table_privilege('vestrace','public.credential_prepared_materials','TRUNCATE'),
                has_table_privilege('vestrace','public.credential_prepared_materials','REFERENCES'),
                has_table_privilege('vestrace','public.credential_prepared_materials','TRIGGER')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        runtime_credential_material_table_acl,
        (false, false, false, false, false, false, false)
    );
    let runtime_credential_attachment_table_acl: (bool, bool, bool, bool, bool, bool, bool) =
        sqlx::query_as(
            "SELECT
                has_table_privilege('vestrace','public.credential_prepared_attachments','SELECT'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','INSERT'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','UPDATE'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','DELETE'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','TRUNCATE'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','REFERENCES'),
                has_table_privilege('vestrace','public.credential_prepared_attachments','TRIGGER')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        runtime_credential_attachment_table_acl,
        (true, false, false, false, false, false, false)
    );
    for relation in [
        "public.credential_prepared_materials",
        "public.credential_prepared_attachments",
    ] {
        let runtime_column_mutation_acl_is_empty: bool = sqlx::query_scalar(
            "SELECT bool_and(
                 NOT has_column_privilege('vestrace',$1,column_name,'INSERT')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'UPDATE')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'REFERENCES'))
               FROM information_schema.columns
              WHERE table_schema='public' AND table_name=split_part($1,'.',2)",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            runtime_column_mutation_acl_is_empty,
            "runtime retained a credential frame column mutation privilege on {relation}"
        );
        let public_acl: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT
                has_table_privilege('public',$1,'SELECT'),
                has_table_privilege('public',$1,'INSERT'),
                has_table_privilege('public',$1,'UPDATE'),
                has_table_privilege('public',$1,'DELETE'),
                has_table_privilege('public',$1,'TRUNCATE'),
                has_table_privilege('public',$1,'REFERENCES'),
                has_table_privilege('public',$1,'TRIGGER')",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            public_acl,
            (false, false, false, false, false, false, false),
            "PUBLIC retained a credential frame privilege on {relation}"
        );
        let public_column_acl_is_empty: bool = sqlx::query_scalar(
            "SELECT bool_and(
                 NOT has_column_privilege('public',$1,column_name,'SELECT')
                 AND NOT has_column_privilege('public',$1,column_name,'INSERT')
                 AND NOT has_column_privilege('public',$1,column_name,'UPDATE')
                 AND NOT has_column_privilege('public',$1,column_name,'REFERENCES'))
               FROM information_schema.columns
              WHERE table_schema='public' AND table_name=split_part($1,'.',2)",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            public_column_acl_is_empty,
            "PUBLIC retained a credential frame column privilege on {relation}"
        );
    }

    let authority: (String, bool, String, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(procedure.proowner), procedure.prosecdef,
                COALESCE(array_to_string(procedure.proconfig,','),''),
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'),
                has_function_privilege('public',procedure.oid,'EXECUTE')
           FROM pg_proc AS procedure
          WHERE procedure.oid='public.vestrace_lock_model_request_evidence_for_reconstruction(uuid,uuid)'::regprocedure",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        authority,
        (
            GUARDED_OWNER.into(),
            true,
            "search_path=public, pg_temp".into(),
            true,
            false
        )
    );

    let trigger: (String, i16) = sqlx::query_as(
        "SELECT trigger.tgname, trigger.tgtype
           FROM pg_trigger AS trigger
          WHERE trigger.tgrelid='public.external_effect_intents'::regclass
            AND trigger.tgname='external_effect_intents_immutable'
            AND NOT trigger.tgisinternal",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trigger.0, "external_effect_intents_immutable");
    assert_ne!(trigger.1 & 8, 0, "trigger must cover DELETE");
    assert_ne!(trigger.1 & 16, 0, "trigger must cover UPDATE");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_catalog_projection_acl_is_exactly_column_scoped(pool: PgPool) {
    let model_projection_acl: (
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
    ) = sqlx::query_as(
        "SELECT
            has_column_privilege('vestrace','public.models','id','SELECT'),
            has_column_privilege('vestrace','public.models','workspace_id','SELECT'),
            has_column_privilege('vestrace','public.models','provider_id','SELECT'),
            has_column_privilege('vestrace','public.models','created_at','SELECT'),
            has_column_privilege('vestrace','public.models','model_name','SELECT'),
            has_column_privilege('vestrace','public.models','context_window','SELECT'),
            has_column_privilege('vestrace','public.models','input_cost_per_mtoken','SELECT'),
            has_column_privilege('vestrace','public.models','output_cost_per_mtoken','SELECT'),
            has_table_privilege('vestrace','public.models','SELECT'),
            has_column_privilege('vestrace','public.models','id','INSERT'),
            has_column_privilege('vestrace','public.models','id','UPDATE'),
            has_table_privilege('vestrace','public.models','DELETE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        model_projection_acl,
        (
            true, true, true, true, false, false, false, false, false, false, false, false,
        ),
        "runtime may read only stable model identity columns for a governed projection"
    );

    let provider_projection_acl: (bool, bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
                has_column_privilege('vestrace','public.providers','id','SELECT'),
                has_column_privilege('vestrace','public.providers','workspace_id','SELECT'),
                has_column_privilege('vestrace','public.providers','name','SELECT'),
                has_column_privilege('vestrace','public.providers','locality','SELECT'),
                has_table_privilege('vestrace','public.providers','SELECT'),
                has_column_privilege('vestrace','public.providers','id','INSERT'),
                has_column_privilege('vestrace','public.providers','id','UPDATE'),
                has_table_privilege('vestrace','public.providers','DELETE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        provider_projection_acl,
        (true, true, false, false, false, false, false, false),
        "runtime may not recover provider display or routing data from legacy rows"
    );

    let connection_projection_acl: (bool, bool, bool, bool, bool, bool, bool, bool, bool) =
        sqlx::query_as(
            "SELECT
                has_column_privilege('vestrace','public.connections','id','SELECT'),
                has_column_privilege('vestrace','public.connections','workspace_id','SELECT'),
                has_column_privilege('vestrace','public.connections','created_at','SELECT'),
                has_column_privilege('vestrace','public.connections','connector_id','SELECT'),
                has_column_privilege('vestrace','public.connections','principal_id','SELECT'),
                has_column_privilege('vestrace','public.connections','name','SELECT'),
                has_column_privilege('vestrace','public.connections','status','SELECT'),
                has_table_privilege('vestrace','public.connections','SELECT'),
                has_table_privilege('vestrace','public.connections','DELETE')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        connection_projection_acl,
        (true, true, true, false, false, false, false, false, false),
        "runtime may read only a stable connection identity for a governed projection"
    );

    for relation in ["public.connections", "public.models", "public.providers"] {
        let column_mutation_acl_is_empty: bool = sqlx::query_scalar(
            "SELECT bool_and(
                 NOT has_column_privilege('vestrace',$1,column_name,'INSERT')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'UPDATE')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'REFERENCES'))
               FROM information_schema.columns
              WHERE table_schema='public' AND table_name=split_part($1,'.',2)",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            column_mutation_acl_is_empty,
            "runtime retained a catalog column mutation privilege on {relation}"
        );
        let table_mutation_acl: (bool, bool, bool, bool) = sqlx::query_as(
            "SELECT
                has_table_privilege('vestrace',$1,'INSERT'),
                has_table_privilege('vestrace',$1,'UPDATE'),
                has_table_privilege('vestrace',$1,'DELETE'),
                has_table_privilege('vestrace',$1,'REFERENCES')",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            table_mutation_acl,
            (false, false, false, false),
            "runtime retained a catalog table mutation privilege on {relation}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_routing_and_credential_consume_pin_the_canonical_lock_order(pool: PgPool) {
    let routing: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef(
            'public.vestrace_lock_provider_dispatch_routing(uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid)'::regprocedure
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let routing_chain = routing
        .find("vestrace_acquire_credential_lock_chain")
        .expect("routing must acquire the credential chain when present");
    let routing_root = routing
        .find("vestrace_lock_model_request_evidence_for_reconstruction")
        .expect("routing must enter through the MRE root-first helper");
    let routing_snapshot = routing
        .find("FROM run_model_binding_snapshots")
        .expect("routing must revalidate the run snapshot");
    assert!(routing_chain < routing_root);
    assert!(routing_root < routing_snapshot);

    let consume: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef(
            'public.vestrace_consume_credential_dispatch_lease(uuid,uuid,uuid,uuid)'::regprocedure
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let consume_chain = consume
        .find("vestrace_acquire_credential_lock_chain")
        .expect("consume must acquire the guard chain");
    let consume_lease_lock = consume
        .find("FOR UPDATE OF lease")
        .expect("consume must lock and revalidate the lease after the chain");
    assert!(consume_chain < consume_lease_lock);
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_attempt_recovery_authority_is_guarded_without_runtime_table_reads(pool: PgPool) {
    let function_acl: (bool, String, bool, bool) = sqlx::query_as(
        "SELECT
            to_regprocedure(
                'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)'
            ) IS NOT NULL,
            COALESCE((
                SELECT pg_get_userbyid(proowner)
                  FROM pg_proc
                 WHERE oid=to_regprocedure(
                    'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)'
                 )
            ), ''),
            COALESCE(has_function_privilege(
                'vestrace',
                to_regprocedure(
                    'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)'
                ),
                'EXECUTE'
            ), FALSE),
            COALESCE(has_function_privilege(
                'public',
                to_regprocedure(
                    'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)'
                ),
                'EXECUTE'
            ), FALSE)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(function_acl, (true, GUARDED_OWNER.into(), true, false));

    let migration = include_str!("../../../migrations/0186_provider_execution_wiring.sql");
    for relation in [
        "provider_dispatch_causes",
        "connection_dispatch_admissions",
        "provider_concurrency_leases",
        "external_effect_authorizations",
        "external_effect_lifecycle_transitions",
    ] {
        assert!(
            !migration.lines().any(|line| line.contains("GRANT")
                && line.contains(relation)
                && line.contains("TO vestrace")),
            "0186 must not change runtime table ACLs for recovery relation {relation}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn p03_guarded_function_runtime_execute_set_is_exact(pool: PgPool) {
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT p.oid::regprocedure::text, pg_get_userbyid(p.proowner), \
                has_function_privilege('vestrace', p.oid, 'EXECUTE'), \
                has_function_privilege('public', p.oid, 'EXECUTE') \
           FROM pg_proc AS p \
           JOIN pg_namespace AS n ON n.oid = p.pronamespace \
          WHERE n.nspname = 'public' AND p.oid::regprocedure::text = ANY($1) \
          ORDER BY p.oid::regprocedure::text",
    )
    .bind(P03_GUARDED_FUNCTIONS.as_slice())
    .fetch_all(&pool)
    .await
    .expect("the exact P03 guarded-function ACL set must be inspectable");

    assert_eq!(rows.len(), P03_GUARDED_FUNCTIONS.len());
    let actual_runtime: BTreeSet<String> = rows
        .iter()
        .filter(|(_, _, runtime_execute, _)| *runtime_execute)
        .map(|(signature, _, _, _)| signature.clone())
        .collect();
    assert_eq!(
        actual_runtime,
        P03_RUNTIME_ENTRYPOINTS.map(str::to_owned).into(),
        "runtime EXECUTE must equal the twenty migration-declared P03 entrypoints"
    );
    for (signature, owner, _, public_execute) in rows {
        assert_eq!(owner, GUARDED_OWNER, "{signature}");
        assert!(!public_execute, "PUBLIC must not execute {signature}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_live_guard_is_internal_fixed_and_deferred(pool: PgPool) {
    let authority: (String, bool, String, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(procedure.proowner), procedure.prosecdef, \
                COALESCE(array_to_string(procedure.proconfig,','),''), \
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                has_function_privilege('public',procedure.oid,'EXECUTE') \
           FROM pg_proc AS procedure \
          WHERE procedure.oid='public.vestrace_validate_provider_result_material_live()'::regprocedure",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        authority,
        (
            GUARDED_OWNER.into(),
            true,
            "search_path=pg_catalog".into(),
            false,
            false,
        )
    );
    let trigger: (bool, bool, i16, String) = sqlx::query_as(
        "SELECT trigger.tgdeferrable, trigger.tginitdeferred, trigger.tgtype, \
                procedure.oid::regprocedure::text \
           FROM pg_trigger AS trigger \
           JOIN pg_proc AS procedure ON procedure.oid=trigger.tgfoid \
          WHERE trigger.tgrelid='public.material_key_creation_intents'::regclass \
            AND trigger.tgname='material_key_creation_intents_provider_result_live' \
            AND NOT trigger.tgisinternal",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(trigger.0);
    assert!(trigger.1);
    assert_ne!(
        trigger.2 & 16,
        0,
        "provider result Live guard must cover UPDATE"
    );
    assert_eq!(
        trigger.3,
        "vestrace_validate_provider_result_material_live()"
    );
    let installer: (String, String, bool, bool, bool, String) = sqlx::query_as(
        "SELECT pg_get_userbyid(procedure.proowner),
                COALESCE(array_to_string(procedure.proconfig,','),''),
                procedure.prosecdef,
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'),
                has_function_privilege('public',procedure.oid,'EXECUTE'),
                procedure.prosrc
           FROM pg_proc AS procedure
          WHERE procedure.oid=to_regprocedure(
              'public.vestrace_install_provider_result_live_trigger()'
          )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let migration_owner: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(installer.0, migration_owner);
    assert_eq!(installer.1, "search_path=pg_catalog");
    assert!(installer.2);
    assert!(!installer.3);
    assert!(!installer.4);
    assert!(
        installer.5.contains(
            "CREATE CONSTRAINT TRIGGER material_key_creation_intents_provider_result_live"
        )
    );
    assert!(!installer.5.contains("format("));
    assert!(!installer.5.contains("EXECUTE IMMEDIATE"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn random_uuid_is_not_an_erasure_bound_provider_result_commitment(pool: PgPool) {
    let commitment_column: (String, String) = sqlx::query_as(
        "SELECT data_type, is_nullable \
           FROM information_schema.columns \
          WHERE table_schema='public' \
            AND table_name='artifact_revision_contents' \
            AND column_name='erasure_bound_commitment'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(commitment_column, ("bytea".into(), "NO".into()));

    let commitment_length_constraint: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(con.oid) \
           FROM pg_constraint AS con \
           JOIN pg_class AS relation ON relation.oid = con.conrelid \
           JOIN pg_namespace AS schema ON schema.oid = relation.relnamespace \
          WHERE schema.nspname='public' \
            AND relation.relname='artifact_revision_contents' \
            AND con.conname='artifact_revision_contents_erasure_bound_commitment_length_check'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        commitment_length_constraint,
        "CHECK ((octet_length(erasure_bound_commitment) = 32))"
    );

    let finalizer_signature: String = sqlx::query_scalar(
        "SELECT procedure.oid::regprocedure::text \
           FROM pg_proc AS procedure \
           JOIN pg_namespace AS schema ON schema.oid = procedure.pronamespace \
          WHERE schema.nspname='public' \
            AND procedure.oid::regprocedure::text='vestrace_finalize_provider_result(uuid,bytea)'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        finalizer_signature,
        "vestrace_finalize_provider_result(uuid,bytea)"
    );

    let runtime = runtime_pool(&pool).await;
    for (commitment, label) in [
        (None, "NULL commitment"),
        (Some(vec![0x8a_u8; 31]), "wrong-length commitment"),
    ] {
        let result = sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
            .bind(Uuid::now_v7())
            .bind(commitment)
            .execute(&runtime)
            .await;
        assert_exact_database_error(
            result,
            "23514",
            "provider result erasure-bound commitment must be exactly 32 bytes",
            label,
        );
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_schema_binds_one_atomic_publication_tuple(pool: PgPool) {
    let preparation_columns: BTreeSet<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
          WHERE table_schema='public' AND table_name='provider_result_preparations'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    assert!(preparation_columns.contains("expected_run_version"));

    let content_columns: BTreeSet<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
          WHERE table_schema='public' AND table_name='artifact_revision_contents'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    assert!(content_columns.contains("provider_result_preparation_id"));

    let constraints: BTreeSet<String> = sqlx::query_scalar(
        "SELECT conname FROM pg_constraint WHERE conname IN ( \
           'provider_result_publications_exact_preparation_fkey', \
           'artifact_revision_contents_exact_preparation_fkey', \
           'provider_result_preparations_deferred_completion', \
           'provider_result_preparations_exact_dispatch_cause_fkey')",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    assert_eq!(constraints.len(), 4);

    let source: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef('public.vestrace_prepare_provider_result(\
         uuid,uuid,uuid,uuid,uuid,uuid,bytea,bigint)'::regprocedure)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let replay_check = source
        .find("provider result replay tuple mismatch")
        .expect("replay must compare the full fixed tuple");
    let replay_return = source
        .find("RETURN target_preparation_id")
        .expect("exact replay must return its original preparation");
    assert!(replay_check < replay_return);
}

#[sqlx::test(migrations = "../../migrations")]
async fn representative_identity_edges_use_composite_foreign_keys(pool: PgPool) {
    let required = [
        "connection_revisions_exact_auth_mode_key",
        "connection_revisions_exact_slot_binding_key",
        "no_auth_binding_revisions_exact_mode_fkey",
        "qualification_target_bindings_exact_job_fkey",
        "qualification_target_bindings_exact_credential_fkey",
        "qualification_target_bindings_exact_revision_slot_fkey",
        "qualification_target_bindings_exact_no_auth_fkey",
        "connection_qualification_revisions_exact_job_fkey",
        "model_qualification_revisions_exact_tuple_fkey",
        "model_binding_snapshots_exact_model_qualification_fkey",
        "model_binding_snapshots_exact_revision_slot_fkey",
        "model_binding_snapshots_exact_no_auth_fkey",
        "connection_admission_policy_heads_exact_policy_fkey",
        "connection_dispatch_admissions_exact_policy_fkey",
        "connection_dispatch_admissions_exact_snapshot_fkey",
        "credential_dispatch_leases_exact_authorization_fkey",
        "credential_dispatch_leases_exact_credential_fkey",
        "credential_slots_exact_connection_key",
        "credential_key_creation_intents_exact_identity_key",
        "credential_activation_events_exact_credential_fkey",
        "credential_activation_events_exact_slot_connection_fkey",
        "credential_activation_events_exact_intent_fkey",
        "credential_rotation_events_exact_previous_fkey",
        "credential_rotation_events_exact_activated_fkey",
        "credential_rotation_events_exact_slot_connection_fkey",
        "content_materials_exact_intent_key",
        "artifact_revision_contents_exact_material_fkey",
    ];
    let actual: BTreeSet<String> =
        sqlx::query_scalar("SELECT conname FROM pg_constraint WHERE conname = ANY($1)")
            .bind(required.as_slice())
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert_eq!(actual, required.map(str::to_owned).into());
}

#[sqlx::test(migrations = "../../migrations")]
async fn representative_composite_edges_reject_mixed_identity_inserts(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_a = Uuid::now_v7();
    let connection_b = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace_id)
        .bind(format!("p03-composite-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("p03-composite-{principal_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connectors (id,workspace_id,name,provider_type) VALUES ($1,$2,'composite','openai')")
        .bind(connector_id).bind(workspace_id).execute(&pool).await.unwrap();
    for (connection, name) in [(connection_a, "a"), (connection_b, "b")] {
        sqlx::query("INSERT INTO connections (id,connector_id,workspace_id,principal_id,name) VALUES ($1,$2,$3,$4,$5)")
            .bind(connection).bind(connector_id).bind(workspace_id).bind(principal_id).bind(name)
            .execute(&pool).await.unwrap();
    }
    let effect_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents (id,workspace_id,adapter,payload) VALUES ($1,$2,'provider','{}'::jsonb)")
        .bind(effect_id).bind(workspace_id).execute(&pool).await.unwrap();

    let guard_a = Uuid::now_v7();
    let guard_b = Uuid::now_v7();
    let revision_a = Uuid::now_v7();
    let revision_b = Uuid::now_v7();
    let no_auth_a = Uuid::now_v7();
    let no_auth_b = Uuid::now_v7();
    let job_a = Uuid::now_v7();
    let job_b = Uuid::now_v7();
    let target_b = Uuid::now_v7();
    let policy_a = Uuid::now_v7();
    let mut setup = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    for (guard, connection) in [(guard_a, connection_a), (guard_b, connection_b)] {
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
            .bind(guard)
            .bind(workspace_id)
            .bind(connection)
            .execute(&mut *setup)
            .await
            .unwrap();
    }
    for (revision, connection, guard) in [
        (revision_a, connection_a, guard_a),
        (revision_b, connection_b, guard_b),
    ] {
        sqlx::query("INSERT INTO connection_revisions (id,workspace_id,connection_id,execution_guard_id,kind,logical_base_url,runtime_base_url,adapter_profile_revision,transport_policy,auth_mode) VALUES ($1,$2,$3,$4,'open_ai_chat_completions_v1','https://logical.invalid','https://runtime.invalid','profile-v1','remote_https','none')")
            .bind(revision).bind(workspace_id).bind(connection).bind(guard).execute(&mut *setup).await.unwrap();
    }
    for (no_auth, connection, revision) in [
        (no_auth_a, connection_a, revision_a),
        (no_auth_b, connection_b, revision_b),
    ] {
        sqlx::query("INSERT INTO no_auth_binding_revisions (id,workspace_id,connection_id,connection_revision_id) VALUES ($1,$2,$3,$4)")
            .bind(no_auth).bind(workspace_id).bind(connection).bind(revision).execute(&mut *setup).await.unwrap();
    }
    for (job, revision) in [(job_a, revision_a), (job_b, revision_b)] {
        sqlx::query("INSERT INTO qualification_jobs (id,workspace_id,connection_revision_id,profile_revision,state) VALUES ($1,$2,$3,'profile-v1','requested')")
            .bind(job).bind(workspace_id).bind(revision).execute(&mut *setup).await.unwrap();
    }
    sqlx::query("INSERT INTO qualification_target_bindings (id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id) VALUES ($1,$2,$3,$4,$5,'no_auth',$6)")
        .bind(target_b).bind(workspace_id).bind(job_b).bind(connection_b).bind(revision_b).bind(no_auth_b)
        .execute(&mut *setup).await.unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_revisions (id,workspace_id,connection_id,version,max_in_flight,requests_per_60_seconds,queue_wait_timeout_seconds,provider_throttle_cap_seconds) VALUES ($1,$2,$3,1,1,1,1,1)")
        .bind(policy_a).bind(workspace_id).bind(connection_a).execute(&mut *setup).await.unwrap();
    setup.commit().await.unwrap();

    let mut mixed_job = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mixed_job)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *mixed_job)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO qualification_target_bindings (id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id) VALUES ($1,$2,$3,$4,$5,'no_auth',$6)")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(job_a).bind(connection_b).bind(revision_b).bind(no_auth_b)
        .execute(&mut *mixed_job).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("qualification_target_bindings_exact_job_fkey")
    );
    mixed_job.rollback().await.unwrap();

    let mut mixed_no_auth = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mixed_no_auth)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *mixed_no_auth)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO qualification_target_bindings (id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id) VALUES ($1,$2,$3,$4,$5,'no_auth',$6)")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(job_a).bind(connection_a).bind(revision_a).bind(no_auth_b)
        .execute(&mut *mixed_no_auth).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("qualification_target_bindings_exact_no_auth_fkey")
    );
    mixed_no_auth.rollback().await.unwrap();

    let mut mixed_policy_head = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mixed_policy_head)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *mixed_policy_head)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO connection_admission_policy_heads (workspace_id,connection_id,current_policy_revision_id,version) VALUES ($1,$2,$3,1)")
        .bind(workspace_id).bind(connection_b).bind(policy_a).execute(&mut *mixed_policy_head).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("connection_admission_policy_heads_exact_policy_fkey")
    );
    mixed_policy_head.rollback().await.unwrap();

    let mut mixed_policy = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mixed_policy)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *mixed_policy)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO connection_dispatch_admissions (id,workspace_id,connection_id,connection_revision_id,policy_revision_id,external_effect_id,qualification_target_binding_id,decision,requested_wait_id,requested_concurrency_lease_id,requested_dispatch_ttl_seconds,dispatch_expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,'admitted',$8,$9,60,NOW()+INTERVAL '1 minute')")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(connection_b).bind(revision_b).bind(policy_a).bind(effect_id).bind(target_b)
        .bind(Uuid::now_v7()).bind(Uuid::now_v7())
        .execute(&mut *mixed_policy).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("connection_dispatch_admissions_exact_policy_fkey")
    );
    mixed_policy.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn automatic_fallback_matches_bootstrap_dependency_and_helper_acls(pool: PgPool) {
    for table in [
        "connection_execution_guards",
        "credential_activation_guards",
        "credential_slots",
        "credential_revisions",
        "credential_guard_occupancies",
        "credential_key_creation_intents",
        "material_key_creation_intents",
        "content_materials",
        "material_erasure_preparations",
    ] {
        let privileges: (bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT has_table_privilege('vestrace', $1, 'SELECT'), \
                    has_table_privilege('vestrace', $1, 'REFERENCES'), \
                    has_table_privilege('vestrace', $1, 'INSERT'), \
                    has_table_privilege('vestrace', $1, 'UPDATE'), \
                    has_table_privilege('vestrace', $1, 'DELETE')",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(privileges, (true, true, false, false, false), "{table}");
    }
    for helper in [
        "public.vestrace_assign_p03_table_owner(regclass)",
        "public.vestrace_assign_p03_function_owner(regprocedure)",
        "public.vestrace_grant_p03_dependency_references()",
    ] {
        let acl: (bool, bool) = sqlx::query_as(
            "SELECT has_function_privilege('vestrace', $1, 'EXECUTE'), \
                    has_function_privilege('public', $1, 'EXECUTE')",
        )
        .bind(helper)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(acl, (true, false), "{helper}");
    }
    for (table, expected_update) in [
        ("external_effect_authorizations", false),
        ("audit_events", false),
        ("idempotency_keys", false),
        ("outbox", true),
    ] {
        let authority: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(relation.relowner) = current_user,
                    relation.relrowsecurity,
                    relation.relforcerowsecurity,
                    has_table_privilege('vestrace',$1,'SELECT'),
                    has_table_privilege('vestrace',$1,'INSERT'),
                    has_table_privilege('vestrace',$1,'UPDATE'),
                    has_table_privilege('vestrace',$1,'DELETE')
               FROM pg_class AS relation
              WHERE relation.oid=$1::regclass",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            authority,
            (true, true, true, true, true, expected_update, false),
            "{table} must retain its owner and forced RLS with only the exact runtime write set"
        );
    }
    // `run_work_items` is the one Run table the guarded owner may write, and it
    // may only insert: migration 0186 grants exactly that so the guarded enqueue
    // function can schedule an `ExecuteStep` item after input material and MRE
    // are complete. Every other Run-adjacent table stays read-only to the owner.
    for (table, expected_insert) in [
        ("external_effect_intents", false),
        ("agent_runs", false),
        ("run_steps", false),
        ("run_work_items", true),
        ("artifacts", false),
        ("artifact_revisions", false),
        ("model_executions", false),
    ] {
        let privileges: (bool, bool, bool, bool) = sqlx::query_as(
            "SELECT has_table_privilege('vestrace_guarded_owner',$1,'SELECT'), \
                    has_table_privilege('vestrace_guarded_owner',$1,'INSERT'), \
                    has_table_privilege('vestrace_guarded_owner',$1,'UPDATE'), \
                    has_table_privilege('vestrace_guarded_owner',$1,'DELETE')",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            privileges,
            (true, expected_insert, false, false),
            "{table} must keep the exact guarded-owner write set"
        );
    }
}

fn assert_commit_db_error(
    result: Result<(), sqlx::Error>,
    expected_code: &str,
    expected_constraint: Option<&str>,
    operation: &str,
) {
    let error = match result {
        Ok(()) => panic!("{operation} unexpectedly committed"),
        Err(error) => error,
    };
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("{operation} returned a non-database error: {error}"));
    assert_eq!(
        database.code().as_deref(),
        Some(expected_code),
        "{operation}"
    );
    if let Some(constraint) = expected_constraint {
        assert_eq!(database.constraint(), Some(constraint), "{operation}");
    }
}

#[derive(Clone, Copy)]
struct ProviderResultEnvelopeIds {
    workspace_id: Uuid,
    principal_id: Uuid,
    effect_id: Uuid,
    run_id: Uuid,
    step_id: Uuid,
    artifact_id: Uuid,
    artifact_revision_id: Uuid,
    model_execution_id: Uuid,
    request_kind: &'static str,
    model_kind: &'static str,
}

async fn seed_provider_result_envelope(pool: &PgPool, ids: ProviderResultEnvelopeIds) {
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_binding_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let model_qualification_id = Uuid::now_v7();
    let snapshot_id = Uuid::now_v7();
    let evidence_id = Uuid::now_v7();
    let evidence_check_id = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(ids.workspace_id)
        .bind(format!("p03-result-{}", ids.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(ids.principal_id)
        .bind(ids.workspace_id)
        .bind(format!("p03-result-{}", ids.principal_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(ids.workspace_id)
    .bind(format!("p03-result-connector-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,$5,'active')")
        .bind(connection_id)
        .bind(connector_id)
        .bind(ids.workspace_id)
        .bind(ids.principal_id)
        .bind(format!("p03-result-connection-{connection_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_intents (id, workspace_id, adapter, payload) VALUES ($1, $2, 'provider', '{}'::jsonb)")
        .bind(ids.effect_id)
        .bind(ids.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_runs (id, workspace_id, principal_id, title, objective, coordinator_snapshot_id, execution_mode, status, run_version) VALUES ($1, $2, $3, 'provider', 'provider', $4, 'supervised', 'running', 1)")
        .bind(ids.run_id)
        .bind(ids.workspace_id)
        .bind(ids.principal_id)
        .bind(Uuid::now_v7())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO run_steps (id, run_id, workspace_id, step_number, title, assigned_actor, status) VALUES ($1, $2, $3, 1, 'provider', $4, 'running')")
        .bind(ids.step_id)
        .bind(ids.run_id)
        .bind(ids.workspace_id)
        .bind(serde_json::json!("system"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1, $2, 'provider', 'remote')")
        .bind(provider_id)
        .bind(ids.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken) VALUES ($1, $2, $3, 'model', 4096, 0, 0)")
        .bind(model_id)
        .bind(provider_id)
        .bind(ids.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_executions (id, workspace_id, model_id, prompt_tokens, completion_tokens, latency_ms, status) VALUES ($1, $2, $3, 1, 1, 1, 'succeeded')")
        .bind(ids.model_execution_id)
        .bind(ids.workspace_id)
        .bind(model_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifacts (id, workspace_id, name) VALUES ($1, $2, 'result')")
        .bind(ids.artifact_id)
        .bind(ids.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifact_revisions (id, artifact_id, workspace_id, revision_number, media_type, content_hash, byte_size) VALUES ($1, $2, $3, 1, 'text/plain', 'fixture', 4096)")
        .bind(ids.artifact_revision_id)
        .bind(ids.artifact_id)
        .bind(ids.workspace_id)
        .execute(pool)
        .await
        .unwrap();

    let runtime = runtime_pool(pool).await;
    let mut governed = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(ids.workspace_id)
        .bind(connection_id)
        .execute(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1','http://127.0.0.1:1234/v1','q1','loopback_only','none',NULL,0)")
        .bind(connection_revision_id)
        .bind(ids.workspace_id)
        .bind(connection_id)
        .bind(guard_id)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_binding_id)
        .bind(ids.workspace_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,'model',$7,NULL,NULL,NULL,NULL,NULL,NULL,0)")
        .bind(model_revision_id)
        .bind(ids.workspace_id)
        .bind(model_id)
        .bind(connection_id)
        .bind(guard_id)
        .bind(connection_revision_id)
        .bind(ids.model_kind)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    governed.commit().await.unwrap();
    runtime.close().await;

    let mut setup = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())")
        .bind(qualification_job_id)
        .bind(ids.workspace_id)
        .bind(connection_revision_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,valid_until,capabilities) VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',ARRAY['chat']::TEXT[])")
        .bind(connection_qualification_id)
        .bind(ids.workspace_id)
        .bind(connection_revision_id)
        .bind(qualification_job_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) VALUES($1,$2,$3,$4,$5,$6,ARRAY['chat']::TEXT[],NOW()+INTERVAL '1 hour')")
        .bind(model_qualification_id)
        .bind(ids.workspace_id)
        .bind(model_revision_id)
        .bind(connection_revision_id)
        .bind(connection_qualification_id)
        .bind(qualification_job_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)")
        .bind(snapshot_id)
        .bind(ids.workspace_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(connection_qualification_id)
        .bind(model_revision_id)
        .bind(model_qualification_id)
        .bind(no_auth_binding_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) VALUES($1,$2,$3,$4,$5,'run_step',$6)")
        .bind(evidence_id)
        .bind(ids.workspace_id)
        .bind(ids.effect_id)
        .bind(ids.request_kind)
        .bind(snapshot_id)
        .bind(ids.step_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,0,'external_effect',$4),($5,$2,$3,1,'binding_snapshot',$6),($7,$2,$3,2,'connection_revision',$8)")
        .bind(Uuid::now_v7())
        .bind(ids.workspace_id)
        .bind(evidence_id)
        .bind(ids.effect_id)
        .bind(Uuid::now_v7())
        .bind(snapshot_id)
        .bind(Uuid::now_v7())
        .bind(connection_revision_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,missing_reference_count) VALUES($1,$2,$3,'complete',0)")
        .bind(evidence_check_id)
        .bind(ids.workspace_id)
        .bind(evidence_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO provider_dispatch_causes(external_effect_id,workspace_id,model_request_evidence_id,model_request_evidence_check_id,cause_kind,run_id,step_id,model_binding_snapshot_id) VALUES($1,$2,$3,$4,'run_step',$5,$6,$7)")
        .bind(ids.effect_id)
        .bind(ids.workspace_id)
        .bind(evidence_id)
        .bind(evidence_check_id)
        .bind(ids.run_id)
        .bind(ids.step_id)
        .bind(snapshot_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();
}

async fn reserve_provider_result_intent(
    runtime: &PgPool,
    workspace_id: Uuid,
    effect_id: Uuid,
) -> Uuid {
    let intent_id = Uuid::now_v7();
    let mut reserve = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
        .bind(intent_id)
        .bind(workspace_id)
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(effect_id)
        .execute(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id)
        .execute(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *reserve)
        .await
        .unwrap();
    reserve.commit().await.unwrap();
    intent_id
}

#[derive(Clone, Copy)]
struct RunAdmissionEnvelope {
    ids: ProviderResultEnvelopeIds,
    evidence_id: Uuid,
    snapshot_id: Uuid,
    connection_id: Uuid,
    connection_revision_id: Uuid,
}

async fn setup_run_admission_envelope(
    pool: &PgPool,
    map_foreign_snapshot: bool,
) -> RunAdmissionEnvelope {
    let ids = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(pool, ids).await;
    let envelope: (Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT cause.model_request_evidence_id, cause.model_binding_snapshot_id, \
                snapshot.connection_id, snapshot.connection_revision_id \
           FROM provider_dispatch_causes AS cause \
           JOIN model_binding_snapshots AS snapshot \
             ON snapshot.id=cause.model_binding_snapshot_id \
          WHERE cause.external_effect_id=$1",
    )
    .bind(ids.effect_id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO run_streams(workspace_id,run_id,current_version) VALUES($1,$2,0) ON CONFLICT (workspace_id,run_id) DO NOTHING")
        .bind(ids.workspace_id)
        .bind(ids.run_id)
        .execute(pool)
        .await
        .unwrap();

    let mapping_snapshot_id = if map_foreign_snapshot {
        Uuid::now_v7()
    } else {
        envelope.1
    };
    let policy_id = Uuid::now_v7();
    let mut setup = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    if map_foreign_snapshot {
        sqlx::query(
            "INSERT INTO model_binding_snapshots(\
                id,workspace_id,connection_id,connection_revision_id,\
                connection_qualification_revision_id,model_revision_id,\
                model_qualification_revision_id,branch,credential_revision_id,\
                credential_slot_id,credential_activation_guard_id,expected_slot_version,\
                no_auth_binding_revision_id) \
             SELECT $1,workspace_id,connection_id,connection_revision_id,\
                    connection_qualification_revision_id,model_revision_id,\
                    model_qualification_revision_id,branch,credential_revision_id,\
                    credential_slot_id,credential_activation_guard_id,expected_slot_version,\
                    no_auth_binding_revision_id \
               FROM model_binding_snapshots WHERE id=$2",
        )
        .bind(mapping_snapshot_id)
        .bind(envelope.1)
        .execute(&mut *setup)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO run_model_binding_snapshots(workspace_id,run_id,snapshot_id) VALUES($1,$2,$3)",
    )
    .bind(ids.workspace_id)
    .bind(ids.run_id)
    .bind(mapping_snapshot_id)
    .execute(&mut *setup)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_revisions(id,workspace_id,connection_id,version,max_in_flight,requests_per_60_seconds,queue_wait_timeout_seconds,provider_throttle_cap_seconds) VALUES($1,$2,$3,1,1,60000,30,900)")
        .bind(policy_id)
        .bind(ids.workspace_id)
        .bind(envelope.2)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_heads(workspace_id,connection_id,current_policy_revision_id,version) VALUES($1,$2,$3,1)")
        .bind(ids.workspace_id)
        .bind(envelope.2)
        .bind(policy_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();
    RunAdmissionEnvelope {
        ids,
        evidence_id: envelope.0,
        snapshot_id: envelope.1,
        connection_id: envelope.2,
        connection_revision_id: envelope.3,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn dispatch_deadline_contract_is_scoped_to_normalized_provider_causes(pool: PgPool) {
    let legacy_workspace = WorkspaceId::new();
    let legacy_context = RequestContext::new(legacy_workspace, PrincipalId::new());
    let legacy_intent = ExternalEffectIntent::new(
        format!("run://{}", Uuid::now_v7()),
        legacy_workspace,
        PrincipalId::new(),
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        Some("budget:reservation-1"),
        Some("policy:decision-1"),
        chrono::Utc::now(),
    )
    .unwrap();
    let legacy_repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    legacy_repository
        .insert_intent(&legacy_context, &legacy_intent)
        .await
        .unwrap();
    legacy_repository
        .record_dispatch_started(
            &legacy_context,
            legacy_intent.id(),
            WorkerId::new(),
            chrono::Utc::now() + chrono::Duration::minutes(1),
            chrono::Utc::now(),
        )
        .await
        .expect("a legacy webhook Dispatching transition needs no provider admission");
    let legacy_admissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1",
    )
    .bind(legacy_intent.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(legacy_admissions, 0);

    let provider = setup_run_admission_envelope(&pool, false).await;
    let pre_cause_effect_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) \
         VALUES($1,$2,'provider','{}'::jsonb)",
    )
    .bind(pre_cause_effect_id)
    .bind(provider.ids.workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    let mut pre_cause_root = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *pre_cause_root)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(provider.ids.workspace_id.to_string())
        .fetch_one(&mut *pre_cause_root)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_request_evidence_roots( \
             id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'chat_completions',$4,'run_step',$5)",
    )
    .bind(Uuid::now_v7())
    .bind(provider.ids.workspace_id)
    .bind(pre_cause_effect_id)
    .bind(provider.snapshot_id)
    .bind(provider.ids.step_id)
    .execute(&mut *pre_cause_root)
    .await
    .unwrap();
    pre_cause_root.commit().await.unwrap();

    let mut pre_cause_dispatch = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,NOW()+INTERVAL '1 minute')",
    )
    .bind(pre_cause_effect_id)
    .bind(provider.ids.workspace_id)
    .bind(pre_cause_effect_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .execute(&mut *pre_cause_dispatch)
    .await
    .unwrap();
    assert_commit_db_error(
        pre_cause_dispatch.commit().await,
        "23514",
        None,
        "MRE-backed provider Dispatching before normalized cause/admission",
    );
    let pre_cause_transitions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='dispatching'",
    )
    .bind(pre_cause_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pre_cause_transitions, 0);

    let absent_expiry = chrono::Utc::now() + chrono::Duration::minutes(2);
    let mut absent = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,$5)",
    )
    .bind(provider.ids.effect_id)
    .bind(provider.ids.workspace_id)
    .bind(provider.ids.effect_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(absent_expiry)
    .execute(&mut *absent)
    .await
    .unwrap();
    assert_commit_db_error(
        absent.commit().await,
        "23514",
        None,
        "provider Dispatching without admission",
    );
    let after_absent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='dispatching'",
    )
    .bind(provider.ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_absent, 0);

    let admitted_expiry = chrono::Utc::now() + chrono::Duration::minutes(3);
    let mut admitted = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *admitted)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(provider.ids.workspace_id.to_string())
        .fetch_one(&mut *admitted)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connection_dispatch_admissions( \
             id,workspace_id,connection_id,connection_revision_id,policy_revision_id, \
             external_effect_id,model_binding_snapshot_id,decision,admitted_at, \
             requested_wait_id,requested_concurrency_lease_id,requested_dispatch_ttl_seconds, \
             dispatch_expires_at) \
         SELECT $1,$2,$3,$4,head.current_policy_revision_id,$5,$6,'admitted',NOW(),$7,$8,60,$9 \
           FROM connection_admission_policy_heads AS head \
          WHERE head.workspace_id=$2 AND head.connection_id=$3",
    )
    .bind(Uuid::now_v7())
    .bind(provider.ids.workspace_id)
    .bind(provider.connection_id)
    .bind(provider.connection_revision_id)
    .bind(provider.ids.effect_id)
    .bind(provider.snapshot_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(admitted_expiry)
    .execute(&mut *admitted)
    .await
    .unwrap();
    admitted.commit().await.unwrap();

    let mut mismatched = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,$5)",
    )
    .bind(provider.ids.effect_id)
    .bind(provider.ids.workspace_id)
    .bind(provider.ids.effect_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(admitted_expiry + chrono::Duration::seconds(1))
    .execute(&mut *mismatched)
    .await
    .unwrap();
    assert_commit_db_error(
        mismatched.commit().await,
        "23514",
        None,
        "provider Dispatching with mismatched admission expiry",
    );
    let after_mismatch: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='dispatching'",
    )
    .bind(provider.ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_mismatch, 0);

    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,$5)",
    )
    .bind(provider.ids.effect_id)
    .bind(provider.ids.workspace_id)
    .bind(provider.ids.effect_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(admitted_expiry)
    .execute(&pool)
    .await
    .expect("provider Dispatching must accept its exact admitted expiry");
    let exact_expiry: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT dispatch_expires_at FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='dispatching'",
    )
    .bind(provider.ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        exact_expiry.timestamp_micros(),
        admitted_expiry.timestamp_micros()
    );
}

async fn add_live_governed_inputs(
    pool: &PgPool,
    runtime: &PgPool,
    envelope: RunAdmissionEnvelope,
    count: usize,
    size: usize,
) {
    let mut material_ids = Vec::with_capacity(count);
    let mut materials = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *materials)
        .await
        .unwrap();
    for ordinal in 0..count {
        let intent_id = Uuid::now_v7();
        let material_id = Uuid::now_v7();
        material_ids.push(material_id);
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_input',$6,$7)")
            .bind(intent_id)
            .bind(envelope.ids.workspace_id)
            .bind(material_id)
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(envelope.evidence_id)
            .bind(ordinal as i64)
            .execute(&mut *materials)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(intent_id)
            .execute(&mut *materials)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *materials)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,$4)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .bind(valid_framed_ciphertext(size, 0x40 + ordinal as u8))
            .bind(size as i64)
            .execute(&mut *materials)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *materials)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
            .bind(intent_id)
            .execute(&mut *materials)
            .await
            .unwrap();
    }
    materials.commit().await.unwrap();

    let mut nodes = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *nodes)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *nodes)
        .await
        .unwrap();
    for (index, material_id) in material_ids.into_iter().enumerate() {
        sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,$4,'governed_input_material',$5)")
            .bind(Uuid::now_v7())
            .bind(envelope.ids.workspace_id)
            .bind(envelope.evidence_id)
            .bind(3_i32 + index as i32)
            .bind(material_id)
            .execute(&mut *nodes)
            .await
            .unwrap();
    }
    nodes.commit().await.unwrap();
}

async fn recreate_task9_tool_boundary_evidence(
    pool: &PgPool,
    runtime: &PgPool,
    envelope: RunAdmissionEnvelope,
    tool_count: usize,
) -> usize {
    add_live_governed_inputs(pool, runtime, envelope, 1, 4096).await;
    let material_id: Uuid = sqlx::query_scalar(
        "SELECT reference_id FROM model_request_evidence_nodes \
          WHERE evidence_root_id=$1 AND reference_kind='governed_input_material'",
    )
    .bind(envelope.evidence_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let source_tuple: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT connection_qualification_revision_id,model_revision_id,\
                model_qualification_revision_id \
           FROM model_binding_snapshots WHERE id=$1",
    )
    .bind(envelope.snapshot_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let shape_id = Uuid::now_v7();
    let sampling_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    let mut canonical = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *canonical)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(\
           $1,$2,1,'chat_completions',false,ARRAY['user']::TEXT[])",
    )
    .bind(shape_id)
    .bind(envelope.ids.workspace_id)
    .fetch_one(&mut *canonical)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_sampling_revision($1,$2,1,0.2,0.9)",
    )
    .bind(sampling_id)
    .bind(envelope.ids.workspace_id)
    .fetch_one(&mut *canonical)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_limits_revision($1,$2,1,128,4096,4194304)",
    )
    .bind(limits_id)
    .bind(envelope.ids.workspace_id)
    .fetch_one(&mut *canonical)
    .await
    .unwrap();
    canonical.commit().await.unwrap();

    let mut tools = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tools)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *tools)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_tool_schema_revisions(\
             id,workspace_id,version,tool_name,description,parameters_schema\
         ) SELECT gen_random_uuid(),$1,1,\
                  'task10-boundary-tool-' || lpad(ordinal::TEXT,4,'0'),\
                  NULL,'{}'::JSONB \
             FROM generate_series(0,$2::INTEGER - 1) AS ordinal",
    )
    .bind(envelope.ids.workspace_id)
    .bind(tool_count as i32)
    .execute(&mut *tools)
    .await
    .unwrap();
    tools.commit().await.unwrap();
    let tool_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM model_tool_schema_revisions \
          WHERE workspace_id=$1 AND tool_name LIKE 'task10-boundary-tool-%' \
          ORDER BY tool_name",
    )
    .bind(envelope.ids.workspace_id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(tool_ids.len(), tool_count);

    let mut reset = pool.begin().await.unwrap();
    for (table, trigger) in [
        (
            "provider_dispatch_causes",
            "provider_dispatch_causes_immutable",
        ),
        (
            "model_request_evidence_checks",
            "model_request_evidence_checks_immutable",
        ),
        (
            "model_request_evidence_nodes",
            "model_request_evidence_nodes_immutable",
        ),
        (
            "model_request_evidence_roots",
            "model_request_evidence_roots_immutable",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} DISABLE TRIGGER {trigger}"))
            .execute(&mut *reset)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM provider_dispatch_causes WHERE external_effect_id=$1")
        .bind(envelope.ids.effect_id)
        .execute(&mut *reset)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_request_evidence_checks WHERE evidence_root_id=$1")
        .bind(envelope.evidence_id)
        .execute(&mut *reset)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_request_evidence_nodes WHERE evidence_root_id=$1")
        .bind(envelope.evidence_id)
        .execute(&mut *reset)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_request_evidence_roots WHERE id=$1")
        .bind(envelope.evidence_id)
        .execute(&mut *reset)
        .await
        .unwrap();
    for (table, trigger) in [
        (
            "provider_dispatch_causes",
            "provider_dispatch_causes_immutable",
        ),
        (
            "model_request_evidence_checks",
            "model_request_evidence_checks_immutable",
        ),
        (
            "model_request_evidence_nodes",
            "model_request_evidence_nodes_immutable",
        ),
        (
            "model_request_evidence_roots",
            "model_request_evidence_roots_immutable",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} ENABLE TRIGGER {trigger}"))
            .execute(&mut *reset)
            .await
            .unwrap();
    }
    reset.commit().await.unwrap();

    let mut kinds = vec![
        "external_effect".to_owned(),
        "binding_snapshot".to_owned(),
        "connection_revision".to_owned(),
        "connection_qualification_revision".to_owned(),
        "model_revision".to_owned(),
        "model_qualification_revision".to_owned(),
        "request_shape_revision".to_owned(),
        "sampling_revision".to_owned(),
        "limits_revision".to_owned(),
    ];
    let mut ids = vec![
        envelope.ids.effect_id,
        envelope.snapshot_id,
        envelope.connection_revision_id,
        source_tuple.0,
        source_tuple.1,
        source_tuple.2,
        shape_id,
        sampling_id,
        limits_id,
    ];
    let mut versions = vec![
        None,
        None,
        None,
        None,
        None,
        None,
        Some(1_i64),
        Some(1_i64),
        Some(1_i64),
    ];
    let mut safe_ordinals = vec![None; kinds.len()];
    for (ordinal, tool_id) in tool_ids.into_iter().enumerate() {
        kinds.push("tool_schema_revision".to_owned());
        ids.push(tool_id);
        versions.push(Some(1_i64));
        safe_ordinals.push(Some(ordinal.to_string()));
    }
    kinds.push("governed_input_material".to_owned());
    ids.push(material_id);
    versions.push(None);
    safe_ordinals.push(Some("0".to_owned()));

    let check_id = Uuid::now_v7();
    let mut create = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *create)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(\
           $1,$2,$3,'chat_completions',$4,NULL,'run_step',$5,$6,$7,$8,$9)",
    )
    .bind(envelope.evidence_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.ids.effect_id)
    .bind(envelope.snapshot_id)
    .bind(envelope.ids.step_id)
    .bind(&kinds)
    .bind(&ids)
    .bind(&versions)
    .bind(&safe_ordinals)
    .fetch_one(&mut *create)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(\
           $1,$2,$3,'complete',ARRAY[]::TEXT[],ARRAY[]::UUID[],\
           ARRAY[]::UUID[],ARRAY[]::UUID[])",
    )
    .bind(check_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.evidence_id)
    .fetch_one(&mut *create)
    .await
    .unwrap();
    create.commit().await.unwrap();
    kinds.len()
}

async fn seed_additional_result_cause(
    pool: &PgPool,
    workspace_id: Uuid,
    effect_id: Uuid,
    run_id: Uuid,
    step_id: Uuid,
) {
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT model_binding_snapshot_id FROM provider_dispatch_causes \
          WHERE workspace_id=$1 AND cause_kind='run_step' LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let connection_revision_id: Uuid = sqlx::query_scalar(
        "SELECT connection_revision_id FROM model_binding_snapshots WHERE id=$1",
    )
    .bind(snapshot_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let evidence_id = Uuid::now_v7();
    let check_id = Uuid::now_v7();
    let mut setup = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) VALUES($1,$2,$3,'chat_completions',$4,'run_step',$5)")
        .bind(evidence_id)
        .bind(workspace_id)
        .bind(effect_id)
        .bind(snapshot_id)
        .bind(step_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,0,'external_effect',$4),($5,$2,$3,1,'binding_snapshot',$6),($7,$2,$3,2,'connection_revision',$8)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(evidence_id)
        .bind(effect_id)
        .bind(Uuid::now_v7())
        .bind(snapshot_id)
        .bind(Uuid::now_v7())
        .bind(connection_revision_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,missing_reference_count) VALUES($1,$2,$3,'complete',0)")
        .bind(check_id)
        .bind(workspace_id)
        .bind(evidence_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO provider_dispatch_causes(external_effect_id,workspace_id,model_request_evidence_id,model_request_evidence_check_id,cause_kind,run_id,step_id,model_binding_snapshot_id) VALUES($1,$2,$3,$4,'run_step',$5,$6,$7)")
        .bind(effect_id)
        .bind(workspace_id)
        .bind(evidence_id)
        .bind(check_id)
        .bind(run_id)
        .bind(step_id)
        .bind(snapshot_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn published_provider_result_is_rejected_at_commit_when_completion_is_incomplete(
    pool: PgPool,
) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    let step_id = Uuid::now_v7();
    let artifact_id = Uuid::now_v7();
    let artifact_revision_id = Uuid::now_v7();
    let model_execution_id = Uuid::now_v7();
    seed_provider_result_envelope(
        &pool,
        ProviderResultEnvelopeIds {
            workspace_id,
            principal_id,
            effect_id,
            run_id,
            step_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            request_kind: "chat_completions",
            model_kind: "chat",
        },
    )
    .await;

    let seeded_run: (Uuid, Uuid, Uuid) =
        sqlx::query_as("SELECT id, workspace_id, principal_id FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(seeded_run, (run_id, workspace_id, principal_id));

    let intent_id = Uuid::now_v7();
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
        .bind(intent_id)
        .bind(workspace_id)
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(effect_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO provider_result_preparations (id,workspace_id,external_effect_id,run_id,step_id,material_intent_id,prepared_attachment_id,artifact_id,artifact_revision_id,model_execution_id,size_class,expected_run_version,state,published_at,model_request_evidence_id,model_request_evidence_check_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,4096,1,'published',NOW(),(SELECT model_request_evidence_id FROM provider_dispatch_causes WHERE external_effect_id=$3),(SELECT model_request_evidence_check_id FROM provider_dispatch_causes WHERE external_effect_id=$3))")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(effect_id)
        .bind(run_id)
        .bind(step_id)
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .bind(artifact_id)
        .bind(artifact_revision_id)
        .bind(model_execution_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    assert_commit_db_error(
        transaction.commit().await,
        "23514",
        None,
        "incomplete published provider result",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_cannot_mix_fields_from_two_preparation_tuples_at_commit(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let effect_a = Uuid::now_v7();
    let run_a = Uuid::now_v7();
    let step_a = Uuid::now_v7();
    let artifact_a = Uuid::now_v7();
    let revision_a = Uuid::now_v7();
    let execution_a = Uuid::now_v7();
    seed_provider_result_envelope(
        &pool,
        ProviderResultEnvelopeIds {
            workspace_id,
            principal_id,
            effect_id: effect_a,
            run_id: run_a,
            step_id: step_a,
            artifact_id: artifact_a,
            artifact_revision_id: revision_a,
            model_execution_id: execution_a,
            request_kind: "chat_completions",
            model_kind: "chat",
        },
    )
    .await;

    let effect_b = Uuid::now_v7();
    let run_b = Uuid::now_v7();
    let step_b = Uuid::now_v7();
    let artifact_b = Uuid::now_v7();
    let revision_b = Uuid::now_v7();
    let execution_b = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents (id, workspace_id, adapter, payload) VALUES ($1, $2, 'provider', '{}'::jsonb)")
        .bind(effect_b).bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_runs (id, workspace_id, principal_id, title, objective, status, run_version) VALUES ($1,$2,$3,'provider-b','provider-b','running',1)")
        .bind(run_b).bind(workspace_id).bind(principal_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO run_steps (id,run_id,workspace_id,step_number,title,status) VALUES ($1,$2,$3,1,'provider-b','running')")
        .bind(step_b).bind(run_b).bind(workspace_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO artifacts (id,workspace_id,name) VALUES ($1,$2,'result-b')")
        .bind(artifact_b)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifact_revisions (id,artifact_id,workspace_id,revision_number,media_type,content_hash,byte_size) VALUES ($1,$2,$3,1,'text/plain','fixture-b',4096)")
        .bind(revision_b).bind(artifact_b).bind(workspace_id).execute(&pool).await.unwrap();
    let model_id: Uuid = sqlx::query_scalar("SELECT model_id FROM model_executions WHERE id=$1")
        .bind(execution_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_executions (id,workspace_id,model_id,prompt_tokens,completion_tokens,latency_ms,status) VALUES ($1,$2,$3,1,1,1,'succeeded')")
        .bind(execution_b).bind(workspace_id).bind(model_id).execute(&pool).await.unwrap();
    seed_additional_result_cause(&pool, workspace_id, effect_b, run_b, step_b).await;

    let preparation_a = Uuid::now_v7();
    let preparation_b = Uuid::now_v7();
    let receipt_a = Uuid::now_v7();
    let receipt_b = Uuid::now_v7();
    let intent_a = Uuid::now_v7();
    let intent_b = Uuid::now_v7();
    let attachment_a = Uuid::now_v7();
    let attachment_b = Uuid::now_v7();
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    for (intent, effect) in [(intent_a, effect_a), (intent_b, effect_b)] {
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
            .bind(intent).bind(workspace_id).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(effect)
            .execute(&mut *transaction).await.unwrap();
    }
    sqlx::query("RESET ROLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    for (receipt, preparation, effect) in [
        (receipt_a, preparation_a, effect_a),
        (receipt_b, preparation_b, effect_b),
    ] {
        sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
            .bind(receipt)
            .bind(effect)
            .bind(workspace_id)
            .bind(serde_json::json!({
                "response_class": "http_200",
                "evidence_refs": [format!("provider_result_preparation:{preparation}")]
            }))
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
            .bind(effect)
            .bind(workspace_id)
            .bind(receipt.to_string())
            .execute(&mut *transaction)
            .await
            .unwrap();
    }
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    for (
        preparation,
        receipt,
        effect,
        run,
        step,
        intent,
        attachment,
        artifact,
        revision,
        execution,
    ) in [
        (
            preparation_a,
            receipt_a,
            effect_a,
            run_a,
            step_a,
            intent_a,
            attachment_a,
            artifact_a,
            revision_a,
            execution_a,
        ),
        (
            preparation_b,
            receipt_b,
            effect_b,
            run_b,
            step_b,
            intent_b,
            attachment_b,
            artifact_b,
            revision_b,
            execution_b,
        ),
    ] {
        sqlx::query("INSERT INTO provider_result_preparations (id,workspace_id,external_effect_id,run_id,step_id,material_intent_id,prepared_attachment_id,artifact_id,artifact_revision_id,model_execution_id,size_class,expected_run_version,model_request_evidence_id,model_request_evidence_check_id,external_effect_receipt_id,receipt_witnessed_at,advance_work_item_id,finish_reason,usage_known,prompt_tokens,completion_tokens) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,4096,1,(SELECT model_request_evidence_id FROM provider_dispatch_causes WHERE external_effect_id=$3),(SELECT model_request_evidence_check_id FROM provider_dispatch_causes WHERE external_effect_id=$3),$11,NOW(),gen_random_uuid(),'stop',TRUE,1,1)")
            .bind(preparation).bind(workspace_id).bind(effect).bind(run).bind(step).bind(intent).bind(attachment).bind(artifact).bind(revision).bind(execution).bind(receipt)
            .execute(&mut *transaction).await.unwrap();
    }
    sqlx::query("INSERT INTO provider_result_publications (id,workspace_id,provider_result_preparation_id,external_effect_id,run_id,step_id,artifact_id,artifact_revision_id,model_execution_id,material_intent_id,prepared_attachment_id,size_class) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,4096)")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(preparation_a).bind(effect_b).bind(run_b).bind(step_b).bind(artifact_b).bind(revision_b).bind(execution_b).bind(intent_b).bind(attachment_b)
        .execute(&mut *transaction).await.unwrap();
    assert_commit_db_error(
        transaction.commit().await,
        "23503",
        Some("provider_result_publications_exact_preparation_fkey"),
        "mixed provider-result publication tuple",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn artifact_content_cannot_mix_content_material_and_intent_at_commit(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    let step_id = Uuid::now_v7();
    let artifact_id = Uuid::now_v7();
    let artifact_revision_id = Uuid::now_v7();
    let model_execution_id = Uuid::now_v7();
    seed_provider_result_envelope(
        &pool,
        ProviderResultEnvelopeIds {
            workspace_id,
            principal_id,
            effect_id,
            run_id,
            step_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            request_kind: "chat_completions",
            model_kind: "chat",
        },
    )
    .await;
    let intent_a = Uuid::now_v7();
    let intent_b = Uuid::now_v7();
    let attachment_a = Uuid::now_v7();
    let attachment_b = Uuid::now_v7();
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    for (intent, attachment) in [(intent_a, attachment_a), (intent_b, attachment_b)] {
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
            .bind(intent).bind(workspace_id).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(effect_id)
            .execute(&mut *transaction).await.unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(intent)
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
            .bind(intent)
            .bind(Uuid::now_v7())
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_prepare_result_material($1,$2,$3,4096)")
            .bind(intent)
            .bind(attachment)
            .bind(vec![0x44_u8; 4096])
            .execute(&mut *transaction)
            .await
            .unwrap();
    }
    let material_b: Uuid =
        sqlx::query_scalar("SELECT id FROM content_materials WHERE intent_id=$1")
            .bind(intent_b)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    let preparation_id = Uuid::now_v7();
    let receipt_id = Uuid::now_v7();
    sqlx::query("RESET ROLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(effect_id)
        .bind(workspace_id)
        .bind(serde_json::json!({
            "response_class": "http_200",
            "evidence_refs": [format!("provider_result_preparation:{preparation_id}")]
        }))
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(effect_id)
        .bind(workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE artifact_revisions SET storage_kind='governed_material',content_hash=NULL,byte_size=NULL WHERE id=$1")
        .bind(artifact_revision_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO provider_result_preparations (id,workspace_id,external_effect_id,run_id,step_id,material_intent_id,prepared_attachment_id,artifact_id,artifact_revision_id,model_execution_id,size_class,expected_run_version,model_request_evidence_id,model_request_evidence_check_id,external_effect_receipt_id,receipt_witnessed_at,advance_work_item_id,finish_reason,usage_known,prompt_tokens,completion_tokens) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,4096,1,(SELECT model_request_evidence_id FROM provider_dispatch_causes WHERE external_effect_id=$3),(SELECT model_request_evidence_check_id FROM provider_dispatch_causes WHERE external_effect_id=$3),$11,NOW(),gen_random_uuid(),'stop',TRUE,1,1)")
        .bind(preparation_id).bind(workspace_id).bind(effect_id).bind(run_id).bind(step_id).bind(intent_a)
        .bind(attachment_a).bind(artifact_id).bind(artifact_revision_id).bind(model_execution_id).bind(receipt_id)
        .execute(&mut *transaction).await.unwrap();
    sqlx::query("INSERT INTO artifact_revision_contents (id,workspace_id,artifact_id,artifact_revision_id,content_material_id,provider_result_preparation_id,external_effect_id,run_id,step_id,material_intent_id,prepared_attachment_id,model_execution_id,erasure_bound_commitment,size_class,media_class) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,4096,'text')")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(artifact_id).bind(artifact_revision_id).bind(material_b)
        .bind(preparation_id).bind(effect_id).bind(run_id).bind(step_id).bind(intent_a).bind(attachment_a)
        .bind(model_execution_id).bind(vec![0x6d_u8; 32]).execute(&mut *transaction).await.unwrap();
    assert_commit_db_error(
        transaction.commit().await,
        "23503",
        Some("artifact_revision_contents_exact_material_fkey"),
        "mixed content material and material intent",
    );
}

fn assert_exact_database_error<T>(
    result: Result<T, sqlx::Error>,
    code: &str,
    message: &str,
    operation: &str,
) {
    let error = match result {
        Ok(_) => panic!("{operation} unexpectedly succeeded"),
        Err(error) => error,
    };
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("{operation} returned a non-database error: {error}"));
    assert_eq!(database.code().as_deref(), Some(code), "{operation}");
    assert_eq!(database.message(), message, "{operation}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn connection_revision_creators_enforce_cas_auth_mode_and_runtime_write_boundaries(
    pool: PgPool,
) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let no_auth_connection_id = Uuid::now_v7();
    let bearer_connection_id = Uuid::now_v7();
    let no_auth_guard_id = Uuid::now_v7();
    let bearer_guard_id = Uuid::now_v7();
    let bearer_slot_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace_id)
        .bind(format!("p03-connection-revision-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("p03-connection-revision-{principal_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connectors (id,workspace_id,name,provider_type) VALUES ($1,$2,'revision-creator','openai')")
        .bind(connector_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    for (connection_id, name) in [
        (no_auth_connection_id, "no-auth"),
        (bearer_connection_id, "bearer"),
    ] {
        sqlx::query("INSERT INTO connections (id,connector_id,workspace_id,principal_id,name) VALUES ($1,$2,$3,$4,$5)")
            .bind(connection_id)
            .bind(connector_id)
            .bind(workspace_id)
            .bind(principal_id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
    }

    let runtime = runtime_pool(&pool).await;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,false)")
        .bind(workspace_id.to_string())
        .fetch_one(&runtime)
        .await
        .unwrap();
    let mut setup = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    for (guard_id, connection_id) in [
        (no_auth_guard_id, no_auth_connection_id),
        (bearer_guard_id, bearer_connection_id),
    ] {
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
            .bind(guard_id)
            .bind(workspace_id)
            .bind(connection_id)
            .execute(&mut *setup)
            .await
            .unwrap();
    }
    sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,$4,$5)")
        .bind(bearer_slot_id)
        .bind(workspace_id)
        .bind(bearer_connection_id)
        .bind("provider")
        .bind("primary")
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();

    let first_revision_id = Uuid::now_v7();
    let returned_first_revision: Uuid = sqlx::query_scalar(
        "SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(first_revision_id)
    .bind(workspace_id)
    .bind(no_auth_connection_id)
    .bind(no_auth_guard_id)
    .bind("open_ai_chat_completions_v1")
    .bind("https://logical.one.invalid")
    .bind("https://runtime.one.invalid")
    .bind("profile-v1")
    .bind("remote_https")
    .bind("none")
    .bind(Option::<Uuid>::None)
    .bind(0_i64)
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(returned_first_revision, first_revision_id);
    let first_head: (Uuid, i64) = sqlx::query_as(
        "SELECT current_revision_id,version FROM connection_revision_heads WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(workspace_id)
    .bind(no_auth_connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_head, (first_revision_id, 1));
    let first_revision_snapshot: String = sqlx::query_scalar(
        "SELECT row_to_json(revision)::text FROM connection_revisions AS revision WHERE id=$1",
    )
    .bind(first_revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let second_revision_id = Uuid::now_v7();
    let returned_second_revision: Uuid = sqlx::query_scalar(
        "SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(second_revision_id)
    .bind(workspace_id)
    .bind(no_auth_connection_id)
    .bind(no_auth_guard_id)
    .bind("open_ai_chat_completions_v1")
    .bind("https://logical.two.invalid")
    .bind("https://runtime.two.invalid")
    .bind("profile-v2")
    .bind("remote_https")
    .bind("none")
    .bind(Option::<Uuid>::None)
    .bind(1_i64)
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(returned_second_revision, second_revision_id);
    let second_head: (Uuid, i64) = sqlx::query_as(
        "SELECT current_revision_id,version FROM connection_revision_heads WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(workspace_id)
    .bind(no_auth_connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(second_head, (second_revision_id, 2));
    let first_revision_after_second: String = sqlx::query_scalar(
        "SELECT row_to_json(revision)::text FROM connection_revisions AS revision WHERE id=$1",
    )
    .bind(first_revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_revision_after_second, first_revision_snapshot);

    let stale_revision_id = Uuid::now_v7();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(stale_revision_id)
            .bind(workspace_id)
            .bind(no_auth_connection_id)
            .bind(no_auth_guard_id)
            .bind("open_ai_chat_completions_v1")
            .bind("https://logical.stale.invalid")
            .bind("https://runtime.stale.invalid")
            .bind("profile-stale")
            .bind("remote_https")
            .bind("none")
            .bind(Option::<Uuid>::None)
            .bind(1_i64)
            .execute(&runtime)
            .await,
        "40001",
        "connection revision head version conflict",
        "stale connection revision head update",
    );
    let stale_effect: (i64, Uuid, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM connection_revisions WHERE id=$1), current_revision_id,version FROM connection_revision_heads WHERE workspace_id=$2 AND connection_id=$3",
    )
    .bind(stale_revision_id)
    .bind(workspace_id)
    .bind(no_auth_connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_effect, (0, second_revision_id, 2));

    let no_auth_binding_id = Uuid::now_v7();
    let returned_no_auth_binding: Uuid =
        sqlx::query_scalar("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
            .bind(no_auth_binding_id)
            .bind(workspace_id)
            .bind(no_auth_connection_id)
            .bind(second_revision_id)
            .fetch_one(&runtime)
            .await
            .unwrap();
    assert_eq!(returned_no_auth_binding, no_auth_binding_id);
    let stored_no_auth_binding: Uuid = sqlx::query_scalar(
        "SELECT connection_revision_id FROM no_auth_binding_revisions WHERE id=$1",
    )
    .bind(no_auth_binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_no_auth_binding, second_revision_id);

    let bearer_revision_id = Uuid::now_v7();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(bearer_revision_id)
            .bind(workspace_id)
            .bind(bearer_connection_id)
            .bind(bearer_guard_id)
            .bind("open_ai_chat_completions_v1")
            .bind("https://logical.bearer.invalid")
            .bind("https://runtime.bearer.invalid")
            .bind("profile-bearer")
            .bind("remote_https")
            .bind("bearer")
            .bind(bearer_slot_id)
            .bind(1_i64)
            .execute(&runtime)
            .await,
        "40001",
        "connection revision head version conflict",
        "first connection revision requires the zero expected-version encoding",
    );
    let returned_bearer_revision: Uuid = sqlx::query_scalar(
        "SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(bearer_revision_id)
    .bind(workspace_id)
    .bind(bearer_connection_id)
    .bind(bearer_guard_id)
    .bind("open_ai_chat_completions_v1")
    .bind("https://logical.bearer.invalid")
    .bind("https://runtime.bearer.invalid")
    .bind("profile-bearer")
    .bind("remote_https")
    .bind("bearer")
    .bind(bearer_slot_id)
    .bind(0_i64)
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(returned_bearer_revision, bearer_revision_id);
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
            .bind(Uuid::now_v7())
            .bind(workspace_id)
            .bind(bearer_connection_id)
            .bind(bearer_revision_id)
            .execute(&runtime)
            .await,
        "23514",
        "no-auth binding requires a connection revision with auth_mode none",
        "auth-required connection revision cannot receive a no-auth binding",
    );

    for (result, table) in [
        (
            sqlx::query("INSERT INTO connection_revisions (id) VALUES ($1)")
                .bind(Uuid::now_v7())
                .execute(&runtime)
                .await,
            "connection_revisions",
        ),
        (
            sqlx::query("INSERT INTO connection_revision_heads (workspace_id,connection_id,current_revision_id,version) VALUES ($1,$2,$3,1)")
                .bind(workspace_id)
                .bind(no_auth_connection_id)
                .bind(Uuid::now_v7())
                .execute(&runtime)
                .await,
            "connection_revision_heads",
        ),
        (
            sqlx::query("INSERT INTO no_auth_binding_revisions (id) VALUES ($1)")
                .bind(Uuid::now_v7())
                .execute(&runtime)
                .await,
            "no_auth_binding_revisions",
        ),
    ] {
        let error = result.unwrap_err();
        let database = error
            .as_database_error()
            .unwrap_or_else(|| panic!("restricted runtime direct {table} insert returned a non-database error"));
        assert_eq!(database.code().as_deref(), Some("42501"), "{table}");
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_dispatch_admits_task9_valid_graph_above_4096_total_nodes(pool: PgPool) {
    let envelope = setup_run_admission_envelope(&pool, false).await;
    let runtime = runtime_pool(&pool).await;
    let execution_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=\
         'public.vestrace_reserve_run_step_execution_attempt(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::regprocedure",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let guarded_run_step_privileges: (bool, bool) = sqlx::query_as(
        "SELECT \
            has_table_privilege('vestrace_guarded_owner', 'public.run_steps', 'SELECT'), \
            has_table_privilege('vestrace_guarded_owner', 'public.run_steps', 'UPDATE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(execution_owner, "vestrace_guarded_owner");
    assert_eq!(guarded_run_step_privileges, (true, false));
    let total_nodes = recreate_task9_tool_boundary_evidence(&pool, &runtime, envelope, 4096).await;
    assert_eq!(total_nodes, 4106);
    let graph_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),\
                COUNT(*) FILTER (WHERE reference_kind='tool_schema_revision'),\
                COUNT(*) FILTER (WHERE reference_kind='governed_input_material') \
           FROM model_request_evidence_nodes WHERE evidence_root_id=$1",
    )
    .bind(envelope.evidence_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(graph_counts, (4106, 4096, 1));

    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let mut admission = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *admission)
        .await
        .unwrap();
    let admitted = sqlx::query(
        "SELECT * FROM vestrace_try_admit_provider_dispatch(\
          $1,$2,$3,$4,$5,$6,$7,$8,'run_step',$9,$10,$11,NULL,NULL,NULL,60)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(lease_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.connection_id)
    .bind(envelope.connection_revision_id)
    .bind(envelope.ids.effect_id)
    .bind(envelope.evidence_id)
    .bind(envelope.ids.run_id)
    .bind(envelope.ids.step_id)
    .bind(envelope.snapshot_id)
    .fetch_one(&mut *admission)
    .await
    .expect("Task9-valid 4106-node evidence must remain admissible");
    assert_eq!(admitted.get::<String, _>("decision"), "admitted");
    assert_eq!(
        admitted.get::<Option<Uuid>, _>("concurrency_lease_id"),
        Some(lease_id)
    );
    admission.commit().await.unwrap();
    let persisted: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE id=$1),\
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE id=$2),\
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$3)",
    )
    .bind(admission_id)
    .bind(lease_id)
    .bind(envelope.ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_dispatch_admission_rejects_aggregate_frames_above_derived_ceiling(pool: PgPool) {
    let envelope = setup_run_admission_envelope(&pool, false).await;
    let runtime = runtime_pool(&pool).await;
    add_live_governed_inputs(&pool, &runtime, envelope, 10, 1_048_576).await;
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let mut admission = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *admission)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query(
            "SELECT * FROM vestrace_try_admit_provider_dispatch(\
              $1,$2,$3,$4,$5,$6,$7,$8,'run_step',$9,$10,$11,NULL,NULL,NULL,60)",
        )
        .bind(admission_id)
        .bind(wait_id)
        .bind(lease_id)
        .bind(envelope.ids.workspace_id)
        .bind(envelope.connection_id)
        .bind(envelope.connection_revision_id)
        .bind(envelope.ids.effect_id)
        .bind(envelope.evidence_id)
        .bind(envelope.ids.run_id)
        .bind(envelope.ids.step_id)
        .bind(envelope.snapshot_id)
        .execute(&mut *admission)
        .await,
        "23514",
        "provider dispatch governed input aggregate exceeds derived framed ceiling",
        "admit ten one-mebibyte framed governed inputs",
    );
    admission.rollback().await.unwrap();
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE id=$2), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE id=$3), \
          (SELECT COUNT(*) FROM connection_admission_states WHERE workspace_id=$4)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(lease_id)
    .bind(envelope.ids.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_run_step_attempt_reservations_converge_on_one_tuple(pool: PgPool) {
    let envelope = setup_run_admission_envelope(&pool, false).await;
    let attempt_id = Uuid::now_v7();
    let tuple = [
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        envelope.ids.effect_id,
        envelope.evidence_id,
    ];
    let winner_runtime = runtime_pool(&pool).await;
    let loser_runtime = runtime_pool(&pool).await;
    let mut winner = winner_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *winner)
        .await
        .unwrap();
    let winner_id: Uuid = sqlx::query_scalar(
        "SELECT vestrace_reserve_run_step_execution_attempt(\
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(attempt_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.ids.run_id)
    .bind(envelope.ids.step_id)
    .bind(envelope.snapshot_id)
    .bind(tuple[0])
    .bind(tuple[1])
    .bind(tuple[2])
    .bind(tuple[3])
    .bind(tuple[4])
    .bind(tuple[5])
    .bind(tuple[6])
    .fetch_one(&mut *winner)
    .await
    .unwrap();
    assert_eq!(winner_id, attempt_id);

    let (loser_started, loser_is_waiting) = tokio::sync::oneshot::channel();
    let mut loser = tokio::spawn(async move {
        let mut transaction = loser_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(envelope.ids.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        loser_started.send(()).unwrap();
        let result: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
            "SELECT vestrace_reserve_run_step_execution_attempt(\
               $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(attempt_id)
        .bind(envelope.ids.workspace_id)
        .bind(envelope.ids.run_id)
        .bind(envelope.ids.step_id)
        .bind(envelope.snapshot_id)
        .bind(tuple[0])
        .bind(tuple[1])
        .bind(tuple[2])
        .bind(tuple[3])
        .bind(tuple[4])
        .bind(tuple[5])
        .bind(tuple[6])
        .fetch_one(&mut *transaction)
        .await;
        if result.is_ok() {
            transaction.commit().await.unwrap();
        } else {
            transaction.rollback().await.unwrap();
        }
        loser_runtime.close().await;
        result
    });
    loser_is_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut loser)
            .await
            .is_err(),
        "the second absent-tuple reservation must wait for its tuple serialization"
    );
    winner.commit().await.unwrap();
    winner_runtime.close().await;
    let loser_id = tokio::time::timeout(std::time::Duration::from_secs(5), loser)
        .await
        .expect("the convergent attempt replay must not deadlock")
        .unwrap()
        .expect("the convergent attempt replay must not surface a unique violation");
    assert_eq!(loser_id, attempt_id);
    let rows: (i64, Uuid) = sqlx::query_as(
        "SELECT COUNT(*), (array_agg(id))[1] FROM run_step_execution_attempts \
         WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
    )
    .bind(envelope.ids.workspace_id)
    .bind(envelope.ids.run_id)
    .bind(envelope.ids.step_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, (1, attempt_id));
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_execution_attempt_is_immutable_replayable_and_phase_governed(pool: PgPool) {
    let envelope = setup_run_admission_envelope(&pool, false).await;
    let runtime = runtime_pool(&pool).await;
    let attempt_id = Uuid::now_v7();
    let tuple = [
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        envelope.ids.effect_id,
        envelope.evidence_id,
    ];
    let mut reserve = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *reserve)
        .await
        .unwrap();
    let reserved: Uuid = sqlx::query_scalar(
        "SELECT vestrace_reserve_run_step_execution_attempt(\
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(attempt_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.ids.run_id)
    .bind(envelope.ids.step_id)
    .bind(envelope.snapshot_id)
    .bind(tuple[0])
    .bind(tuple[1])
    .bind(tuple[2])
    .bind(tuple[3])
    .bind(tuple[4])
    .bind(tuple[5])
    .bind(tuple[6])
    .fetch_one(&mut *reserve)
    .await
    .unwrap();
    assert_eq!(reserved, attempt_id);
    let replay: Uuid = sqlx::query_scalar(
        "SELECT vestrace_reserve_run_step_execution_attempt(\
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(attempt_id)
    .bind(envelope.ids.workspace_id)
    .bind(envelope.ids.run_id)
    .bind(envelope.ids.step_id)
    .bind(envelope.snapshot_id)
    .bind(tuple[0])
    .bind(tuple[1])
    .bind(tuple[2])
    .bind(tuple[3])
    .bind(tuple[4])
    .bind(tuple[5])
    .bind(tuple[6])
    .fetch_one(&mut *reserve)
    .await
    .unwrap();
    assert_eq!(replay, attempt_id);
    reserve.commit().await.unwrap();

    let mut mismatch = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *mismatch)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_reserve_run_step_execution_attempt(\
               $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(attempt_id)
        .bind(envelope.ids.workspace_id)
        .bind(envelope.ids.run_id)
        .bind(envelope.ids.step_id)
        .bind(envelope.snapshot_id)
        .bind(tuple[0])
        .bind(Uuid::now_v7())
        .bind(tuple[2])
        .bind(tuple[3])
        .bind(tuple[4])
        .bind(tuple[5])
        .bind(tuple[6])
        .fetch_one(&mut *mismatch)
        .await,
        "23514",
        "run step execution attempt identity conflicts with its existing tuple",
        "replay a Run attempt with one mismatched governed identity",
    );
    mismatch.rollback().await.unwrap();

    let mut named_phase = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *named_phase)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,$4)",
        )
        .bind(envelope.ids.workspace_id)
        .bind(envelope.ids.run_id)
        .bind(envelope.ids.step_id)
        .bind("admitted")
        .fetch_one(&mut *named_phase)
        .await,
        "23514",
        "run step execution attempt requires its exact admitted dispatch authority",
        "advance a reserved Run attempt to admitted by naming the phase",
    );
    named_phase.rollback().await.unwrap();
    let phase: String = sqlx::query_scalar(
        "SELECT phase FROM run_step_execution_attempts WHERE workspace_id=$1 AND id=$2",
    )
    .bind(envelope.ids.workspace_id)
    .bind(attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(phase, "reserved");
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn governed_run_step_input_enqueue_rejects_malformed_runtime_requests(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let refusal =
        sqlx::query("SELECT vestrace_enqueue_run_step_after_input_ready($1,$2,$3,$4,$5,$6,$7)")
            .bind(Option::<Uuid>::None)
            .bind(Option::<Uuid>::None)
            .bind(Option::<Uuid>::None)
            .bind(Option::<Uuid>::None)
            .bind(Option::<Uuid>::None)
            .bind(Option::<i64>::None)
            .bind(Option::<String>::None)
            .execute(&runtime)
            .await
            .expect_err("the runtime may reach only the guarded enqueue function");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("22023")
    );
    assert_eq!(
        refusal
            .as_database_error()
            .map(|database| database.message()),
        Some("governed Run-step enqueue arguments are malformed")
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_admission_requires_exact_run_snapshot_mapping(pool: PgPool) {
    let envelope = setup_run_admission_envelope(&pool, true).await;
    let runtime = runtime_pool(&pool).await;
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let mut admission = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(envelope.ids.workspace_id.to_string())
        .fetch_one(&mut *admission)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query(
            "SELECT * FROM vestrace_try_admit_provider_dispatch(\
              $1,$2,$3,$4,$5,$6,$7,$8,'run_step',$9,$10,$11,NULL,NULL,NULL,60)",
        )
        .bind(admission_id)
        .bind(wait_id)
        .bind(lease_id)
        .bind(envelope.ids.workspace_id)
        .bind(envelope.connection_id)
        .bind(envelope.connection_revision_id)
        .bind(envelope.ids.effect_id)
        .bind(envelope.evidence_id)
        .bind(envelope.ids.run_id)
        .bind(envelope.ids.step_id)
        .bind(envelope.snapshot_id)
        .execute(&mut *admission)
        .await,
        "23514",
        "provider dispatch run and snapshot mapping is not exact",
        "admit a run-step dispatch with a foreign run snapshot mapping",
    );
    admission.rollback().await.unwrap();
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE id=$2), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE id=$3), \
          (SELECT COUNT(*) FROM connection_admission_states WHERE workspace_id=$4)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(lease_id)
    .bind(envelope.ids.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_prepare_rejects_embeddings_request_and_model(pool: PgPool) {
    let ids = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "embeddings",
        model_kind: "embedding",
    };
    seed_provider_result_envelope(&pool, ids).await;
    let runtime = runtime_pool(&pool).await;
    let intent_id = Uuid::now_v7();
    let attachment_id = Uuid::now_v7();
    let mut reserve = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
        .bind(intent_id)
        .bind(ids.workspace_id)
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(ids.effect_id)
        .execute(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id)
        .execute(&mut *reserve)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *reserve)
        .await
        .unwrap();
    reserve.commit().await.unwrap();

    let mut prepare = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *prepare)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(ids.effect_id)
            .bind(intent_id)
            .bind(attachment_id)
            .bind(ids.artifact_id)
            .bind(ids.artifact_revision_id)
            .bind(ids.model_execution_id)
            .bind(valid_framed_ciphertext(4096, 0x61))
            .bind(4096_i64)
            .execute(&mut *prepare)
            .await,
        "23514",
        "provider result requires chat completions evidence and chat model",
        "prepare a retained provider result for embeddings",
    );
    prepare.rollback().await.unwrap();
    let counts: (i64, i64, String) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM content_material_bytes WHERE material_id=$2), \
          state FROM material_key_creation_intents WHERE id=$3",
    )
    .bind(ids.effect_id)
    .bind(intent_id)
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, "provisional_receipted".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminalization_and_first_prepare_races_have_no_deadlock_or_stranded_state(pool: PgPool) {
    let terminal_wins = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, terminal_wins).await;
    let setup_runtime = runtime_pool(&pool).await;
    let terminal_loser_intent = reserve_provider_result_intent(
        &setup_runtime,
        terminal_wins.workspace_id,
        terminal_wins.effect_id,
    )
    .await;
    let terminal_runtime = runtime_pool(&pool).await;
    let preparing_runtime = runtime_pool(&pool).await;
    let mut terminalize = terminal_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(terminal_wins.workspace_id.to_string())
        .fetch_one(&mut *terminalize)
        .await
        .unwrap();
    sqlx::query("UPDATE agent_runs SET status='failed',finished_at=NOW() WHERE id=$1")
        .bind(terminal_wins.run_id)
        .execute(&mut *terminalize)
        .await
        .unwrap();
    let (prepare_started, prepare_waiting) = tokio::sync::oneshot::channel();
    let ciphertext = valid_framed_ciphertext(4096, 0x71);
    let mut losing_prepare = tokio::spawn(async move {
        let mut transaction = preparing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(terminal_wins.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        prepare_started.send(()).unwrap();
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(terminal_wins.effect_id)
        .bind(terminal_loser_intent)
        .bind(Uuid::now_v7())
        .bind(terminal_wins.artifact_id)
        .bind(terminal_wins.artifact_revision_id)
        .bind(terminal_wins.model_execution_id)
        .bind(ciphertext)
        .bind(4096_i64)
        .fetch_one(&mut *transaction)
        .await;
        transaction.rollback().await.unwrap();
        preparing_runtime.close().await;
        result
    });
    prepare_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut losing_prepare,)
            .await
            .is_err(),
        "first prepare must wait behind the independent terminalization lock"
    );
    terminalize.commit().await.unwrap();
    terminal_runtime.close().await;
    let losing_prepare_result =
        tokio::time::timeout(std::time::Duration::from_secs(5), losing_prepare)
            .await
            .expect("first prepare must resolve after terminalization without deadlock")
            .unwrap();
    assert_exact_database_error(
        losing_prepare_result,
        "23514",
        "provider result requires its active running Run",
        "first prepare after terminalization wins",
    );
    let terminal_winner_state: (i64, i64, String) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$2), \
          (SELECT status FROM agent_runs WHERE id=$3)",
    )
    .bind(terminal_wins.effect_id)
    .bind(terminal_loser_intent)
    .bind(terminal_wins.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(terminal_winner_state, (0, 0, "failed".into()));

    let prepare_wins = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, prepare_wins).await;
    let intent_id = reserve_provider_result_intent(
        &setup_runtime,
        prepare_wins.workspace_id,
        prepare_wins.effect_id,
    )
    .await;
    let preparation_runtime = runtime_pool(&pool).await;
    let terminal_racing_runtime = runtime_pool(&pool).await;
    let mut prepare = preparation_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(prepare_wins.workspace_id.to_string())
        .fetch_one(&mut *prepare)
        .await
        .unwrap();
    let preparation_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(prepare_wins.effect_id)
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .bind(prepare_wins.artifact_id)
            .bind(prepare_wins.artifact_revision_id)
            .bind(prepare_wins.model_execution_id)
            .bind(valid_framed_ciphertext(4096, 0x72))
            .bind(4096_i64)
            .fetch_one(&mut *prepare)
            .await
            .unwrap();
    let receipt_id = Uuid::now_v7();
    let advance_work_item_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(prepare_wins.effect_id)
        .bind(prepare_wins.workspace_id)
        .bind(provider_result_receipt_payload(
            preparation_id,
            advance_work_item_id,
            "stop",
            Some((7, 11)),
        ))
        .execute(&mut *prepare)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(prepare_wins.effect_id)
        .bind(prepare_wins.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *prepare)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_witness_provider_result_receipt($1,$2)")
        .bind(preparation_id)
        .bind(receipt_id)
        .fetch_one(&mut *prepare)
        .await
        .unwrap();

    let (terminal_started, terminal_waiting) = tokio::sync::oneshot::channel();
    let mut losing_terminalization = tokio::spawn(async move {
        let mut transaction = terminal_racing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(prepare_wins.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        terminal_started.send(()).unwrap();
        let updated =
            sqlx::query("UPDATE agent_runs SET status='failed',finished_at=NOW() WHERE id=$1")
                .bind(prepare_wins.run_id)
                .execute(&mut *transaction)
                .await;
        let result = match updated {
            Ok(_) => transaction.commit().await,
            Err(error) => {
                transaction.rollback().await.unwrap();
                Err(error)
            }
        };
        terminal_racing_runtime.close().await;
        result
    });
    terminal_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            &mut losing_terminalization,
        )
        .await
        .is_err(),
        "terminalization must wait behind the independent first-prepare Run lock"
    );
    prepare.commit().await.unwrap();
    preparation_runtime.close().await;
    let losing_terminalization_result =
        tokio::time::timeout(std::time::Duration::from_secs(5), losing_terminalization)
            .await
            .expect("terminalization must resolve after first prepare without deadlock")
            .unwrap();
    assert_exact_database_error(
        losing_terminalization_result,
        "23514",
        "ResultPrepared blocks incompatible Run terminalization",
        "terminalization after first prepare wins",
    );
    let prepare_winner_state: (i64, i64, String, String) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_result_preparations WHERE id=$1), \
          (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$2), \
          (SELECT state FROM provider_result_preparations WHERE id=$1), \
          (SELECT status FROM agent_runs WHERE id=$3)",
    )
    .bind(preparation_id)
    .bind(intent_id)
    .bind(prepare_wins.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        prepare_winner_state,
        (1, 1, "result_prepared".into(), "running".into())
    );
    setup_runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_prepare_maps_cross_effect_identity_collisions_to_policy_conflict(
    pool: PgPool,
) {
    let first = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    let second = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, first).await;
    seed_provider_result_envelope(&pool, second).await;
    let runtime = runtime_pool(&pool).await;
    let first_intent =
        reserve_provider_result_intent(&runtime, first.workspace_id, first.effect_id).await;
    let second_intent =
        reserve_provider_result_intent(&runtime, second.workspace_id, second.effect_id).await;
    let shared_attachment_id = Uuid::now_v7();

    let mut create = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(first.workspace_id.to_string())
        .fetch_one(&mut *create)
        .await
        .unwrap();
    let preparation_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(first.effect_id)
            .bind(first_intent)
            .bind(shared_attachment_id)
            .bind(first.artifact_id)
            .bind(first.artifact_revision_id)
            .bind(first.model_execution_id)
            .bind(valid_framed_ciphertext(4096, 0x91))
            .bind(4096_i64)
            .fetch_one(&mut *create)
            .await
            .unwrap();
    let receipt_id = Uuid::now_v7();
    let advance_work_item_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(first.effect_id)
        .bind(first.workspace_id)
        .bind(provider_result_receipt_payload(
            preparation_id,
            advance_work_item_id,
            "stop",
            Some((1, 1)),
        ))
        .execute(&mut *create)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(first.effect_id)
        .bind(first.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *create)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_witness_provider_result_receipt($1,$2)")
        .bind(preparation_id)
        .bind(receipt_id)
        .execute(&mut *create)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut collide = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(second.workspace_id.to_string())
        .fetch_one(&mut *collide)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(second.effect_id)
            .bind(second_intent)
            .bind(shared_attachment_id)
            .bind(second.artifact_id)
            .bind(second.artifact_revision_id)
            .bind(second.model_execution_id)
            .bind(valid_framed_ciphertext(4096, 0x92))
            .bind(4096_i64)
            .execute(&mut *collide)
            .await,
        "23514",
        "provider result identity collision",
        "reuse a provider-result attachment identity across effects",
    );
    collide.rollback().await.unwrap();
    runtime.close().await;

    let rows: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$2), \
          (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$3)",
    )
    .bind(first.effect_id)
    .bind(second.effect_id)
    .bind(second_intent)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, (1, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_entrypoints_enforce_replay_and_commit_complete_publication(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    let step_id = Uuid::now_v7();
    let artifact_id = Uuid::now_v7();
    let artifact_revision_id = Uuid::now_v7();
    let model_execution_id = Uuid::now_v7();
    seed_provider_result_envelope(
        &pool,
        ProviderResultEnvelopeIds {
            workspace_id,
            principal_id,
            effect_id,
            run_id,
            step_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            request_kind: "chat_completions",
            model_kind: "chat",
        },
    )
    .await;
    let runtime = runtime_pool(&pool).await;
    let intent_id = Uuid::now_v7();
    let alternate_intent_id = Uuid::now_v7();
    let attachment_id = Uuid::now_v7();
    let ciphertext = valid_framed_ciphertext(4096, 0x5a);
    let erasure_bound_commitment = vec![0x3c_u8; 32];
    let mut reserve = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *reserve)
        .await
        .unwrap();
    for intent in [intent_id, alternate_intent_id] {
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
            .bind(intent).bind(workspace_id).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(effect_id)
            .execute(&mut *reserve).await.unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(intent)
            .execute(&mut *reserve)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
            .bind(intent)
            .bind(Uuid::now_v7())
            .execute(&mut *reserve)
            .await
            .unwrap();
    }
    reserve.commit().await.unwrap();

    for (malformed, size, label) in [
        (vec![0x5a_u8; 4096], 4096_i64, "wrong frame header"),
        (
            valid_framed_ciphertext(1_048_576, 0xa5)
                .into_iter()
                .chain([0_u8; 1])
                .collect(),
            1_048_577_i64,
            "oversized frame",
        ),
    ] {
        let mut rejected = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *rejected)
            .await
            .unwrap();
        assert_exact_database_error(
            sqlx::query("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(effect_id)
                .bind(intent_id)
                .bind(attachment_id)
                .bind(artifact_id)
                .bind(artifact_revision_id)
                .bind(model_execution_id)
                .bind(malformed)
                .bind(size)
                .execute(&mut *rejected)
                .await,
            "22023",
            "provider result preparation arguments are malformed",
            label,
        );
        rejected.rollback().await.unwrap();
    }

    let mut prepare_only = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *prepare_only)
        .await
        .unwrap();
    let rolled_back_preparation_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(effect_id)
            .bind(intent_id)
            .bind(attachment_id)
            .bind(artifact_id)
            .bind(artifact_revision_id)
            .bind(model_execution_id)
            .bind(&ciphertext)
            .bind(4096_i64)
            .fetch_one(&mut *prepare_only)
            .await
            .unwrap();
    assert_exact_database_error(
        prepare_only.commit().await,
        "23514",
        "ResultPrepared requires its exact witnessed provider receipt",
        "commit provider result preparation without its receipt witness",
    );
    let rolled_back: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM provider_result_preparations WHERE id=$1), \
                (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$2)",
    )
    .bind(rolled_back_preparation_id)
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rolled_back, (0, 0));

    let receipt_id = Uuid::now_v7();
    let advance_work_item_id = Uuid::now_v7();
    let mut prepare = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *prepare)
        .await
        .unwrap();
    let preparation_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(effect_id)
            .bind(intent_id)
            .bind(attachment_id)
            .bind(artifact_id)
            .bind(artifact_revision_id)
            .bind(model_execution_id)
            .bind(&ciphertext)
            .bind(4096_i64)
            .fetch_one(&mut *prepare)
            .await
            .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(effect_id)
        .bind(workspace_id)
        .bind(provider_result_receipt_payload(
            preparation_id,
            advance_work_item_id,
            "stop",
            None,
        ))
        .execute(&mut *prepare)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(effect_id)
        .bind(workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *prepare)
        .await
        .unwrap();
    let witnessed: Uuid =
        sqlx::query_scalar("SELECT vestrace_witness_provider_result_receipt($1,$2)")
            .bind(preparation_id)
            .bind(receipt_id)
            .fetch_one(&mut *prepare)
            .await
            .unwrap();
    assert_eq!(witnessed, receipt_id);
    let replay_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(effect_id)
            .bind(intent_id)
            .bind(attachment_id)
            .bind(artifact_id)
            .bind(artifact_revision_id)
            .bind(model_execution_id)
            .bind(&ciphertext)
            .bind(4096_i64)
            .fetch_one(&mut *prepare)
            .await
            .unwrap();
    assert_eq!(replay_id, preparation_id);
    let losing_runtime = runtime_pool(&pool).await;
    let losing_ciphertext = ciphertext.clone();
    let (loser_started, loser_is_waiting) = tokio::sync::oneshot::channel();
    let mut first_prepare_loser = tokio::spawn(async move {
        let mut loser = losing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *loser)
            .await
            .unwrap();
        loser_started.send(()).unwrap();
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(effect_id)
        .bind(alternate_intent_id)
        .bind(Uuid::now_v7())
        .bind(artifact_id)
        .bind(artifact_revision_id)
        .bind(model_execution_id)
        .bind(losing_ciphertext)
        .bind(4096_i64)
        .fetch_one(&mut *loser)
        .await;
        loser.rollback().await.unwrap();
        losing_runtime.close().await;
        result
    });
    loser_is_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            &mut first_prepare_loser,
        )
        .await
        .is_err(),
        "the competing first prepare must wait behind the winning Run lock"
    );
    prepare.commit().await.unwrap();
    let losing_result =
        tokio::time::timeout(std::time::Duration::from_secs(5), first_prepare_loser)
            .await
            .expect("the competing first prepare must resolve without deadlock")
            .unwrap();
    assert_exact_database_error(
        losing_result,
        "23514",
        "provider result replay tuple mismatch",
        "lose a competing first provider-result preparation",
    );

    let persisted_unknown_usage: (bool, Option<i32>, Option<i32>) = sqlx::query_as(
        "SELECT usage_known,prompt_tokens,completion_tokens FROM provider_result_preparations WHERE id=$1",
    )
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted_unknown_usage, (false, None, None));

    let mut witness_replay = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *witness_replay)
        .await
        .unwrap();
    let witnessed_again: Uuid =
        sqlx::query_scalar("SELECT vestrace_witness_provider_result_receipt($1,$2)")
            .bind(preparation_id)
            .bind(receipt_id)
            .fetch_one(&mut *witness_replay)
            .await
            .unwrap();
    assert_eq!(witnessed_again, receipt_id);
    let witness_racing_runtime = runtime_pool(&pool).await;
    let witness_racing_ciphertext = ciphertext.clone();
    let (witness_race_started, witness_race_waiting) = tokio::sync::oneshot::channel();
    let mut prepare_against_witness = tokio::spawn(async move {
        let mut replay = witness_racing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *replay)
            .await
            .unwrap();
        witness_race_started.send(()).unwrap();
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(effect_id)
        .bind(intent_id)
        .bind(attachment_id)
        .bind(artifact_id)
        .bind(artifact_revision_id)
        .bind(model_execution_id)
        .bind(witness_racing_ciphertext)
        .bind(4096_i64)
        .fetch_one(&mut *replay)
        .await
        .unwrap();
        replay.commit().await.unwrap();
        witness_racing_runtime.close().await;
        result
    });
    witness_race_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            &mut prepare_against_witness,
        )
        .await
        .is_err(),
        "prepare replay must wait behind receipt-witness replay's Run lock"
    );
    witness_replay.commit().await.unwrap();
    let replay_after_witness =
        tokio::time::timeout(std::time::Duration::from_secs(5), prepare_against_witness)
            .await
            .expect("prepare/witness replay race must resolve without deadlock")
            .unwrap();
    assert_eq!(replay_after_witness, preparation_id);

    let mut fail_run = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *fail_run)
        .await
        .unwrap();
    sqlx::query("UPDATE agent_runs SET status='failed',finished_at=NOW() WHERE id=$1")
        .bind(run_id)
        .execute(&mut *fail_run)
        .await
        .unwrap();
    assert_exact_database_error(
        fail_run.commit().await,
        "23514",
        "ResultPrepared blocks incompatible Run terminalization",
        "terminalize a Run while its provider result remains prepared",
    );

    let mut fail_step = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *fail_step)
        .await
        .unwrap();
    sqlx::query("UPDATE run_steps SET status='failed',finished_at=NOW() WHERE id=$1")
        .bind(step_id)
        .execute(&mut *fail_step)
        .await
        .unwrap();
    assert_exact_database_error(
        fail_step.commit().await,
        "23514",
        "ResultPrepared blocks incompatible step terminalization",
        "terminalize a step while its provider result remains prepared",
    );
    let active_parent: (String, Option<chrono::DateTime<chrono::Utc>>, String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT run.status,run.finished_at,step.status,step.finished_at FROM agent_runs AS run JOIN run_steps AS step ON step.run_id=run.id WHERE run.id=$1 AND step.id=$2",
    )
    .bind(run_id)
    .bind(step_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        active_parent,
        ("running".into(), None, "running".into(), None)
    );

    let racing_runtime = runtime_pool(&pool).await;
    let mut concurrent_failure = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *concurrent_failure)
        .await
        .unwrap();
    sqlx::query("UPDATE agent_runs SET status='failed',finished_at=NOW() WHERE id=$1")
        .bind(run_id)
        .execute(&mut *concurrent_failure)
        .await
        .unwrap();
    let (replay_started, replay_is_waiting) = tokio::sync::oneshot::channel();
    let racing_ciphertext = ciphertext.clone();
    let mut replay_task = tokio::spawn(async move {
        let mut replay = racing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *replay)
            .await
            .unwrap();
        replay_started.send(()).unwrap();
        let replayed: Uuid =
            sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(effect_id)
                .bind(intent_id)
                .bind(attachment_id)
                .bind(artifact_id)
                .bind(artifact_revision_id)
                .bind(model_execution_id)
                .bind(racing_ciphertext)
                .bind(4096_i64)
                .fetch_one(&mut *replay)
                .await
                .unwrap();
        replay.commit().await.unwrap();
        racing_runtime.close().await;
        replayed
    });
    replay_is_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut replay_task)
            .await
            .is_err(),
        "prepare replay must wait behind the independently held Run lock"
    );
    assert_exact_database_error(
        concurrent_failure.commit().await,
        "23514",
        "ResultPrepared blocks incompatible Run terminalization",
        "race Run failure against provider-result prepare replay",
    );
    let raced_replay = tokio::time::timeout(std::time::Duration::from_secs(5), replay_task)
        .await
        .expect("prepare replay must resume after the rejected terminalization without deadlock")
        .unwrap();
    assert_eq!(raced_replay, preparation_id);

    let mismatches = [
        (
            alternate_intent_id,
            attachment_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            ciphertext.clone(),
            4096_i64,
            "intent",
        ),
        (
            intent_id,
            Uuid::now_v7(),
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            ciphertext.clone(),
            4096_i64,
            "attachment",
        ),
        (
            intent_id,
            attachment_id,
            Uuid::now_v7(),
            artifact_revision_id,
            model_execution_id,
            ciphertext.clone(),
            4096_i64,
            "artifact",
        ),
        (
            intent_id,
            attachment_id,
            artifact_id,
            Uuid::now_v7(),
            model_execution_id,
            ciphertext.clone(),
            4096_i64,
            "artifact revision",
        ),
        (
            intent_id,
            attachment_id,
            artifact_id,
            artifact_revision_id,
            Uuid::now_v7(),
            ciphertext.clone(),
            4096_i64,
            "model execution",
        ),
        (
            intent_id,
            attachment_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            valid_framed_ciphertext(4096, 0xa5),
            4096_i64,
            "ciphertext",
        ),
        (
            intent_id,
            attachment_id,
            artifact_id,
            artifact_revision_id,
            model_execution_id,
            valid_framed_ciphertext(8192, 0x5a),
            8192_i64,
            "size",
        ),
    ];
    for (
        candidate_intent,
        candidate_attachment,
        candidate_artifact,
        candidate_revision,
        candidate_execution,
        candidate_ciphertext,
        candidate_size,
        label,
    ) in mismatches
    {
        let mut replay = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *replay)
            .await
            .unwrap();
        let result =
            sqlx::query("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(effect_id)
                .bind(candidate_intent)
                .bind(candidate_attachment)
                .bind(candidate_artifact)
                .bind(candidate_revision)
                .bind(candidate_execution)
                .bind(candidate_ciphertext)
                .bind(candidate_size)
                .execute(&mut *replay)
                .await;
        assert_exact_database_error(
            result,
            "23514",
            "provider result replay tuple mismatch",
            label,
        );
        replay.rollback().await.unwrap();
    }

    let invalid_receipt_id = Uuid::now_v7();
    let mut invalid_witness = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *invalid_witness)
        .await
        .unwrap();
    let mut invalid_payload =
        provider_result_receipt_payload(preparation_id, advance_work_item_id, "stop", None);
    invalid_payload["evidence_refs"] = serde_json::json!([
        format!("provider_result_preparation:{preparation_id}"),
        "extra:marker"
    ]);
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(invalid_receipt_id)
        .bind(effect_id)
        .bind(workspace_id)
        .bind(invalid_payload)
        .execute(&mut *invalid_witness)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(effect_id)
        .bind(workspace_id)
        .bind(invalid_receipt_id.to_string())
        .execute(&mut *invalid_witness)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_witness_provider_result_receipt($1,$2)")
            .bind(preparation_id)
            .bind(invalid_receipt_id)
            .execute(&mut *invalid_witness)
            .await,
        "23514",
        "provider result receipt witness is not the exact canonical receipt",
        "witness a provider receipt with a noncanonical marker set",
    );
    invalid_witness.rollback().await.unwrap();
    let refused_witness: (Option<Uuid>, i64) = sqlx::query_as(
        "SELECT preparation.external_effect_receipt_id,(SELECT COUNT(*) FROM external_effect_receipts WHERE id=$2) FROM provider_result_preparations AS preparation WHERE preparation.id=$1",
    )
    .bind(preparation_id)
    .bind(invalid_receipt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refused_witness, (Some(receipt_id), 0));

    sqlx::query("UPDATE artifacts SET name=$2 WHERE id=$1")
        .bind(artifact_id)
        .bind(format!("provider-result-{artifact_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE model_executions SET usage_known=FALSE,prompt_tokens=0,completion_tokens=0,latency_ms=0,status='succeeded' WHERE id=$1",
    )
    .bind(model_execution_id)
    .execute(&pool)
    .await
    .unwrap();

    let foreign_model_id = Uuid::now_v7();
    let provider_id: Uuid = sqlx::query_scalar(
        "SELECT model.provider_id FROM models AS model \
          JOIN model_executions AS execution ON execution.model_id=model.id \
         WHERE execution.id=$1",
    )
    .bind(model_execution_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,$4,4096,0,0)")
        .bind(foreign_model_id)
        .bind(provider_id)
        .bind(workspace_id)
        .bind(format!("foreign-{foreign_model_id}"))
        .execute(&pool)
        .await
        .unwrap();
    for (foreign_model, failed_status, label) in [
        (
            Some(foreign_model_id),
            false,
            "finalize with an execution for a foreign model",
        ),
        (None, true, "finalize with a failed model execution"),
    ] {
        let mut semantic_mismatch = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *semantic_mismatch)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *semantic_mismatch)
            .await
            .unwrap();
        sqlx::query("UPDATE artifact_revisions SET storage_kind='governed_material',content_hash=NULL,byte_size=NULL WHERE id=$1")
            .bind(artifact_revision_id)
            .execute(&mut *semantic_mismatch)
            .await
            .unwrap();
        if let Some(model_id) = foreign_model {
            sqlx::query("UPDATE model_executions SET model_id=$2 WHERE id=$1")
                .bind(model_execution_id)
                .bind(model_id)
                .execute(&mut *semantic_mismatch)
                .await
                .unwrap();
        }
        if failed_status {
            sqlx::query("UPDATE model_executions SET status='failed' WHERE id=$1")
                .bind(model_execution_id)
                .execute(&mut *semantic_mismatch)
                .await
                .unwrap();
        }
        assert_exact_database_error(
            sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
                .bind(preparation_id)
                .bind(&erasure_bound_commitment)
                .execute(&mut *semantic_mismatch)
                .await,
            "23514",
            "provider result model execution is not the exact safe succeeded snapshot model",
            label,
        );
        semantic_mismatch.rollback().await.unwrap();
        let unchanged: (String, String, i64, i64) = sqlx::query_as(
            "SELECT preparation.state, intent.state, \
              (SELECT COUNT(*) FROM provider_result_publications WHERE provider_result_preparation_id=$1), \
              (SELECT COUNT(*) FROM artifact_revision_contents WHERE provider_result_preparation_id=$1) \
               FROM provider_result_preparations AS preparation \
               JOIN material_key_creation_intents AS intent ON intent.id=preparation.material_intent_id \
              WHERE preparation.id=$1",
        )
        .bind(preparation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            unchanged,
            ("result_prepared".into(), "result_prepared".into(), 0, 0)
        );
    }

    let mut incomplete = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *incomplete)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *incomplete)
        .await
        .unwrap();
    sqlx::query("UPDATE artifact_revisions SET storage_kind='governed_material',content_hash=NULL,byte_size=NULL WHERE id=$1")
        .bind(artifact_revision_id)
        .execute(&mut *incomplete)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
        .bind(preparation_id)
        .bind(&erasure_bound_commitment)
        .execute(&mut *incomplete)
        .await
        .unwrap();
    assert_exact_database_error(
        incomplete.commit().await,
        "23514",
        "published provider result requires its exact publication, content map, succeeded step, Run version and AdvanceRun continuation",
        "incomplete provider result finalization commit",
    );

    let mut complete = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *complete)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *complete)
        .await
        .unwrap();
    sqlx::query("UPDATE artifact_revisions SET storage_kind='governed_material',content_hash=NULL,byte_size=NULL WHERE id=$1")
        .bind(artifact_revision_id)
        .execute(&mut *complete)
        .await
        .unwrap();
    sqlx::query("UPDATE run_steps SET status='succeeded', finished_at=NOW(), output_references=jsonb_build_array(jsonb_build_object('artifact_id',$2::uuid,'artifact_revision_id',$3::uuid,'model_execution_id',$4::uuid)) WHERE id=$1")
        .bind(step_id).bind(artifact_id).bind(artifact_revision_id).bind(model_execution_id)
        .execute(&mut *complete).await.unwrap();
    sqlx::query("UPDATE agent_runs SET run_version=2 WHERE id=$1")
        .bind(run_id)
        .execute(&mut *complete)
        .await
        .unwrap();
    sqlx::query("INSERT INTO run_work_items (id,workspace_id,run_id,step_id,expected_run_version,kind,status,idempotency_key) VALUES ($1,$2,$3,$4,2,'advance_run','ready',$5)")
        .bind(advance_work_item_id).bind(workspace_id).bind(run_id).bind(step_id).bind(format!("advance-{run_id}"))
        .execute(&mut *complete).await.unwrap();
    sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
        .bind(preparation_id)
        .bind(&erasure_bound_commitment)
        .execute(&mut *complete)
        .await
        .unwrap();
    complete.commit().await.unwrap();

    for (mutation, restoration, target_id, expected_message, label) in [
        (
            "UPDATE artifacts SET name='mutated-provider-result' WHERE id=$1",
            "UPDATE artifacts SET name='provider-result-' || id::TEXT WHERE id=$1",
            artifact_id,
            "provider result runtime envelopes are absent or not governed",
            "published replay after artifact-name mutation",
        ),
        (
            "UPDATE artifact_revisions SET media_type='application/octet-stream' WHERE id=$1",
            "UPDATE artifact_revisions SET media_type='text/plain' WHERE id=$1",
            artifact_revision_id,
            "provider result runtime envelopes are absent or not governed",
            "published replay after media-type mutation",
        ),
        (
            "UPDATE model_executions SET usage_known=TRUE WHERE id=$1",
            "UPDATE model_executions SET usage_known=FALSE WHERE id=$1",
            model_execution_id,
            "provider result model execution is not the exact safe succeeded snapshot model",
            "published replay after usage-knownness mutation",
        ),
        (
            "UPDATE model_executions SET latency_ms=1 WHERE id=$1",
            "UPDATE model_executions SET latency_ms=0 WHERE id=$1",
            model_execution_id,
            "provider result model execution is not the exact safe succeeded snapshot model",
            "published replay after latency mutation",
        ),
    ] {
        sqlx::query(mutation)
            .bind(target_id)
            .execute(&pool)
            .await
            .unwrap();
        let mut replay = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *replay)
            .await
            .unwrap();
        assert_exact_database_error(
            sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
                .bind(preparation_id)
                .bind(&erasure_bound_commitment)
                .execute(&mut *replay)
                .await,
            "23514",
            expected_message,
            label,
        );
        replay.rollback().await.unwrap();
        sqlx::query(restoration)
            .bind(target_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let mut finalize_lock = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *finalize_lock)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
        .bind(preparation_id)
        .bind(&erasure_bound_commitment)
        .execute(&mut *finalize_lock)
        .await
        .unwrap();
    let finalize_racing_runtime = runtime_pool(&pool).await;
    let finalize_racing_ciphertext = ciphertext.clone();
    let (finalize_race_started, finalize_race_waiting) = tokio::sync::oneshot::channel();
    let mut prepare_against_finalize = tokio::spawn(async move {
        let mut replay = finalize_racing_runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *replay)
            .await
            .unwrap();
        finalize_race_started.send(()).unwrap();
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(effect_id)
        .bind(intent_id)
        .bind(attachment_id)
        .bind(artifact_id)
        .bind(artifact_revision_id)
        .bind(model_execution_id)
        .bind(finalize_racing_ciphertext)
        .bind(4096_i64)
        .fetch_one(&mut *replay)
        .await;
        replay.rollback().await.unwrap();
        finalize_racing_runtime.close().await;
        result
    });
    finalize_race_waiting.await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            &mut prepare_against_finalize,
        )
        .await
        .is_err(),
        "prepare replay must wait behind finalize replay's Run lock"
    );
    finalize_lock.commit().await.unwrap();
    let replay_after_finalize =
        tokio::time::timeout(std::time::Duration::from_secs(5), prepare_against_finalize)
            .await
            .expect("prepare/finalize replay race must resolve without deadlock")
            .unwrap();
    assert_exact_database_error(
        replay_after_finalize,
        "23514",
        "provider result requires its active running step",
        "prepare replay after the racing finalization terminalizes its step",
    );

    let mut replay_finalize = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *replay_finalize)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
        .bind(preparation_id)
        .bind(&erasure_bound_commitment)
        .execute(&mut *replay_finalize)
        .await
        .unwrap();
    replay_finalize.commit().await.unwrap();

    let mut mismatched_finalize = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *mismatched_finalize)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
            .bind(preparation_id)
            .bind(vec![0xc3_u8; 32])
            .execute(&mut *mismatched_finalize)
            .await,
        "23514",
        "provider result finalization replay commitment mismatch",
        "replay provider result finalization with a different commitment",
    );
    mismatched_finalize.rollback().await.unwrap();
    runtime.close().await;

    let published: (i64, i64, bool) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM provider_result_publications WHERE provider_result_preparation_id=$1), (SELECT COUNT(*) FROM artifact_revision_contents WHERE provider_result_preparation_id=$1), EXISTS (SELECT 1 FROM artifact_revision_contents AS content JOIN content_materials AS material ON material.id=content.content_material_id AND material.workspace_id=content.workspace_id AND material.intent_id=content.material_intent_id WHERE content.provider_result_preparation_id=$1 AND content.erasure_bound_commitment=$2)",
    )
    .bind(preparation_id).bind(&erasure_bound_commitment).fetch_one(&pool).await.unwrap();
    assert_eq!(published, (1, 1, true));
}

#[sqlx::test(migrations = "../../migrations")]
async fn generic_material_resumption_cannot_publish_a_provider_result(pool: PgPool) {
    let ids = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, ids).await;
    let runtime = runtime_pool(&pool).await;
    let intent_id = reserve_provider_result_intent(&runtime, ids.workspace_id, ids.effect_id).await;
    let attachment_id = Uuid::now_v7();
    let receipt_id = Uuid::now_v7();
    let advance_work_item_id = Uuid::now_v7();
    let mut prepare = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *prepare)
        .await
        .unwrap();
    let preparation_id: Uuid =
        sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(ids.effect_id)
            .bind(intent_id)
            .bind(attachment_id)
            .bind(ids.artifact_id)
            .bind(ids.artifact_revision_id)
            .bind(ids.model_execution_id)
            .bind(valid_framed_ciphertext(4096, 0x73))
            .bind(4096_i64)
            .fetch_one(&mut *prepare)
            .await
            .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(ids.effect_id)
        .bind(ids.workspace_id)
        .bind(provider_result_receipt_payload(
            preparation_id,
            advance_work_item_id,
            "stop",
            Some((7, 11)),
        ))
        .execute(&mut *prepare)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(ids.effect_id)
        .bind(ids.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *prepare)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_witness_provider_result_receipt($1,$2)")
        .bind(preparation_id)
        .bind(receipt_id)
        .execute(&mut *prepare)
        .await
        .unwrap();
    prepare.commit().await.unwrap();

    let mut bind = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *bind)
        .await
        .unwrap();
    let binding_receipt = Uuid::now_v7();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id)
        .bind(binding_receipt)
        .execute(&mut *bind)
        .await
        .unwrap();
    bind.commit().await.unwrap();

    let mut generic = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(ids.workspace_id.to_string())
        .fetch_one(&mut *generic)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(intent_id)
        .execute(&mut *generic)
        .await
        .unwrap();
    assert_exact_database_error(
        generic.commit().await,
        "23514",
        "provider result material may become Live only beside its exact publication",
        "commit generic provider-result material resumption",
    );
    let unchanged: (String, String, i64, i64) = sqlx::query_as(
        "SELECT intent.state, preparation.state, \
                (SELECT COUNT(*) FROM provider_result_publications \
                  WHERE provider_result_preparation_id = $2), \
                (SELECT COUNT(*) FROM content_materials WHERE intent_id = $1) \
           FROM material_key_creation_intents AS intent \
           JOIN provider_result_preparations AS preparation \
             ON preparation.material_intent_id = intent.id \
          WHERE intent.id = $1 AND preparation.id = $2",
    )
    .bind(intent_id)
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, ("bound".into(), "result_prepared".into(), 0, 1));
    runtime.close().await;

    let (material_id, material_key_id, intent_nonce, vault_receipt): (Uuid, Uuid, Uuid, Uuid) =
        sqlx::query_as(
            "SELECT material_id,material_key_id,nonce,vault_receipt \
           FROM material_key_creation_intents WHERE id=$1",
        )
        .bind(intent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifact_revisions WHERE id=$1")
        .bind(ids.artifact_revision_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(ids.artifact_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_executions WHERE id=$1")
        .bind(ids.model_execution_id)
        .execute(&pool)
        .await
        .unwrap();
    let restarted_runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(restarted_runtime.clone());
    let vault = Arc::new(ProviderResultTestVault::default());
    *vault
        .key
        .lock()
        .expect("provider-result vault lock poisoned") = Some((
        MaterialKeyId::from_uuid(material_key_id),
        IntentNonce::from_uuid(intent_nonce),
        VaultReceipt::from_uuid(vault_receipt),
    ));
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let runs = Arc::new(PostgresRunStore::new(&store));
    let permit = Arc::new(PgInstallationMutationPermit::new(store));
    let repository = PgProviderResultRepository::new(
        permit,
        vault.clone(),
        Arc::new(ContentMaterialCodec::new()),
        effects,
        runs,
    );
    let context = RequestContext::new(
        WorkspaceId::from_uuid(ids.workspace_id),
        PrincipalId::from_uuid(ids.principal_id),
    );
    let recovered = repository
        .recover_result_prepared(&context, ExternalEffectId::from_uuid(ids.effect_id))
        .await
        .expect("provider-owned recovery must survive the failed generic resumption");
    assert_eq!(recovered.preparation_id, preparation_id);
    assert_eq!(
        recovered.identities,
        ProviderResultIdentities {
            material_intent_id: MaterialKeyCreationIntentId::from_uuid(intent_id),
            content_material_id: ContentMaterialId::from_uuid(material_id),
            material_key_id: MaterialKeyId::from_uuid(material_key_id),
            intent_nonce: IntentNonce::from_uuid(intent_nonce),
            prepared_attachment_id: PreparedMaterialAttachmentId::from_uuid(attachment_id),
            receipt_id: ExternalEffectReceiptId::from_uuid(receipt_id),
            artifact_id: ArtifactId::from_uuid(ids.artifact_id),
            artifact_revision_id: ArtifactRevisionId::from_uuid(ids.artifact_revision_id),
            model_execution_id: ModelExecutionId::from_uuid(ids.model_execution_id),
            advance_work_item_id: WorkItemId::from_uuid(advance_work_item_id),
        }
    );
    let publication = repository
        .finalize(
            &context,
            FinalizeProviderResult {
                prepared: recovered,
                binding_receipt: MaterialKeyBindingReceipt::from_uuid(binding_receipt),
            },
        )
        .await
        .expect("provider-owned restart recovery must publish the same prepared result");
    assert_eq!(publication.preparation_id, preparation_id);
    let singleton: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM provider_result_preparations WHERE id=$1), \
            (SELECT COUNT(*) FROM provider_result_publications WHERE provider_result_preparation_id=$1), \
            (SELECT COUNT(*) FROM artifact_revision_contents WHERE provider_result_preparation_id=$1), \
            (SELECT COUNT(*) FROM run_work_items WHERE id=$2)",
    )
    .bind(preparation_id)
    .bind(advance_work_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(singleton, (1, 1, 1, 1));
    assert_eq!(vault.counts(), (0, 1));
    restarted_runtime.close().await;
}

#[derive(Default)]
struct ProviderResultTestVault {
    key: Mutex<Option<(MaterialKeyId, IntentNonce, VaultReceipt)>>,
    creates: AtomicUsize,
    unwraps: AtomicUsize,
}

impl ProviderResultTestVault {
    fn counts(&self) -> (usize, usize) {
        (
            self.creates.load(Ordering::SeqCst),
            self.unwraps.load(Ordering::SeqCst),
        )
    }
}

impl MaterialKeyVault for ProviderResultTestVault {
    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        let mut key = self
            .key
            .lock()
            .expect("provider-result vault lock poisoned");
        if let Some((existing_key, existing_nonce, receipt)) = *key {
            return if existing_key == key_id && existing_nonce == nonce {
                Ok(receipt)
            } else {
                Err(VaultError::NonceMismatch)
            };
        }
        let receipt = VaultReceipt::new();
        *key = Some((key_id, nonce, receipt));
        Ok(receipt)
    }

    fn unwrap(
        &self,
        key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        let key = self
            .key
            .lock()
            .expect("provider-result vault lock poisoned");
        if !matches!(*key, Some((existing, _, _)) if existing == key_id) {
            return Err(VaultError::NotFound);
        }
        self.unwraps.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([0x5d; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        unreachable!("provider-result publication test never erases its key")
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        unreachable!("provider-result publication test never erases its key")
    }
}

struct OneShotProviderResultFault {
    point: ProviderResultFaultPoint,
    fired: AtomicBool,
}

impl OneShotProviderResultFault {
    fn new(point: ProviderResultFaultPoint) -> Self {
        Self {
            point,
            fired: AtomicBool::new(false),
        }
    }
}

impl ProviderResultFaultInjector for OneShotProviderResultFault {
    fn check(&self, point: ProviderResultFaultPoint) -> Result<(), ApplicationError> {
        if point == self.point && !self.fired.swap(true, Ordering::SeqCst) {
            return Err(ApplicationError::Internal(format!(
                "injected provider-result fault at {point:?}"
            )));
        }
        Ok(())
    }
}

fn retained_chat_result(content: &str) -> EffectiveChatResult {
    RESULT_CONTENT_OBSERVED.store(false, Ordering::SeqCst);
    RESULT_CONTENT_ZERO.store(false, Ordering::SeqCst);
    let result = EffectiveChatResult::new(
        zeroize::Zeroizing::new(content.to_owned()),
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
        ProviderUsage::Known {
            prompt_tokens: 7,
            completion_tokens: 11,
        },
    )
    .unwrap();
    RESULT_CONTENT_LEN.store(result.content().len(), Ordering::SeqCst);
    RESULT_CONTENT_POINTER.store(result.content().as_ptr() as usize, Ordering::SeqCst);
    result
}

fn assert_retained_result_zeroized(point: ProviderResultFaultPoint) {
    assert!(
        RESULT_CONTENT_OBSERVED.load(Ordering::SeqCst),
        "{point:?} did not deallocate the exact retained-result allocation"
    );
    assert!(
        RESULT_CONTENT_ZERO.load(Ordering::SeqCst),
        "{point:?} deallocated retained-result content before zeroization"
    );
}

fn discard_committed_prepared_result(_prepared: vestrace_application::PreparedProviderResult) {}

fn provider_result_identities(
    ids: ProviderResultEnvelopeIds,
    advance_work_item_id: WorkItemId,
) -> ProviderResultIdentities {
    ProviderResultIdentities {
        material_intent_id: MaterialKeyCreationIntentId::new(),
        content_material_id: ContentMaterialId::new(),
        material_key_id: MaterialKeyId::new(),
        intent_nonce: IntentNonce::new(),
        prepared_attachment_id: PreparedMaterialAttachmentId::new(),
        receipt_id: ExternalEffectReceiptId::new(),
        artifact_id: ArtifactId::from_uuid(ids.artifact_id),
        artifact_revision_id: ArtifactRevisionId::from_uuid(ids.artifact_revision_id),
        model_execution_id: ModelExecutionId::from_uuid(ids.model_execution_id),
        advance_work_item_id,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_advance_work_identity_collision_is_typed_and_atomic(pool: PgPool) {
    let first = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    let second = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, first).await;
    seed_provider_result_envelope(&pool, second).await;
    let runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(runtime.clone());
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let runs = Arc::new(PostgresRunStore::new(&store));
    let permit = Arc::new(PgInstallationMutationPermit::new(store));
    let codec = Arc::new(ContentMaterialCodec::new());
    let shared_work_item_id = WorkItemId::new();
    let first_identities = provider_result_identities(first, shared_work_item_id);
    let second_identities = provider_result_identities(second, shared_work_item_id);
    let first_repository = PgProviderResultRepository::new(
        permit.clone(),
        Arc::new(ProviderResultTestVault::default()),
        codec.clone(),
        effects.clone(),
        runs.clone(),
    );
    first_repository
        .prepare(PrepareProviderResult {
            context: RequestContext::new(
                WorkspaceId::from_uuid(first.workspace_id),
                PrincipalId::from_uuid(first.principal_id),
            ),
            effect_id: ExternalEffectId::from_uuid(first.effect_id),
            run_id: AgentRunId::from_uuid(first.run_id),
            step_id: RunStepId::from_uuid(first.step_id),
            identities: first_identities,
            result: EffectiveChatResult::new(
                zeroize::Zeroizing::new("first provider result".to_owned()),
                EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                ProviderUsage::Known {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                },
            )
            .unwrap(),
        })
        .await
        .unwrap();
    let second_repository = PgProviderResultRepository::new(
        permit,
        Arc::new(ProviderResultTestVault::default()),
        codec,
        effects,
        runs,
    );
    let error = second_repository
        .prepare(PrepareProviderResult {
            context: RequestContext::new(
                WorkspaceId::from_uuid(second.workspace_id),
                PrincipalId::from_uuid(second.principal_id),
            ),
            effect_id: ExternalEffectId::from_uuid(second.effect_id),
            run_id: AgentRunId::from_uuid(second.run_id),
            step_id: RunStepId::from_uuid(second.step_id),
            identities: second_identities,
            result: retained_chat_result("second provider result"),
        })
        .await
        .expect_err("a cross-effect continuation identity collision must be typed");
    assert!(
        matches!(error, ApplicationError::Conflict(ref code) if code == "PROVIDER_RESULT_CONFLICT"),
        "unexpected collision error: {error:?}"
    );
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$1), \
            (SELECT COUNT(*) FROM material_key_creation_intents WHERE owner_id=$1 AND owner_kind='provider_result'), \
            (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$2), \
            (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$1), \
            (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$3 AND advance_work_item_id=$4)",
    )
    .bind(second.effect_id)
    .bind(second_identities.material_intent_id.as_uuid())
    .bind(first.effect_id)
    .bind(shared_work_item_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 1));
    let known_zero: (bool, Option<i32>, Option<i32>) = sqlx::query_as(
        "SELECT usage_known,prompt_tokens,completion_tokens \
           FROM provider_result_preparations WHERE external_effect_id=$1",
    )
    .bind(first.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(known_zero, (true, Some(0), Some(0)));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_provider_result_prepare_fails_closed_without_dispatch_release_and_rolls_back(
    pool: PgPool,
) {
    let _observer = observe_retained_result().await;
    let ids = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, ids).await;

    let runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(runtime.clone());
    let repository = PgProviderResultRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(ProviderResultTestVault::default()),
        Arc::new(ContentMaterialCodec::new()),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PostgresRunStore::new(&store)),
    );
    let context = RequestContext::new(
        WorkspaceId::from_uuid(ids.workspace_id),
        PrincipalId::from_uuid(ids.principal_id),
    );
    let identities = provider_result_identities(ids, WorkItemId::new());
    let error = repository
        .prepare_after_dispatch(
            PrepareProviderResult {
                context: context.clone(),
                effect_id: ExternalEffectId::from_uuid(ids.effect_id),
                run_id: AgentRunId::from_uuid(ids.run_id),
                step_id: RunStepId::from_uuid(ids.step_id),
                identities,
                result: retained_chat_result("missing-release-authority"),
            },
            &ProviderDispatchAuthority {
                effect_id: ExternalEffectId::from_uuid(ids.effect_id),
                authorization_id: PolicyDecisionId::new(),
                connection_id: ConnectionId::new(),
                connection_revision_id: ConnectionRevisionId::new(),
                concurrency_lease_id: Uuid::now_v7(),
                credential_lease_id: None,
                dispatch_transition_id: ExternalEffectLifecycleTransitionId::new(),
                dispatch_expires_at: Utc::now(),
            },
        )
        .await
        .expect_err("Run result preparation must not commit without dispatch-release authority");
    assert!(
        matches!(error, ApplicationError::Policy(ref message) if message == "Run provider-result preparation has no dispatch completion authority")
    );
    assert_retained_result_zeroized(ProviderResultFaultPoint::AfterWitness);
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM material_key_creation_intents WHERE id=$1), \
            (SELECT COUNT(*) FROM provider_result_preparations WHERE external_effect_id=$2), \
            (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$2), \
            (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$2)",
    )
    .bind(identities.material_intent_id.as_uuid())
    .bind(ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_result_repository_recovers_one_atomic_publication_and_exact_replay(pool: PgPool) {
    let _observer = observe_retained_result().await;
    const SENTINEL: &str = "task10d-retained-provider-result-sentinel";
    let ids = ProviderResultEnvelopeIds {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        effect_id: Uuid::now_v7(),
        run_id: Uuid::now_v7(),
        step_id: Uuid::now_v7(),
        artifact_id: Uuid::now_v7(),
        artifact_revision_id: Uuid::now_v7(),
        model_execution_id: Uuid::now_v7(),
        request_kind: "chat_completions",
        model_kind: "chat",
    };
    seed_provider_result_envelope(&pool, ids).await;
    sqlx::query("DELETE FROM artifact_revisions WHERE id=$1")
        .bind(ids.artifact_revision_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(ids.artifact_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM model_executions WHERE id=$1")
        .bind(ids.model_execution_id)
        .execute(&pool)
        .await
        .unwrap();

    let runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(runtime.clone());
    let vault = Arc::new(ProviderResultTestVault::default());
    let codec = Arc::new(ContentMaterialCodec::new());
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let runs = Arc::new(PostgresRunStore::new(&store));
    let permit = Arc::new(PgInstallationMutationPermit::new(store));
    let identities = ProviderResultIdentities {
        material_intent_id: MaterialKeyCreationIntentId::new(),
        content_material_id: ContentMaterialId::new(),
        material_key_id: MaterialKeyId::new(),
        intent_nonce: IntentNonce::new(),
        prepared_attachment_id: PreparedMaterialAttachmentId::new(),
        receipt_id: ExternalEffectReceiptId::new(),
        artifact_id: ArtifactId::from_uuid(ids.artifact_id),
        artifact_revision_id: ArtifactRevisionId::from_uuid(ids.artifact_revision_id),
        model_execution_id: ModelExecutionId::from_uuid(ids.model_execution_id),
        advance_work_item_id: WorkItemId::new(),
    };
    let context = RequestContext::new(
        WorkspaceId::from_uuid(ids.workspace_id),
        PrincipalId::from_uuid(ids.principal_id),
    );
    let repository = PgProviderResultRepository::new(
        permit.clone(),
        vault.clone(),
        codec.clone(),
        effects.clone(),
        runs.clone(),
    );
    for point in [
        ProviderResultFaultPoint::BeforeReserve,
        ProviderResultFaultPoint::AfterReserve,
        ProviderResultFaultPoint::AfterVaultCreate,
        ProviderResultFaultPoint::AfterVaultReceipt,
        ProviderResultFaultPoint::AfterEncryption,
        ProviderResultFaultPoint::AfterResultPrepared,
        ProviderResultFaultPoint::AfterReceipt,
        ProviderResultFaultPoint::AfterWitness,
    ] {
        let faulting = PgProviderResultRepository::with_faults(
            permit.clone(),
            vault.clone(),
            codec.clone(),
            effects.clone(),
            runs.clone(),
            Arc::new(OneShotProviderResultFault::new(point)),
        );
        assert!(
            faulting
                .prepare(PrepareProviderResult {
                    context: context.clone(),
                    effect_id: ExternalEffectId::from_uuid(ids.effect_id),
                    run_id: AgentRunId::from_uuid(ids.run_id),
                    step_id: RunStepId::from_uuid(ids.step_id),
                    identities,
                    result: retained_chat_result(SENTINEL),
                })
                .await
                .is_err(),
            "{point:?} must roll back retained-result preparation"
        );
        assert_retained_result_zeroized(point);
        let absent: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
                (SELECT COUNT(*) FROM material_key_creation_intents WHERE id=$1), \
                (SELECT COUNT(*) FROM provider_result_preparations WHERE material_intent_id=$1), \
                (SELECT COUNT(*) FROM content_material_bytes WHERE intent_id=$1), \
                (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$2)",
        )
        .bind(identities.material_intent_id.as_uuid())
        .bind(ids.effect_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(absent, (0, 0, 0, 0), "{point:?}");
    }
    let provider_request_count = Arc::new(AtomicUsize::new(0));
    let request_count = provider_request_count.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let provider = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 8192];
        let _ = socket.read(&mut request).unwrap();
        request_count.fetch_add(1, Ordering::SeqCst);
        let body = format!(
            "{{\"choices\":[{{\"message\":{{\"content\":\"{SENTINEL}\"}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":11}}}}"
        );
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();
    let result = client
        .execute_effective(
            EffectiveModelRequest::chat_completions(
                "model",
                vec![EffectiveChatMessage::user("retained provider result")],
                EffectiveSampling::new(0.0, 1.0).unwrap(),
                EffectiveRequestLimits::new(32, 1, 32).unwrap(),
                Vec::new(),
                false,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    provider.join().unwrap();
    let EffectiveModelResponse::ChatCompletions(result) = result else {
        panic!("governed loopback returned a non-chat result");
    };
    assert_eq!(result.content(), SENTINEL);
    RESULT_CONTENT_OBSERVED.store(false, Ordering::SeqCst);
    RESULT_CONTENT_ZERO.store(false, Ordering::SeqCst);
    RESULT_CONTENT_LEN.store(result.content().len(), Ordering::SeqCst);
    RESULT_CONTENT_POINTER.store(result.content().as_ptr() as usize, Ordering::SeqCst);

    let prepared = repository
        .prepare(PrepareProviderResult {
            context: context.clone(),
            effect_id: ExternalEffectId::from_uuid(ids.effect_id),
            run_id: AgentRunId::from_uuid(ids.run_id),
            step_id: RunStepId::from_uuid(ids.step_id),
            identities,
            result,
        })
        .await
        .unwrap();
    assert!(RESULT_CONTENT_OBSERVED.load(Ordering::SeqCst));
    assert!(RESULT_CONTENT_ZERO.load(Ordering::SeqCst));
    assert_eq!(provider_request_count.load(Ordering::SeqCst), 1);
    let after_prepare_vault = vault.counts();
    assert!(after_prepare_vault.0 >= 1);
    assert!(after_prepare_vault.1 >= 1);
    let prepared_replay = repository
        .prepare(PrepareProviderResult {
            context: context.clone(),
            effect_id: ExternalEffectId::from_uuid(ids.effect_id),
            run_id: AgentRunId::from_uuid(ids.run_id),
            step_id: RunStepId::from_uuid(ids.step_id),
            identities,
            result: retained_chat_result(SENTINEL),
        })
        .await
        .expect("exact durable prepare replay must not reseal fresh AEAD bytes");
    assert_eq!(prepared_replay.preparation_id, prepared.preparation_id);
    assert_eq!(vault.counts(), after_prepare_vault);
    assert_eq!(provider_request_count.load(Ordering::SeqCst), 1);
    discard_committed_prepared_result(prepared_replay);
    let durable_frame: (i64, bool) = sqlx::query_as(
        "SELECT COUNT(*), \
                COALESCE(BOOL_AND(position(convert_to($2,'UTF8') in ciphertext)=0),TRUE) \
           FROM content_material_bytes WHERE intent_id=$1",
    )
    .bind(identities.material_intent_id.as_uuid())
    .bind(SENTINEL)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(durable_frame, (1, true));

    let preparation_id = prepared.preparation_id;
    discard_committed_prepared_result(prepared);
    drop(repository);
    runtime.close().await;

    let runtime = runtime_pool(&pool).await;
    let store = PgStore::from_pool(runtime.clone());
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let runs = Arc::new(PostgresRunStore::new(&store));
    let permit = Arc::new(PgInstallationMutationPermit::new(store));
    let repository = PgProviderResultRepository::new(
        permit.clone(),
        vault.clone(),
        codec.clone(),
        effects.clone(),
        runs.clone(),
    );
    let prepared = repository
        .recover_result_prepared(&context, ExternalEffectId::from_uuid(ids.effect_id))
        .await
        .unwrap();
    assert_eq!(prepared.preparation_id, preparation_id);
    assert_eq!(prepared.identities, identities);
    assert_eq!(
        prepared.evidence,
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop)
    );
    assert_eq!(
        prepared.usage,
        ProviderUsage::Known {
            prompt_tokens: 7,
            completion_tokens: 11,
        }
    );
    assert_eq!(vault.counts(), after_prepare_vault);
    assert_eq!(provider_request_count.load(Ordering::SeqCst), 1);

    let mut wrong_receipt = prepared.clone();
    wrong_receipt.identities.receipt_id = ExternalEffectReceiptId::new();
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: wrong_receipt,
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    let mut wrong_work = prepared.clone();
    wrong_work.identities.advance_work_item_id = WorkItemId::new();
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: wrong_work,
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    let mut wrong_evidence = prepared.clone();
    wrong_evidence.evidence = EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Length);
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: wrong_evidence,
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    let mut wrong_usage = prepared.clone();
    wrong_usage.usage = ProviderUsage::Unknown;
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: wrong_usage,
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));

    sqlx::query("INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,'wrong-name')")
        .bind(ids.artifact_id)
        .bind(ids.workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: prepared.clone(),
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(ids.artifact_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,$3)")
        .bind(ids.artifact_id)
        .bind(ids.workspace_id)
        .bind(format!("provider-result-{}", ids.artifact_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifact_revisions(id,artifact_id,workspace_id,revision_number,media_type,content_hash,byte_size,storage_kind) VALUES($1,$2,$3,2,'application/octet-stream','wrong',1,'legacy_digest')")
        .bind(ids.artifact_revision_id)
        .bind(ids.artifact_id)
        .bind(ids.workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: prepared.clone(),
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    sqlx::query("DELETE FROM artifact_revisions WHERE id=$1")
        .bind(ids.artifact_revision_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(ids.artifact_id)
        .execute(&pool)
        .await
        .unwrap();

    let snapshot_model_id: Uuid = sqlx::query_scalar(
        "SELECT revision.model_id FROM provider_dispatch_causes AS cause \
          JOIN model_binding_snapshots AS snapshot ON snapshot.workspace_id=cause.workspace_id AND snapshot.id=cause.model_binding_snapshot_id \
          JOIN model_revisions AS revision ON revision.workspace_id=snapshot.workspace_id AND revision.id=snapshot.model_revision_id \
         WHERE cause.external_effect_id=$1",
    )
    .bind(ids.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO model_executions(id,workspace_id,model_id,usage_known,prompt_tokens,completion_tokens,latency_ms,status) VALUES($1,$2,$3,FALSE,0,0,1,'succeeded')")
        .bind(ids.model_execution_id)
        .bind(ids.workspace_id)
        .bind(snapshot_model_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: prepared.clone(),
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));
    sqlx::query("DELETE FROM model_executions WHERE id=$1")
        .bind(ids.model_execution_id)
        .execute(&pool)
        .await
        .unwrap();

    let finalize = FinalizeProviderResult {
        prepared: prepared.clone(),
        binding_receipt: MaterialKeyBindingReceipt::new(),
    };
    let mut commit_error = permit.acquire(PermitMode::Shared, &context).await.unwrap();
    repository
        .finalize_in(&context, commit_error.unit_of_work_mut(), finalize.clone())
        .await
        .unwrap();
    let transaction = commit_error
        .unit_of_work_mut()
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .expect("provider-result commit-error proof requires PostgreSQL transaction");
    let incomplete_artifact_id = Uuid::now_v7();
    sqlx::query("INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,'incomplete')")
        .bind(incomplete_artifact_id)
        .bind(ids.workspace_id)
        .execute(transaction.connection())
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifact_revisions(id,artifact_id,workspace_id,revision_number,media_type,content_hash,byte_size,storage_kind) VALUES($1,$2,$3,1,'text/plain',NULL,NULL,'governed_material')")
        .bind(Uuid::now_v7())
        .bind(incomplete_artifact_id)
        .bind(ids.workspace_id)
        .execute(transaction.connection())
        .await
        .unwrap();
    assert!(
        commit_error.commit().await.is_err(),
        "a real deferred database commit error must reject every finalization leg"
    );
    for point in [
        ProviderResultFaultPoint::AfterBind,
        ProviderResultFaultPoint::AfterRuntimeEnvelopes,
        ProviderResultFaultPoint::AfterLivePromotion,
        ProviderResultFaultPoint::BeforeRunSuccess,
    ] {
        let faulting = PgProviderResultRepository::with_faults(
            permit.clone(),
            vault.clone(),
            codec.clone(),
            effects.clone(),
            runs.clone(),
            Arc::new(OneShotProviderResultFault::new(point)),
        );
        assert!(
            faulting.finalize(&context, finalize.clone()).await.is_err(),
            "{point:?} must roll back provider-owned finalization"
        );
    }
    let rolled_back: (String, String, i64, i64, i64, i64, String, i64) = sqlx::query_as(
        "SELECT intent.state, preparation.state, \
                (SELECT COUNT(*) FROM artifacts WHERE id=$3), \
                (SELECT COUNT(*) FROM artifact_revisions WHERE id=$4), \
                (SELECT COUNT(*) FROM model_executions WHERE id=$5), \
                (SELECT COUNT(*) FROM provider_result_publications WHERE provider_result_preparation_id=$2), \
                (SELECT status FROM run_steps WHERE id=$6), \
                (SELECT run_version FROM agent_runs WHERE id=$7) \
           FROM material_key_creation_intents AS intent \
           JOIN provider_result_preparations AS preparation ON preparation.material_intent_id=intent.id \
          WHERE intent.id=$1 AND preparation.id=$2",
    )
    .bind(identities.material_intent_id.as_uuid())
    .bind(prepared.preparation_id)
    .bind(ids.artifact_id)
    .bind(ids.artifact_revision_id)
    .bind(ids.model_execution_id)
    .bind(ids.step_id)
    .bind(ids.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        rolled_back,
        (
            "result_prepared".into(),
            "result_prepared".into(),
            0,
            0,
            0,
            0,
            "running".into(),
            1,
        )
    );

    let publication = repository
        .finalize(&context, finalize.clone())
        .await
        .unwrap();
    let replay = repository.finalize(&context, finalize).await.unwrap();
    assert_eq!(publication, replay);
    assert!(matches!(
        repository
            .finalize(
                &context,
                FinalizeProviderResult {
                    prepared: prepared.clone(),
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await,
        Err(ApplicationError::Conflict(ref code)) if code == "PROVIDER_RESULT_CONFLICT"
    ));

    // The legacy storage API stays available for an older digest-backed
    // artifact.  Its one blob must not be required for, or leak into, the
    // published provider-material projection.
    let stored_legacy = PgArtifactRepository::new(PgStore::from_pool(pool.clone()))
        .store(
            &context,
            "legacy-compatible-artifact",
            &ArtifactContent {
                media_type: "application/x-legacy".to_owned(),
                bytes: b"legacy bytes only".to_vec(),
            },
        )
        .await
        .expect("an owner-scoped legacy artifact is a lawful mixed-list fixture");
    let expected_governed: (Uuid, Vec<u8>, i64, String) = sqlx::query_as(
        "SELECT content_material_id,erasure_bound_commitment,size_class,media_class \
           FROM artifact_revision_contents \
          WHERE workspace_id=$1 AND artifact_revision_id=$2",
    )
    .bind(ids.workspace_id)
    .bind(ids.artifact_revision_id)
    .fetch_one(&pool)
    .await
    .expect("published provider output has one governed content record");
    let expected_commitment: [u8; 32] = expected_governed
        .1
        .try_into()
        .expect("governed artifact commitments have an exact 32-byte shape");

    let mixed = PgArtifactRepository::new(PgStore::from_pool(runtime.clone()))
        .list(&context, 10)
        .await
        .expect("restricted runtime can safely list legacy and provider artifacts together");
    let governed = mixed
        .iter()
        .find(|listing| listing.artifact.id == identities.artifact_id)
        .expect("the published provider artifact is listed");
    assert!(
        governed.latest_revision.is_none(),
        "governed material must not re-enter the legacy digest projection"
    );
    let material = governed
        .governed_material
        .as_ref()
        .expect("the provider artifact has the typed safe material projection");
    assert_eq!(
        material.artifact_revision_id,
        identities.artifact_revision_id
    );
    assert_eq!(material.content_material_id.as_uuid(), expected_governed.0);
    assert_eq!(material.erasure_bound_commitment, expected_commitment);
    assert_eq!(material.size_class, prepared.size_class);
    assert_eq!(
        material.size_class.minimum_bytes() as i64,
        expected_governed.2
    );
    assert_eq!(material.media_class.as_str(), expected_governed.3);

    let legacy = mixed
        .iter()
        .find(|listing| listing.artifact.id == stored_legacy.artifact_id)
        .expect("the historical digest artifact remains visible beside provider material");
    assert!(legacy.governed_material.is_none());
    let legacy_revision = legacy
        .latest_revision
        .as_ref()
        .expect("only the historical artifact exposes legacy digest metadata");
    assert_eq!(legacy_revision.id, stored_legacy.revision_id);
    assert_eq!(legacy_revision.media_type, "application/x-legacy");
    assert_eq!(legacy_revision.content_hash, stored_legacy.content_hash);
    assert_eq!(legacy_revision.byte_size, stored_legacy.byte_size);

    let blobs: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE content_hash=$1) FROM artifact_blobs",
    )
    .bind(&stored_legacy.content_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        blobs,
        (1, 1),
        "artifact_blobs contains the public legacy fixture only, never provider material"
    );

    let singleton: (i64, i64, i64, i64, i64, i64, String, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM artifacts WHERE id=$1), \
            (SELECT COUNT(*) FROM artifact_revisions WHERE id=$2), \
            (SELECT COUNT(*) FROM model_executions WHERE id=$3), \
            (SELECT COUNT(*) FROM provider_result_publications WHERE provider_result_preparation_id=$4), \
            (SELECT COUNT(*) FROM artifact_revision_contents WHERE provider_result_preparation_id=$4), \
            (SELECT COUNT(*) FROM run_work_items WHERE id=$5), \
            (SELECT status FROM run_steps WHERE id=$6), \
            (SELECT run_version FROM agent_runs WHERE id=$7)",
    )
    .bind(ids.artifact_id)
    .bind(ids.artifact_revision_id)
    .bind(ids.model_execution_id)
    .bind(prepared.preparation_id)
    .bind(identities.advance_work_item_id.as_uuid())
    .bind(ids.step_id)
    .bind(ids.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(singleton, (1, 1, 1, 1, 1, 1, "succeeded".into(), 2));
    let final_vault = vault.counts();
    assert_eq!(final_vault.0, after_prepare_vault.0);
    assert_eq!(final_vault.1, after_prepare_vault.1 + 5);
    assert_eq!(provider_request_count.load(Ordering::SeqCst), 1);
    runtime.close().await;
}
