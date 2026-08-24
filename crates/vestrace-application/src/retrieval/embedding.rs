//! Embeddings: producing them, storing them, and what a missing one means.
//!
//! # Why this module did not exist
//!
//! `memory_embeddings` has been in the schema since migration 0009 and held zero
//! rows, because nothing ever wrote one. `RetrievalService::with_channels` takes
//! an optional `SharedVectorRetriever` and nothing ever supplied one. So the
//! fusion step — reciprocal rank fusion across channels, verified by its own
//! conformance case — has been fusing a single channel with itself, and
//! "degraded because a channel failed" could not arise because there was only
//! ever one.
//!
//! # Why embedding is not part of the memory write
//!
//! Producing an embedding is a network call to a model. Putting it inside the
//! transaction that writes a memory would make creating a memory fail whenever
//! the model is unreachable, which trades a real capability for an index.
//!
//! So the write path is unchanged and embeddings are filled in afterwards, by
//! `vestrace rebuild embeddings`. The gap that leaves is a *stated* one: the
//! invariant registry reports memories without an embedding, exactly as it
//! reports memories without a search document. An index that lags is a known
//! state; an index that silently never fills is the defect this system has
//! already been bitten by twice.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use vestrace_domain::id::{EmbeddingSpaceId, MemoryId};

use crate::{ApplicationError, EmbeddingInput, RequestContext, SharedGovernedEmbeddingProvider};

/// One embedding space: a model, its output width, and the name a deployment
/// knows it by.
///
/// The space is what makes two vectors comparable. Searching a query embedded by
/// one model against vectors produced by another returns distances that are
/// arithmetically fine and meaningless, so every read names the space it is
/// searching and every write names the space it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddingSpace {
    pub id: EmbeddingSpaceId,
    pub name: String,
    pub model: String,
    pub dimensions: u32,
}

/// Turns text into a vector.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// The model this provider embeds with, so a caller can check it against the
    /// space it is writing into rather than assume they agree.
    fn model(&self) -> &str;

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError>;
}

pub type SharedEmbeddingProvider = Arc<dyn EmbeddingProvider>;

/// Stores and finds embeddings.
#[async_trait]
pub trait EmbeddingStore: Send + Sync {
    /// The space a deployment writes into, created on first use so that a fresh
    /// database does not need a provisioning step nobody would remember.
    async fn ensure_space(
        &self,
        context: &RequestContext,
        name: &str,
        model: &str,
        dimensions: u32,
    ) -> Result<EmbeddingSpace, ApplicationError>;

    /// Memories in this workspace with no embedding in this space, oldest first.
    async fn memories_without_embedding(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
        limit: u32,
    ) -> Result<Vec<PendingEmbedding>, ApplicationError>;

    /// Write one memory's vector. Replaces the existing one for that space, so
    /// re-embedding a memory does not accumulate copies of it in the index.
    async fn upsert(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
        memory_id: MemoryId,
        embedding: &[f32],
    ) -> Result<(), ApplicationError>;

    /// How many memories in this workspace still have no embedding.
    async fn missing_count(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
    ) -> Result<i64, ApplicationError>;
}

pub type SharedEmbeddingStore = Arc<dyn EmbeddingStore>;

/// A memory awaiting an embedding, with the text to embed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingEmbedding {
    pub memory_id: MemoryId,
    pub content: String,
    pub classification: Option<String>,
}

/// Fills in missing embeddings.
///
/// ```compile_fail
/// use vestrace_application::retrieval::{
///     EmbeddingBackfillService, SharedEmbeddingProvider, SharedEmbeddingStore,
/// };
///
/// let raw: SharedEmbeddingProvider = todo!();
/// let store: SharedEmbeddingStore = todo!();
/// let _ = EmbeddingBackfillService::new(raw, store, "space");
/// ```
pub struct EmbeddingBackfillService {
    provider: SharedGovernedEmbeddingProvider,
    store: SharedEmbeddingStore,
    space_name: String,
    rebuild_invocation_id: uuid::Uuid,
    next_batch_ordinal: AtomicU32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackfillReport {
    pub embedded: usize,
    pub still_missing: i64,
}

impl EmbeddingBackfillService {
    pub fn new(
        provider: SharedGovernedEmbeddingProvider,
        store: SharedEmbeddingStore,
        space_name: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            store,
            space_name: space_name.into(),
            rebuild_invocation_id: uuid::Uuid::now_v7(),
            next_batch_ordinal: AtomicU32::new(1),
        }
    }

