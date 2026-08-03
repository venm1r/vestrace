use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{id::AgentRunId, run::AgentRun};

use crate::{ApplicationError, RequestContext};

use super::CreateRunCommand;

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
pub type SharedRunUseCases = Arc<dyn RunUseCases>;
