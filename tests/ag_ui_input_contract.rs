use vestrace_domain::{
    ag_ui::*, id::*, now,
};

#[test]
fn test_ag_ui_endpoint_creation() {
    let endpoint_id = AgUiEndpointId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let endpoint = AgUiEndpoint {
        id: endpoint_id,
        workspace_id: ws_id,
        name: "Web Console AG-UI Gateway".into(),
        endpoint_url: "http://localhost:8080/ag-ui/v1".into(),
        enabled: true,
        created_at: at,
    };

    assert_eq!(endpoint.name, "Web Console AG-UI Gateway");
    assert!(endpoint.enabled);
}