    /// Embed up to `batch` memories that have none.
    pub async fn run(
        &self,
        context: &RequestContext,
        batch: u32,
    ) -> Result<BackfillReport, ApplicationError> {
        // The space is derived from the provider rather than configured
        // separately: a space whose declared model differs from the one actually
        // embedding into it is the failure this type exists to prevent.
        let batch_ordinal = self
            .next_batch_ordinal
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(1))
            })
            .unwrap_or(u32::MAX);
        let probe = self
            .provider
            .probe_dimensions(self.rebuild_invocation_id, batch_ordinal)
            .await?;
        let dimensions = probe.first().map(|vector| vector.len()).ok_or_else(|| {
            ApplicationError::Unavailable("the embedding provider returned nothing".into())
        })?;
        let dimensions = u32::try_from(dimensions).map_err(|_| {
            ApplicationError::InvalidConfiguration("embedding width does not fit in u32".into())
        })?;

        let space = self
            .store
            .ensure_space(context, &self.space_name, self.provider.model(), dimensions)
            .await?;

        let pending = self
            .store
            .memories_without_embedding(context, &space, batch)
            .await?;

        let mut embedded = 0usize;
        if !pending.is_empty() {
            let inputs = pending
                .iter()
                .map(|item| EmbeddingInput::new(item.content.clone(), item.classification.clone()))
                .collect::<Vec<_>>();
            let vectors = self
                .provider
                .embed_backfill(self.rebuild_invocation_id, batch_ordinal, &inputs)
                .await?;
            if vectors.len() != pending.len() {
                return Err(ApplicationError::Unavailable(format!(
                    "the embedding provider returned {} vectors for {} inputs, so which \
                     memory each belongs to is unknowable",
                    vectors.len(),
                    pending.len()
                )));
            }
            for (item, vector) in pending.iter().zip(vectors) {
                self.store
                    .upsert(context, &space, item.memory_id, &vector)
                    .await?;
                embedded += 1;
            }
        }

        Ok(BackfillReport {
            embedded,
            still_missing: self.store.missing_count(context, &space).await?,
        })
    }

    pub fn space_name(&self) -> &str {
        &self.space_name
    }
}

/// Embeds a memory when the write path announces it.
///
/// # Why this is a handler rather than part of the write
///
/// The transaction that writes a memory must not depend on a model being
/// reachable. The outbox is the seam that already exists for exactly this: the
/// write records what happened, and something else acts on it.
///
/// Idempotent because it must be — outbox delivery is at-least-once, and the
/// store upserts on `(workspace, memory, space)`, so a redelivered message
/// re-embeds rather than duplicating.
///
/// ```compile_fail
/// use vestrace_application::retrieval::{
///     EmbedMemoryHandler, SharedEmbeddingProvider, SharedEmbeddingStore,
///     SharedMemoryTextSource,
/// };
///
/// let raw: SharedEmbeddingProvider = todo!();
/// let store: SharedEmbeddingStore = todo!();
/// let memories: SharedMemoryTextSource = todo!();
/// let _ = EmbedMemoryHandler::new(raw, store, memories, "space", "memory.created");
/// ```
pub struct EmbedMemoryHandler {
    provider: SharedGovernedEmbeddingProvider,
    store: SharedEmbeddingStore,
    memories: SharedMemoryTextSource,
    space_name: String,
    topic: String,
}

impl EmbedMemoryHandler {
    pub fn new(
        provider: SharedGovernedEmbeddingProvider,
        store: SharedEmbeddingStore,
        memories: SharedMemoryTextSource,
        space_name: impl Into<String>,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            store,
            memories,
            space_name: space_name.into(),
            topic: topic.into(),
        }
    }
}

/// Reads the text that should be embedded for a memory.
#[async_trait]
pub trait MemoryTextSource: Send + Sync {
    /// The active revision's content, or `None` if the memory is gone or no
    /// longer active.
    async fn active_text(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
    ) -> Result<Option<EmbeddingInput>, ApplicationError>;
}

pub type SharedMemoryTextSource = Arc<dyn MemoryTextSource>;

#[async_trait]
impl crate::OutboxHandler for EmbedMemoryHandler {
    fn topic(&self) -> &str {
        &self.topic
    }

    async fn handle(
        &self,
        context: &RequestContext,
        message: &crate::OutboxMessage,
    ) -> Result<(), ApplicationError> {
        let memory_id = message
            .payload
            .get("memory_id")
            .and_then(|value| value.as_str())
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .map(MemoryId::from_uuid)
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "outbox message {} on topic {} carries no memory_id",
                    message.id, message.topic
                ))
            })?;

        let Some(input) = self.memories.active_text(context, memory_id).await? else {
            // The memory was superseded or removed between the write and the
            // delivery. There is nothing to embed and nothing wrong; the message
            // is acknowledged so it does not retry forever.
            return Ok(());
        };

        let vectors = self
            .provider
            .embed_delivery(message.id, message.attempts.saturating_add(1), &input)
            .await?;
        let vector = vectors.into_iter().next().ok_or_else(|| {
            ApplicationError::Unavailable("the embedding provider returned nothing".into())
        })?;

        let dimensions = u32::try_from(vector.len()).map_err(|_| {
            ApplicationError::InvalidConfiguration("embedding width does not fit in u32".into())
        })?;
        let space = self
            .store
            .ensure_space(context, &self.space_name, self.provider.model(), dimensions)
            .await?;

        self.store.upsert(context, &space, memory_id, &vector).await
    }
}
