use vestrace_domain::{id::*, now, package::*};

#[test]
fn test_agent_package_creation() {
    let pkg_id = AgentPackageId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let pkg = AgentPackage {
        id: pkg_id,
        workspace_id: ws_id,
        name: "vestrace-code-assistant".into(),
        version: "1.0.0".into(),
        publisher: "vestrace".into(),
        manifest: serde_json::json!({"skills": ["code-review"]}),
        created_at: at,
    };

    assert_eq!(pkg.version, "1.0.0");
    assert_eq!(pkg.publisher, "vestrace");
}
