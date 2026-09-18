use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use vestrace_application::ModelExecutionRecord;
use vestrace_domain::{id::ModelExecutionId, time::now};

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct RecordExecutionRequest {
    pub model_id: uuid::Uuid,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub latency_ms: u32,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: uuid::Uuid,
    pub model_id: uuid::Uuid,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub latency_ms: u32,
    pub status: String,
}

impl From<ModelExecutionRecord> for ExecutionResponse {
    fn from(e: ModelExecutionRecord) -> Self {
        Self {
            id: e.id.as_uuid(),
            model_id: e.model_id.as_uuid(),
            prompt_tokens: e.prompt_tokens,
            completion_tokens: e.completion_tokens,
            latency_ms: e.latency_ms,
            status: e.status,
        }
    }
}

pub async fn record_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RecordExecutionRequest>,
) -> Result<(StatusCode, Json<ExecutionResponse>), ApiError> {
    let context = request_context(&headers)?;
    let id = ModelExecutionId::new();
    let record = ModelExecutionRecord {
        id,
        workspace_id: context.workspace_id,
        model_id: vestrace_domain::id::ModelId::from_uuid(req.model_id),
        prompt_tokens: req.prompt_tokens,
        completion_tokens: req.completion_tokens,
        latency_ms: req.latency_ms,
        status: req.status,
        created_at: now(),
    };
    state
        .model_execution_repository()
        .record(&context, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(ExecutionResponse::from(record))))
}

pub async fn list_executions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ExecutionResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let executions = state
        .model_execution_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(
        executions
            .into_iter()
            .map(ExecutionResponse::from)
            .collect(),
    ))
}
