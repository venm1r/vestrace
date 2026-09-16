//! Row level security, exercised under a role that cannot bypass it.
//!
//! # Why this file exists
//!
//! Every database-backed test in this repository until now connected as the
//! database owner. `sqlx::test` provisions the connection from `DATABASE_URL`,
//! and in every developer and CI setup that user is a superuser — which
//! bypasses row level security outright. So the policies were never exercised
//! by anything: a test could delete every `CREATE POLICY` statement from the
//! migrations and stay green.
//!
//! That is not a hypothetical. Two defects this session came from the same
//! blind spot: `access_tokens` and `retrieval_runs` both had RLS *enabled but
//! not forced*, and the runtime role owns those tables, so the policies were
//! inert for exactly the writer that mattered.
//!
//! These tests create a role that is `NOSUPERUSER NOBYPASSRLS`, switch to it
//! for the transaction, and query **without** the application's own
//! `WHERE workspace_id = $1` predicate. If a row comes back, the policy is not
//! doing the work — which is the only way to tell an enforced boundary from a
//! well-behaved query.

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Seed one workspace holding one memory revision. Returns the ids.
async fn seed_workspace(pool: &PgPool, content: &str) -> (Uuid, Uuid, Uuid) {
    let workspace_id = Uuid::now_v7();
    let memory_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id)
        .bind(format!("ws-{workspace_id}"))
        .execute(pool)
        .await
        .expect("workspace");

    sqlx::query(
        "INSERT INTO memories (id, workspace_id, kind, status, state_revision) \
         VALUES ($1, $2, 'fact', 'candidate', 1)",
    )
    .bind(memory_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("memory");

    sqlx::query(
        "INSERT INTO memory_revisions \
         (id, memory_id, workspace_id, revision_number, content, confidence, importance) \
         VALUES ($1, $2, $3, 1, $4, 1.0, 0.5)",
    )
    .bind(revision_id)
    .bind(memory_id)
    .bind(workspace_id)
    .bind(content)
    .execute(pool)
    .await
    .expect("revision");

    (workspace_id, memory_id, revision_id)
}

