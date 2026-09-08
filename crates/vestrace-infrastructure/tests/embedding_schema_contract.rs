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

use std::{collections::BTreeSet, fs, str::FromStr, time::Duration};

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tempfile::TempDir;
use uuid::Uuid;
use vestrace_application::{MaterialKeyVault, VaultError};
use vestrace_domain::embedding::{
    BarrierState, CarryHeaderState, CarryMappingState, EmbeddingJobKind, EmbeddingJobState,
};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{IntentNonce, MaterialKeyId};
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};

mod common;

const VAULT_BOOTSTRAP_KEY_ID: &str = "embedding-schema-material-vault-bootstrap";
const VAULT_BOOTSTRAP_SCOPE: &str = "embedding-schema-material-vault-bootstrap";
const VAULT_BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

/// Keeps an actual host vault and its mounted bootstrap authority alive for the
/// output-abandonment fixture. The vault has no PostgreSQL dependency.
struct VaultFixture {
    bootstrap_root: TempDir,
    vault_root: TempDir,
    bootstrap_reference: KeyReference,
    bootstrap_request: SecretResolutionRequest,
}

impl VaultFixture {
    fn new() -> Self {
        let bootstrap_root = TempDir::new().expect("bootstrap mount");
        let bootstrap_key = bootstrap_root.path().join(VAULT_BOOTSTRAP_KEY_ID);
        let bootstrap_version = bootstrap_key.join("v1");
        fs::create_dir_all(&bootstrap_version).expect("bootstrap key directory");
        fs::write(bootstrap_key.join("scope"), VAULT_BOOTSTRAP_SCOPE).expect("bootstrap scope");
        fs::write(bootstrap_key.join("purpose"), "storage").expect("bootstrap purpose");
        fs::write(bootstrap_key.join("algorithm"), VAULT_BOOTSTRAP_ALGORITHM)
            .expect("bootstrap algorithm");
        fs::write(bootstrap_version.join("state"), "active").expect("bootstrap state");
        fs::write(bootstrap_version.join("private.pkcs8"), [0x5A; 32])
            .expect("bootstrap key material");
        Self {
            vault_root: TempDir::new().expect("material vault"),
            bootstrap_reference: KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                VAULT_BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                VAULT_BOOTSTRAP_SCOPE,
                VAULT_BOOTSTRAP_ALGORITHM,
            )
            .expect("bootstrap reference"),
            bootstrap_request: SecretResolutionRequest::new(
                vestrace_domain::WorkspaceId::new(),
                VAULT_BOOTSTRAP_SCOPE,
                "test://embedding-schema-material-vault",
            ),
            bootstrap_root,
        }
    }

    fn vault(&self) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault_root.path(),
            self.bootstrap_root.path(),
            self.bootstrap_reference.clone(),
            self.bootstrap_request.clone(),
        )
        .expect("separate host vault")
    }
}

