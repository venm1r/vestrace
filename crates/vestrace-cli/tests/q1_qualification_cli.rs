use std::fs;
use std::process::Command;

use serde_json::Value;

#[test]
fn conformance_bundle_writes_target_bound_json_before_failed_exit() {
    let output_path = std::env::temp_dir().join(format!(
        "vestrace-q1-qualification-{}.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "bundle",
            "--profile",
            "trusted",
            "--lifecycle",
            "release",
            "--output",
            output_path.to_str().expect("UTF-8 temp path"),
            "--target-manifest",
            "manifest://target",
            "--source-revision",
            "source-revision",
            "--build-digest",
            "sha256:build",
            "--configuration-digest",
            "sha256:config",
            "--environment-manifest",
            "environment://test",
            "--suite-version",
            "suite-v1",
            "--known-limitation",
            "provider wording is not qualified",
        ])
        .output()
        .expect("run conformance bundle");

    assert!(
        output_path.exists(),
        "bundle must be written before a failed qualification exit; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "current evaluator has skipped requirements and must not claim a pass"
    );

    let document: Value = serde_json::from_slice(&fs::read(&output_path).expect("read bundle"))
        .expect("parse qualification bundle JSON");
    assert_eq!(document["profile"], "trusted");
    assert_eq!(document["lifecycle"], "release");
    assert!(
        document["target_digest"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(document["conformance_report"].is_object());
    assert_eq!(
        document["known_limitations"][0],
        "provider wording is not qualified"
    );

    fs::remove_file(output_path).expect("remove temporary qualification bundle");
}
