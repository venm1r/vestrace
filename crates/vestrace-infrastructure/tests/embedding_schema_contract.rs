//! The P04 guarded schema, and one hole P03 left open.
//!
//! P03 closed the "hard-coded table list" hole with a closed-world check that
//! derives its set from `pg_class`; that test picked up this package's three new
//! tables with no edit at all. There is no equivalent for **functions**:
//! `provider_schema_contract::p03_guarded_function_runtime_execute_set_is_exact`
//! filters its query through the declared `P03_GUARDED_FUNCTIONS` list, so a
//! guarded function the runtime can execute and no list declares is invisible to
//! every existing test. It passed against this package's two new functions for
//! that reason, which is not the same as having checked them.

use std::collections::BTreeSet;
use std::str::FromStr;

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_domain::embedding::{
    BarrierState, CarryHeaderState, CarryMappingState, EmbeddingJobKind, EmbeddingJobState,
};

const NEW_TABLES: [&str; 11] = [
    "embedding_corpus_generations",
    "embedding_jobs",
    "embedding_space_registrations",
    "embedding_transition_plan_recipes",
    "embedding_transition_plans",
    "embedding_transitions",
    "embedding_transition_ambiguity_carries",
    "embedding_transition_ambiguity_carry_recipes",
    "embedding_transition_barriers",
    "embedding_transition_barrier_recipes",
    "model_binding_snapshot_scopes",
];

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
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
        .max_connections(1)
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

