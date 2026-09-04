use std::fmt;

use secrecy::{ExposeSecret, SecretBox};
use uuid::Uuid;

macro_rules! opaque_material_id {
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

opaque_material_id!(MaterialKeyId);
opaque_material_id!(IntentNonce);
opaque_material_id!(VaultReceipt);
opaque_material_id!(ErasureReceipt);
opaque_material_id!(MaterialKeyCreationIntentId);
opaque_material_id!(ContentMaterialId);
opaque_material_id!(PreparedMaterialAttachmentId);
opaque_material_id!(MaterialKeyBindingReceipt);

/// Bytes that bind a material operation to its declared cryptographic context.
#[derive(Clone, Eq, PartialEq)]
pub struct AssociatedData(Vec<u8>);

impl AssociatedData {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for AssociatedData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AssociatedData")
            .field(&self.0)
            .finish()
    }
}

/// A data-encryption key held in a secrecy zeroizing container.
///
/// ```compile_fail
/// use vestrace_domain::ZeroizingDek;
///
/// let dek = ZeroizingDek::new([7_u8; 32]);
/// let _copied = dek.clone();
/// ```
pub struct ZeroizingDek(SecretBox<[u8; 32]>);

impl ZeroizingDek {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    /// Makes key bytes available only for the immediate cryptographic operation.
    pub fn expose<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(self.0.expose_secret())
    }
}

impl fmt::Debug for ZeroizingDek {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ZeroizingDek(REDACTED)")
    }
}

/// Lifecycle states shared by content and result material-key creation intents.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKeyCreationIntentState {
    Reserved,
    ProvisionalCreated,
    ProvisionalReceipted,
    ContentPrepared,
    ResultPrepared,
    ContentAbandonPrepared,
    /// The abort taken before any prepared marker exists. Distinct from
    /// [`Self::ContentAbandonPrepared`]: the spec refuses to let a
    /// ContentPrepared-only marker stand in for it.
    PrePreparedAbandonPrepared,
    Bound,
    Live,
    ErasurePrepared,
    Tombstoned,
    Abandoned,
}

/// Lifecycle states shared by credential material-key creation intents.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKeyCreationIntentState {
    Reserved,
    ProvisionalCreated,
    ProvisionalReceipted,
    CredentialPrepared,
    CredentialAbandonPrepared,
    Bound,
    Candidate,
    ErasurePrepared,
    Destroyed,
    Abandoned,
}
