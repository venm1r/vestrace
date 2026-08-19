# Mounted Secret Store Crypto Custody Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A production key custody adapter whose store — not the caller — declares key identity, scope, purpose, algorithm and lifecycle, plus a probe that collects all six crypto qualification checks by executing them.

**Architecture:** A new `MountedSecretStoreKeyProvider` in `vestrace-infrastructure` reads a directory tree the orchestrator mounts, resolving by `KeyReference` rather than by path and refusing requests the store's declarations do not support. A `MountedStoreCryptoProbe` beside it implements the application's `CryptoAdapterQualificationProbe` by attempting each property and recording only what actually happened. Two CLI surfaces consume it: `conformance sign` gains a second provider, and `conformance release` gains `--crypto-evidence`.

**Tech Stack:** Rust 2024, `ring` (Ed25519, already a dependency of `vestrace-infrastructure`), `thiserror` via the existing `KeyProviderError`, plain `std::fs`.

**Spec:** `docs/superpowers/specs/2026-08-19-mounted-secret-store-crypto-custody-design.md`

## Global Constraints

- No configuration file changes. The store root is a CLI argument, never a config section.
- Filesystem properties are **not** checked: no permission bits, no refusal of non-ephemeral paths. Pure path-and-content work only, so every test runs on Windows and in a Linux container.
- Private key files are PKCS#8 v2 as `ring::signature::Ed25519KeyPair::from_pkcs8` accepts; public key files are 32 raw bytes.
- No error message, `Display` or `Debug` rendering may contain private key bytes.
- The provider name is exactly `mounted-secret-store`; the custody is `CryptoCustody::MountedSecretStore`.
- `KeyPurpose` values are compared as their serde snake_case names (`storage`, `export`, `signing`, `federation`, `backup`, `provider`).
- Existing behaviour of `LocalFileKeyProvider` and `--private-key-file` must not change.

---

## File Structure

| File | Responsibility |
|---|---|
| Create: `crates/vestrace-infrastructure/src/crypto/mounted_secret_store.rs` | The store contract: declaration reading, segment validation, `KeyProvider` implementation, version listing, public key reading |
| Create: `crates/vestrace-infrastructure/src/crypto/mounted_store_probe.rs` | `CryptoAdapterQualificationProbe` over the adapter above |
| Modify: `crates/vestrace-infrastructure/src/crypto/mod.rs` | Declare and re-export both modules |
| Create: `tests/mounted_secret_store.rs` | Refusal cases and probe cases against a temporary store |
| Modify: `crates/vestrace-cli/src/main.rs` | `--key-store-root` on `sign`; `--crypto-evidence` and key identity flags on `release` |
| Modify: `crates/vestrace-cli/src/commands/conformance.rs` | Provider dispatch in `resolve_signing_key`; `collect_crypto_qualification` |
| Create: `crates/vestrace-cli/tests/crypto_evidence_cli.rs` | End-to-end three-outcome proof through the CLI |

---

### Task 1: The store contract and its refusals

**Files:**
- Create: `crates/vestrace-infrastructure/src/crypto/mounted_secret_store.rs`
- Modify: `crates/vestrace-infrastructure/src/crypto/mod.rs`
- Test: `tests/mounted_secret_store.rs`

**Interfaces:**
- Consumes: `vestrace_domain::trust::{KeyProvider, KeyProviderError, KeyPurpose, KeyReference, ResolvedKeyMaterial, SecretResolutionRequest}`
- Produces:
  - `pub const MOUNTED_SECRET_STORE_PROVIDER: &str = "mounted-secret-store";`
  - `pub struct MountedSecretStoreKeyProvider` with `pub fn new(root: impl Into<PathBuf>) -> Self`
  - `pub fn public_key(&self, key_id: &str, version: &str) -> Result<Vec<u8>, KeyProviderError>`
  - `pub fn versions(&self, key_id: &str) -> Result<Vec<(String, String)>, KeyProviderError>` returning `(version, state)` sorted by version
  - `pub fn declaration(&self, key_id: &str) -> Result<KeyDeclaration, KeyProviderError>`
  - `pub struct KeyDeclaration { pub scope: String, pub purpose: String, pub algorithm: String }`
  - `impl KeyProvider for MountedSecretStoreKeyProvider`

- [ ] **Step 1: Write the failing test for the scope the store did not declare**

