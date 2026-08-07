use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, context::request_context};

pub fn workflow_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/workflows", post(create_workflow).get(list_workflows))
        .route("/workflows/{id}", get(get_workflow))
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkflowResponse {
    pub workflow_id: Uuid,
}

async fn create_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let id = vestrace_domain::id::WorkflowId::new();
    let now = vestrace_domain::time::now();

    let record = vestrace_application::WorkflowDefinitionRecord {
        id,
        workspace_id: ctx.workspace_id,
        name: req.name,
        current_revision: 1,
        created_at: now,
    };

    state
        .workflow_repository()
        .create(&ctx, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateWorkflowResponse {
            workflow_id: id.as_uuid(),
        }),
    ))
}

async fn list_workflows(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;

    let workflows = state
        .workflow_repository()
        .list(&ctx)
        .await
        .map_err(ApiError::from_application)?;

    let result: Vec<_> = workflows
        .into_iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id.as_uuid(),
                "name": w.name,
                "current_revision": w.current_revision,
                "created_at": w.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

async fn get_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let wf_id = vestrace_domain::id::WorkflowId::from_uuid(id);

    let record = state
        .workflow_repository()
        .find_by_id(&ctx, wf_id)
        .await
        .map_err(ApiError::from_application)?;

    match record {
        Some(w) => Ok(Json(serde_json::json!({
            "id": w.id.as_uuid(),
            "name": w.name,
            "current_revision": w.current_revision,
            "created_at": w.created_at,
        }))),
        None => Err(ApiError::not_found("workflow not found")),
    }
}
