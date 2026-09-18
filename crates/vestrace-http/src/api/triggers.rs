use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method},
    routing::get,
};
use serde::Serialize;
use vestrace_domain::conversation::ExternalTrigger;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{ApiError, context::request_context};

const DEFAULT_TRIGGER_LIMIT: u32 = 200;

/// A registered trigger. `enabled` reflects the stored flag only: no scheduler
/// or webhook receiver consults this registry yet, so an enabled trigger does
/// not currently fire.
#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub trigger_type: String,
    pub enabled: bool,
    pub created_at: String,
}

impl From<ExternalTrigger> for TriggerResponse {
    fn from(trigger: ExternalTrigger) -> Self {
        Self {
            id: trigger.id.as_uuid(),
            name: trigger.name,
            trigger_type: trigger.trigger_type,
            enabled: trigger.enabled,
            created_at: trigger.created_at.to_rfc3339(),
        }
    }
}

pub fn trigger_routes() -> axum::Router<AppState> {
    mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/triggers"),
        get(list_triggers),
    )
}

async fn list_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<TriggerResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.trigger_repository()?;
    let triggers = repository
        .list(&context, DEFAULT_TRIGGER_LIMIT)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(triggers.into_iter().map(Into::into).collect()))
}
