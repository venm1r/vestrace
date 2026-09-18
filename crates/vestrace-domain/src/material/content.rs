//! Structural content-material records. Ciphertext, persistence, and vault
//! behavior intentionally remain outside the domain contract.

use super::{
    ContentMaterialId, MaterialKeyCreationIntentId, MaterialKeyId, PreparedMaterialAttachmentId,
    SizeClass,
};

/// Lifecycle visible to content consumers. Prepared forms are deliberately
/// distinct from the sole consumable `Live` state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentMaterialState {
    Prepared,
    AbandonPrepared,
    Live,
    ErasurePrepared,
    Tombstoned,
    Abandoned,
}

/// The material identity and padded disclosure associated with one content
/// record. It never carries plaintext or an exact plaintext length.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentMaterial {
    id: ContentMaterialId,
    material_key_id: MaterialKeyId,
    size_class: SizeClass,
    state: ContentMaterialState,
}

impl ContentMaterial {
    pub const fn new(
        id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        size_class: SizeClass,
        state: ContentMaterialState,
    ) -> Self {
        Self {
            id,
            material_key_id,
            size_class,
            state,
        }
    }

    pub const fn id(&self) -> ContentMaterialId {
        self.id
    }

    pub const fn material_key_id(&self) -> MaterialKeyId {
        self.material_key_id
    }

    pub const fn size_class(&self) -> SizeClass {
        self.size_class
    }

    pub const fn state(&self) -> ContentMaterialState {
        self.state
    }
}

/// The only two internal markers that may carry non-live ciphertext.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparedMaterialMarker {
    ContentPrepared,
    ResultPrepared,
}

/// An internal attachment that is intentionally not an ordinary material
/// reference. It has no conversion into a Live reference; the guarded Bound
/// finalizer is the sole promotion authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedMaterialAttachment {
    id: PreparedMaterialAttachmentId,
    intent_id: MaterialKeyCreationIntentId,
    material_id: ContentMaterialId,
    marker: PreparedMaterialMarker,
}

impl PreparedMaterialAttachment {
    pub const fn new(
        id: PreparedMaterialAttachmentId,
        intent_id: MaterialKeyCreationIntentId,
        material_id: ContentMaterialId,
        marker: PreparedMaterialMarker,
    ) -> Self {
        Self {
            id,
            intent_id,
            material_id,
            marker,
        }
    }

    pub const fn id(&self) -> PreparedMaterialAttachmentId {
        self.id
    }

    pub const fn intent_id(&self) -> MaterialKeyCreationIntentId {
        self.intent_id
    }

    pub const fn material_id(&self) -> ContentMaterialId {
        self.material_id
    }

    pub const fn marker(&self) -> PreparedMaterialMarker {
        self.marker
    }
}
