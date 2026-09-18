//! PostgreSQL contract tests for `qualification_job_work_claims`: the
//! claim/finish lease pair added over `qualification_jobs` by migration 0217,
//! modeled directly on `migrations/0199_embedding_executor_work.sql`'s proven
//! embedding work-claim shape, simplified to one work kind.

use std::str::FromStr;

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

/// A connection to the same `#[sqlx::test]` database, authenticated as the
/// restricted runtime role rather than the superuser `pool` connection.
/// `test` (the superuser) bypasses RLS and every GRANT/REVOKE check, so a
/// claim/finish call made through it would prove nothing about whether the
/// restricted role this program actually runs as can call these functions.
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

struct QualificationJobFixture {
    workspace_id: Uuid,
    job_id: Uuid,
}

/// Seeds the minimal FK chain `qualification_jobs` requires (a workspace, a
/// principal, a connector, a connection, and a connection revision), then
/// inserts one `qualification_jobs` row in state `requested`. Mirrors
/// `qualification_fixture` in `provider_admission.rs`, trimmed to only what
/// this suite needs: no target binding, no model shape/limits, no admission
/// policy -- the claim/finish functions never look past `qualification_jobs`
/// itself.
async fn seed_requested_qualification_job(
    pool: &PgPool,
    runtime: &PgPool,
) -> QualificationJobFixture {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let job_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id)
        .bind(format!("qualification-work-claims-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!(
            "qualification-work-claims-principal-{principal_id}"
        ))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!(
        "qualification-work-claims-connector-{connector_id}"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) \
         VALUES($1,$2,$3,$4,$5,'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(format!(
        "qualification-work-claims-connection-{connection_id}"
    ))
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
    sqlx::query(
        "INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) \
         VALUES($1,$2,$3,'q1','requested')",
    )
    .bind(job_id)
    .bind(workspace_id)
    .bind(connection_revision_id)
    .execute(&mut *setup)
    .await
    .unwrap();
    setup.commit().await.unwrap();

    QualificationJobFixture {
        workspace_id,
        job_id,
    }
}

/// Runs one claim/finish call in its own committed transaction, re-asserting
/// the `vestrace.workspace_id` RLS GUC first. Each call is deliberately its
/// own transaction rather than one shared transaction across a whole test:
/// `now()` is frozen for the lifetime of a single PostgreSQL transaction, and
/// `qualification_job_work_claims`'s own `CHECK (claim_deadline > created_at)`
/// depends on a `retryable_failure` finish's `claim_deadline = now()` landing
/// strictly after the row's `created_at = now()` from the earlier claim --
/// which only holds when those two calls are genuinely different
/// transactions, exactly as two separate worker RPCs would be in production.
async fn claim(runtime: &PgPool, workspace_id: Uuid, owner: &str, limit: i32) -> Vec<Uuid> {
    let mut session = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *session)
        .await
        .unwrap();
    let claimed = sqlx::query_scalar::<_, Uuid>(
        "SELECT job_id FROM vestrace_claim_qualification_work($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind(owner)
    .bind(limit)
    .fetch_all(&mut *session)
    .await
    .unwrap();
    session.commit().await.unwrap();
    claimed
}

async fn finish(
    runtime: &PgPool,
    workspace_id: Uuid,
    job_id: Uuid,
    owner: &str,
    outcome: &str,
) -> bool {
    let mut session = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *session)
        .await
        .unwrap();
    let finished =
        sqlx::query_scalar::<_, bool>("SELECT vestrace_finish_qualification_work($1, $2, $3, $4)")
            .bind(workspace_id)
            .bind(job_id)
            .bind(owner)
            .bind(outcome)
            .fetch_one(&mut *session)
            .await
            .unwrap();
    session.commit().await.unwrap();
    finished
}

