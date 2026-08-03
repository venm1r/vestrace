use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_http::{AppState, build_router};

struct HealthyRepository;

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

fn app() -> axum::Router {
    build_router(AppState::new(Arc::new(HealthyRepository)))
}

#[tokio::test]
async fn v1_is_applied_exactly_once() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

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
