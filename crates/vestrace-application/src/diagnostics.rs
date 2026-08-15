use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};

#[async_trait]
pub trait RuntimeEvidenceProvider: Send + Sync {
    async fn runtime_evidence(
        &self,
    ) -> Result<crate::RuntimeQualificationEvidence, ApplicationError>;
}

pub type SharedRuntimeEvidenceProvider = std::sync::Arc<dyn RuntimeEvidenceProvider>;

/// Counts the console can display without inventing anything.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceCounts {
    pub live_runs: i64,
    pub runs_today: i64,
    pub registered_agents: i64,
    pub registered_models: i64,
}

/// Read-only aggregate counts for a single workspace.
#[async_trait]
pub trait WorkspaceCountsProvider: Send + Sync {
    async fn workspace_counts(
        &self,
        context: &RequestContext,
    ) -> Result<WorkspaceCounts, ApplicationError>;
}

pub type SharedWorkspaceCountsProvider = std::sync::Arc<dyn WorkspaceCountsProvider>;
