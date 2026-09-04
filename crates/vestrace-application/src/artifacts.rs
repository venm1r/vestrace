use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    ContentMaterialId, SizeClass,
    artifact::{Artifact, ArtifactRevision},
    id::ArtifactRevisionId,
};

use crate::{ApplicationError, RequestContext};

/// An artifact together with its latest revision, which is what a reader needs
/// to say anything useful about it: a bare artifact row carries no size, media
/// type or content hash.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactListing {
    pub artifact: Artifact,
    /// Present only for pre-P03 digest-backed revisions. Provider-produced
    /// revisions intentionally leave this absent so their digest and exact
    /// byte size cannot leak through a compatibility projection.
    pub latest_revision: Option<ArtifactRevision>,
    /// The one safe projection for a provider-produced revision. It contains
    /// only opaque identities, a DEK-bound commitment, padded size class, and
    /// closed media class; it is never reconstructible as legacy bytes.
    pub governed_material: Option<GovernedArtifactMaterial>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderArtifactMediaClass {
    Text,
    Json,
    Binary,
    Image,
    Audio,
}

impl ProviderArtifactMediaClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Binary => "binary",
            Self::Image => "image",
            Self::Audio => "audio",
        }
    }
}

/// A provider result's durable, content-safe Artifact revision projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedArtifactMaterial {
    pub artifact_revision_id: ArtifactRevisionId,
    pub content_material_id: ContentMaterialId,
    pub erasure_bound_commitment: [u8; 32],
    pub size_class: SizeClass,
    pub media_class: ProviderArtifactMediaClass,
}

/// Content to be stored, with the media type a reader needs to interpret it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactContent {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl ArtifactContent {
    /// The lowercase-hex SHA-256 of the bytes.
    ///
    /// The digest is computed here rather than accepted from a caller: a
    /// caller-supplied hash would let content be stored under a digest that
    /// does not describe it, which defeats the point of content addressing.
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.bytes);
        format!("{:x}", hasher.finalize())
    }
}

/// A stored artifact revision together with the digest its content was filed
/// under, so a caller can reference the content without holding it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredArtifact {
    pub artifact_id: vestrace_domain::id::ArtifactId,
    pub revision_id: vestrace_domain::id::ArtifactRevisionId,
    pub content_hash: String,
    pub byte_size: u64,
}

#[async_trait]
pub trait ArtifactRepository: Send + Sync {
    /// Most recently created first, capped by `limit`.
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ArtifactListing>, ApplicationError>;

    /// Store `content` as a new artifact and its first revision.
    async fn store(
        &self,
        context: &RequestContext,
        name: &str,
        content: &ArtifactContent,
    ) -> Result<StoredArtifact, ApplicationError>;

    /// Fetch content by digest.
    ///
    /// `Ok(None)` means the content is not held here — a revision may pin a
    /// digest whose bytes live elsewhere — and is distinct from an error.
    async fn fetch_content(
        &self,
        context: &RequestContext,
        content_hash: &str,
    ) -> Result<Option<ArtifactContent>, ApplicationError>;
}

pub type SharedArtifactRepository = Arc<dyn ArtifactRepository>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_sha256_of_the_bytes() {
        let content = ArtifactContent {
            media_type: "text/plain".into(),
            bytes: b"abc".to_vec(),
        };
        // The published SHA-256 of "abc".
        assert_eq!(
            content.content_hash(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn the_digest_shape_matches_what_the_schema_constrains() {
        // Migration 0134 checks `^[0-9a-f]{64}$`; an uppercase or truncated
        // rendering here would be rejected at insert time.
        let hash = ArtifactContent {
            media_type: "text/plain".into(),
            bytes: vec![0u8; 10],
        }
        .content_hash();
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    #[test]
    fn different_content_hashes_differently() {
        let first = ArtifactContent {
            media_type: "text/plain".into(),
            bytes: b"one".to_vec(),
        };
        let second = ArtifactContent {
            media_type: "text/plain".into(),
            bytes: b"two".to_vec(),
        };
        assert_ne!(first.content_hash(), second.content_hash());
    }
}
