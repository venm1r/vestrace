use crate::CorpusGenerationId;

use super::EmbeddingSpaceKey;

/// A corpus generation's two states, taken from the spec rather than designed.
///
/// Line 207: the builder "CAS-publishes a new-space `Ready` generation only
/// after the exact bijection holds". Line 219: the finalizer "marks any current
/// Ready generation for that space `Stale`/not-current". Those are the two
/// states those lines name, and there is no third.
///
/// The edge is one-way. A generation that could return to `Ready` after being
/// superseded would let a retrieval fenced to it read a corpus the activation
/// transaction had already replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorpusGenerationState {
    Building,
    Ready,
    Stale,
}

impl CorpusGenerationState {
    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Building, Self::Ready) | (Self::Ready, Self::Stale)
        )
    }

    pub const fn is_current(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// One published generation of one space's corpus.
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
    /// A generation is born `Ready` or not at all: line 207 publishes it only
    /// after the bijection holds, so there is no earlier state to represent.
    pub const fn publish_ready(
        id: CorpusGenerationId,
        space: EmbeddingSpaceKey,
        member_count: u64,
    ) -> Self {
        Self {
            id,
            space,
            member_count,
            state: CorpusGenerationState::Ready,
        }
    }

    /// Supersession is the only mutation, and it is refused rather than ignored
    /// when the generation is already stale.
    pub fn mark_stale(&mut self) -> Result<(), CorpusGenerationState> {
        if self.state.may_advance_to(CorpusGenerationState::Stale) {
            self.state = CorpusGenerationState::Stale;
            Ok(())
        } else {
            Err(self.state)
        }
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