async fn scope(
    pool: &PgPool,
    workspace: Uuid,
    principal: Uuid,
) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    for (setting, value) in [
        ("vestrace.workspace_id", workspace),
        ("vestrace.principal_id", principal),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(value.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    transaction
}

async fn workspace(pool: &PgPool) -> (Uuid, Uuid) {
    let workspace = Uuid::now_v7();
    let principal = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace)
        .bind(format!("embedding-schema-{workspace}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal)
        .bind(workspace)
        .bind(format!("embedding-schema-principal-{principal}"))
        .execute(pool)
        .await
        .unwrap();
    (workspace, principal)
}

/// Every function the guarded owner owns and the runtime role may execute must
/// be declared somewhere a human maintains.
///
/// This is the function-shaped twin of the closed-world table check. It derives
/// its set from `pg_proc` rather than from a list, so a guarded entry point
/// added by a later migration and forgotten in
/// `docker/postgres/init-runtime-role.sh` or in the schema-contract lists is a
/// red test rather than a silent execute grant.
///
/// The declared set is read from the bootstrap script itself, because that file
/// is the thing a fresh deployment actually runs. Reading it here means the test
/// cannot drift from the deployment the way a second hand-written list would.
#[sqlx::test(migrations = "../../migrations")]
async fn every_runtime_executable_guarded_function_is_declared_in_the_bootstrap(pool: PgPool) {
    let executable: BTreeSet<String> = sqlx::query_scalar(
        "
        SELECT procedure.proname
          FROM pg_proc AS procedure
          JOIN pg_namespace AS namespace ON namespace.oid = procedure.pronamespace
         WHERE namespace.nspname = 'public'
           AND pg_get_userbyid(procedure.proowner) = 'vestrace_guarded_owner'
           AND has_function_privilege('vestrace', procedure.oid, 'EXECUTE')
         ORDER BY procedure.proname
        ",
    )
    .fetch_all(&pool)
    .await
    .expect("the guarded owner's runtime-executable function set must be readable")
    .into_iter()
    .collect();

    assert!(
        executable.len() >= 20,
        "expected at least the twenty declared P03 entrypoints, found {}",
        executable.len()
    );

    let bootstrap = std::fs::read_to_string("../../docker/postgres/init-runtime-role.sh")
        .expect("the bootstrap script is the set a fresh deployment actually provisions");

    // Both arrays, separately. Appearing in the script somewhere is not enough:
    // P03's deployment-blocking defect was a function present in one array and
    // absent from the other, and a whole-file search would pass straight through
    // it. Found by breaking exactly that way — the first version of this test
    // stayed green while the real provisioner failed on the same tree.
    // Every region of each kind, not the first. The script declares two pairs of
    // these arrays — an earlier bridge and the P03 one — and a function may
    // lawfully be declared by either. The first version of this test searched
    // only the first region it found, which was the wrong function's, and
    // reported every entrypoint as missing. A crude parser that silently matches
    // the wrong thing is worse than none.
    let ownership = array_regions(&bootstrap, "allowed_targets REGPROCEDURE[]");
    let runtime_executable = array_regions(&bootstrap, "runtime_executable_targets REGPROCEDURE[]");
    assert!(
        ownership.len() >= 2 && runtime_executable.len() >= 2,
        "expected both bridges to declare both array kinds, found {} ownership and {} executable",
        ownership.len(),
        runtime_executable.len()
    );

    let mut missing: Vec<String> = Vec::new();
    for name in &executable {
        let needle = format!("public.{name}(");
        if !ownership
            .iter()
            .any(|region| region.contains(needle.as_str()))
        {
            missing.push(format!("{name}: absent from every ownership allowlist"));
        }
        if !runtime_executable
            .iter()
            .any(|region| region.contains(needle.as_str()))
        {
            missing.push(format!(
                "{name}: absent from every runtime-executable allowlist"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "these functions are owned by the guarded owner and executable by the runtime role,          but a fresh deployment would not provision them that way: {missing:#?}"
    );
}

/// Reads every `REGPROCEDURE[]` array literal of one kind out of the bootstrap
/// script.
///
/// Deliberately crude: it takes the text between the array's declaration and the
/// first closing `];`. A parser would be the wrong tool, because what this test
/// needs to know is what the shell script literally ships, not what a
/// well-formed version of it would mean.
fn array_regions<'a>(bootstrap: &'a str, declaration: &str) -> Vec<&'a str> {
    let mut regions = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = bootstrap[cursor..].find(declaration) {
        let start = cursor + offset;
        let rest = &bootstrap[start..];
        let end = rest
            .find("];")
            .unwrap_or_else(|| panic!("{declaration} must be a closed array literal"));
        regions.push(&rest[..end]);
        cursor = start + end;
    }
    assert!(
        !regions.is_empty(),
        "the bootstrap script must declare {declaration}"
    );
    regions
}

/// The three new tables carry the same guard every P02 and P03 table carries.
#[sqlx::test(migrations = "../../migrations")]
async fn the_new_tables_are_owned_forced_and_acl_bearing(pool: PgPool) {
    for table in NEW_TABLES {
        let row = sqlx::query(
            "SELECT pg_get_userbyid(class.relowner) AS owner, class.relrowsecurity, \
                    class.relforcerowsecurity, class.relacl IS NOT NULL AS has_acl \
               FROM pg_class AS class \
               JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace \
              WHERE namespace.nspname = 'public' AND class.relname = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{table} must exist after migration 0187: {error}"));
        assert_eq!(
            row.get::<String, _>("owner"),
            "vestrace_guarded_owner",
            "{table}"
        );
        assert!(
            row.get::<bool, _>("relrowsecurity"),
            "{table} must enable RLS"
        );
        assert!(
            row.get::<bool, _>("relforcerowsecurity"),
            "{table} must force RLS"
        );
        assert!(
            row.get::<bool, _>("has_acl"),
            "{table} must retain an explicit ACL"
        );
    }
}

/// The database CHECK and the Rust enum must name the same six states.
///
/// They are written in two languages in two files, and nothing but this test
/// keeps them in step. A state added to one and not the other would be found by
/// a production insert instead.
#[sqlx::test(migrations = "../../migrations")]
async fn the_job_state_check_matches_the_declared_enum(pool: PgPool) {
    let definition: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(constraint_.oid) \
           FROM pg_constraint AS constraint_ \
           JOIN pg_class AS class ON class.oid = constraint_.conrelid \
          WHERE class.relname = 'embedding_jobs' AND constraint_.contype = 'c' \
            AND pg_get_constraintdef(constraint_.oid) LIKE '%state%'",
    )
    .fetch_one(&pool)
    .await
    .expect("embedding_jobs must carry a state CHECK constraint");

    for state in EmbeddingJobState::ALL {
        assert!(
            definition.contains(state.as_str()),
            "the CHECK constraint omits the declared state {}: {definition}",
            state.as_str()
        );
    }
    // And nothing beyond them: a literal in the constraint that no enum variant
    // names would be a state Rust could never produce or match.
    let quoted = definition.matches('\'').count() / 2;
    assert_eq!(
        quoted,
        EmbeddingJobState::ALL.len(),
        "the CHECK constraint names {quoted} literals for {} declared states: {definition}",
        EmbeddingJobState::ALL.len()
    );
}

async fn check_definition(pool: &PgPool, table: &str, marker: &str) -> String {
    sqlx::query_scalar(
        "SELECT pg_get_constraintdef(constraint_.oid) \
           FROM pg_constraint AS constraint_ \
           JOIN pg_class AS class ON class.oid = constraint_.conrelid \
          WHERE class.relname = $1 AND constraint_.contype = 'c' \
            AND pg_get_constraintdef(constraint_.oid) LIKE $2",
    )
    .bind(table)
    .bind(format!("%{marker}%"))
    .fetch_one(pool)
    .await
    .unwrap_or_else(|error| panic!("{table} must carry its {marker} CHECK constraint: {error}"))
}

fn assert_closed_check<T: Copy>(
    definition: &str,
    states: &[T],
    as_str: impl Fn(T) -> &'static str,
) {
    for state in states {
        assert!(
            definition.contains(as_str(*state)),
            "the CHECK constraint omits declared state {}: {definition}",
            as_str(*state)
        );
    }
    let quoted = definition.matches('\'').count() / 2;
    assert_eq!(
        quoted,
        states.len(),
        "the CHECK constraint names {quoted} literals for {} declared states: {definition}",
        states.len()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_barrier_state_check_matches_the_declared_enum(pool: PgPool) {
    let definition = check_definition(&pool, "embedding_transition_barriers", "state").await;
    assert_closed_check(&definition, &BarrierState::ALL, BarrierState::as_str);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_carry_header_state_check_matches_the_declared_enum(pool: PgPool) {
    let definition =
        check_definition(&pool, "embedding_transition_ambiguity_carries", "state").await;
    assert_closed_check(
        &definition,
        &CarryHeaderState::ALL,
        CarryHeaderState::as_str,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_carry_mapping_state_check_matches_the_declared_enum(pool: PgPool) {
    let definition = check_definition(
        &pool,
        "embedding_transition_ambiguity_carry_recipes",
        "state",
    )
    .await;
    assert_closed_check(
        &definition,
        &CarryMappingState::ALL,
        CarryMappingState::as_str,
    );
}

/// The kind CHECK constraint carries the three kinds line 219 closes over, and
/// only those three.
///
/// This test exists because the first draft of both the enum and the constraint
/// carried two. The later sentence "For `delivery` or `rebuild`, the validated
/// production response must match the job's exact EmbeddingSpaceKey" was read as
/// the closed set; it is not, it names the kinds whose response becomes a
/// vector. `retrieval_query` is the kind retrieval actually issues, and it was
/// unrepresentable in both the type and the column.
#[sqlx::test(migrations = "../../migrations")]
async fn the_job_kind_check_matches_the_declared_enum(pool: PgPool) {
    let definition: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(constraint_.oid) \
           FROM pg_constraint AS constraint_ \
           JOIN pg_class AS class ON class.oid = constraint_.conrelid \
          WHERE class.relname = 'embedding_jobs' AND constraint_.contype = 'c' \
            AND pg_get_constraintdef(constraint_.oid) LIKE '%kind%'",
    )
    .fetch_one(&pool)
    .await
    .expect("embedding_jobs must carry a kind CHECK constraint");

    for kind in EmbeddingJobKind::ALL {
        assert!(
            definition.contains(kind.as_str()),
            "the CHECK constraint omits the declared kind {}: {definition}",
            kind.as_str()
        );
    }
    let quoted = definition.matches('\'').count() / 2;
    assert_eq!(
        quoted,
        EmbeddingJobKind::ALL.len(),
        "the CHECK constraint names {quoted} literals for {} declared kinds: {definition}",
        EmbeddingJobKind::ALL.len()
    );
}

/// Spec line 219: the finalizer marks any current Ready generation stale as it
/// publishes the next. The schema, not the finalizer, is what makes two Ready
/// generations impossible.
#[sqlx::test(migrations = "../../migrations")]
async fn a_space_has_at_most_one_ready_generation(pool: PgPool) {
    let (workspace_id, principal_id) = workspace(&pool).await;
    let registration = Uuid::now_v7();

    let mut transaction = scope(&pool, workspace_id, principal_id).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'nomic-768','nomic-embed-text',768)",
    )
    .bind(registration)
    .bind(workspace_id)
    .bind(Uuid::now_v7())
    .fetch_one(&mut *transaction)
    .await
    .expect("a complete space key is registrable");

    let first = sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,7)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(registration)
    .fetch_one(&mut *transaction)
    .await
    .expect("the first generation publishes");
    assert_eq!(first, 1);

    let second = sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,9)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(registration)
    .fetch_one(&mut *transaction)
    .await
    .expect("the second generation publishes and supersedes the first");
    assert_eq!(second, 2);
    transaction.commit().await.unwrap();

    let states: Vec<(i64, String)> = sqlx::query_as(
        "SELECT ordinal, state FROM embedding_corpus_generations \
          WHERE workspace_id = $1 AND space_registration_id = $2 ORDER BY ordinal",
    )
    .bind(workspace_id)
    .bind(registration)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        states,
        vec![(1, "stale".to_owned()), (2, "ready".to_owned())],
        "publishing the second generation must supersede the first in the same transaction"
    );
}