Create `tests/mounted_secret_store.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vestrace_domain::WorkspaceId;
use vestrace_domain::trust::{
    KeyProvider, KeyProviderError, KeyPurpose, KeyReference, SecretResolutionRequest,
};
use vestrace_infrastructure::crypto::{
    MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider,
};

fn store_root(id: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vestrace-store-{id}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// Writes one key directory with its declarations, and one version in `state`.
fn write_key(root: &Path, key_id: &str, scope: &str, version: &str, state: &str) {
    let key_dir = root.join(key_id);
    fs::create_dir_all(&key_dir).unwrap();
    fs::write(key_dir.join("scope"), scope).unwrap();
    fs::write(key_dir.join("purpose"), "signing").unwrap();
    fs::write(key_dir.join("algorithm"), "ed25519").unwrap();
    write_version(root, key_id, version, state);
}

fn write_version(root: &Path, key_id: &str, version: &str, state: &str) {
    let version_dir = root.join(key_id).join(version);
    fs::create_dir_all(&version_dir).unwrap();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    fs::write(version_dir.join("state"), state).unwrap();
    fs::write(version_dir.join("private.pkcs8"), pkcs8.as_ref()).unwrap();
    fs::write(version_dir.join("public.bin"), pair.public_key().as_ref()).unwrap();
}

fn key_ref(key_id: &str, version: &str, scope: &str) -> KeyReference {
    KeyReference::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        key_id,
        version,
        KeyPurpose::Signing,
        scope,
        "ed25519",
    )
    .unwrap()
}

fn request(purpose: &str) -> SecretResolutionRequest {
    SecretResolutionRequest::new(WorkspaceId::new(), purpose, "test://authorization")
}

/// The store's declaration is the one the caller cannot edit. A reference
/// claiming a scope the store never granted is refused even though the caller
/// wrote both halves of its own request.
#[test]
fn a_scope_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "export"),
            &request("export"),
        )
        .expect_err("a scope the store did not declare must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "expected Denied, got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store`
Expected: FAIL to compile with `unresolved import vestrace_infrastructure::crypto::MountedSecretStoreKeyProvider`.

- [ ] **Step 3: Write the adapter**

Create `crates/vestrace-infrastructure/src/crypto/mounted_secret_store.rs`:

```rust
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
    /// A key id is an identifier, not a path: accepting a separator or `..`
    /// would let a caller name any file on the host and call it a key.
    fn segment(name: &str, value: &str) -> Result<(), KeyProviderError> {
        if value.trim().is_empty()
            || value.contains('/')
            || value.contains('\\')
            || value.contains("..")
        {
            return Err(KeyProviderError::Denied(format!(
                "{name} must be a single path segment"
            )));
        }
        Ok(())
    }

    fn read_trimmed(path: &Path, what: &str) -> Result<String, KeyProviderError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|error| KeyProviderError::Unavailable(format!("{what} is unreadable: {error}")))?;
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
    pub fn versions(&self, key_id: &str) -> Result<Vec<(String, String)>, KeyProviderError> {
        Self::segment("key id", key_id)?;
        let key_dir = self.root.join(key_id);
        let entries = std::fs::read_dir(&key_dir).map_err(|error| {
            KeyProviderError::Unavailable(format!("the store holds no key {key_id}: {error}"))
        })?;
        let mut versions = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| KeyProviderError::Unavailable(format!("{error}")))?;
            if !entry.path().is_dir() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().into_owned();
            let state = Self::read_trimmed(&entry.path().join("state"), "declared state")?;
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

        let bytes = std::fs::read(version_dir.join("private.pkcs8")).map_err(|error| {
            KeyProviderError::Unavailable(format!("key material is unreadable: {error}"))
        })?;
        ResolvedKeyMaterial::from_ephemeral(bytes)
    }
}
```

Modify `crates/vestrace-infrastructure/src/crypto/mod.rs`, adding after the existing `pub const ALGORITHM_SUITE` declaration:

```rust
mod mounted_secret_store;

pub use mounted_secret_store::{
    KeyDeclaration, MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider,
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store`
Expected: PASS, 1 test.

- [ ] **Step 5: Write the remaining refusal cases**

Append to `tests/mounted_secret_store.rs`:

