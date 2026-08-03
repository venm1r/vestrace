use vestrace_application::{DeterministicExtractor, MemoryExtractor};
use vestrace_domain::{
    ActorRef, Event,
    id::{EventId, WorkspaceId},
    now,
};

#[tokio::test]
async fn test_deterministic_extractor_returns_candidate() {
    let extractor = DeterministicExtractor;
    let event = Event::new(
        EventId::new(),
        WorkspaceId::new(),
        None,
        "fact.user_preference",
        ActorRef::User("user_1".to_string()),
        serde_json::json!({}),
        now(),
    )
    .unwrap();

    let candidates = extractor.extract(&event).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 0.9);
}