/// The space key is the whole tuple in the database too, not only in Rust.
#[sqlx::test(migrations = "../../migrations")]
async fn registration_is_idempotent_on_the_tuple_and_refuses_a_second_space(pool: PgPool) {
    let (workspace_id, principal_id) = workspace(&pool).await;
    let registration = Uuid::now_v7();
    let space = Uuid::now_v7();

    let mut transaction = scope(&pool, workspace_id, principal_id).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let first = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'space','model',768)",
    )
    .bind(registration)
    .bind(workspace_id)
    .bind(space)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    // The same tuple again converges rather than conflicting: a caller retrying
    // after a crash must reach one identity, not a duplicate or an error.
    let again = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'space','model',768)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(space)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    assert_eq!(
        first, again,
        "the same key must converge on one registration"
    );

    // The same tuple naming a different space is a conflict, not a second row.
    let crossed = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'space','model',768)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(Uuid::now_v7())
    .fetch_one(&mut *transaction)
    .await
    .expect_err("one key may not name two spaces");
    let database = crossed
        .as_database_error()
        .expect("a crossed registration must be a database refusal");
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(
        database.message(),
        "embedding space key already names a different space"
    );
}

/// Spec line 251: a predecessor has at most one direct successor.
#[sqlx::test(migrations = "../../migrations")]
async fn the_runtime_cannot_write_the_new_tables_directly(pool: PgPool) {
    let (workspace_id, _) = workspace(&pool).await;
    let runtime = runtime_pool(&pool).await;
    for table in NEW_TABLES {
        let refusal = sqlx::query(&format!("INSERT INTO public.{table} DEFAULT VALUES"))
            .execute(&runtime)
            .await
            .expect_err("the runtime role must not write a guarded embedding table");
        let database = refusal
            .as_database_error()
            .expect("a direct write must return a database error");
        assert_eq!(database.code().as_deref(), Some("42501"), "{table}");
        // The message, not only the code. P03 established by breaking one that
        // the table ACL and the immutability trigger raise the same SQLSTATE, so
        // a code-only assertion passes straight through a removed ACL.
        assert_eq!(
            database.message(),
            format!("permission denied for table {table}"),
            "{table} must be refused by the ACL rather than by a trigger behind it"
        );
    }
    let _ = workspace_id;
    runtime.close().await;
}
