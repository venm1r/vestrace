//! Embedding spaces, corpus generations, jobs and transitions.
//!
//! This module holds vocabulary only. Nothing here performs I/O, and that is
//! deliberate: the closed lifecycles exist before any table or service does, so
//! a later migration cannot introduce a state that no Rust match site knows
//! about.

mod generation;
mod job;
mod space;

pub use generation::{CorpusGeneration, CorpusGenerationState};
pub use job::{
    BarrierState, CarryHeaderState, CarryMappingState, EmbeddingJobKind, EmbeddingJobState,
    EmbeddingSpaceTransitionState, TransitionInputOrdinal, TransitionRecipeOrdinal,
    TransitionVersion, UnknownEmbeddingJobKind,
};
pub use space::{EmbeddingSpaceKey, EmbeddingSpaceKeyError};
