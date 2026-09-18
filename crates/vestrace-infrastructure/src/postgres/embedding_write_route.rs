//! The governed on-write embedding route.
//!
//! A memory that was just written has to reach the corpus somehow. The retired
//! route embedded it inline and wrote the vector into `memory_embeddings`,
//! which the canonical transition closed; this one does what every other
//! embedding does. It materializes the revision's content as a governed content
//! material and accepts an ordinary `delivery` job, and the worker takes it
//! from there through the same provider path a rebuild uses.
//!
//! Nothing here calls a provider. An outbox handler that made a network call
//! would put the model on the write path's latency and failure budget, which is
//! exactly what the durable job exists to avoid.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, GovernedInputSealer, MaterialKeyVault, OutboxHandler, OutboxMessage,
    RequestContext, embedding::LegacyAdoptionBlocker,
};

use super::PgStore;
use super::embedding_adoption_repository::{
    GovernedEmbeddingJobPurpose, PgGovernedContentMaterializer, PgGovernedEmbeddingJobFactory,
};

/// Turns one written memory into one governed delivery job.
///
/// One handler per topic, because `OutboxHandler` is keyed by topic and a
/// memory's revision changes what it means: a route that only followed creation
/// would leave the corpus answering with the superseded meaning and look
/// perfectly healthy doing it.
pub struct PgGovernedMemoryEmbeddingHandler<V, C> {
    store: PgStore,
    materializer: Arc<PgGovernedContentMaterializer<V, C>>,
    jobs: Arc<PgGovernedEmbeddingJobFactory>,
    topic: String,
}

impl<V, C> PgGovernedMemoryEmbeddingHandler<V, C> {
    pub fn new(
        store: PgStore,
        materializer: Arc<PgGovernedContentMaterializer<V, C>>,
        jobs: Arc<PgGovernedEmbeddingJobFactory>,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            store,
            materializer,
            jobs,
            topic: topic.into(),
        }
    }

    /// The revision a memory currently means, and only while it is active.
    ///
    /// `None` covers both ways this can lawfully find nothing: the memory was
    /// deleted between the write and this message, or it never existed. Neither
    /// is a failure of this route.
    async fn active_revision(
        &self,
        context: &RequestContext,
        memory_id: Uuid,
    ) -> Result<Option<Uuid>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT active_revision_id FROM memories \
              WHERE workspace_id=$1 AND id=$2 AND status='active' \
                AND active_revision_id IS NOT NULL",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(memory_id)
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(row.map(|row| row.0))
    }

    /// The canonical space retrieval is currently answered from.
    ///
    /// Read from the qualification head rather than from configuration: the
    /// head is what activation moves, so a job accepted against anything else
    /// could publish into a space no query will ever read.
    async fn active_registration(
        &self,
        context: &RequestContext,
    ) -> Result<Option<Uuid>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT head.active_space_registration_id \
               FROM model_qualification_heads AS head \
               JOIN embedding_space_registrations AS registration \
                 ON registration.workspace_id = head.workspace_id \
                AND registration.id = head.active_space_registration_id \
              WHERE head.workspace_id = $1 \
                AND head.active_space_registration_id IS NOT NULL \
                AND registration.registration_kind = 'canonical' \
              ORDER BY head.version DESC LIMIT 1",
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(row.map(|row| row.0))
    }
}

#[async_trait]
impl<V, C> OutboxHandler for PgGovernedMemoryEmbeddingHandler<V, C>
where
    V: MaterialKeyVault + Send + Sync + 'static,
    C: GovernedInputSealer + Send + Sync + 'static,
{
    fn topic(&self) -> &str {
        &self.topic
    }

    async fn handle(
        &self,
        context: &RequestContext,
        message: &OutboxMessage,
    ) -> Result<(), ApplicationError> {
        let memory_id = message
            .payload
            .get("memory_id")
            .and_then(|value| value.as_str())
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "outbox message {} on topic {} carries no memory_id",
                    message.id, message.topic
                ))
            })?;

        // No canonical space means this deployment has not finished the
        // transition. Refused rather than skipped: the message stays retriable
        // and visible, and it will succeed unaided once a space is active. A
        // silent success here would let a workspace accumulate memories no
        // query can find while every write looked fine.
        let Some(registration) = self.active_registration(context).await? else {
            return Err(ApplicationError::Unavailable(
                "no canonical embedding space is active for this workspace".to_owned(),
            ));
        };

        // Deleted or gone. Not an error: there is nothing left to embed and no
        // later attempt would find one. The silence is not unreported either --
        // `memory.active_memory_is_embedded` is the invariant that exists to
        // say a workspace's corpus is behind, and it reads the corpus rather
        // than this handler's opinion of it.
        let Some(revision_id) = self.active_revision(context, memory_id).await? else {
            return Ok(());
        };

        match self
            .materializer
            .materialize_revision(context, revision_id)
            .await?
        {
            Ok(source) => {
                self.jobs
                    .create_job(
                        context,
                        registration,
                        source,
                        GovernedEmbeddingJobPurpose {
                            kind: vestrace_domain::embedding::EmbeddingJobKind::Delivery,
                            // The revision, not the memory: revising a memory
                            // means a different thing and must produce its own
                            // job, while a redelivered message about the same
                            // revision must not.
                            subject_id: revision_id,
                            cause: "memory-write",
                            action: "embedding.job.delivery_accepted",
                            summary: "embed one written memory revision through the governed \
                                      provider path",
                            detail: serde_json::json!({
                                "memory_id": memory_id,
                                "memory_revision_id": revision_id,
                            }),
                        },
                    )
                    .await?;
                Ok(())
            }
            Err(LegacyAdoptionBlocker::SourceRevisionAbsent)
            | Err(LegacyAdoptionBlocker::SourceContentErased) => {
                // Empty or erased content. Nothing to embed, and no retry
                // would change that.
                Ok(())
            }
            Err(blocker) => Err(ApplicationError::Unavailable(format!(
                "the written memory could not be materialized: {}",
                blocker.as_str()
            ))),
        }
    }
}
