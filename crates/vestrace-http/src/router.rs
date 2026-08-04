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
use vestrace_application::{
    HealthRepository, RunCommandExecutor, RunUseCases, SharedRunCommandExecutor, SharedRunUseCases,
};

use crate::health;

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const CORRELATION_ID_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

#[derive(Clone)]
pub struct AppState {
    health_repository: Arc<dyn HealthRepository>,
    run_use_cases: SharedRunUseCases,
    run_command_executor: SharedRunCommandExecutor,
}

impl AppState {
    pub fn new(
        health_repository: Arc<dyn HealthRepository>,
        run_use_cases: SharedRunUseCases,
        run_command_executor: SharedRunCommandExecutor,
    ) -> Self {
        Self {
            health_repository,
            run_use_cases,
            run_command_executor,
        }
    }

    pub(crate) fn health_repository(&self) -> &dyn HealthRepository {
        self.health_repository.as_ref()
    }

    pub(crate) fn run_use_cases(&self) -> &dyn RunUseCases {
        self.run_use_cases.as_ref()
    }

    pub fn run_command_executor(&self) -> &dyn RunCommandExecutor {
        self.run_command_executor.as_ref()
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest("/v1", crate::api::api_routes())
        .nest("/ag-ui", crate::api::ag_ui::ag_ui_routes())
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
        p if p.starts_with("/v1") || p.starts_with("/ag-ui") => "api",
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
