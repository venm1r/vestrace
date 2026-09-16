//! What the real worker binary does with embedding work in one bounded pass.
//!
//! Everything here runs `vestrace worker --once` as a child process against a
//! migrated database, as an operator would. That is the point: the embedding
//! composition is reached only through `main`, and a unit test of its parts
//! cannot tell whether the parts are wired together, configured from the values
//! an operator sets, or reachable by the restricted runtime role at all.
//!
//! `provider_runtime_wiring` asserts the same composition by reading the source.
//! This asserts it by running it. Both are needed and neither substitutes: the
//! source test catches a legacy path that is still constructed, and this one
//! catches a governed path that is constructed and does not work.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use sqlx::{ConnectOptions, PgPool, postgres::PgConnectOptions};
use uuid::Uuid;
use vestrace_domain::id::{PrincipalId, WorkspaceId};

const CHILD_WAIT_LIMIT: Duration = Duration::from_secs(30);

/// The tables the worker's legacy halves own, handed to the runtime role the
/// same way `worker_once_cli` does. Guarded P03/P04 tables are never touched:
/// the whole question this suite asks is whether the restricted role can do its
/// work through the guarded authorities, so lending it ownership of those would
/// answer a question nobody asked.
async fn prepare_worker_runtime_ownership(pool: &PgPool) {
    for table in [
        "agent_runs",
        "run_events",
        "run_leases",
        "run_work_items",
        "outbox",
        "external_effect_intents",
        "external_effect_lifecycle_transitions",
        "external_reconciliations",
        "external_effect_worker_presence",
    ] {
        let previous_owner: String = sqlx::query_scalar(
            "SELECT pg_get_userbyid(class.relowner) FROM pg_class AS class \
              JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace \
             WHERE namespace.nspname = 'public' AND class.relname = $1",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("read fixture table owner before reassignment");
        assert_ne!(
            previous_owner, "vestrace_guarded_owner",
            "this fixture must never reassign guarded table {table}"
        );
        sqlx::query(&format!("ALTER TABLE public.{table} OWNER TO vestrace"))
            .execute(pool)
            .await
            .expect("fixture must hand legacy worker table to the runtime role");
    }
}

fn runtime_database_url(pool: &PgPool) -> String {
    let runtime_database_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role");
    let parsed = PgConnectOptions::from_str(&runtime_database_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    assert_eq!(
        parsed.get_username(),
        "vestrace",
        "VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role"
    );
    let password = runtime_database_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must contain a password");

    pool.connect_options()
        .as_ref()
        .clone()
        .username(parsed.get_username())
        .password(password)
        .to_url_lossy()
        .to_string()
}

struct WorkerFixture {
    root: PathBuf,
    material_vault_root: PathBuf,
    bootstrap_secret_root: PathBuf,
}

impl WorkerFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("vestrace-embedding-once-{}", Uuid::now_v7()));
        let material_vault_root = root.join("material-vault");
        let bootstrap_secret_root = root.join("bootstrap-store");
        fs::create_dir_all(&material_vault_root).expect("create material vault root");

        let key_root = bootstrap_secret_root.join("worker-bootstrap");
        let version_root = key_root.join("v1");
        fs::create_dir_all(&version_root).expect("create bootstrap key version");
        fs::write(key_root.join("scope"), "installation").expect("write bootstrap scope");
        fs::write(key_root.join("purpose"), "storage").expect("write bootstrap purpose");
        fs::write(key_root.join("algorithm"), "aes-256-gcm").expect("write bootstrap algorithm");
        fs::write(version_root.join("state"), "active").expect("write bootstrap state");
        fs::write(version_root.join("private.pkcs8"), [0x5A; 32])
            .expect("write bootstrap key material");

        Self {
            root,
            material_vault_root,
            bootstrap_secret_root,
        }
    }
}

impl Drop for WorkerFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

