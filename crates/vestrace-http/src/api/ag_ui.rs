//! AG-UI reads endpoints and streams safe run-event metadata.
//!
//! Its `POST /ag-ui/run` surface creates or extends a Run through the same
//! `RunOrchestrator` the console's own Run detail page uses. It is
//! single-shot: one message becomes one step on the run, with no thread
//! continuation.

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Method},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use vestrace_domain::id::AgentRunId;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{ApiError, context::request_context};

/// How often the event stream looks for new run events.
///
/// A poll rather than a database notification channel: `LISTEN/NOTIFY` would
/// need a dedicated connection per subscriber and a publisher on the write
/// path, neither of which exists yet. One second is responsive enough for an
/// operator watching a run and cheap enough for the single-administrator
/// deployment this build targets.
const STREAM_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Events returned per poll. Bounds a burst so one busy run cannot starve the
/// connection or blow up a client.
const STREAM_BATCH_LIMIT: u32 = 64;

#[derive(Debug, Serialize)]
pub struct AgUiEndpointResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub endpoint_url: String,
    /// Stored state only. Nothing dispatches to these endpoints, so a disabled
    /// endpoint is not being suppressed and an enabled one is not being called.
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RunAgentRequest {
    pub message: String,
    /// Accepted and ignored: the console sends it, but this build has no
    /// thread continuation. Silently accepting it is better than a 400 on a
    /// field the client has always sent, and it is echoed nowhere in the
    /// response so no caller can mistake it for having taken effect.
    #[serde(default)]
    pub thread_id: Option<String>,
    /// When present, names an existing run to extend rather than create: it
    /// is looked up and the message is added to it as a new step, a real
    /// continuation with an effect. Unlike `thread_id`, this is not inert —
    /// it is simply never echoed back in the response body itself, the same
    /// way `/v1/runs/{id}/steps` never echoes the `{id}` path segment back
    /// into its own response.
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunAgentResponse {
    pub run_id: uuid::Uuid,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    /// Narrow the stream to one run. Absent, the stream carries every run event
    /// in the workspace.
    #[serde(default)]
    pub run_id: Option<uuid::Uuid>,
}

pub fn ag_ui_routes() -> axum::Router<AppState> {
    let router = mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/ag-ui/endpoints"),
        get(list_endpoints),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/ag-ui/run"),
        post(run_agent),
    );
    mount(
        router,
        route_descriptor(&Method::GET, "/ag-ui/events/stream"),
        get(event_stream),
    )
}

