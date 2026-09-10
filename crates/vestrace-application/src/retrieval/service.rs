use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use vestrace_domain::{
    ContextPack, RetrievalCandidate, WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
    retrieval::{ClassificationPolicy, HydrationOutcome, RevisionRef, WithheldRevision},
};

use crate::{
    ApplicationError, RequestContext,
    embedding::{
        EmbeddingRetrievalDegradation, EmbeddingRetrievalOutcome, SharedEmbeddingRetrievalJobClient,
    },
    retrieval::{
        ChannelRecord, ContextPackBuilder, NormalizedRetrievalRequest, RetrievalRequest,
        RetrievalRunRecord, SharedCorpusGenerationResolver, SharedExactRetriever,
        SharedRetrievalJournal, SharedRevisionHydrator, SharedStructuredRetriever,
        SharedTextRetriever, SharedVectorRetriever, reciprocal_rank_fusion_pinned, rerank,
    },
};

/// How long a governed retrieval query may wait for its answer.
///
/// Finite by construction: the fence the attempt is admitted against carries
/// this deadline into the database, and an attempt without one could hold a
/// generation pinned indefinitely.
const MAXIMUM_RETRIEVAL_WAIT: Duration = Duration::from_secs(120);

pub struct RetrievalService {
    text_retriever: SharedTextRetriever,
    vector_retriever: Option<SharedVectorRetriever>,
    exact_retriever: Option<SharedExactRetriever>,
    structured_retriever: Option<SharedStructuredRetriever>,
    journal: SharedRetrievalJournal,
    revision_hydrator: Option<SharedRevisionHydrator>,
    classification_policy: Option<ClassificationPolicy>,
    retrieval_policy_version: Option<String>,
    corpus_generation_resolver: Option<(SharedCorpusGenerationResolver, String, String)>,
    embedding_retrieval_client: Option<(SharedEmbeddingRetrievalJobClient, Duration)>,
}

impl RetrievalService {
    pub fn new(text_retriever: SharedTextRetriever, journal: SharedRetrievalJournal) -> Self {
        Self::with_channels(text_retriever, None, None, None, journal)
    }

    pub fn with_channels(
        text_retriever: SharedTextRetriever,
        vector_retriever: Option<SharedVectorRetriever>,
        exact_retriever: Option<SharedExactRetriever>,
        structured_retriever: Option<SharedStructuredRetriever>,
        journal: SharedRetrievalJournal,
    ) -> Self {
        Self {
            text_retriever,
            vector_retriever,
            exact_retriever,
            structured_retriever,
            journal,
            revision_hydrator: None,
            classification_policy: None,
            retrieval_policy_version: None,
            corpus_generation_resolver: None,
            embedding_retrieval_client: None,
        }
    }

    pub fn with_hydration(
        mut self,
        revision_hydrator: SharedRevisionHydrator,
        classification_policy: ClassificationPolicy,
        retrieval_policy_version: impl Into<String>,
    ) -> Self {
        let retrieval_policy_version = retrieval_policy_version.into();
        self.revision_hydrator = Some(revision_hydrator);
        self.classification_policy = Some(classification_policy);
        self.retrieval_policy_version =
            (!retrieval_policy_version.trim().is_empty()).then_some(retrieval_policy_version);
        self
    }

    pub fn with_corpus_generation_resolver(
        mut self,
        resolver: SharedCorpusGenerationResolver,
        space_name: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        self.corpus_generation_resolver = Some((resolver, space_name.into(), model.into()));
        self
    }

    /// Install the governed embedding retrieval client as the vector channel.
    ///
    /// This does not sit next to the legacy vector retriever, it replaces it.
    /// A deployment with both configured is refused at search time rather than
    /// silently preferring one: a fallback from the fenced path to an
    /// unfenced one would answer from an unpinned corpus precisely when the
    /// fence was doing its job.
    pub fn with_embedding_retrieval_client(
        mut self,
        client: SharedEmbeddingRetrievalJobClient,
        wait_budget: Duration,
    ) -> Result<Self, ApplicationError> {
        if wait_budget.is_zero() || wait_budget > MAXIMUM_RETRIEVAL_WAIT {
            return Err(ApplicationError::Policy(format!(
                "retrieval wait budget must be positive and at most {} seconds",
                MAXIMUM_RETRIEVAL_WAIT.as_secs()
            )));
        }
        self.embedding_retrieval_client = Some((client, wait_budget));
        Ok(self)
    }