/// One bounded pass of the real binary, with an embedding provider configured.
///
/// The endpoint is deliberately unreachable. Every assertion here is about what
/// the worker does before it would call a provider -- claiming, sweeping,
/// materializing, refusing -- and an endpoint that answered would make the
/// difference between "never called it" and "called it and ignored the answer"
/// invisible.
fn run_once(database_url: &str, workspace_id: WorkspaceId, fixture: &WorkerFixture) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vestrace"));
    command
        .args(["worker", "--once"])
        .env("VESTRACE_DATABASE__URL", database_url)
        .env("VESTRACE_WORKSPACES", workspace_id.to_string())
        .env(
            "VESTRACE_PROVIDER_EXECUTION__MATERIAL_VAULT_ROOT",
            &fixture.material_vault_root,
        )
        .env(
            "VESTRACE_PROVIDER_EXECUTION__BOOTSTRAP_SECRET_ROOT",
            &fixture.bootstrap_secret_root,
        )
        .env(
            "VESTRACE_PROVIDER_EXECUTION__BOOTSTRAP_KEY_ID",
            "worker-bootstrap",
        )
        .env("VESTRACE_PROVIDER_EXECUTION__BOOTSTRAP_KEY_VERSION", "v1")
        .env("VESTRACE_POLICY__ENGINE", "configured-capabilities")
        .env("VESTRACE_POLICY__CAPABILITIES", "execution.write")
        .env("VESTRACE_POLICY__DATA__MODE", "enforce")
        .env("VESTRACE_POLICY__DATA__CLASSIFICATION", "public")
        .env("VESTRACE_POLICY__DATA__MAXIMUM_SENSITIVITY", "public")
        .env("VESTRACE_POLICY__DATA__ALLOWED_DESTINATIONS", "local_model")
        .env("VESTRACE_EMBEDDING__ENABLED", "true")
        .env("VESTRACE_EMBEDDING__BASE_URL", "http://127.0.0.1:9/v1")
        .env("VESTRACE_EMBEDDING__MODEL_NAME", "embedding-once-test")
        .env("VESTRACE_EMBEDDING__SPACE_NAME", "embedding-once-test")
        .env("VESTRACE_POLICY__DATA__EMBEDDING__MODE", "enforce")
        .env(
            "VESTRACE_POLICY__DATA__EMBEDDING__ADMISSIBLE_LABELS",
            "public",
        )
        .env(
            "VESTRACE_POLICY__DATA__EMBEDDING__ALLOW_UNCLASSIFIED",
            "true",
        )
        .env("VESTRACE_POLICY__DATA__EMBEDDING__CLASSIFICATION", "public")
        .env(
            "VESTRACE_POLICY__DATA__EMBEDDING__MAXIMUM_SENSITIVITY",
            "public",
        )
        .env(
            "VESTRACE_POLICY__DATA__EMBEDDING__ALLOWED_DESTINATIONS",
            "local_model",
        );

    let stdout_path = fixture.root.join("worker.stdout");
    let stderr_path = fixture.root.join("worker.stderr");
    let mut child = command
        .stdout(Stdio::from(
            fs::File::create(&stdout_path).expect("create worker stdout capture"),
        ))
        .stderr(Stdio::from(
            fs::File::create(&stderr_path).expect("create worker stderr capture"),
        ))
        .spawn()
        .expect("vestrace worker should start");
    let deadline = Instant::now() + CHILD_WAIT_LIMIT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("check vestrace worker exit") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            panic!(
                "vestrace worker --once did not finish within {CHILD_WAIT_LIMIT:?}; stderr:\n{}",
                fs::read_to_string(&stderr_path).unwrap_or_default()
            );
        }
        thread::sleep(Duration::from_millis(50));
    };
    Output {
        status,
        stdout: fs::read(&stdout_path).unwrap_or_default(),
        stderr: fs::read(&stderr_path).unwrap_or_default(),
    }
}

async fn seed_workspace(pool: &PgPool) -> (WorkspaceId, PrincipalId) {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-once-{workspace_id}"))
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-once-{principal_id}"))
        .execute(pool)
        .await
        .expect("seed principal");
    (workspace_id, principal_id)
}

/// One active memory with one revision that says something.
///
/// The event and source are not decoration: migration 0114 refuses an active
/// memory with no provenance, and a fixture that reached around that would be
/// embedding something the product cannot produce.
async fn seed_memory(pool: &PgPool, workspace_id: WorkspaceId) -> Uuid {
    let memory_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO events (id, workspace_id, event_type, actor)          VALUES ($1,$2,'memory.seeded','{\"kind\":\"system\"}'::jsonb)",
    )
    .bind(event_id)
    .bind(workspace_id.as_uuid())
    .execute(pool)
    .await
    .expect("seed provenance event");
    sqlx::query(
        "INSERT INTO memories (id, workspace_id, kind, status) VALUES ($1,$2,'fact','candidate')",
    )
    .bind(memory_id)
    .bind(workspace_id.as_uuid())
    .execute(pool)
    .await
    .expect("seed memory");
    sqlx::query(
        "INSERT INTO memory_sources (id, memory_id, workspace_id, event_id, role)          VALUES ($1,$2,$3,$4,'origin')",
    )
    .bind(Uuid::now_v7())
    .bind(memory_id)
    .bind(workspace_id.as_uuid())
    .bind(event_id)
    .execute(pool)
    .await
    .expect("seed memory source");
    sqlx::query(
        "INSERT INTO memory_revisions (id, memory_id, workspace_id, revision_number, content, \
         confidence, importance) VALUES ($1,$2,$3,1,'the worker must try to embed this',0.5,0.5)",
    )
    .bind(revision_id)
    .bind(memory_id)
    .bind(workspace_id.as_uuid())
    .execute(pool)
    .await
    .expect("seed memory revision");
    sqlx::query("UPDATE memories SET active_revision_id=$1, status='active' WHERE id=$2")
        .bind(revision_id)
        .bind(memory_id)
        .execute(pool)
        .await
        .expect("point the memory at its revision and activate it");
    memory_id
}

