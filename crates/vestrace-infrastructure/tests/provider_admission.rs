use std::{str::FromStr, sync::Arc};

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ConnectionRevisionRepository, CreateConnectionRevision, PublishConnectionAdmissionPolicy,
    RequestContext,
};
use vestrace_domain::{
    AuditEvent, ConnectionAdmissionPolicyId, ConnectionAuthMode, ConnectionId, ConnectionKind,
    ConnectionRevisionId, ConnectionTransportPolicy, ConnectorId, PrincipalId, WorkspaceId,
    connection::{Connection, ConnectionStatus},
    id::AuditEventId,
    models::ConnectionAdmissionLimits,
    time::now,
};
use vestrace_infrastructure::{PgConnectionRevisionRepository, PgStore};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(2)
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

fn assert_sqlstate<T>(result: Result<T, sqlx::Error>, expected: &str, operation: &str) {
    let error = match result {
        Ok(_) => panic!("{operation} unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some(expected),
        "{operation}: {error}"
    );
}

#[derive(Clone, Copy)]
struct QualificationFixture {
    workspace_id: Uuid,
    connection_id: Uuid,
    connection_revision_id: Uuid,
    qualification_job_id: Uuid,
    qualification_target_id: Uuid,
    shape_id: Uuid,
    limits_id: Uuid,
}

#[derive(Clone, Copy)]
struct DispatchEvidence {
    effect_id: Uuid,
    evidence_id: Uuid,
    check_id: Uuid,
}

#[derive(Clone, Copy)]
struct AdmissionAttemptIds {
    admission_id: Uuid,
    wait_id: Uuid,
    lease_id: Uuid,
}

async fn qualification_fixture(pool: &PgPool, runtime: &PgPool) -> QualificationFixture {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let qualification_target_id = Uuid::now_v7();
    let policy_id = Uuid::now_v7();
    let shape_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id)
        .bind(format!("task10-admission-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("task10-principal-{principal_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("task10-connector-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,$5,'active')")
        .bind(connection_id)
        .bind(connector_id)
        .bind(workspace_id)
        .bind(principal_id)
        .bind(format!("task10-connection-{connection_id}"))
        .execute(pool)
        .await
        .unwrap();

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
          $1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1',\
          'http://127.0.0.1:1234/v1','lm-studio-local/v1','loopback_only','none',NULL,0)",
    )
    .bind(connection_revision_id)
    .bind(workspace_id)
    .bind(connection_id)
    .bind(guard_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(\
          $1,$2,1,'models_list',false,ARRAY[]::TEXT[])",
    )
    .bind(shape_id)
    .bind(workspace_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,1,1,1)")
        .bind(limits_id)
        .bind(workspace_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

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
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) VALUES($1,$2,$3,'q1','running')")
        .bind(qualification_job_id)
        .bind(workspace_id)
        .bind(connection_revision_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6)")
        .bind(qualification_target_id)
        .bind(workspace_id)
        .bind(qualification_job_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(no_auth_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();

    // The policy is published the way a deployment publishes it: the guarded
    // compare-and-swap function, called by the runtime role, against a head
    // that does not exist yet. Until P04 added that function this fixture
    // inserted both rows itself as the guarded owner, which no deployment can
    // do -- so every admission test below was proving admission against a
    // policy no production path could have written.
    let mut publish = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *publish)
        .await
        .unwrap();
    let published_version = sqlx::query_scalar::<_, i64>(
        // The casts are load-bearing: max_in_flight is SMALLINT and the head
        // version is BIGINT, and PostgreSQL will not narrow an INTEGER literal
        // to SMALLINT while resolving an overload, so bare literals fail with
        // 42883 rather than calling this function.
        "SELECT vestrace_publish_connection_admission_policy(           $1,$2,$3,0::BIGINT,1::SMALLINT,60000,30,900)",
    )
    .bind(policy_id)
    .bind(workspace_id)
    .bind(connection_id)
    .fetch_one(&mut *publish)
    .await
    .unwrap();
    assert_eq!(published_version, 1, "the first policy is version one");
    publish.commit().await.unwrap();

    QualificationFixture {
        workspace_id,
        connection_id,
        connection_revision_id,
        qualification_job_id,
        qualification_target_id,
        shape_id,
        limits_id,
    }
}

async fn add_dispatch_evidence(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: QualificationFixture,
) -> DispatchEvidence {
    let effect_id = Uuid::now_v7();
    let evidence_id = Uuid::now_v7();
    let check_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'openai-compatible','{}'::jsonb)")
        .bind(effect_id)
        .bind(fixture.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    let kinds = vec![
        "external_effect",
        "connection_revision",
        "qualification_target",
        "qualification_probe",
        "request_shape_revision",
        "limits_revision",
    ];
    let ids = vec![
        effect_id,
        fixture.connection_revision_id,
        fixture.qualification_target_id,
        fixture.qualification_job_id,
        fixture.shape_id,
        fixture.limits_id,
    ];
    let versions = vec![None, None, None, None, Some(1_i64), Some(1_i64)];
    let ordinals = vec![None, None, None, Some("00"), None, None];
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(\
          $1,$2,$3,'models_list',NULL,$4,'qualification_probe',$5,$6,$7,$8,$9)",
    )
    .bind(evidence_id)
    .bind(fixture.workspace_id)
    .bind(effect_id)
    .bind(fixture.qualification_target_id)
    .bind(fixture.qualification_job_id)
    .bind(&kinds)
    .bind(&ids)
    .bind(&versions)
    .bind(&ordinals)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(\
          $1,$2,$3,'complete',ARRAY[]::TEXT[],ARRAY[]::UUID[],ARRAY[]::UUID[],ARRAY[]::UUID[])",
    )
    .bind(check_id)
    .bind(fixture.workspace_id)
    .bind(evidence_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    DispatchEvidence {
        effect_id,
        evidence_id,
        check_id,
    }
}

async fn admit(
    runtime: &PgPool,
    fixture: QualificationFixture,
    evidence: DispatchEvidence,
    admission_id: Uuid,
    wait_id: Uuid,
    lease_id: Uuid,
) -> sqlx::postgres::PgRow {
    try_admit_with_ttl(
        runtime,
        fixture,
        evidence,
        admission_id,
        wait_id,
        lease_id,
        60,
    )
    .await
    .unwrap()
}

async fn try_admit_with_ttl(
    runtime: &PgPool,
    fixture: QualificationFixture,
    evidence: DispatchEvidence,
    admission_id: Uuid,
    wait_id: Uuid,
    lease_id: Uuid,
    ttl_seconds: i32,
) -> Result<sqlx::postgres::PgRow, sqlx::Error> {
    try_admit_with_nullable_replay(
        runtime,
        fixture,
        evidence,
        AdmissionAttemptIds {
            admission_id,
            wait_id,
            lease_id,
        },
        Some("qualification_probe"),
        Some(ttl_seconds),
    )
    .await
}

async fn try_admit_with_nullable_replay(
    runtime: &PgPool,
    fixture: QualificationFixture,
    evidence: DispatchEvidence,
    attempt: AdmissionAttemptIds,
    cause_kind: Option<&str>,
    ttl_seconds: Option<i32>,
) -> Result<sqlx::postgres::PgRow, sqlx::Error> {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let row = sqlx::query(
        "SELECT * FROM vestrace_try_admit_provider_dispatch(\
          $1,$2,$3,$4,$5,$6,$7,$8,$9::TEXT,\
          NULL,NULL,NULL,$10,$11,'00',$12)",
    )
    .bind(attempt.admission_id)
    .bind(attempt.wait_id)
    .bind(attempt.lease_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(evidence.effect_id)
    .bind(evidence.evidence_id)
    .bind(cause_kind)
    .bind(fixture.qualification_job_id)
    .bind(fixture.qualification_target_id)
    .bind(ttl_seconds)
    .fetch_one(&mut *transaction)
    .await;
    match row {
        Ok(row) => {
            transaction.commit().await?;
            Ok(row)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn admission_rejects_caller_time_shifting_and_writes_nothing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let workspace_id = Uuid::now_v7();
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let evidence_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let qualification_target_id = Uuid::now_v7();

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let result = sqlx::query(
        "SELECT * FROM vestrace_try_admit_provider_dispatch(\
             $1,$2,$3,$4,$5,$6,$7,$8,'qualification_probe',\
             NULL,NULL,NULL,$9,$10,'00',0)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(lease_id)
    .bind(workspace_id)
    .bind(connection_id)
    .bind(connection_revision_id)
    .bind(effect_id)
    .bind(evidence_id)
    .bind(qualification_job_id)
    .bind(qualification_target_id)
    .fetch_all(&mut *transaction)
    .await;
    assert_sqlstate(result, "22023", "admit with TTL zero");
    transaction.rollback().await.unwrap();

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE id=$1), \
           (SELECT COUNT(*) FROM provider_admission_waits WHERE id=$2), \
           (SELECT COUNT(*) FROM provider_concurrency_leases WHERE id=$3), \
           (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$4)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(lease_id)
    .bind(effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn release_and_throttle_require_the_exact_effect_receipt_pair(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let workspace_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let receipt_id = Uuid::now_v7();

    for (sql, operation) in [
        (
            "SELECT vestrace_release_provider_dispatch($1,$2,$3)",
            "release a foreign receipt pair",
        ),
        (
            "SELECT vestrace_record_provider_throttle($4,$1,$2,$3,60)",
            "throttle a foreign receipt pair",
        ),
    ] {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let result = sqlx::query(sql)
            .bind(workspace_id)
            .bind(effect_id)
            .bind(receipt_id)
            .bind(Uuid::now_v7())
            .fetch_all(&mut *transaction)
            .await;
        assert_sqlstate(result, "23514", operation);
        transaction.rollback().await.unwrap();
    }

    let counts: (i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM provider_concurrency_leases WHERE released_at IS NOT NULL), \
           (SELECT COUNT(*) FROM provider_throttle_observations)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn task10_admission_entrypoint_is_guarded_and_returns_only_bounded_db_time(pool: PgPool) {
    let row = sqlx::query(
        "SELECT p.prosecdef, pg_get_userbyid(p.proowner) AS owner, \
                pg_get_function_result(p.oid) AS result, \
                has_function_privilege('vestrace',p.oid,'EXECUTE') AS runtime_execute, \
                has_function_privilege('public',p.oid,'EXECUTE') AS public_execute \
           FROM pg_proc AS p \
          WHERE p.oid = to_regprocedure(\
            'public.vestrace_try_admit_provider_dispatch(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid,text,integer)'\
          )",
    )
    .fetch_one(&pool)
    .await
    .expect("the exact Task10 admission function must exist");
    assert!(row.get::<bool, _>("prosecdef"));
    assert_eq!(row.get::<String, _>("owner"), "vestrace_guarded_owner");
    assert_eq!(
        row.get::<String, _>("result"),
        "TABLE(decision text, retry_after_seconds integer, concurrency_lease_id uuid, wait_deadline_at timestamp with time zone, dispatch_expires_at timestamp with time zone)"
    );
    assert!(row.get::<bool, _>("runtime_execute"));
    assert!(!row.get::<bool, _>("public_execute"));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn valid_admission_pins_latest_complete_evidence_and_db_authored_expiry(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &runtime).await;
    let evidence = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();

    let row = admit(&runtime, fixture, evidence, admission_id, wait_id, lease_id).await;
    assert_eq!(row.get::<String, _>("decision"), "admitted");
    assert_eq!(row.get::<Option<i32>, _>("retry_after_seconds"), None);
    assert_eq!(
        row.get::<Option<Uuid>, _>("concurrency_lease_id"),
        Some(lease_id)
    );
    assert_eq!(
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("wait_deadline_at"),
        None
    );

    let persisted = sqlx::query(
        "SELECT admission.decision, admission.admitted_at, lease.issued_at, \
                lease.expires_at, cause.model_request_evidence_check_id \
           FROM connection_dispatch_admissions AS admission \
           JOIN provider_concurrency_leases AS lease \
             ON lease.id=$2 AND lease.external_effect_id=admission.external_effect_id \
           JOIN provider_dispatch_causes AS cause \
             ON cause.external_effect_id=admission.external_effect_id \
          WHERE admission.id=$1",
    )
    .bind(admission_id)
    .bind(lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted.get::<String, _>("decision"), "admitted");
    assert_eq!(
        persisted.get::<chrono::DateTime<chrono::Utc>, _>("admitted_at"),
        persisted.get::<chrono::DateTime<chrono::Utc>, _>("issued_at")
    );
    assert_eq!(
        persisted.get::<chrono::DateTime<chrono::Utc>, _>("expires_at"),
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("dispatch_expires_at")
            .unwrap()
    );
    assert_eq!(
        persisted.get::<Uuid, _>("model_request_evidence_check_id"),
        evidence.check_id
    );

    let later_check_id = Uuid::now_v7();
    let mut later_check = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *later_check)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(\
          $1,$2,$3,'complete',ARRAY[]::TEXT[],ARRAY[]::UUID[],ARRAY[]::UUID[],ARRAY[]::UUID[])",
    )
    .bind(later_check_id)
    .bind(fixture.workspace_id)
    .bind(evidence.evidence_id)
    .fetch_one(&mut *later_check)
    .await
    .unwrap();
    later_check.commit().await.unwrap();

    let replay = admit(&runtime, fixture, evidence, admission_id, wait_id, lease_id).await;
    assert_eq!(replay.get::<String, _>("decision"), "admitted");
    let before_null_replays: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for (cause_kind, ttl_seconds, operation) in [
        (None, Some(60), "replay an admission with NULL cause kind"),
        (
            Some("qualification_probe"),
            None,
            "replay an admission with NULL requested TTL",
        ),
    ] {
        assert_sqlstate(
            try_admit_with_nullable_replay(
                &runtime,
                fixture,
                evidence,
                AdmissionAttemptIds {
                    admission_id,
                    wait_id,
                    lease_id,
                },
                cause_kind,
                ttl_seconds,
            )
            .await,
            "22023",
            operation,
        );
    }
    let after_null_replays: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_null_replays, before_null_replays);
    assert_sqlstate(
        try_admit_with_ttl(
            &runtime,
            fixture,
            evidence,
            admission_id,
            wait_id,
            lease_id,
            61,
        )
        .await,
        "23514",
        "replay an admission with a changed requested TTL",
    );
    let requested_ttls: (i32, Option<i32>) = sqlx::query_as(
        "SELECT admission.requested_dispatch_ttl_seconds, wait.requested_dispatch_ttl_seconds \
           FROM connection_dispatch_admissions AS admission \
           LEFT JOIN provider_admission_waits AS wait \
             ON wait.external_effect_id=admission.external_effect_id \
          WHERE admission.id=$1",
    )
    .bind(admission_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(requested_ttls, (60, None));
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1)",
    )
    .bind(evidence.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 1));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn admission_revalidates_pinned_canonical_sources_before_replay(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &runtime).await;
    let evidence = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let admitted = admit(&runtime, fixture, evidence, admission_id, wait_id, lease_id).await;
    assert_eq!(admitted.get::<String, _>("decision"), "admitted");

    let before: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut mutation = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE model_limits_revisions \
         DISABLE TRIGGER model_limits_revisions_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query("DELETE FROM model_limits_revisions WHERE id=$1")
        .bind(fixture.limits_id)
        .execute(&mut *mutation)
        .await
        .unwrap();
    sqlx::query(
        "ALTER TABLE model_limits_revisions \
         ENABLE TRIGGER model_limits_revisions_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    mutation.commit().await.unwrap();

    assert_sqlstate(
        try_admit_with_ttl(
            &runtime,
            fixture,
            evidence,
            admission_id,
            wait_id,
            lease_id,
            60,
        )
        .await,
        "23514",
        "replay an admission after a pinned canonical source was removed",
    );
    let after: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn admission_rejects_more_than_8200_locked_evidence_nodes(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &runtime).await;
    let evidence = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let admission_id = Uuid::now_v7();
    let wait_id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let admitted = admit(&runtime, fixture, evidence, admission_id, wait_id, lease_id).await;
    assert_eq!(admitted.get::<String, _>("decision"), "admitted");

    let before: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut mutation = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE model_request_evidence_nodes \
         DISABLE TRIGGER model_request_evidence_nodes_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_request_evidence_nodes(\
             id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,safe_ordinal\
         ) SELECT gen_random_uuid(),$1,$2,ordinal,'qualification_probe',$3,'00' \
             FROM generate_series(6,8200) AS ordinal",
    )
    .bind(fixture.workspace_id)
    .bind(evidence.evidence_id)
    .bind(fixture.qualification_job_id)
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE model_request_evidence_nodes \
         ENABLE TRIGGER model_request_evidence_nodes_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    mutation.commit().await.unwrap();

    assert_sqlstate(
        try_admit_with_ttl(
            &runtime,
            fixture,
            evidence,
            admission_id,
            wait_id,
            lease_id,
            60,
        )
        .await,
        "23514",
        "replay an admission with 8201 locked evidence nodes",
    );
    let after: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
          (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$2 AND connection_id=$3)",
    )
    .bind(evidence.effect_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn saturated_wait_is_durable_and_expired_lease_is_reclaimed_without_sleep(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &runtime).await;
    let first = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let second = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let first_lease = Uuid::now_v7();
    let second_admission = Uuid::now_v7();
    let second_wait = Uuid::now_v7();
    let second_lease = Uuid::now_v7();
    let _ = admit(
        &runtime,
        fixture,
        first,
        Uuid::now_v7(),
        Uuid::now_v7(),
        first_lease,
    )
    .await;

    let waiting = admit(
        &runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(waiting.get::<String, _>("decision"), "conflict");
    assert!(
        waiting
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("wait_deadline_at")
            .is_some()
    );
    let pending: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_admission_waits WHERE id=$1 AND terminal_reason IS NULL), \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$2), \
          (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$2)",
    )
    .bind(second_wait)
    .bind(second.effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, (1, 0, 0));

    let mut expire = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *expire)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *expire)
        .await
        .unwrap();
    sqlx::query("UPDATE provider_concurrency_leases SET issued_at=NOW()-INTERVAL '2 minutes', expires_at=NOW()-INTERVAL '1 minute' WHERE id=$1")
        .bind(first_lease)
        .execute(&mut *expire)
        .await
        .unwrap();
    expire.commit().await.unwrap();

    let admitted = admit(
        &runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(admitted.get::<String, _>("decision"), "admitted");
    let terminal: (Option<String>, bool, bool) = sqlx::query_as(
        "SELECT wait.terminal_reason, old_lease.released_at IS NOT NULL, \
                new_lease.released_at IS NULL \
           FROM provider_admission_waits AS wait \
           JOIN provider_concurrency_leases AS old_lease ON old_lease.id=$2 \
           JOIN provider_concurrency_leases AS new_lease ON new_lease.id=$3 \
          WHERE wait.id=$1",
    )
    .bind(second_wait)
    .bind(first_lease)
    .bind(second_lease)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(terminal, (Some("admitted".into()), true, true));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn exact_429_receipt_releases_lease_and_terminalizes_wait_as_throttled(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &runtime).await;
    let pinned_policy_id = Uuid::now_v7();
    let mut policy = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *policy)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *policy)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_revisions(id,workspace_id,connection_id,version,max_in_flight,requests_per_60_seconds,queue_wait_timeout_seconds,provider_throttle_cap_seconds) VALUES($1,$2,$3,2,1,60000,30,60)")
        .bind(pinned_policy_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .execute(&mut *policy)
        .await
        .unwrap();
    sqlx::query("UPDATE connection_admission_policy_heads SET current_policy_revision_id=$3,version=2,updated_at=NOW() WHERE workspace_id=$1 AND connection_id=$2")
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(pinned_policy_id)
        .execute(&mut *policy)
        .await
        .unwrap();
    policy.commit().await.unwrap();
    let first = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let second = add_dispatch_evidence(&pool, &runtime, fixture).await;
    let first_lease = Uuid::now_v7();
    let _ = admit(
        &runtime,
        fixture,
        first,
        Uuid::now_v7(),
        Uuid::now_v7(),
        first_lease,
    )
    .await;
    let second_admission = Uuid::now_v7();
    let second_wait = Uuid::now_v7();
    let second_lease = Uuid::now_v7();
    let waiting = admit(
        &runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(waiting.get::<String, _>("decision"), "conflict");

    let receipt_id = Uuid::now_v7();
    let observation_id = Uuid::now_v7();

    let mut receipt_only = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *receipt_only)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(serde_json::json!({
            "response_class": "http_429",
            "evidence_refs": ["provider:http_429"]
        }))
        .execute(&mut *receipt_only)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *receipt_only)
        .await
        .unwrap();
    assert_sqlstate(
        receipt_only.commit().await,
        "23514",
        "commit a dispatch http_429 receipt without release and throttle observation",
    );
    let receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM external_effect_receipts WHERE id=$1")
            .bind(receipt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(receipt_count, 0);

    let mut release_only = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *release_only)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(serde_json::json!({
            "response_class": "http_429",
            "evidence_refs": ["provider:http_429"]
        }))
        .execute(&mut *release_only)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *release_only)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
        .bind(fixture.workspace_id)
        .bind(first.effect_id)
        .bind(receipt_id)
        .execute(&mut *release_only)
        .await
        .unwrap();
    assert_sqlstate(
        release_only.commit().await,
        "23514",
        "commit a dispatch http_429 receipt and release without throttle observation",
    );
    let release_rollback: (i64, bool) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM external_effect_receipts WHERE id=$1), \
                released_at IS NULL \
           FROM provider_concurrency_leases WHERE id=$2",
    )
    .bind(receipt_id)
    .bind(first_lease)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(release_rollback, (0, true));

    let mut receipt = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *receipt)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
        .bind(receipt_id)
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(serde_json::json!({
            "response_class": "http_429",
            "evidence_refs": ["provider:http_429"]
        }))
        .execute(&mut *receipt)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(first.effect_id)
        .bind(fixture.workspace_id)
        .bind(receipt_id.to_string())
        .execute(&mut *receipt)
        .await
        .unwrap();
    let released: Uuid = sqlx::query_scalar("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
        .bind(fixture.workspace_id)
        .bind(first.effect_id)
        .bind(receipt_id)
        .fetch_one(&mut *receipt)
        .await
        .unwrap();
    assert_eq!(released, first_lease);
    let observed: Uuid =
        sqlx::query_scalar("SELECT vestrace_record_provider_throttle($1,$2,$3,$4,120)")
            .bind(observation_id)
            .bind(fixture.workspace_id)
            .bind(first.effect_id)
            .bind(receipt_id)
            .fetch_one(&mut *receipt)
            .await
            .unwrap();
    assert_eq!(observed, observation_id);
    receipt.commit().await.unwrap();

    let before_null_retry: (i64, bool, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_throttle_observations WHERE external_effect_id=$1), \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE id=$2), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$3 AND connection_id=$4)",
    )
    .bind(first.effect_id)
    .bind(first_lease)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut null_retry = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *null_retry)
        .await
        .unwrap();
    let null_retry_result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_record_provider_throttle($1,$2,$3,$4,$5::INTEGER)",
    )
    .bind(observation_id)
    .bind(fixture.workspace_id)
    .bind(first.effect_id)
    .bind(receipt_id)
    .bind(Option::<i32>::None)
    .fetch_one(&mut *null_retry)
    .await;
    assert_sqlstate(
        null_retry_result,
        "22023",
        "replay throttle with NULL retry",
    );
    null_retry.rollback().await.unwrap();
    let after_null_retry: (i64, bool, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM provider_throttle_observations WHERE external_effect_id=$1), \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE id=$2), \
          (SELECT version FROM connection_admission_states WHERE workspace_id=$3 AND connection_id=$4)",
    )
    .bind(first.effect_id)
    .bind(first_lease)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_null_retry, before_null_retry);

    let mismatched_replay = {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_record_provider_throttle($1,$2,$3,$4,121)",
        )
        .bind(observation_id)
        .bind(fixture.workspace_id)
        .bind(first.effect_id)
        .bind(receipt_id)
        .fetch_one(&mut *transaction)
        .await;
        transaction.rollback().await.unwrap();
        result
    };
    assert_sqlstate(
        mismatched_replay,
        "23514",
        "replay throttle with unequal requested retry under the same cap",
    );

    let advanced_policy_id = Uuid::now_v7();
    let mut advance = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *advance)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *advance)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_admission_policy_revisions(id,workspace_id,connection_id,version,max_in_flight,requests_per_60_seconds,queue_wait_timeout_seconds,provider_throttle_cap_seconds) VALUES($1,$2,$3,3,1,60000,30,30)")
        .bind(advanced_policy_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .execute(&mut *advance)
        .await
        .unwrap();
    sqlx::query("UPDATE connection_admission_policy_heads SET current_policy_revision_id=$3,version=3,updated_at=NOW() WHERE workspace_id=$1 AND connection_id=$2")
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(advanced_policy_id)
        .execute(&mut *advance)
        .await
        .unwrap();
    advance.commit().await.unwrap();

    let mut exact_replay = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *exact_replay)
        .await
        .unwrap();
    let replayed_observation: Uuid =
        sqlx::query_scalar("SELECT vestrace_record_provider_throttle($1,$2,$3,$4,120)")
            .bind(observation_id)
            .bind(fixture.workspace_id)
            .bind(first.effect_id)
            .bind(receipt_id)
            .fetch_one(&mut *exact_replay)
            .await
            .unwrap();
    exact_replay.commit().await.unwrap();
    assert_eq!(replayed_observation, observation_id);

    let throttled = admit(
        &runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(throttled.get::<String, _>("decision"), "throttled");
    assert_eq!(
        throttled.get::<Option<i32>, _>("retry_after_seconds"),
        Some(30)
    );
    let persisted: (Option<String>, Uuid, Uuid, bool, Uuid, i32, i32) = sqlx::query_as(
        "SELECT wait.terminal_reason, observation.external_effect_receipt_id, \
                lease.released_receipt_id, lease.released_at IS NOT NULL, \
                observation.policy_revision_id, \
                observation.requested_retry_after_seconds, \
                observation.retry_after_seconds \
           FROM provider_admission_waits AS wait \
           JOIN provider_throttle_observations AS observation ON observation.id=$2 \
           JOIN provider_concurrency_leases AS lease ON lease.id=$3 \
          WHERE wait.id=$1",
    )
    .bind(second_wait)
    .bind(observation_id)
    .bind(first_lease)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted,
        (
            Some("throttled".into()),
            receipt_id,
            receipt_id,
            true,
            pinned_policy_id,
            120,
            60,
        )
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn throttle_and_wait_retry_race_serializes_without_deadlock(pool: PgPool) {
    let setup_runtime = runtime_pool(&pool).await;
    let fixture = qualification_fixture(&pool, &setup_runtime).await;
    let first = add_dispatch_evidence(&pool, &setup_runtime, fixture).await;
    let second = add_dispatch_evidence(&pool, &setup_runtime, fixture).await;
    let first_lease = Uuid::now_v7();
    let _ = admit(
        &setup_runtime,
        fixture,
        first,
        Uuid::now_v7(),
        Uuid::now_v7(),
        first_lease,
    )
    .await;
    let second_admission = Uuid::now_v7();
    let second_wait = Uuid::now_v7();
    let second_lease = Uuid::now_v7();
    let waiting = admit(
        &setup_runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(waiting.get::<String, _>("decision"), "conflict");

    let receipt_id = Uuid::now_v7();
    let observation_id = Uuid::now_v7();
    let throttle_runtime = runtime_pool(&pool).await;
    let retry_runtime = runtime_pool(&pool).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let throttle_barrier = Arc::clone(&barrier);
    let throttle_task = tokio::spawn(async move {
        let mut transaction = throttle_runtime.begin().await?;
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;
        throttle_barrier.wait().await;
        sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged',$4)")
            .bind(receipt_id)
            .bind(first.effect_id)
            .bind(fixture.workspace_id)
            .bind(serde_json::json!({
                "response_class": "http_429",
                "evidence_refs": ["provider:http_429"]
            }))
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
            .bind(first.effect_id)
            .bind(fixture.workspace_id)
            .bind(receipt_id.to_string())
            .execute(&mut *transaction)
            .await?;
        sqlx::query_scalar::<_, Uuid>("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
            .bind(fixture.workspace_id)
            .bind(first.effect_id)
            .bind(receipt_id)
            .fetch_one(&mut *transaction)
            .await?;
        sqlx::query_scalar::<_, Uuid>("SELECT vestrace_record_provider_throttle($1,$2,$3,$4,120)")
            .bind(observation_id)
            .bind(fixture.workspace_id)
            .bind(first.effect_id)
            .bind(receipt_id)
            .fetch_one(&mut *transaction)
            .await?;
        transaction.commit().await?;
        throttle_runtime.close().await;
        Ok::<(), sqlx::Error>(())
    });
    let retry_barrier = Arc::clone(&barrier);
    let retry_task = tokio::spawn(async move {
        retry_barrier.wait().await;
        let row = try_admit_with_ttl(
            &retry_runtime,
            fixture,
            second,
            second_admission,
            second_wait,
            second_lease,
            60,
        )
        .await?;
        let decision = row.get::<String, _>("decision");
        retry_runtime.close().await;
        Ok::<String, sqlx::Error>(decision)
    });
    let (throttle_result, retry_result) =
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(throttle_task, retry_task)
        })
        .await
        .expect("throttle/wait retry race must resolve within its bound");
    throttle_result
        .expect("throttle race task must join")
        .expect("throttle race must not deadlock");
    let raced_decision = retry_result
        .expect("wait retry race task must join")
        .expect("wait retry race must not deadlock");
    assert!(
        matches!(raced_decision.as_str(), "conflict" | "throttled"),
        "either serialized winner is valid, got {raced_decision}"
    );

    let final_retry = admit(
        &setup_runtime,
        fixture,
        second,
        second_admission,
        second_wait,
        second_lease,
    )
    .await;
    assert_eq!(final_retry.get::<String, _>("decision"), "throttled");
    let persisted: (i64, i64, bool, Option<String>) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM external_effect_receipts WHERE id=$1), \
          (SELECT COUNT(*) FROM provider_throttle_observations WHERE id=$2), \
          (SELECT released_at IS NOT NULL FROM provider_concurrency_leases WHERE id=$3), \
          (SELECT terminal_reason FROM provider_admission_waits WHERE id=$4)",
    )
    .bind(receipt_id)
    .bind(observation_id)
    .bind(first_lease)
    .bind(second_wait)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (1, 1, true, Some("throttled".into())));
}