```rust
/// The lifecycle state belongs to the store. A revoked key is refused however
/// the caller describes it.
#[test]
fn a_revoked_version_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "revoked");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("a revoked version must be refused");

    assert!(matches!(error, KeyProviderError::NotUsable), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

/// A key rotating out is still usable: during a rotation both halves have to
/// verify material signed by the other.
#[test]
fn a_rotating_version_still_resolves() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "rotating");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    assert!(
        provider
            .resolve(
                &key_ref("release-signing", "v1", "release"),
                &request("release")
            )
            .is_ok()
    );

    fs::remove_dir_all(&root).ok();
}

/// The resolution's own purpose is checked against the declaration separately
/// from the reference's scope, because they are two different claims.
#[test]
fn a_resolution_outside_the_declared_scope_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("export"),
        )
        .expect_err("a resolution outside the declared scope must be refused");

    assert!(matches!(error, KeyProviderError::Denied(_)), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

/// A key id is an identifier, not a path.
#[test]
fn a_key_id_cannot_walk_out_of_the_store() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("../../etc", "v1", "release"),
            &request("release"),
        )
        .expect_err("a traversing key id must be refused");

    assert!(matches!(error, KeyProviderError::Denied(_)), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unknown_key_is_unavailable() {
    let root = store_root(&suffix());
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref("absent", "v1", "release"), &request("release"))
        .expect_err("an absent key must be refused");

    assert!(
        matches!(error, KeyProviderError::Unavailable(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unknown_version_is_unavailable() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v9", "release"),
            &request("release"),
        )
        .expect_err("an absent version must be refused");

    assert!(
        matches!(error, KeyProviderError::Unavailable(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A resolution nobody authorized is not a resolution.
#[test]
fn a_request_without_an_authorization_reference_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &SecretResolutionRequest::new(WorkspaceId::new(), "release", "   "),
        )
        .expect_err("an unauthorized request must be refused");

    assert!(matches!(error, KeyProviderError::Denied(_)), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

/// The purpose a store declares is not overridden by the purpose a caller
/// names.
#[test]
fn a_purpose_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    fs::write(root.join("release-signing").join("purpose"), "storage").unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("a purpose the store did not declare must be refused");

    assert!(matches!(error, KeyProviderError::Denied(_)), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_algorithm_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    fs::write(root.join("release-signing").join("algorithm"), "p256").unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("an algorithm the store did not declare must be refused");

    assert!(matches!(error, KeyProviderError::Denied(_)), "got {error:?}");

    fs::remove_dir_all(&root).ok();
}

/// Nothing the adapter renders may carry the material it protects.
#[test]
fn no_refusal_discloses_key_material() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let private = fs::read(
        root.join("release-signing")
            .join("v1")
            .join("private.pkcs8"),
    )
    .unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let rendered = [
        provider
            .resolve(
                &key_ref("release-signing", "v1", "export"),
                &request("export"),
            )
            .err()
            .map(|error| format!("{error} {error:?}"))
            .unwrap_or_default(),
        provider
            .resolve(&key_ref("absent", "v1", "release"), &request("release"))
            .err()
            .map(|error| format!("{error} {error:?}"))
            .unwrap_or_default(),
    ]
    .join(" ");

    let needle: String = private.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(!rendered.contains(&needle), "a refusal disclosed key material");
    assert!(
        !rendered.as_bytes().windows(private.len()).any(|w| w == private),
        "a refusal disclosed key material"
    );

    fs::remove_dir_all(&root).ok();
}
```

- [ ] **Step 6: Run the suite**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store`
Expected: PASS, 11 tests.

- [ ] **Step 7: Mutation-prove the store's authority**

Comment out the `declaration.scope != key.scope()` check in `resolve`, run the suite, and confirm `a_scope_the_store_did_not_declare_is_refused` fails **and no other case does**. Restore it. Repeat for the `state` check against `a_revoked_version_is_refused`, and for the `Self::segment("key id", …)` call against `a_key_id_cannot_walk_out_of_the_store`.

Expected each time: exactly one failing case, the matching one.

- [ ] **Step 8: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-integration-tests --test mounted_secret_store
git add crates/vestrace-infrastructure/src/crypto/mounted_secret_store.rs \
        crates/vestrace-infrastructure/src/crypto/mod.rs \
        tests/mounted_secret_store.rs
git commit -m "feat(infrastructure): read signing keys from a mounted secret store"
```

---

### Task 2: The probe that executes the six checks

**Files:**
- Create: `crates/vestrace-infrastructure/src/crypto/mounted_store_probe.rs`
- Modify: `crates/vestrace-infrastructure/src/crypto/mod.rs`
- Test: `tests/mounted_secret_store.rs` (append)

**Interfaces:**
- Consumes: `MountedSecretStoreKeyProvider` from Task 1; `vestrace_application::{ApplicationError, CryptoAdapterQualificationEvidence, CryptoAdapterQualificationProbe, CryptoAdapterQualificationTarget, CryptoQualificationCheck}`
- Produces: `pub struct MountedStoreCryptoProbe` with `pub fn new(provider: MountedSecretStoreKeyProvider) -> Self`, implementing `CryptoAdapterQualificationProbe`

- [ ] **Step 1: Write the failing test for rotation over identical material**

Append to `tests/mounted_secret_store.rs`:

```rust
use vestrace_application::{
    CryptoAdapterQualificationProbe, CryptoAdapterQualificationTarget, CryptoCustody,
    CryptoQualificationCheck,
};
use vestrace_infrastructure::crypto::MountedStoreCryptoProbe;

fn target(key_id: &str, version: &str, scope: &str) -> CryptoAdapterQualificationTarget {
    CryptoAdapterQualificationTarget::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        CryptoCustody::MountedSecretStore,
        key_id,
        version,
        "ed25519",
        KeyPurpose::Signing,
        scope,
    )
    .unwrap()
}

/// Two versions holding the same key are one key under two names. Counting
/// directories would call that a rotation; comparing the material does not.
#[test]
fn identical_material_under_two_versions_is_not_a_rotation() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let key_dir = root.join("release-signing");
    fs::create_dir_all(key_dir.join("v2")).unwrap();
    fs::write(key_dir.join("v2").join("state"), "retired").unwrap();
    fs::copy(
        key_dir.join("v1").join("private.pkcs8"),
        key_dir.join("v2").join("private.pkcs8"),
    )
    .unwrap();
    fs::copy(
        key_dir.join("v1").join("public.bin"),
        key_dir.join("v2").join("public.bin"),
    )
    .unwrap();

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v1", "release"))
        .unwrap();

    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Rotation),
        "the same key under two version names is not a rotation"
    );

    fs::remove_dir_all(&root).ok();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store identical_material`
