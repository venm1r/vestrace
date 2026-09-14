use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::embedding::{CanonicalGenerationSnapshot, EmbeddingSpaceKey};
use vestrace_domain::retrieval::{HydratedRevision, RevisionRef};
use vestrace_domain::{CorpusGenerationId, RetrievalCandidate, WorkspaceId, id::RetrievalRunId};

use super::NormalizedRetrievalRequest;
use crate::{ApplicationError, RequestContext};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCorpusGeneration {
    pub embedding_space_key: EmbeddingSpaceKey,
    pub corpus_generation_id: CorpusGenerationId,
}

#[async_trait]
pub trait CorpusGenerationResolver: Send + Sync {
    async fn resolve_canonical(
        &self,
        _context: &RequestContext,
        _space_name: &str,
        _model: &str,
    ) -> Result<CanonicalGenerationSnapshot, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding-canonical-generation-required".into(),
        ))
    }

    async fn resolve(
        &self,
        context: &RequestContext,
        space_name: &str,
        model: &str,
    ) -> Result<ResolvedCorpusGeneration, ApplicationError>;
}

pub type SharedCorpusGenerationResolver = Arc<dyn CorpusGenerationResolver>;

#[async_trait]
pub trait TextRetriever: Send + Sync {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError>;
}

#[async_trait]
pub trait VectorRetriever: Send + Sync {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError>;
}

#[async_trait]
pub trait ExactRetriever: Send + Sync {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError>;
}

#[async_trait]
pub trait StructuredRetriever: Send + Sync {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError>;
}

/// What happened to one retrieval channel.
///
/// Every configured channel gets an entry, including the ones that failed. A
/// journal listing only successes cannot answer "was the vector channel
/// consulted?", because the two reasons it might be absent — not configured,
/// and failed — are exactly the two an investigation needs to tell apart.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ChannelOutcome {
    Succeeded {
        candidates: usize,
    },
    Failed {
        reason: String,
    },
    /// The channel answered, and its answer was "not from the canonical corpus".
    ///
    /// Distinct from `Failed` because nothing broke: a missing local index or an
    /// unactivated transition is a known state of the embedding side, and the
    /// reason is drawn from `EmbeddingRetrievalDegradation`, a closed set. A
    /// failure reason is an unbounded error string, so the two cannot share a
    /// variant without making the closed vocabulary unreadable in the journal.
    Degraded {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChannelRecord {
    pub channel: String,
    #[serde(flatten)]
    pub outcome: ChannelOutcome,
}

impl ChannelRecord {
    pub fn succeeded(channel: impl Into<String>, candidates: usize) -> Self {
        Self {
            channel: channel.into(),
            outcome: ChannelOutcome::Succeeded { candidates },
        }
    }

    pub fn failed(channel: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            outcome: ChannelOutcome::Failed {
                reason: reason.into(),
            },
        }
    }

    /// Record a closed degradation. The reason comes from
    /// `EmbeddingRetrievalDegradation::as_str`, never from an error message.
    pub fn degraded(channel: impl Into<String>, reason: &'static str) -> Self {
        Self {
            channel: channel.into(),
            outcome: ChannelOutcome::Degraded {
                reason: reason.to_owned(),
            },
        }
    }

    /// True when the channel contributed no candidates a caller may rely on,
    /// whether it broke or declined.
    pub fn is_degraded(&self) -> bool {
        matches!(
            self.outcome,
            ChannelOutcome::Failed { .. } | ChannelOutcome::Degraded { .. }
        )
    }
}

/// One journal entry for one retrieval.
///
/// A record rather than a positional argument list. The previous signature took
/// six parameters and was already `#[allow(clippy::too_many_arguments)]`; every
/// field the journal was missing would have made it worse, which is part of why
/// the parameters and channels were never added.
#[derive(Clone, Debug)]
pub struct RetrievalRunRecord {
    pub run_id: RetrievalRunId,
    pub query: String,
    pub intent: String,
    /// The normalized parameters that produced this result. Two retrievals with
    /// the same query and intent can legitimately differ because these did, and
    /// without them the journal cannot be replayed.
    pub parameters: serde_json::Value,
    /// Every configured channel and its outcome.
    pub channels: Vec<ChannelRecord>,
    pub candidate_count: usize,
    pub execution_time_ms: i32,
}

