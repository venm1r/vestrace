use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Sse, sse::Event},
    routing::{get, post},
    Json,
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{convert::Infallible, time::Duration};
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

pub fn ag_ui_routes() -> Router<AppState> {
    Router::new()
        .route("/ag-ui/endpoints", get(list_endpoints).post(create_endpoint))
        .route("/ag-ui/events/stream", get(ag_ui_event_stream))
        .route("/ag-ui/run", post(ag_ui_run_agent))
}

#[derive(Debug, Deserialize)]
pub struct CreateEndpointPayload {
    pub name: String,
    pub endpoint_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AgUiRunInput {
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub message: String,
}

async fn list_endpoints() -> impl IntoResponse {
    let endpoints = json!([
        {
            "id": "agui_0194f4a0-7b3c-7000-8000-000000000001",
            "name": "Primary Interactive Agent Gateway",
            "endpoint_url": "http://127.0.0.1:8080/ag-ui/events/stream",
            "enabled": true,
            "created_at": "2026-08-03T10:00:00Z"
        }
    ]);
    (StatusCode::OK, Json(endpoints))
}

async fn create_endpoint(Json(payload): Json<CreateEndpointPayload>) -> impl IntoResponse {
    let endpoint = json!({
        "id": format!("agui_{}", Uuid::now_v7()),
        "name": payload.name,
        "endpoint_url": payload.endpoint_url,
        "enabled": true,
        "created_at": chrono::Utc::now().to_rfc3339()
    });
    (StatusCode::CREATED, Json(endpoint))
}

async fn ag_ui_run_agent(Json(input): Json<AgUiRunInput>) -> impl IntoResponse {
    let run_id = input.run_id.unwrap_or_else(|| format!("run_{}", Uuid::now_v7()));
    let response = json!({
        "event": "RunAgentOutput",
        "run_id": run_id,
        "status": "Accepted",
        "ag_ui_event_stream": "/ag-ui/events/stream",
        "message": format!("Processing message through AG-UI boundary: {}", input.message)
    });
    (StatusCode::ACCEPTED, Json(response))
}

async fn ag_ui_event_stream() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = vec![
        Event::default().event("ag_ui.run.started").data(json!({
            "run_id": "0194f4a0-7b3c-7000-8000-000000000001",
            "status": "Running",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }).to_string()),
        Event::default().event("ag_ui.step.delta").data(json!({
            "step": 1,
            "action": "Evaluating RLS Policy Isolation",
            "status": "Completed"
        }).to_string()),
        Event::default().event("ag_ui.activity.heartbeat").data(json!({
            "health": "Healthy",
            "active_connections": 1
        }).to_string()),
    ];

    let stream = stream::iter(events).map(Ok);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
}
