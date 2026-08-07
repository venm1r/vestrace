use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::{RetrievalCandidate, WorkspaceId, id::RetrievalRunId};

use super::NormalizedRetrievalRequest;
use crate::{ApplicationError, RequestContext};

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

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait RetrievalJournal: Send + Sync {
    async fn record_run(
        &self,
        context: &RequestContext,
        run_id: RetrievalRunId,
        query: &str,
        intent: &str,
        candidate_count: usize,
        execution_time_ms: i32,
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

pub type SharedTextRetriever = Arc<dyn TextRetriever>;
pub type SharedVectorRetriever = Arc<dyn VectorRetriever>;
pub type SharedExactRetriever = Arc<dyn ExactRetriever>;
pub type SharedStructuredRetriever = Arc<dyn StructuredRetriever>;
pub type SharedRetrievalJournal = Arc<dyn RetrievalJournal>;