Expected: FAIL to compile with `unresolved import vestrace_infrastructure::crypto::MountedStoreCryptoProbe`.

- [ ] **Step 3: Write the probe**

Create `crates/vestrace-infrastructure/src/crypto/mounted_store_probe.rs`:

```rust
//! Crypto qualification evidence, collected by doing rather than by claiming.
//!
//! Each check below is recorded only when its attempt produced the outcome the
//! check is about. A probe that reports six checks unconditionally is a probe
//! that checks nothing, so every recording here is downstream of an operation
//! that could have gone the other way.

use ring::signature::{ED25519, Ed25519KeyPair, UnparsedPublicKey};
use vestrace_application::{
    ApplicationError, CryptoAdapterQualificationEvidence, CryptoAdapterQualificationProbe,
    CryptoAdapterQualificationTarget, CryptoQualificationCheck,
};
use vestrace_domain::WorkspaceId;
use vestrace_domain::trust::{KeyProvider, KeyReference, SecretResolutionRequest};

use super::mounted_secret_store::{MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider};

const PROBE_PAYLOAD: &[u8] = b"vestrace crypto qualification probe";

pub struct MountedStoreCryptoProbe {
    provider: MountedSecretStoreKeyProvider,
}

impl MountedStoreCryptoProbe {
    pub fn new(provider: MountedSecretStoreKeyProvider) -> Self {
        Self { provider }
    }

    fn request(purpose: &str) -> SecretResolutionRequest {
        SecretResolutionRequest::new(WorkspaceId::new(), purpose, "conformance://crypto-qualify")
    }
}

impl CryptoAdapterQualificationProbe for MountedStoreCryptoProbe {
    fn collect(
        &self,
        target: &CryptoAdapterQualificationTarget,
    ) -> Result<CryptoAdapterQualificationEvidence, ApplicationError> {
        let declaration = self
            .provider
            .declaration(target.key_id())
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        // The observed key describes what the store says, not what the target
        // asked for. The qualification service compares the two, which it can
        // only do if they are gathered from different places.
        let observed = KeyReference::new(
            MOUNTED_SECRET_STORE_PROVIDER,
            target.key_id(),
            target.key_version(),
            target.purpose(),
            declaration.scope.clone(),
            declaration.algorithm.clone(),
        )
        .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        let mut checks = Vec::new();
        let mut refs = Vec::new();

        let active = self
            .provider
            .resolve(&observed, &Self::request(&declaration.scope));
        let material = match active {
            Ok(material) => {
                checks.push(CryptoQualificationCheck::Resolution);
                refs.push(format!(
                    "evidence:mounted-store/resolution/{}/{}",
                    target.key_id(),
                    target.key_version()
                ));
                Some(material.into_bytes())
            }
            Err(_) => None,
        };

        // Scope isolation: a resolution for something the store did not declare
        // must be refused. Attempted, not assumed.
        let foreign_scope = format!("{}-not-this", declaration.scope);
        if self
            .provider
            .resolve(&observed, &Self::request(&foreign_scope))
            .is_err()
        {
            checks.push(CryptoQualificationCheck::ScopeIsolation);
            refs.push(format!(
                "evidence:mounted-store/scope-denial/{}",
                target.key_id()
            ));
        }

        if let Some(bytes) = &material {
            if let Ok(pair) = Ed25519KeyPair::from_pkcs8(bytes) {
                let signature = pair.sign(PROBE_PAYLOAD);
                if let Ok(public) = self
                    .provider
                    .public_key(target.key_id(), target.key_version())
                {
                    if UnparsedPublicKey::new(&ED25519, public)
                        .verify(PROBE_PAYLOAD, signature.as_ref())
                        .is_ok()
                    {
                        checks.push(CryptoQualificationCheck::CryptographicRoundTrip);
                        refs.push(format!(
                            "evidence:mounted-store/sign-verify/{}/{}",
                            target.key_id(),
                            target.key_version()
                        ));
                    }
                }
            }
        }

        let versions = self
            .provider
            .versions(target.key_id())
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        // Lifecycle: a version the store has revoked is refused.
        if let Some((version, _)) = versions.iter().find(|(_, state)| state == "revoked") {
            let revoked = KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                target.key_id(),
                version,
                target.purpose(),
                declaration.scope.clone(),
                declaration.algorithm.clone(),
            )
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;
            if self
                .provider
                .resolve(&revoked, &Self::request(&declaration.scope))
                .is_err()
            {
                checks.push(CryptoQualificationCheck::Lifecycle);
                refs.push(format!(
                    "evidence:mounted-store/lifecycle/{}/{}",
                    target.key_id(),
                    version
                ));
            }
        }

        // Rotation: a superseded version exists, is refused, and is a
        // different key. The last clause is the one that matters — the same
        // material under two names would satisfy the first two.
        let active_public = self
            .provider
            .public_key(target.key_id(), target.key_version())
            .ok();
        if let Some(active_public) = active_public {
            let rotated = versions.iter().find(|(version, state)| {
                version != target.key_version() && (state == "retired" || state == "revoked")
            });
            if let Some((version, _)) = rotated {
                let superseded = self.provider.public_key(target.key_id(), version).ok();
                if superseded.is_some_and(|public| public != active_public) {
                    checks.push(CryptoQualificationCheck::Rotation);
                    refs.push(format!(
                        "evidence:mounted-store/rotation/{}/{}→{}",
                        target.key_id(),
                        version,
                        target.key_version()
                    ));
                }
            }
        }

        // Non-disclosure is a property of formatting code, which no type
        // prevents from regressing, so it is searched for rather than asserted.
        if let Some(bytes) = &material {
            let rendered = match self
                .provider
                .resolve(&observed, &Self::request(&foreign_scope))
            {
                Err(error) => format!("{error} {error:?}"),
                Ok(_) => String::new(),
            };
            let leaked = rendered
                .as_bytes()
                .windows(bytes.len().max(1))
                .any(|window| window == bytes.as_slice());
            if !rendered.is_empty() && !leaked {
                checks.push(CryptoQualificationCheck::SecretNonDisclosure);
                refs.push(format!(
                    "evidence:mounted-store/non-disclosure/{}",
                    target.key_id()
                ));
            }
        }

        Ok(CryptoAdapterQualificationEvidence::new(
            observed, checks, refs,
        ))
    }
}
```

