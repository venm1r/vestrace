use std::fs;
use std::process::Command;

#[test]
fn persist_flag_keeps_local_artifact_when_database_configuration_fails() {
    let output_path = std::env::temp_dir().join(format!(
        "vestrace-q3-qualification-{}.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env_remove("VESTRACE__DATABASE__URL")
        .args([
            "conformance",
            "bundle",
            "--profile",
            "trusted",
            "--persist",
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
        ])
        .output()
        .expect("run persisted conformance bundle");

    assert!(
        output_path.exists(),
        "artifact must exist even when persistence setup fails; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("database") || stderr.contains("configuration"));
    assert!(stderr.contains(output_path.to_str().expect("UTF-8 temp path")));

    fs::remove_file(output_path).expect("remove temporary qualification bundle");
}
