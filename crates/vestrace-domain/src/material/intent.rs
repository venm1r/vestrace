//! Fixed-identity lifecycle records for provisional material keys.

use crate::{PrincipalId, WorkspaceId};

use super::{
    ContentMaterialId, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyCreationIntentState,
    MaterialKeyId,
};

/// A creation reservation fixes the identity, owner, ordinal, nonce, and key
/// before the host vault creates its provisional key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialKeyCreationIntent {
    id: MaterialKeyCreationIntentId,
    workspace_id: WorkspaceId,
    material_id: ContentMaterialId,
    material_key_id: MaterialKeyId,
    nonce: IntentNonce,
    owner_kind: String,
    owner_id: PrincipalId,
    output_ordinal: u64,
    state: MaterialKeyCreationIntentState,
}

impl MaterialKeyCreationIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        id: MaterialKeyCreationIntentId,
        workspace_id: WorkspaceId,
        material_id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        nonce: IntentNonce,
        owner_kind: impl Into<String>,
        owner_id: PrincipalId,
        output_ordinal: u64,
    ) -> Self {
        Self {
            id,
            workspace_id,
            material_id,
            material_key_id,
            nonce,
            owner_kind: owner_kind.into(),
            owner_id,
            output_ordinal,
            state: MaterialKeyCreationIntentState::Reserved,
        }
    }

    pub const fn id(&self) -> MaterialKeyCreationIntentId {
        self.id
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn material_id(&self) -> ContentMaterialId {
        self.material_id
    }

    pub const fn material_key_id(&self) -> MaterialKeyId {
        self.material_key_id
    }

    pub const fn nonce(&self) -> IntentNonce {
        self.nonce
    }

    pub fn owner_kind(&self) -> &str {
        &self.owner_kind
    }

    pub const fn owner_id(&self) -> PrincipalId {
        self.owner_id
    }

    pub const fn output_ordinal(&self) -> u64 {
        self.output_ordinal
    }

    pub const fn state(&self) -> MaterialKeyCreationIntentState {
        self.state
    }
}
