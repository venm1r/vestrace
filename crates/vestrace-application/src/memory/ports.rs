use crate::ApplicationError;
use async_trait::async_trait;
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource,
    id::{EventId, MemoryId, MemoryRevisionId},
};

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save(&self, event: &Event) -> Result<(), ApplicationError>;
    async fn find_by_id(&self, id: EventId) -> Result<Option<Event>, ApplicationError>;
}

#[async_trait]
pub trait MemoryRepository: Send + Sync {
    async fn save_memory(&self, memory: &Memory) -> Result<(), ApplicationError>;
    async fn save_revision(&self, revision: &MemoryRevision) -> Result<(), ApplicationError>;
    async fn find_memory_by_id(&self, id: MemoryId) -> Result<Option<Memory>, ApplicationError>;
    async fn find_revision_by_id(
        &self,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError>;
}

#[async_trait]
pub trait ProvenanceRepository: Send + Sync {
    async fn save_source(&self, source: &MemorySource) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait RelationRepository: Send + Sync {
    async fn save_relation(&self, relation: &KnowledgeRelation) -> Result<(), ApplicationError>;
}
