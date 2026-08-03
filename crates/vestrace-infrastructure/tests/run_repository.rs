use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{RequestContext, RunRepository};
use vestrace_domain::{PrincipalId, WorkspaceId, id::AgentRunId, run::AgentRun};
use vestrace_infrastructure::{PgRunRepository, PgStore};

#[sqlx::test(migrations = "../../migrations")]
async fn run_repository_isolates_workspaces(pool: PgPool) {
    let workspace_a = WorkspaceId::new();
    let workspace_b = WorkspaceId::new();
    let principal_a = PrincipalId::new();
    let principal_b = PrincipalId::new();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, slug)
        VALUES
            ($1, 'workspace-a'),
            ($2, 'workspace-b')
        "#,
    )
    .bind(workspace_a.as_uuid())
    .bind(workspace_b.as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO principals (id, workspace_id, identifier)
        VALUES
            ($1, $2, 'principal-a'),
            ($3, $4, 'principal-b')
        "#,
    )
    .bind(principal_a.as_uuid())
    .bind(workspace_a.as_uuid())
    .bind(principal_b.as_uuid())
    .bind(workspace_b.as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    let repository = PgRunRepository::new(PgStore::from_pool(pool));
    let context_a = RequestContext::new(workspace_a, principal_a);
    let context_b = RequestContext::new(workspace_b, principal_b);
    let at = Utc.with_ymd_and_hms(2026, 8, 4, 0, 0, 0).single().unwrap();
    let run = AgentRun::new(
        AgentRunId::new(),
        workspace_a,
        principal_a,
        "Workspace A run",
        at,
    );

    repository.create(&context_a, &run).await.unwrap();
    assert_eq!(
        repository.list(&context_a, 50).await.unwrap(),
        vec![run.clone()]
    );
    assert_eq!(
        repository.find_by_id(&context_a, run.id).await.unwrap(),
        Some(run.clone())
    );
    assert!(repository.list(&context_b, 50).await.unwrap().is_empty());
    assert_eq!(
        repository.find_by_id(&context_b, run.id).await.unwrap(),
        None
    );
}
