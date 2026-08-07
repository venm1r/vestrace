use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};
use vestrace_application::RetrievalRequest;

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct RetrievalSearchRequest {
    pub query: String,
    pub intent: Option<String>,
    pub token_budget: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct RetrievalSearchResponse {
    pub candidates: Vec<CandidateDto>,
    pub degraded: bool,
    pub warnings: Vec<String>,
    pub context_pack: Option<ContextPackDto>,
}

#[derive(Debug, Serialize)]
pub struct CandidateDto {
    pub memory_id: uuid::Uuid,
    pub score: f32,
    pub channel: String,
    pub channel_rank: u32,
    pub explanation: String,
}

#[derive(Debug, Serialize)]
pub struct ContextPackDto {
    pub token_budget: u32,
    pub used_tokens: u32,
    pub candidate_count: usize,
    pub section_count: usize,
}

pub async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetrievalSearchRequest>,
) -> Result<Json<RetrievalSearchResponse>, ApiError> {
    let context = request_context(&headers)?;

    let intent = match request.intent.as_deref() {
        Some("current_state") => vestrace_domain::RetrievalIntent::CurrentState,
        Some("decision_recall") => vestrace_domain::RetrievalIntent::DecisionRecall,
        Some("timeline") => vestrace_domain::RetrievalIntent::Timeline,
        Some("task_resume") => vestrace_domain::RetrievalIntent::TaskResume,
        Some("procedure_lookup") => vestrace_domain::RetrievalIntent::ProcedureLookup,
        Some("user_preferences") => vestrace_domain::RetrievalIntent::UserPreferences,
        Some("semantic_recall") | None => vestrace_domain::RetrievalIntent::SemanticRecall,
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "unknown retrieval intent: {other}"
            )));
        }
    };

    let mut req = RetrievalRequest::new(context.workspace_id, &request.query).with_intent(intent);

    if let Some(limit) = request.limit {
        req = req.with_limit(limit);
    }
    if let Some(budget) = request.token_budget {
        req = req.with_token_budget(budget);
    }

    let result = state
        .retrieval_service()
        .search(&context, req)
        .await
        .map_err(ApiError::from_application)?;

    let candidates: Vec<CandidateDto> = result
        .candidates
        .iter()
        .map(|c| CandidateDto {
            memory_id: c.memory_id.as_uuid(),
            score: c.score,
            channel: c.channel.clone(),
            channel_rank: c.channel_rank,
            explanation: c.explanation.clone(),
        })
        .collect();

    let context_pack = if let Some(budget) = request.token_budget {
        match state
            .retrieval_service()
            .build_context(&context, &result, budget)
            .await
        {
            Ok(pack) => Some(ContextPackDto {
                token_budget: pack.token_budget,
                used_tokens: pack.used_tokens,
                candidate_count: pack.candidate_ids.len(),
                section_count: pack.sections.len(),
            }),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(Json(RetrievalSearchResponse {
        candidates,
        degraded: result.degraded,
        warnings: result.warnings,
        context_pack,
    }))
}
