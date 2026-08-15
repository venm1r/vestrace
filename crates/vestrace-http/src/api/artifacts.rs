use axum::{Json, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;
use vestrace_application::ArtifactListing;

use crate::AppState;

use super::{ApiError, context::request_context};

const DEFAULT_ARTIFACT_LIMIT: u32 = 200;

/// A registry entry. The `content_sha256` pins what the bytes must hash to;
/// this system does not store or serve them, so there is deliberately no
/// download link here.
#[derive(Debug, Serialize)]
pub struct ArtifactResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub status: String,
    pub created_at: String,
    /// Absent until a revision is registered.
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub content_sha256: Option<String>,
    pub revision_number: Option<u32>,
}

impl From<ArtifactListing> for ArtifactResponse {
    fn from(listing: ArtifactListing) -> Self {
        let status = serde_json::to_value(listing.artifact.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "unknown".to_owned());
        Self {
            id: listing.artifact.id.as_uuid(),
            name: listing.artifact.name,
            status,
            created_at: listing.artifact.created_at.to_rfc3339(),
            media_type: listing
                .latest_revision
                .as_ref()
                .map(|revision| revision.media_type.clone()),
            size_bytes: listing
                .latest_revision
                .as_ref()
                .map(|revision| revision.byte_size),
            content_sha256: listing
                .latest_revision
                .as_ref()
                .map(|revision| revision.content_hash.clone()),
            revision_number: listing
                .latest_revision
                .as_ref()
                .map(|revision| revision.revision_number),
        }
    }
}

pub fn artifact_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/artifacts", get(list_artifacts))
}

async fn list_artifacts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ArtifactResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.artifact_repository()?;
    let artifacts = repository
        .list(&context, DEFAULT_ARTIFACT_LIMIT)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(artifacts.into_iter().map(Into::into).collect()))
}
