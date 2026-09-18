#![allow(dead_code)]

use sqlx::postgres::PgQueryResult;
use std::future::Future;

pub const WORKSPACE_A: &str = "40000000-0000-0000-0000-000000000001";
pub const WORKSPACE_B: &str = "40000000-0000-0000-0000-000000000002";

pub const TENANT_TABLES: [&str; 6] = [
    "workspaces",
    "principals",
    "roles",
    "capabilities",
    "principal_roles",
    "role_capabilities",
];

#[derive(Clone)]
pub struct RestrictedRole {
    name: String,
}

impl RestrictedRole {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn quoted(&self) -> String {
        quote_identifier(&self.name)
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub async fn create_restricted_role(pool: &sqlx::PgPool) -> RestrictedRole {
    let name: String =
        sqlx::query_scalar("SELECT 'vestrace_rls_' || replace(gen_random_uuid()::text, '-', '')")
            .fetch_one(pool)
            .await
            .unwrap();
    let quoted = quote_identifier(&name);

    sqlx::query(&format!(
        "CREATE ROLE {quoted} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS"
    ))
    .execute(pool)
    .await
    .unwrap();
    let role = RestrictedRole { name };
    let grants = async {
        sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {quoted}"))
            .execute(pool)
            .await?;
        sqlx::query(&format!(
            "GRANT SELECT, INSERT, UPDATE ON TABLE workspaces, principals, roles, capabilities, principal_roles, role_capabilities TO {quoted}"
        ))
        .execute(pool)
        .await?;
        Ok::<(), sqlx::Error>(())
    }
    .await;

    if let Err(error) = grants {
        let cleanup_errors = cleanup_restricted_role(pool, &role).await;
        panic!("restricted role setup failed: {error}; cleanup errors: {cleanup_errors:?}");
    }

    role
}

async fn cleanup_restricted_role(pool: &sqlx::PgPool, role: &RestrictedRole) -> Vec<sqlx::Error> {
    let quoted = role.quoted();
    let statements = [
        format!(
            "REVOKE ALL PRIVILEGES ON TABLE workspaces, principals, roles, capabilities, principal_roles, role_capabilities FROM {quoted}"
        ),
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

pub async fn with_restricted_role<T, F, Fut>(pool: &sqlx::PgPool, test: F) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RestrictedRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let role = create_restricted_role(pool).await;
    let test_pool = pool.clone();
    let test_role = role.clone();
    let outcome = tokio::spawn(async move { test(test_pool, test_role).await }).await;
    let cleanup_errors = cleanup_restricted_role(pool, &role).await;
    assert!(
        cleanup_errors.is_empty(),
        "restricted role cleanup failed: {cleanup_errors:?}"
    );

    match outcome {
        Ok(value) => value,
        Err(join_error) if join_error.is_panic() => {
            std::panic::resume_unwind(join_error.into_panic())
        }
        Err(join_error) => panic!("restricted role test task was cancelled: {join_error}"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum RestrictedRuntimeRoleSetupStep {
    GrantConnect,
    GrantSchemaUsage,
    GrantTableSelect,
}

#[derive(Default)]
struct RestrictedRuntimeRoleSetupProgress {
    connect_granted: bool,
    schema_usage_granted: bool,
    table_select_granted: bool,
}

async fn cleanup_restricted_runtime_role(
    admin_pool: &sqlx::PgPool,
    role: &RestrictedRole,
    database_name: &str,
    progress: &RestrictedRuntimeRoleSetupProgress,
) -> Vec<sqlx::Error> {
    let quoted = role.quoted();
    let quoted_db = quote_identifier(database_name);
    let mut statements = Vec::new();
    if progress.table_select_granted {
        statements.push(format!(
            "REVOKE ALL PRIVILEGES ON TABLE memories, memory_revisions FROM {quoted}"
        ));
    }
    if progress.schema_usage_granted {
        statements.push(format!("REVOKE USAGE ON SCHEMA public FROM {quoted}"));
    }
    if progress.connect_granted {
        statements.push(format!(
            "REVOKE CONNECT ON DATABASE {quoted_db} FROM {quoted}"
        ));
    }
    statements.push(format!("DROP ROLE {quoted}"));

    let mut errors = Vec::new();
    for statement in statements {
        if let Err(error) = sqlx::query(&statement).execute(admin_pool).await {
            errors.push(error);
        }
    }
    errors
}

pub async fn with_restricted_runtime_role<T, F, Fut>(admin_pool: &sqlx::PgPool, test: F) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RestrictedRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    with_restricted_runtime_role_connector(
        admin_pool,
        None,
        |options, _role| async move {
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
        },
        test,
    )
    .await
}

pub async fn with_restricted_runtime_role_connector<T, F, Fut, C, CFut>(
    admin_pool: &sqlx::PgPool,
    fail_before: Option<RestrictedRuntimeRoleSetupStep>,
    connect: C,
    test: F,
) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RestrictedRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    C: FnOnce(sqlx::postgres::PgConnectOptions, RestrictedRole) -> CFut + Send + 'static,
    CFut: Future<Output = Result<sqlx::PgPool, sqlx::Error>> + Send + 'static,
{
    let name: String = sqlx::query_scalar(
        "SELECT 'vestrace_runtime_' || replace(gen_random_uuid()::text, '-', '')",
    )
    .fetch_one(admin_pool)
    .await
    .unwrap();
    let password: String = sqlx::query_scalar("SELECT replace(gen_random_uuid()::text, '-', '')")
        .fetch_one(admin_pool)
        .await
        .unwrap();
    let role = RestrictedRole { name };
    let quoted = role.quoted();

    sqlx::query(&format!(
        "CREATE ROLE {quoted} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS"
    ))
    .execute(admin_pool)
    .await
    .unwrap();

    let database_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(admin_pool)
        .await
        .unwrap();
    let quoted_db = quote_identifier(&database_name);

    let mut progress = RestrictedRuntimeRoleSetupProgress::default();

    if let Some(step) = fail_before {
        if step == RestrictedRuntimeRoleSetupStep::GrantConnect {
            let cleanup_errors =
                cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
            assert!(
                cleanup_errors.is_empty(),
                "cleanup failed: {cleanup_errors:?}"
            );
            panic!(
                "restricted runtime role setup failed for {}: failpoint before GrantConnect",
                role.name()
            );
        }
    }
    if let Err(err) = sqlx::query(&format!(
        "GRANT CONNECT ON DATABASE {quoted_db} TO {quoted}"
    ))
    .execute(admin_pool)
    .await
    {
        let cleanup_errors =
            cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
        panic!(
            "restricted runtime role setup failed for {}: {err}; cleanup errors: {cleanup_errors:?}",
            role.name()
        );
    }
    progress.connect_granted = true;

    if let Some(step) = fail_before {
        if step == RestrictedRuntimeRoleSetupStep::GrantSchemaUsage {
            let cleanup_errors =
                cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
            assert!(
                cleanup_errors.is_empty(),
                "cleanup failed: {cleanup_errors:?}"
            );
            panic!(
                "restricted runtime role setup failed for {}: failpoint before GrantSchemaUsage",
                role.name()
            );
        }
    }
    if let Err(err) = sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {quoted}"))
        .execute(admin_pool)
        .await
    {
        let cleanup_errors =
            cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
        panic!(
            "restricted runtime role setup failed for {}: {err}; cleanup errors: {cleanup_errors:?}",
            role.name()
        );
    }
    progress.schema_usage_granted = true;

    if let Some(step) = fail_before {
        if step == RestrictedRuntimeRoleSetupStep::GrantTableSelect {
            let cleanup_errors =
                cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
            assert!(
                cleanup_errors.is_empty(),
                "cleanup failed: {cleanup_errors:?}"
            );
            panic!(
                "restricted runtime role setup failed for {}: failpoint before GrantTableSelect",
                role.name()
            );
        }
    }
    if let Err(err) = sqlx::query(&format!(
        "GRANT SELECT ON TABLE memories, memory_revisions TO {quoted}"
    ))
    .execute(admin_pool)
    .await
    {
        let cleanup_errors =
            cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
        panic!(
            "restricted runtime role setup failed for {}: {err}; cleanup errors: {cleanup_errors:?}",
            role.name()
        );
    }
    progress.table_select_granted = true;

    let options = admin_pool
        .connect_options()
        .as_ref()
        .clone()
        .username(role.name())
        .password(&password);
    let connect_result = connect(options, role.clone()).await;
    let runtime_pool = match connect_result {
        Ok(pool) => pool,
        Err(err) => {
            let cleanup_errors =
                cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
            assert!(
                cleanup_errors.is_empty(),
                "cleanup failed: {cleanup_errors:?}"
            );
            panic!("restricted runtime role pool connection failed: {err}");
        }
    };

    let test_pool = runtime_pool.clone();
    let test_role = role.clone();
    let outcome = tokio::spawn(async move { test(test_pool, test_role).await }).await;

    runtime_pool.close().await;

    let cleanup_errors =
        cleanup_restricted_runtime_role(admin_pool, &role, &database_name, &progress).await;
    assert!(
        cleanup_errors.is_empty(),
        "cleanup failed: {cleanup_errors:?}"
    );

    match outcome {
        Ok(value) => value,
        Err(join_error) if join_error.is_panic() => {
            std::panic::resume_unwind(join_error.into_panic())
        }
        Err(join_error) => panic!("restricted runtime role test task was cancelled: {join_error}"),
    }
}

pub async fn assume_restricted_workspace(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role: &RestrictedRole,
    workspace_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!("SET LOCAL ROLE {}", role.quoted()))
        .execute(&mut **transaction)
        .await?;

    if let Some(workspace_id) = workspace_id {
        sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
            .bind(workspace_id)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

pub async fn execute_as_restricted_workspace(
    pool: &sqlx::PgPool,
    role: &RestrictedRole,
    workspace_id: Option<&str>,
    statement: &str,
) -> Result<PgQueryResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_restricted_workspace(&mut transaction, role, workspace_id).await?;
    let result = sqlx::query(statement).execute(&mut *transaction).await;
    if let Err(rollback_error) = transaction.rollback().await {
        return result.and(Err(rollback_error));
    }
    result
}

pub async fn execute_many_as_restricted_workspace(
    pool: &sqlx::PgPool,
    role: &RestrictedRole,
    workspace_id: Option<&str>,
    statements: &[&str],
) -> Result<Vec<u64>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_restricted_workspace(&mut transaction, role, workspace_id).await?;
    let mut rows_affected = Vec::new();
    for statement in statements {
        match sqlx::query(statement).execute(&mut *transaction).await {
            Ok(result) => rows_affected.push(result.rows_affected()),
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        }
    }
    transaction.commit().await?;
    Ok(rows_affected)
}

pub async fn seed_rls_tenants(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ('40000000-0000-0000-0000-000000000001', 'rls-workspace-a'),
         ('40000000-0000-0000-0000-000000000002', 'rls-workspace-b')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         ('40000000-0000-0000-0000-000000000003', '40000000-0000-0000-0000-000000000001', 'principal-a'),
         ('40000000-0000-0000-0000-000000000004', '40000000-0000-0000-0000-000000000002', 'principal-b')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO roles (id, workspace_id, name) VALUES
         ('40000000-0000-0000-0000-000000000005', '40000000-0000-0000-0000-000000000001', 'role-a'),
         ('40000000-0000-0000-0000-000000000006', '40000000-0000-0000-0000-000000000002', 'role-b')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO capabilities (id, workspace_id, name) VALUES
         ('40000000-0000-0000-0000-000000000007', '40000000-0000-0000-0000-000000000001', 'capability-a'),
         ('40000000-0000-0000-0000-000000000008', '40000000-0000-0000-0000-000000000002', 'capability-b')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id) VALUES
         ('40000000-0000-0000-0000-000000000009', '40000000-0000-0000-0000-000000000001', '40000000-0000-0000-0000-000000000003', '40000000-0000-0000-0000-000000000005'),
         ('40000000-0000-0000-0000-000000000010', '40000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000004', '40000000-0000-0000-0000-000000000006')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id) VALUES
         ('40000000-0000-0000-0000-000000000011', '40000000-0000-0000-0000-000000000001', '40000000-0000-0000-0000-000000000005', '40000000-0000-0000-0000-000000000007'),
         ('40000000-0000-0000-0000-000000000012', '40000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000006', '40000000-0000-0000-0000-000000000008')",
    )
    .execute(pool)
    .await
    .unwrap();
}

pub fn assert_sqlstate(result: Result<PgQueryResult, sqlx::Error>, expected: &str) {
    let error = result.expect_err("statement unexpectedly succeeded");
    let database_error = error
        .as_database_error()
        .expect("statement failed without a PostgreSQL error");

    assert_eq!(database_error.code().as_deref(), Some(expected));
}

pub async fn seed_two_workspace_identities(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ('30000000-0000-0000-0000-000000000001', 'constraint-workspace-a'),
         ('30000000-0000-0000-0000-000000000002', 'constraint-workspace-b')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         (
             '30000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000001',
             'shared-principal'
         ),
         (
             '30000000-0000-0000-0000-000000000004',
             '30000000-0000-0000-0000-000000000002',
             'shared-principal'
         )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO roles (id, workspace_id, name) VALUES
         (
             '30000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000001',
             'shared-role'
         ),
         (
             '30000000-0000-0000-0000-000000000006',
             '30000000-0000-0000-0000-000000000002',
             'shared-role'
         )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO capabilities (id, workspace_id, name) VALUES
         (
             '30000000-0000-0000-0000-000000000007',
             '30000000-0000-0000-0000-000000000001',
             'shared-capability'
         ),
         (
             '30000000-0000-0000-0000-000000000008',
             '30000000-0000-0000-0000-000000000002',
             'shared-capability'
         )",
    )
    .execute(pool)
    .await
    .unwrap();
}

pub async fn seed_workspace_associations(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id) VALUES
         (
             '30000000-0000-0000-0000-000000000009',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000005'
         ),
         (
             '30000000-0000-0000-0000-000000000010',
             '30000000-0000-0000-0000-000000000002',
             '30000000-0000-0000-0000-000000000004',
             '30000000-0000-0000-0000-000000000006'
         )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id) VALUES
         (
             '30000000-0000-0000-0000-000000000011',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000007'
         ),
         (
             '30000000-0000-0000-0000-000000000012',
             '30000000-0000-0000-0000-000000000002',
             '30000000-0000-0000-0000-000000000006',
             '30000000-0000-0000-0000-000000000008'
         )",
    )
    .execute(pool)
    .await
    .unwrap();
}

pub async fn count(pool: &sqlx::PgPool, query: &str) -> i64 {
    sqlx::query_scalar(query).fetch_one(pool).await.unwrap()
}
