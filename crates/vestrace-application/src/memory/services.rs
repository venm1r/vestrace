use crate::{
    RequestContext,
    error::ApplicationError,
    idempotency::{IdempotencyRecord, IdempotencyRepository},
    memory::{
        commands::{
            LinkKnowledgeCommand, RecordEventCommand, RememberMemoryCommand, ReviseMemoryCommand,
        },
        ports::{EventRepository, MemoryRepository, ProvenanceRepository, RelationRepository},
    },
    outbox::{OutboxMessage, OutboxRepository},
};
use chrono::Duration;
use sha2::{Digest, Sha256};
use vestrace_domain::{
    DomainError, Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource, now,
};

pub struct MemoryService<E, M, P, R, O, I> {
    event_repo: E,
    memory_repo: M,
    provenance_repo: P,
    relation_repo: R,
    outbox_repo: O,
    idempotency_repo: I,
}

impl<E, M, P, R, O, I> MemoryService<E, M, P, R, O, I>
where
    E: EventRepository,
    M: MemoryRepository,
    P: ProvenanceRepository,
    R: RelationRepository,
    O: OutboxRepository,
    I: IdempotencyRepository,
{
    pub fn new(
        event_repo: E,
        memory_repo: M,
        provenance_repo: P,
        relation_repo: R,
        outbox_repo: O,
        idempotency_repo: I,
    ) -> Self {
        Self {
            event_repo,
            memory_repo,
            provenance_repo,
            relation_repo,
            outbox_repo,
            idempotency_repo,
        }
    }

    pub async fn find_memory(
        &self,
        ctx: &RequestContext,
        id: vestrace_domain::id::MemoryId,
    ) -> Result<Option<Memory>, ApplicationError> {
        let memory = self.memory_repo.find_memory_by_id(ctx, id).await?;
        if let Some(ref mem) = memory {
            if mem.workspace_id != ctx.workspace_id {
                return Ok(None);
            }
        }
        Ok(memory)
    }

    pub async fn record_event(
        &self,
        ctx: &RequestContext,
        cmd: RecordEventCommand,
    ) -> Result<Event, ApplicationError> {
        let request_hash = compute_hash(&cmd);
        if let Some(cached) = check_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
        )
        .await?
        {
            return serde_json::from_value(cached).map_err(internal_error);
        }

        let at = now();
        let event = Event::new(
            cmd.id,
            ctx.workspace_id,
            cmd.session_id,
            cmd.event_type,
            cmd.actor,
            cmd.payload,
            at,
        )?;

        self.event_repo.save(ctx, &event).await?;

        // No `event.recorded` message is written.
        //
        // The outbox is a delivery mechanism: a message exists because
        // something is waiting to act on it. This topic had no handler, so
        // every event recorded since the system began left a row that would
        // never be delivered — visible as a backlog the doctor reported
        // forever, and as a promise the system never intended to keep.
        //
        // It also had nothing to say that `events` does not. The event table is
        // the record; the outbox is not a second event log.
        //
        // The only candidate consumer was `DeterministicExtractor`, whose
        // output is the literal string "Extracted fact from event {id}".
        // Wiring that would have manufactured placeholder memories, which is
        // worse than the unconsumed topic.

        save_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
            &event,
        )
        .await?;

        Ok(event)
    }

    pub async fn remember_memory(
        &self,
        ctx: &RequestContext,
        cmd: RememberMemoryCommand,
    ) -> Result<Memory, ApplicationError> {
        let request_hash = compute_hash(&cmd);
        if let Some(cached) = check_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
        )
        .await?
        {
            return serde_json::from_value(cached).map_err(internal_error);
        }

        let at = now();
        let memory = Memory::new(cmd.memory_id, ctx.workspace_id, cmd.kind, at);

        let rev_id = vestrace_domain::id::MemoryRevisionId::new();
        let revision = MemoryRevision {
            id: rev_id,
            memory_id: cmd.memory_id,
            workspace_id: ctx.workspace_id,
            revision_number: 1,
            content: cmd.content,
            structured: cmd.structured,
            confidence: cmd.confidence,
            importance: cmd.importance,
            created_at: at,
            valid_from: None,
            valid_until: None,
            change_reason: None,
            canonical_hash: None,
            classification: None,
        };

        let memory = memory.activate(&revision, at)?;

        let source_id = vestrace_domain::id::MemorySourceId::new();
        let source = MemorySource::new_direct(
            source_id,
            cmd.memory_id,
            ctx.workspace_id,
            cmd.source_event_id,
            at,
        );

        // One call, one transaction. These were three separate writes, and the
        // first of them committed an active memory before its source existed —
        // which `tr_active_memory_has_source` refuses at commit, so creating a
        // memory through this path had never once succeeded.
        self.memory_repo
            .save_memory_with_revision(ctx, &memory, &revision, &source)
            .await?;

        let outbox = OutboxMessage::new(
            ctx.workspace_id,
            "memory.created",
            serde_json::json!({
                "memory_id": memory.id.as_uuid(),
                "revision_id": rev_id.as_uuid(),
                "source_event_id": cmd.source_event_id.as_uuid(),
            }),
            at,
        );
        self.outbox_repo.save(ctx, &outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
            &memory,
        )
        .await?;

        Ok(memory)
    }

    pub async fn link_knowledge(
        &self,
        ctx: &RequestContext,
        cmd: LinkKnowledgeCommand,
    ) -> Result<KnowledgeRelation, ApplicationError> {
        let request_hash = compute_hash(&cmd);
        if let Some(cached) = check_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
        )
        .await?
        {
            return serde_json::from_value(cached).map_err(internal_error);
        }

        let at = now();
        let relation = KnowledgeRelation::new(
            cmd.relation_id,
            ctx.workspace_id,
            cmd.source_memory_id,
            cmd.target_memory_id,
            cmd.relation_type,
            cmd.confidence,
            at,
        )?;

        self.relation_repo.save_relation(ctx, &relation).await?;

        // No `memory.relation.linked` message is written, for the same reason
        // as `event.recorded` above: nothing consumes it, and
        // `knowledge_relations` already holds the link.

        save_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
            &relation,
        )
        .await?;

        Ok(relation)
    }

    pub async fn revise_memory(
        &self,
        ctx: &RequestContext,
        cmd: ReviseMemoryCommand,
    ) -> Result<Memory, ApplicationError> {
        let request_hash = compute_hash(&cmd);
        if let Some(cached) = check_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
        )
        .await?
        {
            return serde_json::from_value(cached).map_err(internal_error);
        }

        let memory = self
            .memory_repo
            .find_memory_by_id(ctx, cmd.memory_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::from(DomainError::NotFound("memory does not exist".to_owned()))
            })?;

        if memory.workspace_id != ctx.workspace_id {
            return Err(ApplicationError::from(DomainError::NotFound(
                "memory does not exist".to_owned(),
            )));
        }

        let current_revision = self
            .memory_repo
            .find_revision_by_id(
                ctx,
                memory.active_revision_id.ok_or_else(|| {
                    ApplicationError::from(DomainError::PolicyViolation(
                        "memory has no active revision".to_owned(),
                    ))
                })?,
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::from(DomainError::NotFound(
                    "active revision does not exist".to_owned(),
                ))
            })?;

        if current_revision.revision_number != cmd.expected_revision {
            return Err(ApplicationError::from(DomainError::RevisionConflict {
                expected: cmd.expected_revision as u64,
                current: current_revision.revision_number as u64,
            }));
        }

        let at = now();
        let new_rev_id = vestrace_domain::id::MemoryRevisionId::new();
        let new_revision = MemoryRevision {
            id: new_rev_id,
            memory_id: cmd.memory_id,
            workspace_id: ctx.workspace_id,
            revision_number: current_revision.revision_number + 1,
            content: cmd.content,
            structured: cmd.structured,
            confidence: cmd.confidence,
            importance: cmd.importance,
            created_at: at,
            valid_from: None,
            valid_until: None,
            change_reason: Some(
                cmd.change_reason
                    .unwrap_or_else(|| "content update".to_owned()),
            ),
            canonical_hash: None,
            classification: None,
        };

        let memory = memory.activate(&new_revision, at)?;

        let source_id = vestrace_domain::id::MemorySourceId::new();
        let source = MemorySource::new_direct(
            source_id,
            cmd.memory_id,
            ctx.workspace_id,
            cmd.source_event_id,
            at,
        );

        // Same atomic write as creation. Revising had the identical defect:
        // `save_memory` committed alone with an `active_revision_id` naming a
        // revision that did not exist yet.
        self.memory_repo
            .save_memory_with_revision(ctx, &memory, &new_revision, &source)
            .await?;

        let outbox = OutboxMessage::new(
            ctx.workspace_id,
            "memory.revised",
            serde_json::json!({
                "memory_id": memory.id.as_uuid(),
                "revision_id": new_rev_id.as_uuid(),
                "revision_number": new_revision.revision_number,
                "source_event_id": cmd.source_event_id.as_uuid(),
            }),
            at,
        );
        self.outbox_repo.save(ctx, &outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx,
            &cmd.idempotency_key,
            &request_hash,
            &memory,
        )
        .await?;

        Ok(memory)
    }
}

