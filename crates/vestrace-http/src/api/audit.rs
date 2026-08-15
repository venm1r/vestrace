use axum::{Json, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;
use vestrace_domain::AuditEvent;

use crate::AppState;

use super::{ApiError, context::request_context};

/// Bound on a single page of audit history, so an unbounded read cannot be
/// requested by accident.
const DEFAULT_AUDIT_LIMIT: u32 = 200;

#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    pub id: uuid::Uuid,
    pub timestamp: String,
    /// The principal that performed the action.
    pub actor: uuid::Uuid,
    pub action: String,
    /// Qualified as `type:id`, because an identifier alone does not say what
    /// was acted upon.
    pub resource: String,
}

impl From<AuditEvent> for AuditEventResponse {
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.id.as_uuid(),
            timestamp: event.created_at.to_rfc3339(),
            actor: event.principal_id.as_uuid(),
            action: event.action,
            resource: format!("{}:{}", event.resource_type, event.resource_id),
        }
    }
}

pub fn audit_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/audit", get(list_audit_events))
}

async fn list_audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AuditEventResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.audit_repository()?;
    let events = repository
        .list(&context, DEFAULT_AUDIT_LIMIT)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(events.into_iter().map(Into::into).collect()))
}
