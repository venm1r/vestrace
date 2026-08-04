//! Restricted runtime permissions and workspace isolation for run events.

mod support;

use std::future::Future;

use support::assert_sqlstate;

const WORKSPACE_A: &str = "54000000-0000-0000-0000-000000000001";
const WORKSPACE_B: &str = "54000000-0000-0000-0000-000000000002";
const PRINCIPAL_A: &str = "54000000-0000-0000-0000-000000000003";
const PRINCIPAL_B: &str = "54000000-0000-0000-0000-000000000004";
const RUN_A: &str = "54000000-0000-0000-0000-000000000005";
const RUN_B: &str = "54000000-0000-0000-0000-000000000006";

#[derive(Clone)]
struct RuntimeRole {
    name: String,
}

impl RuntimeRole {
    fn quoted(&self) -> String {
        format!("\"{}\"", self.name.replace('"', "\"\""))
    }
}

async fn create_runtime_role(pool: &sqlx::PgPool) -> RuntimeRole {
    let name: String = sqlx::query_scalar(
        "SELECT 'vestrace_run_events_' || replace(gen_random_uuid()::text, '-', '')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let role = RuntimeRole { name };
    let quoted = role.quoted();

    sqlx::query(&format!(
        "CREATE ROLE {quoted} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {quoted}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT SELECT ON TABLE agent_runs TO {quoted}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT, INSERT ON TABLE run_events TO {quoted}"
    ))
    .execute(pool)
    .await
    .unwrap();

    role
}

async fn cleanup_runtime_role(pool: &sqlx::PgPool, role: &RuntimeRole) -> Vec<sqlx::Error> {
    let quoted = role.quoted();
    let statements = [
        format!("REVOKE ALL PRIVILEGES ON TABLE agent_runs, run_events FROM {quoted}"),
        format!("REVOKE USAGE ON SCHEMA public FROM {quoted}"),
        format!("DROP ROLE {quoted}"),
    ];
    let mut errors = Vec::new();
    for statement in statements {
        if let Err(error) = sqlx::query(&statement).execute(pool).await {
            errors.push(error);
        }
    }
    errors
}

async fn with_runtime_role<T, F, Fut>(pool: &sqlx::PgPool, test: F) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RuntimeRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let role = create_runtime_role(pool).await;
    let test_pool = pool.clone();
    let test_role = role.clone();
    let outcome = tokio::spawn(async move { test(test_pool, test_role).await }).await;
    let cleanup_errors = cleanup_runtime_role(pool, &role).await;
    assert!(
        cleanup_errors.is_empty(),
        "runtime role cleanup failed: {cleanup_errors:?}"
    );

    match outcome {
        Ok(value) => value,
        Err(join_error) if join_error.is_panic() => {
            std::panic::resume_unwind(join_error.into_panic())
        }
        Err(join_error) => panic!("runtime role test task was cancelled: {join_error}"),
    }
}

async fn assume_workspace(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role: &RuntimeRole,
    workspace_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!("SET LOCAL ROLE {}", role.quoted()))
        .execute(&mut **transaction)
        .await?;
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(workspace_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn visible_event_count(
    pool: &sqlx::PgPool,
    role: &RuntimeRole,
    workspace_id: &str,
) -> Result<i64, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_workspace(&mut transaction, role, workspace_id).await?;
    let count = sqlx::query_scalar("SELECT count(*) FROM run_events")
        .fetch_one(&mut *transaction)
        .await?;
    transaction.rollback().await?;
    Ok(count)
}

async fn execute_and_rollback(
    pool: &sqlx::PgPool,
    role: &RuntimeRole,
    workspace_id: &str,
    statement: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_workspace(&mut transaction, role, workspace_id).await?;
    let result = sqlx::query(statement).execute(&mut *transaction).await;
    let rollback = transaction.rollback().await;
    match (result, rollback) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) | (Err(_), Err(error)) => Err(error),
    }
}

