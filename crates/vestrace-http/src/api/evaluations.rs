use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};
use vestrace_domain::{
    EvaluationAuthority, EvaluationFact, EvaluationMetric, EvaluationResult, EvaluationTarget,
    EvaluatorRef, EvidenceRef,
};

use super::{ApiError, context::request_context};

pub fn evaluation_routes() -> axum::Router<AppState> {
    let router = mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/evaluations"),
        get(list_evaluations),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/evaluations"),
        post(create_evaluation),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/evaluations/{id}"),
        get(get_evaluation),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/evaluation-facts"),
        get(list_evaluation_facts),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/evaluation-facts"),
        post(create_evaluation_fact),
    );
    mount(
        router,
        route_descriptor(&Method::GET, "/v1/evaluation-facts/{id}"),
        get(get_evaluation_fact),
    )
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

#[derive(Debug, Deserialize)]
pub struct CreateEvaluationFactRequest {
    pub target: EvaluationTarget,
    pub evaluator: EvaluatorRef,
    pub metric: EvaluationMetric,
    pub result: EvaluationResult,
    pub evidence_refs: Vec<EvidenceRef>,
    pub authority: EvaluationAuthority,
    pub policy_version: String,
}

#[derive(Debug, Serialize)]
pub struct CreateEvaluationFactResponse {
    pub evaluation_fact_id: Uuid,
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

async fn create_evaluation_fact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateEvaluationFactRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let fact_id = vestrace_domain::id::EvaluationId::new();
    let fact = EvaluationFact::new(
        fact_id,
        ctx.workspace_id,
        req.target,
        req.evaluator,
        req.metric,
        req.result,
        req.evidence_refs,
        req.authority,
        req.policy_version,
        vestrace_domain::time::now(),
    )
    .map_err(|error| {
        ApiError::from_application(vestrace_application::ApplicationError::Domain(error))
    })?;

    state
        .evaluation_repository()
        .create_fact(&ctx, &fact)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateEvaluationFactResponse {
            evaluation_fact_id: fact_id.as_uuid(),
        }),
    ))
}

async fn list_evaluation_facts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let facts = state
        .evaluation_repository()
        .list_facts(&ctx)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(facts))
}

async fn get_evaluation_fact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = request_context(&headers)?;
    let fact_id = vestrace_domain::id::EvaluationId::from_uuid(id);
    let fact = state
        .evaluation_repository()
        .find_fact(&ctx, fact_id)
        .await
        .map_err(ApiError::from_application)?;

    fact.map(Json)
        .ok_or_else(|| ApiError::not_found("evaluation fact"))
}
