use std::fs;
use std::process::Command;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::Value;
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::now;
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle};

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

fn bundle() -> QualificationBundle {
    QualificationBundle::new(
        QualificationProfile::Core,
        "sha256:manifest",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        Vec::new(),
        Vec::new(),
        now(),
        Some(now()),
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

fn test_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn key_files(id: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let private = std::env::temp_dir().join(format!("vestrace-q7-private-{id}.der"));
    let public = std::env::temp_dir().join(format!("vestrace-q7-public-{id}.bin"));
    fs::write(&private, pkcs8.as_ref()).unwrap();
    fs::write(&public, pair.public_key().as_ref()).unwrap();
    (private, public)
}

fn run_sign(
    artifact: &str,
    input: &std::path::Path,
    private_key: &std::path::Path,
    output: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            artifact,
            "--artifact-file",
            input.to_str().unwrap(),
            "--private-key-file",
            private_key.to_str().unwrap(),
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
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn run_verify_signature(
    artifact: &str,
    input: &std::path::Path,
    public_key: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "verify-signature",
            "--artifact",
            artifact,
            "--artifact-file",
            input.to_str().unwrap(),
            "--public-key-file",
            public_key.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn sign_and_verify_manifest_and_bundle_with_ed25519() {
    let id = test_suffix();
    let (private, public) = key_files(&id);
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q7-manifest-{id}.json"));
    let signed_manifest_path =
        std::env::temp_dir().join(format!("vestrace-q7-signed-manifest-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-q7-bundle-{id}.json"));
    let signed_bundle_path =
        std::env::temp_dir().join(format!("vestrace-q7-signed-bundle-{id}.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest()).unwrap(),
    )
    .unwrap();
    fs::write(&bundle_path, serde_json::to_vec_pretty(&bundle()).unwrap()).unwrap();

    let sign_manifest = run_sign("manifest", &manifest_path, &private, &signed_manifest_path);
    assert!(
        sign_manifest.status.success(),
        "stdout={}; stderr={}",
        String::from_utf8_lossy(&sign_manifest.stdout),
        String::from_utf8_lossy(&sign_manifest.stderr)
    );
    let sign_bundle = run_sign("bundle", &bundle_path, &private, &signed_bundle_path);
    assert!(sign_bundle.status.success());

    for (kind, path) in [
        ("manifest", &signed_manifest_path),
        ("bundle", &signed_bundle_path),
    ] {
        let verified = run_verify_signature(kind, path, &public);
        assert!(
            verified.status.success(),
            "{kind}: stdout={}; stderr={}",
            String::from_utf8_lossy(&verified.stdout),
            String::from_utf8_lossy(&verified.stderr)
        );
        let result: Value = serde_json::from_slice(&verified.stdout).unwrap();
        assert_eq!(result["status"], "passed");
        assert_eq!(result["algorithm"], "ed25519");
    }

    for path in [
        manifest_path,
        signed_manifest_path,
        bundle_path,
        signed_bundle_path,
        private,
        public,
    ] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn verify_signature_rejects_tampered_artifact_and_wrong_public_key() {
    let id = test_suffix();
    let (private, public) = key_files(&id);
    let (wrong_private, wrong_public) = key_files(&format!("wrong-{id}"));
    let input = std::env::temp_dir().join(format!("vestrace-q7-tamper-input-{id}.json"));
    let signed = std::env::temp_dir().join(format!("vestrace-q7-tamper-signed-{id}.json"));
    fs::write(&input, serde_json::to_vec_pretty(&manifest()).unwrap()).unwrap();
    assert!(
        run_sign("manifest", &input, &private, &signed)
            .status
            .success()
    );

    let mut tampered: Value = serde_json::from_slice(&fs::read(&signed).unwrap()).unwrap();
    tampered["product_version"] = Value::String("9.9.9".into());
    fs::write(&signed, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    assert!(
        !run_verify_signature("manifest", &signed, &public)
            .status
            .success()
    );

    fs::write(&signed, serde_json::to_vec_pretty(&manifest()).unwrap()).unwrap();
    assert!(
        run_sign("manifest", &signed, &private, &signed)
            .status
            .success()
    );
    assert!(
        !run_verify_signature("manifest", &signed, &wrong_public)
            .status
            .success()
    );

    for path in [input, signed, private, public, wrong_private, wrong_public] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn deployment_verify_records_a_passed_signature_check_when_required() {
    let id = test_suffix();
    let (private, public) = key_files(&id);
    let manifest = manifest();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q7-deploy-manifest-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-q7-deploy-bundle-{id}.json"));
    let signed_bundle_path =
        std::env::temp_dir().join(format!("vestrace-q7-deploy-signed-{id}.json"));
    let result_path = std::env::temp_dir().join(format!("vestrace-q7-deploy-result-{id}.json"));
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
    assert!(
        run_sign("bundle", &bundle_path, &private, &signed_bundle_path)
            .status
            .success()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env(
            "VESTRACE_DATABASE__URL",
            "postgres://q7:q7@127.0.0.1:9/vestrace",
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
            signed_bundle_path.to_str().unwrap(),
            "--output",
            result_path.to_str().unwrap(),
            "--public-key-file",
            public.to_str().unwrap(),
            "--require-signature",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&fs::read(&result_path).unwrap_or_else(|error| {
        panic!(
            "deployment result missing ({error}); stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }))
    .unwrap();
    assert_eq!(result["checks"]["signature"], "passed");
    assert_eq!(result["status"], "failed");

    for path in [
        manifest_path,
        bundle_path,
        signed_bundle_path,
        result_path,
        private,
        public,
    ] {
        let _ = fs::remove_file(path);
    }
}

#[allow(dead_code)]
fn _lifecycle_is_exercised() {
    let _ = QualificationLifecycle::Release;
}
