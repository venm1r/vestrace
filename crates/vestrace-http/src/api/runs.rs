use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::{
    Timestamp,
    id::{AgentRunId, CorrelationId, OperationId},
    now,
    run::{AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunStatus, RunVersion},
};

use crate::AppState;

use super::{ApiError, context::request_context};

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: RunStatus,
    pub version: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl From<AgentRun> for RunResponse {
    fn from(run: AgentRun) -> Self {
        Self {
            id: run.id.as_uuid(),
            title: run.title,
            status: run.status,
            version: run.version.value(),
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}

pub async fn create_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let context = request_context(&headers)?;
    let run_id = AgentRunId::new();
    let command = RunCommandEnvelope {
        command_id: OperationId::from_uuid(required_uuid_header(&headers, REQUEST_ID_HEADER)?),
        idempotency_key: None,
        workspace_id: context.workspace_id,
        run_id,
        actor: RunActor::Principal(context.principal_id),
        expected_version: RunVersion::ZERO,
        correlation_id: CorrelationId::from_uuid(required_uuid_header(
            &headers,
            CORRELATION_ID_HEADER,
        )?),
        issued_at: now(),
        command: RunCommand::Create {
            principal_id: context.principal_id,
            title: request.title,
        },
    };
    let result = state
        .run_command_executor()
        .execute(&context, command)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(result.run.into())))
}

pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RunResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let runs = state
        .run_use_cases()
        .list_runs(&context, 50)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(runs.into_iter().map(RunResponse::from).collect()))
}

pub async fn get_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let id = id
        .parse::<AgentRunId>()
        .map_err(|_| ApiError::bad_request("run id must be a UUID"))?;
    let run = state
        .run_use_cases()
        .get_run(&context, id)
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| ApiError::not_found("run"))?;

    Ok(Json(run.into()))
}

fn required_uuid_header(headers: &HeaderMap, name: &'static str) -> Result<Uuid, ApiError> {
    let value = headers
        .get(name)
        .ok_or_else(|| ApiError::bad_request(format!("missing {name} header")))?;
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request(format!("{name} header must be valid UTF-8")))?;
    Uuid::parse_str(value)
        .map_err(|_| ApiError::bad_request(format!("{name} header must be a UUID")))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use uuid::Uuid;
    use vestrace_application::{
        ApplicationError, CreateRunCommand, HealthRepository, NullExecutionHistoryRepository,
        RequestContext, RunCommandExecutor, RunCommandResult, RunUseCases,
    };
    use vestrace_domain::{
        id::{AgentRunId, PrincipalId},
        run::{AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunVersion},
    };

    use crate::{AppState, build_router};

    struct Healthy;

    #[async_trait]
    impl HealthRepository for Healthy {
        async fn check(&self) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    struct ReadOnlyRuns;

    #[async_trait]
    impl RunUseCases for ReadOnlyRuns {
        async fn create_run(
            &self,
            _context: &RequestContext,
            _command: CreateRunCommand,
        ) -> Result<AgentRun, ApplicationError> {
            panic!("POST /v1/runs must not call the legacy write path")
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

    #[derive(Clone, Default)]
    struct RecordingCommands {
        recorded: Arc<Mutex<Option<RunCommandEnvelope>>>,
    }

    #[async_trait]
    impl RunCommandExecutor for RecordingCommands {
        async fn execute(
            &self,
            _context: &RequestContext,
            command: RunCommandEnvelope,
        ) -> Result<RunCommandResult, ApplicationError> {
            let (principal_id, title) = match &command.command {
                RunCommand::Create {
                    principal_id,
                    title,
                } => (*principal_id, title.clone()),
                other => panic!("unexpected command: {other:?}"),
            };
            let run = AgentRun::new(
                command.run_id,
                command.workspace_id,
                principal_id,
                title,
                command.issued_at,
            );
            *self.recorded.lock().unwrap() = Some(command);
            Ok(RunCommandResult {
                run,
                events: Vec::new(),
            })
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
        ) -> Result<vestrace_domain::KnowledgeRelation, vestrace_application::ApplicationError>
        {
            Err(vestrace_application::ApplicationError::Unavailable(
                "stub".to_owned(),
            ))
        }
        async fn find_memory(
            &self,
            _: &vestrace_application::RequestContext,
            _: vestrace_domain::id::MemoryId,
        ) -> Result<Option<vestrace_domain::Memory>, vestrace_application::ApplicationError>
        {
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

    #[tokio::test]
    async fn create_run_uses_the_canonical_command_executor() {
        let workspace_id = Uuid::now_v7();
        let principal_id = Uuid::now_v7();
        let request_id = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();
        let commands = RecordingCommands::default();
        let state = AppState::new(
            Arc::new(Healthy),
            Arc::new(ReadOnlyRuns),
            Arc::new(commands.clone()),
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
            std::sync::Arc::new(crate::MetricsRegistry::new()),
        );
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/runs")
                    .header("content-type", "application/json")
                    .header("x-workspace-id", workspace_id.to_string())
                    .header("x-principal-id", principal_id.to_string())
                    .header("x-request-id", request_id.to_string())
                    .header("x-correlation-id", correlation_id.to_string())
                    .body(Body::from(r#"{"title":"Canonical create"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let command = commands.recorded.lock().unwrap().clone().unwrap();
        assert_eq!(command.command_id.as_uuid(), request_id);
        assert_eq!(command.correlation_id.as_uuid(), correlation_id);
        assert_eq!(command.workspace_id.as_uuid(), workspace_id);
        assert_eq!(command.expected_version, RunVersion::ZERO);
        assert_eq!(command.idempotency_key, None);
        assert_eq!(
            command.actor,
            RunActor::Principal(PrincipalId::from_uuid(principal_id))
        );
        assert!(matches!(
            command.command,
            RunCommand::Create { title, .. } if title == "Canonical create"
        ));
    }
}
