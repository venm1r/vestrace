use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::{
    Timestamp, now,
    id::{AgentRunId, CorrelationId, OperationId},
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
        ApplicationError, CreateRunCommand, HealthRepository, RequestContext, RunCommandExecutor,
        RunCommandResult, RunUseCases,
    };
    use vestrace_domain::{
        id::AgentRunId,
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

    #[tokio::test]
    async fn create_run_uses_the_canonical_command_executor() {
        let workspace_id = Uuid::new_v4();
        let principal_id = Uuid::new_v4();
        let request_id = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();
        let commands = RecordingCommands::default();
        let state = AppState::new(
            Arc::new(Healthy),
            Arc::new(ReadOnlyRuns),
            Arc::new(commands.clone()),
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
        assert_eq!(command.actor, RunActor::Principal(principal_id.into()));
        assert!(matches!(
            command.command,
            RunCommand::Create { title, .. } if title == "Canonical create"
        ));
    }
}
