use crate::{
    id::{EventId, SessionId, WorkspaceId},
    time::Timestamp,
    DomainError,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActorRef {
    User(String),
    Agent(String),
    System(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubjectRef {
    Session(SessionId),
    Channel(String),
    Resource(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Event {
    pub id: EventId,
    pub workspace_id: WorkspaceId,
    pub session_id: Option<SessionId>,
    pub event_type: String,
    pub actor: ActorRef,
    pub subject: Option<SubjectRef>,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}

impl Event {
    pub fn new(
        id: EventId,
        workspace_id: WorkspaceId,
        session_id: Option<SessionId>,
        event_type: impl Into<String>,
        actor: ActorRef,
        payload: serde_json::Value,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        let event_type = event_type.into();
        if event_type.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "event_type must not be empty".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            session_id,
            event_type,
            actor,
            subject: None,
            payload,
            created_at: at,
        })
    }
}
