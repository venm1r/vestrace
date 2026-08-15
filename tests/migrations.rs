//! Migration behavior verified against a real PostgreSQL database.

mod support;

use support::{assert_sqlstate, count, seed_two_workspace_identities, seed_workspace_associations};

#[sqlx::test(migrations = "./migrations")]
async fn migrations_create_required_extensions(pool: sqlx::PgPool) {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pg_trgm') ORDER BY extname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(names, vec!["pg_trgm", "vector"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn migrations_create_required_identity_tables(pool: sqlx::PgPool) {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT table_name
         FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN (
               'workspaces',
               'principals',
               'roles',
               'capabilities',
               'principal_roles',
               'role_capabilities'
           )
         ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        names,
        vec![
            "capabilities",
            "principal_roles",
            "principals",
            "role_capabilities",
            "roles",
            "workspaces",
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn workspace_slug_uniqueness_is_case_insensitive(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ('00000000-0000-0000-0000-000000000001', 'Studio')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let duplicate = sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ('00000000-0000-0000-0000-000000000002', 'STUDIO')",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_identity_tables_reject_missing_workspace_ownership(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ('10000000-0000-0000-0000-000000000001', 'ownership')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES (
             '10000000-0000-0000-0000-000000000002',
             '10000000-0000-0000-0000-000000000001',
             'principal'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO roles (id, workspace_id, name)
         VALUES (
             '10000000-0000-0000-0000-000000000003',
             '10000000-0000-0000-0000-000000000001',
             'role'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO capabilities (id, workspace_id, name)
         VALUES (
             '10000000-0000-0000-0000-000000000004',
             '10000000-0000-0000-0000-000000000001',
             'capability'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let statements = [
        "INSERT INTO principals (id, identifier)
         VALUES ('10000000-0000-0000-0000-000000000010', 'unowned')",
        "INSERT INTO roles (id, name)
         VALUES ('10000000-0000-0000-0000-000000000011', 'unowned')",
        "INSERT INTO capabilities (id, name)
         VALUES ('10000000-0000-0000-0000-000000000012', 'unowned')",
        "INSERT INTO principal_roles (id, principal_id, role_id)
         VALUES (
             '10000000-0000-0000-0000-000000000013',
             '10000000-0000-0000-0000-000000000002',
             '10000000-0000-0000-0000-000000000003'
         )",
        "INSERT INTO role_capabilities (id, role_id, capability_id)
         VALUES (
             '10000000-0000-0000-0000-000000000014',
             '10000000-0000-0000-0000-000000000003',
             '10000000-0000-0000-0000-000000000004'
         )",
    ];

    for statement in statements {
        assert_sqlstate(sqlx::query(statement).execute(&pool).await, "23502");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn association_tables_reject_cross_workspace_links(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ('20000000-0000-0000-0000-000000000001', 'workspace-a'),
         ('20000000-0000-0000-0000-000000000002', 'workspace-b')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         (
             '20000000-0000-0000-0000-000000000003',
             '20000000-0000-0000-0000-000000000001',
             'principal-a'
         ),
         (
             '20000000-0000-0000-0000-000000000004',
             '20000000-0000-0000-0000-000000000002',
             'principal-b'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO roles (id, workspace_id, name) VALUES
         (
             '20000000-0000-0000-0000-000000000005',
             '20000000-0000-0000-0000-000000000001',
             'role-a'
         ),
         (
             '20000000-0000-0000-0000-000000000006',
             '20000000-0000-0000-0000-000000000002',
             'role-b'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO capabilities (id, workspace_id, name) VALUES
         (
             '20000000-0000-0000-0000-000000000007',
             '20000000-0000-0000-0000-000000000001',
             'capability-a'
         ),
         (
             '20000000-0000-0000-0000-000000000008',
             '20000000-0000-0000-0000-000000000002',
             'capability-b'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let principal_role = sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id)
         VALUES (
             '20000000-0000-0000-0000-000000000009',
             '20000000-0000-0000-0000-000000000001',
             '20000000-0000-0000-0000-000000000003',
             '20000000-0000-0000-0000-000000000006'
         )",
    )
    .execute(&pool)
    .await;
    assert_sqlstate(principal_role, "23503");

    let role_capability = sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id)
         VALUES (
             '20000000-0000-0000-0000-000000000010',
             '20000000-0000-0000-0000-000000000001',
             '20000000-0000-0000-0000-000000000005',
             '20000000-0000-0000-0000-000000000008'
         )",
    )
    .execute(&pool)
    .await;
    assert_sqlstate(role_capability, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_reject_cross_workspace_principal(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;

    let result = sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id)
         VALUES (
             '31000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000004',
             '30000000-0000-0000-0000-000000000005'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(result, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_reject_cross_workspace_role_for_principal(
    pool: sqlx::PgPool,
) {
    seed_two_workspace_identities(&pool).await;

    let result = sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id)
         VALUES (
             '31000000-0000-0000-0000-000000000002',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000006'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(result, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_reject_cross_workspace_role_for_capability(
    pool: sqlx::PgPool,
) {
    seed_two_workspace_identities(&pool).await;

    let result = sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id)
         VALUES (
             '31000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000006',
             '30000000-0000-0000-0000-000000000007'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(result, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_reject_cross_workspace_capability(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;

    let result = sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id)
         VALUES (
             '31000000-0000-0000-0000-000000000004',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000008'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(result, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_accept_same_workspace_principal_role(
    pool: sqlx::PgPool,
) {
    seed_two_workspace_identities(&pool).await;

    sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id)
         VALUES (
             '31000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000005'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn association_workspace_constraints_accept_same_workspace_role_capability(
    pool: sqlx::PgPool,
) {
    seed_two_workspace_identities(&pool).await;

    sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id)
         VALUES (
             '31000000-0000-0000-0000-000000000006',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000007'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascade_workspace_removes_only_its_tenant_identity_rows(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    sqlx::query(
        "DELETE FROM workspaces
         WHERE id = '30000000-0000-0000-0000-000000000001'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM workspaces
             WHERE id = '30000000-0000-0000-0000-000000000001'",
        )
        .await,
        0
    );
    for query in [
        "SELECT count(*) FROM principals WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        "SELECT count(*) FROM roles WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        "SELECT count(*) FROM capabilities WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        "SELECT count(*) FROM principal_roles WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        "SELECT count(*) FROM role_capabilities WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
    ] {
        assert_eq!(count(&pool, query).await, 0);
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM workspaces
             WHERE id = '30000000-0000-0000-0000-000000000002'",
        )
        .await,
        1
    );
    for query in [
        "SELECT count(*) FROM principals WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        "SELECT count(*) FROM roles WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        "SELECT count(*) FROM capabilities WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        "SELECT count(*) FROM principal_roles WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        "SELECT count(*) FROM role_capabilities WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
    ] {
        assert_eq!(count(&pool, query).await, 1);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascade_principal_removes_only_its_role_associations(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    sqlx::query(
        "DELETE FROM principals
         WHERE id = '30000000-0000-0000-0000-000000000003'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM principal_roles
             WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM principal_roles
             WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM roles
             WHERE id = '30000000-0000-0000-0000-000000000005'",
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascade_role_removes_only_its_principal_associations(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    sqlx::query(
        "DELETE FROM roles
         WHERE id = '30000000-0000-0000-0000-000000000005'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM principal_roles
             WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM principal_roles
             WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM principals
             WHERE id = '30000000-0000-0000-0000-000000000003'",
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascade_role_removes_only_its_capability_associations(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    sqlx::query(
        "DELETE FROM roles
         WHERE id = '30000000-0000-0000-0000-000000000005'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM role_capabilities
             WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM role_capabilities
             WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM capabilities
             WHERE id = '30000000-0000-0000-0000-000000000007'",
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascade_capability_removes_only_its_role_associations(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    sqlx::query(
        "DELETE FROM capabilities
         WHERE id = '30000000-0000-0000-0000-000000000007'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM role_capabilities
             WHERE workspace_id = '30000000-0000-0000-0000-000000000001'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM role_capabilities
             WHERE workspace_id = '30000000-0000-0000-0000-000000000002'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM roles
             WHERE id = '30000000-0000-0000-0000-000000000005'",
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_scoped_uniqueness_principal_identifiers(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES (
             '32000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000001',
             'shared-principal'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_scoped_uniqueness_role_names(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO roles (id, workspace_id, name)
         VALUES (
             '32000000-0000-0000-0000-000000000002',
             '30000000-0000-0000-0000-000000000001',
             'shared-role'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_scoped_uniqueness_capability_names(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO capabilities (id, workspace_id, name)
         VALUES (
             '32000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000001',
             'shared-capability'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_scoped_uniqueness_principal_role_tuples(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO principal_roles (id, workspace_id, principal_id, role_id)
         VALUES (
             '32000000-0000-0000-0000-000000000004',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000003',
             '30000000-0000-0000-0000-000000000005'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn tenant_scoped_uniqueness_role_capability_tuples(pool: sqlx::PgPool) {
    seed_two_workspace_identities(&pool).await;
    seed_workspace_associations(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO role_capabilities (id, workspace_id, role_id, capability_id)
         VALUES (
             '32000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000001',
             '30000000-0000-0000-0000-000000000005',
             '30000000-0000-0000-0000-000000000007'
         )",
    )
    .execute(&pool)
    .await;

    assert_sqlstate(duplicate, "23505");
}

#[sqlx::test(migrations = "./migrations")]
async fn cognitive_mutation_schema(pool: sqlx::PgPool) {
    let state_revision: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'conflicts'
           AND column_name = 'state_revision'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(state_revision, vec!["state_revision"]);

    let policies: Vec<String> = sqlx::query_scalar(
        "SELECT tablename
         FROM pg_policies
         WHERE schemaname = 'public'
           AND policyname LIKE '%workspace_isolation'
           AND tablename IN (
               'claims',
               'claim_evidence_links',
               'claim_assessments',
               'conflicts',
               'cognitive_mutations',
               'reconciliation_records'
           )
         ORDER BY tablename",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        policies,
        vec![
            "claim_assessments",
            "claim_evidence_links",
            "claims",
            "cognitive_mutations",
            "conflicts",
            "reconciliation_records",
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn migrations_expose_temporal_and_state_revision_columns(pool: sqlx::PgPool) {
    let event_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'events'
           AND column_name IN ('occurred_at', 'recorded_at')
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(event_columns, vec!["occurred_at", "recorded_at"]);

    let memory_state_revision: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'memories'
               AND column_name = 'state_revision'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(memory_state_revision);

    let revision_temporal_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'memory_revisions'
           AND column_name IN ('valid_from', 'valid_until', 'classification')
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        revision_temporal_columns,
        vec!["classification", "valid_from", "valid_until"]
    );

    let temporal_index: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_indexes
             WHERE schemaname = 'public'
               AND indexname = 'idx_memory_revisions_temporal_lookup'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(temporal_index);

    // Ordered in Rust rather than in SQL: the server collation decides where an
    // underscore sorts, so `ORDER BY column_name` is not portable across
    // databases. The assertion is about which columns exist, not their order.
    let mut evaluation_fact_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'evaluation_facts'
           AND column_name IN (
               'target', 'evaluator', 'metric', 'result', 'evidence_refs',
               'authority', 'policy_version'
           )",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    evaluation_fact_columns.sort();

    assert_eq!(
        evaluation_fact_columns,
        vec![
            "authority",
            "evaluator",
            "evidence_refs",
            "metric",
            "policy_version",
            "result",
            "target",
        ]
    );

    let evaluation_fact_index: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_indexes
             WHERE schemaname = 'public'
               AND indexname = 'idx_evaluation_facts_workspace_created'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(evaluation_fact_index);
}

#[sqlx::test(migrations = "./migrations")]
async fn learning_schema_keeps_projections_and_proposals_separate(pool: sqlx::PgPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name
         FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN ('evaluation_facts', 'learned_projections', 'learning_proposals')
         ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        tables,
        vec![
            "evaluation_facts",
            "learned_projections",
            "learning_proposals",
        ]
    );

    let projection_authority_constraint: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_constraint
             WHERE conname = 'learned_projections_advisory_only'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(projection_authority_constraint);

    let policies: Vec<String> = sqlx::query_scalar(
        "SELECT tablename
         FROM pg_policies
         WHERE schemaname = 'public'
           AND policyname LIKE '%workspace_isolation'
           AND tablename IN ('learned_projections', 'learning_proposals')
         ORDER BY tablename",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(policies, vec!["learned_projections", "learning_proposals"]);

    let proposal_index: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_indexes
             WHERE schemaname = 'public'
               AND indexname = 'idx_learning_proposals_workspace_created'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(proposal_index);

    let forced_rls: Vec<(String, bool)> = sqlx::query_as(
        "SELECT c.relname, c.relforcerowsecurity
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public'
           AND c.relname IN ('learned_projections', 'learning_proposals')
         ORDER BY c.relname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        forced_rls,
        vec![
            ("learned_projections".to_owned(), true),
            ("learning_proposals".to_owned(), true),
        ]
    );
}
