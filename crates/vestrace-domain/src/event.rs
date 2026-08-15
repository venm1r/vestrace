use crate::{
    DomainError,
    id::{EventId, SessionId, WorkspaceId},
    time::Timestamp,
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
    /// Compatibility timestamp for the original event API.
    pub created_at: Timestamp,
    /// Optional source-world occurrence time. `None` means the source did not provide it.
    pub occurred_at: Option<Timestamp>,
    /// Time when Vestrace authoritatively recorded this event.
    pub recorded_at: Timestamp,
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
            occurred_at: None,
            recorded_at: at,
        })
    }

    pub fn with_occurred_at(mut self, occurred_at: Timestamp) -> Self {
        self.occurred_at = Some(occurred_at);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ActorRef, Event};
    use crate::{EventId, WorkspaceId, now};

    #[test]
    fn event_keeps_unknown_occurrence_time_and_records_registration_time() {
        let recorded_at = now();
        let event = Event::new(
            EventId::new(),
            WorkspaceId::new(),
            None,
            "observation",
            ActorRef::System("test".into()),
            serde_json::json!({}),
            recorded_at,
        )
        .unwrap();

        assert_eq!(event.occurred_at, None);
        assert_eq!(event.recorded_at, recorded_at);
    }
}
