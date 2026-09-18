use vestrace_domain::{
    Confidence, Importance, Memory, MemoryKind, MemoryStatus, MemoryWritePolicy,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
    memory::MemoryRevision,
    now,
};

/// A revision that genuinely belongs to the given memory.
fn memory_revision(
    memory_id: MemoryId,
    workspace_id: WorkspaceId,
    revision_number: u32,
) -> MemoryRevision {
    MemoryRevision {
        id: MemoryRevisionId::new(),
        memory_id,
        workspace_id,
        revision_number,
        content: "content".to_string(),
        structured: None,
        confidence: Confidence::new(1.0).unwrap(),
        importance: Importance::new(0.5).unwrap(),
        created_at: now(),
        valid_from: None,
        valid_until: None,
        change_reason: None,
        canonical_hash: None,
        classification: None,
    }
}

#[test]
fn test_memory_lifecycle_state_transitions() {
    let at = now();
    let mem_id = MemoryId::new();
    let ws_id = WorkspaceId::new();

    let memory = Memory::new(mem_id, ws_id, MemoryKind::Fact, at);
    assert_eq!(memory.status, MemoryStatus::Candidate);

    // `activate` takes the revision rather than its id so it can verify that
    // the revision belongs to this memory and this workspace.
    let revision = memory_revision(mem_id, ws_id, 1);
    let rev_id = revision.id;
    let memory = memory.activate(&revision, at).unwrap();
    assert_eq!(memory.status, MemoryStatus::Active);
    assert_eq!(memory.active_revision_id, Some(rev_id));
    assert_eq!(memory.state_revision, 1);

    let memory = memory.supersede(at).unwrap();
    assert_eq!(memory.status, MemoryStatus::Superseded);

    // Superseded memory cannot be activated again
    assert!(memory.activate(&revision, at).is_err());
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
