use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
};
use vestrace_domain::now;
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle, QualificationStatus};

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

fn report(profile: QualificationProfile) -> ConformanceReport {
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("q5-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Pass,
            message: "deterministic q5 test evidence".into(),
            evidence: Some(format!("test://q5/{requirement_id}")),
            // A fixture standing in for a run that happened.
            origin: vestrace_domain::conformance::CaseOrigin::Executed,
        })
        .collect();
    ConformanceReport::from_results(Some(profile), results)
}

fn evidence(profile: QualificationProfile) -> Vec<HardGateEvidence> {
    profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::new(
                requirement_id,
                GateEvidenceStatus::Pass,
                Some(format!("test://q5/{requirement_id}")),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}

#[test]
fn manifest_bound_bundle_uses_exact_manifest_identity() {
    let profile = QualificationProfile::Core;
    let manifest = manifest();
    let bundle = QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::Deployment,
        profile,
        &manifest,
        "suite-v1",
        report(profile),
        evidence(profile),
        Vec::new(),
        now(),
        Some(now()),
    )
    .expect("manifest-bound bundle");

    assert_eq!(bundle.status(), QualificationStatus::Passed);
    assert_eq!(bundle.target_manifest(), manifest.manifest_digest());
    let document = serde_json::to_value(&bundle).unwrap();
    assert_eq!(document["source_revision"], "source-revision");
    assert_eq!(document["build_digest"], "sha256:build");
    assert_eq!(document["configuration_digest"], "sha256:config");
    assert_eq!(document["environment_manifest"], "environment://test");
}

#[test]
fn manifest_bound_bundle_rejects_an_unsupported_profile() {
    let profile = QualificationProfile::Memory;
    let manifest = manifest();
    let error = QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::Deployment,
        profile,
        &manifest,
        "suite-v1",
        report(profile),
        evidence(profile),
        Vec::new(),
        now(),
        Some(now()),
    )
    .expect_err("manifest must not claim an unsupported profile");

    assert!(error.to_string().contains("supported profiles"));
}
