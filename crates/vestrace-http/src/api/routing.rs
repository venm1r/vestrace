use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use vestrace_application::RoutingDecisionRecord;
use vestrace_domain::{
    ModelRouter, RoutingCandidate, RoutingStrategy, TaskRequirements, id::ModelId, time::now,
};

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct RouteModelRequest {
    pub strategy: RoutingStrategy,
    pub task: TaskRequirements,
    pub candidates: Vec<RouteCandidateInput>,
}

#[derive(Debug, Deserialize)]
pub struct RouteCandidateInput {
    pub model_id: uuid::Uuid,
    pub provider_id: uuid::Uuid,
    pub model_name: String,
    pub locality: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
    pub expected_quality: f32,
    pub estimated_latency_ms: u32,
    pub reliability: f32,
    pub observation_count: u32,
}

#[derive(Debug, Serialize)]
pub struct RouteModelResponse {
    pub decision_id: uuid::Uuid,
    pub selected_model_id: Option<uuid::Uuid>,
    pub selected_model_name: Option<String>,
    pub rationale: String,
    pub fallbacks: Vec<String>,
}

pub async fn route_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RouteModelRequest>,
) -> Result<(StatusCode, Json<RouteModelResponse>), ApiError> {
    let context = request_context(&headers)?;

    let candidates: Vec<RoutingCandidate> = req
        .candidates
        .into_iter()
        .map(|c| {
            let cost = vestrace_domain::ModelCostProfile::new(
                c.input_cost_per_mtoken,
                c.output_cost_per_mtoken,
            )
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
            let locality = match c.locality.as_str() {
                "local" => vestrace_domain::ProviderLocality::Local,
                "remote" => vestrace_domain::ProviderLocality::Remote,
                _ => {
                    return Err(ApiError::bad_request(
                        "locality must be 'local' or 'remote'",
                    ));
                }
            };
            Ok(RoutingCandidate {
                model_id: ModelId::from_uuid(c.model_id),
                provider_id: vestrace_domain::id::ProviderId::from_uuid(c.provider_id),
                model_name: c.model_name,
                locality,
                context_window: c.context_window,
                cost,
                expected_quality: c.expected_quality,
                estimated_latency_ms: c.estimated_latency_ms,
                reliability: c.reliability,
                observation_count: c.observation_count,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    let decision = ModelRouter::route(
        context.workspace_id,
        req.strategy,
        &req.task,
        &candidates,
        now(),
    );

    let selected_model_id = decision.selected.as_ref().map(|c| c.model_id);
    let selected_model_name = decision.selected.as_ref().map(|c| c.model_name.clone());
    let fallback_names: Vec<String> = decision
        .fallbacks
        .iter()
        .map(|c| c.model_name.clone())
        .collect();

    let record = RoutingDecisionRecord {
        id: decision.id,
        workspace_id: context.workspace_id,
        selected_model_id,
        intent: decision.task_type.clone(),
        rationale: decision.rationale.clone(),
        created_at: decision.created_at,
    };
    state
        .routing_decision_repository()
        .record(&context, &record)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        StatusCode::OK,
        Json(RouteModelResponse {
            decision_id: decision.id.as_uuid(),
            selected_model_id: selected_model_id.map(|m| m.as_uuid()),
            selected_model_name,
            rationale: decision.rationale,
            fallbacks: fallback_names,
        }),
    ))
}

pub async fn list_routing_decisions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RoutingDecisionResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let decisions = state
        .routing_decision_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(
        decisions
            .into_iter()
            .map(RoutingDecisionResponse::from)
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
pub struct RoutingDecisionResponse {
    pub id: uuid::Uuid,
    pub selected_model_id: Option<uuid::Uuid>,
    pub intent: String,
    pub rationale: String,
}

impl From<RoutingDecisionRecord> for RoutingDecisionResponse {
    fn from(d: RoutingDecisionRecord) -> Self {
        Self {
            id: d.id.as_uuid(),
            selected_model_id: d.selected_model_id.map(|m| m.as_uuid()),
            intent: d.intent,
            rationale: d.rationale,
        }
    }
}