#[sqlx::test(migrator = "vestrace_infrastructure::QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR")]
async fn claiming_qualification_work_leases_a_requested_job_and_hides_it_from_a_second_claimant(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = seed_requested_qualification_job(&pool, &runtime).await;

    let claimed_by_first = claim(&runtime, fixture.workspace_id, "worker-a", 10).await;
    assert_eq!(claimed_by_first, vec![fixture.job_id]);

    let claimed_by_second = claim(&runtime, fixture.workspace_id, "worker-b", 10).await;
    assert!(
        claimed_by_second.is_empty(),
        "a live claim must hide the job from a second claimant"
    );

    let finished = finish(
        &runtime,
        fixture.workspace_id,
        fixture.job_id,
        "worker-a",
        "completed",
    )
    .await;
    assert!(finished);

    // This migration's claim/finish pair is a lease over `qualification_jobs`,
    // not a state machine for it: `vestrace_finish_qualification_work` only
    // ever touches `qualification_job_work_claims`, and this fixture's job
    // stays in state 'requested' throughout (nothing here calls the existing
    // `vestrace_finalize_qualification_job`). So once the completed claim's
    // row is deleted, the job is -- correctly, for this migration alone --
    // claimable again: it still matches `job.state IN ('requested',
    // 'running')` and now has no live claim. A caller that wants "completed
    // work never runs twice" must pair a 'completed' finish with its own
    // transition of `qualification_jobs.state` to a terminal value; that
    // pairing is Task 2's job, not this table's.
    let reclaimed = claim(&runtime, fixture.workspace_id, "worker-c", 10).await;
    assert_eq!(
        reclaimed,
        vec![fixture.job_id],
        "a completed claim's release makes the still-'requested' job claimable again; \
         only a caller-side job-state transition (out of this migration's scope) prevents that"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR")]
async fn a_retryable_failure_makes_the_job_immediately_reclaimable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = seed_requested_qualification_job(&pool, &runtime).await;

    let claimed = claim(&runtime, fixture.workspace_id, "worker-a", 10).await;
    assert_eq!(claimed, vec![fixture.job_id]);

    let finished = finish(
        &runtime,
        fixture.workspace_id,
        fixture.job_id,
        "worker-a",
        "retryable_failure",
    )
    .await;
    assert!(finished);

    let reclaimed = claim(&runtime, fixture.workspace_id, "worker-b", 10).await;
    assert_eq!(
        reclaimed,
        vec![fixture.job_id],
        "a retryable failure must leave the job immediately claimable again"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR")]
async fn a_definite_failure_deletes_the_claim_like_completion_does(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = seed_requested_qualification_job(&pool, &runtime).await;

    let claimed = claim(&runtime, fixture.workspace_id, "worker-a", 10).await;
    assert_eq!(claimed, vec![fixture.job_id]);

    let finished = finish(
        &runtime,
        fixture.workspace_id,
        fixture.job_id,
        "worker-a",
        "definite_failure",
    )
    .await;
    assert!(finished);

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM qualification_job_work_claims WHERE job_id = $1")
            .bind(fixture.job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "a definite failure must delete the claim row");
}

#[sqlx::test(migrator = "vestrace_infrastructure::QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR")]
async fn finishing_with_the_wrong_owner_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = seed_requested_qualification_job(&pool, &runtime).await;

    let claimed = claim(&runtime, fixture.workspace_id, "worker-a", 10).await;
    assert_eq!(claimed, vec![fixture.job_id]);

    let finished_by_impostor = finish(
        &runtime,
        fixture.workspace_id,
        fixture.job_id,
        "worker-b",
        "completed",
    )
    .await;
    assert!(
        !finished_by_impostor,
        "a claim must only be finished by the owner that holds it"
    );

    let still_hidden = claim(&runtime, fixture.workspace_id, "worker-c", 10).await;
    assert!(
        still_hidden.is_empty(),
        "the live claim must survive a finish attempt from the wrong owner"
    );
}

/// Established acceptance pattern for every guarded table in this program
/// (see e.g. `runtime_cannot_bypass_the_guarded_qualification_tables` in
/// `qualification_target_binding.rs`, and the P02 security-material
/// foundation plan's "the runtime role cannot perform direct INSERT, UPDATE,
/// or DELETE on any P02 table, proved by connecting as the runtime role"):
/// after this migration's closing hardening block, `qualification_job_work_
/// claims` is owned by `vestrace_guarded_owner` with `FORCE ROW LEVEL
/// SECURITY` and `REVOKE ALL ... FROM PUBLIC, vestrace`, so the restricted
/// runtime role must be refused a raw write even before its own
/// `vestrace_reject_raw_p03_mutation` trigger would fire. No fixture rows are
/// needed: PostgreSQL enforces table-level ACLs before evaluating any
/// constraint or scanning any row, so bogus/nonexistent ids still trigger the
/// same 42501 refusal.
#[sqlx::test(migrator = "vestrace_infrastructure::QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR")]
async fn runtime_role_cannot_write_qualification_job_work_claims_directly(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;

    let insert_error = sqlx::query(
        "INSERT INTO qualification_job_work_claims \
         (workspace_id, job_id, claim_owner, claim_deadline) \
         VALUES ($1, $2, 'runtime-direct', NOW() + INTERVAL '60 seconds')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await
    .expect_err("the runtime role must not insert a claim row directly");
    assert_eq!(
        insert_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("42501"),
        "direct INSERT must be refused with insufficient_privilege: {insert_error}"
    );

    let update_error = sqlx::query(
        "UPDATE qualification_job_work_claims SET claim_owner = 'runtime-direct' WHERE job_id = $1",
    )
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await
    .expect_err("the runtime role must not update a claim row directly");
    assert_eq!(
        update_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("42501"),
        "direct UPDATE must be refused with insufficient_privilege: {update_error}"
    );

    let delete_error = sqlx::query("DELETE FROM qualification_job_work_claims WHERE job_id = $1")
        .bind(Uuid::now_v7())
        .execute(&runtime)
        .await
        .expect_err("the runtime role must not delete a claim row directly");
    assert_eq!(
        delete_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("42501"),
        "direct DELETE must be refused with insufficient_privilege: {delete_error}"
    );
}