async fn execute_and_commit(
    pool: &sqlx::PgPool,
    role: &RuntimeRole,
    workspace_id: &str,
    statement: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_workspace(&mut transaction, role, workspace_id).await?;
    let result = sqlx::query(statement).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(result)
}

async fn seed_run_events(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1::uuid, 'run-events-rls-a'),
         ($2::uuid, 'run-events-rls-b')",
    )
    .bind(WORKSPACE_A)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         ($1::uuid, $2::uuid, 'run-events-rls-principal-a'),
         ($3::uuid, $4::uuid, 'run-events-rls-principal-b')",
    )
    .bind(PRINCIPAL_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_B)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, status, run_version
         ) VALUES
         ($1::uuid, $2::uuid, $3::uuid, 'run-a', 'created', 1),
         ($4::uuid, $5::uuid, $6::uuid, 'run-b', 'created', 1)",
    )
    .bind(RUN_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_A)
    .bind(RUN_B)
    .bind(WORKSPACE_B)
    .bind(PRINCIPAL_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO run_events (
             id, workspace_id, run_id, sequence, event_type, event_version,
             actor, causation_id, correlation_id, payload, occurred_at, created_at
         ) VALUES (
             '54000000-0000-0000-0000-000000000010',
             $1::uuid,
             $2::uuid,
             1,
             'run.created',
             1,
             '{\"system\":{\"component\":\"seed\"}}'::jsonb,
             '54000000-0000-0000-0000-000000000011',
             '54000000-0000-0000-0000-000000000012',
             '{}'::jsonb,
             now(),
             now()
         )",
    )
    .bind(WORKSPACE_A)
    .bind(RUN_A)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn restricted_runtime_role_enforces_run_event_boundaries(pool: sqlx::PgPool) {
    seed_run_events(&pool).await;

    with_runtime_role(&pool, |pool, role| async move {
        assert_eq!(
            visible_event_count(&pool, &role, WORKSPACE_A)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            visible_event_count(&pool, &role, WORKSPACE_B)
                .await
                .unwrap(),
            0
        );

        let cross_workspace_insert = execute_and_rollback(
            &pool,
            &role,
            WORKSPACE_B,
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at, created_at
             ) VALUES (
                 '54000000-0000-0000-0000-000000000020',
                 '54000000-0000-0000-0000-000000000001',
                 '54000000-0000-0000-0000-000000000005',
                 2,
                 'run.marked_ready',
                 1,
                 '{\"system\":{\"component\":\"runtime\"}}'::jsonb,
                 '54000000-0000-0000-0000-000000000021',
                 '54000000-0000-0000-0000-000000000022',
                 '{}'::jsonb,
                 now(),
                 now()
             )",
        )
        .await;
        assert_sqlstate(cross_workspace_insert, "42501");

        execute_and_commit(
            &pool,
            &role,
            WORKSPACE_A,
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at, created_at
             ) VALUES (
                 '54000000-0000-0000-0000-000000000030',
                 '54000000-0000-0000-0000-000000000001',
                 '54000000-0000-0000-0000-000000000005',
                 2,
                 'run.marked_ready',
                 1,
                 '{\"system\":{\"component\":\"runtime\"}}'::jsonb,
                 '54000000-0000-0000-0000-000000000031',
                 '54000000-0000-0000-0000-000000000032',
                 '{}'::jsonb,
                 now(),
                 now()
             )",
        )
        .await
        .unwrap();
        assert_eq!(
            visible_event_count(&pool, &role, WORKSPACE_A)
                .await
                .unwrap(),
            2
        );

        let update = execute_and_rollback(
            &pool,
            &role,
            WORKSPACE_A,
            "UPDATE run_events SET event_type = 'changed'",
        )
        .await;
        assert_sqlstate(update, "42501");

        let delete =
            execute_and_rollback(&pool, &role, WORKSPACE_A, "DELETE FROM run_events").await;
        assert_sqlstate(delete, "42501");
    })
    .await;
}
