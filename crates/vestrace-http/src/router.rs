use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, Request},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use tracing::Instrument;
use uuid::{Uuid, Version};
use vestrace_application::HealthRepository;

use crate::health;

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const CORRELATION_ID_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

#[derive(Clone)]
pub struct AppState {
    health_repository: Arc<dyn HealthRepository>,
}

impl AppState {
    pub fn new(health_repository: Arc<dyn HealthRepository>) -> Self {
        Self { health_repository }
    }

    pub(crate) fn health_repository(&self) -> &dyn HealthRepository {
        self.health_repository.as_ref()
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .with_state(state)
        .layer(middleware::from_fn(add_request_context))
}

async fn add_request_context(mut request: Request<Body>, next: Next) -> Response {
    let request_id = validated_id(request.headers(), &REQUEST_ID_HEADER);
    let correlation_id = validated_id(request.headers(), &CORRELATION_ID_HEADER);
    let request_id_value = header_value(request_id);
    let correlation_id_value = header_value(correlation_id);

    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_value.clone());
    request
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, correlation_id_value.clone());
    let route = match request.uri().path() {
        "/health/live" => "/health/live",
        "/health/ready" => "/health/ready",
        _ => "unmatched",
    };

    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        correlation_id = %correlation_id,
        method = %request.method(),
        route,
    );
    let mut response = next.run(request).instrument(span).await;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_value);
    response
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, correlation_id_value);
    response
}

fn validated_id(headers: &HeaderMap, name: &HeaderName) -> Uuid {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| value.get_version() == Some(Version::SortRand))
        .unwrap_or_else(Uuid::now_v7)
}

fn header_value(id: Uuid) -> HeaderValue {
    HeaderValue::from_str(&id.to_string()).expect("a UUID is always a valid HTTP header value")
}
