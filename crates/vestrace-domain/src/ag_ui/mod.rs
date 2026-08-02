use crate::{
    id::{AgUiEndpointId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgUiEndpoint {
    pub id: AgUiEndpointId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub endpoint_url: String,
    pub enabled: bool,
    pub created_at: Timestamp,
}
