use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{
    ApplicationError, CreateRunCommand, HealthRepository, RequestContext, RunUseCases,
};
use vestrace_domain::{id::AgentRunId, run::AgentRun};
use vestrace_http::{AppState, build_router};

struct HealthyRepository;
struct EmptyRunUseCases;

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl RunUseCases for EmptyRunUseCases {
    async fn create_run(
        &self,
        _context: &RequestContext,
        _command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "run creation is unavailable in this test".to_owned(),
        ))
    }

    async fn list_runs(
        &self,
        _context: &RequestContext,
        _limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn get_run(
        &self,
        _context: &RequestContext,
        _id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        Ok(None)
    }
}

fn app() -> axum::Router {
    build_router(AppState::new(
        Arc::new(HealthyRepository),
        Arc::new(EmptyRunUseCases),
    ))
}

#[tokio::test]
async fn v1_is_applied_exactly_once() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let duplicated = app()
        .oneshot(
            Request::builder()
                .uri("/v1/v1/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicated.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ag_ui_is_explicitly_unavailable() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ag-ui/run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}