async fn list_endpoints(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AgUiEndpointResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.ag_ui_repository()?;
    let endpoints = repository
        .list_endpoints(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(
        endpoints
            .into_iter()
            .map(|endpoint| AgUiEndpointResponse {
                id: endpoint.id,
                name: endpoint.name,
                endpoint_url: endpoint.endpoint_url,
                enabled: endpoint.enabled,
                created_at: endpoint.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

async fn run_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RunAgentRequest>,
) -> Result<Json<RunAgentResponse>, ApiError> {
    let context = request_context(&headers)?;
    let confidential_input =
        vestrace_application::run::ConfidentialRunInput::parse(request.message)
            .map_err(ApiError::from_application)?;

    let (run_id, expected_version) = match request.run_id {
        Some(ref raw) => {
            let run_id = raw
                .parse::<vestrace_domain::id::AgentRunId>()
                .map_err(|_| ApiError::bad_request("run id must be a UUID"))?;
            let run = state
                .run_use_cases()
                .get_run(&context, run_id)
                .await
                .map_err(ApiError::from_application)?
                .ok_or_else(|| ApiError::not_found("run"))?;
            (run_id, run.version)
        }
        None => {
            let created = state
                .run_orchestrator()?
                .create_run(
                    &context,
                    vestrace_application::run::CreateRun {
                        objective: "AG-UI run".to_owned(),
                        coordinator_snapshot_id: vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                        execution_mode: vestrace_domain::run::RunExecutionMode::Supervised,
                        parent: None,
                        correlation_id: None,
                        idempotency_key: uuid::Uuid::now_v7().to_string(),
                    },
                )
                .await
                .map_err(ApiError::from_application)?;
            (created.run.id, created.run.version)
        }
    };

    let result = state
        .run_orchestrator()?
        .add_steps(
            &context,
            vestrace_application::run::AddRunSteps {
                run_id,
                expected_version,
                correlation_id: None,
                steps: vec![vestrace_application::run::NewRunStepDto {
                    id: vestrace_domain::id::RunStepId::new(),
                    plan_step_reference: None,
                    assigned_actor: vestrace_domain::run::RunActorRef::AgentSnapshot(
                        vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                    ),
                    input_references: vec![],
                    input: vestrace_application::run::NewRunStepInput::Confidential(
                        confidential_input,
                    ),
                }],
                actor: vestrace_domain::run::RunActorRef::Principal(context.principal_id),
                idempotency_key: uuid::Uuid::now_v7().to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(RunAgentResponse {
        run_id: result.run.id.as_uuid(),
        status: "accepted".to_owned(),
        message: "run accepted".to_owned(),
    }))
}

async fn event_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let context = request_context(&headers)?;
    // Resolved before the stream starts so an unconfigured reader answers 501
    // here, rather than opening a connection that silently carries nothing.
    let repository = state.ag_ui_repository()?.clone();
    let run_id = query.run_id.map(AgentRunId::from_uuid);

    let stream = async_stream::stream! {
        // Starts from now: a client opening the stream wants to watch what
        // happens next, and replaying the whole history would flood it. A
        // caller wanting history reads the run's events over `/v1`.
        let mut cursor = vestrace_domain::time::now();

        loop {
            tokio::time::sleep(STREAM_POLL_INTERVAL).await;

            match repository
                .events_since(&context, run_id, cursor, STREAM_BATCH_LIMIT)
                .await
            {
                Ok(events) => {
                    for event in events {
                        cursor = event.created_at;
                        let payload = serde_json::json!({
                            "run_id": event.run_id,
                            "event_type": event.event_type,
                            "sequence": event.sequence,
                            "created_at": event.created_at.to_rfc3339(),
                        });
                        // `unwrap_or_else` rather than `?`: one unserialisable
                        // event must not tear down a stream that is otherwise
                        // healthy.
                        yield Ok(Event::default().data(
                            serde_json::to_string(&payload)
                                .unwrap_or_else(|_| "{}".to_string()),
                        ));
                    }
                }
                Err(error) => {
                    // Reported to the client instead of silently stalling: a
                    // stream that has stopped delivering looks identical to a
                    // quiet system.
                    tracing::warn!(error = %error, "ag-ui event stream poll failed");
                    yield Ok(Event::default()
                        .event("error")
                        .data(r#"{"message":"the event stream could not be read"}"#));
                    break;
                }
            }
        }
    };

    // A comment line keeps proxies and browsers from closing an idle stream,
    // which they otherwise do well before anything interesting happens.
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use vestrace_application::run::{AddRunSteps, CreateRun, RunOrchestrator, RunSnapshot};
    use vestrace_application::{ApplicationError, RequestContext};

    #[test]
    // Renamed from `thread_and_run_identifiers_are_accepted_without_being_echoed`:
    // that name read as if neither field has an effect, which is no longer
    // true of `run_id` (it now genuinely extends a run — see `run_agent`).
    // What this test actually covers is narrower and still true of both
    // fields: they parse off the request, and the *response* body never
    // echoes their literal values back, regardless of what effect they had.
    fn thread_and_run_identifiers_are_parsed_and_never_echoed_in_the_response() {
        let request: RunAgentRequest =
            serde_json::from_str(r#"{"message":"do the thing","thread_id":"t-1","run_id":"r-1"}"#)
                .unwrap();
        assert_eq!(request.message, "do the thing");
        assert_eq!(request.thread_id.as_deref(), Some("t-1"));
        assert_eq!(request.run_id.as_deref(), Some("r-1"));

        let response = RunAgentResponse {
            run_id: uuid::Uuid::nil(),
            status: "created".into(),
            message: "created run".into(),
        };
        let rendered = serde_json::to_string(&response).unwrap();
        assert!(!rendered.contains("t-1"));
        assert!(!rendered.contains("r-1"));
        assert!(!rendered.contains("thread"));
    }

    #[test]
    fn the_stream_query_defaults_to_the_whole_workspace() {
        let query: StreamQuery = serde_urlencoded::from_str("").unwrap();
        assert!(query.run_id.is_none());
    }

    /// Records what the transport asked the durable coordinator to do, exactly
    /// like `api::runs::tests::RecordingOrchestrator` — redefined here because
    /// that one is private to `runs.rs`'s own test module.
    #[derive(Default, Clone)]
    struct RecordingOrchestrator {
        created: std::sync::Arc<std::sync::Mutex<Option<CreateRun>>>,
        added: std::sync::Arc<std::sync::Mutex<Option<AddRunSteps>>>,
    }

    fn snapshot(context: &RequestContext, objective: String) -> RunSnapshot {
        let at = vestrace_domain::time::now();
        let run_id = vestrace_domain::id::AgentRunId::new();
        RunSnapshot {
            run: vestrace_domain::run::AgentRun {
                id: run_id,
                workspace_id: context.workspace_id,
                objective,
                coordinator_snapshot_id: vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                active_plan_revision_id: None,
                execution_mode: vestrace_domain::run::RunExecutionMode::Supervised,
                status: vestrace_domain::run::RunStatus::Created,
                current_step_id: None,
                checkpoint_id: None,
                parent: None,
                root_run_id: run_id,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
                version: vestrace_domain::run::RunVersion::INITIAL,
                result: None,
                created_at: at,
                updated_at: at,
                finished_at: None,
            },
            steps: vec![],
            checkpoint: None,
        }
    }

    #[async_trait::async_trait]
    impl RunOrchestrator for RecordingOrchestrator {
        async fn create_run(
            &self,
            context: &RequestContext,
            command: CreateRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            let objective = command.objective.clone();
            *self.created.lock().unwrap() = Some(command);
            Ok(snapshot(context, objective))
        }

        async fn add_steps(
            &self,
            context: &RequestContext,
            command: AddRunSteps,
        ) -> Result<RunSnapshot, ApplicationError> {
            *self.added.lock().unwrap() = Some(command);
            Ok(snapshot(context, "stepped".to_string()))
        }

        async fn pause_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::PauseRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "paused".to_string()))
        }

        async fn resume_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::ResumeRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "resumed".to_string()))
        }

        async fn cancel_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::CancelRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "cancelled".to_string()))
        }

        async fn approve_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::ApproveRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "approved".to_string()))
        }
    }

    fn post_run(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/ag-ui/run")
            .header("content-type", "application/json")
            .header("x-workspace-id", uuid::Uuid::now_v7().to_string())
            .header("x-principal-id", uuid::Uuid::now_v7().to_string())
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn a_message_with_no_run_id_creates_a_run_and_adds_an_agent_step() {
        use std::sync::Arc;
        let orchestrator = RecordingOrchestrator::default();
        let app = crate::build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_run_orchestrator(Arc::new(orchestrator.clone())),
        );

        let response = app
            .oneshot(post_run(r#"{"message":"hello there"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let created = orchestrator.created.lock().unwrap().take();
        assert!(
            created.is_some(),
            "run_agent must create a run when no run_id is given"
        );
        let added = orchestrator.added.lock().unwrap().take();
        let added = added.expect("run_agent must add a step to the run it created");
        assert_eq!(added.steps.len(), 1);
        assert!(matches!(
            added.steps[0].assigned_actor,
            vestrace_domain::run::RunActorRef::AgentSnapshot(_)
        ));

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["run_id"].is_string());
    }

    #[tokio::test]
    async fn a_blank_message_is_refused_before_anything_is_created() {
        use std::sync::Arc;
        let orchestrator = RecordingOrchestrator::default();
        let app = crate::build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_run_orchestrator(Arc::new(orchestrator.clone())),
        );

        let response = app.oneshot(post_run(r#"{"message":"   "}"#)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(orchestrator.created.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn no_orchestrator_configured_answers_not_implemented_rather_than_a_stub_refusal() {
        let response = crate::build_router(crate::api::runs::tests::test_state().with_policy(
            std::sync::Arc::new(crate::api::runs::tests::TestAllowPolicy),
        ))
        .oneshot(post_run(r#"{"message":"hello there"}"#))
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
