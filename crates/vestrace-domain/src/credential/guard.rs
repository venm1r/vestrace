use std::{fmt, str::FromStr};

use uuid::Uuid;

use crate::{ConnectionId, Timestamp, WorkspaceId, credential_slot::CredentialSlotId};

macro_rules! credential_guard_id {
    ($name:ident) => {
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
        pub struct $name(Uuid);

        impl $name {
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

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

credential_guard_id!(ConnectionExecutionGuardId);
credential_guard_id!(CredentialActivationGuardId);

/// Permanent serialization authority for every execution path of one connection.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ConnectionExecutionGuard {
    pub id: ConnectionExecutionGuardId,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub created_at: Timestamp,
}

/// Permanent serialization authority for one connection/credential-slot lineage.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CredentialActivationGuard {
    pub id: CredentialActivationGuardId,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub execution_guard_id: ConnectionExecutionGuardId,
    pub created_at: Timestamp,
}
