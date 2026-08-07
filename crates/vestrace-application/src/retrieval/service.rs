use std::time::Instant;
use vestrace_domain::{
    ContextPack, RetrievalCandidate, WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
};

use crate::{
    ApplicationError, RequestContext,
    retrieval::{
        ContextPackBuilder, NormalizedRetrievalRequest, RetrievalRequest, SharedRetrievalJournal,
        SharedTextRetriever, reciprocal_rank_fusion, rerank,
    },
};

pub struct RetrievalService {
    text_retriever: SharedTextRetriever,
    journal: SharedRetrievalJournal,
}

impl RetrievalService {
    pub fn new(text_retriever: SharedTextRetriever, journal: SharedRetrievalJournal) -> Self {
        Self {
            text_retriever,
            journal,
        }
    }

    pub async fn search(
        &self,
        context: &RequestContext,
        request: RetrievalRequest,
    ) -> Result<RetrievalResult, ApplicationError> {
        let workspace_id = request.workspace_id;
        let normalized = NormalizedRetrievalRequest::normalize(request)?;
        let run_id = RetrievalRunId::new();

        let start = Instant::now();
        let mut channels: Vec<Vec<RetrievalCandidate>> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut degraded = false;

        match self.text_retriever.search(context, &normalized).await {
            Ok(results) => channels.push(results),
            Err(e) => {
                warnings.push(format!("text channel failed: {e}"));
                degraded = true;
            }
        }

        let fused = reciprocal_rank_fusion(&channels, 60.0);
        let ranked = rerank(fused);

        let candidates: Vec<RetrievalCandidate> =
            ranked.iter().map(|r| r.candidate.clone()).collect();

        let elapsed_ms = start.elapsed().as_millis() as i32;

        let intent_str = format!("{:?}", normalized.intent).to_lowercase();
        self.journal
            .record_run(
                context,
                run_id,
                &normalized.query,
                &intent_str,
                candidates.len(),
                elapsed_ms,
            )
            .await?;

        Ok(RetrievalResult {
            run_id,
            workspace_id,
            candidates,
            degraded,
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
        let pack_id = ContextPackId::new();
        let builder = ContextPackBuilder::new(token_budget);

        let mut pack = builder.build(result.workspace_id, result.run_id, &result.candidates)?;

        pack.degraded = result.degraded;
        pack.warnings = result.warnings.clone();

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

pub struct RetrievalResult {
    pub run_id: RetrievalRunId,
    pub workspace_id: WorkspaceId,
    pub candidates: Vec<RetrievalCandidate>,
    pub degraded: bool,
    pub warnings: Vec<String>,
    pub normalized: NormalizedRetrievalRequest,
}