    pub async fn search(
        &self,
        context: &RequestContext,
        request: RetrievalRequest,
    ) -> Result<RetrievalResult, ApplicationError> {
        if request.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "retrieval workspace does not match request context".to_owned(),
            ));
        }
        if self.revision_hydrator.is_none()
            || self.classification_policy.is_none()
            || self.retrieval_policy_version.is_none()
        {
            return Err(ApplicationError::Policy(
                "retrieval hydration policy is not configured".to_owned(),
            ));
        }
        if self.embedding_retrieval_client.is_some() && self.vector_retriever.is_some() {
            return Err(ApplicationError::Policy(
                "retrieval cannot serve one vector channel from two authorities".to_owned(),
            ));
        }

        let workspace_id = request.workspace_id;
        let Some((resolver, space_name, model)) = &self.corpus_generation_resolver else {
            return Err(ApplicationError::Unavailable(
                "retrieval corpus generation resolver is not configured".to_owned(),
            ));
        };
        let pin = resolver.resolve(context, space_name, model).await?;
        let normalized = NormalizedRetrievalRequest::normalize_with_pin(
            request,
            pin.embedding_space_key,
            pin.corpus_generation_id,
        )?;
        let run_id = normalized.request_id;

        let start = Instant::now();
        let mut channels: Vec<Vec<RetrievalCandidate>> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        // One entry per **configured** channel, whatever the outcome. The
        // successes and the failures were previously tracked in two separate
        // lists, and only the failures ever left this function, so a journal
        // could not say which channels had actually been consulted.
        let mut channel_records: Vec<ChannelRecord> = Vec::new();
        let mut successful_channels = 0usize;
        // The closed reason the embedding side gave, if it gave one. Kept as
        // the value rather than its string so a caller deciding whether to
        // authorize a retry reads the vocabulary, not a journal message.
        let mut embedding_degradation: Option<EmbeddingRetrievalDegradation> = None;

        match self.text_retriever.search(context, &normalized).await {
            Ok(results) => {
                successful_channels += 1;
                channel_records.push(ChannelRecord::succeeded("text", results.len()));
                channels.push(results);
            }
            Err(e) => {
                warnings.push(format!("text channel failed: {e}"));
                channel_records.push(ChannelRecord::failed("text", e.to_string()));
            }
        }

        // The three optional channels. An absent one is skipped rather than
        // recorded as a failure: a channel that was never asked is not a
        // channel that broke, and conflating them is what makes a journal
        // unable to answer whether a channel was consulted.
        macro_rules! poll_channel {
            ($retriever:expr, $name:literal) => {
                if let Some(retriever) = &$retriever {
                    match retriever.search(context, &normalized).await {
                        Ok(results) => {
                            successful_channels += 1;
                            channel_records.push(ChannelRecord::succeeded($name, results.len()));
                            channels.push(results);
                        }
                        Err(e) => {
                            warnings.push(format!("{} channel failed: {e}", $name));
                            channel_records.push(ChannelRecord::failed($name, e.to_string()));
                        }
                    }
                }
            };
        }

        // The vector channel is served by exactly one authority: the governed
        // embedding client when it is installed, and otherwise whatever legacy
        // retriever is configured. The refusal above makes "both" impossible.
        if let Some((client, wait_budget)) = &self.embedding_retrieval_client {
            let deadline = chrono::Utc::now()
                + chrono::Duration::from_std(*wait_budget).map_err(|error| {
                    ApplicationError::Internal(format!(
                        "retrieval wait budget is unusable: {error}"
                    ))
                })?;
            match client
                .retrieve(context, run_id, &normalized, deadline)
                .await
            {
                Ok(EmbeddingRetrievalOutcome::Completed(results)) => {
                    successful_channels += 1;
                    channel_records.push(ChannelRecord::succeeded("vector", results.len()));
                    channels.push(results);
                }
                Ok(EmbeddingRetrievalOutcome::Degraded(degradation)) => {
                    // Not a failure: the embedding side answered, and its answer
                    // was that no canonical corpus can serve this query right
                    // now. It contributes no candidates, so it is not counted as
                    // a successful channel either.
                    warnings.push(format!("vector channel degraded: {}", degradation.as_str()));
                    channel_records.push(ChannelRecord::degraded("vector", degradation.as_str()));
                    embedding_degradation = Some(degradation);
                }
                Err(e) => {
                    warnings.push(format!("vector channel failed: {e}"));
                    channel_records.push(ChannelRecord::failed("vector", e.to_string()));
                }
            }
        } else {
            poll_channel!(self.vector_retriever, "vector");
        }
        poll_channel!(self.exact_retriever, "exact");
        poll_channel!(self.structured_retriever, "structured");

        if successful_channels == 0 {
            return Err(ApplicationError::Unavailable(
                "all retrieval channels failed".to_owned(),
            ));
        }

        let degraded_channels = channel_records
            .iter()
            .filter(|record| record.is_degraded())
            .map(|record| record.channel.clone())
            .collect::<Vec<_>>();

        // Policy sees every exact revision discovered by every channel before
        // memory-level fusion chooses a representative. Otherwise a historical
        // sibling could disappear without a structured withholding record.
        let mut seen_references = HashSet::new();
        let references = channels
            .iter()
            .flatten()
            .map(|candidate| RevisionRef {
                memory_id: candidate.memory_id,
                revision_id: candidate.revision_id,
            })
            .filter(|reference| seen_references.insert(*reference))
            .collect::<Vec<_>>();
        let resolved = self
            .revision_hydrator
            .as_ref()
            .expect("hydrator was checked above")
            .hydrate(context, &references)
            .await?;
        let outcome = HydrationOutcome::apply_policy(
            resolved,
            &references,
            self.classification_policy
                .as_ref()
                .expect("classification policy was checked above"),
        );
        let channels = channels
            .into_iter()
            .map(|channel| {
                channel
                    .into_iter()
                    .filter_map(|mut candidate| {
                        let revision = outcome.revisions.iter().find(|revision| {
                            revision.memory_id == candidate.memory_id
                                && revision.revision_id == candidate.revision_id
                        })?;
                        candidate.revision_number = revision.revision_number;
                        candidate.memory_status = revision.memory_status;
                        candidate.content = revision.content.clone();
                        candidate.classification = revision.classification.clone();
                        candidate.valid_from = revision.valid_from;
                        candidate.valid_until = revision.valid_until;
                        candidate.revision_created_at = revision.created_at;
                        Some(candidate)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let fused =
            reciprocal_rank_fusion_pinned(&channels, 60.0, normalized.corpus_generation_id)?;
        let ranked = rerank(fused);
        let candidates: Vec<RetrievalCandidate> = ranked
            .iter()
            .map(|ranked| ranked.candidate.clone())
            .collect();
        let withheld = outcome.withheld;
        let retrieval_policy_version = self
            .retrieval_policy_version
            .as_ref()
            .expect("retrieval policy version was checked above")
            .clone();

        let elapsed_ms = start.elapsed().as_millis() as i32;

        let record = RetrievalRunRecord::from_request(
            run_id,
            &normalized,
            channel_records,
            candidates.len(),
            elapsed_ms,
        )
        .with_policy_decision(&retrieval_policy_version, &withheld);
        self.journal.record_run(context, &record).await?;

        Ok(RetrievalResult {
            run_id,
            workspace_id,
            candidates,
            withheld,
            retrieval_policy_version,
            degraded: !degraded_channels.is_empty(),
            degraded_channels,
            embedding_degradation,
            warnings,
            normalized,
        })
    }

    pub async fn build_context(
        &self,
        context: &RequestContext,
        result: &RetrievalResult,
        token_budget: u32,
    ) -> Result<ContextPack, ApplicationError> {
        if result.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "context workspace does not match request context".to_owned(),
            ));
        }
        if self.revision_hydrator.is_none()
            || self.classification_policy.is_none()
            || self.retrieval_policy_version.is_none()
        {
            return Err(ApplicationError::Policy(
                "retrieval hydration policy is not configured".to_owned(),
            ));
        }

        let pack_id = ContextPackId::new();
        let builder = ContextPackBuilder::new(token_budget);

        let mut pack = builder.build_with_temporal_perspective(
            result.workspace_id,
            context.principal_id,
            result.run_id,
            result.normalized.time_perspective,
            &result.candidates,
        )?;
        pack.retrieval_policy_version = self
            .retrieval_policy_version
            .as_ref()
            .expect("retrieval policy version was checked above")
            .clone();

        // One call rather than three assignments: `with_degradation` keeps the
        // flag and the channel list from disagreeing, which three separate
        // field writes could not.
        let pack =
            pack.with_degradation(result.degraded_channels.clone(), result.warnings.clone())?;
        let pack = pack.with_withholding(result.withheld.clone());

        let items_json = serde_json::json!({
            "sections": &pack.sections,
            "withheld": &pack.withheld,
        });

        self.journal
            .record_context_pack(
                context,
                pack_id,
                result.run_id,
                result.workspace_id,
                token_budget,
                pack.used_tokens,
                &items_json,
            )
            .await?;

        Ok(pack)
    }
}