/// The hash an idempotency record is matched on.
///
/// Takes a fingerprint rather than a command, because hashing the command
/// included the per-request identifier the caller never sent and made every
/// replay look like a different request. See [`IdempotentRequest`].
fn compute_hash<T: crate::memory::IdempotentRequest>(value: &T) -> String {
    let json = serde_json::to_vec(&value.fingerprint()).unwrap_or_default();
    let digest = Sha256::digest(&json);
    let mut hash = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hash, "{byte:02x}").ok();
    }
    hash
}

async fn check_idempotency<I: IdempotencyRepository>(
    repo: &I,
    ctx: &RequestContext,
    key: &str,
    request_hash: &str,
) -> Result<Option<serde_json::Value>, ApplicationError> {
    match repo.find_by_key(ctx, key).await? {
        Some(record) if record.request_hash == request_hash => Ok(record.response_payload),
        Some(_) => Err(ApplicationError::Conflict(
            "idempotency key reused with different request".to_owned(),
        )),
        None => Ok(None),
    }
}

async fn save_idempotency<I: IdempotencyRepository, T: serde::Serialize>(
    repo: &I,
    ctx: &RequestContext,
    key: &str,
    request_hash: &str,
    response: &T,
) -> Result<(), ApplicationError> {
    let at = now();
    let response_payload = serde_json::to_value(response).map_err(internal_error)?;
    let record = IdempotencyRecord {
        idempotency_key: key.to_owned(),
        workspace_id: ctx.workspace_id,
        request_hash: request_hash.to_owned(),
        response_payload: Some(response_payload),
        status: "completed".to_owned(),
        created_at: at,
        expires_at: at + Duration::hours(24),
    };
    repo.save(ctx, &record).await
}

