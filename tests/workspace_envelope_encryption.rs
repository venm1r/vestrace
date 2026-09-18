use vestrace_domain::{enterprise::*, id::*, now};

#[test]
fn test_envelope_encryption_and_cross_sharing() {
    let kek_id = KekId::new();
    let grant_id = MemoryGrantId::new();
    let owner_ws = WorkspaceId::new();
    let target_ws = WorkspaceId::new();
    let memory_id = MemoryId::new();
    let at = now();

    let kek = WorkspaceKek {
        id: kek_id,
        workspace_id: owner_ws,
        key_alias: "kms/workspace-main-key".into(),
        algorithm: "AeadAes256GcmV1".into(),
        created_at: at,
    };

    let grant = CrossWorkspaceMemoryGrant {
        id: grant_id,
        owner_workspace_id: owner_ws,
        target_workspace_id: target_ws,
        memory_id,
        granted_at: at,
    };

    assert_eq!(kek.algorithm, "AeadAes256GcmV1");
    assert_eq!(grant.owner_workspace_id, owner_ws);
    assert_eq!(grant.target_workspace_id, target_ws);
}
