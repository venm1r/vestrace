//! Embedding spaces, corpus generations, jobs and transitions.
//!
//! This module holds vocabulary only. Nothing here performs I/O, and that is
//! deliberate: the closed lifecycles exist before any table or service does, so
//! a later migration cannot introduce a state that no Rust match site knows
//! about.

mod adoption;
mod generation;
mod index;
mod job;
mod retrieval;
mod space;

pub use adoption::LegacyAdoptionState;
pub use generation::{CorpusGeneration, CorpusGenerationState};
pub use index::{
    CanonicalGenerationSnapshot, CorpusChangeCause, EmbeddingGenerationError,
    GenerationMemberRepresentation, IndexBuildAttemptState,
};
pub use job::{
    BarrierState, CarryHeaderState, CarryMappingState, EmbeddingJobKind, EmbeddingJobState,
    EmbeddingSpaceTransitionState, TransitionInputOrdinal, TransitionRecipeOrdinal,
    TransitionVersion, UnknownEmbeddingJobKind,
};
pub use retrieval::{RetrievalGenerationChangedReason, RetrievalGenerationFence};
pub use space::{CanonicalEmbeddingSpace, EmbeddingSpaceKey, EmbeddingSpaceKeyError};
