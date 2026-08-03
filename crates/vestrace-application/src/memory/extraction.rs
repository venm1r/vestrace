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

pub struct DeterministicExtractor;

#[async_trait]
impl MemoryExtractor for DeterministicExtractor {
    async fn extract(&self, event: &Event) -> Result<Vec<ExtractedCandidate>, ApplicationError> {
        // Deterministic extraction logic for test suite
        if event.event_type.starts_with("fact.") {
            Ok(vec![ExtractedCandidate {
                kind: MemoryKind::Fact,
                content: format!("Extracted fact from event {}", event.id),
                confidence: 0.9,
                importance: 0.8,
            }])
        } else {
            Ok(vec![])
        }
    }
}
