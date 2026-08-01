use sqlx::postgres::PgQueryResult;

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
