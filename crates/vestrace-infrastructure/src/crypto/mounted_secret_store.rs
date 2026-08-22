//! Key custody delivered by the orchestrator.
//!
//! # Why this is not [`super::LOCAL_FILE_PROVIDER`] under another name
//!
//! With a local file the caller names a path and receives the bytes. Provider,
//! key id, version, purpose, scope and algorithm all live in a [`KeyReference`]
//! the caller constructed, so every check compares the caller's word against
//! the caller's word: a caller that wants a different scope writes a different
//! reference.
//!
//! Here those five facts are declared by the store and read from it. The
//! request is checked against what the store says, which is a statement the
//! caller cannot edit — so this adapter can refuse, and refusing is the whole
//! difference.
//!
//! # What this does not guarantee
//!
//! Nothing about the filesystem: not permissions, not ephemerality, not who
//! else can read the mount. Those checks fork per platform and would go
//! unverified on the machine that develops them. The custody guarantee is the
//! orchestrator's to make; this adapter reads what it was given and reports
//! what it finds.

use std::path::{Path, PathBuf};

use vestrace_domain::trust::{
    KeyProvider, KeyProviderError, KeyPurpose, KeyReference, ResolvedKeyMaterial,
    SecretResolutionRequest,
};

/// The provider name every production custody accepts and
/// [`super::LOCAL_FILE_PROVIDER`] is refused for.
pub const MOUNTED_SECRET_STORE_PROVIDER: &str = "mounted-secret-store";

/// What the store says a key is for, as opposed to what a caller claims.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyDeclaration {
    pub scope: String,
    pub purpose: String,
    pub algorithm: String,
}

pub struct MountedSecretStoreKeyProvider {
    root: PathBuf,
}

impl MountedSecretStoreKeyProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// One path segment, or nothing.
    ///
    /// A key id is an identifier, not a path: accepting a separator, `..`, a
    /// bare `.`, or a drive-relative fragment like `C:keys` would let a
    /// caller name any file on the host and call it a key. Windows resolves
    /// `Path::new(root).join(value)` by replacing the whole path outright
    /// when `value` carries a prefix or is rooted, so string-contains checks
    /// on `/`, `\` and `..` are not sufficient on this platform: the value is
    /// required to parse as exactly one [`std::path::Component::Normal`].
    fn segment(name: &str, value: &str) -> Result<(), KeyProviderError> {
        if value.trim().is_empty() {
            return Err(KeyProviderError::Denied(format!(
                "{name} must be a single path segment"
            )));
        }
        let mut components = Path::new(value).components();
        match (components.next(), components.next()) {
            (Some(std::path::Component::Normal(_)), None) => Ok(()),
            _ => Err(KeyProviderError::Denied(format!(
                "{name} must be a single path segment"
            ))),
        }
    }

    fn read_trimmed(path: &Path, what: &str) -> Result<String, KeyProviderError> {
        let raw = std::fs::read_to_string(path).map_err(|error| {
            KeyProviderError::Unavailable(format!("{what} is unreadable: {error}"))
        })?;
        Ok(raw.trim().to_owned())
    }

    pub fn declaration(&self, key_id: &str) -> Result<KeyDeclaration, KeyProviderError> {
        Self::segment("key id", key_id)?;
        let key_dir = self.root.join(key_id);
        if !key_dir.is_dir() {
            return Err(KeyProviderError::Unavailable(format!(
                "the store holds no key {key_id}"
            )));
        }
        Ok(KeyDeclaration {
            scope: Self::read_trimmed(&key_dir.join("scope"), "declared scope")?,
            purpose: Self::read_trimmed(&key_dir.join("purpose"), "declared purpose")?,
            algorithm: Self::read_trimmed(&key_dir.join("algorithm"), "declared algorithm")?,
        })
    }

    /// Every version the store holds with the state it declares, sorted so a
    /// probe reading two of them gets the same pair on every run.
    ///
    /// An orchestrator-fed mount is not only the version directories this
    /// adapter cares about: Kubernetes Secret projections add sidecar
    /// entries such as `..data` (a symlink to the current revision) and
    /// `..<timestamp>` beside them. This adapter's whole point is being fed
    /// by an orchestrator, so an entry that is not a version this store can
    /// name — its name fails [`Self::segment`], or it holds no readable
    /// `state` file — is skipped rather than failing the entire listing.
    pub fn versions(&self, key_id: &str) -> Result<Vec<(String, String)>, KeyProviderError> {
        Self::segment("key id", key_id)?;
        let key_dir = self.root.join(key_id);
        let entries = std::fs::read_dir(&key_dir).map_err(|error| {
            KeyProviderError::Unavailable(format!("the store holds no key {key_id}: {error}"))
        })?;
        let mut versions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| KeyProviderError::Unavailable(format!("{error}")))?;
            if !entry.path().is_dir() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().into_owned();
            if Self::segment("key version", &version).is_err() {
                continue;
            }
            let state = match Self::read_trimmed(&entry.path().join("state"), "declared state") {
                Ok(state) => state,
                Err(_) => continue,
            };
            versions.push((version, state));
        }
        versions.sort();
        Ok(versions)
    }

    pub fn public_key(&self, key_id: &str, version: &str) -> Result<Vec<u8>, KeyProviderError> {
        Self::segment("key id", key_id)?;
        Self::segment("key version", version)?;
        std::fs::read(self.root.join(key_id).join(version).join("public.bin")).map_err(|error| {
            KeyProviderError::Unavailable(format!("public key is unreadable: {error}"))
        })
    }

    /// Read a public key only after applying the same custody validation as a
    /// private-key resolution. Callers verifying signatures must not need to
    /// read PKCS#8 material, but they still need provider, declaration,
    /// authorization, and lifecycle enforcement.
    pub fn resolve_public_key(
        &self,
        key: &KeyReference,
        request: &SecretResolutionRequest,
    ) -> Result<Vec<u8>, KeyProviderError> {
        let version_dir = self.validate_resolution(key, request)?;
        std::fs::read(version_dir.join("public.bin")).map_err(|error| {
            KeyProviderError::Unavailable(format!("public key is unreadable: {error}"))
        })
    }

    fn validate_resolution(
        &self,
        key: &KeyReference,
        request: &SecretResolutionRequest,
    ) -> Result<PathBuf, KeyProviderError> {
        if key.provider() != MOUNTED_SECRET_STORE_PROVIDER {
            return Err(KeyProviderError::Denied(format!(
                "provider '{}' is not implemented by the mounted store adapter",
                key.provider()
            )));
        }
        if !key.is_usable() {
            return Err(KeyProviderError::NotUsable);
        }
        if request.authorization_ref().trim().is_empty() {
            return Err(KeyProviderError::Denied(
                "key resolution request carries no authorization reference".into(),
            ));
        }

        Self::segment("key id", key.key_id())?;
        Self::segment("key version", key.version())?;

        let declaration = self.declaration(key.key_id())?;
        if declaration.purpose != purpose_name(key.purpose()) {
            return Err(KeyProviderError::Denied(format!(
                "the store declares key {} for {}, not {}",
                key.key_id(),
                declaration.purpose,
                purpose_name(key.purpose())
            )));
        }
        if declaration.algorithm != key.algorithm_suite() {
            return Err(KeyProviderError::Denied(format!(
                "the store declares key {} as {}, not {}",
                key.key_id(),
                declaration.algorithm,
                key.algorithm_suite()
            )));
        }
        // Two different claims, checked separately: what the caller says the
        // key is for, and what this particular resolution is for.
        if declaration.scope != key.scope() {
            return Err(KeyProviderError::Denied(format!(
                "the store declares key {} for scope {}, not {}",
                key.key_id(),
                declaration.scope,
                key.scope()
            )));
        }
        if declaration.scope != request.purpose() {
            return Err(KeyProviderError::Denied(format!(
                "resolution for {} is outside the declared scope {}",
                request.purpose(),
                declaration.scope
            )));
        }

        let version_dir = self.root.join(key.key_id()).join(key.version());
        if !version_dir.is_dir() {
            return Err(KeyProviderError::Unavailable(format!(
                "the store holds no version {} of {}",
                key.version(),
                key.key_id()
            )));
        }
        let state = Self::read_trimmed(&version_dir.join("state"), "declared state")?;
        if state != "active" && state != "rotating" {
            return Err(KeyProviderError::NotUsable);
        }
        Ok(version_dir)
    }
}

