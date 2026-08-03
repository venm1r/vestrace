use vestrace_application::RequestContext;
use vestrace_domain::{
    ActorRef, Confidence, Event, Importance, Memory, MemoryKind, MemoryRevision, MemoryStatus,
    MemoryWritePolicy, RelationType, StructuredMemory,
    id::{EventId, MemoryId, WorkspaceId},
    now,
};

#[test]
fn test_memory_lifecycle_state_transitions() {
    let at = now();
    let mem_id = MemoryId::new();
    let ws_id = WorkspaceId::new();

    let memory = Memory::new(mem_id, ws_id, MemoryKind::Fact, at);
    assert_eq!(memory.status, MemoryStatus::Candidate);

    let rev_id = vestrace_domain::id::MemoryRevisionId::new();
    let memory = memory.activate(rev_id, at).unwrap();
    assert_eq!(memory.status, MemoryStatus::Active);
    assert_eq!(memory.active_revision_id, Some(rev_id));

    let memory = memory.supersede(at).unwrap();
    assert_eq!(memory.status, MemoryStatus::Superseded);

    // Superseded memory cannot be activated again
    assert!(memory.activate(rev_id, at).is_err());
}

#[test]
fn test_policy_evaluation() {
    let policy = MemoryWritePolicy::Automatic;
    let conf = Confidence::new(0.8).unwrap();
    let decision = policy.evaluate(MemoryKind::Fact, conf);
    assert_eq!(decision, vestrace_domain::ActivationDecision::AutoActivate);

    let low_conf = Confidence::new(0.3).unwrap();
    let decision_low = policy.evaluate(MemoryKind::Fact, low_conf);
    assert_eq!(
        decision_low,
        vestrace_domain::ActivationDecision::DiscardCandidate
    );
}
