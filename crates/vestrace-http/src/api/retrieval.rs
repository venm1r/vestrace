use axum::{Json, extract::State, http::HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vestrace_application::RetrievalRequest;
use vestrace_domain::{MemoryStatus, TimePerspective, retrieval::WithheldRevision};

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct RetrievalSearchRequest {
    pub query: String,
    pub intent: Option<String>,
    pub token_budget: Option<u32>,
    pub limit: Option<u32>,
    pub time_perspective: Option<String>,
    pub as_of: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RetrievalSearchResponse {
    pub candidates: Vec<CandidateDto>,
    pub withheld: Vec<WithheldRevision>,
    pub retrieval_policy_version: String,
    pub temporal_perspective: TimePerspective,
    pub degraded: bool,
    pub degraded_channels: Vec<String>,
    pub warnings: Vec<String>,
    pub context_pack: Option<ContextPackDto>,
}

#[derive(Debug, Serialize)]
pub struct CandidateDto {
    pub memory_id: uuid::Uuid,
    pub revision_id: uuid::Uuid,
    pub memory_status: MemoryStatus,
    pub revision_number: u32,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub revision_created_at: DateTime<Utc>,
    pub source_generation: u32,
    pub score: f32,
    pub channel: String,
    pub channel_rank: u32,
    pub explanation: String,
    pub source_classification: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContextPackDto {
    pub temporal_perspective: TimePerspective,
    pub token_budget: u32,
    pub used_tokens: u32,
    pub candidate_count: usize,
    pub section_count: usize,
    pub degraded_channels: Vec<String>,
    pub withheld: Vec<WithheldRevision>,
    pub retrieval_policy_version: String,
}

pub fn parse_time_perspective(
    value: Option<&str>,
    as_of: Option<DateTime<Utc>>,
) -> Result<TimePerspective, ApiError> {
    match value.unwrap_or("current") {
        "current" => Ok(TimePerspective::Current),
        "as_of" => as_of
            .map(TimePerspective::AsOf)
            .ok_or_else(|| ApiError::bad_request("as_of time_perspective requires as_of")),
        "timeline" => Ok(TimePerspective::Timeline),
        "all_history" => Ok(TimePerspective::AllHistory),
        other => Err(ApiError::bad_request(format!(
            "unknown time perspective: {other}"
        ))),
    }
}

pub async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetrievalSearchRequest>,
) -> Result<Json<RetrievalSearchResponse>, ApiError> {
    let context = request_context(&headers)?;
    let time_perspective =
        parse_time_perspective(request.time_perspective.as_deref(), request.as_of)?;

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

    let mut req = RetrievalRequest::new(context.workspace_id, &request.query)
        .with_intent(intent)
        .with_time_perspective(time_perspective);

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
            revision_id: c.revision_id.as_uuid(),
            memory_status: c.memory_status,
            revision_number: c.revision_number,
            valid_from: c.valid_from,
            valid_until: c.valid_until,
            revision_created_at: c.revision_created_at,
            source_generation: c.source_generation,
            score: c.score,
            channel: c.channel.clone(),
            channel_rank: c.channel_rank,
            explanation: c.explanation.clone(),
            source_classification: c.classification.clone(),
        })
        .collect();

    let context_pack = if let Some(budget) = request.token_budget {
        Some(
            state
                .retrieval_service()
                .build_context(&context, &result, budget)
                .await
                .map(|pack| ContextPackDto {
                    temporal_perspective: pack.temporal_perspective,
                    token_budget: pack.token_budget,
                    used_tokens: pack.used_tokens,
                    candidate_count: pack.candidate_ids.len(),
                    section_count: pack.sections.len(),
                    degraded_channels: pack.degraded_channels,
                    withheld: pack.withheld,
                    retrieval_policy_version: pack.retrieval_policy_version,
                })
                .map_err(ApiError::from_application)?,
        )
    } else {
        None
    };

    Ok(Json(RetrievalSearchResponse {
        candidates,
        withheld: result.withheld,
        retrieval_policy_version: result.retrieval_policy_version,
        temporal_perspective: result.normalized.time_perspective,
        degraded: result.degraded,
        degraded_channels: result.degraded_channels,
        warnings: result.warnings,
        context_pack,
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_time_perspective;
    use chrono::{TimeZone, Utc};
    use vestrace_domain::TimePerspective;

    #[test]
    fn parses_current_timeline_and_all_history() {
        let at = Utc.with_ymd_and_hms(2026, 8, 11, 12, 0, 0).unwrap();

        assert_eq!(
            parse_time_perspective(Some("current"), None).unwrap(),
            TimePerspective::Current
        );
        assert_eq!(
            parse_time_perspective(Some("timeline"), None).unwrap(),
            TimePerspective::Timeline
        );
        assert_eq!(
            parse_time_perspective(Some("all_history"), None).unwrap(),
            TimePerspective::AllHistory
        );
        assert_eq!(
            parse_time_perspective(Some("as_of"), Some(at)).unwrap(),
            TimePerspective::AsOf(at)
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_temporal_perspective() {
        assert!(parse_time_perspective(Some("unknown"), None).is_err());
        assert!(parse_time_perspective(Some("as_of"), None).is_err());
    }
}
