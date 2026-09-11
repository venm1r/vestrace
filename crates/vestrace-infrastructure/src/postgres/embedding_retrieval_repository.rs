//! PostgreSQL authority for job-owned retrieval fences, results and retries.
//!
//! Nothing here binds a query vector or a query digest.  A result reaches the
//! database as safe references, ranks and scores only.

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext,
    embedding::{
        AcceptRetrievalAttempt, EmbeddingRetrievalRepository, FinalizeRetrievalResult,
        ObserveRetrievalGenerationChange, RetrievalAttemptAdmission, RetrievalMemberReference,
        RetryRetrievalGenerationChanged,
    },
};
use vestrace_domain::{
    CorpusGenerationId, EmbeddingJobId, ModelQualificationRevisionId, ModelRevisionId,
    embedding::{CanonicalEmbeddingSpace, CanonicalGenerationSnapshot, EmbeddingSpaceKey},
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgEmbeddingRetrievalRepository {
    store: PgStore,
}

impl PgEmbeddingRetrievalRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EmbeddingRetrievalRepository for PgEmbeddingRetrievalRepository {
    async fn pinned_attempt(
        &self,
        context: &RequestContext,
        job_id: vestrace_domain::EmbeddingJobId,
    ) -> Result<(uuid::Uuid, CanonicalGenerationSnapshot), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        // The generation's own numbers, not the fence's, for everything the
        // registry keys an index by. The fence records what it pinned so a
        // later reader can see it; the index was built from the generation, and
        // a snapshot assembled from two sources could name a pair that never
        // existed together.
        let row: (
            uuid::Uuid,
            String,
            uuid::Uuid,
            uuid::Uuid,
            uuid::Uuid,
            String,
            String,
            String,
            i32,
            uuid::Uuid,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            "SELECT fence.id, registration.name, registration.model_revision_id, \
                    registration.model_qualification_revision_id, \
                    registration.request_shape_revision_id, \
                    registration.adapter_profile_revision, registration.returned_model, \
                    registration.encoding_format, registration.dimensions, \
                    generation.id, generation.generation_epoch, \
                    generation.captured_guard_version, generation.corpus_revision, \
                    generation.built_through_projection_ordinal, generation.member_count \
               FROM embedding_retrieval_fences AS fence \
               JOIN embedding_space_registrations AS registration \
                 ON registration.workspace_id = fence.workspace_id \
                AND registration.id = fence.space_registration_id \
               JOIN embedding_corpus_generations AS generation \
                 ON generation.workspace_id = fence.workspace_id \
                AND generation.id = fence.generation_id \
              WHERE fence.workspace_id = $1 AND fence.job_id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        let space = EmbeddingSpaceKey::canonical(
            context.workspace_id,
            row.1,
            CanonicalEmbeddingSpace {
                model_revision_id: ModelRevisionId::from_uuid(row.2),
                model_qualification_revision_id: ModelQualificationRevisionId::from_uuid(row.3),
                request_shape_revision_id: row.4,
                adapter_profile_revision: row.5,
                returned_model: row.6,
                encoding_format: row.7,
                dimensions: u32::try_from(row.8).map_err(|_| {
                    ApplicationError::Storage("stored space dimensions are negative".to_owned())
                })?,
            },
        )
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let snapshot = CanonicalGenerationSnapshot::new(
            space,
            CorpusGenerationId::from_uuid(row.9),
            non_negative(row.10, "generation epoch")?,
            positive(row.11, "captured guard version")?,
            non_negative(row.12, "corpus revision")?,
            // Zero is lawful: a generation captured from an empty corpus has
            // reached no projection ordinal, and the snapshot's own validator
            // requires only the epoch and the guard version to be positive.
            non_negative(row.13, "built-through ordinal")?,
            non_negative(row.14, "member count")?,
        )
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok((row.0, snapshot))
    }

    async fn resolve_members(
        &self,
        context: &RequestContext,
        projection_ids: &[uuid::Uuid],
    ) -> Result<Vec<RetrievalMemberReference>, ApplicationError> {
        if projection_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        // The link from a projection back to what it means: the projection's
        // source material, the intent that governs it, and the memory revision
        // that intent names as its owner. A projection whose source has been
        // erased has no row here, which is why the caller drops rather than
        // guesses.
        let rows: Vec<(uuid::Uuid, uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
            "SELECT DISTINCT dependency.projection_id, revision.memory_id, revision.id \
               FROM embedding_projection_source_dependencies AS dependency \
               JOIN content_materials AS material \
                 ON material.workspace_id = dependency.workspace_id \
                AND material.id = dependency.source_material_id \
               JOIN material_key_creation_intents AS intent \
                 ON intent.workspace_id = material.workspace_id \
                AND intent.id = material.intent_id \
                AND intent.owner_kind = 'memory_revision' \
               JOIN memory_revisions AS revision \
                 ON revision.workspace_id = intent.workspace_id \
                AND revision.id = intent.owner_id \
              WHERE dependency.workspace_id = $1 \
                AND dependency.projection_id = ANY($2) \
                AND material.state = 'live'",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(projection_ids)
        .fetch_all(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(rows
            .into_iter()
            .map(
                |(projection_id, memory_id, revision_id)| RetrievalMemberReference {
                    projection_id,
                    memory_id,
                    revision_id,
                },
            )
            .collect())
    }

    async fn accept_attempt(
        &self,
        context: &RequestContext,
        command: AcceptRetrievalAttempt,
    ) -> Result<RetrievalAttemptAdmission, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let fence_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_accept_embedding_retrieval_attempt($1,$2,$3,$4,$5)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.job_id.as_uuid())
        .bind(command.request_id.as_uuid())
        .bind(command.space_registration_id)
        .bind(command.deadline)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        // Read the pinned tuple back from the row the database actually wrote,
        // so the caller can never revalidate against a snapshot it invented.
        let row: (uuid::Uuid, uuid::Uuid, i64, i64) = sqlx::query_as(
            "SELECT space_registration_id, generation_id, generation_epoch, guard_version \
             FROM embedding_retrieval_fences WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(fence_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(RetrievalAttemptAdmission {
            fence_id,
            generation_id: row.1,
            generation_epoch: non_negative(row.2, "generation epoch")?,
            guard_version: positive(row.3, "guard version")?,
        })
    }

    async fn finalize_result(
        &self,
        context: &RequestContext,
        command: FinalizeRetrievalResult,
    ) -> Result<(), ApplicationError> {
        let mut memory_ids = Vec::with_capacity(command.references.len());
        let mut revision_ids = Vec::with_capacity(command.references.len());
        let mut ranks = Vec::with_capacity(command.references.len());
        let mut scores = Vec::with_capacity(command.references.len());
        for (index, reference) in command.references.iter().enumerate() {
            if reference.ordinal as usize != index {
                return Err(ApplicationError::Policy(
                    "retrieval result ordinals must be contiguous and ascending".to_owned(),
                ));
            }
            if !reference.score.is_finite() {
                return Err(ApplicationError::Policy(
                    "retrieval result scores must be finite".to_owned(),
                ));
            }
            memory_ids.push(reference.memory_id);
            revision_ids.push(reference.revision_id);
            ranks.push(i64::from(reference.rank));
            scores.push(f64::from(reference.score));
        }
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.job_id.as_uuid())
        .bind(command.fence_id)
        .bind(memory_ids)
        .bind(revision_ids)
        .bind(ranks)
        .bind(scores)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(())
    }

    async fn observe_generation_change(
        &self,
        context: &RequestContext,
        command: ObserveRetrievalGenerationChange,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_observe_embedding_retrieval_generation_change($1,$2,$3,$4)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.job_id.as_uuid())
        .bind(command.fence_id)
        .bind(command.reason.as_str())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(())
    }

    async fn authorize_retry(
        &self,
        context: &RequestContext,
        command: RetryRetrievalGenerationChanged,
    ) -> Result<EmbeddingJobId, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        // The version the caller agreed to, read in the transaction that
        // authorizes rather than by the caller beforehand. It is an agreement
        // check and not a lock: a predecessor that earned a retry is terminal,
        // so its version does not move on its own, and what this catches is a
        // caller acting on a reading it took before something else acted.
        let observed: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.predecessor_job_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        let observed = observed.ok_or_else(|| {
            ApplicationError::Conflict(
                "the retrieval retry predecessor does not exist in this workspace".to_owned(),
            )
        })?;
        if non_negative(observed, "predecessor version")? != command.expected_predecessor_version {
            return Err(ApplicationError::Conflict(format!(
                "the retrieval retry predecessor is at version {observed}, not the expected {}",
                command.expected_predecessor_version
            )));
        }
        let successor: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_authorize_embedding_retrieval_retry($1,$2,$3,$4,$5)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.predecessor_job_id.as_uuid())
        .bind(command.successor_job_id.as_uuid())
        .bind(command.successor_request_id.as_uuid())
        .bind(command.idempotency_key.as_str())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_retrieval_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(EmbeddingJobId::from_uuid(successor))
    }
}

fn non_negative(value: i64, what: &str) -> Result<u64, ApplicationError> {
    u64::try_from(value)
        .map_err(|_| ApplicationError::Internal(format!("{what} must be non-negative")))
}

fn positive(value: i64, what: &str) -> Result<u64, ApplicationError> {
    if value <= 0 {
        return Err(ApplicationError::Internal(format!(
            "{what} must be positive"
        )));
    }
    non_negative(value, what)
}

fn map_retrieval_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("40001") | Some("23505") => {
            ApplicationError::Conflict("EMBEDDING_RETRIEVAL_CONFLICT".to_owned())
        }
        Some("23514") | Some("22023") => ApplicationError::Policy(
            error
                .as_database_error()
                .map(|database| database.message().to_owned())
                .unwrap_or_else(|| "embedding retrieval refused".to_owned()),
        ),
        _ => ApplicationError::Storage(error.to_string()),
    }
}
