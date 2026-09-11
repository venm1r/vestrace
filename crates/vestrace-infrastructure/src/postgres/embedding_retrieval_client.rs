//! The one governed route from a retrieval request to the embedding side.
//!
//! This does not embed anything. It accepts a durable `retrieval_query` job
//! under a generation fence and waits for a worker to answer it, within the
//! request's own budget and no longer. That split is deliberate: the answer
//! comes from a process-local index, and only a worker holds one, so a surface
//! that tried to answer inline would either duplicate the index in every web
//! process or call a provider outside the governed dispatch path.
//!
//! The query text is a governed input material like every other thing sent to
//! a provider -- the reconstruction contract requires the evidence to name at
//! least one, and a query is content that was sent. It is erased as soon as the
//! attempt is terminal, on every branch, because a query has no reason to
//! outlive its answer.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, GovernedInputSealer, MaterialErasureService, MaterialKeyVault,
    NormalizedRetrievalRequest, RequestContext,
    embedding::{
        AcceptRetrievalAttempt, DegradedRetrievalAttempt, EmbeddingRetrievalDegradation,
        EmbeddingRetrievalJobClient, EmbeddingRetrievalOutcome, EmbeddingRetrievalRepository,
    },
};
use vestrace_domain::{
    ContentMaterialId, EmbeddingJobId, RetrievalCandidate, embedding::EmbeddingJobKind,
    embedding::RetrievalGenerationChangedReason, id::RetrievalRunId,
};

use super::embedding_adoption_repository::{
    GovernedEmbeddingJobPurpose, PgGovernedContentMaterializer, PgGovernedEmbeddingJobFactory,
};
use super::embedding_retrieval_repository::PgEmbeddingRetrievalRepository;
use super::{PgMaterialErasureRepository, PgStore};

/// How often the client asks whether the attempt is terminal.
///
/// Short enough that a fast worker is not made to look slow, long enough that a
/// waiting request is not a busy loop against the database. The budget, not
/// this, is what bounds the wait.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct PgEmbeddingRetrievalJobClient<V, C> {
    store: PgStore,
    materializer: Arc<PgGovernedContentMaterializer<V, C>>,
    jobs: Arc<PgGovernedEmbeddingJobFactory>,
    retrieval: Arc<PgEmbeddingRetrievalRepository>,
    erasure: Arc<MaterialErasureService<PgMaterialErasureRepository, Arc<V>>>,
}

impl<V, C> PgEmbeddingRetrievalJobClient<V, C> {
    pub fn new(
        store: PgStore,
        materializer: Arc<PgGovernedContentMaterializer<V, C>>,
        jobs: Arc<PgGovernedEmbeddingJobFactory>,
        erasure: Arc<MaterialErasureService<PgMaterialErasureRepository, Arc<V>>>,
    ) -> Self {
        Self {
            retrieval: Arc::new(PgEmbeddingRetrievalRepository::new(store.clone())),
            store,
            materializer,
            jobs,
            erasure,
        }
    }

    /// The canonical space retrieval is currently answered from.
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

