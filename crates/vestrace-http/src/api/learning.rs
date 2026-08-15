use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::AppState;
use vestrace_domain::{
    EvaluationId, EvidenceRef, LearnedProjection, LearningChange, LearningProjectionId,
    LearningProposal, LearningProposalId, LearningTarget, ProjectionGenerator, ProjectionKind,
};

use super::{ApiError, context::request_context};

/// L2 deliberately exposes creation and inspection only. There is no endpoint
/// that applies a learned result to an agent, skill, workflow, tool, or policy.
pub fn learning_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/learning/projections",
            axum::routing::post(create_projection).get(list_projections),
        )
        .route(
            "/learning/projections/{id}",
            axum::routing::get(get_projection),
        )
        .route(
            "/learning/proposals",
            axum::routing::post(create_proposal).get(list_proposals),
        )
        .route(
            "/learning/proposals/{id}/submit",
            axum::routing::post(submit_proposal),
        )
        .route("/learning/proposals/{id}", axum::routing::get(get_proposal))
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectionRequest {
    pub kind: ProjectionKind,
    pub target: LearningTarget,
    pub generator: ProjectionGenerator,
    pub source_generation: u32,
    pub source_evaluation_fact_ids: Vec<EvaluationId>,
    pub source_evidence_refs: Vec<EvidenceRef>,
    pub content: Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    pub source_projection_ids: Vec<LearningProjectionId>,
    pub source_evaluation_fact_ids: Vec<EvaluationId>,
    pub target: LearningTarget,
    pub change: LearningChange,
    pub expected_target_revision: u32,
    pub rationale: String,
    pub policy_version: String,
}

async fn create_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateProjectionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let id = LearningProjectionId::new();
    let projection = LearnedProjection::new(
        id,
        context.workspace_id,
        request.kind,
        request.target,
        request.generator,
        request.source_generation,
        request.source_evaluation_fact_ids,
        request.source_evidence_refs,
        request.content,
        vestrace_domain::now(),
    )
    .map_err(domain_error)?;

    state
        .learning_repository()
        .create_projection(&context, &projection)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(projection)))
}

async fn list_projections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let projections = state
        .learning_repository()
        .list_projections(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(projections))
}

async fn get_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let projection = state
        .learning_repository()
        .find_projection(&context, LearningProjectionId::from_uuid(id))
        .await
        .map_err(ApiError::from_application)?;
    projection
        .map(Json)
        .ok_or_else(|| ApiError::not_found("learned projection"))
}

async fn create_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateProposalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let id = LearningProposalId::new();
    let proposal = LearningProposal::new(
        id,
        context.workspace_id,
        request.source_projection_ids,
        request.source_evaluation_fact_ids,
        request.target,
        request.change,
        request.expected_target_revision,
        request.rationale,
        request.policy_version,
        context.principal_id,
        vestrace_domain::now(),
    )
    .map_err(domain_error)?;

    state
        .learning_repository()
        .create_proposal(&context, &proposal)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(proposal)))
}

async fn list_proposals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let proposals = state
        .learning_repository()
        .list_proposals(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(proposals))
}

async fn get_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let proposal = state
        .learning_repository()
        .find_proposal(&context, LearningProposalId::from_uuid(id))
        .await
        .map_err(ApiError::from_application)?;
    proposal
        .map(Json)
        .ok_or_else(|| ApiError::not_found("learning proposal"))
}

async fn submit_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let proposal = state
        .learning_repository()
        .submit_proposal(
            &context,
            LearningProposalId::from_uuid(id),
            vestrace_domain::now(),
        )
        .await
        .map_err(ApiError::from_application)?;
    proposal
        .map(Json)
        .ok_or_else(|| ApiError::not_found("draft learning proposal"))
}

fn domain_error(error: vestrace_domain::DomainError) -> ApiError {
    ApiError::from_application(vestrace_application::ApplicationError::Domain(error))
}