impl RetrievalRunRecord {
    /// Build the entry from the request that produced it.
    pub fn from_request(
        run_id: RetrievalRunId,
        request: &NormalizedRetrievalRequest,
        channels: Vec<ChannelRecord>,
        candidate_count: usize,
        execution_time_ms: i32,
    ) -> Self {
        Self {
            run_id,
            query: request.query.clone(),
            // Serialised, not `Debug`-formatted. `format!("{:?}").to_lowercase()`
            // produced "semanticrecall", which matches neither the variant name
            // nor the "semantic_recall" the API accepts — so a journal entry
            // could not be correlated with the request that produced it without
            // knowing about the mangling.
            intent: serde_json::to_value(&request.intent)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{:?}", request.intent)),
            parameters: serde_json::json!({
                "time_perspective": format!("{:?}", request.time_perspective),
                "allowed_statuses": request
                    .allowed_statuses
                    .iter()
                    .map(|status| format!("{status:?}"))
                    .collect::<Vec<_>>(),
                "allowed_kinds": request
                    .allowed_kinds
                    .iter()
                    .map(|kind| format!("{kind:?}"))
                    .collect::<Vec<_>>(),
                "channel_limit": request.channel_limit,
                "token_budget": request.token_budget,
                "include_explanation": request.include_explanation,
            }),
            channels,
            candidate_count,
            execution_time_ms,
        }
    }

    /// The channels that failed or declined, in the order they were attempted.
    pub fn degraded_channels(&self) -> Vec<String> {
        self.channels
            .iter()
            .filter(|record| record.is_degraded())
            .map(|record| record.channel.clone())
            .collect()
    }

    /// Bind the admission decision to the exact policy and every withheld
    /// reference in the same durable run record as the query.
    pub fn with_policy_decision(
        mut self,
        policy_version: &str,
        withheld: &[vestrace_domain::retrieval::WithheldRevision],
    ) -> Self {
        if let Some(parameters) = self.parameters.as_object_mut() {
            parameters.insert(
                "retrieval_policy_version".to_owned(),
                serde_json::Value::String(policy_version.to_owned()),
            );
            parameters.insert(
                "withheld".to_owned(),
                serde_json::to_value(withheld).unwrap_or(serde_json::Value::Null),
            );
        }
        self
    }
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait RetrievalJournal: Send + Sync {
    async fn record_run(
        &self,
        context: &RequestContext,
        record: &RetrievalRunRecord,
    ) -> Result<(), ApplicationError>;

    async fn record_context_pack(
        &self,
        context: &RequestContext,
        pack_id: vestrace_domain::id::ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        token_budget: u32,
        used_tokens: u32,
        items: &serde_json::Value,
    ) -> Result<(), ApplicationError>;
}

/// Resolves revision references into the content they name.
///
/// Separate from the retrievers on purpose. A retriever *finds* candidates; a
/// hydrator *resolves* a reference. Collapsing the two is what let content
/// reach a caller because the search happened to carry it along, with nothing
/// checking that the text was the text of the revision the reference named.
#[async_trait]
pub trait RevisionHydrator: Send + Sync {
    /// Resolve each reference to the exact revision it names.
    ///
    /// Implementations must select on the revision id, never on "the current
    /// revision of this memory". A reference that resolves to nothing must be
    /// absent from the result rather than substituted, so the caller can report
    /// it as missing.
    async fn hydrate(
        &self,
        context: &RequestContext,
        references: &[RevisionRef],
    ) -> Result<Vec<HydratedRevision>, ApplicationError>;
    /// Revalidate the exact canonical pin at the authoritative content read.
    /// General hydration remains available to callers without a canonical pin.
    async fn hydrate_for_retrieval(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
        references: &[RevisionRef],
    ) -> Result<Vec<HydratedRevision>, ApplicationError> {
        if request.embedding_space_key.is_canonical() {
            return Err(ApplicationError::Unavailable(
                "canonical retrieval hydration is not configured".to_owned(),
            ));
        }
        self.hydrate(context, references).await
    }
}

pub type SharedRevisionHydrator = Arc<dyn RevisionHydrator>;

pub type SharedTextRetriever = Arc<dyn TextRetriever>;
pub type SharedVectorRetriever = Arc<dyn VectorRetriever>;
pub type SharedExactRetriever = Arc<dyn ExactRetriever>;
pub type SharedStructuredRetriever = Arc<dyn StructuredRetriever>;
pub type SharedRetrievalJournal = Arc<dyn RetrievalJournal>;