/// Everything the governed publisher needs and nothing it does not: a
/// workspace, a principal, and a connector for the Connection to belong to.
async fn seed_publisher_context(pool: &PgPool, context: &RequestContext, connector_id: Uuid) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("publisher-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("principal-{}", context.principal_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) \
         VALUES ($1, $2, $3, 'local')",
    )
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
}

fn connection_revision_command(
    context: &RequestContext,
    connector_id: Uuid,
    connection_id: ConnectionId,
) -> CreateConnectionRevision {
    let at = now();
    let connection = Connection {
        id: connection_id,
        connector_id: ConnectorId::from_uuid(connector_id),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        name: format!("publisher-{connection_id}"),
        status: ConnectionStatus::Active,
        created_at: at,
    };
    CreateConnectionRevision {
        connection,
        revision_id: ConnectionRevisionId::new(),
        execution_guard_id: Uuid::now_v7(),
        kind: ConnectionKind::LMStudioLocal,
        logical_base_url: "http://127.0.0.1:1234/v1".to_owned(),
        runtime_base_url: "http://127.0.0.1:1234/v1".to_owned(),
        adapter_profile_revision: "lm-studio-local/v1".to_owned(),
        transport_policy: ConnectionTransportPolicy::LoopbackOnly,
        auth_mode: ConnectionAuthMode::None,
        credential_slot_id: None,
        expected_head_version: 0,
        idempotency: None,
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "connection.revision.created",
            "connection",
            connection_id.as_uuid(),
            serde_json::json!({"connection_id": connection_id}),
            at,
        )
        .unwrap(),
    }
}

