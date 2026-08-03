use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub mod ag_ui;
mod context;
mod error;
mod runs;

pub use error::ApiError;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(runs::list_runs).post(runs::create_run))
        .route("/runs/{id}", get(runs::get_run))
        .route("/runs/{id}/approve", post(unsupported_approval))
        .route("/artifacts", get(unsupported_artifacts))
        .route("/agents", get(unsupported_agents))
        .route("/workflows", get(unsupported_workflows))
        .route("/triggers", get(unsupported_triggers))
        .route("/connections", get(unsupported_connections))
        .route("/models", get(unsupported_models))
        .route("/evaluations", get(unsupported_evaluations))
        .route("/audit", get(unsupported_audit))
        .route("/metrics/summary", get(unsupported_metrics))
        .route("/system/health", get(unsupported_system_health))
        .route("/profile", get(unsupported_profile))
}

macro_rules! unsupported_handler {
    ($name:ident, $feature:literal) => {
        async fn $name() -> ApiError {
            ApiError::not_implemented($feature)
        }
    };
}

unsupported_handler!(unsupported_approval, "run approval API");
unsupported_handler!(unsupported_artifacts, "artifact API");
unsupported_handler!(unsupported_agents, "agent API");
unsupported_handler!(unsupported_workflows, "workflow API");
unsupported_handler!(unsupported_triggers, "trigger API");
unsupported_handler!(unsupported_connections, "connection API");
unsupported_handler!(unsupported_models, "model API");
unsupported_handler!(unsupported_evaluations, "evaluation API");
unsupported_handler!(unsupported_audit, "audit API");
unsupported_handler!(unsupported_metrics, "metrics API");
unsupported_handler!(unsupported_system_health, "system health API");
unsupported_handler!(unsupported_profile, "profile API");
