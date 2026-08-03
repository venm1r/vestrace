use vestrace_domain::{connection::*, id::*, now};

#[test]
fn test_connector_and_connection_creation() {
    let connector_id = ConnectorId::new();
    let connection_id = ConnectionId::new();
    let ws_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let at = now();

    let connector = Connector {
        id: connector_id,
        workspace_id: ws_id,
        name: "GitHub OAuth Connector".into(),
        provider_type: "oauth2".into(),
        created_at: at,
    };

    let connection = Connection {
        id: connection_id,
        connector_id: connector.id,
        workspace_id: ws_id,
        principal_id,
        name: "User GitHub Connection".into(),
        status: ConnectionStatus::Active,
        created_at: at,
    };

    assert_eq!(connection.status, ConnectionStatus::Active);
    assert_eq!(connector.provider_type, "oauth2");
}
