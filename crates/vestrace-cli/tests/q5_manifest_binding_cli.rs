use std::fs;
use std::process::Command;

use serde_json::Value;
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;

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

#[test]
fn bundle_command_binds_a_capability_manifest_file() {
    let id = test_suffix();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q5-manifest-{id}.json"));
    let output_path = std::env::temp_dir().join(format!("vestrace-q5-bundle-{id}.json"));
    let manifest = manifest();
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "bundle",
            "--profile",
            "core",
            "--target-manifest-file",
            manifest_path.to_str().unwrap(),
            "--suite-version",
            "suite-v1",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run manifest-bound bundle");

    // This asserted failure while CORE still had unregistered requirements, so
    // the bundle could not qualify. CORE now closes — nine executed cases and
    // nineteen attestations, zero skips — and the command succeeds. The
    // assertion is inverted rather than deleted: it is what would catch CORE
    // regressing back into an unqualifiable state.
    assert!(
        output.status.success(),
        "the CORE profile no longer has unregistered requirements, so bundling must \
         succeed; stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(&fs::read(&output_path).unwrap_or_else(|error| {
        panic!(
            "bundle artifact missing ({error}); stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }))
    .unwrap();
    assert_eq!(document["target_manifest"], manifest.manifest_digest());
    assert_eq!(document["source_revision"], "source-revision");
    assert_eq!(document["configuration_digest"], "sha256:config");

    let _ = fs::remove_file(manifest_path);
    fs::remove_file(output_path).unwrap();
}

#[test]
fn bundle_command_rejects_a_tampered_capability_manifest_before_writing_bundle() {
    let id = test_suffix();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q5-tampered-{id}.json"));
    let output_path = std::env::temp_dir().join(format!("vestrace-q5-tampered-bundle-{id}.json"));
    let mut document = serde_json::to_value(manifest()).unwrap();
    document["manifest_digest"] = Value::String("sha256:tampered".into());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "bundle",
            "--profile",
            "core",
            "--target-manifest-file",
            manifest_path.to_str().unwrap(),
            "--suite-version",
            "suite-v1",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run tampered manifest bundle");

    assert!(!output.status.success());
    assert!(
        !output_path.exists(),
        "tampered manifest must fail before artifact emission"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("digest"),
        "stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_file(manifest_path).unwrap();
}
