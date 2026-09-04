//! AG-UI reads endpoints and streams safe run-event metadata.
//!
//! Its `POST /ag-ui/run` surface is deliberately closed: confidential agent
//! input must first be accepted by the governed Run authority, which is not
//! composed here. The route therefore returns `governed_run_input_required`
//! and never invokes the run orchestrator.

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
    /// Accepted and ignored: the console sends them, and there is no thread or
    /// run continuation in this build. Silently accepting them is better than
    /// a 400 on a field the client has always sent, and they are echoed nowhere
    /// so no caller can mistake them for having taken effect.
    #[serde(default)]
    pub thread_id: Option<String>,
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
    State(_state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<RunAgentRequest>,
) -> Result<(), ApiError> {
    let _context = request_context(&headers)?;
    Err(ApiError::refused(
        "governed_run_input_required",
        "AG-UI run execution requires governed confidential input acceptance",
    ))
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

    #[test]
    fn thread_and_run_identifiers_are_accepted_without_being_echoed() {
        // The console sends them; this build has no thread continuation. They
        // must not appear in the response, where a caller could read them as
        // having taken effect.
        let request: RunAgentRequest =
            serde_json::from_str(r#"{"message":"do the thing","thread_id":"t-1","run_id":"r-1"}"#)
                .unwrap();
        assert_eq!(request.message, "do the thing");
        assert_eq!(request.thread_id.as_deref(), Some("t-1"));

        let response = RunAgentResponse {
            run_id: uuid::Uuid::nil(),
            status: "created".into(),
            message: "created run".into(),
        };
        let rendered = serde_json::to_string(&response).unwrap();
        assert!(!rendered.contains("t-1"));
        assert!(!rendered.contains("thread"));
    }

    #[test]
    fn a_message_is_required() {
        let request: RunAgentRequest = serde_json::from_str(r#"{"message":"   "}"#).unwrap();
        assert!(request.message.trim().is_empty());
    }

    #[test]
    fn the_stream_query_defaults_to_the_whole_workspace() {
        let query: StreamQuery = serde_urlencoded::from_str("").unwrap();
        assert!(query.run_id.is_none());
    }
}
