use vestrace_application::{
    commands::*, ports::*, services::*, DeterministicExtractor, MemoryExtractor, RequestContext,
};
use vestrace_domain::{
    id::*, now, ActorRef, Confidence, EvidenceRole, Importance, MemoryKind, MemoryWritePolicy,
};

#[test]
fn test_end_to_end_memory_core_flow() {
    let ws_id = WorkspaceId::new();
    let ctx = RequestContext::new(ws_id, PrincipalId::new());
    let at = now();

    // 1. Record event
    let event_id = EventId::new();
    let event = vestrace_domain::Event::new(
        event_id,
        ws_id,
        None,
        "fact.user_language",
        ActorRef::User("user_123".into()),
        serde_json::json!({"language": "Rust"}),
        at,
    )
    .unwrap();

    assert_eq!(event.workspace_id, ws_id);

    // 2. Memory Extraction
    let candidate = ExtractedCandidate {
        kind: MemoryKind::Fact,
        content: "User prefers Rust language".into(),
        confidence: 0.95,
        importance: 0.9,
    };

    // 3. Activate under Policy
    let policy = MemoryWritePolicy::Automatic;
    let conf = Confidence::new(candidate.confidence).unwrap();
    let decision = policy.evaluate(candidate.kind, conf);

    assert_eq!(decision, vestrace_domain::ActivationDecision::AutoActivate);
}