    /// What the attempt has settled into, or `None` while it has not.
    ///
    /// Three terminal shapes, and the order they are asked in is the order
    /// they exclude each other: a result is the answer, a recorded generation
    /// change is the one degradation a retry may follow, and a job that failed
    /// with neither is a worker that could not answer at all.
    async fn settled(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
        job_id: EmbeddingJobId,
    ) -> Result<Option<EmbeddingRetrievalOutcome>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let row: (Option<Uuid>, Option<String>, String) = sqlx::query_as(
            "SELECT (SELECT id FROM embedding_retrieval_results \
                      WHERE workspace_id=$1 AND job_id=$2), \
                    (SELECT reason FROM embedding_retrieval_generation_changes \
                      WHERE workspace_id=$1 AND job_id=$2), \
                    (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        let outcome = if let Some(result_id) = row.0 {
            // The stored result names references, ranks and scores and nothing
            // else, which is the whole point of it. A candidate is a hydrated
            // thing, so the memory and its exact revision are read here -- and
            // filtered by what the request allows, because a channel that
            // returned a status the caller excluded would be answering a
            // different question.
            let rows: Vec<HydratedReference> = sqlx::query_as(
                "SELECT reference.memory_id, reference.revision_id, reference.rank, \
                        reference.score, memory.kind, memory.status, memory.state_revision, \
                        revision.revision_number, revision.content, revision.valid_from, \
                        revision.valid_until, revision.created_at, fence.generation_id \
                   FROM embedding_retrieval_result_references AS reference \
                   JOIN embedding_retrieval_results AS result \
                     ON result.workspace_id = reference.workspace_id \
                    AND result.id = reference.result_id \
                   JOIN embedding_retrieval_fences AS fence \
                     ON fence.workspace_id = result.workspace_id AND fence.id = result.fence_id \
                   JOIN memories AS memory \
                     ON memory.workspace_id = reference.workspace_id \
                    AND memory.id = reference.memory_id \
                   JOIN memory_revisions AS revision \
                     ON revision.workspace_id = reference.workspace_id \
                    AND revision.id = reference.revision_id \
                  WHERE reference.workspace_id = $1 AND reference.result_id = $2 \
                    AND memory.status = ANY($3) \
                    AND (cardinality($4::text[]) = 0 OR memory.kind = ANY($4)) \
                  ORDER BY reference.ordinal",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(result_id)
            .bind(statuses(request))
            .bind(kinds(request))
            .fetch_all(transaction.connection())
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            let mut candidates = Vec::with_capacity(rows.len());
            for (position, row) in rows.into_iter().enumerate() {
                candidates.push(candidate(row, position)?);
            }
            Some(EmbeddingRetrievalOutcome::Completed(candidates))
        } else if let Some(reason) = row.1 {
            Some(EmbeddingRetrievalOutcome::Degraded(
                DegradedRetrievalAttempt::of(
                    job_id,
                    EmbeddingRetrievalDegradation::GenerationChanged(changed_reason(&reason)?),
                ),
            ))
        } else if row.2 == "failed_definite" || row.2 == "cancelled" {
            // Terminal with neither record: no worker could answer it. The
            // local index is the only thing that can, so its absence is what
            // this says, and it says it without inventing a new vocabulary.
            Some(EmbeddingRetrievalOutcome::Degraded(
                DegradedRetrievalAttempt::of(
                    job_id,
                    EmbeddingRetrievalDegradation::MissingLocalIndex,
                ),
            ))
        } else {
            None
        };
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(outcome)
    }
}

#[derive(sqlx::FromRow)]
struct HydratedReference {
    memory_id: Uuid,
    revision_id: Uuid,
    rank: i32,
    score: f64,
    kind: String,
    status: String,
    state_revision: i32,
    revision_number: i32,
    content: String,
    valid_from: Option<chrono::DateTime<Utc>>,
    valid_until: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    generation_id: Uuid,
}

fn candidate(
    row: HydratedReference,
    position: usize,
) -> Result<RetrievalCandidate, ApplicationError> {
    Ok(RetrievalCandidate {
        memory_id: vestrace_domain::MemoryId::from_uuid(row.memory_id),
        revision_id: vestrace_domain::id::MemoryRevisionId::from_uuid(row.revision_id),
        kind: super::text_retriever::memory_kind_from_str(&row.kind)?,
        memory_status: super::text_retriever::memory_status_from_str(&row.status)?,
        revision_number: u32::try_from(row.revision_number)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        content: row.content,
        // The hydration policy boundary sets this, not a finder.
        classification: None,
        valid_from: row.valid_from,
        valid_until: row.valid_until,
        revision_created_at: row.created_at,
        source_generation: u32::try_from(row.state_revision)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        corpus_generation_id: vestrace_domain::CorpusGenerationId::from_uuid(row.generation_id),
        score: row.score as f32,
        // The stored rank is where the search put it; the channel rank is where
        // it sits in what this channel is actually returning, and a reference
        // the request filtered out leaves a gap in the first but not the
        // second.
        channel_rank: u32::try_from(position + 1)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        channel: "vector".to_owned(),
        explanation: format!("governed embedding retrieval, rank {}", row.rank),
        conflict_ids: Vec::new(),
    })
}

/// The stored spellings, matching the text channel's exactly. Two channels
/// filtering one request by different vocabularies would answer different
/// questions and fuse the results as if they had not.
fn statuses(request: &NormalizedRetrievalRequest) -> Vec<&'static str> {
    use vestrace_domain::MemoryStatus;
    request
        .allowed_statuses
        .iter()
        .map(|status| match status {
            MemoryStatus::Active => "active",
            MemoryStatus::Candidate => "candidate",
            MemoryStatus::Superseded => "superseded",
            MemoryStatus::Rejected => "rejected",
            MemoryStatus::Expired => "expired",
            MemoryStatus::Deleted => "deleted",
        })
        .collect()
}

fn kinds(request: &NormalizedRetrievalRequest) -> Vec<&'static str> {
    use vestrace_domain::MemoryKind;
    request
        .allowed_kinds
        .iter()
        .map(|kind| match kind {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Constraint => "constraint",
            MemoryKind::Decision => "decision",
            MemoryKind::Task => "task",
            MemoryKind::Procedure => "procedure",
            MemoryKind::Observation => "observation",
            MemoryKind::Outcome => "outcome",
            MemoryKind::Summary => "summary",
        })
        .collect()
}

