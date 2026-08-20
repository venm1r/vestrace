use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::{QualificationProfile, RequirementFamily, RequirementId};
use vestrace_domain::now;
use vestrace_domain::trust::QualificationBundle;

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

// The manifest/bundle fixtures below are the same shape as
// `v1_release_gate_cli.rs`'s, copied rather than shared for the same reason
// as the store helpers above.

fn release_manifest(profile: QualificationProfile) -> VestraceCapabilityManifest {
    VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release",
        vec!["schema-1"],
        vec![profile],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["mounted-secret-store"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["crypto custody is mounted-secret-store"],
    )
    .unwrap()
}

fn release_evidence() -> Vec<HardGateEvidence> {
    vec![HardGateEvidence::pass(
        RequirementId::new(RequirementFamily::Arc, 1),
        "crates/vestrace-domain/src/execution/mod.rs",
        None,
        EvidenceOrigin::LocalExecutable,
    )]
}

fn release_bundle(
    manifest: &VestraceCapabilityManifest,
    profile: QualificationProfile,
) -> QualificationBundle {
    QualificationBundle::new(
        profile,
        manifest.manifest_digest(),
        manifest.source_revision(),
        manifest.build_digest(),
        manifest.configuration_digest(),
        manifest.environment_manifest(),
        "suite-v1",
        release_evidence(),
        manifest.known_limitations().to_vec(),
        now(),
        Some(now()),
    )
    .unwrap()
}

/// A manifest and a bundle bound to it, ready for `conformance release`.
fn release_pair(id: &str) -> (PathBuf, PathBuf) {
    let manifest = release_manifest(QualificationProfile::Trusted);
    let bundle = release_bundle(&manifest, QualificationProfile::Trusted);
    let manifest_path =
        std::env::temp_dir().join(format!("vestrace-crypto-release-manifest-{id}.json"));
    let bundle_path =
        std::env::temp_dir().join(format!("vestrace-crypto-release-bundle-{id}.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(&bundle_path, serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();
    (manifest_path, bundle_path)
}

/// Runs `conformance release --json`, adding `--crypto-evidence` and the key
/// coordinates only when a store root is given, and returns the raw process
/// output so callers can inspect either the report or the failure.
fn run_release(manifest_path: &Path, bundle_path: &Path, key_store_root: Option<&Path>) -> Output {
    let mut args = vec![
        "conformance".to_owned(),
        "release".to_owned(),
        "--manifest-file".to_owned(),
        manifest_path.to_str().unwrap().to_owned(),
        "--bundle-file".to_owned(),
        bundle_path.to_str().unwrap().to_owned(),
        "--profile".to_owned(),
        "trusted".to_owned(),
        "--json".to_owned(),
    ];
    if let Some(root) = key_store_root {
        args.push("--crypto-evidence".to_owned());
        args.push("--key-store-root".to_owned());
        args.push(root.to_str().unwrap().to_owned());
        args.push("--key-id".to_owned());
        args.push("release-signing".to_owned());
        args.push("--key-scope".to_owned());
        args.push("release".to_owned());
    }
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(&args)
        .output()
        .unwrap()
}

/// Runs the release gate and returns the `failures` array of its JSON report.
fn release_failures(
    manifest_path: &Path,
    bundle_path: &Path,
    key_store_root: Option<&Path>,
) -> Vec<String> {
    let output = run_release(manifest_path, bundle_path, key_store_root);
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "release report is not JSON: {error}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
    report["failures"]
        .as_array()
        .expect("release report has no failures array")
        .iter()
        .map(|failure| failure.as_str().unwrap().to_owned())
        .collect()
}

/// The same three outcomes the runtime evidence flag already proves: nobody
/// looked, it was collected, it was collected and failed.
#[test]
fn crypto_evidence_is_missing_collected_or_failed() {
    let id = suffix();
    let complete = store_root(&format!("{id}-complete"));
    write_key(&complete, "release-signing", "release", "v1", "active");
    write_version(&complete, "release-signing", "v2", "revoked");

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
    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

/// Asking for crypto evidence and pointing it at a store that cannot be read
/// is an error, not silence: it must not be reported as `missing`, because
/// `missing` means nobody looked, and here somebody looked and failed.
#[test]
fn crypto_evidence_from_a_store_that_does_not_exist_is_an_error_not_a_silence() {
    let id = suffix();
    let (manifest_path, bundle_path) = release_pair(&id);
    let nonexistent = std::env::temp_dir().join(format!("vestrace-store-{id}-does-not-exist"));

    let output = run_release(&manifest_path, &bundle_path, Some(&nonexistent));

    assert!(
        !output.status.success(),
        "a store that cannot be read must not silently pass or report missing evidence"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("crypto evidence could not be collected"),
        "the operator must be told the store could not be read, got stderr: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "a release report that could not read the key store must not be published"
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}
