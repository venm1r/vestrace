//! PostgreSQL row-level workspace isolation verified under a restricted role.

mod support;

use support::{
    TENANT_TABLES, WORKSPACE_A, WORKSPACE_B, assert_sqlstate, assume_restricted_workspace,
    execute_as_restricted_workspace, execute_many_as_restricted_workspace, seed_rls_tenants,
    with_restricted_role,
};

#[sqlx::test(migrations = "./migrations")]
async fn principal_from_workspace_a_is_invisible_in_workspace_b(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES ($1::uuid, 'rls-workspace-a'), ($2::uuid, 'rls-workspace-b')",
    )
    .bind(WORKSPACE_A)
    .bind(WORKSPACE_B)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ('40000000-0000-0000-0000-000000000003', $1::uuid, 'principal-a')",
    )
    .bind(WORKSPACE_A)
    .execute(&pool)
    .await
    .unwrap();

    let visible_count: i64 = with_restricted_role(&pool, |pool, role| async move {
        let mut transaction = pool.begin().await?;
        assume_restricted_workspace(&mut transaction, &role, Some(WORKSPACE_B)).await?;
        let result = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM principals")
            .fetch_one(&mut *transaction)
            .await;
        transaction.rollback().await?;
        result
    })
    .await
    .unwrap();

    assert_eq!(visible_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn restricted_role_is_dropped_when_a_test_body_panics(pool: sqlx::PgPool) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let supervised_pool = pool.clone();
    let outcome = tokio::spawn(async move {
        with_restricted_role(&supervised_pool, move |_pool, role| async move {
            sender.send(role.name().to_owned()).unwrap();
            panic!("intentional cleanup-path panic");
        })
        .await
    })
    .await;
    let role_name = receiver.recv().unwrap();
    let role_still_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(role_name)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(outcome.unwrap_err().is_panic());
    assert!(!role_still_exists);
}

#[sqlx::test(migrations = "./migrations")]
async fn context_helpers_return_scoped_ids_and_treat_empty_values_as_missing(pool: sqlx::PgPool) {
    let (scoped, empty) = with_restricted_role(&pool, |pool, role| async move {
        let mut scoped_transaction = pool.begin().await?;
        assume_restricted_workspace(&mut scoped_transaction, &role, Some(WORKSPACE_A)).await?;
        sqlx::query("SELECT set_config('vestrace.principal_id', $1, true)")
            .bind("40000000-0000-0000-0000-000000000003")
            .execute(&mut *scoped_transaction)
            .await?;
        let scoped = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT vestrace_current_workspace_id()::text, vestrace_current_principal_id()::text",
        )
        .fetch_one(&mut *scoped_transaction)
        .await?;
        scoped_transaction.rollback().await?;

        let mut empty_transaction = pool.begin().await?;
        assume_restricted_workspace(&mut empty_transaction, &role, Some("")).await?;
        sqlx::query("SELECT set_config('vestrace.principal_id', '', true)")
            .execute(&mut *empty_transaction)
            .await?;
        let empty = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT vestrace_current_workspace_id()::text, vestrace_current_principal_id()::text",
        )
        .fetch_one(&mut *empty_transaction)
        .await?;
        empty_transaction.rollback().await?;
        Ok::<_, sqlx::Error>((scoped, empty))
    })
    .await
    .unwrap();

    assert_eq!(
        scoped,
        (
            Some(WORKSPACE_A.to_owned()),
            Some("40000000-0000-0000-0000-000000000003".to_owned())
        )
    );
    assert_eq!(empty, (None, None));
}

async fn visible_identities(
    pool: &sqlx::PgPool,
    role: &support::RestrictedRole,
    workspace_id: Option<&str>,
) -> Result<Vec<Vec<String>>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    assume_restricted_workspace(&mut transaction, role, workspace_id).await?;
    let mut identities = Vec::new();
    for table in TENANT_TABLES {
        let expression = if table == "workspaces" {
            "id::text".to_owned()
        } else {
            "id::text || '@' || workspace_id::text".to_owned()
        };
        identities.push(
            sqlx::query_scalar::<_, String>(&format!(
                "SELECT {expression} FROM {table} ORDER BY id"
            ))
            .fetch_all(&mut *transaction)
            .await?,
        );
    }
    transaction.rollback().await?;
    Ok(identities)
}