/// The serde name of a purpose, which is what a store declaration writes.
fn purpose_name(purpose: KeyPurpose) -> &'static str {
    match purpose {
        KeyPurpose::Storage => "storage",
        KeyPurpose::Export => "export",
        KeyPurpose::Signing => "signing",
        KeyPurpose::Federation => "federation",
        KeyPurpose::Backup => "backup",
        KeyPurpose::Provider => "provider",
    }
}

impl KeyProvider for MountedSecretStoreKeyProvider {
    fn resolve(
        &self,
        key: &KeyReference,
        request: &SecretResolutionRequest,
    ) -> Result<ResolvedKeyMaterial, KeyProviderError> {
        let version_dir = self.validate_resolution(key, request)?;

        let bytes = std::fs::read(version_dir.join("private.pkcs8")).map_err(|error| {
            KeyProviderError::Unavailable(format!("key material is unreadable: {error}"))
        })?;
        ResolvedKeyMaterial::from_ephemeral(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `purpose_name` is a hand-written mirror of `KeyPurpose`'s serde
    /// mapping, and nothing pins the two together — a rename on either side
    /// would drift silently. Serializing each variant through `serde_json`
    /// and comparing pins the mirror to the thing it mirrors.
    #[test]
    fn purpose_name_matches_every_key_purpose_serde_name() {
        for purpose in [
            KeyPurpose::Storage,
            KeyPurpose::Export,
            KeyPurpose::Signing,
            KeyPurpose::Federation,
            KeyPurpose::Backup,
            KeyPurpose::Provider,
        ] {
            let serialized = serde_json::to_string(&purpose).unwrap();
            let serde_name = serialized.trim_matches('"');
            assert_eq!(
                purpose_name(purpose),
                serde_name,
                "purpose_name drifted from KeyPurpose's serde mapping for {purpose:?}"
            );
        }
    }
}
