use std::{fmt, str::FromStr};

use uuid::Uuid;

use crate::{ConnectionId, Timestamp, WorkspaceId, credential_revision::CredentialRevisionId};

/// Stable opaque identity for the workspace/purpose/name credential reference.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    PartialEq,
    schemars::JsonSchema,
    serde::Deserialize,
    serde::Serialize,
)]
#[serde(transparent)]
pub struct CredentialSlotId(Uuid);

impl CredentialSlotId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for CredentialSlotId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CredentialSlotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for CredentialSlotId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

/// The stable, ciphertext-free reference a connection uses for one credential purpose.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CredentialSlot {
    pub id: CredentialSlotId,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub purpose: String,
    pub name: String,
    pub current_revision_id: Option<CredentialRevisionId>,
    pub current_revision_version: u64,
    pub tombstone_version: Option<u64>,
    pub tombstoned_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
