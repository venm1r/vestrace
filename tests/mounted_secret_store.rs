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

    assert!(
        matches!(error, KeyProviderError::NotUsable),
        "got {error:?}"
    );

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

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A key id is an identifier, not a path.
#[test]
fn a_key_id_cannot_walk_out_of_the_store() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref("../../etc", "v1", "release"), &request("release"))
        .expect_err("a traversing key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

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

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

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

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

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

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

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

    // clippy::format_collect: a `fold` would be faster, but this is a one-shot
    // needle for an assertion, not a hot path.
    #[allow(clippy::format_collect)]
    let needle: String = private.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(
        !rendered.contains(&needle),
        "a refusal disclosed key material"
    );
    assert!(
        !rendered
            .as_bytes()
            .windows(private.len())
            .any(|w| w == private),
        "a refusal disclosed key material"
    );

    fs::remove_dir_all(&root).ok();
}
