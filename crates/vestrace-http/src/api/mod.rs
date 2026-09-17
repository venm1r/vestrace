use axum::{
    Router,
    http::Method,
    routing::{get, post},
};

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

pub mod access_tokens;
pub mod ag_ui;
pub mod agents;
pub mod artifacts;
pub mod audit;
pub mod capability_grants;
pub mod connections;
pub(crate) mod context;
pub mod effects;
pub mod embedding_jobs;
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
pub mod system;
pub mod triggers;
pub mod workflow_executions;
pub mod workflows;

pub use error::ApiError;

/// The header a governed mutation is deduplicated on.
pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// The caller's own idempotency key, kept exactly as sent.
///
/// # Why this is not `x-request-id`
///
/// `add_request_context` inserts a normalised UUIDv7 `x-request-id` into every
/// request, so a handler can never observe its absence, and a caller that sends
/// none — or sends a non-UUID — has one invented per attempt. A governed
/// mutation keyed on that value would treat every retry as a new request and
/// publish a second immutable revision, silently. `api::memory` found this the
/// hard way; the provider mutation surfaces take the same remedy rather than
/// rediscovering it.
///
/// Mandatory rather than optional: a key the server invents deduplicates
/// nothing.
pub(crate) fn required_idempotency_key(
    headers: &axum::http::HeaderMap,
) -> Result<String, ApiError> {
    let value = headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .ok_or_else(|| ApiError::bad_request(format!("missing {IDEMPOTENCY_KEY_HEADER} header")))?
        .to_str()
        .map_err(|_| {
            ApiError::bad_request(format!(
                "{IDEMPOTENCY_KEY_HEADER} header must be valid UTF-8"
            ))
        })?
        .trim();
    if value.is_empty() || value.len() > 200 {
        return Err(ApiError::bad_request(format!(
            "{IDEMPOTENCY_KEY_HEADER} header must be between 1 and 200 characters"
        )));
    }
    Ok(value.to_owned())
}

pub fn api_routes() -> Router<AppState> {
    let router = Router::new();
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/runs"),
        get(runs::list_runs),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs"),
        post(runs::create_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/runs/{id}"),
        get(runs::get_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs/{id}/steps"),
        post(runs::add_run_steps),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs/{id}/pause"),
        post(runs::pause_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs/{id}/resume"),
        post(runs::resume_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs/{id}/cancel"),
        post(runs::cancel_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/runs/{id}/approve"),
        post(runs::approve_run),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/events"),
        post(memory::create_event),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/memories"),
        post(memory::create_memory),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/memories/{id}"),
        get(memory::get_memory),
    );
    let router = mount(
        router,
        route_descriptor(&Method::DELETE, "/v1/memories/{id}"),
        axum::routing::delete(memory::purge_memory),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/memories/{id}/revisions"),
        post(memory::revise_memory),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/retrieval/search"),
        post(retrieval::search),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/agents"),
        get(agents::list_agents),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/agents"),
        post(agents::create_agent),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/models"),
        get(models::list_models),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/models"),
        post(models::create_model),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/models/{id}/revisions"),
        post(models::create_model_revision),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/models/{id}/default"),
        post(models::set_workspace_model_default),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/models/default"),
        get(models::get_workspace_model_default),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/models/{id}/qualifications"),
        post(models::request_model_qualification),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/providers"),
        get(models::list_providers),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/providers"),
        post(models::create_provider),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/skills"),
        get(skills::list_skills),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/skills"),
        post(skills::create_skill),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/routing/decisions"),
        get(routing::list_routing_decisions),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/routing/decisions"),
        post(routing::route_model),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/executions"),
        get(executions::list_executions),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/executions"),
        post(executions::record_execution),
    );

    router
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
        .merge(embedding_jobs::embedding_job_routes())
        .merge(system::system_routes())
        .merge(metrics_summary::metrics_summary_routes())
}

// The `unsupported_handler!` macro that stood here was removed once its last
// caller was implemented. Keeping it would have made it cheap to add another
// route that answers 501 while looking finished from the outside.
