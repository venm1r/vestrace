use crate::{
    id::{ConnectionId, ConnectorId, PrincipalId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Connector {
    pub id: ConnectorId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub provider_type: String,
    pub created_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    Revoked,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Connection {
    pub id: ConnectionId,
    pub connector_id: ConnectorId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub name: String,
    pub status: ConnectionStatus,
    pub created_at: Timestamp,
}
