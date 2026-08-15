use chrono::{Duration, Utc};
use sqlx::PgPool;
use vestrace_application::{RequestContext, StartupRecoveryCandidateSource};
use vestrace_domain::{PrincipalId, RecoveryTarget, WorkspaceId};
use vestrace_infrastructure::{PgStartupRecoverySource, PgStore};

async fn seed_workspace(pool: &PgPool, workspace_id: WorkspaceId) -> uuid::Uuid {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("workspace-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    let principal_id = PrincipalId::new().as_uuid();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id)
        .bind(workspace_id.as_uuid())
        .bind(format!("principal-{principal_id}"))
        .execute(pool)
        .await
        .unwrap();
    principal_id
}

async fn seed_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    principal_id: uuid::Uuid,
    status: &str,
) -> uuid::Uuid {
    let run_id = vestrace_domain::AgentRunId::new().as_uuid();
    let terminal = matches!(
        status,
        "succeeded" | "succeeded_with_warnings" | "partial" | "failed" | "cancelled" | "expired"
    );
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, principal_id, title, objective, status, finished_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run_id)
    .bind(workspace_id.as_uuid())
    .bind(principal_id)
    .bind("startup recovery")
    .bind("startup recovery")
    .bind(status)
    .bind(terminal.then(Utc::now))
    .execute(pool)
    .await
    .unwrap();
    run_id
}

async fn seed_lease(pool: &PgPool, workspace_id: WorkspaceId, run_id: uuid::Uuid, expired: bool) {
    let lease_until = if expired {
        Utc::now() - Duration::minutes(5)
    } else {
        Utc::now() + Duration::minutes(5)
    };
    sqlx::query(
        "INSERT INTO run_leases (run_id, workspace_id, worker_id, lease_until)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(run_id)
    .bind(workspace_id.as_uuid())
    .bind("worker-1")
    .bind(lease_until)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_recovery_discovers_only_interrupted_runs_in_the_workspace(pool: PgPool) {
    let workspace = WorkspaceId::new();
    let other_workspace = WorkspaceId::new();
    let principal = seed_workspace(&pool, workspace).await;
    let other_principal = seed_workspace(&pool, other_workspace).await;

    let expired_lease = seed_run(&pool, workspace, principal, "running").await;
    seed_lease(&pool, workspace, expired_lease, true).await;

    let live_lease = seed_run(&pool, workspace, principal, "running").await;
    seed_lease(&pool, workspace, live_lease, false).await;

    let unleased_executing = seed_run(&pool, workspace, principal, "preparing").await;

    // Not interrupted work: never started, or already finished.
    let _created = seed_run(&pool, workspace, principal, "created").await;
    let terminal = seed_run(&pool, workspace, principal, "succeeded").await;
    seed_lease(&pool, workspace, terminal, true).await;

    // Another workspace must never leak into this sweep.
    let foreign = seed_run(&pool, other_workspace, other_principal, "running").await;
    seed_lease(&pool, other_workspace, foreign, true).await;

    let source = PgStartupRecoverySource::new(PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, PrincipalId::new());

    let candidates = source
        .find_startup_recovery_candidates(&context)
        .await
        .unwrap();

    let mut found: Vec<(uuid::Uuid, RecoveryTarget)> = candidates
        .iter()
        .map(|candidate| (candidate.run_id.as_uuid(), candidate.target))
        .collect();
    found.sort_by_key(|(id, _)| *id);

    let mut expected = vec![
        (expired_lease, RecoveryTarget::StaleLease),
        (unleased_executing, RecoveryTarget::UnknownOutcome),
    ];
    expected.sort_by_key(|(id, _)| *id);

    assert_eq!(found, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_recovery_discovery_is_empty_when_nothing_was_interrupted(pool: PgPool) {
    let workspace = WorkspaceId::new();
    let principal = seed_workspace(&pool, workspace).await;
    let live = seed_run(&pool, workspace, principal, "running").await;
    seed_lease(&pool, workspace, live, false).await;

    let source = PgStartupRecoverySource::new(PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, PrincipalId::new());

    let candidates = source
        .find_startup_recovery_candidates(&context)
        .await
        .unwrap();

    assert!(
        candidates.is_empty(),
        "unexpected candidates: {candidates:?}"
    );
}
