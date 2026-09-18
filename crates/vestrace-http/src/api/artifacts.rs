use std::fmt::Write as _;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method},
    routing::get,
};
use serde::Serialize;
use vestrace_application::ArtifactListing;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

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
    /// Legacy metadata only. Provider-material revisions expose their closed
    /// media class below instead of an exact provider media type.
    pub media_type: Option<String>,
    /// Legacy exact byte size only; never populated for provider material.
    pub size_bytes: Option<u64>,
    /// Legacy digest only; provider material has no unkeyed digest projection.
    pub content_sha256: Option<String>,
    pub revision_number: Option<u32>,
    /// Opaque revision identity for either legacy or provider material.
    pub revision_id: Option<uuid::Uuid>,
    /// Present only for a provider-material revision.
    pub content_material_id: Option<uuid::Uuid>,
    /// Hex encoding of the 32-byte DEK-bound commitment, not a content digest.
    pub erasure_bound_commitment: Option<String>,
    /// Padded class only; never an exact provider-result byte size.
    pub size_class: Option<vestrace_domain::SizeClass>,
    /// Closed provider media class only.
    pub media_class: Option<&'static str>,
}

impl From<ArtifactListing> for ArtifactResponse {
    fn from(listing: ArtifactListing) -> Self {
        let legacy = listing.latest_revision.as_ref();
        let governed = listing.governed_material.as_ref();
        let status = serde_json::to_value(listing.artifact.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "unknown".to_owned());
        Self {
            id: listing.artifact.id.as_uuid(),
            name: listing.artifact.name,
            status,
            created_at: listing.artifact.created_at.to_rfc3339(),
            media_type: legacy.map(|revision| revision.media_type.clone()),
            size_bytes: legacy.map(|revision| revision.byte_size),
            content_sha256: legacy.map(|revision| revision.content_hash.clone()),
            revision_number: legacy.map(|revision| revision.revision_number),
            revision_id: governed
                .map(|material| material.artifact_revision_id.as_uuid())
                .or_else(|| legacy.map(|revision| revision.id.as_uuid())),
            content_material_id: governed.map(|material| material.content_material_id.as_uuid()),
            erasure_bound_commitment: governed.map(|material| {
                let commitment = &material.erasure_bound_commitment;
                commitment.iter().fold(
                    String::with_capacity(commitment.len() * 2),
                    |mut hex, byte| {
                        // Writing into a String cannot fail.
                        let _ = write!(hex, "{byte:02x}");
                        hex
                    },
                )
            }),
            size_class: governed.map(|material| material.size_class),
            media_class: governed.map(|material| material.media_class.as_str()),
        }
    }
}

pub fn artifact_routes() -> axum::Router<AppState> {
    mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/artifacts"),
        get(list_artifacts),
    )
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
