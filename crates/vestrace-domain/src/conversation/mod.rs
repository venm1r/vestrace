use crate::{
    id::{ConversationThreadId, TriggerId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Web,
    Cli,
    Webhook,
    ScheduledTrigger,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConversationThread {
    pub id: ConversationThreadId,
    pub workspace_id: WorkspaceId,
    pub title: String,
    pub channel_type: ChannelType,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExternalTrigger {
    pub id: TriggerId,
    pub workspace_id: WorkspaceId,
    pub trigger_type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: Timestamp,
}