fn publish_policy_command(
    context: &RequestContext,
    connection_id: ConnectionId,
    expected_head_version: u64,
) -> PublishConnectionAdmissionPolicy {
    let at = now();
    PublishConnectionAdmissionPolicy {
        connection_id,
        policy_revision_id: ConnectionAdmissionPolicyId::new(),
        limits: ConnectionAdmissionLimits::new(4, 60, 30, 900).unwrap(),
        expected_head_version,
        idempotency: None,
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "connection.admission_policy.published",
            "connection",
            connection_id.as_uuid(),
            serde_json::json!({"connection_id": connection_id}),
            at,
        )
        .unwrap(),
    }
}

/// The admission policy has a producer, and it is one a deployment can reach.
///
/// P03 declared `ConnectionAdmissionPolicy`, made
/// `vestrace_try_admit_provider_dispatch` refuse without a current revision,
/// and shipped no way to write one: every test that needed an admissible
/// Connection inserted the two rows itself as the guarded owner. That left
/// provider dispatch unreachable in production while the suite reported it
/// working. This runs the whole published path -- application command, governed
/// mutation, guarded compare-and-swap -- over a Connection created moments
/// earlier by the same repository.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_fresh_connection_can_be_given_its_admission_policy(pool: PgPool) {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let connector_id = Uuid::now_v7();
    let connection_id = ConnectionId::new();
    seed_publisher_context(&pool, &context, connector_id).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(
            context.clone(),
            connection_revision_command(&context, connector_id, connection_id),
        )
        .await
        .expect("the governed connection is created by its own repository");

    let policy = publish_policy_command(&context, connection_id, 0);
    let policy_revision_id = policy.policy_revision_id;
    repository
        .publish_admission_policy_governed(context.clone(), policy)
        .await
        .expect("a connection with no policy accepts its first one");

    let head: (Uuid, i64) = sqlx::query_as(
        "SELECT current_policy_revision_id, version FROM connection_admission_policy_heads \
         WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(connection_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(head, (policy_revision_id.as_uuid(), 1));
    let limits: (i16, i32, i32, i32) = sqlx::query_as(
        "SELECT max_in_flight, requests_per_60_seconds, queue_wait_timeout_seconds, \
         provider_throttle_cap_seconds FROM connection_admission_policy_revisions WHERE id=$1",
    )
    .bind(policy_revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        limits,
        (4, 60, 30, 900),
        "the stored limits must be the ones the caller stated"
    );
}