fn changed_reason(stored: &str) -> Result<RetrievalGenerationChangedReason, ApplicationError> {
    RetrievalGenerationChangedReason::ALL
        .into_iter()
        .find(|reason| reason.as_str() == stored)
        .ok_or_else(|| {
            ApplicationError::Storage(format!(
                "stored retrieval generation change reason {stored} is unknown"
            ))
        })
}

#[async_trait]
impl<V, C> EmbeddingRetrievalJobClient for PgEmbeddingRetrievalJobClient<V, C>
where
    V: MaterialKeyVault + Send + Sync + 'static,
    C: GovernedInputSealer + Send + Sync + 'static,
{
    async fn retrieve(
        &self,
        context: &RequestContext,
        request_id: RetrievalRunId,
        request: &NormalizedRetrievalRequest,
        deadline: DateTime<Utc>,
    ) -> Result<EmbeddingRetrievalOutcome, ApplicationError> {
        // Nothing canonical to answer from. Reported rather than refused: a
        // deployment mid-transition still answers from its other channels, and
        // the journal records that this one declined and why.
        let Some(registration) = self.active_registration(context).await? else {
            // No attempt was admitted, so the degradation names none. A
            // caller cannot retry what was never tried.
            return Ok(EmbeddingRetrievalOutcome::Degraded(
                DegradedRetrievalAttempt::unattempted(
                    EmbeddingRetrievalDegradation::LegacyAdoptionPending,
                ),
            ));
        };

        // The query becomes a governed input material, because that is what
        // every other thing sent to a provider is and the reconstruction
        // contract requires the evidence to name one. It is erased below.
        let source = self
            .materializer
            .materialize_bytes(
                context,
                "retrieval_query",
                request_id.as_uuid(),
                request.query.as_bytes(),
            )
            .await?;

        let outcome = self
            .attempt(
                context,
                registration,
                request_id,
                source.material_id,
                request,
                deadline,
            )
            .await;

        // On every branch, including the failing one. A query that outlived the
        // answer it was asked for is the thing this whole path exists to avoid,
        // and an erasure that only ran on success would leave exactly the
        // queries that went wrong lying around.
        let erased = self
            .erasure
            .erase_content(context, ContentMaterialId::from_uuid(source.material_id))
            .await;
        match (outcome, erased) {
            (Ok(outcome), Ok(_)) => Ok(outcome),
            // The answer is real and the caller may have it; the query material
            // surviving is a durable fault for an operator, not a reason to
            // discard a result the provider was already paid for.
            (Ok(outcome), Err(_)) => Ok(outcome),
            (Err(error), _) => Err(error),
        }
    }
}

impl<V, C> PgEmbeddingRetrievalJobClient<V, C>
where
    V: MaterialKeyVault + Send + Sync + 'static,
    C: GovernedInputSealer + Send + Sync + 'static,
{
    /// Accept one attempt and wait out the budget for its terminal outcome.
    async fn attempt(
        &self,
        context: &RequestContext,
        registration: Uuid,
        request_id: RetrievalRunId,
        material_id: Uuid,
        request: &NormalizedRetrievalRequest,
        deadline: DateTime<Utc>,
    ) -> Result<EmbeddingRetrievalOutcome, ApplicationError> {
        let job_id = self
            .jobs
            .create_job(
                context,
                registration,
                vestrace_application::embedding::MaterializedSource {
                    material_id,
                    intent_id: Uuid::nil(),
                },
                GovernedEmbeddingJobPurpose {
                    kind: EmbeddingJobKind::RetrievalQuery,
                    // The logical request identity, minted once by
                    // `RetrievalService::search`. Two dispatches of one request
                    // are one job; no layer below invents a second identity.
                    subject_id: request_id.as_uuid(),
                    cause: "retrieval-query",
                    action: "embedding.job.retrieval_accepted",
                    summary: "embed one retrieval query through the governed provider path",
                    detail: serde_json::json!({ "request_id": request_id.as_uuid() }),
                },
            )
            .await?;

        // The fence pins the generation this attempt must answer from. It is
        // taken before any worker can claim the job, because the claim itself
        // requires a live fence.
        self.retrieval
            .accept_attempt(
                context,
                AcceptRetrievalAttempt {
                    job_id,
                    request_id,
                    space_registration_id: registration,
                    deadline,
                },
            )
            .await?;

        loop {
            if let Some(outcome) = self.settled(context, request, job_id).await? {
                return Ok(outcome);
            }
            if Utc::now() >= deadline {
                // Accepted, not answered. The job stays durable and a worker
                // may still complete it; what expired is this caller's
                // patience, and the other channels still have an answer.
                return Ok(EmbeddingRetrievalOutcome::Degraded(
                    DegradedRetrievalAttempt::of(job_id, EmbeddingRetrievalDegradation::Pending),
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}
