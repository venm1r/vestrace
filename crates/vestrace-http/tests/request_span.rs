use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use tracing::{Subscriber, field::Visit, span::Attributes};
use tracing_subscriber::{Layer, layer::Context, prelude::*};
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_http::{AppState, build_router};

const INVALID_REQUEST_ID: &str = "invalid-request-id-secret";
const INVALID_CORRELATION_ID: &str = "invalid-correlation-id-secret";
const SECRET_PATH: &str = "/customers/path-secret-7731";
const BODY_SECRET: &str = "request-body-secret-1882";
const AUTH_SECRET: &str = "Bearer authorization-secret-9642";
const COOKIE_SECRET: &str = "session=cookie-secret-5104";
const HEADER_SECRET: &str = "arbitrary-header-secret-2267";

struct HealthyRepository;

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct RequestSpanCapture(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

impl RequestSpanCapture {
    fn spans(&self) -> Vec<BTreeMap<String, String>> {
        self.0.lock().unwrap().clone()
    }
}

impl<S> Layer<S> for RequestSpanCapture
where
    S: Subscriber,
{
    fn on_new_span(
        &self,
        attributes: &Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        if attributes.metadata().name() != "http_request" {
            return;
        }

        let mut fields = BTreeMap::new();
        attributes.record(&mut FieldVisitor(&mut fields));
        self.0.lock().unwrap().push(fields);
    }
}

struct FieldVisitor<'fields>(&'fields mut BTreeMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

#[test]
fn request_span_contains_only_sanitized_bounded_metadata() {
    let capture = RequestSpanCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let response = tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(
            build_router(AppState::new(Arc::new(HealthyRepository))).oneshot(
                Request::get(SECRET_PATH)
                    .header("x-request-id", INVALID_REQUEST_ID)
                    .header("x-correlation-id", INVALID_CORRELATION_ID)
                    .header("authorization", AUTH_SECRET)
                    .header("cookie", COOKIE_SECRET)
                    .header("x-arbitrary", HEADER_SECRET)
                    .body(Body::from(BODY_SECRET))
                    .unwrap(),
            ),
        )
    })
    .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let request_id = response.headers()["x-request-id"].to_str().unwrap();
    let correlation_id = response.headers()["x-correlation-id"].to_str().unwrap();
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

    let spans = capture.spans();
    assert_eq!(spans.len(), 1, "{spans:?}");
    let span = &spans[0];
    assert_eq!(span.get("request_id").unwrap(), request_id);
    assert_eq!(span.get("correlation_id").unwrap(), correlation_id);
    assert_eq!(span.get("route").unwrap(), "unmatched");

    let recorded = format!("{span:?}");
    for secret in [
        INVALID_REQUEST_ID,
        INVALID_CORRELATION_ID,
        SECRET_PATH,
        BODY_SECRET,
        AUTH_SECRET,
        COOKIE_SECRET,
        HEADER_SECRET,
    ] {
        assert!(
            !recorded.contains(secret),
            "span leaked {secret}: {recorded}"
        );
    }
}
