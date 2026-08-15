use axum::{Json, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;

use crate::AppState;

use super::{ApiError, context::request_context};

/// Counts the runtime can actually observe.
///
/// The fields are deliberately few. An earlier client contract also declared an
/// average latency and a spent budget; neither is measured or accounted
/// anywhere in this system, so reporting them would mean inventing numbers. The
/// budget *ceiling* is real — it comes from workspace settings — and is
/// reported as such.
#[derive(Debug, Serialize)]
pub struct MetricsSummaryResponse {
    /// Runs in a non-terminal state right now.
    pub live_runs: i64,
    /// Runs created since midnight UTC.
    pub runs_today: i64,
    pub registered_agents: i64,
    pub registered_models: i64,
    /// Per-run ceiling in micro-units from workspace settings; `null` when no
    /// cap is expressed.
    pub run_budget_cap_micros: Option<u64>,
}

pub fn metrics_summary_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/metrics/summary", get(metrics_summary))
}

async fn metrics_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MetricsSummaryResponse>, ApiError> {
    let context = request_context(&headers)?;

    let counts = state
        .workspace_counts(&context)
        .await
        .map_err(ApiError::from_application)?;

    let settings = state
        .workspace_settings_service()?
        .get(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(MetricsSummaryResponse {
        live_runs: counts.live_runs,
        runs_today: counts.runs_today,
        registered_agents: counts.registered_agents,
        registered_models: counts.registered_models,
        run_budget_cap_micros: (!settings.budget_is_unlimited())
            .then(|| settings.run_budget_cap_micros()),
    }))
}
