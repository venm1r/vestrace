//! Fixed-identity lifecycle records for provisional credential keys.

use std::fmt;

use uuid::Uuid;

use crate::{
    ConnectionId, CredentialKeyCreationIntentState, CredentialRevisionId, CredentialSlotId,
    IntentNonce, MaterialKeyId, WorkspaceId,
};

macro_rules! opaque_credential_intent_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
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
    };
}

opaque_credential_intent_id!(CredentialKeyCreationIntentId);
opaque_credential_intent_id!(CredentialPreparedAttachmentId);

/// A reservation fixes the credential-revision identity before a caller can
/// create a provisional vault key or encrypt credential material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialKeyCreationIntent {
    id: CredentialKeyCreationIntentId,
    workspace_id: WorkspaceId,
    connection_id: ConnectionId,
    credential_slot_id: CredentialSlotId,
    occupancy_id: Uuid,
    credential_revision_id: CredentialRevisionId,
    material_key_id: MaterialKeyId,
    nonce: IntentNonce,
    state: CredentialKeyCreationIntentState,
}

impl CredentialKeyCreationIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        id: CredentialKeyCreationIntentId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        credential_slot_id: CredentialSlotId,
        occupancy_id: Uuid,
        credential_revision_id: CredentialRevisionId,
        material_key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Self {
        Self {
            id,
            workspace_id,
            connection_id,
            credential_slot_id,
            occupancy_id,
            credential_revision_id,
            material_key_id,
            nonce,
            state: CredentialKeyCreationIntentState::Reserved,
        }
    }

    pub const fn id(&self) -> CredentialKeyCreationIntentId {
        self.id
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub const fn credential_slot_id(&self) -> CredentialSlotId {
        self.credential_slot_id
    }

    pub const fn occupancy_id(&self) -> Uuid {
        self.occupancy_id
    }

    pub const fn credential_revision_id(&self) -> CredentialRevisionId {
        self.credential_revision_id
    }

    pub const fn material_key_id(&self) -> MaterialKeyId {
        self.material_key_id
    }

    pub const fn nonce(&self) -> IntentNonce {
        self.nonce
    }

    pub const fn state(&self) -> CredentialKeyCreationIntentState {
        self.state
    }
}
