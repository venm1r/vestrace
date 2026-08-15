use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub mod access_tokens;
pub mod ag_ui;
pub mod agents;
pub mod artifacts;
pub mod audit;
pub mod capability_grants;
pub mod connections;
pub(crate) mod context;
pub mod error;
pub mod evaluations;
pub mod executions;
pub mod learning;
pub mod memory;
pub mod metrics_summary;
pub mod models;
pub mod profile;
pub mod retrieval;
pub mod routing;
pub mod runs;
pub mod secrets;
pub mod settings;
pub mod skills;
pub mod effects;
pub mod system;
pub mod triggers;
pub mod workflow_executions;
pub mod workflows;

pub use error::ApiError;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(runs::list_runs).post(runs::create_run))
        .route("/runs/{id}", get(runs::get_run))
        .route("/runs/{id}/steps", post(runs::add_run_steps))
        .route("/runs/{id}/pause", post(runs::pause_run))
        .route("/runs/{id}/resume", post(runs::resume_run))
        .route("/runs/{id}/cancel", post(runs::cancel_run))
        .route("/runs/{id}/approve", post(runs::approve_run))
        .route("/events", post(memory::create_event))
        .route("/memories", post(memory::create_memory))
        .route(
            "/memories/{id}",
            get(memory::get_memory).delete(memory::purge_memory),
        )
        .route("/memories/{id}/revisions", post(memory::revise_memory))
        .route("/retrieval/search", post(retrieval::search))
        .route(
            "/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
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
        .merge(workflow_executions::workflow_execution_routes())
        .merge(workflows::workflow_routes())
        .merge(evaluations::evaluation_routes())
        .merge(learning::learning_routes())
        .merge(capability_grants::capability_grant_routes())
        .merge(access_tokens::access_token_routes())
        .merge(secrets::secret_routes())
        .merge(settings::settings_routes())
        .merge(audit::audit_routes())
        .merge(artifacts::artifact_routes())
        .merge(triggers::trigger_routes())
        .merge(connections::connection_routes())
        .merge(profile::profile_routes())
        .merge(effects::effect_routes())
        .merge(system::system_routes())
        .merge(metrics_summary::metrics_summary_routes())
}

// The `unsupported_handler!` macro that stood here was removed once its last
// caller was implemented. Keeping it would have made it cheap to add another
// route that answers 501 while looking finished from the outside.