const NEW_TABLES: [&str; 20] = [
    "embedding_corpus_generations",
    "embedding_corpus_generation_members",
    "embedding_jobs",
    "embedding_job_material_intents",
    "embedding_job_termination_receipts",
    "embedding_space_corpus_states",
    "embedding_index_generation_guards",
    "embedding_job_result_preparations",
    "embedding_projection_entries",
    "embedding_job_result_prepared_attachments",
    "embedding_projection_source_dependencies",
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

/// Every governed transition table carries the same owner, forced-RLS, and
/// explicit-ACL posture as the earlier P02/P03 tables.
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
        .unwrap_or_else(|error| {
            panic!("{table} must exist after the guarded migration set: {error}")
        });
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

#[sqlx::test(migrations = "../../migrations")]
async fn termination_tables_force_workspace_rls_before_row_constraints(pool: PgPool) {
    let (active_workspace, active_principal) = workspace(&pool).await;
    let (cross_workspace, _) = workspace(&pool).await;

    for table in [
        "embedding_job_material_intents",
        "embedding_job_termination_receipts",
    ] {
        let mut transaction = scope(&pool, active_workspace, active_principal).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *transaction)
            .await
            .unwrap();
        let refusal = sqlx::query(&format!(
            "INSERT INTO public.{table}(workspace_id) VALUES($1)"
        ))
        .bind(cross_workspace)
        .execute(&mut *transaction)
        .await
        .expect_err(
            "forced workspace RLS must reject the crossed row before its other constraints",
        );
        assert_eq!(
            refusal
                .as_database_error()
                .and_then(|database| database.code())
                .as_deref(),
            Some("42501"),
            "{table} must reject a guarded-owner cross-workspace insert through RLS: {refusal}"
        );
        transaction.rollback().await.unwrap();
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

async fn check_definition(pool: &PgPool, table: &str, constraint_name: &str) -> String {
    sqlx::query_scalar(
        "SELECT pg_get_constraintdef(constraint_.oid) \
           FROM pg_constraint AS constraint_ \
           JOIN pg_class AS class ON class.oid = constraint_.conrelid \
          WHERE class.relname = $1 AND constraint_.contype = 'c' \
            AND constraint_.conname = $2",
    )
    .bind(table)
    .bind(constraint_name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|error| {
        panic!("{table} must carry CHECK constraint {constraint_name}: {error}")
    })
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
    let definition = check_definition(
        &pool,
        "embedding_transition_barriers",
        "embedding_transition_barriers_state_check",
    )
    .await;
    assert_closed_check(&definition, &BarrierState::ALL, BarrierState::as_str);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_carry_header_state_check_matches_the_declared_enum(pool: PgPool) {
    let definition = check_definition(
        &pool,
        "embedding_transition_ambiguity_carries",
        "embedding_transition_ambiguity_carries_state_check",
    )
    .await;
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
        "embedding_transition_ambiguity_carry_recipes_state_check",
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
    let space_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) \
         VALUES($1,$2,'nomic-768',768,'nomic-embed-text')",
    )
    .bind(space_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

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
    .bind(space_id)
    .fetch_one(&mut *transaction)
    .await
    .expect("a complete space key is registrable");

    let first_generation = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_open_embedding_corpus_generation($1,$2,$3)")
        .bind(first_generation)
        .bind(workspace_id)
        .bind(registration)
        .fetch_one(&mut *transaction)
        .await
        .expect("the first generation opens");
    let first = sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,7)",
    )
    .bind(first_generation)
    .bind(workspace_id)
    .bind(registration)
    .fetch_one(&mut *transaction)
    .await
    .expect("the first generation publishes");
    assert_eq!(first, 1);

    let second_generation = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_open_embedding_corpus_generation($1,$2,$3)")
        .bind(second_generation)
        .bind(workspace_id)
        .bind(registration)
        .fetch_one(&mut *transaction)
        .await
        .expect("the second generation opens");
    let second = sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,9)",
    )
    .bind(second_generation)
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
    let crossed_space = Uuid::now_v7();
    for space_id in [space, crossed_space] {
        sqlx::query(
            "INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) \
             VALUES($1,$2,'space',768,'model')",
        )
        .bind(space_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    }

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
    .bind(crossed_space)
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

async fn reserve_embedding_output(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    job_id: Uuid,
    intent_id: Uuid,
    material_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT vestrace_reserve_embedding_job_output_intent($1,$2,$3,0,$4,$5,$6)")
        .bind(intent_id)
        .bind(workspace_id)
        .bind(job_id)
        .bind(material_id)
        .bind(material_key_id)
        .bind(nonce)
        .fetch_one(&mut **transaction)
        .await
}

async fn terminate_embedding_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    principal_id: Uuid,
    job_id: Uuid,
    idempotency_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT * FROM vestrace_terminate_embedding_job_pre_dispatch( \
         $1,$2,$3,$4,1,$5,'cancelled','cancellation_authorization',$6, \
         'fixture-policy','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(principal_id)
    .bind(job_id)
    .bind(idempotency_key)
    .bind(Uuid::now_v7())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
}

async fn wait_for_blocker(pool: &PgPool, waiting_pid: i32, blocker_pid: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blockers: Vec<i32> = sqlx::query_scalar("SELECT unnest(pg_blocking_pids($1))")
                .bind(waiting_pid)
                .fetch_all(pool)
                .await
                .unwrap();
            if blockers.contains(&blocker_pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("session never reached the required observed lock wait");
}

#[sqlx::test(migrations = "../../migrations")]
async fn embedding_output_membership_reservation_is_exact_and_replayable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let job_id = fixture.job_id.as_uuid();
    let intent_id = Uuid::now_v7();
    let material_id = Uuid::now_v7();
    let material_key_id = Uuid::now_v7();
    let nonce = Uuid::now_v7();

    let mut first = scope(
        &runtime,
        workspace_id,
        fixture.context.principal_id.as_uuid(),
    )
    .await;
    assert_eq!(
        reserve_embedding_output(
            &mut first,
            workspace_id,
            job_id,
            intent_id,
            material_id,
            material_key_id,
            nonce,
        )
        .await
        .unwrap(),
        intent_id
    );
    first.commit().await.unwrap();

    let mut observer = scope(
        &runtime,
        workspace_id,
        fixture.context.principal_id.as_uuid(),
    )
    .await;
    let persisted: (String, Uuid, i64, String) = sqlx::query_as(
        "SELECT intent.owner_kind,intent.owner_id,membership.output_ordinal,intent.state \
           FROM embedding_job_material_intents AS membership \
           JOIN material_key_creation_intents AS intent \
             ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id \
          WHERE membership.workspace_id=$1 AND membership.job_id=$2",
    )
    .bind(workspace_id)
    .bind(job_id)
    .fetch_one(&mut *observer)
    .await
    .unwrap();
    observer.commit().await.unwrap();
    assert_eq!(
        persisted,
        (
            "embedding_job_output".to_owned(),
            job_id,
            0,
            "reserved".to_owned()
        )
    );

    let mut replay = scope(
        &runtime,
        workspace_id,
        fixture.context.principal_id.as_uuid(),
    )
    .await;
    assert_eq!(
        reserve_embedding_output(
            &mut replay,
            workspace_id,
            job_id,
            intent_id,
            material_id,
            material_key_id,
            nonce,
        )
        .await
        .unwrap(),
        intent_id,
        "an exact reservation replay must return the enrolled identity"
    );
    replay.commit().await.unwrap();

    for (changed_material, changed_key, changed_nonce) in [
        (Uuid::now_v7(), material_key_id, nonce),
        (material_id, Uuid::now_v7(), nonce),
        (material_id, material_key_id, Uuid::now_v7()),
    ] {
        let mut changed = scope(
            &runtime,
            workspace_id,
            fixture.context.principal_id.as_uuid(),
        )
        .await;
        let refusal = reserve_embedding_output(
            &mut changed,
            workspace_id,
            job_id,
            intent_id,
            changed_material,
            changed_key,
            changed_nonce,
        )
        .await
        .expect_err("a replay that changes the material tuple must not converge");
        assert!(
            refusal.as_database_error().is_some(),
            "the guarded reservation must reject a changed replay structurally: {refusal}"
        );
        changed.rollback().await.unwrap();
    }

    let mut shared_gate = scope(&pool, workspace_id, fixture.context.principal_id.as_uuid()).await;
    let shared_refusal =
        sqlx::query("SELECT vestrace_lock_embedding_job_pre_dispatch_gate($1,$2,false)")
            .bind(workspace_id)
            .bind(fixture.external_effect_id)
            .execute(&mut *shared_gate)
            .await
            .expect_err("an enrolled output job must not reach shared provider dispatch");
    assert_eq!(
        shared_refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    shared_gate.rollback().await.unwrap();

    let mut direct = scope(
        &runtime,
        workspace_id,
        fixture.context.principal_id.as_uuid(),
    )
    .await;
    let direct_refusal = sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
           effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,NOW()+interval '5 minutes')",
    )
    .bind(fixture.external_effect_id)
    .bind(workspace_id)
    .bind(fixture.external_effect_id.to_string())
    .bind(Uuid::now_v7())
    .execute(&mut *direct)
    .await
    .expect_err("an enrolled output job must refuse the direct lifecycle dispatch writer");
    assert_eq!(
        direct_refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    direct.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn embedding_output_membership_deferred_validator_rejects_orphans_and_mismatches(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let principal_id = fixture.context.principal_id.as_uuid();
    let job_id = fixture.job_id.as_uuid();

    let mut orphan = scope(&runtime, workspace_id, principal_id).await;
    sqlx::query_scalar::<_, ()>(
        "SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'embedding_job_output',$6,0)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(job_id)
    .fetch_one(&mut *orphan)
    .await
    .unwrap();
    let error = orphan
        .commit()
        .await
        .expect_err("a canonical embedding output without membership must fail at commit");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );

    for (owner_kind, owner_id, member_ordinal) in [
        ("other_owner", job_id, 0_i64),
        ("embedding_job_output", Uuid::now_v7(), 0_i64),
        ("embedding_job_output", job_id, 1_i64),
    ] {
        let intent_id = Uuid::now_v7();
        let mut mismatch = scope(&pool, workspace_id, principal_id).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *mismatch)
            .await
            .unwrap();
        sqlx::query_scalar::<_, ()>(
            "SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,$6,$7,0)",
        )
        .bind(intent_id)
        .bind(workspace_id)
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_one(&mut *mismatch)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO embedding_job_material_intents(workspace_id,job_id,output_ordinal,intent_id) \
             VALUES($1,$2,$3,$4)",
        )
        .bind(workspace_id)
        .bind(job_id)
        .bind(member_ordinal)
        .bind(intent_id)
        .execute(&mut *mismatch)
        .await
        .unwrap();
        let error = mismatch
            .commit()
            .await
            .expect_err("a mismatched membership must fail its deferred validator at commit");
        assert_eq!(
            error
                .as_database_error()
                .and_then(|database| database.code())
                .as_deref(),
            Some("23514"),
            "owner={owner_kind}, ordinal={member_ordinal}"
        );
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_witnessed_abandoned_output_member_allows_pre_dispatch_termination(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let principal_id = fixture.context.principal_id.as_uuid();
    let job_id = fixture.job_id.as_uuid();
    let intent_id = Uuid::now_v7();
    let material_key_id = Uuid::now_v7();
    let nonce = Uuid::now_v7();
    let termination_receipt_id = Uuid::now_v7();
    let termination_evidence_id = Uuid::now_v7();

    let mut reserve = scope(&runtime, workspace_id, principal_id).await;
    reserve_embedding_output(
        &mut reserve,
        workspace_id,
        job_id,
        intent_id,
        Uuid::now_v7(),
        material_key_id,
        nonce,
    )
    .await
    .unwrap();
    reserve.commit().await.unwrap();

    let mut active = scope(&runtime, workspace_id, principal_id).await;
    let active_refusal = sqlx::query(
        "SELECT * FROM vestrace_terminate_embedding_job_pre_dispatch( \
         $1,$2,$3,$4,1,'active-member','cancelled','cancellation_authorization',$5, \
         'fixture-policy','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(principal_id)
    .bind(job_id)
    .bind(Uuid::now_v7())
    .execute(&mut *active)
    .await
    .expect_err("a reserved output member must block cancellation");
    assert_eq!(
        active_refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    active.rollback().await.unwrap();

    // Commit the output-owned retirement authority before touching the
    // independent host vault. The generic abandonment helper must no longer be
    // a public bypass for an active embedding output.
    let mut abandon = scope(&runtime, workspace_id, principal_id).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_request_embedding_output_retirement( \
         $1,$2,$3,$4,1,'witnessed-member','cancelled','cancellation_authorization',$5, \
         'fixture-policy','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(termination_receipt_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(job_id)
    .bind(termination_evidence_id)
    .fetch_one(&mut *abandon)
    .await
    .unwrap();
    abandon.commit().await.unwrap();

    // The actual host-vault sequence is outside every database transaction.
    // Reopening verifies durable erased state and exact-replay receipt stability.
    let vault_fixture = VaultFixture::new();
    let key_id = MaterialKeyId::from_uuid(material_key_id);
    let intent_nonce = IntentNonce::from_uuid(nonce);
    let vault = vault_fixture.vault();
    vault
        .create_if_absent(key_id, intent_nonce)
        .expect("the host vault creates the exact reserved output key");
    vault
        .prepare_erasure(key_id)
        .expect("the host vault records the erasure fence");
    let erasure_receipt = vault
        .erase(key_id)
        .expect("the host vault performs the witnessed erase");
    let reopened = vault_fixture.vault();
    assert_eq!(
        reopened
            .erase(key_id)
            .expect("an erased key replays its receipt"),
        erasure_receipt,
        "the host-vault witness must be stable after reopening"
    );
    assert!(matches!(
        reopened.unwrap(key_id, &mut |_| panic!("an erased DEK must not be exposed")),
        Err(VaultError::Erased)
    ));

    let mut record_witness = scope(&runtime, workspace_id, principal_id).await;
    sqlx::query_scalar::<_, ()>("SELECT vestrace_record_embedding_output_key_retirement($1,$2,$3)")
        .bind(workspace_id)
        .bind(intent_id)
        .bind(erasure_receipt.as_uuid())
        .fetch_one(&mut *record_witness)
        .await
        .unwrap();
    record_witness.commit().await.unwrap();

    // The P02 receipt table is intentionally opaque to the runtime role. Use
    // the independent owner observer only to prove the host-vault receipt was
    // durably recorded; all guarded transitions above remain runtime-scoped.
    let witness: Uuid = sqlx::query_scalar(
        "SELECT erasure_receipt FROM material_key_creation_intent_erasure_receipts \
          WHERE workspace_id=$1 AND intent_id=$2",
    )
    .bind(workspace_id)
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        witness,
        erasure_receipt.as_uuid(),
        "the durable witness must name this output intent exactly"
    );

    let mut terminate = scope(&runtime, workspace_id, principal_id).await;
    sqlx::query(
        "SELECT * FROM vestrace_terminate_embedding_job_pre_dispatch( \
         $1,$2,$3,$4,1,'witnessed-member','cancelled','cancellation_authorization',$5, \
         'fixture-policy','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(termination_receipt_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(job_id)
    .bind(termination_evidence_id)
    .execute(&mut *terminate)
    .await
    .expect("the exact output retirement receipt and abandoned intent must authorize termination");
    terminate.commit().await.unwrap();

    let mut state_observer = scope(&runtime, workspace_id, principal_id).await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(workspace_id)
            .bind(job_id)
            .fetch_one(&mut *state_observer)
            .await
            .unwrap();
    state_observer.commit().await.unwrap();
    assert_eq!(state, "cancelled");
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn enrollment_and_termination_serialize_on_the_embedding_job_lock_chain(pool: PgPool) {
    // Enrollment owns the canonical guards first. Cancellation must visibly
    // wait, then inspect the committed active member and refuse.
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let principal_id = fixture.context.principal_id.as_uuid();
    let job_id = fixture.job_id.as_uuid();
    let mut enrollment = scope(&runtime, workspace_id, principal_id).await;
    let enrollment_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *enrollment)
        .await
        .unwrap();
    reserve_embedding_output(
        &mut enrollment,
        workspace_id,
        job_id,
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    )
    .await
    .unwrap();

    let cancellation_pool = runtime_pool(&pool).await;
    let (cancellation_pid_sender, cancellation_pid_receiver) = tokio::sync::oneshot::channel();
    let cancelling = tokio::spawn(async move {
        let mut transaction = scope(&cancellation_pool, workspace_id, principal_id).await;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        cancellation_pid_sender.send(pid).unwrap();
        let outcome = terminate_embedding_job(
            &mut transaction,
            workspace_id,
            principal_id,
            job_id,
            "enrollment-first-cancellation",
        )
        .await;
        match outcome {
            Ok(()) => {
                transaction.commit().await?;
                Ok(())
            }
            Err(error) => {
                transaction.rollback().await.unwrap();
                Err(error)
            }
        }
    });
    let cancellation_pid = cancellation_pid_receiver.await.unwrap();
    wait_for_blocker(&pool, cancellation_pid, enrollment_pid).await;
    enrollment.commit().await.unwrap();
    let cancellation_error = tokio::time::timeout(Duration::from_secs(5), cancelling)
        .await
        .expect("cancellation task must complete after enrollment commits")
        .expect("cancellation task must join")
        .expect_err("committed enrollment must make cancellation refuse its active member");
    assert_eq!(
        cancellation_error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );

    let mut first_observer = scope(&runtime, workspace_id, principal_id).await;
    let first_persisted: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
            (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
            (SELECT COUNT(*) FROM embedding_job_material_intents WHERE workspace_id=$1 AND job_id=$2), \
            (SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2)",
    )
    .bind(workspace_id)
    .bind(job_id)
    .fetch_one(&mut *first_observer)
    .await
    .unwrap();
    first_observer.commit().await.unwrap();
    assert_eq!(
        first_persisted,
        ("requested".to_owned(), 1, 1, 0),
        "enrollment-first ordering must persist only the requested job and its one active member"
    );
    runtime.close().await;

    // Termination owns the same chain first. A concurrent enrollment visibly
    // waits, then sees a persisted terminal job and leaves no output facts.
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let principal_id = fixture.context.principal_id.as_uuid();
    let job_id = fixture.job_id.as_uuid();
    let mut cancellation = scope(&runtime, workspace_id, principal_id).await;
    let cancellation_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *cancellation)
        .await
        .unwrap();
    terminate_embedding_job(
        &mut cancellation,
        workspace_id,
        principal_id,
        job_id,
        "termination-first-enrollment",
    )
    .await
    .unwrap();

    let enrollment_pool = runtime_pool(&pool).await;
    let (enrollment_pid_sender, enrollment_pid_receiver) = tokio::sync::oneshot::channel();
    let enroll = tokio::spawn(async move {
        let mut transaction = scope(&enrollment_pool, workspace_id, principal_id).await;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        enrollment_pid_sender.send(pid).unwrap();
        let outcome = reserve_embedding_output(
            &mut transaction,
            workspace_id,
            job_id,
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
        )
        .await;
        match outcome {
            Ok(_) => {
                transaction.commit().await?;
                Ok(())
            }
            Err(error) => {
                transaction.rollback().await.unwrap();
                Err(error)
            }
        }
    });
    let enrollment_pid = enrollment_pid_receiver.await.unwrap();
    wait_for_blocker(&pool, enrollment_pid, cancellation_pid).await;
    cancellation.commit().await.unwrap();
    let enrollment_error = tokio::time::timeout(Duration::from_secs(5), enroll)
        .await
        .expect("enrollment task must complete after cancellation commits")
        .expect("enrollment task must join")
        .expect_err("committed terminal cancellation must refuse enrollment");
    assert_eq!(
        enrollment_error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );

    let mut observer = scope(&runtime, workspace_id, principal_id).await;
    let persisted: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_job_material_intents WHERE workspace_id=$1 AND job_id=$2), \
            (SELECT COUNT(*) FROM material_key_creation_intents WHERE workspace_id=$1 \
              AND owner_kind='embedding_job_output' AND owner_id=$2)",
    )
    .bind(workspace_id)
    .bind(job_id)
    .fetch_one(&mut *observer)
    .await
    .unwrap();
    observer.commit().await.unwrap();
    assert_eq!(
        persisted,
        (0, 0),
        "a terminated job must leave no enrolled output membership or intent"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn result_finalization_runtime_inventory_preserves_prior_acl_and_closes_new_authority(
    pool: PgPool,
) {
    common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
    for table in [
        "embedding_job_credential_completion_blockers",
        "embedding_result_credential_blocker_adoptions",
        "embedding_result_key_binding_receipts",
        "embedding_job_result_publications",
        "embedding_index_rebuild_events",
    ] {
        let inventory: (String, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity,has_table_privilege('vestrace',$1,'INSERT'),has_table_privilege('vestrace',$1,'UPDATE'),has_table_privilege('vestrace',$1,'DELETE'),has_table_privilege('vestrace',$1,'TRIGGER') FROM pg_class WHERE oid=$1::regclass",
        ).bind(table).fetch_one(&pool).await.unwrap();
        assert_eq!(
            inventory,
            (
                "vestrace_guarded_owner".into(),
                true,
                true,
                false,
                false,
                false,
                false
            ),
            "{table}"
        );
    }
    for signature in [
        "vestrace_ensure_embedding_credential_completion_blocker(uuid,uuid,uuid)",
        "vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)",
        "vestrace_lock_embedding_result_finalization(uuid,uuid,uuid,uuid)",
        "vestrace_validate_embedding_credential_completion_owner()",
        "vestrace_guard_embedding_credential_completion_blocker()",
        "vestrace_guard_embedding_projection_publication()",
        "vestrace_validate_embedding_result_preparation()",
    ] {
        let authority: (String, bool, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(proowner),prosecdef,has_function_privilege('vestrace',oid,'EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure",
        ).bind(signature).fetch_one(&pool).await.unwrap();
        assert_eq!(
            authority,
            ("vestrace_guarded_owner".into(), true, false),
            "{signature}"
        );
    }
    for signature in [
        "vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid)",
        "vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid)",
        "vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])",
        "vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid)",
        //0191's published fallback promises this same runtime capability.
        "vestrace_publish_embedding_corpus_generation(uuid,uuid,uuid,bigint)",
    ] {
        let allowed: bool =
            sqlx::query_scalar("SELECT has_function_privilege('vestrace',$1,'EXECUTE')")
                .bind(signature)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(allowed, "{signature}");
    }
    for table in [
        "credential_revisions",
        "credential_key_creation_intents",
        "embedding_space_registrations",
    ] {
        let previous: bool =
            sqlx::query_scalar("SELECT has_table_privilege('vestrace',$1,'REFERENCES')")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(previous, "preexisting REFERENCES must survive0195: {table}");
    }
}