Modify `crates/vestrace-infrastructure/src/crypto/mod.rs`, adding beside the Task 1 lines:

```rust
mod mounted_store_probe;

pub use mounted_store_probe::MountedStoreCryptoProbe;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store identical_material`
Expected: PASS.

- [ ] **Step 5: Write the complete-store and thin-store cases**

Append to `tests/mounted_secret_store.rs`:

```rust
/// A store with an active key, a revoked predecessor holding different
/// material, and readable public halves supports all six checks.
#[test]
fn a_complete_store_yields_every_check() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v2", "active");
    write_version(&root, "release-signing", "v1", "revoked");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v2", "release"))
        .unwrap();

    for check in [
        CryptoQualificationCheck::Resolution,
        CryptoQualificationCheck::ScopeIsolation,
        CryptoQualificationCheck::CryptographicRoundTrip,
        CryptoQualificationCheck::Lifecycle,
        CryptoQualificationCheck::Rotation,
        CryptoQualificationCheck::SecretNonDisclosure,
    ] {
        assert!(
            evidence.passed_checks().contains(&check),
            "{check:?} was not collected"
        );
    }
    assert_eq!(evidence.evidence_refs().len(), 6);

    fs::remove_dir_all(&root).ok();
}

/// One version is not a rotation and no revocation is not a lifecycle.
#[test]
fn a_single_version_store_yields_neither_rotation_nor_lifecycle() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v1", "release"))
        .unwrap();

    assert!(
        evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Resolution)
    );
    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Rotation)
    );
    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Lifecycle)
    );

    fs::remove_dir_all(&root).ok();
}

/// The service's own verdict over a complete store, which is the point of all
/// of it.
#[test]
fn a_complete_store_passes_crypto_qualification() {
    use vestrace_application::CryptoAdapterQualificationService;

    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v2", "active");
    write_version(&root, "release-signing", "v1", "revoked");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let decision = CryptoAdapterQualificationService::evaluate_with_probe(
        &target("release-signing", "v2", "release"),
        &probe,
    )
    .unwrap();

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());

    fs::remove_dir_all(&root).ok();
}
```

- [ ] **Step 6: Run the suite**

Run: `cargo test -p vestrace-integration-tests --test mounted_secret_store`
Expected: PASS, 15 tests.

- [ ] **Step 7: Mutation-prove each check**

For each of the six, break the property and confirm the matching case fails while the others stand:

| Break | Case that must fail |
|---|---|
| Make `resolve` return the material for any scope | `a_complete_store_yields_every_check` (ScopeIsolation) |
| Sign `PROBE_PAYLOAD` but verify a different payload | `a_complete_store_yields_every_check` (CryptographicRoundTrip) |
| Allow `revoked` in the adapter's state check | `a_complete_store_yields_every_check` (Lifecycle) |
| Record `Rotation` without comparing public keys | `identical_material_under_two_versions_is_not_a_rotation` |
| Record `SecretNonDisclosure` unconditionally | none — note this in the commit message as a check the suite does not pin, and add the leak case below |

