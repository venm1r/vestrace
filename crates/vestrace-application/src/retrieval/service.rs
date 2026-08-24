use std::time::Instant;
use vestrace_domain::{
    ContextPack, RetrievalCandidate, WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
};

use crate::{
    ApplicationError, RequestContext,
    retrieval::{
        ChannelRecord, ContextPackBuilder, NormalizedRetrievalRequest, RetrievalRequest,
        RetrievalRunRecord, SharedExactRetriever, SharedRetrievalJournal,
        SharedStructuredRetriever, SharedTextRetriever, SharedVectorRetriever,
        reciprocal_rank_fusion, rerank,
    },
};

pub struct RetrievalService {
    text_retriever: SharedTextRetriever,
    vector_retriever: Option<SharedVectorRetriever>,
    exact_retriever: Option<SharedExactRetriever>,
    structured_retriever: Option<SharedStructuredRetriever>,
    journal: SharedRetrievalJournal,
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
        }
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

        let workspace_id = request.workspace_id;
        let normalized = NormalizedRetrievalRequest::normalize(request)?;
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

        poll_channel!(self.vector_retriever, "vector");
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

        let fused = reciprocal_rank_fusion(&channels, 60.0);
        let ranked = rerank(fused);

        let candidates: Vec<RetrievalCandidate> =
            ranked.iter().map(|r| r.candidate.clone()).collect();

        let elapsed_ms = start.elapsed().as_millis() as i32;

        let record = RetrievalRunRecord::from_request(
            run_id,
            &normalized,
            channel_records,
            candidates.len(),
            elapsed_ms,
        );
        self.journal.record_run(context, &record).await?;

        Ok(RetrievalResult {
            run_id,
            workspace_id,
            candidates,
            degraded: !degraded_channels.is_empty(),
            degraded_channels,
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

        let pack_id = ContextPackId::new();
        let builder = ContextPackBuilder::new(token_budget);

        let pack = builder.build_with_temporal_perspective(
            result.workspace_id,
            context.principal_id,
            result.run_id,
            result.normalized.time_perspective,
            &result.candidates,
        )?;

        // One call rather than three assignments: `with_degradation` keeps the
        // flag and the channel list from disagreeing, which three separate
        // field writes could not.
        let pack =
            pack.with_degradation(result.degraded_channels.clone(), result.warnings.clone())?;

        let items_json = serde_json::to_value(&pack.sections)
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

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
    pub degraded: bool,
    pub degraded_channels: Vec<String>,
    pub warnings: Vec<String>,
    pub normalized: NormalizedRetrievalRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use vestrace_domain::{MemoryKind, MemoryStatus, PrincipalId, RetrievalCandidate, WorkspaceId};

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
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
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

    #[tokio::test]
    async fn search_rejects_workspace_mismatch() {
        let requested_workspace = WorkspaceId::new();
        let trusted_workspace = WorkspaceId::new();
        let context = RequestContext::new(trusted_workspace, PrincipalId::new());
        let service = RetrievalService::new(Arc::new(StubTextRetriever), Arc::new(StubJournal));

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
    async fn build_context_rejects_result_from_another_workspace() {
        let result_workspace = WorkspaceId::new();
        let context_workspace = WorkspaceId::new();
        let service = RetrievalService::new(Arc::new(StubTextRetriever), Arc::new(StubJournal));
        let normalized = NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
            result_workspace,
            "current fact",
        ))
        .unwrap();
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: result_workspace,
            candidates: Vec::new(),
            degraded: false,
            degraded_channels: Vec::new(),
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
    async fn build_context_preserves_temporal_perspective() {
        let workspace = WorkspaceId::new();
        let at = chrono::Utc::now();
        let service =
            RetrievalService::new(Arc::new(SuccessfulTextRetriever), Arc::new(StubJournal));
        let normalized = NormalizedRetrievalRequest::normalize(
            RetrievalRequest::new(workspace, "historical fact")
                .with_time_perspective(vestrace_domain::TimePerspective::AsOf(at)),
        )
        .unwrap();
        let result = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: workspace,
            candidates: Vec::new(),
            degraded: false,
            degraded_channels: Vec::new(),
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
    async fn optional_channel_failure_marks_result_degraded_but_keeps_safe_results() {
        let workspace = WorkspaceId::new();
        let context = RequestContext::new(workspace, PrincipalId::new());
        let service = RetrievalService::with_channels(
            Arc::new(SuccessfulTextRetriever),
            Some(Arc::new(SuccessfulVectorRetriever)),
            Some(Arc::new(FailingExactRetriever)),
            None,
            Arc::new(StubJournal),
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
        let service = RetrievalService::with_channels(
            Arc::new(FailingTextRetriever),
            Some(Arc::new(FailingVectorRetriever)),
            None,
            None,
            Arc::new(StubJournal),
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
}
