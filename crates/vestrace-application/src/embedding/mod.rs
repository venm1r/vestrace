//! Governed embedding job acceptance.
//!
//! Dispatch itself is not here and must not move here: an embedding job reaches
//! the provider through `ProviderDispatchRepository`, the same trait a Run step
//! uses, and this module exists only for the step before that — turning a
//! request to embed something into a durable job with its identities fixed.

pub mod job;

pub use job::{AcceptEmbeddingJob, EmbeddingJobRepository, SharedEmbeddingJobRepository};
