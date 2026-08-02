use crate::{
    id::{MetricRollupId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricRollup {
    pub id: MetricRollupId,
    pub workspace_id: WorkspaceId,
    pub metric_name: String,
    pub metric_value: f32,
    pub recorded_at: Timestamp,
}
