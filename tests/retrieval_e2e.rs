use vestrace_domain::{
    ContextPack, EvidenceRef, MemoryStatus, PrincipalId,
    id::{ContextPackId, MemoryId, MemoryRevisionId, RetrievalRunId, WorkspaceId},
    now,
    retrieval::{ContextItem, ContextSection, RepresentationLevel},
};

/// One section whose single item accounts for exactly `tokens`.
///
/// This test used to pass `vec![]` for the sections while claiming 800 used
/// tokens. `ContextPack::new` now requires the per-item counts to add up to the
/// declared total, because a pack that honours its budget in the summary while
/// its payload disagrees is the failure the budget exists to prevent — so the
/// fixture has to carry the tokens it claims.
fn sections_accounting_for(tokens: u32) -> Vec<ContextSection> {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    vec![ContextSection {
        label: "facts".to_string(),
        items: vec![ContextItem {
            memory_id,
            revision_id,
            memory_status: MemoryStatus::Active,
            revision_number: 1,
            valid_from: None,
            valid_until: None,
            revision_created_at: now(),
            source_generation: 1,
            representation: RepresentationLevel::Full,
            rendered_text: "rendered".to_string(),
            accounted_tokens: tokens,
            provenance_refs: vec![EvidenceRef::MemoryRevisionRef {
                memory_id,
                revision_id,
            }],
            inclusion_explanation: "budget boundary fixture".to_string(),
            source_classification: None,
        }],
    }]
}

#[test]
fn test_context_pack_token_budget_boundary() {
    let at = now();
    let ws_id = WorkspaceId::new();
    let actor = PrincipalId::new();

    let valid_pack = ContextPack::new(
        ContextPackId::new(),
        RetrievalRunId::new(),
        ws_id,
        actor,
        vestrace_domain::TimePerspective::Current,
        1000,
        800,
        vec![],
        sections_accounting_for(800),
        false,
        vec![],
        vec![],
        vec![],
        "v1".to_string(),
        true,
        at,
    );
    assert!(valid_pack.is_ok());

    let invalid_pack = ContextPack::new(
        ContextPackId::new(),
        RetrievalRunId::new(),
        ws_id,
        actor,
        vestrace_domain::TimePerspective::Current,
        1000,
        1050,
        vec![],
        sections_accounting_for(1050),
        false,
        vec![],
        vec![],
        vec![],
        "v1".to_string(),
        true,
        at,
    );
    assert!(invalid_pack.is_err());
}

#[test]
fn a_pack_cannot_claim_tokens_its_items_do_not_account_for() {
    // The complement of the boundary test: staying under the ceiling is not
    // enough if the declared usage and the payload disagree.
    let result = ContextPack::new(
        ContextPackId::new(),
        RetrievalRunId::new(),
        WorkspaceId::new(),
        PrincipalId::new(),
        vestrace_domain::TimePerspective::Current,
        1000,
        800,
        vec![],
        // Half of what the pack says it used.
        sections_accounting_for(400),
        false,
        vec![],
        vec![],
        vec![],
        "v1".to_string(),
        true,
        now(),
    );
    assert!(result.is_err());
}
