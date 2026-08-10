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
    DomainError, Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource, id::OutboxId, now,
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
        let memory = self.memory_repo.find_memory_by_id(id).await?;
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
            ctx.workspace_id,
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

        self.event_repo.save(&event).await?;

        let outbox = OutboxMessage {
            id: OutboxId::new(),
            workspace_id: ctx.workspace_id,
            topic: "event.recorded".to_owned(),
            payload: serde_json::json!({ "event_id": event.id.as_uuid() }),
            created_at: at,
        };
        self.outbox_repo.save(&outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx.workspace_id,
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
            ctx.workspace_id,
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
        };

        let memory = memory.activate(rev_id, at)?;

        self.memory_repo.save_memory(&memory).await?;
        self.memory_repo.save_revision(&revision).await?;

        let source_id = vestrace_domain::id::MemorySourceId::new();
        let source = MemorySource::new_direct(
            source_id,
            cmd.memory_id,
            ctx.workspace_id,
            cmd.source_event_id,
            at,
        );
        self.provenance_repo.save_source(&source).await?;

        let outbox = OutboxMessage {
            id: OutboxId::new(),
            workspace_id: ctx.workspace_id,
            topic: "memory.created".to_owned(),
            payload: serde_json::json!({
                "memory_id": memory.id.as_uuid(),
                "revision_id": rev_id.as_uuid(),
                "source_event_id": cmd.source_event_id.as_uuid(),
            }),
            created_at: at,
        };
        self.outbox_repo.save(&outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx.workspace_id,
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
            ctx.workspace_id,
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

        self.relation_repo.save_relation(&relation).await?;

        let outbox = OutboxMessage {
            id: OutboxId::new(),
            workspace_id: ctx.workspace_id,
            topic: "memory.relation.linked".to_owned(),
            payload: serde_json::json!({
                "relation_id": relation.id.as_uuid(),
                "source_memory_id": relation.source_memory_id.as_uuid(),
                "target_memory_id": relation.target_memory_id.as_uuid(),
            }),
            created_at: at,
        };
        self.outbox_repo.save(&outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx.workspace_id,
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
            ctx.workspace_id,
            &cmd.idempotency_key,
            &request_hash,
        )
        .await?
        {
            return serde_json::from_value(cached).map_err(internal_error);
        }

        let memory = self
            .memory_repo
            .find_memory_by_id(cmd.memory_id)
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
            .find_revision_by_id(memory.active_revision_id.ok_or_else(|| {
                ApplicationError::from(DomainError::PolicyViolation(
                    "memory has no active revision".to_owned(),
                ))
            })?)
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
        };

        let memory = memory.activate(new_rev_id, at)?;

        self.memory_repo.save_memory(&memory).await?;
        self.memory_repo.save_revision(&new_revision).await?;

        let source_id = vestrace_domain::id::MemorySourceId::new();
        let source = MemorySource::new_direct(
            source_id,
            cmd.memory_id,
            ctx.workspace_id,
            cmd.source_event_id,
            at,
        );
        self.provenance_repo.save_source(&source).await?;

        let outbox = OutboxMessage {
            id: OutboxId::new(),
            workspace_id: ctx.workspace_id,
            topic: "memory.revised".to_owned(),
            payload: serde_json::json!({
                "memory_id": memory.id.as_uuid(),
                "revision_id": new_rev_id.as_uuid(),
                "revision_number": new_revision.revision_number,
                "source_event_id": cmd.source_event_id.as_uuid(),
            }),
            created_at: at,
        };
        self.outbox_repo.save(&outbox).await?;

        save_idempotency(
            &self.idempotency_repo,
            ctx.workspace_id,
            &cmd.idempotency_key,
            &request_hash,
            &memory,
        )
        .await?;

        Ok(memory)
    }
}

fn compute_hash<T: serde::Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).unwrap_or_default();
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
    workspace_id: vestrace_domain::WorkspaceId,
    key: &str,
    request_hash: &str,
) -> Result<Option<serde_json::Value>, ApplicationError> {
    match repo.find_by_key(workspace_id, key).await? {
        Some(record) if record.request_hash == request_hash => Ok(record.response_payload),
        Some(_) => Err(ApplicationError::Conflict(
            "idempotency key reused with different request".to_owned(),
        )),
        None => Ok(None),
    }
}

async fn save_idempotency<I: IdempotencyRepository, T: serde::Serialize>(
    repo: &I,
    workspace_id: vestrace_domain::WorkspaceId,
    key: &str,
    request_hash: &str,
    response: &T,
) -> Result<(), ApplicationError> {
    let at = now();
    let response_payload = serde_json::to_value(response).map_err(internal_error)?;
    let record = IdempotencyRecord {
        idempotency_key: key.to_owned(),
        workspace_id,
        request_hash: request_hash.to_owned(),
        response_payload: Some(response_payload),
        status: "completed".to_owned(),
        created_at: at,
        expires_at: at + Duration::hours(24),
    };
    repo.save(&record).await
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
