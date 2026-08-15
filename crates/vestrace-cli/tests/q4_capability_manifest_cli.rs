use std::fs;
use std::process::Command;

#[test]
fn manifest_command_writes_machine_readable_capability_artifact_without_database() {
    let output_path = std::env::temp_dir().join(format!(
        "vestrace-q4-capability-manifest-{}.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env_remove("VESTRACE__DATABASE__URL")
        .args([
            "conformance",
            "manifest",
            "--manifest-version",
            "manifest-v1",
            "--product",
            "vestrace",
            "--product-version",
            "0.2.0",
            "--source-revision",
            "revision-a",
            "--build-digest",
            "sha256:build",
            "--configuration-digest",
            "sha256:config",
            "--environment-manifest",
            "environment://test",
            "--schema-version",
            "schema-1",
            "--schema-version",
            "schema-2",
            "--profile",
            "core",
            "--profile",
            "trusted",
            "--storage-backend",
            "postgres-17",
            "--crypto-provider",
            "local-key-provider",
            "--known-limitation",
            "deployment qualification remains open",
            "--output",
            output_path.to_str().expect("UTF-8 temp path"),
        ])
        .output()
        .expect("run capability manifest command");

    assert!(
        output.status.success(),
        "manifest command must not require database configuration; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&output_path).unwrap())
        .expect("parse capability manifest JSON");
    assert_eq!(document["product"], "vestrace");
    assert_eq!(document["supported_profiles"][0], "core");
    assert_eq!(document["supported_profiles"][1], "trusted");
    assert!(
        document["manifest_digest"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:"))
    );

    fs::remove_file(output_path).expect("remove temporary capability manifest");
}
