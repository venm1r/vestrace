//! RED contract for R1.4 HTTP command wiring.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, CreateRunCommand, HealthRepository, NullExecutionHistoryRepository,
    RequestContext, RunCommandExecutor, RunCommandResult, RunUseCases,
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

struct StubMemoryUseCases;

#[async_trait]
impl vestrace_application::MemoryUseCases for StubMemoryUseCases {
    async fn record_event(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::RecordEventCommand,
    ) -> Result<vestrace_domain::Event, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn remember_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::RememberMemoryCommand,
    ) -> Result<vestrace_domain::Memory, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn revise_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::ReviseMemoryCommand,
    ) -> Result<vestrace_domain::Memory, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn link_knowledge(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::LinkKnowledgeCommand,
    ) -> Result<vestrace_domain::KnowledgeRelation, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn find_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::MemoryId,
    ) -> Result<Option<vestrace_domain::Memory>, vestrace_application::ApplicationError> {
        Ok(None)
    }
}

struct StubTextRetriever;

#[async_trait]
impl vestrace_application::TextRetriever for StubTextRetriever {
    async fn search(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::NormalizedRetrievalRequest,
    ) -> Result<Vec<vestrace_domain::RetrievalCandidate>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

struct StubRetrievalJournal;

#[async_trait]
impl vestrace_application::RetrievalJournal for StubRetrievalJournal {
    async fn record_run(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::RetrievalRunId,
        _: &str,
        _: &str,
        _: usize,
        _: i32,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn record_context_pack(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ContextPackId,
        _: vestrace_domain::id::RetrievalRunId,
        _: vestrace_domain::WorkspaceId,
        _: u32,
        _: u32,
        _: &serde_json::Value,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
}

struct StubProviderRepository;
struct StubModelRepository;
struct StubAgentRepository;
struct StubSkillRepository;

#[async_trait]
impl vestrace_application::ProviderRepository for StubProviderRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ProviderId,
        _: &str,
        _: vestrace_domain::ProviderLocality,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ProviderRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

#[async_trait]
impl vestrace_application::ModelRepository for StubModelRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::ModelRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ModelRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ModelId,
    ) -> Result<Option<vestrace_application::ModelRecord>, vestrace_application::ApplicationError>
    {
        Ok(None)
    }
}

#[async_trait]
impl vestrace_application::AgentRepository for StubAgentRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::AgentRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::AgentRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::AgentId,
    ) -> Result<Option<vestrace_application::AgentRecord>, vestrace_application::ApplicationError>
    {
        Ok(None)
    }
}

#[async_trait]
impl vestrace_application::SkillRepository for StubSkillRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::SkillRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::SkillRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

struct StubRoutingDecisionRepository;
struct StubModelExecutionRepository;

#[async_trait]
impl vestrace_application::RoutingDecisionRepository for StubRoutingDecisionRepository {
    async fn record(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::RoutingDecisionRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<
        Vec<vestrace_application::RoutingDecisionRecord>,
        vestrace_application::ApplicationError,
    > {
        Ok(Vec::new())
    }
}

#[async_trait]
impl vestrace_application::ModelExecutionRepository for StubModelExecutionRepository {
    async fn record(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::ModelExecutionRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<
        Vec<vestrace_application::ModelExecutionRecord>,
        vestrace_application::ApplicationError,
    > {
        Ok(Vec::new())
    }
}

#[test]
fn app_state_requires_a_canonical_run_command_executor() {
    let state = AppState::new(
        Arc::new(Healthy),
        Arc::new(Reads),
        Arc::new(Commands),
        Arc::new(StubMemoryUseCases),
        std::sync::Arc::new(vestrace_application::RetrievalService::new(
            std::sync::Arc::new(StubTextRetriever),
            std::sync::Arc::new(StubRetrievalJournal),
        )),
        std::sync::Arc::new(StubProviderRepository),
        std::sync::Arc::new(StubModelRepository),
        std::sync::Arc::new(StubAgentRepository),
        std::sync::Arc::new(StubSkillRepository),
        std::sync::Arc::new(StubRoutingDecisionRepository),
        std::sync::Arc::new(StubModelExecutionRepository),
        std::sync::Arc::new(NullExecutionHistoryRepository::new()),
        std::sync::Arc::new(NullExecutionHistoryRepository::new()),
        std::sync::Arc::new(NullExecutionHistoryRepository::new()),
        std::sync::Arc::new(vestrace_http::MetricsRegistry::new()),
    );

    let _: &dyn RunCommandExecutor = state.run_command_executor();
}
