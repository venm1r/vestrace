use crate::ApplicationError;
use async_trait::async_trait;
use vestrace_domain::{Event, MemoryKind};

pub struct ExtractedCandidate {
    pub kind: MemoryKind,
    pub content: String,
    pub confidence: f32,
    pub importance: f32,
}

#[async_trait]
pub trait MemoryExtractor: Send + Sync {
    async fn extract(&self, event: &Event) -> Result<Vec<ExtractedCandidate>, ApplicationError>;
}

// `DeterministicExtractor` used to live here. Its whole body produced the
// literal string "Extracted fact from event {id}" for any event whose type
// began with `fact.`, and its own comment said "for test suite" while it sat in
// the library beside the trait it implements.
//
// It was called by nothing, and it was the obvious thing to reach for when
// giving `event.recorded` a consumer — which would have filled a workspace with
// placeholder memories that look exactly like real ones. A stub named as though
// it were an implementation is worse than an empty extension point, so the
// extension point is what remains: [`MemoryExtractor`] and
// [`ExtractedCandidate`], with no implementation until there is a real one.
