use sqlx::PgPool;
use vestrace_application::{
    ApplicationError, RequestContext, WorkspaceSettingsRepository, WorkspaceSettingsService,
};
use vestrace_domain::{LogLevel, PrincipalId, WorkspaceId, WorkspaceSettings};
use vestrace_infrastructure::{PgStore, PgWorkspaceSettingsRepository};

async fn seed_workspace(pool: &PgPool, workspace_id: WorkspaceId) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("settings-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
}

fn context(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
}

/// A workspace that was never configured reads as the conservative defaults, so
/// no caller has to distinguish "absent" from "unset".
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn unconfigured_workspace_reads_conservative_defaults(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgWorkspaceSettingsRepository::new(PgStore::from_pool(pool));

    let settings = repository.load(&context(workspace)).await.unwrap();

    assert_eq!(settings, WorkspaceSettings::defaults(workspace));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn settings_round_trip_and_advance_their_version(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let service = WorkspaceSettingsService::new(std::sync::Arc::new(
        PgWorkspaceSettingsRepository::new(PgStore::from_pool(pool)),
    ));
    let context = context(workspace);

    let updated = service
        .update(&context, 1, 12, 7_500_000, LogLevel::Debug)
        .await
        .unwrap();

    assert_eq!(updated.max_concurrent_runs(), 12);
    assert_eq!(updated.run_budget_cap_micros(), 7_500_000);
    assert_eq!(updated.log_level(), LogLevel::Debug);
    assert_eq!(updated.version(), 2);

    let reloaded = service.get(&context).await.unwrap();
    assert_eq!(reloaded, updated);
}

/// The second writer must be told, not silently overwrite the first.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_stale_version_is_rejected_without_writing(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let service = WorkspaceSettingsService::new(std::sync::Arc::new(
        PgWorkspaceSettingsRepository::new(PgStore::from_pool(pool)),
    ));
    let context = context(workspace);

    service
        .update(&context, 1, 4, 0, LogLevel::Warn)
        .await
        .unwrap();

    let error = service
        .update(&context, 1, 99, 0, LogLevel::Trace)
        .await
        .unwrap_err();

    assert!(
        matches!(error, ApplicationError::Conflict(_)),
        "unexpected error: {error:?}"
    );
    let unchanged = service.get(&context).await.unwrap();
    assert_eq!(unchanged.max_concurrent_runs(), 4);
    assert_eq!(unchanged.version(), 2);
}

/// Invalid input must be refused by the database as well as the domain, so a
/// direct writer cannot leave a value the domain would reject on read.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn the_schema_refuses_values_the_domain_would_reject(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;

    let error = sqlx::query(
        "INSERT INTO workspace_settings
             (workspace_id, max_concurrent_runs, run_budget_cap_micros, log_level, version)
         VALUES ($1, 0, 0, 'info', 1)",
    )
    .bind(workspace.as_uuid())
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("chk_workspace_settings_concurrency"),
        "unexpected error: {error}"
    );
}

/// Settings are workspace-scoped like every other row in the system.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn settings_do_not_leak_across_workspaces(pool: PgPool) {
    let first = WorkspaceId::new();
    let second = WorkspaceId::new();
    seed_workspace(&pool, first).await;
    seed_workspace(&pool, second).await;
    let repository = PgWorkspaceSettingsRepository::new(PgStore::from_pool(pool));
    let service = WorkspaceSettingsService::new(std::sync::Arc::new(repository));

    service
        .update(&context(first), 1, 9, 0, LogLevel::Error)
        .await
        .unwrap();

    let other = service.get(&context(second)).await.unwrap();
    assert_eq!(other, WorkspaceSettings::defaults(second));
}