For the last row, add a case that writes the private material into the refusal by formatting it into a `Denied` message in the adapter, and confirm the probe stops recording `SecretNonDisclosure`.

- [ ] **Step 8: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-integration-tests --test mounted_secret_store
git add crates/vestrace-infrastructure/src/crypto/mounted_store_probe.rs \
        crates/vestrace-infrastructure/src/crypto/mod.rs \
        tests/mounted_secret_store.rs
git commit -m "feat(infrastructure): collect crypto qualification by executing it"
```

---

### Task 3: Signing from the mounted store

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs` (the `Sign` variant)
- Modify: `crates/vestrace-cli/src/commands/conformance.rs` (`resolve_signing_key`, `run_sign`)
- Test: `crates/vestrace-cli/tests/crypto_evidence_cli.rs`

**Interfaces:**
- Consumes: `MountedSecretStoreKeyProvider` from Task 1
- Produces: `conformance sign --key-provider mounted-secret-store --key-store-root <path>`

- [ ] **Step 1: Write the failing test**

Create `crates/vestrace-cli/tests/crypto_evidence_cli.rs` with the store helpers from `tests/mounted_secret_store.rs` (repeated deliberately — this is a different crate and the helpers are not shared), plus:

```rust
/// A signature made from the store verifies against the store's public half,
/// which is what makes the second provider a provider rather than a name.
#[test]
fn an_artifact_can_be_signed_from_the_mounted_store() {
    let id = suffix();
    let root = store_root(&id);
    write_key(&root, "release-signing", "release", "v1", "active");
    let (manifest_path, signed_path) = manifest_paths(&id);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            "manifest",
            "--artifact-file",
            manifest_path.to_str().unwrap(),
            "--key-provider",
            "mounted-secret-store",
            "--key-store-root",
            root.to_str().unwrap(),
            "--signer-identity",
            "issuer://release",
            "--key-id",
            "release-signing",
            "--key-version",
            "v1",
            "--key-scope",
            "release",
            "--output",
            signed_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "signing failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let verified = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "verify-signature",
            "--artifact",
            "manifest",
            "--artifact-file",
            signed_path.to_str().unwrap(),
            "--public-key-file",
            root.join("release-signing")
                .join("v1")
                .join("public.bin")
                .to_str()
                .unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        verified.status.success(),
        "verification failed: {}",
        String::from_utf8_lossy(&verified.stderr)
    );

    fs::remove_dir_all(&root).ok();
}
```

Write `manifest_paths` to build the same manifest fixture used by `crates/vestrace-cli/tests/q7_signed_artifacts_cli.rs` and write it to a temp file, returning the unsigned and signed paths.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-cli --test crypto_evidence_cli an_artifact_can_be_signed`
Expected: FAIL with `unexpected argument '--key-store-root'`.

- [ ] **Step 3: Add the flag and the dispatch**

In `crates/vestrace-cli/src/main.rs`, change the `Sign` variant's `private_key_file: PathBuf` to `private_key_file: Option<PathBuf>` and add:

```rust
        #[arg(long)]
        key_store_root: Option<PathBuf>,
```

In `crates/vestrace-cli/src/commands/conformance.rs`, replace `resolve_signing_key`:

```rust
/// The custody named by `--key-provider` decides where the key comes from, and
/// exactly one source may be given for it: a command that silently preferred
/// one over the other would sign with a key its operator did not choose.
fn resolve_signing_key(
    key_ref: &KeyReference,
    private_key_file: Option<PathBuf>,
    key_store_root: Option<PathBuf>,
) -> anyhow::Result<Ed25519KeyPair> {
    let request =
        SecretResolutionRequest::new(WorkspaceId::new(), key_ref.scope(), "conformance://sign");
    let key_material = match (key_ref.provider(), private_key_file, key_store_root) {
        (vestrace_infrastructure::crypto::LOCAL_FILE_PROVIDER, Some(file), None) => {
            LocalFileKeyProvider::new(file).resolve(key_ref, &request)
        }
        (vestrace_infrastructure::crypto::MOUNTED_SECRET_STORE_PROVIDER, None, Some(root)) => {
            vestrace_infrastructure::crypto::MountedSecretStoreKeyProvider::new(root)
                .resolve(key_ref, &request)
        }
        (provider, _, _) => anyhow::bail!(
            "provider '{provider}' requires exactly one of --private-key-file (local-file) or \
             --key-store-root (mounted-secret-store)"
        ),
    }
    .map_err(|error| anyhow::anyhow!("signing key resolution failed: {error}"))?;
    Ed25519KeyPair::from_pkcs8(&key_material.into_bytes())
        .map_err(|_| anyhow::anyhow!("Ed25519 private key is not valid PKCS#8"))
}
```

Thread the new argument through `run_sign`'s signature and the `ConformanceAction::Sign` dispatch arm.

- [ ] **Step 4: Run tests**

Run: `cargo test -p vestrace-cli --test crypto_evidence_cli && cargo test -p vestrace-cli --test q7_signed_artifacts_cli`
Expected: both PASS — the second proves `local-file` signing is unchanged.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-cli --all-targets
git add crates/vestrace-cli/src/main.rs crates/vestrace-cli/src/commands/conformance.rs \
        crates/vestrace-cli/tests/crypto_evidence_cli.rs
git commit -m "feat(cli): sign release artifacts from a mounted secret store"
```

