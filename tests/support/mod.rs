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
