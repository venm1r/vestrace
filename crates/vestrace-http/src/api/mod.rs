use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub mod ag_ui;
pub mod agents;
mod context;
pub mod error;
pub mod evaluations;
pub mod executions;
pub mod memory;
pub mod models;
pub mod retrieval;
pub mod routing;
pub mod runs;
pub mod skills;
pub mod workflow_executions;
pub mod workflows;

pub use error::ApiError;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(runs::list_runs).post(runs::create_run))
        .route("/runs/{id}", get(runs::get_run))
        .route("/runs/{id}/pause", post(runs::pause_run))
        .route("/runs/{id}/resume", post(runs::resume_run))
        .route("/runs/{id}/cancel", post(runs::cancel_run))
        .route("/runs/{id}/approve", post(unsupported_approval))
        .route("/events", post(memory::create_event))
        .route("/memories", post(memory::create_memory))
        .route("/memories/{id}", get(memory::get_memory))
        .route("/retrieval/search", post(retrieval::search))
        .route("/artifacts", get(unsupported_artifacts))
        .route(
            "/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route("/triggers", get(unsupported_triggers))
        .route("/connections", get(unsupported_connections))
        .route(
            "/models",
            get(models::list_models).post(models::create_model),
        )
        .route(
            "/providers",
            get(models::list_providers).post(models::create_provider),
        )
        .route(
            "/skills",
            get(skills::list_skills).post(skills::create_skill),
        )
        .route(
            "/routing/decisions",
            post(routing::route_model).get(routing::list_routing_decisions),
        )
        .route(
            "/executions",
            post(executions::record_execution).get(executions::list_executions),
        )
        .route("/audit", get(unsupported_audit))
        .route("/metrics/summary", get(unsupported_metrics))
        .route("/system/health", get(unsupported_system_health))
        .route("/profile", get(unsupported_profile))
        .merge(workflow_executions::workflow_execution_routes())
        .merge(workflows::workflow_routes())
        .merge(evaluations::evaluation_routes())
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
unsupported_handler!(unsupported_triggers, "trigger API");
unsupported_handler!(unsupported_connections, "connection API");
unsupported_handler!(unsupported_audit, "audit API");
unsupported_handler!(unsupported_metrics, "metrics API");
unsupported_handler!(unsupported_system_health, "system health API");
unsupported_handler!(unsupported_profile, "profile API");
