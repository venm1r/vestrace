use axum::{Json, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;
use vestrace_application::ConnectionListing;

use crate::AppState;

use super::{ApiError, context::request_context};

const DEFAULT_CONNECTION_LIMIT: u32 = 200;

/// A connection's identity and state.
///
/// Deliberately carries no credential material: the schema stores none, and
/// exposing one here would be the first step toward leaking it. Credentials for
/// these connections are not stored by this system yet.
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub connector_name: String,
    pub provider_type: String,
    pub status: String,
    pub created_at: String,
}

impl From<ConnectionListing> for ConnectionResponse {
    fn from(listing: ConnectionListing) -> Self {
        let status = serde_json::to_value(listing.connection.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "unknown".to_owned());
        Self {
            id: listing.connection.id.as_uuid(),
            name: listing.connection.name,
            connector_name: listing.connector_name,
            provider_type: listing.provider_type,
            status,
            created_at: listing.connection.created_at.to_rfc3339(),
        }
    }
}

pub fn connection_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/connections", get(list_connections))
}

async fn list_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConnectionResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.connection_repository()?;
    let connections = repository
        .list(&context, DEFAULT_CONNECTION_LIMIT)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(connections.into_iter().map(Into::into).collect()))
}
