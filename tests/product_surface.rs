use vestrace_domain::{
    id::*, product::*, now,
};

#[test]
fn test_product_release_and_session() {
    let release_id = ProductReleaseId::new();
    let session_id = InteractionSessionId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let release = ProductRelease {
        id: release_id,
        version: "v0.2.0".into(),
        manifest: serde_json::json!({"components": ["web-console", "ag-ui-gateway"]}),
        created_at: at,
    };

    let session = InteractionSession {
        id: session_id,
        workspace_id: ws_id,
        run_id: None,
        client_type: "web-console".into(),
        status: SessionStatus::Active,
        created_at: at,
    };

    assert_eq!(release.version, "v0.2.0");
    assert_eq!(session.status, SessionStatus::Active);
}
