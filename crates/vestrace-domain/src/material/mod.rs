//! Shared material-domain vocabulary.
//!
//! This module deliberately declares only values and lifecycle names. Vault
//! and persistence behavior belong to their later package boundaries.

mod content;
mod identity;
mod intent;
mod size_class;

pub use content::{
    ContentMaterial, ContentMaterialState, PreparedMaterialAttachment, PreparedMaterialMarker,
};
pub use identity::{
    AssociatedData, ContentMaterialId, CredentialKeyCreationIntentState, ErasureReceipt,
    IntentNonce, MaterialKeyBindingReceipt, MaterialKeyCreationIntentId,
    MaterialKeyCreationIntentState, MaterialKeyId, PreparedMaterialAttachmentId, VaultReceipt,
    ZeroizingDek,
};
pub use intent::MaterialKeyCreationIntent;
pub use size_class::{SizeClass, size_class_for};
