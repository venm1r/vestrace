//! RED contract for R1.4 HTTP command wiring.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, CreateRunCommand, HealthRepository, RequestContext, RunCommandExecutor,
    RunCommandResult, RunUseCases,
};
use vestrace_domain::{
    id::AgentRunId,
    run::{AgentRun, RunCommandEnvelope},
};
use vestrace_http::AppState;

struct Healthy;

#[async_trait]
impl HealthRepository for Healthy {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

struct Reads;

#[async_trait]
impl RunUseCases for Reads {
    async fn create_run(
        &self,
        _context: &RequestContext,
        _command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        panic!("HTTP create must not use the legacy RunUseCases write path")
    }

    async fn list_runs(
        &self,
        _context: &RequestContext,
        _limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn get_run(
        &self,
        _context: &RequestContext,
        _id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        Ok(None)
    }
}

struct Commands;

#[async_trait]
impl RunCommandExecutor for Commands {
    async fn execute(
        &self,
        _context: &RequestContext,
        _command: RunCommandEnvelope,
    ) -> Result<RunCommandResult, ApplicationError> {
        Err(ApplicationError::Internal(
            "not exercised by this constructor contract".to_owned(),
        ))
    }
}

#[test]
fn app_state_requires_a_canonical_run_command_executor() {
    let state = AppState::new(Arc::new(Healthy), Arc::new(Reads), Arc::new(Commands));

    let _: &dyn RunCommandExecutor = state.run_command_executor();
}
