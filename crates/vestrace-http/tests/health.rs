use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_http::{AppState, build_router};

const VALID_REQUEST_ID: &str = "01890f3e-7b28-7c00-8000-000000000001";
const VALID_CORRELATION_ID: &str = "01890f3e-7b28-7c00-8000-000000000002";

struct FakeHealthRepository {
    available: bool,
    checks: AtomicUsize,
}

impl FakeHealthRepository {
    fn new(available: bool) -> Self {
        Self {
            available,
            checks: AtomicUsize::new(0),
        }
    }

    fn check_count(&self) -> usize {
        self.checks.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HealthRepository for FakeHealthRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.available {
            Ok(())
        } else {
            Err(ApplicationError::Unavailable(
                "postgres://secret-user:secret-password@database:5432/vestrace: raw SQL failure"
                    .to_owned(),
            ))
        }
    }
}

fn router(repository: Arc<FakeHealthRepository>) -> axum::Router {
    build_router(AppState::new(repository))
}

async fn body(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn live_returns_exact_safe_body_without_checking_dependencies() {
    let repository = Arc::new(FakeHealthRepository::new(false));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, r#"{"status":"ok"}"#);
    assert_eq!(repository.check_count(), 0);
}

#[tokio::test]
async fn ready_returns_exact_ok_body_when_repository_is_healthy() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, r#"{"status":"ok"}"#);
    assert_eq!(repository.check_count(), 1);
}

#[tokio::test]
async fn ready_returns_opaque_not_ready_body_when_repository_fails() {
    let repository = Arc::new(FakeHealthRepository::new(false));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body(response).await;
    assert_eq!(body, r#"{"status":"not_ready"}"#);
    assert!(!body.contains("secret"));
    assert!(!body.contains("SQL"));
    assert_eq!(repository.check_count(), 1);
}

#[tokio::test]
async fn valid_request_and_correlation_ids_are_propagated() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository)
        .oneshot(
            Request::get("/health/live")
                .header("x-request-id", VALID_REQUEST_ID)
                .header("x-correlation-id", VALID_CORRELATION_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.headers()["x-request-id"], VALID_REQUEST_ID);
    assert_eq!(response.headers()["x-correlation-id"], VALID_CORRELATION_ID);
}

#[tokio::test]
async fn missing_or_invalid_ids_are_replaced_with_uuid_v7_values() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository)
        .oneshot(
            Request::get("/health/live")
                .header("x-request-id", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let request_id = response.headers()["x-request-id"].to_str().unwrap();
    let correlation_id = response.headers()["x-correlation-id"].to_str().unwrap();
    assert_ne!(request_id, "not-a-uuid");
    assert_eq!(
        uuid::Uuid::parse_str(request_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(
        uuid::Uuid::parse_str(correlation_id)
            .unwrap()
            .get_version_num(),
        7
    );
}
