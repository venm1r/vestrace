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

fn report(profile: QualificationProfile, status: CaseStatus) -> ConformanceReport {
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("q6-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status,
            message: "deterministic q6 test evidence".into(),
            evidence: Some(format!("test://q6/{requirement_id}")),
            // A fixture standing in for a run that happened.
            origin: vestrace_domain::conformance::CaseOrigin::Executed,
        })
        .collect();
    ConformanceReport::from_results(Some(profile), results)
}

fn evidence(profile: QualificationProfile, status: GateEvidenceStatus) -> Vec<HardGateEvidence> {
    profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::new(
                requirement_id,
                status,
                Some(format!("test://q6/{requirement_id}")),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}

fn bundle(profile: QualificationProfile, lifecycle: QualificationLifecycle) -> QualificationBundle {
    QualificationBundle::from_conformance_report_for_manifest(
        lifecycle,
        profile,
        &manifest(),
        "suite-v1",
        report(profile, CaseStatus::Pass),
        evidence(profile, GateEvidenceStatus::Pass),
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap()
}

#[test]
fn deployment_verifier_accepts_an_exact_manifest_bound_passed_bundle() {
    let bundle = bundle(
        QualificationProfile::Core,
        QualificationLifecycle::Deployment,
    );

    bundle
        .validate_manifest_binding(
            &manifest(),
            QualificationProfile::Core,
            QualificationLifecycle::Deployment,
        )
        .expect("exact target identity must verify");
}

#[test]
fn deployment_verifier_rejects_tampered_identity_profile_lifecycle_and_failed_status() {
    let mut tampered = serde_json::to_value(bundle(
        QualificationProfile::Core,
        QualificationLifecycle::Deployment,
    ))
    .unwrap();
    tampered["build_digest"] = serde_json::Value::String("sha256:tampered".into());
    let tampered_bundle: QualificationBundle = serde_json::from_value(tampered).unwrap();
    assert!(
        tampered_bundle
            .validate_manifest_binding(
                &manifest(),
                QualificationProfile::Core,
                QualificationLifecycle::Deployment,
            )
            .is_err()
    );

    let exact = bundle(
        QualificationProfile::Core,
        QualificationLifecycle::Deployment,
    );
    assert!(
        exact
            .validate_manifest_binding(
                &manifest(),
                QualificationProfile::Memory,
                QualificationLifecycle::Deployment,
            )
            .is_err()
    );
    assert!(
        exact
            .validate_manifest_binding(
                &manifest(),
                QualificationProfile::Core,
                QualificationLifecycle::Release,
            )
            .is_err()
    );

    let failed = QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::Deployment,
        QualificationProfile::Core,
        &manifest(),
        "suite-v1",
        report(QualificationProfile::Core, CaseStatus::Skip),
        evidence(QualificationProfile::Core, GateEvidenceStatus::Skipped),
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap();
    assert!(
        failed
            .validate_manifest_binding(
                &manifest(),
                QualificationProfile::Core,
                QualificationLifecycle::Deployment,
            )
            .is_err()
    );
}
