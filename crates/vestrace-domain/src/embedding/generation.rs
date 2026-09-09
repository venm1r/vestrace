use crate::CorpusGenerationId;

use super::{EmbeddingGenerationError, EmbeddingSpaceKey};

/// Durable generation lifecycle; only Ready may be current.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorpusGenerationState {
    Building,
    Ready,
    Stale,
    Revoked,
}

impl CorpusGenerationState {
    pub const ALL: [Self; 4] = [Self::Building, Self::Ready, Self::Stale, Self::Revoked];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Revoked => "revoked",
        }
    }
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Revoked)
    }

    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Building, Self::Ready | Self::Stale | Self::Revoked)
                | (Self::Ready, Self::Stale | Self::Revoked)
                | (Self::Stale, Self::Revoked)
        )
    }

    pub const fn is_current(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// One canonical generation of a space corpus, initially Building.
///
/// `member_count` is the count the activation transaction matched against the
/// recipe set (line 207): "The target corpus revision, projection/member count,
/// and Ready generation must match that same recipe set." It is carried here so
/// a later reader can check that match without recomputing it, not as a
/// convenience counter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusGeneration {
    id: CorpusGenerationId,
    space: EmbeddingSpaceKey,
    member_count: u64,
    state: CorpusGenerationState,
}

impl CorpusGeneration {
    /// Construct structural metadata only; durable publication still requires its guard.
    pub fn building(
        id: CorpusGenerationId,
        space: EmbeddingSpaceKey,
        member_count: u64,
    ) -> Result<Self, EmbeddingGenerationError> {
        if !space.is_canonical() {
            return Err(EmbeddingGenerationError::LegacySpace);
        }
        if id.as_uuid().is_nil() {
            return Err(EmbeddingGenerationError::NilGeneration);
        }
        Ok(Self {
            id,
            space,
            member_count,
            state: CorpusGenerationState::Building,
        })
    }

    /// Apply exactly one allowed lifecycle edge, preserving state on refusal.
    pub fn advance_to(&mut self, next: CorpusGenerationState) -> Result<(), CorpusGenerationState> {
        if !self.state.may_advance_to(next) {
            return Err(self.state);
        }
        self.state = next;
        Ok(())
    }

    pub fn mark_stale(&mut self) -> Result<(), CorpusGenerationState> {
        self.advance_to(CorpusGenerationState::Stale)
    }

    pub const fn id(&self) -> CorpusGenerationId {
        self.id
    }

    pub const fn space(&self) -> &EmbeddingSpaceKey {
        &self.space
    }

    pub const fn member_count(&self) -> u64 {
        self.member_count
    }

    pub const fn state(&self) -> CorpusGenerationState {
        self.state
    }
}
