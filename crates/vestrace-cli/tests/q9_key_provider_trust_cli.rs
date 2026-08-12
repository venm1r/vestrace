use std::fs;
use std::process::Command;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::Value;
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::now;
use vestrace_domain::trust::QualificationBundle;

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
        vec!["local-file"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
    )
    .unwrap()
}

fn bundle_for_manifest(manifest: &VestraceCapabilityManifest) -> QualificationBundle {
    QualificationBundle::new(
        QualificationProfile::Core,
        manifest.manifest_digest(),
        manifest.source_revision(),
        manifest.build_digest(),
        manifest.configuration_digest(),
        manifest.environment_manifest(),
        "suite-v1",
        Vec::new(),
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap()
}

fn key_files(id: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let private = std::env::temp_dir().join(format!("vestrace-q9-private-{id}.der"));
    let public = std::env::temp_dir().join(format!("vestrace-q9-public-{id}.bin"));
    fs::write(&private, pkcs8.as_ref()).unwrap();
    fs::write(&public, pair.public_key().as_ref()).unwrap();
    (private, public)
}

#[test]
fn sign_rejects_an_unimplemented_provider_instead_of_treating_it_as_local_file() {
    let id = suffix();
    let (private, _) = key_files(&id);
    let input = std::env::temp_dir().join(format!("vestrace-q9-input-{id}.json"));
    let output = std::env::temp_dir().join(format!("vestrace-q9-output-{id}.json"));
    fs::write(&input, serde_json::to_vec_pretty(&manifest()).unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            "manifest",
            "--artifact-file",
            input.to_str().unwrap(),
            "--private-key-file",
            private.to_str().unwrap(),
            "--signer-identity",
            "issuer://release",
            "--key-provider",
            "vault",
            "--key-id",
            "release-signing",
            "--key-version",
            "v1",
            "--key-scope",
            "release",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("provider"));
    for path in [input, output, private] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn verify_signature_reports_crypto_and_trust_status_separately() {
    let id = suffix();
    let (private, public) = key_files(&id);
    let input = std::env::temp_dir().join(format!("vestrace-q9-input-{id}.json"));
    let signed = std::env::temp_dir().join(format!("vestrace-q9-signed-{id}.json"));
    fs::write(&input, serde_json::to_vec_pretty(&manifest()).unwrap()).unwrap();

    let signed_result = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            "manifest",
            "--artifact-file",
            input.to_str().unwrap(),
            "--private-key-file",
            private.to_str().unwrap(),
            "--signer-identity",
            "issuer://release",
            "--key-provider",
            "local-file",
            "--key-id",
            "release-signing",
            "--key-version",
            "v1",
            "--key-scope",
            "release",
            "--output",
            signed.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(signed_result.status.success());

    let verified = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "verify-signature",
            "--artifact",
            "manifest",
            "--artifact-file",
            signed.to_str().unwrap(),
            "--public-key-file",
            public.to_str().unwrap(),
            "--trusted-signer",
            "issuer://release",
            "--trusted-key-provider",
            "local-file",
            "--trusted-key-id",
            "release-signing",
            "--trusted-key-version",
            "v1",
            "--trusted-key-scope",
            "release",
            "--require-trusted-signer",
        ])
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "stdout={}; stderr={}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let result: Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(result["cryptographic_status"], "passed");
    assert_eq!(result["trust_status"], "trusted");

    for path in [input, signed, private, public] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn deployment_verify_fails_when_signature_is_valid_but_signer_is_untrusted() {
    let id = suffix();
    let (private, public) = key_files(&id);
    let manifest = manifest();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q9-deploy-manifest-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-q9-deploy-bundle-{id}.json"));
    let signed_bundle = std::env::temp_dir().join(format!("vestrace-q9-deploy-signed-{id}.json"));
    let result_path = std::env::temp_dir().join(format!("vestrace-q9-deploy-result-{id}.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        &bundle_path,
        serde_json::to_vec_pretty(&bundle_for_manifest(&manifest)).unwrap(),
    )
    .unwrap();

    let sign = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            "bundle",
            "--artifact-file",
            bundle_path.to_str().unwrap(),
            "--private-key-file",
            private.to_str().unwrap(),
            "--signer-identity",
            "issuer://release",
            "--key-provider",
            "local-file",
            "--key-id",
            "release-signing",
            "--key-version",
            "v1",
            "--key-scope",
            "release",
            "--output",
            signed_bundle.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(sign.status.success());

    let verified = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env(
            "VESTRACE_DATABASE__URL",
            "postgres://q9:q9@127.0.0.1:9/vestrace",
        )
        .args([
            "conformance",
            "verify",
            "--profile",
            "core",
            "--lifecycle",
            "release",
            "--target-manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            signed_bundle.to_str().unwrap(),
            "--output",
            result_path.to_str().unwrap(),
            "--public-key-file",
            public.to_str().unwrap(),
            "--require-signature",
            "--trusted-signer",
            "issuer://unexpected",
            "--trusted-key-provider",
            "local-file",
            "--trusted-key-id",
            "release-signing",
            "--trusted-key-version",
            "v1",
            "--trusted-key-scope",
            "release",
            "--require-trusted-signer",
        ])
        .output()
        .unwrap();
    assert!(!verified.status.success());
    let result: Value = serde_json::from_slice(&fs::read(&result_path).unwrap()).unwrap();
    assert_eq!(result["checks"]["signature"], "passed");
    assert_eq!(result["checks"]["signer_trust"], "failed");
    assert_eq!(result["status"], "failed");

    for path in [
        manifest_path,
        bundle_path,
        signed_bundle,
        result_path,
        private,
        public,
    ] {
        let _ = fs::remove_file(path);
    }
}
