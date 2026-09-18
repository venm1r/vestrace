use std::{fs, path::PathBuf, process::Command};

use vestrace_domain::conformance::{
    CaseOrigin, CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
    runner::profile_requirements,
};
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle};

fn bundle_file() -> PathBuf {
    let profile = QualificationProfile::Core;
    let started_at = vestrace_domain::now();
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("baseline-cli-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Pass,
            message: "passed".into(),
            evidence: Some(format!("test://{requirement_id}")),
            origin: CaseOrigin::Executed,
        })
        .collect();
    let bundle = QualificationBundle::from_conformance_report(
        QualificationLifecycle::Release,
        profile,
        "target-manifest",
        "source-revision",
        "sha256:build",
        "sha256:configuration",
        "environment-manifest",
        "suite-v1",
        ConformanceReport::from_results(Some(profile), results),
        Vec::new(),
        vec!["test fixture".to_owned()],
        started_at,
        Some(started_at),
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!(
        "vestrace-qualification-baseline-{}.json",
        std::process::id()
    ));
    fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    path
}

#[test]
fn publish_baseline_command_requires_a_bundle_file_and_profile() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["conformance", "publish-baseline", "--help"])
        .output()
        .expect("vestrace command should start");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("--bundle-file"), "{stdout}");
    assert!(stdout.contains("--profile"), "{stdout}");
}

#[test]
fn publish_baseline_command_rejects_a_missing_bundle_file_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["conformance", "publish-baseline", "--profile", "core"])
        .output()
        .expect("vestrace command should start");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("--bundle-file"), "{stderr}");
}

#[test]
fn publish_baseline_command_rejects_a_missing_profile_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "publish-baseline",
            "--bundle-file",
            "unused-bundle.json",
        ])
        .output()
        .expect("vestrace command should start");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("--profile"), "{stderr}");
}

#[test]
fn publish_baseline_refuses_a_requested_profile_that_differs_from_the_bundle() {
    let bundle_file = bundle_file();
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env_remove("VESTRACE_DATABASE__URL")
        .args([
            "conformance",
            "publish-baseline",
            "--bundle-file",
            bundle_file.to_str().expect("UTF-8 temp path"),
            "--profile",
            "trusted",
        ])
        .output()
        .expect("vestrace command should start");
    fs::remove_file(bundle_file).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("qualification bundle profile does not match requested profile"),
        "{stderr}"
    );
}
