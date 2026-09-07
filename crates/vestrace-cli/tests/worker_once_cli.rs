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
use vestrace_application::RequestContext;
use vestrace_application::run::ports::{CommitRun, RunStorePort, WorkItem, WorkItemKind};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, PrincipalId, WorkItemId, WorkspaceId,
};
use vestrace_domain::run::{
    AgentRun, NewAgentRun, ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunExecutionMode,
    RunVersion,
};
use vestrace_infrastructure::{PgStore, PostgresRunStore};

const UNAVAILABLE_DATABASE_URL: &str =
    "postgres://worker-once:unavailable@127.0.0.1:9/vestrace_worker_once";
const CHILD_WAIT_LIMIT: Duration = Duration::from_secs(15);

/// SQLx creates disposable databases as the administrative migrator. Production
/// bootstrap gives the runtime role these legacy worker tables; keep the
/// fixture's authority the same without touching guarded P03/P04 tables.
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
            "worker fixture must never reassign guarded table {table}"
        );
        sqlx::query(&format!("ALTER TABLE public.{table} OWNER TO vestrace"))
            .execute(pool)
            .await
            .expect("fixture must hand legacy worker table to the runtime role");
        let owner: String = sqlx::query_scalar(
            "SELECT pg_get_userbyid(class.relowner) FROM pg_class AS class \
              JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace \
             WHERE namespace.nspname = 'public' AND class.relname = $1",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("read fixture table owner");
        assert_eq!(owner, "vestrace", "fixture owner for {table}");
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
        let root = std::env::temp_dir().join(format!("vestrace-worker-once-{}", Uuid::now_v7()));
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

fn run_once(database_url: &str, workspace_id: WorkspaceId, fixture: &WorkerFixture) -> Output {
    run_once_with_embedding_handler(database_url, workspace_id, fixture, false)
}

fn run_once_with_embedding_handler(
    database_url: &str,
    workspace_id: WorkspaceId,
    fixture: &WorkerFixture,
    enable_embedding_handler: bool,
) -> Output {
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
        .env("VESTRACE_POLICY__DATA__ALLOWED_DESTINATIONS", "local_model");
    if enable_embedding_handler {
        // The malformed payload below fails before this loopback endpoint could
        // be called. It exists only to construct the real outbox handler.
        command
            .env("VESTRACE_EMBEDDING__ENABLED", "true")
            .env("VESTRACE_EMBEDDING__BASE_URL", "http://127.0.0.1:9/v1")
            .env("VESTRACE_EMBEDDING__MODEL_NAME", "worker-once-test")
            .env("VESTRACE_EMBEDDING__SPACE_NAME", "worker-once-test")
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
    }

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
    loop {
        if let Some(status) = child.try_wait().expect("check vestrace worker exit") {
            return Output {
                status,
                stdout: fs::read(&stdout_path).expect("read worker stdout capture"),
                stderr: fs::read(&stderr_path).expect("read worker stderr capture"),
            };
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .expect("stop worker exceeding bounded test wait");
            let status = child.wait().expect("wait for timed-out worker");
            let output = Output {
                status,
                stdout: fs::read(&stdout_path).expect("read timed-out worker stdout"),
                stderr: fs::read(&stderr_path).expect("read timed-out worker stderr"),
            };
            panic!(
                "vestrace worker exceeded {CHILD_WAIT_LIMIT:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

async fn seed_workspace(pool: &PgPool) -> (WorkspaceId, PrincipalId) {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("worker-once-{workspace_id}"))
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("worker-once-{principal_id}"))
        .execute(pool)
        .await
        .expect("seed principal");
    (workspace_id, principal_id)
}

async fn seed_run_work(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
) -> WorkItemId {
    let context = RequestContext::new(workspace_id, principal_id);
    let at = vestrace_domain::now();
    let run = AgentRun::create(
        NewAgentRun {
            id: AgentRunId::new(),
            workspace_id,
            objective: "prove the worker handled one item".into(),
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            execution_mode: RunExecutionMode::Supervised,
            parent: None,
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
        },
        at,
    )
    .expect("create seeded run");
    let event = RunEvent::new(
        run.id,
        workspace_id,
        RunVersion::INITIAL,
        ResumeCursor::from_version(RunVersion::INITIAL),
        RunActorRef::System,
        RunEventPayload::RunCreated {
            objective: run.objective.clone(),
            execution_mode: run.execution_mode,
            coordinator_snapshot_id: run.coordinator_snapshot_id,
            parent: None,
        },
        vestrace_domain::CorrelationId::new(),
        None,
        at,
    )
    .expect("create seeded run event");
    let item = WorkItem {
        id: WorkItemId::new(),
        run_id: run.id,
        kind: WorkItemKind::AdvanceRun,
        expected_run_version: RunVersion::INITIAL,
        available_at: at,
        idempotency_key: format!("worker-once:{}", run.id),
        attempt: 1,
    };

    let item_id = item.id;
    PostgresRunStore::new(&PgStore::from_pool(pool.clone()))
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![item],
            },
        )
        .await
        .expect("seed queued run work");
    item_id
}

#[sqlx::test(migrations = "../../migrations")]
async fn once_exits_zero_when_a_workspace_has_work(pool: PgPool) {
    let (workspace_id, principal_id) = seed_workspace(&pool).await;
    let item_id = seed_run_work(&pool, workspace_id, principal_id).await;
    prepare_worker_runtime_ownership(&pool).await;
    let fixture = WorkerFixture::new();

    let output = run_once(&runtime_database_url(&pool), workspace_id, &fixture);

    assert_eq!(
        output.status.code(),
        Some(0),
        "a worker pass with queued work must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: String = sqlx::query_scalar("SELECT status FROM run_work_items WHERE id = $1")
        .bind(item_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("read processed work item");
    assert_eq!(status, "completed", "the seeded handler work must complete");
}

#[sqlx::test(migrations = "../../migrations")]
async fn once_exits_three_when_every_poll_is_idle(pool: PgPool) {
    let (workspace_id, _) = seed_workspace(&pool).await;
    prepare_worker_runtime_ownership(&pool).await;
    let fixture = WorkerFixture::new();

    let output = run_once(&runtime_database_url(&pool), workspace_id, &fixture);

    assert_eq!(
        output.status.code(),
        Some(3),
        "an idle worker pass must be distinguishable from success and failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn once_reports_a_failed_outbox_poll_after_successful_work(pool: PgPool) {
    let (workspace_id, principal_id) = seed_workspace(&pool).await;
    let item_id = seed_run_work(&pool, workspace_id, principal_id).await;
    let first_outbox_id = Uuid::now_v7();
    for index in 0..33 {
        let id = if index == 0 {
            first_outbox_id
        } else {
            Uuid::now_v7()
        };
        sqlx::query(
            "INSERT INTO outbox (id, workspace_id, topic, payload) VALUES ($1, $2, 'memory.created', '{\"malformed\":true}'::jsonb)",
        )
        .bind(id)
        .bind(workspace_id.as_uuid())
        .execute(&pool)
        .await
        .expect("seed malformed outbox message");
    }
    prepare_worker_runtime_ownership(&pool).await;
    let fixture = WorkerFixture::new();

    let output =
        run_once_with_embedding_handler(&runtime_database_url(&pool), workspace_id, &fixture, true);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a poll failure must take precedence over work completed earlier in the cycle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run_status: String = sqlx::query_scalar("SELECT status FROM run_work_items WHERE id = $1")
        .bind(item_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("read completed run work");
    assert_eq!(
        run_status, "completed",
        "the preceding run poll must still complete work"
    );

    let (attempts, processed, dead_lettered, last_error): (i32, bool, bool, Option<String>) =
        sqlx::query_as(
            "SELECT attempts, processed_at IS NOT NULL, dead_lettered_at IS NOT NULL, last_error \
         FROM outbox WHERE id = $1",
        )
        .bind(first_outbox_id)
        .fetch_one(&pool)
        .await
        .expect("read persisted failed delivery attempt");
    assert_eq!(attempts, 1, "the failed handler attempt must be durable");
    assert!(!processed, "failed delivery must remain pending");
    assert!(!dead_lettered, "first failure must remain retryable");
    assert!(
        last_error
            .as_deref()
            .is_some_and(|error| error.contains("carries no memory_id")),
        "the persisted failure must identify the malformed handler input"
    );

    let attempted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE workspace_id = $1 AND attempts = 1")
            .bind(workspace_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("count one-cycle outbox attempts");
    let untouched: i64 =
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE workspace_id = $1 AND attempts = 0")
            .bind(workspace_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("count outbox messages beyond one cycle");
    assert_eq!(
        attempted, 32,
        "one cycle must claim no more than OUTBOX_BATCH"
    );
    assert_eq!(
        untouched, 1,
        "the next message must remain queued for a later cycle"
    );

    let (registered, active): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE stopped_at IS NULL) \
         FROM external_effect_worker_presence WHERE workspace_id = $1",
    )
    .bind(workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("read worker presence cleanup");
    assert_eq!(
        registered, 1,
        "the child must register its real worker presence"
    );
    assert_eq!(
        active, 0,
        "failed once cycle must still clear worker presence"
    );
}

#[test]
fn once_exits_one_when_the_database_is_unreachable() {
    let fixture = WorkerFixture::new();
    let output = run_once(UNAVAILABLE_DATABASE_URL, WorkspaceId::new(), &fixture);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a failed worker pass must not be reported as idle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
