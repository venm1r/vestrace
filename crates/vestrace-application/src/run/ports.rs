use async_trait::async_trait;
use vestrace_domain::{
    id::{
        AgentRunId, ArtifactId, ArtifactRevisionId, ModelExecutionId, RunStepId, WorkItemId,
        WorkerId,
    },
    run::{AgentRun, RunCheckpoint, RunEvent, RunFailure, RunStep, RunVersion},
    time::Timestamp,
};

use crate::{ApplicationError, RequestContext, UnitOfWork};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunSnapshot {
    pub run: AgentRun,
    pub steps: Vec<RunStep>,
    pub checkpoint: Option<RunCheckpoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRun {
    pub run: AgentRun,
    pub event: RunEvent,
    pub new_steps: Vec<RunStep>,
    pub checkpoint: Option<RunCheckpoint>,
    pub work_items: Vec<WorkItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitProviderResultRun {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub expected_run_version: u64,
    pub artifact_id: ArtifactId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub model_execution_id: ModelExecutionId,
    pub work_item_id: WorkItemId,
    pub occurred_at: Timestamp,
}

#[async_trait]
pub trait RunStorePort: Send + Sync {
    async fn load(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError>;

    async fn create(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn commit(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    /// Commit a Run transition inside the caller's transaction.
    ///
    /// The default fails closed so non-transactional test adapters remain
    /// source compatible without ever opening a hidden second transaction.
    async fn commit_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        Err(ApplicationError::Internal(
            "transaction-bound run persistence is unsupported".into(),
        ))
    }

    async fn commit_provider_result_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _commit: CommitProviderResultRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        Err(ApplicationError::Internal(
            "transaction-bound provider-result Run persistence is unsupported".into(),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunLease {
    pub run_id: AgentRunId,
    pub worker_id: WorkerId,
    pub generation: u64,
    pub acquired_at: Timestamp,
    pub heartbeat_at: Timestamp,
    pub lease_until: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcquireRunLease {
    pub run_id: AgentRunId,
    pub worker_id: WorkerId,
    pub now: Timestamp,
    pub lease_until: Timestamp,
}

#[async_trait]
pub trait RunLeasePort: Send + Sync {
    async fn acquire(
        &self,
        context: &RequestContext,
        request: AcquireRunLease,
    ) -> Result<RunLease, ApplicationError>;

    async fn heartbeat(
        &self,
        context: &RequestContext,
        lease: &RunLease,
        extend_until: Timestamp,
    ) -> Result<RunLease, ApplicationError>;

    async fn release(
        &self,
        context: &RequestContext,
        lease: &RunLease,
    ) -> Result<(), ApplicationError>;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum WorkItemKind {
    AdvanceRun,
    ResumeRun,
    ExecuteStep {
        step_id: vestrace_domain::id::RunStepId,
    },
}

impl WorkItemKind {
    pub fn discriminant(&self) -> WorkItemKindDiscriminant {
        match self {
            Self::AdvanceRun => WorkItemKindDiscriminant::AdvanceRun,
            Self::ResumeRun => WorkItemKindDiscriminant::ResumeRun,
            Self::ExecuteStep { .. } => WorkItemKindDiscriminant::ExecuteStep,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkItemKindDiscriminant {
    AdvanceRun,
    ResumeRun,
    ExecuteStep,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkItem {
    pub id: WorkItemId,
    pub run_id: AgentRunId,
    pub kind: WorkItemKind,
    pub expected_run_version: RunVersion,
    pub available_at: Timestamp,
    pub idempotency_key: String,
    pub attempt: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseWorkRequest {
    pub worker_id: WorkerId,
    pub now: Timestamp,
    pub lease_until: Timestamp,
}

#[async_trait]
pub trait WorkQueuePort: Send + Sync {
    async fn lease_next(
        &self,
        context: &RequestContext,
        request: LeaseWorkRequest,
    ) -> Result<Option<WorkItem>, ApplicationError>;

    async fn complete(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        at: Timestamp,
    ) -> Result<(), ApplicationError>;

    async fn retry(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        available_at: Timestamp,
        error: RunFailure,
    ) -> Result<(), ApplicationError>;

    async fn cancel_for_run(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        at: Timestamp,
    ) -> Result<u64, ApplicationError>;

    async fn dead_letter(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        error: RunFailure,
        at: Timestamp,
    ) -> Result<(), ApplicationError>;
}

pub trait RunClockPort: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub fn deterministic_idempotency_key(
    run_id: AgentRunId,
    version: RunVersion,
    action: &str,
) -> String {
    format!("run:{run_id}:version:{}:action:{action}", version.value())
}

#[cfg(test)]
mod transaction_bound_api_contract {
    use super::*;

    #[test]
    fn run_store_exposes_caller_owned_commit() {
        async fn type_check(
            store: &dyn RunStorePort,
            context: &RequestContext,
            unit_of_work: &mut dyn UnitOfWork,
            commit: CommitRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            store.commit_in(context, unit_of_work, commit).await
        }

        let _ = type_check;
    }
}
