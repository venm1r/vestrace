//! Reads for the AG-UI gateway.
//!
//! Two reads over data the system already keeps: the endpoint registry from
//! migration 0110, and the run event log. Neither introduces a second event
//! stream or a parallel history.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::id::AgentRunId;
use vestrace_domain::time::Timestamp;

use crate::{ApplicationError, RequestContext};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgUiEndpoint {
    pub id: uuid::Uuid,
    pub name: String,
    pub endpoint_url: String,
    /// Stored state only: nothing dispatches to these endpoints.
    pub enabled: bool,
    pub created_at: Timestamp,
}

/// A run event as the gateway forwards it.
///
/// Carries the event's identity and type, never its payload. Payloads hold run
/// content, and a gateway stream is the wrong place to widen who can read it —
/// a client that needs the payload reads the run's events through `/v1`, where
/// the usual authorization applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgUiRunEvent {
    pub run_id: uuid::Uuid,
    pub event_type: String,
    pub sequence: i64,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait AgUiRepository: Send + Sync {
    async fn list_endpoints(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<AgUiEndpoint>, ApplicationError>;

    /// Run events recorded strictly after `since`, oldest first.
    ///
    /// Strictly after, so a cursor advanced to the last delivered event cannot
    /// redeliver it. `run_id` narrows to a single run; absent, the workspace's
    /// events are returned.
    async fn events_since(
        &self,
        context: &RequestContext,
        run_id: Option<AgentRunId>,
        since: Timestamp,
        limit: u32,
    ) -> Result<Vec<AgUiRunEvent>, ApplicationError>;
}

pub type SharedAgUiRepository = Arc<dyn AgUiRepository>;
