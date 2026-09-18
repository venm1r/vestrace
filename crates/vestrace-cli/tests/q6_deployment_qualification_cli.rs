use std::fs;
use std::process::Command;

use serde_json::Value;
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
};
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

fn failed_bundle() -> QualificationBundle {
    let profile = QualificationProfile::Core;
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("q6-cli-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Skip,
            message: "runtime check remains outside the conformance fixture".into(),
            evidence: Some(format!("test://q6-cli/{requirement_id}")),
            // A fixture standing in for a run that happened.
            origin: vestrace_domain::conformance::CaseOrigin::Executed,
        })
        .collect();
    let evidence = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::new(
                requirement_id,
                GateEvidenceStatus::Skipped,
                Some(format!("test://q6-cli/{requirement_id}")),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect();

    QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::Deployment,
        profile,
        &manifest(),
        "suite-v1",
        ConformanceReport::from_results(Some(profile), results),
        evidence,
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

#[test]
fn verify_writes_machine_readable_failed_evidence_before_a_nonzero_exit() {
    let id = test_suffix();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q6-manifest-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-q6-bundle-{id}.json"));
    let output_path = std::env::temp_dir().join(format!("vestrace-q6-result-{id}.json"));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest()).unwrap(),
    )
    .unwrap();
    fs::write(
        &bundle_path,
        serde_json::to_vec_pretty(&failed_bundle()).unwrap(),
    )
    .unwrap();
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env(
            "VESTRACE_DATABASE__URL",
            "postgres://q6:q6@127.0.0.1:9/vestrace",
        )
        .args([
            "conformance",
            "verify",
            "--profile",
            "core",
            "--lifecycle",
            "deployment",
            "--target-manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run deployment verifier");

    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&fs::read(&output_path).unwrap_or_else(|error| {
        panic!(
            "verification result missing ({error}); stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }))
    .unwrap();
    assert_eq!(result["profile"], "core");
    assert_eq!(result["lifecycle"], "deployment");
    assert_eq!(result["status"], "failed");
    assert_eq!(result["checks"]["bundle_status"], "failed");
    assert_eq!(result["checks"]["runtime_database"], "failed");

    let _ = fs::remove_file(manifest_path);
    let _ = fs::remove_file(bundle_path);
    fs::remove_file(output_path).unwrap();
}

#[test]
fn verify_rejects_a_tampered_manifest_before_writing_evidence() {
    let id = test_suffix();
    let manifest_path = std::env::temp_dir().join(format!("vestrace-q6-tampered-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-q6-tampered-bundle-{id}.json"));
    let output_path = std::env::temp_dir().join(format!("vestrace-q6-tampered-result-{id}.json"));
    let mut document = serde_json::to_value(manifest()).unwrap();
    document["manifest_digest"] = Value::String("sha256:tampered".into());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();
    fs::write(
        &bundle_path,
        serde_json::to_vec_pretty(&failed_bundle()).unwrap(),
    )
    .unwrap();
    let _ = fs::remove_file(&output_path);

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "verify",
            "--profile",
            "core",
            "--lifecycle",
            "deployment",
            "--target-manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run tampered deployment verifier");

    assert!(!output.status.success());
    assert!(!output_path.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("digest"));

    let _ = fs::remove_file(manifest_path);
    let _ = fs::remove_file(bundle_path);
}
