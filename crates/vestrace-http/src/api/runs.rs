use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::{
    Timestamp, now,
    id::{AgentRunId, CorrelationId, OperationId},
    run::{AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunStatus, RunVersion},
};

use crate::AppState;

use super::{ApiError, context::request_context};

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";

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
    let run_id = AgentRunId::new();
    let command = RunCommandEnvelope {
        command_id: OperationId::from_uuid(required_uuid_header(&headers, REQUEST_ID_HEADER)?),
        idempotency_key: None,
        workspace_id: context.workspace_id,
        run_id,
        actor: RunActor::Principal(context.principal_id),
        expected_version: RunVersion::ZERO,
        correlation_id: CorrelationId::from_uuid(required_uuid_header(
            &headers,
            CORRELATION_ID_HEADER,
        )?),
        issued_at: now(),
        command: RunCommand::Create {
            principal_id: context.principal_id,
            title: request.title,
        },
    };
    let result = state
        .run_command_executor()
        .execute(&context, command)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(result.run.into())))
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

fn required_uuid_header(headers: &HeaderMap, name: &'static str) -> Result<Uuid, ApiError> {
    let value = headers
        .get(name)
        .ok_or_else(|| ApiError::bad_request(format!("missing {name} header")))?;
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request(format!("{name} header must be valid UTF-8")))?;
    Uuid::parse_str(value)
        .map_err(|_| ApiError::bad_request(format!("{name} header must be a UUID")))
}
