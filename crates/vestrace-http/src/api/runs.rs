use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_application::run::{
    AddRunSteps, ApproveRun, CancelRun, CreateRun, NewRunStepDto, PauseRun, ResumeRun,
};
use vestrace_domain::{
    Timestamp,
    id::{AgentRunId, AgentRuntimeSnapshotId},
    now,
    run::{AgentRun, RunActorRef, RunExecutionMode, RunStatus, RunStep, RunStepStatus, RunVersion},
};

use crate::AppState;

use super::{ApiError, context::request_context};

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";
const IF_MATCH_HEADER: &str = "if-match";

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct CancelRunRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddStepsRequest {
    pub steps: Vec<NewStepRequest>,
}

#[derive(Debug, Deserialize)]
pub struct NewStepRequest {
    /// `agent`, `principal` or `system`. Only `agent` invokes a model.
    pub assigned_actor: String,
    #[serde(default)]
    pub plan_step_reference: Option<String>,
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
            title: run.objective,
            status: run.status,
            version: run.version.value(),
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RunStepResponse {
    pub id: uuid::Uuid,
    pub status: RunStepStatus,
    pub attempt: u32,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

impl From<RunStep> for RunStepResponse {
    fn from(step: RunStep) -> Self {
        Self {
            id: step.id.as_uuid(),
            status: step.status,
            attempt: step.attempt,
            started_at: step.started_at,
            finished_at: step.finished_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RunDetailResponse {
    #[serde(flatten)]
    pub run: RunResponse,
    pub steps: Vec<RunStepResponse>,
}

pub async fn create_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let context = request_context(&headers)?;
    // The request id is the idempotency key: two deliveries of the same request
    // must not create two runs. It is required rather than generated, because a
    // key the server invents deduplicates nothing.
    let idempotency_key = required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string();
    // Threaded through so a run event can be traced back to the request that
    // caused it. The coordinator mints one only when a caller supplies none.
    let correlation_id = vestrace_domain::id::CorrelationId::from_uuid(required_uuid_header(
        &headers,
        CORRELATION_ID_HEADER,
    )?);

    let result = state
        .run_orchestrator()?
        .create_run(
            &context,
            CreateRun {
                objective: request.title,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                // Supervised rather than autopilot: this build enforces no
                // budget on the execution path, and defaulting an API-created
                // run to unattended execution would claim more than it can back.
                execution_mode: RunExecutionMode::Supervised,
                parent: None,
                correlation_id: Some(correlation_id),
                idempotency_key,
            },
        )
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

pub async fn pause_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let run_id = parse_run_id(&id)?;
    let expected_version = if_match_version(&headers)?;

    let result = state
        .run_orchestrator()?
        .pause_run(
            &context,
            PauseRun {
                run_id,
                expected_version,
                correlation_id: Some(correlation_id(&headers)?),
                actor: RunActorRef::Principal(context.principal_id),
                idempotency_key: required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(result.run.into()))
}

pub async fn resume_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let run_id = parse_run_id(&id)?;
    let expected_version = if_match_version(&headers)?;

    let result = state
        .run_orchestrator()?
        .resume_run(
            &context,
            ResumeRun {
                run_id,
                expected_version,
                correlation_id: Some(correlation_id(&headers)?),
                actor: RunActorRef::Principal(context.principal_id),
                idempotency_key: required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(result.run.into()))
}

/// Grant a pending approval for a run.
///
/// This is not an un-pause: it records who approved through a distinct
/// canonical event, and the domain accepts it only while an approval is
/// actually pending.
pub async fn approve_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let run_id = parse_run_id(&id)?;
    let expected_version = if_match_version(&headers)?;

    let result = state
        .run_orchestrator()?
        .approve_run(
            &context,
            ApproveRun {
                run_id,
                expected_version,
                correlation_id: Some(correlation_id(&headers)?),
                approver_id: context.principal_id,
                actor: RunActorRef::Principal(context.principal_id),
                idempotency_key: required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    // An approval is a governed act, so it is recorded in the audit trail as
    // well as in the run's own history.
    let audit = state.audit_repository()?;
    let event = vestrace_domain::AuditEvent::new(
        vestrace_domain::id::AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        "run.approval_granted",
        "agent_run",
        run_id.as_uuid(),
        serde_json::json!({ "run_version": result.run.version.value() }),
        now(),
    )
    .map_err(|error| ApiError::from_application(error.into()))?;
    audit
        .record(&context, &event)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(result.run.into()))
}

pub async fn cancel_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CancelRunRequest>,
) -> Result<Json<RunResponse>, ApiError> {
    let context = request_context(&headers)?;
    let run_id = parse_run_id(&id)?;
    let expected_version = if_match_version(&headers)?;

    let result = state
        .run_orchestrator()?
        .cancel_run(
            &context,
            CancelRun {
                run_id,
                expected_version,
                correlation_id: Some(correlation_id(&headers)?),
                reason: request.reason,
                actor: RunActorRef::Principal(context.principal_id),
                idempotency_key: required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(result.run.into()))
}

/// Add steps to a run.
///
/// This route is what makes a run executable. Without it nothing called
/// `AddRunSteps`, so a run created through the API never acquired a step, no
/// work item was ever queued, and the worker never saw it.
///
/// A step assigned to an agent is the one that invokes a model; a step with
/// input references is not scheduled until they are satisfied.
pub async fn add_run_steps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<AddStepsRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let context = request_context(&headers)?;
    let run_id = parse_run_id(&id)?;
    let expected_version = if_match_version(&headers)?;

    if request.steps.is_empty() {
        return Err(ApiError::bad_request("steps must not be empty"));
    }

    let steps = request
        .steps
        .into_iter()
        .map(|step| {
            Ok(NewRunStepDto {
                id: vestrace_domain::id::RunStepId::new(),
                plan_step_reference: step.plan_step_reference,
                assigned_actor: parse_actor(&step.assigned_actor, context.principal_id)?,
                // Deliberately empty: input references name artifacts and
                // approvals a caller has no way to create through this route
                // yet, and accepting unresolvable ones would leave a step that
                // is never scheduled with no indication why.
                input_references: vec![],
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    let result = state
        .run_orchestrator()?
        .add_steps(
            &context,
            AddRunSteps {
                run_id,
                expected_version,
                correlation_id: Some(correlation_id(&headers)?),
                steps,
                actor: RunActorRef::Principal(context.principal_id),
                idempotency_key: required_uuid_header(&headers, REQUEST_ID_HEADER)?.to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(result.run.into())))
}

/// Map the request's actor name onto a domain actor.
///
/// `agent` is the only value that causes a model invocation. The others are
/// accepted so a caller can model human or system work in the same run, and an
/// unknown value is refused rather than defaulted — silently turning a typo
/// into a system step would produce a run that completes without doing
/// anything.
fn parse_actor(
    value: &str,
    principal_id: vestrace_domain::id::PrincipalId,
) -> Result<RunActorRef, ApiError> {
    match value.trim() {
        "agent" => Ok(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new())),
        "principal" => Ok(RunActorRef::Principal(principal_id)),
        "system" => Ok(RunActorRef::System),
        other => Err(ApiError::bad_request(format!(
            "assigned_actor must be one of agent, principal, system; got {other:?}"
        ))),
    }
}

/// The caller's correlation id, so events can be traced to their request.
fn correlation_id(headers: &HeaderMap) -> Result<vestrace_domain::id::CorrelationId, ApiError> {
    Ok(vestrace_domain::id::CorrelationId::from_uuid(
        required_uuid_header(headers, CORRELATION_ID_HEADER)?,
    ))
}

fn parse_run_id(id: &str) -> Result<AgentRunId, ApiError> {
    id.parse::<AgentRunId>()
        .map_err(|_| ApiError::bad_request("run id must be a UUID"))
}

fn if_match_version(headers: &HeaderMap) -> Result<RunVersion, ApiError> {
    let value = headers
        .get(IF_MATCH_HEADER)
        .ok_or_else(|| ApiError::bad_request("missing If-Match header"))?;
    let s = value
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match header must be valid UTF-8"))?;
    let version: u64 = s
        .parse()
        .map_err(|_| ApiError::bad_request("If-Match header must be a version number"))?;
    RunVersion::new(version).map_err(|_| ApiError::bad_request("invalid run version"))
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
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use uuid::Uuid;
    use vestrace_application::{
        ApplicationError, CreateRunCommand, HealthRepository, NullExecutionHistoryRepository,
        PolicyDecisionEngine, RequestContext, RunUseCases,
    };
    use vestrace_domain::{
        id::{AgentRunId, AgentRuntimeSnapshotId},
        run::{AgentRun, RunExecutionMode, RunVersion},
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
            _: &vestrace_application::retrieval::RetrievalRunRecord,
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

    struct TestAllowPolicy;

    #[async_trait]
    impl PolicyDecisionEngine for TestAllowPolicy {
        async fn decide(
            &self,
            context: &RequestContext,
            request: vestrace_domain::AuthorizationRequest,
        ) -> Result<vestrace_domain::PolicyDecision, ApplicationError> {
            let at = vestrace_domain::now();
            let grant = vestrace_domain::CapabilityGrant::issue(
                vestrace_domain::CapabilityGrantSpec {
                    id: vestrace_domain::id::CapabilityGrantId::new(),
                    workspace_id: context.workspace_id,
                    subject_id: context.principal_id,
                    issuer_id: context.principal_id,
                    capability: request.capability.clone(),
                    operation: request.operation.clone(),
                    resource_scope: request.resource_scope.clone(),
                    valid_from: at,
                    valid_until: None,
                    budget: None,
                    risk_ceiling: vestrace_domain::RiskCategory::Critical,
                    conditions: request.conditions.clone(),
                },
                at,
            )?;
            Ok(vestrace_domain::evaluate_capability_grants(
                vestrace_domain::id::PolicyDecisionId::new(),
                context.workspace_id,
                context.principal_id,
                "test-allow-v1",
                &request,
                &[grant],
                at,
            )?)
        }
    }

    fn test_state() -> AppState {
        AppState::new(
            Arc::new(Healthy),
            Arc::new(ReadOnlyRuns),
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
        )
    }

    /// Records what the transport asked the durable coordinator to do.
    #[derive(Default, Clone)]
    struct RecordingOrchestrator {
        created: Arc<std::sync::Mutex<Option<vestrace_application::run::CreateRun>>>,
        added: Arc<std::sync::Mutex<Option<vestrace_application::run::AddRunSteps>>>,
    }

    #[async_trait::async_trait]
    impl vestrace_application::run::RunOrchestrator for RecordingOrchestrator {
        async fn create_run(
            &self,
            context: &vestrace_application::RequestContext,
            command: vestrace_application::run::CreateRun,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            let objective = command.objective.clone();
            *self.created.lock().unwrap() = Some(command);
            Ok(snapshot(context, objective))
        }

        async fn add_steps(
            &self,
            context: &vestrace_application::RequestContext,
            command: vestrace_application::run::AddRunSteps,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            *self.added.lock().unwrap() = Some(command);
            Ok(snapshot(context, "stepped".to_string()))
        }

        async fn pause_run(
            &self,
            context: &vestrace_application::RequestContext,
            _command: vestrace_application::run::PauseRun,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            Ok(snapshot(context, "paused".to_string()))
        }

        async fn resume_run(
            &self,
            context: &vestrace_application::RequestContext,
            _command: vestrace_application::run::ResumeRun,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            Ok(snapshot(context, "resumed".to_string()))
        }

        async fn cancel_run(
            &self,
            context: &vestrace_application::RequestContext,
            _command: vestrace_application::run::CancelRun,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            Ok(snapshot(context, "cancelled".to_string()))
        }

        async fn approve_run(
            &self,
            context: &vestrace_application::RequestContext,
            _command: vestrace_application::run::ApproveRun,
        ) -> Result<vestrace_application::run::RunSnapshot, vestrace_application::ApplicationError>
        {
            Ok(snapshot(context, "approved".to_string()))
        }
    }

    fn snapshot(
        context: &vestrace_application::RequestContext,
        objective: String,
    ) -> vestrace_application::run::RunSnapshot {
        let at = vestrace_domain::now();
        let run_id = vestrace_domain::id::AgentRunId::new();
        vestrace_application::run::RunSnapshot {
            run: vestrace_domain::run::AgentRun {
                id: run_id,
                workspace_id: context.workspace_id,
                objective,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                active_plan_revision_id: None,
                execution_mode: RunExecutionMode::Supervised,
                status: super::RunStatus::Created,
                current_step_id: None,
                checkpoint_id: None,
                parent: None,
                root_run_id: run_id,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
                version: RunVersion::INITIAL,
                result: None,
                created_at: at,
                updated_at: at,
                finished_at: None,
            },
            steps: vec![],
            checkpoint: None,
        }
    }

    /// Creation must reach the durable coordinator, which is the path that also
    /// writes `run_steps` and `run_work_items`. Before this, `/v1/runs` drove a
    /// second mechanism the worker could not see.
    #[tokio::test]
    async fn create_run_reaches_the_durable_coordinator() {
        let workspace_id = Uuid::now_v7();
        let principal_id = Uuid::now_v7();
        let request_id = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();
        let orchestrator = RecordingOrchestrator::default();
        let state = test_state()
            .with_policy(Arc::new(TestAllowPolicy))
            .with_run_orchestrator(Arc::new(orchestrator.clone()));
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
        let created = orchestrator.created.lock().unwrap().clone().unwrap();
        assert_eq!(created.objective, "Canonical create");
        // The request id is the idempotency key, so a redelivered request does
        // not create a second run.
        assert_eq!(created.idempotency_key, request_id.to_string());
        // Supervised, not autopilot: nothing enforces a budget on the execution
        // path in this build.
        assert_eq!(created.execution_mode, RunExecutionMode::Supervised);

        // That the legacy committer is not on the write path is now enforced by
        // the compiler rather than asserted here: `AppState` no longer holds a
        // `RunCommandExecutor` at all.
    }

    /// The route that makes a run executable. Without it nothing called
    /// `AddRunSteps`, so no run ever acquired a step.
    #[tokio::test]
    async fn adding_an_agent_step_reaches_the_coordinator_as_an_agent_actor() {
        let workspace_id = Uuid::now_v7();
        let principal_id = Uuid::now_v7();
        let orchestrator = RecordingOrchestrator::default();
        let state = test_state()
            .with_policy(Arc::new(TestAllowPolicy))
            .with_run_orchestrator(Arc::new(orchestrator.clone()));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/runs/{}/steps", Uuid::now_v7()))
                    .header("content-type", "application/json")
                    .header("x-workspace-id", workspace_id.to_string())
                    .header("x-principal-id", principal_id.to_string())
                    .header("x-request-id", Uuid::now_v7().to_string())
                    .header("x-correlation-id", Uuid::now_v7().to_string())
                    .header("if-match", "1")
                    .body(Body::from(r#"{"steps":[{"assigned_actor":"agent"}]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let added = orchestrator.added.lock().unwrap().clone().unwrap();
        assert_eq!(added.steps.len(), 1);
        // Only an agent-assigned step invokes a model, so the mapping from the
        // request's actor name is what decides whether anything executes.
        assert!(matches!(
            added.steps[0].assigned_actor,
            vestrace_domain::run::RunActorRef::AgentSnapshot(_)
        ));
    }

    #[tokio::test]
    async fn an_unknown_step_actor_is_refused_rather_than_defaulted() {
        let orchestrator = RecordingOrchestrator::default();
        let state = test_state()
            .with_policy(Arc::new(TestAllowPolicy))
            .with_run_orchestrator(Arc::new(orchestrator.clone()));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/runs/{}/steps", Uuid::now_v7()))
                    .header("content-type", "application/json")
                    .header("x-workspace-id", Uuid::now_v7().to_string())
                    .header("x-principal-id", Uuid::now_v7().to_string())
                    .header("x-request-id", Uuid::now_v7().to_string())
                    .header("x-correlation-id", Uuid::now_v7().to_string())
                    .header("if-match", "1")
                    .body(Body::from(r#"{"steps":[{"assigned_actor":"agnet"}]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Defaulting a typo to a system step would produce a run that completes
        // having done nothing.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(orchestrator.added.lock().unwrap().is_none());
    }
}
