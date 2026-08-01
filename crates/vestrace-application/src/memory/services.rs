use crate::{
    context::RequestContext,
    error::ApplicationError,
    memory::{
        commands::{LinkKnowledgeCommand, RecordEventCommand, RememberMemoryCommand, ReviseMemoryCommand},
        ports::{EventRepository, MemoryRepository, ProvenanceRepository, RelationRepository},
    },
    outbox::OutboxMessage,
};
use vestrace_domain::{
    id::OutboxId, now, Derivation, KnowledgeRelation, Memory, MemoryRevision, MemorySource, Event,
};

pub struct MemoryService<E, M, P, R> {
    event_repo: E,
    memory_repo: M,
    provenance_repo: P,
    relation_repo: R,
}

impl<E, M, P, R> MemoryService<E, M, P, R>
where
    E: EventRepository,
    M: MemoryRepository,
    P: ProvenanceRepository,
    R: RelationRepository,
{
    pub fn new(event_repo: E, memory_repo: M, provenance_repo: P, relation_repo: R) -> Self {
        Self {
            event_repo,
            memory_repo,
            provenance_repo,
            relation_repo,
        }
    }

    pub async fn record_event(
        &mut self,
        ctx: &RequestContext,
        cmd: RecordEventCommand,
    ) -> Result<Event, ApplicationError> {
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
        Ok(event)
    }

    pub async fn remember_memory(
        &mut self,
        ctx: &RequestContext,
        cmd: RememberMemoryCommand,
    ) -> Result<Memory, ApplicationError> {
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

        Ok(memory)
    }

    pub async fn link_knowledge(
        &mut self,
        ctx: &RequestContext,
        cmd: LinkKnowledgeCommand,
    ) -> Result<KnowledgeRelation, ApplicationError> {
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
        Ok(relation)
    }
}