---

### Task 4: Crypto evidence in the release gate

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs` (the `Release` variant)
- Modify: `crates/vestrace-cli/src/commands/conformance.rs` (`run_release`, new `collect_crypto_qualification`)
- Test: `crates/vestrace-cli/tests/crypto_evidence_cli.rs` (append)

**Interfaces:**
- Consumes: `MountedStoreCryptoProbe` from Task 2; `run_release` from the existing release gate
- Produces: `conformance release --crypto-evidence --key-store-root <path> --key-id <id> --key-version <v> --key-scope <s>`

- [ ] **Step 1: Write the failing three-outcome test**

Append to `crates/vestrace-cli/tests/crypto_evidence_cli.rs`:

```rust
/// The same three outcomes the runtime evidence flag already proves: nobody
/// looked, it was collected, it was collected and failed.
#[test]
fn crypto_evidence_is_missing_collected_or_failed() {
    let id = suffix();
    let complete = store_root(&format!("{id}-complete"));
    write_key(&complete, "release-signing", "release", "v2", "active");
    write_version(&complete, "release-signing", "v1", "revoked");

    let thin = store_root(&format!("{id}-thin"));
    write_key(&thin, "release-signing", "release", "v1", "active");

    let (manifest_path, bundle_path) = release_pair(&id);

    let without = release_failures(&manifest_path, &bundle_path, None);
    assert!(without.contains(&"crypto_qualification_missing".to_owned()));

    let collected = release_failures(&manifest_path, &bundle_path, Some(&complete));
    assert!(
        !collected.contains(&"crypto_qualification_missing".to_owned())
            && !collected.contains(&"crypto_qualification_failed".to_owned()),
        "a complete store must satisfy crypto qualification: {collected:?}"
    );

    let thin_result = release_failures(&manifest_path, &bundle_path, Some(&thin));
    assert!(
        thin_result.contains(&"crypto_qualification_failed".to_owned()),
        "a store proving neither rotation nor lifecycle must fail: {thin_result:?}"
    );

    fs::remove_dir_all(&complete).ok();
    fs::remove_dir_all(&thin).ok();
}
```

Write `release_pair` to produce a manifest and a bundle bound to it (copy the fixture helpers from `crates/vestrace-cli/tests/v1_release_gate_cli.rs`), and `release_failures` to run `conformance release --json` with the flags below and return the `failures` array.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-cli --test crypto_evidence_cli crypto_evidence_is_missing`
Expected: FAIL with `unexpected argument '--crypto-evidence'`.

- [ ] **Step 3: Add the flags**

In `crates/vestrace-cli/src/main.rs`, add to the `Release` variant:

```rust
        /// Collect crypto adapter qualification from a mounted secret store.
        #[arg(long)]
        crypto_evidence: bool,
        #[arg(long)]
        key_store_root: Option<PathBuf>,
        #[arg(long)]
        key_id: Option<String>,
        #[arg(long, default_value = "v1")]
        key_version: String,
        #[arg(long, default_value = "release")]
        key_scope: String,
```

- [ ] **Step 4: Collect the evidence**

In `crates/vestrace-cli/src/commands/conformance.rs`:

```rust
/// Qualify the custody that holds the release signing key.
///
/// Like runtime evidence, asking for this and not getting it is an error: a
/// release whose key store could not be read has not been shown to have weak
/// custody, it has been shown to be unexamined, and those are different facts.
fn collect_crypto_qualification(
    key_store_root: Option<PathBuf>,
    key_id: Option<String>,
    key_version: &str,
    key_scope: &str,
) -> anyhow::Result<CryptoQualificationDecision> {
    let root = key_store_root.ok_or_else(|| {
        anyhow::anyhow!("crypto evidence could not be collected: --key-store-root is required")
    })?;
    let key_id = key_id.ok_or_else(|| {
        anyhow::anyhow!("crypto evidence could not be collected: --key-id is required")
    })?;
    let target = CryptoAdapterQualificationTarget::new(
        vestrace_infrastructure::crypto::MOUNTED_SECRET_STORE_PROVIDER,
        CryptoCustody::MountedSecretStore,
        key_id,
        key_version,
        "ed25519",
        KeyPurpose::Signing,
        key_scope,
    )
    .map_err(|error| anyhow::anyhow!("crypto evidence could not be collected: {error}"))?;
    let probe = vestrace_infrastructure::crypto::MountedStoreCryptoProbe::new(
        vestrace_infrastructure::crypto::MountedSecretStoreKeyProvider::new(root),
    );
    CryptoAdapterQualificationService::evaluate_with_probe(&target, &probe)
        .map_err(|error| anyhow::anyhow!("crypto evidence could not be collected: {error}"))
}
```

