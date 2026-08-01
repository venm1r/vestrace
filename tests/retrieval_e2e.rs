use vestrace_domain::{
    id::{ContextPackId, MemoryId, RetrievalRunId, WorkspaceId},
    now, ContextPack, RetrievalCandidate, RetrievalIntent,
};

#[test]
fn test_context_pack_token_budget_boundary() {
    let at = now();
    let pack_id = ContextPackId::new();
    let run_id = RetrievalRunId::new();
    let ws_id = WorkspaceId::new();

    let valid_pack = ContextPack::new(pack_id, run_id, ws_id, 1000, 800, vec![], at);
    assert!(valid_pack.is_ok());

    let invalid_pack = ContextPack::new(pack_id, run_id, ws_id, 1000, 1050, vec![], at);
    assert!(invalid_pack.is_err());
}
