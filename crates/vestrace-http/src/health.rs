use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    status: &'static str,
}

pub(crate) async fn live() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub(crate) async fn ready(State(state): State<AppState>) -> Response {
    match state.health_repository().check().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ok" })).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
            }),
        )
            .into_response(),
    }
}
