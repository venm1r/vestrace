use vestrace_domain::{
    id::{EventId, MemoryId, MemoryRevisionId, MemorySourceId, RelationId, SessionId, WorkspaceId},
    ActorRef, Confidence, EvidenceRole, Importance, MemoryKind, MemoryStatus, MemoryWritePolicy,
    RelationType, StructuredMemory, SubjectRef,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RecordEventCommand {
    pub id: EventId,
    pub session_id: Option<SessionId>,
    pub event_type: String,
    pub actor: ActorRef,
    pub subject: Option<SubjectRef>,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RememberMemoryCommand {
    pub memory_id: MemoryId,
    pub kind: MemoryKind,
    pub content: String,
    pub structured: Option<StructuredMemory>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub source_event_id: EventId,
    pub evidence_role: EvidenceRole,
    pub policy: MemoryWritePolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReviseMemoryCommand {
    pub memory_id: MemoryId,
    pub expected_revision: u32,
    pub content: String,
    pub structured: Option<StructuredMemory>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub source_event_id: EventId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinkKnowledgeCommand {
    pub relation_id: RelationId,
    pub source_memory_id: MemoryId,
    pub target_memory_id: MemoryId,
    pub relation_type: RelationType,
    pub confidence: Confidence,
}