/// Every embedding cycle runs, as the restricted runtime role, and none fails.
///
/// This is the assertion that would have caught the two defects migration 0205
/// repairs. `vestrace_claim_embedding_work` refused every call -- first on an
/// ambiguous `job_id`, then because the guarded owner had been left with an
/// empty ACL on the table its own definer function inserts into -- and a cycle
/// that cannot claim reports a retryable failure, so a worker with nothing to do
/// would have exited 1 instead of 3. No test called that function, and every
/// existing worker test ran with no embedding provider configured, so the whole
/// dispatch path was unreached.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_idle_worker_runs_every_embedding_cycle_without_failing(pool: PgPool) {
    let (workspace_id, _) = seed_workspace(&pool).await;
    prepare_worker_runtime_ownership(&pool).await;
    let fixture = WorkerFixture::new();

    let output = run_once(&runtime_database_url(&pool), workspace_id, &fixture);

    assert_eq!(
        output.status.code(),
        Some(3),
        "a workspace with no work must report idle, not failure; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("embedding cycle failed"),
        "no embedding cycle may fail on an idle workspace; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("embedding erasure invalidation sweep failed"),
        "the erasure sweep must run cleanly on an idle workspace; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no embedding provider is configured"),
        "this fixture configures one, so the absent-provider branch must not be taken"
    );
}

/// The governed on-write route is composed, reached, and refuses honestly.
///
/// A workspace with no canonical embedding space cannot embed anything, and the
/// route says so rather than succeeding quietly. That distinction is the whole
/// reason this route exists: the one it replaced failed every message, and a
/// replacement that swallowed them would look healthier while leaving exactly
/// the same corpus behind.
///
/// The message stays retriable, because the refusal is about the workspace's
/// state and not about the message -- a transition that establishes a canonical
/// space makes this same message succeed unaided.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn the_on_write_route_refuses_a_memory_with_no_canonical_space(pool: PgPool) {
    let (workspace_id, _) = seed_workspace(&pool).await;
    let memory_id = seed_memory(&pool, workspace_id).await;
    let message_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO outbox (id, workspace_id, topic, payload) \
         VALUES ($1, $2, 'memory.created', jsonb_build_object('memory_id', $3::text))",
    )
    .bind(message_id)
    .bind(workspace_id.as_uuid())
    .bind(memory_id.to_string())
    .execute(&pool)
    .await
    .expect("seed the on-write message");
    prepare_worker_runtime_ownership(&pool).await;
    let fixture = WorkerFixture::new();

    let output = run_once(&runtime_database_url(&pool), workspace_id, &fixture);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a failed outbox delivery must be reported; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (attempts, processed, dead_lettered, last_error): (i32, bool, bool, Option<String>) =
        sqlx::query_as(
            "SELECT attempts, processed_at IS NOT NULL, dead_lettered_at IS NOT NULL, last_error \
               FROM outbox WHERE id = $1",
        )
        .bind(message_id)
        .fetch_one(&pool)
        .await
        .expect("read the delivery attempt");
    assert_eq!(attempts, 1, "the attempt must be durable");
    assert!(!processed, "a refused message is not processed");
    assert!(
        !dead_lettered,
        "the refusal is about the workspace, not the message, so it stays retriable"
    );
    let reported = last_error.unwrap_or_default();
    assert!(
        reported.contains("no canonical embedding space is active"),
        "the recorded reason must name what is missing, not a generic failure: {reported}"
    );

    // And nothing was embedded: no job, and no content material minted for a
    // revision the route could not place.
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding_jobs WHERE workspace_id=$1")
        .bind(workspace_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0, "a refused route accepts no job");
    let materials: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM material_key_creation_intents \
          WHERE workspace_id=$1 AND owner_kind='memory_revision'",
    )
    .bind(workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        materials, 0,
        "the space is checked before the revision is materialized, so a refusal leaves no \
         ciphertext of content it never embedded"
    );
}
