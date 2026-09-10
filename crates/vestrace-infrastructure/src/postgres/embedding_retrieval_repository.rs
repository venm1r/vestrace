//! PostgreSQL authority for job-owned retrieval fences, results and retries.
//!
//! Nothing here binds a query vector or a query digest.  A result reaches the
//! database as safe references, ranks and scores only.

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext,
    embedding::{
        AcceptRetrievalAttempt, EmbeddingRetrievalRepository, FinalizeRetrievalResult,
        ObserveRetrievalGenerationChange, RetrievalAttemptAdmission,
        RetryRetrievalGenerationChanged,
    },
};
use vestrace_domain::EmbeddingJobId;

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
