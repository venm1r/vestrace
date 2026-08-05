use async_trait::async_trait;
use vestrace_application::{ApplicationError, RequestContext, RunEventStore};
use vestrace_domain::{
    id::AgentRunId,
    run::{RunEventEnvelope, RunVersion},
};

struct MemoryStore;

#[async_trait]
impl RunEventStore for MemoryStore {
    async fn load_stream(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn append(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _expected_version: RunVersion,
        events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        events
            .last()
            .map(|event| event.sequence)
            .ok_or_else(|| ApplicationError::Conflict("empty event batch".to_owned()))
    }
}

#[test]
fn run_event_store_is_object_safe() {
    fn accept_store(_store: &dyn RunEventStore) {}

    let store = MemoryStore;
    accept_store(&store);
}
