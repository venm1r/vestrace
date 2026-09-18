use serde::Serialize;
use vestrace_domain::{
    ActorRef, Confidence, EvidenceRole, Importance, MemoryKind, MemoryWritePolicy, RelationType,
    StructuredMemory, SubjectRef,
    id::{EventId, MemoryId, RelationId, SessionId},
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecordEventCommand {
    pub id: EventId,
    pub session_id: Option<SessionId>,
    pub event_type: String,
    pub actor: ActorRef,
    pub subject: Option<SubjectRef>,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
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
    pub classification: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "intent", content = "label")]
pub enum MemoryClassificationUpdate {
    /// The caller did not state a classification, so the active one carries.
    Inherit,
    /// The caller explicitly stated JSON null and therefore asked to clear it.
    Clear,
    /// The caller stated a label. Only the active revision's identical label
    /// can be accepted until a label-native transition mechanism exists.
    Set(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReviseMemoryCommand {
    pub memory_id: MemoryId,
    pub expected_revision: u32,
    pub content: String,
    pub structured: Option<StructuredMemory>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub source_event_id: EventId,
    pub change_reason: Option<String>,
    pub classification: MemoryClassificationUpdate,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LinkKnowledgeCommand {
    pub relation_id: RelationId,
    pub source_memory_id: MemoryId,
    pub target_memory_id: MemoryId,
    pub relation_type: RelationType,
    pub confidence: Confidence,
    pub idempotency_key: String,
}

/// What a request means, for the purpose of deciding whether it has already
/// been made.
///
/// # Why this is not "hash the command"
///
/// It was. `compute_hash(&cmd)` serialised the whole command, **including the
/// identifier the HTTP layer generates per request** — `EventId::new()`,
/// `MemoryId::new()`, `RelationId::new()`. Two byte-identical requests with the
/// same idempotency key therefore produced two different hashes, and the second
/// was rejected with `idempotency key reused with different request`. A replay
/// could not succeed: the check could only ever pass through to a new write or
/// fail as a conflict.
///
/// The fingerprint covers what the caller actually asked for. A server-minted
/// identifier is excluded because it is not part of the request, and the
/// idempotency key is excluded because it is the thing being looked up rather
/// than part of what was asked.
///
/// Adding a field to a command now forces a decision about whether it belongs
/// here, which is the point: a silently unhashed field would make two different
/// requests look like a replay of one another, and that is the failure mode
/// worth being noisy about.
pub trait IdempotentRequest {
    fn fingerprint(&self) -> serde_json::Value;
}

impl IdempotentRequest for RecordEventCommand {
    fn fingerprint(&self) -> serde_json::Value {
        serde_json::json!({
            "session_id": self.session_id,
            "event_type": self.event_type,
            "actor": self.actor,
            "subject": self.subject,
            "payload": self.payload,
        })
    }
}

impl IdempotentRequest for RememberMemoryCommand {
    fn fingerprint(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "content": self.content,
            "structured": self.structured,
            "confidence": self.confidence,
            "importance": self.importance,
            "source_event_id": self.source_event_id,
            "evidence_role": self.evidence_role,
            "policy": self.policy,
            "classification": self.classification,
        })
    }
}

impl IdempotentRequest for ReviseMemoryCommand {
    fn fingerprint(&self) -> serde_json::Value {
        serde_json::json!({
            // The memory and the revision it expects are part of the request:
            // the same content against a different expected revision is a
            // different ask, not a replay.
            "memory_id": self.memory_id,
            "expected_revision": self.expected_revision,
            "content": self.content,
            "structured": self.structured,
            "confidence": self.confidence,
            "importance": self.importance,
            "source_event_id": self.source_event_id,
            "change_reason": self.change_reason,
            "classification": self.classification,
        })
    }
}

impl IdempotentRequest for LinkKnowledgeCommand {
    fn fingerprint(&self) -> serde_json::Value {
        serde_json::json!({
            "source_memory_id": self.source_memory_id,
            "target_memory_id": self.target_memory_id,
            "relation_type": self.relation_type,
            "confidence": self.confidence,
        })
    }
}
