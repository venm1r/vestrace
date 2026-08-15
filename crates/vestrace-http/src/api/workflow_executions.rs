use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, context::request_context};

pub fn workflow_execution_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/workflow-executions", post(start_execution))
        .route(
            "/workflow-executions/{id}",
            axum::routing::get(get_execution),
        )
        .route(
            "/workflow-executions/{id}/steps",
            axum::routing::post(record_step).get(list_steps),
        )
        .route(
            "/workflow-executions/{id}/complete",
            post(complete_execution),
        )
        .route(
            "/workflow-executions/{id}/outcomes",
            axum::routing::post(record_outcome).get(list_outcomes),
        )
}

#[derive(Debug, Deserialize)]
pub struct StartExecutionRequest {
    pub workflow_id: Uuid,
    pub workflow_revision: u32,
    pub workflow_revision_id: Uuid,
    pub attempt: u32,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StartExecutionResponse {
    pub execution_id: Uuid,
}

async fn start_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StartExecutionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let id = vestrace_domain::id::WorkflowExecutionId::new();
    let now = vestrace_domain::time::now();

    let record = vestrace_application::WorkflowExecutionRecord {
        id,
        workspace_id: ctx.workspace_id,
        workflow_id: vestrace_domain::id::WorkflowId::from_uuid(req.workflow_id),
        workflow_revision: req.workflow_revision,
        workflow_revision_id: vestrace_domain::id::WorkflowRevisionId::from_uuid(
            req.workflow_revision_id,
        ),
        status: vestrace_domain::ExecutionStatus::Queued,
        attempt: req.attempt,
        started_at: now,
        completed_at: None,
        correlation_id: req.correlation_id,
        causation_id: req.causation_id,
        run_id: None,
    };

    state
        .execution_history_repository()
        .start_workflow_execution(&ctx, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(StartExecutionResponse {
            execution_id: id.as_uuid(),
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CompleteExecutionRequest {
    pub status: String,
}

async fn complete_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteExecutionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(id);
    let now = vestrace_domain::time::now();

    let status = match req.status.as_str() {
        "succeeded" => vestrace_domain::ExecutionStatus::Succeeded,
        "failed" => vestrace_domain::ExecutionStatus::Failed,
        "cancelled" => vestrace_domain::ExecutionStatus::Cancelled,
        _ => return Err(ApiError::bad_request("invalid status")),
    };

    state
        .execution_history_repository()
        .update_workflow_execution_status(&ctx, exec_id, status, Some(now))
        .await
        .map_err(ApiError::from_application)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(id);

    let record = state
        .execution_history_repository()
        .get_workflow_execution(&ctx, exec_id)
        .await
        .map_err(ApiError::from_application)?;

    match record {
        Some(r) => Ok(Json(serde_json::json!({
            "id": r.id.as_uuid(),
            "workflow_id": r.workflow_id.as_uuid(),
            "status": format!("{:?}", r.status),
            "attempt": r.attempt,
            "started_at": r.started_at,
            "completed_at": r.completed_at,
        }))),
        None => Err(ApiError::not_found("execution not found")),
    }
}

#[derive(Debug, Deserialize)]
pub struct RecordStepRequest {
    pub node_id: Uuid,
    pub node_label: String,
    pub kind: String,
    pub agent_ref: Option<Uuid>,
    pub attempt: u32,
}

async fn record_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
    Json(req): Json<RecordStepRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let id = vestrace_domain::id::StepExecutionId::new();
    let now = vestrace_domain::time::now();
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(execution_id);

    let kind = match req.kind.as_str() {
        "agent" => vestrace_domain::StepKind::Agent,
        "skill" => vestrace_domain::StepKind::Skill,
        "tool" => vestrace_domain::StepKind::Tool,
        "decision" => vestrace_domain::StepKind::Decision,
        "parallel" => vestrace_domain::StepKind::Parallel,
        "join" => vestrace_domain::StepKind::Join,
        "human_approval" => vestrace_domain::StepKind::HumanApproval,
        "sub_workflow" => vestrace_domain::StepKind::SubWorkflow,
        "end" => vestrace_domain::StepKind::End,
        _ => return Err(ApiError::bad_request("invalid step kind")),
    };

    let record = vestrace_application::StepExecutionRecord {
        id,
        workflow_execution_id: exec_id,
        workspace_id: ctx.workspace_id,
        node_id: vestrace_domain::id::WorkflowNodeId::from_uuid(req.node_id),
        node_label: req.node_label,
        kind,
        agent_ref: req.agent_ref.map(vestrace_domain::id::AgentId::from_uuid),
        attempt: req.attempt,
        status: vestrace_domain::ExecutionStatus::Queued,
        input_artifact_id: None,
        output_artifact_id: None,
        model_attempt_id: None,
        tool_invocation_id: None,
        error_message: None,
        started_at: now,
        completed_at: None,
        run_id: None,
    };

    state
        .execution_history_repository()
        .record_step(&ctx, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "step_id": id.as_uuid() })),
    ))
}

async fn list_steps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(execution_id);

    let steps = state
        .execution_history_repository()
        .list_steps(&ctx, exec_id)
        .await
        .map_err(ApiError::from_application)?;

    let result: Vec<_> = steps
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id.as_uuid(),
                "node_id": s.node_id.as_uuid(),
                "node_label": s.node_label,
                "kind": format!("{:?}", s.kind),
                "attempt": s.attempt,
                "status": format!("{:?}", s.status),
                "started_at": s.started_at,
                "completed_at": s.completed_at,
                "error_message": s.error_message,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct RecordOutcomeRequest {
    pub step_execution_id: Option<Uuid>,
    pub outcome_kind: String,
    pub summary: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
}

async fn record_outcome(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
    Json(req): Json<RecordOutcomeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let id = vestrace_domain::id::ExecutionOutcomeId::new();
    let now = vestrace_domain::time::now();
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(execution_id);

    let outcome_kind = match req.outcome_kind.as_str() {
        "success" => vestrace_domain::OutcomeKind::Success,
        "failure" => vestrace_domain::OutcomeKind::Failure,
        "cancelled" => vestrace_domain::OutcomeKind::Cancelled,
        _ => return Err(ApiError::bad_request("invalid outcome kind")),
    };

    let record = vestrace_application::ExecutionOutcomeRecord {
        id,
        workspace_id: ctx.workspace_id,
        workflow_execution_id: exec_id,
        step_execution_id: req
            .step_execution_id
            .map(vestrace_domain::id::StepExecutionId::from_uuid),
        outcome_kind,
        summary: req.summary,
        error_code: req.error_code,
        error_detail: req.error_detail,
        created_at: now,
    };

    state
        .execution_history_repository()
        .record_outcome(&ctx, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "outcome_id": id.as_uuid() })),
    ))
}

async fn list_outcomes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let exec_id = vestrace_domain::id::WorkflowExecutionId::from_uuid(execution_id);

    let outcomes = state
        .execution_history_repository()
        .list_outcomes(&ctx, exec_id)
        .await
        .map_err(ApiError::from_application)?;

    let result: Vec<_> = outcomes
        .into_iter()
        .map(|o| {
            serde_json::json!({
                "id": o.id.as_uuid(),
                "step_execution_id": o.step_execution_id.map(|s| s.as_uuid()),
                "outcome_kind": format!("{:?}", o.outcome_kind),
                "summary": o.summary,
                "error_code": o.error_code,
                "error_detail": o.error_detail,
                "created_at": o.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}
