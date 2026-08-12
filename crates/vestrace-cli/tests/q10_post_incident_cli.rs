use std::fs;
use std::process::Command;

use serde_json::json;

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

#[test]
fn post_incident_bundle_requires_typed_revalidation_evidence_file() {
    let id = suffix();
    let output = std::env::temp_dir().join(format!("vestrace-q10-bundle-{id}.json"));
    let result = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "bundle",
            "--profile",
            "trusted",
            "--lifecycle",
            "post-incident",
            "--target-manifest",
            "manifest",
            "--source-revision",
            "source",
            "--build-digest",
            "build",
            "--configuration-digest",
            "config",
            "--environment-manifest",
            "environment",
            "--suite-version",
            "suite",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("post-incident"));
    assert!(!output.exists());
}

#[test]
fn post_incident_bundle_rejects_failed_revalidation_before_writing_pass_claim() {
    let id = suffix();
    let evidence = std::env::temp_dir().join(format!("vestrace-q10-evidence-{id}.json"));
    let output = std::env::temp_dir().join(format!("vestrace-q10-bundle-{id}.json"));
    fs::write(
        &evidence,
        serde_json::to_vec_pretty(&json!({
            "incident_id": "00000000-0000-0000-0000-000000000001",
            "revalidation_run_id": "00000000-0000-0000-0000-000000000002",
            "revalidation_result": "failed",
            "evidence_refs": ["evidence:failed-revalidation"]
        }))
        .unwrap(),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "bundle",
            "--profile",
            "trusted",
            "--lifecycle",
            "post-incident",
            "--target-manifest",
            "manifest",
            "--source-revision",
            "source",
            "--build-digest",
            "build",
            "--configuration-digest",
            "config",
            "--environment-manifest",
            "environment",
            "--suite-version",
            "suite",
            "--post-incident-evidence-file",
            evidence.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("not passed"));
    let artifact =
        serde_json::from_slice::<serde_json::Value>(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(
        artifact["post_incident_evidence"]["revalidation_result"],
        "failed"
    );

    for path in [evidence, output] {
        let _ = fs::remove_file(path);
    }
}
