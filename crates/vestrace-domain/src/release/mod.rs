use crate::{id::ReleaseManifestId, time::Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseManifest {
    pub id: ReleaseManifestId,
    pub release_version: String,
    pub components: Vec<String>,
    pub checksum: String,
    pub created_at: Timestamp,
}
