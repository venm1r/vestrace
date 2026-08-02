use crate::{
    id::{WebhookSubscriptionId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookSubscription {
    pub id: WebhookSubscriptionId,
    pub workspace_id: WorkspaceId,
    pub target_url: String,
    pub secret_token: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: Timestamp,
}
