use vestrace_domain::{id::*, now, tool::*};

#[test]
fn test_tool_definition_and_invocation() {
    let tool_id = ToolDefinitionId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let def = ToolDefinition {
        id: tool_id,
        workspace_id: ws_id,
        name: "test_tool".into(),
        description: "A test tool".into(),
        binding_kind: ToolBindingKind::Native,
        parameters_schema: serde_json::json!({}),
        created_at: at,
    };

    let invocation = ToolInvocation {
        id: ToolInvocationId::new(),
        tool_id: def.id,
        workspace_id: ws_id,
        arguments: serde_json::json!({}),
        status: ToolInvocationStatus::Prepared,
        output: None,
        created_at: at,
    };

    assert_eq!(invocation.status, ToolInvocationStatus::Prepared);
    assert_eq!(def.binding_kind, ToolBindingKind::Native);
}