const WORKSPACE_A_IDENTITIES: [&str; 6] = [
    "40000000-0000-0000-0000-000000000001",
    "40000000-0000-0000-0000-000000000003@40000000-0000-0000-0000-000000000001",
    "40000000-0000-0000-0000-000000000005@40000000-0000-0000-0000-000000000001",
    "40000000-0000-0000-0000-000000000007@40000000-0000-0000-0000-000000000001",
    "40000000-0000-0000-0000-000000000009@40000000-0000-0000-0000-000000000001",
    "40000000-0000-0000-0000-000000000011@40000000-0000-0000-0000-000000000001",
];

const WORKSPACE_B_IDENTITIES: [&str; 6] = [
    "40000000-0000-0000-0000-000000000002",
    "40000000-0000-0000-0000-000000000004@40000000-0000-0000-0000-000000000002",
    "40000000-0000-0000-0000-000000000006@40000000-0000-0000-0000-000000000002",
    "40000000-0000-0000-0000-000000000008@40000000-0000-0000-0000-000000000002",
    "40000000-0000-0000-0000-000000000010@40000000-0000-0000-0000-000000000002",
    "40000000-0000-0000-0000-000000000012@40000000-0000-0000-0000-000000000002",
];

fn singleton_identities(expected: [&str; 6]) -> Vec<Vec<String>> {
    expected
        .into_iter()
        .map(|identity| vec![identity.to_owned()])
        .collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn every_identity_table_exposes_only_the_current_workspace(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let (workspace_a, workspace_b) = with_restricted_role(&pool, |pool, role| async move {
        let workspace_a = visible_identities(&pool, &role, Some(WORKSPACE_A)).await?;
        let workspace_b = visible_identities(&pool, &role, Some(WORKSPACE_B)).await?;
        Ok::<_, sqlx::Error>((workspace_a, workspace_b))
    })
    .await
    .unwrap();

    assert_eq!(workspace_a, singleton_identities(WORKSPACE_A_IDENTITIES));
    assert_eq!(workspace_b, singleton_identities(WORKSPACE_B_IDENTITIES));
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_or_empty_workspace_context_hides_every_identity_table(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let (missing, empty) = with_restricted_role(&pool, |pool, role| async move {
        let missing = visible_identities(&pool, &role, None).await?;
        let empty = visible_identities(&pool, &role, Some("")).await?;
        Ok::<_, sqlx::Error>((missing, empty))
    })
    .await
    .unwrap();

    assert_eq!(missing, vec![Vec::<String>::new(); TENANT_TABLES.len()]);
    assert_eq!(empty, vec![Vec::<String>::new(); TENANT_TABLES.len()]);
}

const CROSS_WORKSPACE_INSERTS: [&str; 6] = [
    "INSERT INTO workspaces (id, slug) VALUES ('40000000-0000-0000-0000-000000000020', 'cross-insert')",
    "INSERT INTO principals (id, workspace_id, identifier) VALUES ('40000000-0000-0000-0000-000000000021', '40000000-0000-0000-0000-000000000002', 'cross-insert')",
    "INSERT INTO roles (id, workspace_id, name) VALUES ('40000000-0000-0000-0000-000000000022', '40000000-0000-0000-0000-000000000002', 'cross-insert')",
    "INSERT INTO capabilities (id, workspace_id, name) VALUES ('40000000-0000-0000-0000-000000000023', '40000000-0000-0000-0000-000000000002', 'cross-insert')",
    "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id) VALUES ('40000000-0000-0000-0000-000000000024', '40000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000004', '40000000-0000-0000-0000-000000000006')",
    "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id) VALUES ('40000000-0000-0000-0000-000000000025', '40000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000006', '40000000-0000-0000-0000-000000000008')",
];

#[sqlx::test(migrations = "./migrations")]
async fn every_identity_table_rejects_cross_workspace_inserts(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let results = with_restricted_role(&pool, |pool, role| async move {
        let mut results = Vec::new();
        for statement in CROSS_WORKSPACE_INSERTS {
            results.push(
                execute_as_restricted_workspace(&pool, &role, Some(WORKSPACE_A), statement).await,
            );
        }
        results
    })
    .await;

    for result in results {
        assert_sqlstate(result, "42501");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_and_empty_context_reject_tenant_inserts(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let results = with_restricted_role(&pool, |pool, role| async move {
        let mut results = Vec::new();
        for workspace_id in [None, Some("")] {
            for statement in CROSS_WORKSPACE_INSERTS {
                results.push(
                    execute_as_restricted_workspace(&pool, &role, workspace_id, statement).await,
                );
            }
        }
        results
    })
    .await;

    for result in results {
        assert_sqlstate(result, "42501");
    }
}

const CROSS_WORKSPACE_UPDATES: [&str; 6] = [
    "UPDATE workspaces SET id = '40000000-0000-0000-0000-000000000030' WHERE id = '40000000-0000-0000-0000-000000000001'",
    "UPDATE principals SET workspace_id = '40000000-0000-0000-0000-000000000002' WHERE id = '40000000-0000-0000-0000-000000000003'",
    "UPDATE roles SET workspace_id = '40000000-0000-0000-0000-000000000002' WHERE id = '40000000-0000-0000-0000-000000000005'",
    "UPDATE capabilities SET workspace_id = '40000000-0000-0000-0000-000000000002' WHERE id = '40000000-0000-0000-0000-000000000007'",
    "UPDATE principal_roles SET workspace_id = '40000000-0000-0000-0000-000000000002' WHERE id = '40000000-0000-0000-0000-000000000009'",
    "UPDATE role_capabilities SET workspace_id = '40000000-0000-0000-0000-000000000002' WHERE id = '40000000-0000-0000-0000-000000000011'",
];

#[sqlx::test(migrations = "./migrations")]
async fn every_identity_table_rejects_cross_workspace_updates(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let results = with_restricted_role(&pool, |pool, role| async move {
        let mut results = Vec::new();
        for statement in CROSS_WORKSPACE_UPDATES {
            results.push(
                execute_as_restricted_workspace(&pool, &role, Some(WORKSPACE_A), statement).await,
            );
        }
        results
    })
    .await;

    for result in results {
        assert_sqlstate(result, "42501");
    }
}

const SAME_WORKSPACE_INSERTS: [&str; 6] = [
    "INSERT INTO workspaces (id, slug) VALUES ('40000000-0000-0000-0000-000000000040', 'same-workspace')",
    "INSERT INTO principals (id, workspace_id, identifier) VALUES ('40000000-0000-0000-0000-000000000041', '40000000-0000-0000-0000-000000000040', 'same-workspace')",
    "INSERT INTO roles (id, workspace_id, name) VALUES ('40000000-0000-0000-0000-000000000042', '40000000-0000-0000-0000-000000000040', 'same-workspace')",
    "INSERT INTO capabilities (id, workspace_id, name) VALUES ('40000000-0000-0000-0000-000000000043', '40000000-0000-0000-0000-000000000040', 'same-workspace')",
    "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id) VALUES ('40000000-0000-0000-0000-000000000044', '40000000-0000-0000-0000-000000000040', '40000000-0000-0000-0000-000000000041', '40000000-0000-0000-0000-000000000042')",
    "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id) VALUES ('40000000-0000-0000-0000-000000000045', '40000000-0000-0000-0000-000000000040', '40000000-0000-0000-0000-000000000042', '40000000-0000-0000-0000-000000000043')",
];

const SAME_WORKSPACE_UPDATES: [&str; 6] = [
    "UPDATE workspaces SET slug = 'same-workspace-updated' WHERE id = '40000000-0000-0000-0000-000000000040'",
    "UPDATE principals SET identifier = 'same-workspace-updated' WHERE id = '40000000-0000-0000-0000-000000000041'",
    "UPDATE roles SET name = 'same-workspace-updated' WHERE id = '40000000-0000-0000-0000-000000000042'",
    "UPDATE capabilities SET name = 'same-workspace-updated' WHERE id = '40000000-0000-0000-0000-000000000043'",
    "UPDATE principal_roles SET created_at = created_at + interval '1 second' WHERE id = '40000000-0000-0000-0000-000000000044'",
    "UPDATE role_capabilities SET created_at = created_at + interval '1 second' WHERE id = '40000000-0000-0000-0000-000000000045'",
];

#[sqlx::test(migrations = "./migrations")]
async fn every_identity_table_accepts_same_workspace_inserts_and_updates(pool: sqlx::PgPool) {
    let (inserted, updated) = with_restricted_role(&pool, |pool, role| async move {
        let inserted = execute_many_as_restricted_workspace(
            &pool,
            &role,
            Some("40000000-0000-0000-0000-000000000040"),
            &SAME_WORKSPACE_INSERTS,
        )
        .await?;
        let updated = execute_many_as_restricted_workspace(
            &pool,
            &role,
            Some("40000000-0000-0000-0000-000000000040"),
            &SAME_WORKSPACE_UPDATES,
        )
        .await?;
        Ok::<_, sqlx::Error>((inserted, updated))
    })
    .await
    .unwrap();

    assert_eq!(inserted, vec![1; TENANT_TABLES.len()]);
    assert_eq!(updated, vec![1; TENANT_TABLES.len()]);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_and_empty_context_hide_rows_from_updates(pool: sqlx::PgPool) {
    seed_rls_tenants(&pool).await;
    let results = with_restricted_role(&pool, |pool, role| async move {
        let mut results = Vec::new();
        for workspace_id in [None, Some("")] {
            for statement in CROSS_WORKSPACE_UPDATES {
                results.push(
                    execute_as_restricted_workspace(&pool, &role, workspace_id, statement).await,
                );
            }
        }
        results
    })
    .await;

    for result in results {
        assert_eq!(result.unwrap().rows_affected(), 0);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn all_identity_tables_force_rls_and_have_one_workspace_policy(pool: sqlx::PgPool) {
    let relations: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT relname, relrowsecurity, relforcerowsecurity
         FROM pg_class
         WHERE relname = ANY($1::text[])
         ORDER BY relname",
    )
    .bind(TENANT_TABLES)
    .fetch_all(&pool)
    .await
    .unwrap();
    let policies: Vec<(String, String)> = sqlx::query_as(
        "SELECT tablename, policyname
         FROM pg_policies
         WHERE schemaname = 'public' AND tablename = ANY($1::text[])
         ORDER BY tablename",
    )
    .bind(TENANT_TABLES)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        relations,
        vec![
            ("capabilities".to_owned(), true, true),
            ("principal_roles".to_owned(), true, true),
            ("principals".to_owned(), true, true),
            ("role_capabilities".to_owned(), true, true),
            ("roles".to_owned(), true, true),
            ("workspaces".to_owned(), true, true),
        ]
    );
    assert_eq!(
        policies,
        vec![
            (
                "capabilities".to_owned(),
                "capabilities_workspace_isolation".to_owned()
            ),
            (
                "principal_roles".to_owned(),
                "principal_roles_workspace_isolation".to_owned()
            ),
            (
                "principals".to_owned(),
                "principals_workspace_isolation".to_owned()
            ),
            (
                "role_capabilities".to_owned(),
                "role_capabilities_workspace_isolation".to_owned()
            ),
            ("roles".to_owned(), "roles_workspace_isolation".to_owned()),
            (
                "workspaces".to_owned(),
                "workspaces_workspace_isolation".to_owned()
            ),
        ]
    );
}
