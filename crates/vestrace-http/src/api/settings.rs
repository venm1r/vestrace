use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use vestrace_domain::{LogLevel, WorkspaceSettings};

use crate::AppState;

use super::{ApiError, context::request_context};

const IF_MATCH_HEADER: &str = "if-match";

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub workspace_id: uuid::Uuid,
    pub max_concurrent_runs: u32,
    /// Budget ceiling per run in micro-units. Zero means no cap, which
    /// `budget_unlimited` states explicitly so a client need not infer it.
    pub run_budget_cap_micros: u64,
    pub budget_unlimited: bool,
    pub log_level: String,
    /// Optimistic-concurrency token; send it back as `If-Match` when updating.
    pub version: u64,
}

impl From<WorkspaceSettings> for SettingsResponse {
    fn from(settings: WorkspaceSettings) -> Self {
        Self {
            workspace_id: settings.workspace_id().as_uuid(),
            max_concurrent_runs: settings.max_concurrent_runs(),
            run_budget_cap_micros: settings.run_budget_cap_micros(),
            budget_unlimited: settings.budget_is_unlimited(),
            log_level: settings.log_level().as_str().to_owned(),
            version: settings.version(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub max_concurrent_runs: u32,
    pub run_budget_cap_micros: u64,
    pub log_level: String,
}

pub fn settings_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
}

async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SettingsResponse>, ApiError> {
    let context = request_context(&headers)?;
    let service = state.workspace_settings_service()?;
    let settings = service
        .get(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(settings.into()))
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>, ApiError> {
    let context = request_context(&headers)?;
    let expected_version = if_match_version(&headers)?;
    let log_level = request
        .log_level
        .parse::<LogLevel>()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;

    let service = state.workspace_settings_service()?;
    let updated = service
        .update(
            &context,
            expected_version,
            request.max_concurrent_runs,
            request.run_budget_cap_micros,
            log_level,
        )
        .await
        .map_err(ApiError::from_application)?;

    // Changing execution limits is an administrative act, so it is recorded
    // before the caller is told it succeeded. A trail that silently drops
    // entries is worse than no trail: an unconfigured audit repository fails
    // the request rather than letting the change pass unrecorded.
    {
        let audit = state.audit_repository()?;
        let event = vestrace_domain::AuditEvent::new(
            vestrace_domain::id::AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "workspace_settings.updated",
            "workspace_settings",
            context.workspace_id.as_uuid(),
            serde_json::json!({
                "from_version": expected_version,
                "to_version": updated.version(),
                "max_concurrent_runs": updated.max_concurrent_runs(),
                "run_budget_cap_micros": updated.run_budget_cap_micros(),
                "log_level": updated.log_level().as_str(),
            }),
            vestrace_domain::time::now(),
        )
        .map_err(|error| ApiError::from_application(error.into()))?;
        audit
            .record(&context, &event)
            .await
            .map_err(ApiError::from_application)?;
    }

    Ok(Json(updated.into()))
}

/// Settings updates are compare-and-set: the caller states which revision it
/// read, so a concurrent change is reported instead of being overwritten.
fn if_match_version(headers: &HeaderMap) -> Result<u64, ApiError> {
    let value = headers
        .get(IF_MATCH_HEADER)
        .ok_or_else(|| ApiError::bad_request("missing If-Match header"))?;
    value
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match header must be valid UTF-8"))?
        .trim()
        .trim_matches('"')
        .parse()
        .map_err(|_| ApiError::bad_request("If-Match header must be a version number"))
}
