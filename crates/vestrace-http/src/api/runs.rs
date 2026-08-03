use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use vestrace_application::CreateRunCommand;
use vestrace_domain::{
    Timestamp,
    id::AgentRunId,
    run::{AgentRun, RunStatus},
};

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: RunStatus,
    pub version: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl From<AgentRun> for RunResponse {
    fn from(run: AgentRun) -> Self {
        Self {
            id: run.id.as_uuid(),
            title: run.title,
            status: run.status,
            version: run.version.value(),
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}

pub async fn create_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let context = request_context(&headers)?;
    let run = state
        .run_use_cases()
        .create_run(
            &context,
            CreateRunCommand {
                title: request.title,
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(run.into())))
}

pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RunResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let runs = state
        .run_use_cases()
        .list_runs(&context, 50)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(runs.into_iter().map(RunResponse::from).collect()))
}

pub async fn get_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let id = id
        .parse::<AgentRunId>()
        .map_err(|_| ApiError::bad_request("run id must be a UUID"))?;
    let run = state
        .run_use_cases()
        .get_run(&context, id)
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| ApiError::not_found("run"))?;

    Ok(Json(run.into()))
}
