use std::fmt;

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{PrincipalId, WorkspaceId};

pub const FINGERPRINT_CONTINUITY_DOMAIN: &[u8] = b"vestrace-installation-fingerprint-v1";
const EXTERNAL_ID_FINGERPRINT_DOMAIN: &[u8] = b"vestrace-external-id-fingerprint-v1";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InstallationId(uuid::Uuid);

impl InstallationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for InstallationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for InstallationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FingerprintKeyId(uuid::Uuid);

impl FingerprintKeyId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for FingerprintKeyId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for FingerprintKeyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FingerprintKeyVersion(i32);

impl FingerprintKeyVersion {
    pub const V1: Self = Self(1);

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

/// Key material held by the host vault. It has no serialization or database
/// representation, and its bytes are zeroized when the value is dropped.
pub struct FingerprintKey(Zeroizing<[u8; 32]>);

impl FingerprintKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The create-only host-vault record for the one installation fingerprint key.
pub struct InstallationFingerprintKey {
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    version: FingerprintKeyVersion,
    key: FingerprintKey,
}

impl InstallationFingerprintKey {
    pub fn new(
        installation_id: InstallationId,
        fingerprint_key_id: FingerprintKeyId,
        version: FingerprintKeyVersion,
        key: FingerprintKey,
    ) -> Self {
        Self {
            installation_id,
            fingerprint_key_id,
            version,
            key,
        }
    }

    pub const fn installation_id(&self) -> InstallationId {
        self.installation_id
    }

    pub const fn fingerprint_key_id(&self) -> FingerprintKeyId {
        self.fingerprint_key_id
    }

    pub const fn version(&self) -> FingerprintKeyVersion {
        self.version
    }

    pub fn continuity_proof(&self) -> FingerprintKeyContinuityProof {
        continuity_proof(&self.key, &self.installation_id, &self.fingerprint_key_id)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct FingerprintKeyContinuityProof([u8; 32]);

impl FingerprintKeyContinuityProof {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ExternalIdFingerprint([u8; 32]);

impl ExternalIdFingerprint {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FingerprintScope {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    protocol: String,
    endpoint: String,
}

impl FingerprintScope {
    pub fn new(
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        protocol: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id,
            principal_id,
            protocol: protocol.into(),
            endpoint: endpoint.into(),
        }
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

pub fn continuity_proof(
    key: &FingerprintKey,
    installation: &InstallationId,
    key_id: &FingerprintKeyId,
) -> FingerprintKeyContinuityProof {
    let mut message = Vec::with_capacity(
        FINGERPRINT_CONTINUITY_DOMAIN.len()
            + installation.as_uuid().as_bytes().len()
            + key_id.as_uuid().as_bytes().len(),
    );
    message.extend_from_slice(FINGERPRINT_CONTINUITY_DOMAIN);
    message.extend_from_slice(installation.as_uuid().as_bytes());
    message.extend_from_slice(key_id.as_uuid().as_bytes());

    FingerprintKeyContinuityProof(hmac_sha256(key.bytes(), &message))
}

/// Produces the scoped uniqueness index for an untrusted external identifier.
/// It is intentionally a different type and domain from continuity evidence.
pub fn external_id_fingerprint(
    key: &FingerprintKey,
    scope: &FingerprintScope,
    external_id: &str,
) -> ExternalIdFingerprint {
    let mut message = Vec::new();
    message.extend_from_slice(EXTERNAL_ID_FINGERPRINT_DOMAIN);
    message.extend_from_slice(scope.workspace_id.as_uuid().as_bytes());
    message.extend_from_slice(scope.principal_id.as_uuid().as_bytes());
    append_length_prefixed(&mut message, &scope.protocol);
    append_length_prefixed(&mut message, &scope.endpoint);
    append_length_prefixed(&mut message, external_id);

    ExternalIdFingerprint(hmac_sha256(key.bytes(), &message))
}

fn append_length_prefixed(message: &mut Vec<u8>, value: &str) {
    let length = u32::try_from(value.len()).expect("fingerprint input exceeds u32 length");
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(value.as_bytes());
}

fn hmac_sha256(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut inner_pad = [0x36; 64];
    let mut outer_pad = [0x5c; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    let digest = outer.finalize();
    let mut output = [0; 32];
    output.copy_from_slice(&digest);
    output
}
