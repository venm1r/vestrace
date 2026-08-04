use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    id::AgentRunId,
    run::{AgentRun, RunCommandEnvelope, RunEventEnvelope, RunVersion},
};

use crate::{ApplicationError, RequestContext};

use super::{CreateRunCommand, RunCommandResult};

#[async_trait]
pub trait RunRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        run: &AgentRun,
    ) -> Result<(), ApplicationError>;

    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError>;

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError>;
}

#[async_trait]
pub trait RunEventStore: Send + Sync {
    async fn load_stream(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;

    async fn append(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError>;
}

#[async_trait]
pub trait RunCommandCommitter: Send + Sync {
    async fn commit(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[RunEventEnvelope],
        projection: &AgentRun,
    ) -> Result<RunVersion, ApplicationError>;
}

#[async_trait]
pub trait RunCommandExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        command: RunCommandEnvelope,
    ) -> Result<RunCommandResult, ApplicationError>;
}

#[async_trait]
pub trait RunUseCases: Send + Sync {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError>;

    async fn list_runs(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError>;

    async fn get_run(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError>;
}

pub type SharedRunRepository = Arc<dyn RunRepository>;
pub type SharedRunEventStore = Arc<dyn RunEventStore>;
pub type SharedRunCommandCommitter = Arc<dyn RunCommandCommitter>;
pub type SharedRunCommandExecutor = Arc<dyn RunCommandExecutor>;
pub type SharedRunUseCases = Arc<dyn RunUseCases>;