/// Two publishers cannot both believe they wrote the policy the next dispatch
/// will consult.
///
/// The second caller states the version it read, and it read zero -- the same
/// value the first caller legitimately used to create the head. Without the
/// compare-and-swap that second publication would silently replace a policy its
/// author never saw, which is the failure the revision head already refuses for
/// Connections.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_publisher_that_did_not_see_the_current_policy_loses(pool: PgPool) {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let connector_id = Uuid::now_v7();
    let connection_id = ConnectionId::new();
    seed_publisher_context(&pool, &context, connector_id).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(
            context.clone(),
            connection_revision_command(&context, connector_id, connection_id),
        )
        .await
        .unwrap();
    repository
        .publish_admission_policy_governed(
            context.clone(),
            publish_policy_command(&context, connection_id, 0),
        )
        .await
        .unwrap();

    let stale = publish_policy_command(&context, connection_id, 0);
    let stale_revision_id = stale.policy_revision_id;
    let error = repository
        .publish_admission_policy_governed(context.clone(), stale)
        .await
        .expect_err("a stale expected version must lose");
    assert!(
        matches!(
            &error,
            vestrace_application::ApplicationError::Conflict(code)
                if code == "CONNECTION_ADMISSION_POLICY_VERSION_CONFLICT"
        ),
        "unexpected error: {error:?}"
    );
    let orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM connection_admission_policy_revisions WHERE id=$1",
    )
    .bind(stale_revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        orphans, 0,
        "a refused publication leaves no revision behind"
    );

    // The caller that names the version it actually read succeeds, and the
    // superseded revision stays in place as history.
    repository
        .publish_admission_policy_governed(
            context.clone(),
            publish_policy_command(&context, connection_id, 1),
        )
        .await
        .expect("naming the current version publishes the next one");
    let (version, revisions): (i64, i64) = sqlx::query_as(
        "SELECT head.version, (SELECT COUNT(*) FROM connection_admission_policy_revisions \
          WHERE workspace_id=head.workspace_id AND connection_id=head.connection_id) \
           FROM connection_admission_policy_heads AS head \
          WHERE head.workspace_id=$1 AND head.connection_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(connection_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((version, revisions), (2, 2));
}

/// A policy for a Connection that cannot execute would be a policy nothing
/// consults.
///
/// The guarded function takes the permanent execution guard first, and a
/// Connection without one has no governed revision either. Refusing here keeps
/// the guard's lock order canonical and stops a policy row from outliving the
/// Connection it claims to govern.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_connection_without_an_execution_guard_gets_no_policy(pool: PgPool) {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let connector_id = Uuid::now_v7();
    let connection_id = ConnectionId::new();
    seed_publisher_context(&pool, &context, connector_id).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));

    let error = repository
        .publish_admission_policy_governed(
            context.clone(),
            publish_policy_command(&context, connection_id, 0),
        )
        .await
        .expect_err("a Connection that was never created governs nothing");
    assert!(
        matches!(
            &error,
            vestrace_application::ApplicationError::Policy(message)
                if message == "connection admission policy requires an existing governed connection"
        ),
        "unexpected error: {error:?}"
    );
}
