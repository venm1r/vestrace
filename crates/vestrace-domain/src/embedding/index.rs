//! Canonical generation metadata. These values carry no database authority.
use super::EmbeddingSpaceKey;
use crate::{CorpusGenerationId, WorkspaceId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EmbeddingGenerationError {
    #[error("canonical generation requires a complete canonical space")]
    LegacySpace,
    #[error("generation workspace differs from its space")]
    WorkspaceMismatch,
    #[error("generation identity must not be nil")]
    NilGeneration,
    #[error("generation epoch must be positive")]
    ZeroEpoch,
    #[error("generation guard version must be positive")]
    ZeroGuardVersion,
    #[error("retrieval job and request identities must not be nil")]
    NilRetrievalIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalGenerationSnapshot {
    pub workspace_id: WorkspaceId,
    pub space: EmbeddingSpaceKey,
    pub generation_id: CorpusGenerationId,
    pub generation_epoch: u64,
    pub guard_version: u64,
    pub corpus_revision: u64,
    pub built_through_projection_ordinal: u64,
    pub member_count: u64,
}

impl CanonicalGenerationSnapshot {
    pub fn new(
        space: EmbeddingSpaceKey,
        generation_id: CorpusGenerationId,
        generation_epoch: u64,
        guard_version: u64,
        corpus_revision: u64,
        built_through_projection_ordinal: u64,
        member_count: u64,
    ) -> Result<Self, EmbeddingGenerationError> {
        let value = Self {
            workspace_id: space.workspace_id(),
            space,
            generation_id,
            generation_epoch,
            guard_version,
            corpus_revision,
            built_through_projection_ordinal,
            member_count,
        };
        value.validate()?;
        Ok(value)
    }

    /// Public fields are data, not a capability: consumers must revalidate them.
    pub fn validate(&self) -> Result<(), EmbeddingGenerationError> {
        if !self.space.is_canonical() {
            return Err(EmbeddingGenerationError::LegacySpace);
        }
        if self.workspace_id != self.space.workspace_id() {
            return Err(EmbeddingGenerationError::WorkspaceMismatch);
        }
        if self.generation_id.as_uuid().is_nil() {
            return Err(EmbeddingGenerationError::NilGeneration);
        }
        if self.generation_epoch == 0 {
            return Err(EmbeddingGenerationError::ZeroEpoch);
        }
        if self.guard_version == 0 {
            return Err(EmbeddingGenerationError::ZeroGuardVersion);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GenerationMemberRepresentation {
    LegacyUpgrade,
    EncryptedProjection,
}

impl GenerationMemberRepresentation {
    pub const ALL: [Self; 2] = [Self::LegacyUpgrade, Self::EncryptedProjection];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyUpgrade => "legacy_upgrade",
            Self::EncryptedProjection => "encrypted_projection",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IndexBuildAttemptState {
    Claimed,
    Building,
    Published,
    Loaded,
    Discarded,
    Failed,
}

impl IndexBuildAttemptState {
    pub const ALL: [Self; 6] = [
        Self::Claimed,
        Self::Building,
        Self::Published,
        Self::Loaded,
        Self::Discarded,
        Self::Failed,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Building => "building",
            Self::Published => "published",
            Self::Loaded => "loaded",
            Self::Discarded => "discarded",
            Self::Failed => "failed",
        }
    }
}

impl IndexBuildAttemptState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Published | Self::Loaded | Self::Discarded | Self::Failed
        )
    }
    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Claimed,
                Self::Building | Self::Discarded | Self::Failed
            ) | (
                Self::Building,
                Self::Published | Self::Loaded | Self::Discarded | Self::Failed
            )
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CorpusChangeCause {
    ResultPublication,
    MaterialErasure,
    TransitionPublication,
    LegacyCutover,
    OperatorRebuild,
}

impl CorpusChangeCause {
    pub const ALL: [Self; 5] = [
        Self::ResultPublication,
        Self::MaterialErasure,
        Self::TransitionPublication,
        Self::LegacyCutover,
        Self::OperatorRebuild,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResultPublication => "result_publication",
            Self::MaterialErasure => "material_erasure",
            Self::TransitionPublication => "transition_publication",
            Self::LegacyCutover => "legacy_cutover",
            Self::OperatorRebuild => "operator_rebuild",
        }
    }
}