#[derive(Debug)]
pub struct RetrievalResult {
    pub run_id: RetrievalRunId,
    pub workspace_id: WorkspaceId,
    pub candidates: Vec<RetrievalCandidate>,
    pub withheld: Vec<WithheldRevision>,
    pub retrieval_policy_version: String,
    pub degraded: bool,
    pub degraded_channels: Vec<String>,
    /// Present only when the governed embedding client declined. The one
    /// retryable member of the vocabulary is `GenerationChanged`; every other
    /// value tells a caller that retrying changes nothing on its own.
    pub embedding_degradation: Option<EmbeddingRetrievalDegradation>,
    pub warnings: Vec<String>,
    pub normalized: NormalizedRetrievalRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval::ChannelOutcome;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex, OnceLock};
    use vestrace_domain::{
        CorpusGenerationId, MemoryKind, MemoryStatus, PrincipalId, RetrievalCandidate, WorkspaceId,
        embedding::EmbeddingSpaceKey,
        retrieval::{ClassificationPolicy, HydratedRevision, RevisionRef, WithholdingReason},
    };

    struct StubTextRetriever;

    #[async_trait]
    impl crate::TextRetriever for StubTextRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            panic!("text retrieval must not run for a workspace mismatch");
        }
    }

    struct SuccessfulTextRetriever;

    #[async_trait]
    impl crate::TextRetriever for SuccessfulTextRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Ok(vec![candidate("text")])
        }
    }

    struct SuccessfulVectorRetriever;

    #[async_trait]
    impl crate::VectorRetriever for SuccessfulVectorRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Ok(vec![candidate("vector")])
        }
    }

    struct OneCandidateTextRetriever {
        candidate: RetrievalCandidate,
    }

    struct ManyCandidateTextRetriever {
        candidates: Vec<RetrievalCandidate>,
    }

    #[async_trait]
    impl crate::TextRetriever for ManyCandidateTextRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Ok(self.candidates.clone())
        }
    }

    #[async_trait]
    impl crate::TextRetriever for OneCandidateTextRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Ok(vec![self.candidate.clone()])
        }
    }

    struct OneRevisionHydrator {
        revision: HydratedRevision,
    }

    struct ManyRevisionHydrator {
        revisions: Vec<HydratedRevision>,
    }

    #[async_trait]
    impl crate::retrieval::RevisionHydrator for ManyRevisionHydrator {
        async fn hydrate(
            &self,
            _context: &RequestContext,
            references: &[RevisionRef],
        ) -> Result<Vec<HydratedRevision>, ApplicationError> {
            Ok(self
                .revisions
                .iter()
                .filter(|revision| {
                    references.iter().any(|reference| {
                        reference.memory_id == revision.memory_id
                            && reference.revision_id == revision.revision_id
                    })
                })
                .cloned()
                .collect())
        }
    }

    #[async_trait]
    impl crate::retrieval::RevisionHydrator for OneRevisionHydrator {
        async fn hydrate(
            &self,
            _context: &RequestContext,
            references: &[RevisionRef],
        ) -> Result<Vec<HydratedRevision>, ApplicationError> {
            if references.iter().any(|reference| {
                reference.memory_id == self.revision.memory_id
                    && reference.revision_id == self.revision.revision_id
            }) {
                Ok(vec![self.revision.clone()])
            } else {
                Ok(Vec::new())
            }
        }
    }

    struct EchoRevisionHydrator;

    #[async_trait]
    impl crate::retrieval::RevisionHydrator for EchoRevisionHydrator {
        async fn hydrate(
            &self,
            _context: &RequestContext,
            references: &[RevisionRef],
        ) -> Result<Vec<HydratedRevision>, ApplicationError> {
            Ok(references
                .iter()
                .map(|reference| HydratedRevision {
                    memory_id: reference.memory_id,
                    revision_id: reference.revision_id,
                    revision_number: 1,
                    memory_status: MemoryStatus::Active,
                    content: "hydrated test content".to_owned(),
                    classification: None,
                    valid_from: None,
                    valid_until: None,
                    created_at: vestrace_domain::now(),
                })
                .collect())
        }
    }

    fn governed(service: RetrievalService) -> RetrievalService {
        service.with_hydration(
            Arc::new(EchoRevisionHydrator),
            ClassificationPolicy::permissive(),
            "test-retrieval-policy",
        )
    }

    struct StubCorpusGenerationResolver;

    #[async_trait]
    impl crate::retrieval::CorpusGenerationResolver for StubCorpusGenerationResolver {
        async fn resolve(
            &self,
            context: &RequestContext,
            space_name: &str,
            model: &str,
        ) -> Result<crate::retrieval::ResolvedCorpusGeneration, ApplicationError> {
            Ok(crate::retrieval::ResolvedCorpusGeneration {
                embedding_space_key: EmbeddingSpaceKey::new(
                    context.workspace_id,
                    space_name,
                    model,
                    3,
                )
                .expect("test resolver receives valid space configuration"),
                corpus_generation_id: test_corpus_generation_id(),
            })
        }
    }

    fn test_corpus_generation_id() -> CorpusGenerationId {
        static ID: OnceLock<CorpusGenerationId> = OnceLock::new();
        *ID.get_or_init(CorpusGenerationId::new)
    }

    struct FailingExactRetriever;

    #[async_trait]
    impl crate::ExactRetriever for FailingExactRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Err(ApplicationError::Unavailable(
                "exact unavailable".to_owned(),
            ))
        }
    }

    struct FailingTextRetriever;

    #[async_trait]
    impl crate::TextRetriever for FailingTextRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Err(ApplicationError::Unavailable("text unavailable".to_owned()))
        }
    }

    struct FailingVectorRetriever;

    #[async_trait]
    impl crate::VectorRetriever for FailingVectorRetriever {
        async fn search(
            &self,
            _context: &RequestContext,
            _request: &NormalizedRetrievalRequest,
        ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
            Err(ApplicationError::Unavailable(
                "vector unavailable".to_owned(),
            ))
        }
    }

    fn candidate(channel: &str) -> RetrievalCandidate {
        RetrievalCandidate {
            memory_id: vestrace_domain::MemoryId::new(),
            revision_id: vestrace_domain::id::MemoryRevisionId::new(),
            kind: MemoryKind::Fact,
            memory_status: MemoryStatus::Active,
            revision_number: 1,
            content: format!("candidate from {channel}"),
            classification: None,
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
            corpus_generation_id: test_corpus_generation_id(),
            score: 0.9,
            channel_rank: 1,
            channel: channel.to_owned(),
            explanation: format!("{channel} match"),
            conflict_ids: Vec::new(),
        }
    }

    struct StubJournal;

    #[async_trait]
    impl crate::RetrievalJournal for StubJournal {
        async fn record_run(
            &self,
            _context: &RequestContext,
            _record: &RetrievalRunRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn record_context_pack(
            &self,
            _context: &RequestContext,
            _pack_id: ContextPackId,
            _retrieval_run_id: RetrievalRunId,
            _workspace_id: WorkspaceId,
            _token_budget: u32,
            _used_tokens: u32,
            _items: &serde_json::Value,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct CapturingJournal {
        run_parameters: Mutex<Option<serde_json::Value>>,
        context_items: Mutex<Option<serde_json::Value>>,
    }

    #[async_trait]
    impl crate::RetrievalJournal for CapturingJournal {
        async fn record_run(
            &self,
            _context: &RequestContext,
            record: &RetrievalRunRecord,
        ) -> Result<(), ApplicationError> {
            *self.run_parameters.lock().unwrap() = Some(record.parameters.clone());
            Ok(())
        }

        async fn record_context_pack(
            &self,
            _context: &RequestContext,
            _pack_id: ContextPackId,
            _retrieval_run_id: RetrievalRunId,
            _workspace_id: WorkspaceId,
            _token_budget: u32,
            _used_tokens: u32,
            items: &serde_json::Value,
        ) -> Result<(), ApplicationError> {
            *self.context_items.lock().unwrap() = Some(items.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn search_rejects_workspace_mismatch() {
        let requested_workspace = WorkspaceId::new();
        let trusted_workspace = WorkspaceId::new();
        let context = RequestContext::new(trusted_workspace, PrincipalId::new());
        let service = governed(RetrievalService::new(
            Arc::new(StubTextRetriever),
            Arc::new(StubJournal),
        ));

        let result = service
            .search(
                &context,
                RetrievalRequest::new(requested_workspace, "current fact"),
            )
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Policy(message))
                if message == "retrieval workspace does not match request context"
        ));
    }

    #[tokio::test]
    async fn search_without_a_hydration_boundary_fails_closed() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let service =
            RetrievalService::new(Arc::new(SuccessfulTextRetriever), Arc::new(StubJournal));

        let error = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Policy(message)
                if message == "retrieval hydration policy is not configured"
        ));
    }

    #[tokio::test]
    async fn search_with_a_blank_policy_version_fails_closed() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let service =
            RetrievalService::new(Arc::new(SuccessfulTextRetriever), Arc::new(StubJournal))
                .with_hydration(
                    Arc::new(EchoRevisionHydrator),
                    ClassificationPolicy::permissive(),
                    "   ",
                );

        let error = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Policy(message)
                if message == "retrieval hydration policy is not configured"
        ));
    }

    #[tokio::test]
    async fn search_returns_content_from_the_exact_hydrated_revision() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let raw_candidate = candidate("text");
        let hydrated = HydratedRevision {
            memory_id: raw_candidate.memory_id,
            revision_id: raw_candidate.revision_id,
            revision_number: 7,
            memory_status: MemoryStatus::Superseded,
            content: "authoritative revision content".to_owned(),
            classification: Some("internal".to_owned()),
            valid_from: None,
            valid_until: None,
            created_at: vestrace_domain::now(),
        };
        let service = RetrievalService::new(
            Arc::new(OneCandidateTextRetriever {
                candidate: raw_candidate,
            }),
            Arc::new(StubJournal),
        )
        .with_hydration(
            Arc::new(OneRevisionHydrator { revision: hydrated }),
            ClassificationPolicy::new(["internal"], false).unwrap(),
            "retrieval-policy-v2",
        )
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(
            result.candidates[0].content,
            "authoritative revision content"
        );
        assert_eq!(result.candidates[0].revision_number, 7);
        assert_eq!(result.candidates[0].memory_status, MemoryStatus::Superseded);
        assert_eq!(
            result.candidates[0].classification.as_deref(),
            Some("internal")
        );
        assert_eq!(result.retrieval_policy_version, "retrieval-policy-v2");
    }

    #[tokio::test]
    async fn search_reports_inadmissible_revision_without_disclosing_its_content() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let raw_candidate = candidate("text");
        let memory_id = raw_candidate.memory_id;
        let revision_id = raw_candidate.revision_id;
        let hydrated = HydratedRevision {
            memory_id,
            revision_id,
            revision_number: 1,
            memory_status: MemoryStatus::Active,
            content: "restricted secret".to_owned(),
            classification: Some("restricted".to_owned()),
            valid_from: None,
            valid_until: None,
            created_at: vestrace_domain::now(),
        };
        let journal = Arc::new(CapturingJournal::default());
        let service = RetrievalService::new(
            Arc::new(OneCandidateTextRetriever {
                candidate: raw_candidate,
            }),
            journal.clone(),
        )
        .with_hydration(
            Arc::new(OneRevisionHydrator { revision: hydrated }),
            ClassificationPolicy::new(["internal"], false).unwrap(),
            "retrieval-policy-v2",
        )
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert!(result.candidates.is_empty());
        assert_eq!(result.withheld.len(), 1);
        assert_eq!(result.withheld[0].memory_id, memory_id);
        assert_eq!(result.withheld[0].revision_id, revision_id);
        assert_eq!(
            result.withheld[0].reason,
            WithholdingReason::ClassificationNotAdmissible {
                classification: "restricted".to_owned(),
            }
        );
        assert!(
            !serde_json::to_string(&result.withheld)
                .unwrap()
                .contains("restricted secret")
        );
        let recorded = journal.run_parameters.lock().unwrap().clone().unwrap();
        assert_eq!(recorded["retrieval_policy_version"], "retrieval-policy-v2");
        assert_eq!(
            recorded["withheld"][0]["revision_id"],
            revision_id.to_string()
        );
        assert!(!recorded.to_string().contains("restricted secret"));
    }

    #[tokio::test]
    async fn inadmissible_sibling_cannot_displace_or_inflate_an_admitted_revision() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let first = candidate("text");
        let mut second = first.clone();
        second.revision_id = vestrace_domain::id::MemoryRevisionId::new();
        second.revision_number = 2;
        second.content = "second raw candidate".to_owned();
        second.channel_rank = 2;
        second.score = 0.99;
        let restricted_revision_id = second.revision_id;
        let revisions = [&first, &second]
            .into_iter()
            .map(|candidate| HydratedRevision {
                memory_id: candidate.memory_id,
                revision_id: candidate.revision_id,
                revision_number: candidate.revision_number,
                memory_status: MemoryStatus::Active,
                content: format!("hydrated revision {}", candidate.revision_number),
                classification: Some(
                    if candidate.revision_number == 1 {
                        "internal"
                    } else {
                        "restricted"
                    }
                    .to_owned(),
                ),
                valid_from: None,
                valid_until: None,
                created_at: vestrace_domain::now(),
            })
            .collect();
        let service = RetrievalService::new(
            Arc::new(ManyCandidateTextRetriever {
                candidates: vec![first, second],
            }),
            Arc::new(StubJournal),
        )
        .with_hydration(
            Arc::new(ManyRevisionHydrator { revisions }),
            ClassificationPolicy::new(["internal"], false).unwrap(),
            "retrieval-policy-v2",
        )
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].revision_number, 1);
        assert_eq!(
            result.candidates[0].classification.as_deref(),
            Some("internal")
        );
        assert_eq!(result.withheld.len(), 1);
        assert_eq!(result.withheld[0].revision_id, restricted_revision_id);
    }

    #[tokio::test]
    async fn build_context_rejects_result_from_another_workspace() {
        let result_workspace = WorkspaceId::new();
        let context_workspace = WorkspaceId::new();
        let service = governed(RetrievalService::new(
            Arc::new(StubTextRetriever),
            Arc::new(StubJournal),
        ));
        let normalized = NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
            result_workspace,
            "current fact",
        ))
        .unwrap();
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: result_workspace,
            candidates: Vec::new(),
            withheld: Vec::new(),
            retrieval_policy_version: "test-retrieval-policy".to_owned(),
            degraded: false,
            degraded_channels: Vec::new(),
            embedding_degradation: None,
            warnings: Vec::new(),
            normalized,
        };

        let error = service
            .build_context(
                &RequestContext::new(context_workspace, PrincipalId::new()),
                &result,
                100,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Policy(message)
                if message == "context workspace does not match request context"
        ));
    }

    #[tokio::test]
    async fn build_context_without_a_hydration_boundary_fails_closed() {
        let workspace = WorkspaceId::new();
        let service = RetrievalService::new(Arc::new(StubTextRetriever), Arc::new(StubJournal));
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: workspace,
            candidates: Vec::new(),
            withheld: Vec::new(),
            retrieval_policy_version: "test-retrieval-policy".to_owned(),
            degraded: false,
            degraded_channels: Vec::new(),
            embedding_degradation: None,
            warnings: Vec::new(),
            normalized: NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
                workspace, "fact",
            ))
            .unwrap(),
        };

        let error = service
            .build_context(
                &RequestContext::new(workspace, PrincipalId::new()),
                &result,
                100,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Policy(message)
                if message == "retrieval hydration policy is not configured"
        ));
    }

    #[tokio::test]
    async fn build_context_preserves_structured_withholding() {
        let workspace = WorkspaceId::new();
        let memory_id = vestrace_domain::MemoryId::new();
        let revision_id = vestrace_domain::id::MemoryRevisionId::new();
        let service = governed(RetrievalService::new(
            Arc::new(StubTextRetriever),
            Arc::new(StubJournal),
        ));
        let normalized = NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
            workspace,
            "restricted fact",
        ))
        .unwrap();
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: workspace,
            candidates: Vec::new(),
            withheld: vec![WithheldRevision {
                memory_id,
                revision_id,
                reason: WithholdingReason::ClassificationNotAdmissible {
                    classification: "restricted".to_owned(),
                },
            }],
            retrieval_policy_version: "test-retrieval-policy".to_owned(),
            degraded: false,
            degraded_channels: Vec::new(),
            embedding_degradation: None,
            warnings: Vec::new(),
            normalized,
        };

        let pack = service
            .build_context(
                &RequestContext::new(workspace, PrincipalId::new()),
                &result,
                100,
            )
            .await
            .unwrap();

        assert_eq!(pack.withheld, result.withheld);
    }

    #[tokio::test]
    async fn context_journal_persists_structured_withholding() {
        let workspace = WorkspaceId::new();
        let memory_id = vestrace_domain::MemoryId::new();
        let revision_id = vestrace_domain::id::MemoryRevisionId::new();
        let journal = Arc::new(CapturingJournal::default());
        let service = governed(RetrievalService::new(
            Arc::new(StubTextRetriever),
            journal.clone(),
        ));
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: workspace,
            candidates: Vec::new(),
            withheld: vec![WithheldRevision {
                memory_id,
                revision_id,
                reason: WithholdingReason::RevisionNotFound,
            }],
            retrieval_policy_version: "test-retrieval-policy".to_owned(),
            degraded: false,
            degraded_channels: Vec::new(),
            embedding_degradation: None,
            warnings: Vec::new(),
            normalized: NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
                workspace,
                "missing fact",
            ))
            .unwrap(),
        };

        service
            .build_context(
                &RequestContext::new(workspace, PrincipalId::new()),
                &result,
                100,
            )
            .await
            .unwrap();

        let recorded = journal.context_items.lock().unwrap().clone().unwrap();
        assert_eq!(
            recorded["withheld"][0]["memory_id"],
            serde_json::json!(memory_id)
        );
        assert_eq!(
            recorded["withheld"][0]["revision_id"],
            serde_json::json!(revision_id)
        );
        assert_eq!(recorded["withheld"][0]["reason"], "revision_not_found");
    }

    #[tokio::test]
    async fn build_context_preserves_temporal_perspective() {
        let workspace = WorkspaceId::new();
        let at = chrono::Utc::now();
        let service = governed(RetrievalService::new(
            Arc::new(SuccessfulTextRetriever),
            Arc::new(StubJournal),
        ));
        let normalized = NormalizedRetrievalRequest::normalize(
            RetrievalRequest::new(workspace, "historical fact")
                .with_time_perspective(vestrace_domain::TimePerspective::AsOf(at)),
        )
        .unwrap();
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: workspace,
            candidates: Vec::new(),
            withheld: Vec::new(),
            retrieval_policy_version: "retrieval-policy-v2".to_owned(),
            degraded: false,
            degraded_channels: Vec::new(),
            embedding_degradation: None,
            warnings: Vec::new(),
            normalized,
        };

        let pack = service
            .build_context(
                &RequestContext::new(workspace, PrincipalId::new()),
                &result,
                100,
            )
            .await
            .unwrap();

        assert_eq!(
            pack.temporal_perspective,
            vestrace_domain::TimePerspective::AsOf(at)
        );
    }

    #[tokio::test]
    async fn build_context_carries_the_configured_retrieval_policy_version() {
        let workspace = WorkspaceId::new();
        let raw_candidate = candidate("text");
        let hydrated = HydratedRevision {
            memory_id: raw_candidate.memory_id,
            revision_id: raw_candidate.revision_id,
            revision_number: 1,
            memory_status: MemoryStatus::Active,
            content: "fact".to_owned(),
            classification: Some("internal".to_owned()),
            valid_from: None,
            valid_until: None,
            created_at: vestrace_domain::now(),
        };
        let service = RetrievalService::new(
            Arc::new(OneCandidateTextRetriever {
                candidate: raw_candidate,
            }),
            Arc::new(StubJournal),
        )
        .with_hydration(
            Arc::new(OneRevisionHydrator { revision: hydrated }),
            ClassificationPolicy::new(["internal"], false).unwrap(),
            "retrieval-policy-v2",
        )
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );
        let context = RequestContext::new(workspace, PrincipalId::new());
        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        let pack = service.build_context(&context, &result, 100).await.unwrap();

        assert_eq!(pack.retrieval_policy_version, "retrieval-policy-v2");
    }

    #[tokio::test]
    async fn optional_channel_failure_marks_result_degraded_but_keeps_safe_results() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let service = governed(RetrievalService::with_channels(
            Arc::new(SuccessfulTextRetriever),
            Some(Arc::new(SuccessfulVectorRetriever)),
            Some(Arc::new(FailingExactRetriever)),
            None,
            Arc::new(StubJournal),
        ))
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert!(result.degraded);
        assert_eq!(result.degraded_channels, vec!["exact"]);
        assert!(result.candidates.len() >= 2);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("exact"))
        );
    }

    #[tokio::test]
    async fn all_channel_failure_is_fail_closed() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let service = governed(RetrievalService::with_channels(
            Arc::new(FailingTextRetriever),
            Some(Arc::new(FailingVectorRetriever)),
            None,
            None,
            Arc::new(StubJournal),
        ))
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        );

        let error = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Unavailable(message)
                if message == "all retrieval channels failed"
        ));
    }

    /// One governed embedding client, recording exactly what the service handed
    /// it and answering with whatever this test needs.
    struct FakeRetrievalClient {
        answer: Mutex<Option<EmbeddingRetrievalOutcome>>,
        seen: Mutex<Vec<(RetrievalRunId, chrono::DateTime<chrono::Utc>)>>,
    }

    impl FakeRetrievalClient {
        fn answering(answer: EmbeddingRetrievalOutcome) -> Arc<Self> {
            Arc::new(Self {
                answer: Mutex::new(Some(answer)),
                seen: Mutex::new(Vec::new()),
            })
        }

        fn calls(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl crate::embedding::EmbeddingRetrievalJobClient for FakeRetrievalClient {
        async fn retrieve(
            &self,
            _context: &RequestContext,
            request_id: RetrievalRunId,
            _request: &NormalizedRetrievalRequest,
            deadline: chrono::DateTime<chrono::Utc>,
        ) -> Result<EmbeddingRetrievalOutcome, ApplicationError> {
            self.seen.lock().unwrap().push((request_id, deadline));
            Ok(self
                .answer
                .lock()
                .unwrap()
                .take()
                .expect("the fake client answers one attempt"))
        }
    }

    struct RecordingJournal {
        runs: Mutex<Vec<RetrievalRunRecord>>,
    }

    impl RecordingJournal {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                runs: Mutex::new(Vec::new()),
            })
        }

        fn only_run(&self) -> RetrievalRunRecord {
            let runs = self.runs.lock().unwrap();
            assert_eq!(runs.len(), 1, "one search must journal exactly one run");
            runs[0].clone()
        }
    }

    #[async_trait]
    impl crate::RetrievalJournal for RecordingJournal {
        async fn record_run(
            &self,
            _context: &RequestContext,
            record: &RetrievalRunRecord,
        ) -> Result<(), ApplicationError> {
            self.runs.lock().unwrap().push(record.clone());
            Ok(())
        }

        async fn record_context_pack(
            &self,
            _context: &RequestContext,
            _pack_id: ContextPackId,
            _retrieval_run_id: RetrievalRunId,
            _workspace_id: WorkspaceId,
            _token_budget: u32,
            _used_tokens: u32,
            _items: &serde_json::Value,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    fn with_client(
        journal: Arc<dyn crate::RetrievalJournal>,
        client: Arc<FakeRetrievalClient>,
    ) -> RetrievalService {
        governed(RetrievalService::new(
            Arc::new(SuccessfulTextRetriever),
            journal,
        ))
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        )
        .with_embedding_retrieval_client(client, Duration::from_secs(30))
        .expect("thirty seconds is inside the wait bound")
    }

    fn vector_record(record: &RetrievalRunRecord) -> ChannelRecord {
        record
            .channels
            .iter()
            .find(|channel| channel.channel == "vector")
            .cloned()
            .expect("the vector channel must appear in the journal once configured")
    }

    /// The client serves the vector channel, and it is handed the identity the
    /// service already minted rather than being left to invent one.
    #[tokio::test]
    async fn the_embedding_client_serves_the_vector_channel_under_the_run_identity() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let client =
            FakeRetrievalClient::answering(EmbeddingRetrievalOutcome::Completed(vec![candidate(
                "vector",
            )]));
        let journal = RecordingJournal::new();
        let service = with_client(journal.clone(), client.clone());
        let before = chrono::Utc::now();

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert_eq!(client.calls(), 1);
        let (seen_run_id, deadline) = client.seen.lock().unwrap()[0];
        assert_eq!(
            seen_run_id, result.run_id,
            "the client must answer under the identity the journal records"
        );
        assert!(deadline > before, "the deadline must be ahead of the call");
        assert!(
            deadline <= before + chrono::Duration::seconds(31),
            "the deadline must be the configured budget, not an open one"
        );
        assert!(!result.degraded);
        assert!(result.embedding_degradation.is_none());
        assert!(result.candidates.len() >= 2, "both channels contributed");
        assert!(matches!(
            vector_record(&journal.only_run()).outcome,
            ChannelOutcome::Succeeded { candidates: 1 }
        ));
    }

    /// A decline is not a failure. It degrades the channel, keeps the other
    /// channels' results, and reaches the journal as a closed reason rather
    /// than as an error message.
    #[tokio::test]
    async fn a_declined_retrieval_degrades_the_vector_channel_with_a_closed_reason() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let client = FakeRetrievalClient::answering(EmbeddingRetrievalOutcome::Degraded(
            EmbeddingRetrievalDegradation::MissingLocalIndex,
        ));
        let journal = RecordingJournal::new();
        let service = with_client(journal.clone(), client.clone());

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        assert!(result.degraded);
        assert_eq!(result.degraded_channels, vec!["vector"]);
        assert_eq!(
            result.embedding_degradation,
            Some(EmbeddingRetrievalDegradation::MissingLocalIndex)
        );
        assert!(
            !result.candidates.is_empty(),
            "a declined vector channel must not discard the text channel"
        );
        let record = vector_record(&journal.only_run());
        assert!(record.is_degraded());
        match record.outcome {
            ChannelOutcome::Degraded { reason } => assert_eq!(reason, "missing_local_index"),
            other => panic!("a decline must not be journalled as {other:?}"),
        }
    }

    /// Only a generation change tells a caller that a successor is worth
    /// authorizing; the other degradations reach it as themselves.
    #[tokio::test]
    async fn only_a_generation_change_reaches_the_caller_as_retryable() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let client = FakeRetrievalClient::answering(EmbeddingRetrievalOutcome::Degraded(
            EmbeddingRetrievalDegradation::GenerationChanged(
                vestrace_domain::embedding::RetrievalGenerationChangedReason::Revoked,
            ),
        ));
        let service = with_client(Arc::new(StubJournal), client);

        let result = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .unwrap();

        let degradation = result
            .embedding_degradation
            .expect("a decline must be readable as a value");
        assert!(degradation.is_retryable());
        assert!(
            !EmbeddingRetrievalDegradation::TransitionNotReady.is_retryable(),
            "an unactivated transition is not fixed by asking again"
        );
    }

    /// Two vector authorities are refused before either one is consulted, so no
    /// deployment can fall back from the fenced path to an unfenced one.
    #[tokio::test]
    async fn two_vector_authorities_are_refused_before_any_channel_runs() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let client =
            FakeRetrievalClient::answering(EmbeddingRetrievalOutcome::Completed(Vec::new()));
        let service = governed(RetrievalService::with_channels(
            Arc::new(SuccessfulTextRetriever),
            Some(Arc::new(SuccessfulVectorRetriever)),
            None,
            None,
            Arc::new(StubJournal),
        ))
        .with_corpus_generation_resolver(
            Arc::new(StubCorpusGenerationResolver),
            "test-space",
            "test-model",
        )
        .with_embedding_retrieval_client(client.clone(), Duration::from_secs(5))
        .expect("installing the client is what the refusal must catch later");

        let error = service
            .search(&context, RetrievalRequest::new(workspace, "fact"))
            .await
            .expect_err("two authorities for one channel must be refused");

        assert!(matches!(error, ApplicationError::Policy(_)), "{error:?}");
        assert_eq!(client.calls(), 0, "the refusal must precede the query");
    }

    /// An unbounded or absent wait would pin a generation for as long as the
    /// caller cared to hold it.
    #[test]
    fn a_wait_budget_outside_its_bounds_is_refused() {
        let client =
            FakeRetrievalClient::answering(EmbeddingRetrievalOutcome::Completed(Vec::new()));
        let build = |budget| {
            RetrievalService::new(Arc::new(SuccessfulTextRetriever), Arc::new(StubJournal))
                .with_embedding_retrieval_client(client.clone(), budget)
                .map(|_| ())
        };
        assert!(build(Duration::ZERO).is_err());
        assert!(build(MAXIMUM_RETRIEVAL_WAIT + Duration::from_secs(1)).is_err());
        assert!(build(MAXIMUM_RETRIEVAL_WAIT).is_ok());
    }
}
