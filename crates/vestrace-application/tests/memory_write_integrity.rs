use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, IdempotencyRecord, MemoryUnitOfWork, MemoryUnitOfWorkManager,
    OutboxMessage, RequestContext,
};
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
};

struct FakeUnitOfWork;

#[async_trait]
impl MemoryUnitOfWork for FakeUnitOfWork {
    async fn find_idempotency(
        &mut self,
        _workspace_id: WorkspaceId,
        _key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError> {
        Ok(None)
    }

    async fn insert_event(&mut self, _event: &Event) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_memory(&mut self, _memory: &Memory) -> Result<(), ApplicationError> { Ok(()) }
    async fn update_memory(&mut self, _memory: &Memory) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_revision(&mut self, _revision: &MemoryRevision) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_source(&mut self, _source: &MemorySource) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_relation(&mut self, _relation: &KnowledgeRelation) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_outbox(&mut self, _message: &OutboxMessage) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_idempotency(&mut self, _record: &IdempotencyRecord) -> Result<(), ApplicationError> { Ok(()) }

    async fn load_memory_for_update(
        &mut self,
        _id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError> {
        Ok(None)
    }

    async fn find_revision(
        &mut self,
        _id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError> {
        Ok(None)
    }

    async fn commit(self: Box<Self>) -> Result<(), ApplicationError> { Ok(()) }
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> { Ok(()) }
}

struct FakeManager;

#[async_trait]
impl MemoryUnitOfWorkManager for FakeManager {
    async fn begin(
        &self,
        _context: &RequestContext,
    ) -> Result<Box<dyn MemoryUnitOfWork>, ApplicationError> {
        Ok(Box::new(FakeUnitOfWork))
    }
}

#[test]
fn memory_unit_of_work_contract_is_object_safe() {
    fn assert_manager<T: MemoryUnitOfWorkManager>() {}
    assert_manager::<FakeManager>();
}