In `run_release`, add the parameters and replace the third `None` passed to `ExactEnvironmentReleaseEvidence::new` with:

```rust
    let crypto_qualification = if crypto_evidence {
        Some(collect_crypto_qualification(
            key_store_root,
            key_id,
            &key_version,
            &key_scope,
        )?)
    } else {
        None
    };
```

Update the comment above the evidence construction so it names the sources that remain absent — release approval, recovery qualification, fault suite and capability restoration — rather than claiming all of them are.

- [ ] **Step 5: Run tests**

Run: `cargo test -p vestrace-cli --test crypto_evidence_cli && cargo test -p vestrace-cli --test v1_release_gate_cli`
Expected: both PASS. The release gate's `the_release_gate_names_every_evidence_source_that_has_no_producer` case still expects `crypto_qualification_missing` when the flag is absent, so it must be unchanged.

- [ ] **Step 6: Full gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-cli --all-targets
cargo test -p vestrace-domain -p vestrace-application --all-targets
bash ./scripts/foundation-doc-truth.sh
bash ./scripts/foundation-boundary-truth.sh
bash ./scripts/foundation-cli-truth.sh
git add crates/vestrace-cli/src/main.rs crates/vestrace-cli/src/commands/conformance.rs \
        crates/vestrace-cli/tests/crypto_evidence_cli.rs
git commit -m "feat(cli): collect crypto qualification for the release gate"
```

---

### Task 5: Record the delta

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-19-custody-that-can-refuse.md`
- Modify: `docs/documentation-status-v0.2.md`

- [ ] **Step 1: Write the delta**

Follow the house format of `docs/documentation-gap-delta-2026-08-19-a-gate-nobody-could-run.md`: a header block with date, scope and non-claim; the problem; what makes this not a rename; the six checks and how each was obtained; the mutation proofs; and a **What this does not do** section covering, at minimum — filesystem properties are unchecked so a world-readable store satisfies this adapter; the mounted store is the weakest production custody and a KMS that signs internally is strictly stronger; the workspace secrets master key still has local-file custody; and the release gate still cannot pass because release approval, recovery qualification, fault suite and capability restoration remain without producers.

- [ ] **Step 2: Add the status paragraph**

Insert one paragraph into `docs/documentation-status-v0.2.md` immediately before the line beginning `The pinned implementation baseline remains`, in the style of the entries above it: dense, specific, with the counts from the final gate run and the same non-claims.

- [ ] **Step 3: Verify and commit**

```bash
bash ./scripts/foundation-doc-truth.sh
git add docs/
git diff --cached --check
git commit -m "docs: record the mounted secret store custody delta"
```

---

## Self-Review

**Spec coverage:** §4 store contract → Task 1. §4.1 resolution rules → Task 1 Steps 1 and 5, one case per table row. §5 probe and its six checks → Task 2. §5's two reasoning notes (rotation compares public keys, non-disclosure is scanned) → Task 2 Steps 1 and 7. §6 wiring → Tasks 3 and 4. §7 test strategy → Tasks 1, 2 and 4, including the three-outcome end-to-end. §8 non-goals → carried into Task 5's required content and the module documentation in Task 1.

**Placeholder scan:** two steps delegate fixture construction rather than showing it — Task 3 Step 1 (`manifest_paths`) and Task 4 Step 1 (`release_pair`, `release_failures`) — and both name the existing file to copy from (`q7_signed_artifacts_cli.rs`, `v1_release_gate_cli.rs`). This is deliberate: reproducing two 60-line fixtures already present in the repository would be a worse instruction than pointing at them.

**Type consistency:** `MountedSecretStoreKeyProvider::new`, `declaration`, `versions`, `public_key` are defined in Task 1 and used with those names and signatures in Task 2. `MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider)` is defined in Task 2 and used in Task 4. `MOUNTED_SECRET_STORE_PROVIDER` is defined in Task 1 and used in Tasks 2, 3 and 4. `KeyDeclaration`'s three public fields are written in Task 1 and read in Task 2.

**Known gap the plan itself carries:** Task 4 asserts a complete store makes crypto qualification pass, which requires the observed key to match the target on provider, algorithm, purpose, scope, key id and version. The probe builds the observed reference from the store's declaration, so a store whose declaration differs from the target produces mismatch failures rather than a pass. That is the intended behaviour and the test fixtures must therefore declare `scope=release`, `purpose=signing`, `algorithm=ed25519` to match the target built in Task 4.
