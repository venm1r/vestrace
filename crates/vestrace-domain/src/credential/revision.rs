use std::{fmt, str::FromStr};

use uuid::Uuid;

use crate::{MaterialKeyId, Timestamp, WorkspaceId, credential_slot::CredentialSlotId};

/// Opaque identity for one immutable credential revision.
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
pub struct CredentialRevisionId(Uuid);

impl CredentialRevisionId {
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

impl Default for CredentialRevisionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CredentialRevisionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for CredentialRevisionId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

/// The associated-data schema used by one immutable credential revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialAssociatedDataProfile {
    CredentialV2,
    LegacyV1,
}

/// Immutable metadata for a credential revision; ciphertext lives outside this contract.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CredentialRevision {
    pub id: CredentialRevisionId,
    pub workspace_id: WorkspaceId,
    pub credential_slot_id: CredentialSlotId,
    pub material_key_id: MaterialKeyId,
    pub associated_data_profile: CredentialAssociatedDataProfile,
    pub created_at: Timestamp,
}