fn internal_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Internal(error.to_string())
}

#[async_trait::async_trait]
impl<E, M, P, R, O, I> crate::memory::MemoryUseCases for MemoryService<E, M, P, R, O, I>
where
    E: EventRepository,
    M: MemoryRepository,
    P: ProvenanceRepository,
    R: RelationRepository,
    O: OutboxRepository,
    I: IdempotencyRepository,
{
    async fn record_event(
        &self,
        ctx: &RequestContext,
        cmd: RecordEventCommand,
    ) -> Result<Event, ApplicationError> {
        self.record_event(ctx, cmd).await
    }

    async fn remember_memory(
        &self,
        ctx: &RequestContext,
        cmd: RememberMemoryCommand,
    ) -> Result<Memory, ApplicationError> {
        self.remember_memory(ctx, cmd).await
    }

    async fn revise_memory(
        &self,
        ctx: &RequestContext,
        cmd: ReviseMemoryCommand,
    ) -> Result<Memory, ApplicationError> {
        self.revise_memory(ctx, cmd).await
    }

    async fn link_knowledge(
        &self,
        ctx: &RequestContext,
        cmd: LinkKnowledgeCommand,
    ) -> Result<KnowledgeRelation, ApplicationError> {
        self.link_knowledge(ctx, cmd).await
    }

    async fn find_memory(
        &self,
        ctx: &RequestContext,
        id: vestrace_domain::id::MemoryId,
    ) -> Result<Option<Memory>, ApplicationError> {
        self.find_memory(ctx, id).await
    }
}

