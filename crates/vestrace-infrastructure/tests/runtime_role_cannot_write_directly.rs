use std::{borrow::Cow, str::FromStr};

use sqlx::{
    PgPool, Row,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const RUNTIME_ROLE: &str = "vestrace";
const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const RUNTIME_ROLE_PROVISIONING: &str =
    include_str!("../../../docker/postgres/init-runtime-role.sh");
const P02_MIGRATION_SOURCES: [&str; 9] = [
    include_str!("../../../migrations/0166_guarded_operation_owner_role.sql"),
    include_str!("../../../migrations/0167_installation_fingerprint_continuity.sql"),
    include_str!("../../../migrations/0168_installation_mutation_permit.sql"),
    include_str!("../../../migrations/0169_material_key_creation_intents.sql"),
    include_str!("../../../migrations/0170_content_material_guards.sql"),
    include_str!("../../../migrations/0171_connection_execution_guards.sql"),
    include_str!("../../../migrations/0172_credential_slots_and_revisions.sql"),
    include_str!("../../../migrations/0173_credential_key_creation_intents.sql"),
    include_str!("../../../migrations/0174_material_erasure_primitives.sql"),
];
const P03_GUARDED_TABLES: [&str; 40] = [
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
    "embedding_job_material_intents",
    "embedding_job_termination_receipts",
];

fn normalized_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
        .replace("timestamptz", "timestampwithtimezone")
}

fn has_explicit_runtime_execute_grant(regprocedure: &str) -> bool {
    let statement = normalized_sql(&format!(
        "GRANT EXECUTE ON FUNCTION {regprocedure} TO vestrace;"
    ));
    P02_MIGRATION_SOURCES
        .iter()
        .map(|migration| normalized_sql(migration))
        .any(|migration| migration.contains(&statement))
}

fn runtime_role_provisioning_bridge_sql() -> &'static str {
    let bridge_start = RUNTIME_ROLE_PROVISIONING
        .find("-- P02 migrations run as the runtime role")
        .expect("the deployment provisioner must contain the bounded P02 bridge marker");
    RUNTIME_ROLE_PROVISIONING[bridge_start..]
        .rsplit_once("\nSQL\n")
        .map(|(sql, _)| sql)
        .expect("the deployment provisioner must contain the bounded P02 bridge SQL")
}

fn regprocedure_array_count(source: &str, declaration: &str, terminator: &str) -> usize {
    source
        .split_once(declaration)
        .unwrap_or_else(|| panic!("missing {declaration}"))
        .1
        .split_once(terminator)
        .unwrap_or_else(|| panic!("missing {terminator} after {declaration}"))
        .0
        .matches("to_regprocedure('public.")
        .count()
}

fn assert_insufficient_privilege<T>(result: Result<T, sqlx::Error>, operation: &str) {
    let error = match result {
        Ok(_) => panic!("the runtime role unexpectedly {operation}"),
        Err(error) => error,
    };
    let sqlstate = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned());

    assert_eq!(
        sqlstate.as_deref(),
        Some("42501"),
        "{operation} must be refused with SQLSTATE 42501 (insufficient_privilege), got {}",
        sqlstate.as_deref().unwrap_or("no SQLSTATE")
    );
    if operation.starts_with("directly") {
        let database_error = error
            .as_database_error()
            .expect("a direct table write must return a database error");
        assert!(
            database_error
                .message()
                .contains("permission denied for table"),
            "{operation} must be refused by the table ACL rather than RLS, got {:?}",
            database_error.message(),
        );
    }
}

fn assert_check_violation<T>(result: Result<T, sqlx::Error>, operation: &str) {
    let error = match result {
        Ok(_) => panic!("the runtime role unexpectedly {operation}"),
        Err(error) => error,
    };
    let sqlstate = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned());
    assert_eq!(sqlstate.as_deref(), Some("23514"), "{operation}: {error}");
}

fn assert_exact_check_violation<T>(result: Result<T, sqlx::Error>, message: &str, operation: &str) {
    let error = match result {
        Ok(_) => panic!("the runtime role unexpectedly {operation}"),
        Err(error) => error,
    };
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("{operation} returned a non-database error: {error}"));
    assert_eq!(database.code().as_deref(), Some("23514"), "{operation}");
    assert_eq!(database.message(), message, "{operation}");
}

fn assert_insufficient_table_privilege<T>(result: Result<T, sqlx::Error>, operation: &str) {
    let error = match result {
        Ok(_) => panic!("the runtime role unexpectedly {operation}"),
        Err(error) => error,
    };
    let database_error = error
        .as_database_error()
        .expect("a direct table write must return a database error");

    assert_eq!(
        database_error.code().as_deref(),
        Some("42501"),
        "{operation} must be refused with SQLSTATE 42501 (insufficient_privilege), got {}",
        database_error.code().as_deref().unwrap_or("no SQLSTATE")
    );
    assert!(
        database_error
            .message()
            .contains("permission denied for table"),
        "{operation} must be refused by the table ACL rather than RLS, got {:?}",
        database_error.message(),
    );
}

fn runtime_credentials() -> (String, String) {
    let runtime_database_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role");
    PgConnectOptions::from_str(&runtime_database_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let authority = runtime_database_url
        .split_once("://")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a scheme")
        .1;
    let credentials = authority
        .rsplit_once('@')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include runtime credentials")
        .0;
    let (username, password) = credentials
        .split_once(':')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a runtime password");

    assert_eq!(
        username, RUNTIME_ROLE,
        "VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role"
    );

    (username.to_owned(), password.to_owned())
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let (username, password) = runtime_credentials();
    let options = source
        .connect_options()
        .as_ref()
        .clone()
        .username(&username)
        .password(&password);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("the runtime role must be able to connect");

    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&pool)
    .await
    .expect("the runtime role identity must be observable");
    assert_eq!(role.get::<String, _>("role"), RUNTIME_ROLE);
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));

    pool
}

#[derive(Clone, Copy)]
struct CandidateFixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    connection_id: Uuid,
    slot_id: Uuid,
    intent_id: Uuid,
    revision_id: Uuid,
}

