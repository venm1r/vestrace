use vestrace_domain::{a2a::*, id::*, now};

#[test]
fn test_remote_agent_invocation() {
    let inv_id = RemoteAgentInvocationId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let inv = RemoteAgentInvocation {
        id: inv_id,
        workspace_id: ws_id,
        target_agent_url: "https://agent.example.com/a2a".into(),
        protocol_version: "v1".into(),
        status: InvocationStatus::Initiated,
        payload: serde_json::json!({"task": "analyze_code"}),
        created_at: at,
    };

    assert_eq!(inv.status, InvocationStatus::Initiated);
    assert_eq!(inv.protocol_version, "v1");
}