#[cfg(test)]
mod idempotency_hash_tests {
    use super::compute_hash;
    use crate::memory::{IdempotentRequest, RecordEventCommand, RememberMemoryCommand};
    use vestrace_domain::{
        ActorRef, Confidence, EvidenceRole, Importance, MemoryKind, MemoryWritePolicy,
        id::{EventId, MemoryId},
    };

    fn event(payload: serde_json::Value, key: &str) -> RecordEventCommand {
        RecordEventCommand {
            // Minted per request by the HTTP layer, which is the whole problem:
            // two calls to this helper produce different ids for what is, from
            // the caller's side, the same request.
            id: EventId::new(),
            session_id: None,
            event_type: "observation".to_owned(),
            actor: ActorRef::User("someone".to_owned()),
            subject: None,
            payload,
            idempotency_key: key.to_owned(),
        }
    }

    /// The defect: `compute_hash` serialised the whole command, id included, so
    /// two identical requests never matched and every replay was rejected with
    /// `idempotency key reused with different request`. Idempotency could not
    /// succeed for any request, ever.
    #[test]
    fn two_identical_requests_hash_the_same() {
        let first = event(serde_json::json!({"note": "the same thing"}), "key-1");
        let second = event(serde_json::json!({"note": "the same thing"}), "key-1");

        assert_ne!(first.id, second.id, "the ids differ, as they do in life");
        assert_eq!(compute_hash(&first), compute_hash(&second));
    }

    /// The other half: a different request under the same key must not be
    /// mistaken for a replay, or the caller silently receives an answer to a
    /// question it did not ask.
    #[test]
    fn a_changed_request_hashes_differently() {
        let first = event(serde_json::json!({"note": "the same thing"}), "key-1");
        let changed = event(serde_json::json!({"note": "something else"}), "key-1");

        assert_ne!(compute_hash(&first), compute_hash(&changed));
    }

    /// The key identifies the record; it is not part of what was asked. Two
    /// replays under different keys are still two identical requests.
    #[test]
    fn the_key_itself_is_not_part_of_the_request() {
        let first = event(serde_json::json!({"note": "the same thing"}), "key-1");
        let other_key = event(serde_json::json!({"note": "the same thing"}), "key-2");

        assert_eq!(compute_hash(&first), compute_hash(&other_key));
    }

    #[test]
    fn a_memory_write_hashes_on_what_was_written() {
        let base = |content: &str| RememberMemoryCommand {
            memory_id: MemoryId::new(),
            kind: MemoryKind::Fact,
            content: content.to_owned(),
            structured: None,
            confidence: Confidence::new(0.9).unwrap(),
            importance: Importance::new(0.7).unwrap(),
            source_event_id: EventId::new(),
            evidence_role: EvidenceRole::DirectSource,
            policy: MemoryWritePolicy::Manual,
            idempotency_key: "key-1".to_owned(),
        };

        let first = base("a fact");
        let same_content = RememberMemoryCommand {
            memory_id: MemoryId::new(),
            source_event_id: first.source_event_id,
            ..first.clone()
        };
        let different_content = RememberMemoryCommand {
            content: "another fact".to_owned(),
            ..first.clone()
        };

        assert_eq!(compute_hash(&first), compute_hash(&same_content));
        assert_ne!(compute_hash(&first), compute_hash(&different_content));
        // And the fingerprint says so in the open, rather than the hash being
        // the only place the decision lives.
        assert!(first.fingerprint().get("memory_id").is_none());
        assert!(first.fingerprint().get("content").is_some());
    }
}
