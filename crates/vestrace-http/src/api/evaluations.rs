use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, context::request_context};

pub fn evaluation_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/evaluations",
            post(create_evaluation).get(list_evaluations),
        )
        .route("/evaluations/{id}", get(get_evaluation))
}

#[derive(Debug, Deserialize)]
pub struct CreateEvaluationRequest {
    pub model_id: Option<Uuid>,
    pub name: String,
    pub status: Option<String>,
    pub score: Option<f64>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateEvaluationResponse {
    pub evaluation_id: Uuid,
}

async fn create_evaluation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateEvaluationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let id = vestrace_domain::id::EvaluationId::new();
    let now = vestrace_domain::time::now();

    let record = vestrace_application::EvaluationRecord {
        id,
        workspace_id: ctx.workspace_id,
        model_id: req.model_id.map(vestrace_domain::id::ModelId::from_uuid),
        name: req.name,
        status: req.status.unwrap_or_else(|| "pending".to_string()),
        score: req.score,
        summary: req.summary,
        created_at: now,
    };

    state
        .evaluation_repository()
        .create(&ctx, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateEvaluationResponse {
            evaluation_id: id.as_uuid(),
        }),
    ))
}

async fn list_evaluations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;

    let evaluations = state
        .evaluation_repository()
        .list(&ctx)
        .await
        .map_err(ApiError::from_application)?;

    let result: Vec<_> = evaluations
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id.as_uuid(),
                "model_id": e.model_id.map(|m| m.as_uuid()),
                "name": e.name,
                "status": e.status,
                "score": e.score,
                "summary": e.summary,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

async fn get_evaluation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let eval_id = vestrace_domain::id::EvaluationId::from_uuid(id);

    let record = state
        .evaluation_repository()
        .find_by_id(&ctx, eval_id)
        .await
        .map_err(ApiError::from_application)?;

    match record {
        Some(e) => Ok(Json(serde_json::json!({
            "id": e.id.as_uuid(),
            "model_id": e.model_id.map(|m| m.as_uuid()),
            "name": e.name,
            "status": e.status,
            "score": e.score,
            "summary": e.summary,
            "created_at": e.created_at,
        }))),
        None => Err(ApiError::not_found("evaluation not found")),
    }
}
