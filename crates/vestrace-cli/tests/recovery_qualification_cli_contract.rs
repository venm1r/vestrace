use std::process::{Command, Output};

use sqlx::PgPool;

fn vestrace(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(args)
        .output()
        .expect("vestrace command should start")
}

#[test]
fn recovery_qualification_command_names_its_ephemeral_run_boundary() {
    let output = vestrace(&["conformance", "recovery-qualification", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for required in ["--workspace-id", "--principal-id", "--isolation"] {
        assert!(
            stdout.contains(required),
            "missing {required} in:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("--database-url"),
        "database credentials must come from configuration, not argv:\n{stdout}"
    );
}

#[test]
fn recovery_qualification_command_refuses_a_non_ephemeral_isolation() {
    let output = vestrace(&[
        "conformance",
        "recovery-qualification",
        "--workspace-id",
        "10000000-0000-0000-0000-000000000001",
        "--principal-id",
        "10000000-0000-0000-0000-000000000002",
        "--isolation",
        "designated-non-production",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("invalid value 'designated-non-production'"),
        "{stderr}"
    );
    assert!(stderr.contains("ephemeral"), "{stderr}");
}

#[test]
fn recovery_qualification_uses_configured_credentials_without_disclosing_them() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "recovery-qualification",
            "--workspace-id",
            "10000000-0000-0000-0000-000000000001",
            "--principal-id",
            "10000000-0000-0000-0000-000000000002",
            "--isolation",
            "ephemeral",
        ])
        .env(
            "VESTRACE_DATABASE__URL",
            "postgres://recovery-user:recovery-secret@127.0.0.1:9/unreachable",
        )
        .output()
        .expect("vestrace command should start");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("recovery qualification evidence database is unavailable"),
        "{stderr}"
    );
    assert!(!stderr.contains("recovery-user"), "{stderr}");
    assert!(!stderr.contains("recovery-secret"), "{stderr}");
}

async fn ephemeral_database_url(pool: &PgPool) -> String {
    let base = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must name the PostgreSQL used by #[sqlx::test]");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    let (base, query) = match base.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (base.as_str(), None),
    };
    let (prefix, _) = base.rsplit_once('/').unwrap();
    match query {
        Some(query) => format!("{prefix}/{database}?{query}"),
        None => format!("{prefix}/{database}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn recovery_qualification_runs_every_target_once_through_canonical_runs(pool: PgPool) {
    let workspace_id = vestrace_domain::WorkspaceId::new().as_uuid();
    let principal_id = vestrace_domain::PrincipalId::new().as_uuid();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id)
        .bind(format!("recovery-qualification-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("recovery-qualification-{principal_id}"))
        .execute(&pool)
        .await
        .unwrap();

    let database_url = ephemeral_database_url(&pool).await;
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "recovery-qualification",
            "--workspace-id",
            &workspace_id.to_string(),
            "--principal-id",
            &principal_id.to_string(),
            "--isolation",
            "ephemeral",
        ])
        .env("VESTRACE_DATABASE__URL", &database_url)
        .output()
        .expect("vestrace command should start");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["observation_count"], 8);

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT target, classification, action
         FROM recovery_qualification_observations
         ORDER BY target",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "dispatching_external_effect".into(),
                "must_reconcile".into(),
                "reconcile".into()
            ),
            (
                "divergent_history".into(),
                "human_required".into(),
                "human_review".into()
            ),
            (
                "orphan_temporary_state".into(),
                "safe_to_retry".into(),
                "retry".into()
            ),
            (
                "running_execution".into(),
                "safe_to_resume".into(),
                "resume".into()
            ),
            ("stale_lease".into(), "safe_to_retry".into(), "retry".into()),
            (
                "unfinished_workflow".into(),
                "safe_to_resume".into(),
                "resume".into()
            ),
            (
                "unknown_outcome".into(),
                "must_reconcile".into(),
                "reconcile".into()
            ),
            (
                "verifying_repair".into(),
                "must_abort".into(),
                "abort".into()
            ),
        ]
    );
    let canonical_runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run_streams WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(canonical_runs, 8);

    let repeated = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "recovery-qualification",
            "--workspace-id",
            &workspace_id.to_string(),
            "--principal-id",
            &principal_id.to_string(),
            "--isolation",
            "ephemeral",
        ])
        .env("VESTRACE_DATABASE__URL", database_url)
        .output()
        .expect("vestrace command should start");
    assert!(!repeated.status.success());
    assert!(
        String::from_utf8_lossy(&repeated.stderr)
            .contains("requires an empty recovery qualification evidence set")
    );
    let preserved: i64 =
        sqlx::query_scalar("SELECT count(*) FROM recovery_qualification_observations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved, 8);
    let preserved_runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run_streams WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved_runs, 8);
}