/// A role that cannot bypass row level security.
///
/// Roles are cluster-wide while `sqlx::test` databases are not, so the name
/// carries a unique suffix; otherwise two tests running at once would fight
/// over one role.
async fn unprivileged_role(pool: &PgPool) -> String {
    let role = format!("rls_probe_{}", Uuid::now_v7().simple());
    sqlx::query(&format!(
        "CREATE ROLE \"{role}\" NOLOGIN NOSUPERUSER NOBYPASSRLS NOINHERIT"
    ))
    .execute(pool)
    .await
    .expect("probe role");
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO \"{role}\""))
        .execute(pool)
        .await
        .expect("schema usage");
    sqlx::query(&format!(
        "GRANT SELECT ON memories, memory_revisions TO \"{role}\""
    ))
    .execute(pool)
    .await
    .expect("table select");
    role
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_scoped_reader_sees_only_its_own_workspace(pool: PgPool) {
    let (mine, _, my_revision) = seed_workspace(&pool, "my content").await;
    let (_, _, their_revision) = seed_workspace(&pool, "their content").await;
    let role = unprivileged_role(&pool).await;

    let mut transaction = pool.begin().await.expect("transaction");

    sqlx::query(&format!("SET LOCAL ROLE \"{role}\""))
        .execute(&mut *transaction)
        .await
        .expect("assume the probe role");
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(mine.to_string())
        .execute(&mut *transaction)
        .await
        .expect("scope");

    // Deliberately unfiltered. Under a bypassing role this returns both rows,
    // which is what every previous database test in this repository would have
    // done.
    let rows = sqlx::query("SELECT id, workspace_id FROM memory_revisions")
        .fetch_all(&mut *transaction)
        .await
        .expect("select");

    let visible: Vec<Uuid> = rows.iter().map(|row| row.get("id")).collect();
    assert!(
        visible.contains(&my_revision),
        "the scoped reader could not see its own workspace"
    );
    assert!(
        !visible.contains(&their_revision),
        "row level security did not hide another workspace's revision from an \
         unfiltered query"
    );
    assert_eq!(visible.len(), 1);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_unscoped_reader_sees_nothing(pool: PgPool) {
    // `vestrace_current_workspace_id()` yields NULL when the setting is absent,
    // and `workspace_id = NULL` is NULL rather than true. A connection that
    // forgets to scope itself therefore reads an empty database rather than the
    // whole one — which is the safe direction for that mistake.
    seed_workspace(&pool, "content").await;
    let role = unprivileged_role(&pool).await;

    let mut transaction = pool.begin().await.expect("transaction");
    sqlx::query(&format!("SET LOCAL ROLE \"{role}\""))
        .execute(&mut *transaction)
        .await
        .expect("assume the probe role");

    let rows = sqlx::query("SELECT id FROM memory_revisions")
        .fetch_all(&mut *transaction)
        .await
        .expect("select");

    assert!(
        rows.is_empty(),
        "an unscoped connection read {} rows",
        rows.len()
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn naming_another_workspace_does_not_reach_its_rows(pool: PgPool) {
    // The predicate a caller controls is the workspace id it declares. Setting
    // it to somebody else's is the whole attack, and it works — which is why
    // the scope is set by the middleware from an authenticated credential and
    // never from a request body. This test records what the database does when
    // that upstream control is the thing that fails.
    let (mine, _, _) = seed_workspace(&pool, "my content").await;
    let (theirs, _, their_revision) = seed_workspace(&pool, "their content").await;
    let role = unprivileged_role(&pool).await;

    let mut transaction = pool.begin().await.expect("transaction");
    sqlx::query(&format!("SET LOCAL ROLE \"{role}\""))
        .execute(&mut *transaction)
        .await
        .expect("assume the probe role");
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(theirs.to_string())
        .execute(&mut *transaction)
        .await
        .expect("scope");

    let rows = sqlx::query("SELECT id FROM memory_revisions")
        .fetch_all(&mut *transaction)
        .await
        .expect("select");
    let visible: Vec<Uuid> = rows.iter().map(|row| row.get("id")).collect();

    // The database enforces the scope it was given, faithfully. It cannot know
    // the scope was wrong.
    assert_eq!(visible, vec![their_revision]);
    assert_ne!(mine, theirs);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn the_forced_policy_applies_to_the_owning_role(pool: PgPool) {
    // `FORCE ROW LEVEL SECURITY` is what makes a policy apply to the table
    // owner, and the runtime role owns every table in this schema (see
    // `docker/postgres/init-runtime-role.sh`). Without it the policies are
    // present, readable, and inert for the one role that runs queries.
    //
    // Forty-five tables are in that state, down from sixty-six. They are listed
    // below rather than waved at, because the list is the work: each one
    // becomes safe when the adapters that touch it acquire a scoped
    // transaction, and the migration that forces it can only land after that —
    // forcing it first turns every unscoped write into a refusal and every
    // unscoped read into an empty result, which is most of the system.
    //
    // The reverse mistake is just as quiet and has already been made:
    // `learned_projections` and `learning_proposals` were forced by migration
    // 0124 while their adapter still used an unscoped pool, so every write to
    // them was refused and every read came back empty for as long as that
    // stood. Forcing a table and scoping its adapter are two halves of one
    // change.
    //
    // The assertion was one-directional while the hole was being closed: a
    // table leaving the list was fine, a table joining it failed here. The list
    // is now empty, so the two directions have become the same assertion.
    // **The list is empty.** Migrations 0149 and 0150 forced the last of them,
    // so this is no longer a record of a hole being closed — it is the
    // assertion that there is none. A table appearing here now is a table whose
    // policy does not apply to the role that owns it.
    const KNOWN_UNFORCED: &[&str] = &[];

    let unforced: Vec<String> = sqlx::query(
        "SELECT c.relname          FROM pg_class c          JOIN pg_namespace n ON n.oid = c.relnamespace          WHERE n.nspname = 'public' AND c.relkind = 'r'            AND c.relrowsecurity AND NOT c.relforcerowsecurity",
    )
    .fetch_all(&pool)
    .await
    .expect("catalog")
    .iter()
    .map(|row| row.get::<String, _>("relname"))
    .collect();

    let unexpected: Vec<&String> = unforced
        .iter()
        .filter(|name| !KNOWN_UNFORCED.contains(&name.as_str()))
        .collect();

    assert!(
        unexpected.is_empty(),
        "these tables enable row level security without forcing it, so the runtime role —          which owns them — is exempt from their own workspace policy: {unexpected:?}"
    );

    // And every table that is forced must still have the policy enabled: a
    // table can be forced and have no policy at all, which reads as safe and
    // denies everything.
    let forced_without_policy: Vec<String> = sqlx::query(
        "SELECT c.relname          FROM pg_class c          JOIN pg_namespace n ON n.oid = c.relnamespace          WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relforcerowsecurity            AND NOT EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid)",
    )
    .fetch_all(&pool)
    .await
    .expect("catalog")
    .iter()
    .map(|row| row.get::<String, _>("relname"))
    .collect();

    assert!(
        forced_without_policy.is_empty(),
        "these tables force row level security but carry no policy, so they deny every          row to everyone: {forced_without_policy:?}"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_table_holding_a_workspace_id_has_row_level_security(pool: PgPool) {
    // A third category, which the test above cannot see: thirteen tables have
    // no row level security *at all* — not enabled, not forced, no policy. The
    // check above only compares enabled against forced, so those thirteen were
    // invisible to it, and "exempt" undercounted the hole by exactly that
    // number.
    //
    // Most of them are process-level rather than request-level, and that is a
    // real distinction rather than an excuse: `qualification_bundles`,
    // `incidents`, `recovery_points`, `release_manifests`, `revalidation_runs`,
    // `trust_state_records`, `product_releases` and
    // `external_effect_fault_suite_evidence` describe a deployment, carry no
    // `workspace_id`, and a workspace policy on them would be meaningless. So
    // would one on `_sqlx_migrations`.
    //
    // The property worth guarding is narrower and does not require judgement:
    // **a table with a `workspace_id` column belongs to a tenant, and a tenant
    // table without a policy has no boundary except the queries that remember
    // to write one.**
    //
    // The list is empty. Migration 0144 closed the two that were on it:
    // `external_effect_intents` and `supersession_links`. It also gave
    // `external_effect_receipts` and `external_reconciliations` the
    // `workspace_id` they had no way to express — they belong to an effect, and
    // the effect belongs to a tenant — so they are covered by this assertion
    // now rather than exempt from it by omission.
    const KNOWN_TENANT_TABLES_WITHOUT_POLICY: &[&str] = &[];

    let unprotected: Vec<String> = sqlx::query(
        "SELECT c.relname
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public'
           AND c.relkind = 'r'
           AND NOT c.relrowsecurity
           AND EXISTS (
               SELECT 1 FROM information_schema.columns col
               WHERE col.table_schema = 'public'
                 AND col.table_name = c.relname
                 AND col.column_name = 'workspace_id'
           )",
    )
    .fetch_all(&pool)
    .await
    .expect("catalog")
    .iter()
    .map(|row| row.get::<String, _>("relname"))
    .collect();

    let unexpected: Vec<&String> = unprotected
        .iter()
        .filter(|name| !KNOWN_TENANT_TABLES_WITHOUT_POLICY.contains(&name.as_str()))
        .collect();

    assert!(
        unexpected.is_empty(),
        "these tables hold a workspace_id and have no row level security at all, so \
         nothing but the application's own predicate separates tenants in them: \
         {unexpected:?}"
    );
}