async fn create_candidate_fixture(
    pool: &PgPool,
    runtime: &PgPool,
    label: &str,
) -> CandidateFixture {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace_id)
        .bind(format!("erasure-{label}-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("erasure-{label}-{principal_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id,workspace_id,name,provider_type) VALUES ($1,$2,$3,'openai')",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("erasure-{label}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections (id,connector_id,workspace_id,principal_id,name) VALUES ($1,$2,$3,$4,$5)")
        .bind(connection_id).bind(connector_id).bind(workspace_id).bind(principal_id).bind(format!("erasure-{label}"))
        .execute(pool).await.unwrap();

    let slot_id = Uuid::now_v7();
    let occupancy_id = Uuid::now_v7();
    let intent_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider',$4)")
        .bind(slot_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(format!("slot-{label}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1,$2,$3,$4)")
        .bind(occupancy_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_key_creation_intent($1,$2,$3,$4,$5,$6,$7,$8,'credential_v2')")
        .bind(intent_id).bind(workspace_id).bind(connection_id).bind(slot_id).bind(occupancy_id)
        .bind(revision_id).bind(Uuid::now_v7()).bind(Uuid::now_v7())
        .execute(&mut *transaction).await.unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
        .bind(intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_credential_prepared_material($1,$2,$3)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .bind(vec![0xc3_u8; 32])
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
        .bind(intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    CandidateFixture {
        workspace_id,
        principal_id,
        connection_id,
        slot_id,
        intent_id,
        revision_id,
    }
}

async fn create_sibling_candidate_fixture(
    runtime: &PgPool,
    base: CandidateFixture,
    label: &str,
) -> CandidateFixture {
    let slot_id = Uuid::now_v7();
    let occupancy_id = Uuid::now_v7();
    let intent_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(base.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider',$4)")
        .bind(slot_id)
        .bind(base.workspace_id)
        .bind(base.connection_id)
        .bind(format!("slot-{label}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
        .bind(Uuid::now_v7())
        .bind(base.workspace_id)
        .bind(base.connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1,$2,$3,$4)")
        .bind(occupancy_id)
        .bind(base.workspace_id)
        .bind(base.connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_key_creation_intent($1,$2,$3,$4,$5,$6,$7,$8,'credential_v2')")
        .bind(intent_id).bind(base.workspace_id).bind(base.connection_id).bind(slot_id).bind(occupancy_id)
        .bind(revision_id).bind(Uuid::now_v7()).bind(Uuid::now_v7())
        .execute(&mut *transaction).await.unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
        .bind(intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_credential_prepared_material($1,$2,$3)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .bind(vec![0x7c_u8; 32])
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1,$2)")
        .bind(intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
        .bind(intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    CandidateFixture {
        slot_id,
        intent_id,
        revision_id,
        ..base
    }
}

async fn append_activation_fixture(pool: &PgPool, fixture: CandidateFixture, event_kind: &str) {
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events (id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES ($1,$2,$3,'credential.fixture','credential',$4,'{}'::jsonb,NOW())")
        .bind(audit_id).bind(fixture.workspace_id).bind(fixture.principal_id).bind(fixture.revision_id)
        .execute(pool).await.unwrap();
    let mut guarded = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    sqlx::query("INSERT INTO credential_activation_events (id,workspace_id,connection_id,credential_slot_id,credential_revision_id,credential_intent_id,event_kind,expected_slot_version,resulting_slot_version,audit_event_id) VALUES ($1,$2,$3,$4,$5,$6,$7,0,1,$8)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(fixture.connection_id).bind(fixture.slot_id)
        .bind(fixture.revision_id).bind(fixture.intent_id).bind(event_kind).bind(audit_id)
        .execute(&mut *guarded).await.unwrap();
    if event_kind == "active" {
        sqlx::query("UPDATE credential_slots SET current_revision_id=$2,current_revision_version=1 WHERE id=$1")
            .bind(fixture.slot_id).bind(fixture.revision_id).execute(&mut *guarded).await.unwrap();
    }
    guarded.commit().await.unwrap();
}

async fn append_rotation_fixture(pool: &PgPool, fixture: CandidateFixture) {
    let activated_revision_id = Uuid::now_v7();
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events (id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES ($1,$2,$3,'credential.fixture','credential',$4,'{}'::jsonb,NOW())")
        .bind(audit_id).bind(fixture.workspace_id).bind(fixture.principal_id).bind(fixture.revision_id)
        .execute(pool).await.unwrap();
    let mut guarded = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    sqlx::query("INSERT INTO credential_revisions (id,workspace_id,credential_slot_id,material_key_id,associated_data_profile) VALUES ($1,$2,$3,$4,'credential_v2')")
        .bind(activated_revision_id).bind(fixture.workspace_id).bind(fixture.slot_id).bind(Uuid::now_v7())
        .execute(&mut *guarded).await.unwrap();
    sqlx::query("INSERT INTO credential_rotation_events (id,workspace_id,connection_id,credential_slot_id,previous_credential_revision_id,activated_credential_revision_id,expected_slot_version,resulting_slot_version,audit_event_id) VALUES ($1,$2,$3,$4,$5,$6,1,2,$7)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(fixture.connection_id).bind(fixture.slot_id)
        .bind(fixture.revision_id).bind(activated_revision_id).bind(audit_id)
        .execute(&mut *guarded).await.unwrap();
    guarded.commit().await.unwrap();
}

async fn assert_activation_connection_mismatch(pool: &PgPool, fixture: CandidateFixture) {
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events (id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES ($1,$2,$3,'credential.mismatch','credential',$4,'{}'::jsonb,NOW())")
        .bind(audit_id).bind(fixture.workspace_id).bind(fixture.principal_id).bind(fixture.revision_id)
        .execute(pool).await.unwrap();
    let mut guarded = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO credential_activation_events (id,workspace_id,connection_id,credential_slot_id,credential_revision_id,credential_intent_id,event_kind,expected_slot_version,resulting_slot_version,audit_event_id) VALUES ($1,$2,$3,$4,$5,$6,'revoked',0,1,$7)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(Uuid::now_v7()).bind(fixture.slot_id)
        .bind(fixture.revision_id).bind(fixture.intent_id).bind(audit_id)
        .execute(&mut *guarded).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("credential_activation_events_exact_slot_connection_fkey")
    );
    guarded.rollback().await.unwrap();
}

async fn assert_activation_intent_mismatch(
    pool: &PgPool,
    fixture: CandidateFixture,
    other_intent: CandidateFixture,
) {
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events (id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES ($1,$2,$3,'credential.mismatch','credential',$4,'{}'::jsonb,NOW())")
        .bind(audit_id).bind(fixture.workspace_id).bind(fixture.principal_id).bind(fixture.revision_id)
        .execute(pool).await.unwrap();
    let mut guarded = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO credential_activation_events (id,workspace_id,connection_id,credential_slot_id,credential_revision_id,credential_intent_id,event_kind,expected_slot_version,resulting_slot_version,audit_event_id) VALUES ($1,$2,$3,$4,$5,$6,'revoked',0,1,$7)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(fixture.connection_id).bind(fixture.slot_id)
        .bind(fixture.revision_id).bind(other_intent.intent_id).bind(audit_id)
        .execute(&mut *guarded).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("credential_activation_events_exact_intent_fkey")
    );
    guarded.rollback().await.unwrap();
}

async fn assert_rotation_connection_mismatch(pool: &PgPool, fixture: CandidateFixture) {
    let activated_revision_id = Uuid::now_v7();
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events (id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES ($1,$2,$3,'credential.mismatch','credential',$4,'{}'::jsonb,NOW())")
        .bind(audit_id).bind(fixture.workspace_id).bind(fixture.principal_id).bind(fixture.revision_id)
        .execute(pool).await.unwrap();
    let mut guarded = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    sqlx::query("INSERT INTO credential_revisions (id,workspace_id,credential_slot_id,material_key_id,associated_data_profile) VALUES ($1,$2,$3,$4,'credential_v2')")
        .bind(activated_revision_id).bind(fixture.workspace_id).bind(fixture.slot_id).bind(Uuid::now_v7())
        .execute(&mut *guarded).await.unwrap();
    let error = sqlx::query("INSERT INTO credential_rotation_events (id,workspace_id,connection_id,credential_slot_id,previous_credential_revision_id,activated_credential_revision_id,expected_slot_version,resulting_slot_version,audit_event_id) VALUES ($1,$2,$3,$4,$5,$6,1,2,$7)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(Uuid::now_v7()).bind(fixture.slot_id)
        .bind(fixture.revision_id).bind(activated_revision_id).bind(audit_id)
        .execute(&mut *guarded).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("credential_rotation_events_exact_slot_connection_fkey")
    );
    guarded.rollback().await.unwrap();
}

async fn assert_qualification_and_snapshot_branch_mismatches(
    pool: &PgPool,
    base: CandidateFixture,
    sibling: CandidateFixture,
) {
    let provider_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO providers (id,workspace_id,name,locality) VALUES ($1,$2,'identity','remote')",
    )
    .bind(provider_id)
    .bind(base.workspace_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO models (id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES ($1,$2,$3,'identity',4096,0,0)")
        .bind(model_id).bind(provider_id).bind(base.workspace_id).execute(pool).await.unwrap();
    let credential_connection_revision = Uuid::now_v7();
    let no_auth_revision_a = Uuid::now_v7();
    let no_auth_revision_b = Uuid::now_v7();
    let no_auth_binding_a = Uuid::now_v7();
    let no_auth_binding_b = Uuid::now_v7();
    let credential_job = Uuid::now_v7();
    let no_auth_job = Uuid::now_v7();
    let credential_connection_qualification = Uuid::now_v7();
    let no_auth_connection_qualification = Uuid::now_v7();
    let credential_model_revision = Uuid::now_v7();
    let no_auth_model_revision = Uuid::now_v7();
    let credential_model_qualification = Uuid::now_v7();
    let no_auth_model_qualification = Uuid::now_v7();
    let mut setup = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(base.workspace_id.to_string())
        .fetch_one(&mut *setup)
        .await
        .unwrap();
    let execution_guard: Uuid = sqlx::query_scalar(
        "SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(base.workspace_id)
    .bind(base.connection_id)
    .fetch_one(&mut *setup)
    .await
    .unwrap();
    let sibling_activation_guard: Uuid = sqlx::query_scalar("SELECT id FROM credential_activation_guards WHERE workspace_id=$1 AND connection_id=$2 AND credential_slot_id=$3")
        .bind(base.workspace_id).bind(base.connection_id).bind(sibling.slot_id).fetch_one(&mut *setup).await.unwrap();
    sqlx::query("INSERT INTO connection_revisions (id,workspace_id,connection_id,execution_guard_id,kind,logical_base_url,runtime_base_url,adapter_profile_revision,transport_policy,auth_mode,credential_slot_id) VALUES ($1,$2,$3,$4,'open_ai_chat_completions_v1','https://logical.invalid','https://runtime.invalid','profile-v1','remote_https','bearer',$5)")
        .bind(credential_connection_revision).bind(base.workspace_id).bind(base.connection_id).bind(execution_guard).bind(base.slot_id)
        .execute(&mut *setup).await.unwrap();
    for revision in [no_auth_revision_a, no_auth_revision_b] {
        sqlx::query("INSERT INTO connection_revisions (id,workspace_id,connection_id,execution_guard_id,kind,logical_base_url,runtime_base_url,adapter_profile_revision,transport_policy,auth_mode) VALUES ($1,$2,$3,$4,'open_ai_chat_completions_v1','https://logical.invalid','https://runtime.invalid','profile-v1','remote_https','none')")
            .bind(revision).bind(base.workspace_id).bind(base.connection_id).bind(execution_guard).execute(&mut *setup).await.unwrap();
    }
    for (binding, revision) in [
        (no_auth_binding_a, no_auth_revision_a),
        (no_auth_binding_b, no_auth_revision_b),
    ] {
        sqlx::query("INSERT INTO no_auth_binding_revisions (id,workspace_id,connection_id,connection_revision_id) VALUES ($1,$2,$3,$4)")
            .bind(binding).bind(base.workspace_id).bind(base.connection_id).bind(revision).execute(&mut *setup).await.unwrap();
    }
    for (job, revision) in [
        (credential_job, credential_connection_revision),
        (no_auth_job, no_auth_revision_a),
    ] {
        sqlx::query("INSERT INTO qualification_jobs (id,workspace_id,connection_revision_id,profile_revision,state) VALUES ($1,$2,$3,'profile-v1','requested')")
            .bind(job).bind(base.workspace_id).bind(revision).execute(&mut *setup).await.unwrap();
    }
    for (qualification, revision, job) in [
        (
            credential_connection_qualification,
            credential_connection_revision,
            credential_job,
        ),
        (
            no_auth_connection_qualification,
            no_auth_revision_a,
            no_auth_job,
        ),
    ] {
        sqlx::query("INSERT INTO connection_qualification_revisions (id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,valid_until,capabilities) VALUES ($1,$2,$3,$4,'profile-v1',NOW()+INTERVAL '1 hour',ARRAY['chat'])")
            .bind(qualification).bind(base.workspace_id).bind(revision).bind(job).execute(&mut *setup).await.unwrap();
    }
    for (revision, connection_revision) in [
        (credential_model_revision, credential_connection_revision),
        (no_auth_model_revision, no_auth_revision_a),
    ] {
        sqlx::query("INSERT INTO model_revisions (id,workspace_id,model_id,connection_revision_id,wire_model_id,kind) VALUES ($1,$2,$3,$4,'identity','chat')")
            .bind(revision).bind(base.workspace_id).bind(model_id).bind(connection_revision).execute(&mut *setup).await.unwrap();
    }
    for (qualification, model_revision, connection_revision, connection_qualification, job) in [
        (
            credential_model_qualification,
            credential_model_revision,
            credential_connection_revision,
            credential_connection_qualification,
            credential_job,
        ),
        (
            no_auth_model_qualification,
            no_auth_model_revision,
            no_auth_revision_a,
            no_auth_connection_qualification,
            no_auth_job,
        ),
    ] {
        sqlx::query("INSERT INTO model_qualification_revisions (id,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) VALUES ($1,$2,$3,$4,$5,$6,ARRAY['chat'],NOW()+INTERVAL '1 hour')")
            .bind(qualification).bind(base.workspace_id).bind(model_revision).bind(connection_revision).bind(connection_qualification).bind(job)
            .execute(&mut *setup).await.unwrap();
    }
    setup.commit().await.unwrap();

    for (sql, bindings, constraint) in [
        (
            "INSERT INTO qualification_target_bindings (id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,expected_slot_version) VALUES ($1,$2,$3,$4,$5,'credential',$6,$7,$8,0)",
            vec![
                Uuid::now_v7(),
                base.workspace_id,
                credential_job,
                base.connection_id,
                credential_connection_revision,
                sibling.revision_id,
                sibling.slot_id,
                sibling_activation_guard,
            ],
            "qualification_target_bindings_exact_revision_slot_fkey",
        ),
        (
            "INSERT INTO model_binding_snapshots (id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,expected_slot_version) VALUES ($1,$2,$3,$4,$5,$6,$7,'credential',$8,$9,$10,0)",
            vec![
                Uuid::now_v7(),
                base.workspace_id,
                base.connection_id,
                credential_connection_revision,
                credential_connection_qualification,
                credential_model_revision,
                credential_model_qualification,
                sibling.revision_id,
                sibling.slot_id,
                sibling_activation_guard,
            ],
            "model_binding_snapshots_exact_revision_slot_fkey",
        ),
    ] {
        let mut attempt = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *attempt)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(base.workspace_id.to_string())
            .fetch_one(&mut *attempt)
            .await
            .unwrap();
        let mut query = sqlx::query(sql);
        for binding in bindings {
            query = query.bind(binding);
        }
        let error = query.execute(&mut *attempt).await.unwrap_err();
        let database = error.as_database_error().unwrap();
        assert_eq!(database.code().as_deref(), Some("23503"));
        assert_eq!(database.constraint(), Some(constraint));
        attempt.rollback().await.unwrap();
    }
    let mut no_auth_attempt = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *no_auth_attempt)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(base.workspace_id.to_string())
        .fetch_one(&mut *no_auth_attempt)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO model_binding_snapshots (id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES ($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)")
        .bind(Uuid::now_v7()).bind(base.workspace_id).bind(base.connection_id).bind(no_auth_revision_a)
        .bind(no_auth_connection_qualification).bind(no_auth_model_revision).bind(no_auth_model_qualification).bind(no_auth_binding_b)
        .execute(&mut *no_auth_attempt).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("model_binding_snapshots_exact_no_auth_fkey")
    );
    no_auth_attempt.rollback().await.unwrap();

    let mut mode_attempt = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *mode_attempt)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(base.workspace_id.to_string())
        .fetch_one(&mut *mode_attempt)
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO no_auth_binding_revisions (id,workspace_id,connection_id,connection_revision_id) VALUES ($1,$2,$3,$4)")
        .bind(Uuid::now_v7()).bind(base.workspace_id).bind(base.connection_id).bind(credential_connection_revision)
        .execute(&mut *mode_attempt).await.unwrap_err();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23503"));
    assert_eq!(
        database.constraint(),
        Some("no_auth_binding_revisions_exact_mode_fkey")
    );
    mode_attempt.rollback().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn p02_migrations_keep_legacy_tables_owned_by_the_runtime_role_and_function_acl_intent(
    pool: PgPool,
) {
    let pre_p02_migrator = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version < 166)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    pre_p02_migrator.run(&pool).await.unwrap();

    sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE _sqlx_migrations OWNER TO vestrace")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "DO $$
         DECLARE target RECORD;
         BEGIN
             FOR target IN
                 SELECT c.relname
                   FROM pg_class AS c
                   JOIN pg_namespace AS n ON n.oid = c.relnamespace
                  WHERE n.nspname = 'public' AND c.relkind = 'r'
             LOOP
                 EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace', target.relname);
             END LOOP;
         END
         $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(runtime_role_provisioning_bridge_sql())
        .execute(&pool)
        .await
        .unwrap();

    let p02_prerequisite_migrator = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| (166..=167).contains(&migration.version))
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    let runtime = runtime_pool(&pool).await;
    p02_prerequisite_migrator.run(&runtime).await.unwrap();

    let governed_marks_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) \
         FROM pg_class WHERE oid = 'public.governed_mutation_audit_marks'::regclass",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(governed_marks_owner, RUNTIME_ROLE);

    let p02_remaining_migrator = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| (168..=175).contains(&migration.version))
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    p02_remaining_migrator.run(&runtime).await.unwrap();

    let (runtime_owned, guarded_owned, unexpectedly_owned): (i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE pg_get_userbyid(relowner) = 'vestrace'),
             COUNT(*) FILTER (WHERE pg_get_userbyid(relowner) = 'vestrace_guarded_owner'),
             COUNT(*) FILTER (
                 WHERE pg_get_userbyid(relowner) NOT IN ('vestrace', 'vestrace_guarded_owner')
             )
           FROM pg_class AS c
           JOIN pg_namespace AS n ON n.oid = c.relnamespace
          WHERE n.nspname = 'public' AND c.relkind = 'r'",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(runtime_owned, 107);
    assert_eq!(guarded_owned, 26);
    assert_eq!(unexpectedly_owned, 0);

    let may_write_agent_runs: bool = sqlx::query_scalar(
        "SELECT has_table_privilege(current_user, 'public.agent_runs', 'INSERT, UPDATE, DELETE')",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert!(may_write_agent_runs);

    let empty_p02_table_acls: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname \
         FROM pg_class AS c \
         JOIN pg_namespace AS n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND c.relkind = 'r' \
           AND pg_get_userbyid(c.relowner) = 'vestrace_guarded_owner' \
           AND c.relacl = '{}'::aclitem[] \
         ORDER BY c.relname",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert!(
        empty_p02_table_acls.is_empty(),
        "P02 tables must retain the guarded owner's ACL after handoff; empty ACLs: {}",
        empty_p02_table_acls.join(", ")
    );

    let installation_id = Uuid::now_v7();
    let fingerprint_key_id = Uuid::now_v7();
    sqlx::query("SELECT vestrace_record_installation_fingerprint_continuity($1, $2, $3, $4)")
        .bind(installation_id)
        .bind(fingerprint_key_id)
        .bind(1_i32)
        .bind(Vec::<u8>::from([0x5A; 32]))
        .execute(&runtime)
        .await
        .expect("the runtime must successfully write through a guarded SECURITY DEFINER function");
    let fingerprint_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM installation_fingerprint_continuity \
         WHERE installation_id = $1 AND fingerprint_key_id = $2",
    )
    .bind(installation_id)
    .bind(fingerprint_key_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fingerprint_rows, 1);

    let guarded_functions: Vec<String> = sqlx::query_scalar(
        "SELECT p.oid::regprocedure::text \
         FROM pg_proc AS p \
         JOIN pg_namespace AS n ON n.oid = p.pronamespace \
         WHERE n.nspname = 'public' \
           AND pg_get_userbyid(p.proowner) = 'vestrace_guarded_owner' \
           AND p.proname LIKE 'vestrace_%' \
         ORDER BY p.oid::regprocedure::text",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(
        guarded_functions.len(),
        56,
        "the final P02/P03 ownership handoff must retain exactly its 56 distinct functions"
    );
    let mut function_acl_mismatches = Vec::new();
    for regprocedure in guarded_functions {
        let explicitly_granted = has_explicit_runtime_execute_grant(&regprocedure);
        let executable: bool = sqlx::query_scalar(
            "SELECT has_function_privilege(current_user, $1::regprocedure, 'EXECUTE')",
        )
        .bind(&regprocedure)
        .fetch_one(&runtime)
        .await
        .unwrap();
        if executable != explicitly_granted {
            function_acl_mismatches.push(format!(
                "{regprocedure}: EXECUTE={executable}, explicitly granted={explicitly_granted}"
            ));
        }
    }
    assert!(
        function_acl_mismatches.is_empty(),
        "runtime EXECUTE must exactly match the migration's explicit grant intent:\n{}",
        function_acl_mismatches.join("\n")
    );

    let result =
        sqlx::query("SELECT public.vestrace_assign_p02_table_owner('public.agent_runs'::regclass)")
            .execute(&runtime)
            .await;
    assert_insufficient_privilege(result, "handed a pre-existing table to the guarded owner");

    sqlx::query(
        "CREATE FUNCTION public.vestrace_record_guarded_operation_probe(TEXT) \
         RETURNS TEXT LANGUAGE SQL SECURITY DEFINER SET search_path = pg_catalog AS 'SELECT $1'",
    )
    .execute(&runtime)
    .await
    .expect("the production runtime role must retain its declared CREATE privilege");
    let result = sqlx::query(
        "SELECT public.vestrace_assign_p02_function_owner(\
             'public.vestrace_record_guarded_operation_probe(text)'::regprocedure\
         )",
    )
    .execute(&runtime)
    .await;
    assert_insufficient_privilege(
        result,
        "handed an unlisted overload of an allowlisted function name to the guarded owner",
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_owner_bridge_refuses_an_unlisted_overload_of_an_allowlisted_name(pool: PgPool) {
    sqlx::query(
        "CREATE FUNCTION public.vestrace_record_guarded_operation_probe(TEXT) \
         RETURNS TEXT LANGUAGE SQL SECURITY DEFINER SET search_path = pg_catalog AS 'SELECT $1'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "ALTER FUNCTION public.vestrace_record_guarded_operation_probe(TEXT) OWNER TO vestrace",
    )
    .execute(&pool)
    .await
    .unwrap();

    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "SELECT public.vestrace_assign_p02_function_owner(\
             'public.vestrace_record_guarded_operation_probe(text)'::regprocedure\
         )",
    )
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "handed an unlisted overload of an allowlisted function name to the guarded owner",
    );
}

#[test]
fn p02_owner_helper_is_bounded_and_only_p02_migrations_use_it() {
    assert!(RUNTIME_ROLE_PROVISIONING.contains("SECURITY DEFINER"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("SET search_path = pg_catalog"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("OWNER TO vestrace_guarded_owner"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("only declared P02 tables"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("only exact declared P02 function signatures"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("WITH ADMIN FALSE, INHERIT FALSE, SET FALSE"));

    for migration in &P02_MIGRATION_SOURCES[..8] {
        assert!(
            !migration.contains("OWNER TO vestrace_guarded_owner"),
            "P02 ownership must use the bounded bootstrap helper after the final migration"
        );
    }
    assert!(
        P02_MIGRATION_SOURCES[8].contains("sqlx::test creates fresh databases"),
        "the final migration must provide its superuser-only test bootstrap"
    );
    assert!(
        P02_MIGRATION_SOURCES[8].contains("IF NOT (SELECT rolsuper"),
        "a missing bootstrap bridge must remain refused for non-superuser migration executors"
    );
    assert!(
        !RUNTIME_ROLE_PROVISIONING.contains("LIKE 'vestrace_%'"),
        "the production function bridge must use an exact P02 allowlist"
    );
    assert!(
        !RUNTIME_ROLE_PROVISIONING.contains("IF target_name = ANY"),
        "runtime EXECUTE grants must be selected by exact regprocedure identity, not by name"
    );
    assert_eq!(
        regprocedure_array_count(
            RUNTIME_ROLE_PROVISIONING,
            "allowed_targets REGPROCEDURE[] := ARRAY[",
            "runtime_executable_targets REGPROCEDURE[] := ARRAY[",
        ),
        56,
        "the production bridge must declare 56 exact ownership identities"
    );
    assert_eq!(
        regprocedure_array_count(
            RUNTIME_ROLE_PROVISIONING,
            "runtime_executable_targets REGPROCEDURE[] := ARRAY[",
            "BEGIN",
        ),
        37,
        "the production bridge must declare 37 exact runtime-executable identities"
    );
    let fallback_function_bridge = P02_MIGRATION_SOURCES[8]
        .split_once("CREATE FUNCTION public.vestrace_assign_p02_function_owner")
        .expect("the final migration must define the sqlx-test fallback function bridge")
        .1
        .split_once("$function$;")
        .expect("the fallback function bridge must have a bounded body")
        .0;
    assert_eq!(
        regprocedure_array_count(
            fallback_function_bridge,
            "allowed_targets REGPROCEDURE[] := ARRAY[",
            "runtime_executable_targets REGPROCEDURE[] := ARRAY[",
        ),
        56,
        "the sqlx-test fallback bridge must enforce the same 56 ownership identities"
    );
    assert_eq!(
        regprocedure_array_count(
            fallback_function_bridge,
            "runtime_executable_targets REGPROCEDURE[] := ARRAY[",
            "BEGIN",
        ),
        37,
        "the sqlx-test fallback bridge must enforce the same 37 runtime-executable identities"
    );

    assert!(
        P02_MIGRATION_SOURCES[..8]
            .iter()
            .all(|migration| !migration.contains("vestrace_assign_p02_")),
        "ownership must wait until every P02 migration has completed its DDL"
    );
    assert!(
        P02_MIGRATION_SOURCES[..8]
            .iter()
            .all(|migration| !migration.contains("REVOKE ALL ON TABLE")),
        "runtime table ACL revocation must wait until later P02 foreign-key DDL has completed"
    );
    assert!(
        P02_MIGRATION_SOURCES[8].contains("P02 ownership handoff"),
        "the final P02 migration must perform the bounded ownership handoff"
    );
    assert_eq!(
        P02_MIGRATION_SOURCES[8]
            .lines()
            .filter(|line| line.starts_with("SELECT vestrace_assign_p02_table_owner"))
            .count(),
        26,
        "the final handoff must transfer exactly the 26 P02 tables"
    );
    assert_eq!(
        P02_MIGRATION_SOURCES[8]
            .lines()
            .filter(|line| line.starts_with("SELECT vestrace_assign_p02_function_owner"))
            .count(),
        57,
        "the final handoff must preserve all 57 P02 function-ownership assignments"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_is_not_the_owner_of_p02_tables(pool: PgPool) {
    let expected_tables = [
        "p02_guarded_operation_probe",
        "governed_mutation_audit_marks",
        "installation_fingerprint_continuity",
        "installation_mutation_watermark",
        "installation_mutation_watermark_advances",
        "material_key_creation_intents",
        "material_key_creation_intent_erasure_receipts",
        "content_materials",
        "content_material_bytes",
        "prepared_material_attachments",
        "content_material_ordinary_references",
        "connection_execution_guards",
        "credential_activation_guards",
        "credential_slots",
        "credential_revisions",
        "credential_guard_occupancies",
        "credential_key_creation_intents",
        "credential_prepared_materials",
        "credential_prepared_attachments",
        "credential_association_events",
        "credential_lifecycle_events",
        "credential_key_creation_intent_erasure_receipts",
        "material_erasure_preparations",
        "material_erasure_events",
        "material_erasure_audit_tombstones",
        "material_erasure_blockers",
    ];
    let owners = sqlx::query(
        "SELECT c.relname, pg_get_userbyid(c.relowner) AS owner \
         FROM pg_class AS c \
         WHERE c.relname = ANY($1) \
         ORDER BY c.relname",
    )
    .bind(expected_tables)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(owners.len(), expected_tables.len());
    for row in owners {
        let owner: String = row.get("owner");
        assert_eq!(owner, "vestrace_guarded_owner");
        assert_ne!(owner, RUNTIME_ROLE);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_cannot_set_the_guarded_owner(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("SET ROLE vestrace_guarded_owner")
        .execute(&runtime)
        .await;

    assert_insufficient_privilege(result, "assumed the guarded owner with SET ROLE");
}

#[sqlx::test(migrations = "../../migrations")]
async fn guarded_owner_membership_is_pinned_to_the_runtime_role_with_no_options(pool: PgPool) {
    let memberships = sqlx::query(
        "SELECT member_role.rolname, membership.admin_option, membership.inherit_option, membership.set_option \
         FROM pg_auth_members AS membership \
         JOIN pg_roles AS member_role ON member_role.oid = membership.member \
         JOIN pg_roles AS role ON role.oid = membership.roleid \
         WHERE role.rolname = 'vestrace_guarded_owner' \
         ORDER BY member_role.rolname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        memberships.len(),
        1,
        "vestrace must be the guarded owner's only member"
    );
    let membership = &memberships[0];
    assert_eq!(membership.get::<String, _>("rolname"), RUNTIME_ROLE);
    assert!(!membership.get::<bool, _>("admin_option"));
    assert!(!membership.get::<bool, _>("inherit_option"));
    assert!(!membership.get::<bool, _>("set_option"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_has_no_direct_write_privileges_on_any_p02_table(pool: PgPool) {
    let expected_tables = [
        "p02_guarded_operation_probe",
        "governed_mutation_audit_marks",
        "installation_fingerprint_continuity",
        "installation_mutation_watermark",
        "installation_mutation_watermark_advances",
        "material_key_creation_intents",
        "material_key_creation_intent_erasure_receipts",
        "content_materials",
        "content_material_bytes",
        "prepared_material_attachments",
        "content_material_ordinary_references",
        "connection_execution_guards",
        "credential_activation_guards",
        "credential_slots",
        "credential_revisions",
        "credential_guard_occupancies",
        "credential_key_creation_intents",
        "credential_prepared_materials",
        "credential_prepared_attachments",
        "credential_association_events",
        "credential_lifecycle_events",
        "credential_key_creation_intent_erasure_receipts",
        "material_erasure_preparations",
        "material_erasure_events",
        "material_erasure_audit_tombstones",
        "material_erasure_blockers",
    ];
    let privileges: Vec<(String, bool, bool, bool)> = sqlx::query_as(
        "SELECT t, \
                has_table_privilege('vestrace', t, 'INSERT'), \
                has_table_privilege('vestrace', t, 'UPDATE'), \
                has_table_privilege('vestrace', t, 'DELETE') \
         FROM unnest($1::text[]) AS tables(t) \
         ORDER BY t",
    )
    .bind(expected_tables)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(privileges.len(), 26, "every P02 table must be checked");
    for (table, insert, update, delete) in privileges {
        assert!(
            !insert && !update && !delete,
            "the runtime role must have no direct INSERT, UPDATE, or DELETE privilege on {table}; got INSERT={insert}, UPDATE={update}, DELETE={delete}",
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("INSERT INTO p02_guarded_operation_probe (id) VALUES ($1)")
        .bind(Uuid::now_v7())
        .execute(&runtime)
        .await;
    runtime.close().await;

    assert_insufficient_table_privilege(result, "directly inserted a P02 row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_update_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("UPDATE p02_guarded_operation_probe SET created_at = NOW()")
        .execute(&runtime)
        .await;
    runtime.close().await;

    assert_insufficient_table_privilege(result, "directly updated a P02 row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_delete_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("DELETE FROM p02_guarded_operation_probe")
        .execute(&runtime)
        .await;
    runtime.close().await;

    assert_insufficient_table_privilege(result, "directly deleted a P02 row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_fingerprint_continuity_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO installation_fingerprint_continuity \
         (installation_id, fingerprint_key_id, fingerprint_key_version, continuity_proof) \
         VALUES ($1, $2, 1, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(vec![0_u8; 32])
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted installation fingerprint continuity",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_installation_mutation_watermark_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO installation_mutation_watermark (singleton, watermark) VALUES (TRUE, 1)",
    )
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted installation mutation watermark");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_installation_mutation_watermark_advances_is_refused(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO installation_mutation_watermark_advances \
         (governed_mutation_mark_id, watermark, advanced_at) VALUES ($1, 1, NOW())",
    )
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted installation mutation watermark advance",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_key_creation_intents_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_key_creation_intents \
         (id, workspace_id, material_id, material_key_id, nonce, owner_kind, owner_id, output_ordinal, state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'reserved')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("content")
    .bind(Uuid::now_v7())
    .bind(0_i64)
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted a material-key creation intent");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_key_creation_intent_erasure_receipts_is_refused(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_key_creation_intent_erasure_receipts \
         (id, workspace_id, intent_id, erasure_receipt) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a material-key creation intent erasure receipt",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_content_materials_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO content_materials \
         (id, workspace_id, intent_id, material_key_id, state) \
         VALUES ($1, $2, $3, $4, 'prepared')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted a content material");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_content_material_bytes_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO content_material_bytes \
         (id, workspace_id, intent_id, material_id, ciphertext) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(vec![0_u8; 4096])
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted content-material ciphertext");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_prepared_material_attachments_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO prepared_material_attachments \
         (id, workspace_id, intent_id, material_id, marker) \
         VALUES ($1, $2, $3, $4, 'content_prepared')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a nonordinary prepared-material attachment",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_content_material_ordinary_references_is_refused(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO content_material_ordinary_references \
         (id, workspace_id, material_id, intent_id, owner_kind, owner_id, output_ordinal) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("content")
    .bind(Uuid::now_v7())
    .bind(0_i64)
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly published an ordinary content-material reference",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_connection_execution_guards_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO connection_execution_guards (id, workspace_id, connection_id) \
         VALUES ($1, $2, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a permanent connection execution guard",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_activation_guards_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_activation_guards \
         (id, workspace_id, connection_id, credential_slot_id, execution_guard_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a permanent credential activation guard",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_slots_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_slots (id, workspace_id, connection_id, purpose, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("provider")
    .bind("primary")
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted a credential slot");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_revisions_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_revisions \
         (id, workspace_id, credential_slot_id, material_key_id, associated_data_profile) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("credential_v2")
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted immutable credential metadata");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_guard_occupancies_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_guard_occupancies \
         (id, workspace_id, connection_id, credential_slot_id, activation_guard_id, state) \
         VALUES ($1, $2, $3, $4, $5, 'preparing')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a credential activation occupancy",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_key_creation_intents_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_key_creation_intents \
         (id, workspace_id, connection_id, credential_slot_id, occupancy_id, credential_revision_id, material_key_id, nonce, state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'reserved')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted a credential-key creation intent");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_prepared_materials_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_prepared_materials \
         (id, workspace_id, intent_id, credential_revision_id, ciphertext) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(vec![0xA5_u8])
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted CredentialPrepared ciphertext");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_prepared_attachments_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_prepared_attachments \
         (id, workspace_id, intent_id, credential_revision_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted an internal CredentialPrepared attachment",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_association_events_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_association_events \
         (id, workspace_id, occupancy_id, intent_id, event_kind, expected_version, resulting_version) \
         VALUES ($1, $2, $3, $4, 'credential_association_cancelled', 1, 2)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly appended a cancellation association event");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_lifecycle_events_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_lifecycle_events \
         (id, workspace_id, intent_id, credential_revision_id, event_kind) \
         VALUES ($1, $2, $3, $4, 'candidate')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly appended a Candidate lifecycle event");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_credential_intent_erasure_receipts_is_refused(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_key_creation_intent_erasure_receipts \
         (id, workspace_id, intent_id, erasure_receipt) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(
        result,
        "directly inserted a witnessed credential intent erasure receipt",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_erasure_preparations_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_erasure_preparations \
         (id, workspace_id, target_kind, content_material_id, material_key_id) \
         VALUES ($1, $2, 'content', $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly inserted a material erasure preparation");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_erasure_events_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind) \
         VALUES ($1, $2, $3, 'erasure_prepared')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly appended a material erasure event");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_erasure_audit_tombstones_is_refused(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_erasure_audit_tombstones \
         (id, workspace_id, preparation_id, target_kind, target_id, erasure_receipt) \
         VALUES ($1, $2, $3, 'content', $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly wrote a material erasure audit tombstone");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_insert_into_material_erasure_blockers_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_erasure_blockers \
         (id, workspace_id, target_kind, content_material_id, blocker_kind, state) \
         VALUES ($1, $2, 'content', $3, 'effect', 'nonterminal')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "directly wrote a material erasure blocker");
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_may_execute_guarded_functions(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    sqlx::query("SELECT vestrace_record_guarded_operation_probe($1)")
        .bind(Uuid::now_v7())
        .execute(&runtime)
        .await
        .expect("the runtime role must reach the guarded probe function");

    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let audit_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id)
        .bind(format!("runtime-guard-{}", workspace_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("runtime-principal-{}", principal_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO audit_events \
         (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
    )
    .bind(audit_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind("runtime.guarded_function")
    .bind("installation")
    .bind(Uuid::now_v7())
    .bind(serde_json::json!({}))
    .execute(&pool)
    .await
    .unwrap();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let slot_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("runtime-credential-connector-{connector_id}"))
    .bind("openai")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(format!("runtime-credential-connection-{connection_id}"))
    .execute(&pool)
    .await
    .unwrap();

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.principal_id")
        .bind(principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_record_governed_mutation_audit_mark_and_advance($1, $2, $3, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(audit_id)
    .fetch_one(&mut *transaction)
    .await
    .expect("the runtime role must reach the guarded watermark function");
    let material_intent_id = Uuid::now_v7();
    sqlx::query(
        "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(material_intent_id)
    .bind(workspace_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("content")
    .bind(principal_id)
    .bind(0_i64)
    .execute(&mut *transaction)
    .await
    .expect("the runtime role must reach the guarded material-intent reservation function");
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must reach the guarded execution-guard function");
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
        .bind(slot_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind("provider")
        .bind("primary")
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must reach the guarded credential-slot function");
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must reach the guarded activation-guard function");
    let occupancy_id = Uuid::now_v7();
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
        .bind(occupancy_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must reach the guarded occupancy function");
    let credential_intent_id = Uuid::now_v7();
    let credential_revision_id = Uuid::now_v7();
    let credential_material_key_id = Uuid::now_v7();
    let credential_nonce = Uuid::now_v7();
    sqlx::query(
        "SELECT vestrace_reserve_credential_key_creation_intent(\
         $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
    )
    .bind(credential_intent_id)
    .bind(workspace_id)
    .bind(connection_id)
    .bind(slot_id)
    .bind(occupancy_id)
    .bind(credential_revision_id)
    .bind(credential_material_key_id)
    .bind(credential_nonce)
    .execute(&mut *transaction)
    .await
    .expect("the runtime role must reach the guarded credential-intent reservation function");
    sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
        .bind(credential_intent_id)
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must record provisional credential-key creation");
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
        .bind(credential_intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must record the provisional credential-key receipt");
    sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
        .bind(credential_intent_id)
        .bind(Uuid::now_v7())
        .bind(Vec::<u8>::from([0xC3; 32]))
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must create prepared credential material");
    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
        .bind(credential_intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must bind prepared credential material");
    sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
        .bind(credential_intent_id)
        .execute(&mut *transaction)
        .await
        .expect("the runtime role must finalize the exact Candidate");
    let candidate_preparation_id: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(credential_intent_id)
    .bind(2_i64)
    .fetch_one(&mut *transaction)
    .await
    .expect("the runtime role must reach the narrow P03 Candidate-abandon entrypoint");
    let replay_preparation_id: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(credential_intent_id)
    .bind(2_i64)
    .fetch_one(&mut *transaction)
    .await
    .expect(
        "exact Candidate recovery replays the original association version          and must return the original preparation",
    );
    assert_eq!(replay_preparation_id, candidate_preparation_id);
    transaction
        .commit()
        .await
        .expect("the guarded watermark operation must commit");
    let guarded_write_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM material_key_creation_intents WHERE id = $1")
            .bind(material_intent_id)
            .fetch_one(&pool)
            .await
            .expect("the guarded write must remain visible after commit");
    assert_eq!(guarded_write_count, 1);
    let p03_guarded_write: (String, String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT i.state, o.state, o.association_version, COUNT(p.id), \
                COUNT(a.id), COUNT(active.id) \
           FROM credential_key_creation_intents AS i \
           JOIN credential_guard_occupancies AS o ON o.id = i.occupancy_id \
           JOIN material_erasure_preparations AS p ON p.credential_intent_id = i.id \
           LEFT JOIN credential_association_events AS a \
             ON a.intent_id = i.id \
            AND a.event_kind = 'credential_association_cancelled' \
            AND a.expected_version = 2 AND a.resulting_version = 3 \
           LEFT JOIN credential_activation_events AS active \
             ON active.credential_revision_id = i.credential_revision_id \
            AND active.event_kind = 'active' \
          WHERE i.id = $1 \
          GROUP BY i.state, o.state, o.association_version",
    )
    .bind(credential_intent_id)
    .fetch_one(&pool)
    .await
    .expect("the P03 Candidate-abandon write must remain visible after commit");
    assert_eq!(
        p03_guarded_write,
        (
            "erasure_prepared".to_owned(),
            "candidate".to_owned(),
            3,
            1,
            1,
            0,
        ),
        "Candidate abandon must close the association by event/version while retaining occupancy until witnessed destruction"
    );
    let mut wrong_version_transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *wrong_version_transaction)
        .await
        .unwrap();
    let wrong_version_replay = sqlx::query(
        "SELECT preparation_id FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(credential_intent_id)
    .bind(3_i64)
    .execute(&mut *wrong_version_transaction)
    .await;
    assert_check_violation(
        wrong_version_replay,
        "replayed Candidate abandonment with the resulting association version          rather than the original one it was prepared with",
    );
    wrong_version_transaction.rollback().await.unwrap();
    let mut cross_transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *cross_transaction)
        .await
        .unwrap();
    let cross_call = sqlx::query(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(credential_intent_id)
    .execute(&mut *cross_transaction)
    .await;
    assert_check_violation(
        cross_call,
        "cross-called ordinary erasure on Candidate abandonment",
    );
    cross_transaction.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_cannot_execute_internal_provider_result_live_guard(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("SELECT vestrace_validate_provider_result_material_live()")
        .execute(&runtime)
        .await;
    assert_insufficient_privilege(
        result,
        "executed the internal provider-result Live constraint function",
    );
    let installer = sqlx::query("SELECT vestrace_install_provider_result_live_trigger()")
        .execute(&runtime)
        .await;
    runtime.close().await;
    assert_insufficient_privilege(
        installer,
        "executed the closed provider-result Live trigger installer",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_cannot_execute_result_material_preparation(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let can_execute: bool = sqlx::query_scalar(
        "SELECT has_function_privilege( \
         current_user, \
         'public.vestrace_prepare_result_material(uuid, uuid, bytea, bigint)'::regprocedure, \
         'EXECUTE' \
         )",
    )
    .fetch_one(&runtime)
    .await
    .expect("the runtime function privilege must be observable from the live catalog");
    let result = sqlx::query("SELECT public.vestrace_prepare_result_material($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Vec::<u8>::from([0xA5; 32]))
        .bind(1_i64)
        .execute(&runtime)
        .await;
    runtime.close().await;

    assert!(
        !can_execute,
        "the restricted runtime role must not execute ResultPrepared material creation"
    );
    assert_insufficient_privilege(result, "executed ResultPrepared material creation");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_role_cannot_log_in(pool: PgPool) {
    let role = sqlx::query(
        "SELECT rolcanlogin, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = 'vestrace_guarded_owner'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(!role.get::<bool, _>("rolcanlogin"));
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_direct_dml_on_every_p03_table_is_exact_42501(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for table in P03_GUARDED_TABLES {
        for (verb, statement) in [
            (
                "INSERT",
                format!("INSERT INTO public.{table} DEFAULT VALUES"),
            ),
            (
                "UPDATE",
                format!("UPDATE public.{table} SET workspace_id = workspace_id"),
            ),
            ("DELETE", format!("DELETE FROM public.{table}")),
        ] {
            let result = sqlx::query(&statement).execute(&runtime).await;
            assert_insufficient_table_privilege(
                result,
                &format!("directly executed {verb} on P03 table {table}"),
            );
        }
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_has_select_only_on_deployment_policy_evidence(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for (verb, statement) in [
        (
            "INSERT",
            "INSERT INTO public.model_data_policy_decisions(
                id,run_id,step_id,destination,classification,verdict,reason,policy_version,mode,decided_at
             ) VALUES(gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),
                'remote_provider','confidential','allowed','direct','v1','enforce',NOW())",
        ),
        (
            "UPDATE",
            "UPDATE public.model_data_policy_decisions SET reason=reason",
        ),
        (
            "DELETE",
            "DELETE FROM public.model_data_policy_decisions",
        ),
    ] {
        assert_insufficient_table_privilege(
            sqlx::query(statement).execute(&runtime).await,
            &format!("directly executed {verb} on deployment policy evidence"),
        );
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_reconstruction_lock_is_executable_and_effect_intents_are_immutable(pool: PgPool) {
    let workspace = Uuid::now_v7();
    let effect = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace)
        .bind(format!("reconstruction-lock-{workspace}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'provider','{}'::jsonb)",
    )
    .bind(effect)
    .bind(workspace)
    .execute(&pool)
    .await
    .unwrap();

    let runtime = runtime_pool(&pool).await;
    let mut missing = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *missing)
        .await
        .unwrap();
    assert_check_violation(
        sqlx::query("SELECT vestrace_lock_model_request_evidence_for_reconstruction($1,$2)")
            .bind(workspace)
            .bind(Uuid::now_v7())
            .execute(&mut *missing)
            .await,
        "locked an absent model request evidence root",
    );
    missing.rollback().await.unwrap();

    for (verb, statement) in [
        (
            "UPDATE",
            "UPDATE external_effect_intents SET id=id WHERE id=$1",
        ),
        ("DELETE", "DELETE FROM external_effect_intents WHERE id=$1"),
    ] {
        let mut mutation = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace.to_string())
            .fetch_one(&mut *mutation)
            .await
            .unwrap();
        assert_check_violation(
            sqlx::query(statement)
                .bind(effect)
                .execute(&mut *mutation)
                .await,
            &format!("{verb}d an immutable external effect intent"),
        );
        mutation.rollback().await.unwrap();
    }
    let unchanged: (Uuid, Uuid, String) =
        sqlx::query_as("SELECT id,workspace_id,adapter FROM external_effect_intents WHERE id=$1")
            .bind(effect)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, (effect, workspace, "provider".into()));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_governed_mutation_dependencies_have_exact_writes_and_forced_workspace_rls(
    pool: PgPool,
) {
    let workspace_a = Uuid::now_v7();
    let workspace_b = Uuid::now_v7();
    let principal_a = Uuid::now_v7();
    let principal_b = Uuid::now_v7();
    let effect_a = Uuid::now_v7();
    let effect_b = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2),($3,$4)")
        .bind(workspace_a)
        .bind(format!("mutation-a-{workspace_a}"))
        .bind(workspace_b)
        .bind(format!("mutation-b-{workspace_b}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO principals(id,workspace_id,identifier)
         VALUES($1,$2,$3),($4,$5,$6)",
    )
    .bind(principal_a)
    .bind(workspace_a)
    .bind(format!("mutation-a-{principal_a}"))
    .bind(principal_b)
    .bind(workspace_b)
    .bind(format!("mutation-b-{principal_b}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'provider','{}'::jsonb),($3,$4,'provider','{}'::jsonb)",
    )
    .bind(effect_a)
    .bind(workspace_a)
    .bind(effect_b)
    .bind(workspace_b)
    .execute(&pool)
    .await
    .unwrap();
    let audit_b = Uuid::now_v7();
    let outbox_b = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload)
         VALUES($1,$2,$3,'provider.dispatch','external_effect',$4,'{}'::jsonb)",
    )
    .bind(audit_b)
    .bind(workspace_b)
    .bind(principal_b)
    .bind(effect_b)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO idempotency_keys(
            idempotency_key,workspace_id,request_hash,response_payload,status,expires_at
         ) VALUES('workspace-b',$1,'hash-b','{}'::jsonb,'completed',NOW()+INTERVAL '1 day')",
    )
    .bind(workspace_b)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO outbox(id,workspace_id,topic,payload) VALUES($1,$2,'provider.dispatch','{}'::jsonb)",
    )
    .bind(outbox_b)
    .bind(workspace_b)
    .execute(&pool)
    .await
    .unwrap();

    let runtime = runtime_pool(&pool).await;
    let mut active = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_a.to_string())
        .fetch_one(&mut *active)
        .await
        .unwrap();
    let authorization_a = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO external_effect_authorizations(
            id,effect_id,workspace_id,policy_version,subject_id,capability,operation,
            resource_scope,result,reason,input_state,decided_at,payload
         ) VALUES($1,$2,$3,'v1',$4,'model.use','dispatch','provider','allow',
                  'configured_allowance','{}'::jsonb,NOW(),'{}'::jsonb)",
    )
    .bind(authorization_a)
    .bind(effect_a)
    .bind(workspace_a)
    .bind(Uuid::now_v7())
    .execute(&mut *active)
    .await
    .unwrap();
    let audit_a = Uuid::now_v7();
    let outbox_a = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload)
         VALUES($1,$2,$3,'provider.dispatch','external_effect',$4,'{}'::jsonb)",
    )
    .bind(audit_a)
    .bind(workspace_a)
    .bind(principal_a)
    .bind(effect_a)
    .execute(&mut *active)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO idempotency_keys(
            idempotency_key,workspace_id,request_hash,response_payload,status,expires_at
         ) VALUES('workspace-a',$1,'hash-a','{}'::jsonb,'completed',NOW()+INTERVAL '1 day')",
    )
    .bind(workspace_a)
    .execute(&mut *active)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO outbox(id,workspace_id,topic,payload) VALUES($1,$2,'provider.dispatch','{}'::jsonb)",
    )
    .bind(outbox_a)
    .bind(workspace_a)
    .execute(&mut *active)
    .await
    .unwrap();
    let updated = sqlx::query("UPDATE outbox SET processed_at=NOW() WHERE id=$1")
        .bind(outbox_a)
        .execute(&mut *active)
        .await
        .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    let visible_active: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM external_effect_authorizations ORDER BY id")
            .fetch_all(&mut *active)
            .await
            .unwrap();
    assert_eq!(visible_active, vec![authorization_a]);
    let visible_dependencies: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM audit_events),
                (SELECT COUNT(*) FROM idempotency_keys),
                (SELECT COUNT(*) FROM outbox)",
    )
    .fetch_one(&mut *active)
    .await
    .unwrap();
    assert_eq!(visible_dependencies, (1, 1, 1));
    active.commit().await.unwrap();

    let mut cross_workspace = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_a.to_string())
        .fetch_one(&mut *cross_workspace)
        .await
        .unwrap();
    let hidden: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_authorizations WHERE workspace_id=$1",
    )
    .bind(workspace_b)
    .fetch_one(&mut *cross_workspace)
    .await
    .unwrap();
    assert_eq!(
        hidden, 0,
        "cross-workspace authorization rows must be invisible"
    );
    let hidden_dependencies: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1),
                (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id=$1),
                (SELECT COUNT(*) FROM outbox WHERE workspace_id=$1)",
    )
    .bind(workspace_b)
    .fetch_one(&mut *cross_workspace)
    .await
    .unwrap();
    assert_eq!(hidden_dependencies, (0, 0, 0));
    let cross_insert = sqlx::query(
        "INSERT INTO external_effect_authorizations(
            id,effect_id,workspace_id,policy_version,subject_id,capability,operation,
            resource_scope,result,reason,input_state,decided_at,payload
         ) VALUES($1,$2,$3,'v1',$4,'model.use','dispatch','provider','allow',
                  'configured_allowance','{}'::jsonb,NOW(),'{}'::jsonb)",
    )
    .bind(Uuid::now_v7())
    .bind(effect_b)
    .bind(workspace_b)
    .bind(Uuid::now_v7())
    .execute(&mut *cross_workspace)
    .await
    .expect_err("forced RLS must refuse a cross-workspace authorization insert");
    assert_eq!(
        cross_insert
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );
    cross_workspace.rollback().await.unwrap();

    for (operation, statement) in [
        (
            "inserted a cross-workspace audit event",
            "INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload)
             VALUES($1,$2,$3,'provider.dispatch','external_effect',$4,'{}'::jsonb)",
        ),
        (
            "inserted a cross-workspace idempotency key",
            "INSERT INTO idempotency_keys(
                idempotency_key,workspace_id,request_hash,response_payload,status,expires_at
             ) VALUES(($1::uuid)::text,$2,'hash-cross','{}'::jsonb,'completed',NOW()+INTERVAL '1 day')",
        ),
        (
            "inserted a cross-workspace outbox row",
            "INSERT INTO outbox(id,workspace_id,topic,payload)
             VALUES($1,$2,'provider.dispatch','{}'::jsonb)",
        ),
    ] {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_a.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let mut query = sqlx::query(statement).bind(Uuid::now_v7()).bind(workspace_b);
        if operation.contains("audit") {
            query = query.bind(principal_b).bind(effect_b);
        }
        let result = query.execute(&mut *transaction).await;
        assert_insufficient_privilege(result, operation);
        transaction.rollback().await.unwrap();
    }

    let mut cross_update = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_a.to_string())
        .fetch_one(&mut *cross_update)
        .await
        .unwrap();
    let update = sqlx::query("UPDATE outbox SET processed_at=NOW() WHERE id=$1")
        .bind(outbox_b)
        .execute(&mut *cross_update)
        .await
        .unwrap();
    assert_eq!(
        update.rows_affected(),
        0,
        "cross-workspace outbox UPDATE must be inert"
    );
    cross_update.commit().await.unwrap();

    let persisted: (i64, i64, i64, i64, bool) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM external_effect_authorizations WHERE id=$1),
                (SELECT COUNT(*) FROM audit_events WHERE id=$2),
                (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id=$3 AND idempotency_key='workspace-a'),
                (SELECT COUNT(*) FROM outbox WHERE id=$4),
                (SELECT processed_at IS NULL FROM outbox WHERE id=$5)",
    )
    .bind(authorization_a)
    .bind(audit_a)
    .bind(workspace_a)
    .bind(outbox_a)
    .bind(outbox_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (1, 1, 1, 1, true));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn superseded_p02_candidate_erasure_is_runtime_exact_42501(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query("SELECT public.vestrace_prepare_credential_material_erasure($1)")
        .bind(Uuid::now_v7())
        .execute(&runtime)
        .await;
    runtime.close().await;

    assert_insufficient_privilege(result, "executed superseded P02 Candidate-only erasure");
}

#[sqlx::test(migrations = "../../migrations")]
async fn retired_revoked_active_erasure_and_independent_recovery_are_exact(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let retired = create_candidate_fixture(&pool, &runtime, "retired").await;
    assert_rotation_connection_mismatch(&pool, retired).await;
    append_rotation_fixture(&pool, retired).await;
    let mut retired_transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(retired.workspace_id.to_string())
        .fetch_one(&mut *retired_transaction)
        .await
        .unwrap();
    let retired_preparation: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(retired.intent_id)
    .fetch_one(&mut *retired_transaction)
    .await
    .unwrap();
    let same_transaction_replay: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(retired.intent_id)
    .fetch_one(&mut *retired_transaction)
    .await
    .unwrap();
    assert_eq!(same_transaction_replay, retired_preparation);
    retired_transaction.commit().await.unwrap();
    runtime.close().await;

    let recovered_runtime = runtime_pool(&pool).await;
    let mut recovery = recovered_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(retired.workspace_id.to_string())
        .fetch_one(&mut *recovery)
        .await
        .unwrap();
    let recovered_preparation: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(retired.intent_id)
    .fetch_one(&mut *recovery)
    .await
    .unwrap();
    assert_eq!(recovered_preparation, retired_preparation);
    recovery.commit().await.unwrap();
    let mut ordinary_to_candidate = recovered_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(retired.workspace_id.to_string())
        .fetch_one(&mut *ordinary_to_candidate)
        .await
        .unwrap();
    let cross_call = sqlx::query(
        "SELECT preparation_id FROM vestrace_prepare_candidate_abandon_and_erasure($1,2)",
    )
    .bind(retired.intent_id)
    .execute(&mut *ordinary_to_candidate)
    .await;
    assert_exact_check_violation(
        cross_call,
        "Candidate erasure replay requires its exact original cancellation evidence and preparation tuple",
        "cross-called Candidate abandonment after ordinary Retired preparation",
    );
    ordinary_to_candidate.rollback().await.unwrap();

    let revoked = create_candidate_fixture(&pool, &recovered_runtime, "revoked").await;
    let revoked_sibling =
        create_sibling_candidate_fixture(&recovered_runtime, revoked, "revoked-sibling").await;
    assert_qualification_and_snapshot_branch_mismatches(&pool, revoked, revoked_sibling).await;
    assert_activation_connection_mismatch(&pool, revoked).await;
    assert_activation_intent_mismatch(&pool, revoked, revoked_sibling).await;
    append_activation_fixture(&pool, revoked, "revoked").await;
    let mut revoked_transaction = recovered_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(revoked.workspace_id.to_string())
        .fetch_one(&mut *revoked_transaction)
        .await
        .unwrap();
    let revoked_preparation: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(revoked.intent_id)
    .fetch_one(&mut *revoked_transaction)
    .await
    .unwrap();
    assert_ne!(revoked_preparation, retired_preparation);
    revoked_transaction.commit().await.unwrap();

    let active = create_candidate_fixture(&pool, &recovered_runtime, "active").await;
    append_activation_fixture(&pool, active, "active").await;
    let mut active_transaction = recovered_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(active.workspace_id.to_string())
        .fetch_one(&mut *active_transaction)
        .await
        .unwrap();
    let ordinary_active = sqlx::query(
        "SELECT preparation_id FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)",
    )
    .bind(active.intent_id)
    .execute(&mut *active_transaction)
    .await;
    assert_exact_check_violation(
        ordinary_active,
        "only exact non-current Retired or Revoked credential material may prepare erasure",
        "prepared Active credential erasure through the ordinary entrypoint",
    );
    active_transaction.rollback().await.unwrap();
    let mut active_candidate = recovered_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(active.workspace_id.to_string())
        .fetch_one(&mut *active_candidate)
        .await
        .unwrap();
    let candidate_active = sqlx::query(
        "SELECT preparation_id FROM vestrace_prepare_candidate_abandon_and_erasure($1,2)",
    )
    .bind(active.intent_id)
    .execute(&mut *active_candidate)
    .await;
    assert_exact_check_violation(
        candidate_active,
        "Candidate abandon requires the exact non-current Candidate association version",
        "prepared Active credential erasure through Candidate abandonment",
    );
    active_candidate.rollback().await.unwrap();
    recovered_runtime.close().await;
}

#[test]
fn p03_owner_helpers_are_exact_and_do_not_broaden_p02_allowlists() {
    assert!(RUNTIME_ROLE_PROVISIONING.contains(
        "CREATE OR REPLACE FUNCTION public.vestrace_assign_p03_table_owner(target REGCLASS)"
    ));
    assert!(RUNTIME_ROLE_PROVISIONING.contains(
        "CREATE OR REPLACE FUNCTION public.vestrace_assign_p03_function_owner(target REGPROCEDURE)"
    ));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("only declared P03 tables"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains("only exact declared P03 function signatures"));
    assert!(RUNTIME_ROLE_PROVISIONING.contains(
        "CREATE OR REPLACE FUNCTION public.vestrace_disable_superseded_p02_credential_erasure()"
    ));
    assert!(RUNTIME_ROLE_PROVISIONING.contains(
        "CREATE OR REPLACE FUNCTION public.vestrace_install_provider_result_live_trigger()"
    ));
    let task10_helper = RUNTIME_ROLE_PROVISIONING
        .split_once("CREATE OR REPLACE FUNCTION public.vestrace_prepare_task10_p03_upgrade()")
        .expect("Task 10 helper declaration")
        .1
        .split_once("CREATE OR REPLACE FUNCTION public.vestrace_grant_p03_dependency_references()")
        .expect("Task 10 helper terminator")
        .0;
    assert!(
        !task10_helper.contains("'material_key_creation_intents'"),
        "Task 10 hand-back must contain only P03-owned relations"
    );
    assert!(
        !RUNTIME_ROLE_PROVISIONING
            .contains("vestrace_assign_p03_table_owner(target REGCLASS, owner")
    );
    assert!(
        !RUNTIME_ROLE_PROVISIONING
            .contains("vestrace_assign_p03_function_owner(target REGPROCEDURE, owner")
    );

    assert_eq!(
        P02_MIGRATION_SOURCES[8]
            .lines()
            .filter(|line| line.starts_with("SELECT vestrace_assign_p02_table_owner"))
            .count(),
        26,
        "the P02 table allowlist/handoff must remain unchanged"
    );
    assert_eq!(
        P02_MIGRATION_SOURCES[8]
            .lines()
            .filter(|line| line.starts_with("SELECT vestrace_assign_p02_function_owner"))
            .count(),
        57,
        "the P02 function allowlist/handoff must remain unchanged"
    );
}
