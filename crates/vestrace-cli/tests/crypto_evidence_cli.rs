use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;

// Repeated deliberately from `tests/mounted_secret_store.rs`: this is a
// different crate's test binary and the helpers are not shared.

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

fn store_root(id: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vestrace-store-{id}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
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

// Same manifest fixture as `q7_signed_artifacts_cli.rs`, copied rather than
// shared for the same reason as the store helpers above.
fn manifest() -> VestraceCapabilityManifest {
    VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "0.2.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        vec!["schema-1"],
        vec![QualificationProfile::Core],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-key-provider"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["runtime qualification remains open"],
    )
    .unwrap()
}

/// Writes the shared manifest fixture to a temp file and returns the unsigned
/// input path alongside the path `conformance sign` should write the signed
/// artifact to.
fn manifest_paths(id: &str) -> (PathBuf, PathBuf) {
    let manifest_path = std::env::temp_dir().join(format!("vestrace-crypto-manifest-{id}.json"));
    let signed_path =
        std::env::temp_dir().join(format!("vestrace-crypto-signed-manifest-{id}.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest()).unwrap(),
    )
    .unwrap();
    (manifest_path, signed_path)
}

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

/// Exactly one key source may be given for the chosen provider: a command
/// that silently preferred one over the other would sign with a key its
/// operator did not choose, so both present at once must fail loudly rather
/// than pick one.
#[test]
fn giving_both_a_private_key_file_and_a_store_root_is_rejected() {
    let id = suffix();
    let root = store_root(&id);
    write_key(&root, "release-signing", "release", "v1", "active");
    let (manifest_path, signed_path) = manifest_paths(&id);
    let private_key_file = root
        .join("release-signing")
        .join("v1")
        .join("private.pkcs8");

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
            "--private-key-file",
            private_key_file.to_str().unwrap(),
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
        !output.status.success(),
        "signing unexpectedly succeeded with both key sources given"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exactly one"),
        "error did not name the conflict between the two key sources: {stderr}"
    );

    fs::remove_dir_all(&root).ok();
}
