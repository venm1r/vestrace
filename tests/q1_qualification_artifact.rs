use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile, RequirementId,
};
use vestrace_domain::now;
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle, QualificationStatus};

fn report(profile: QualificationProfile, skipped: Option<RequirementId>) -> ConformanceReport {
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            let status = if skipped == Some(requirement_id) {
                CaseStatus::Skip
            } else {
                CaseStatus::Pass
            };
            ConformanceCaseResult {
                case_id: format!("q1-{requirement_id}"),
                requirement_ids: vec![requirement_id],
                status,
                message: "deterministic q1 test evidence".into(),
                evidence: Some(format!("test://q1/{requirement_id}")),
                // A fixture standing in for a run that happened.
                origin: vestrace_domain::conformance::CaseOrigin::Executed,
            }
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
                Some(format!("test://q1/{requirement_id}")),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}

#[test]
fn report_backed_bundle_preserves_target_identity_and_reports_passed() {
    let profile = QualificationProfile::Core;
    let bundle = QualificationBundle::from_conformance_report(
        QualificationLifecycle::Release,
        profile,
        "manifest://target",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        report(profile, None),
        evidence(profile),
        vec!["provider wording is not qualified".into()],
        now(),
        Some(now()),
    )
    .expect("valid report-backed bundle");

    assert_eq!(bundle.lifecycle(), QualificationLifecycle::Release);
    assert_eq!(bundle.status(), QualificationStatus::Passed);
    assert_eq!(
        bundle
            .conformance_report()
            .expect("report-backed bundle report")
            .profile,
        Some(profile)
    );
    assert!(!bundle.target_digest().is_empty());
    assert_eq!(
        bundle.known_limitations(),
        &["provider wording is not qualified"]
    );
}

#[test]
fn skipped_requirement_remains_a_failed_qualification_status() {
    let profile = QualificationProfile::Core;
    let skipped = profile_requirements(profile)[0];
    let bundle = QualificationBundle::from_conformance_report(
        QualificationLifecycle::PreMerge,
        profile,
        "manifest://target",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        report(profile, Some(skipped)),
        evidence(profile),
        Vec::new(),
        now(),
        Some(now()),
    )
    .expect("bundle should preserve failed evidence");

    assert_eq!(bundle.status(), QualificationStatus::Failed);
    assert_eq!(
        bundle
            .conformance_report()
            .expect("report-backed bundle report")
            .results
            .iter()
            .find(|result| result.requirement_ids.contains(&skipped))
            .expect("skipped result")
            .status,
        CaseStatus::Skip
    );
}

#[test]
fn report_profile_must_match_bundle_profile() {
    let result = QualificationBundle::from_conformance_report(
        QualificationLifecycle::Deployment,
        QualificationProfile::Trusted,
        "manifest://target",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        report(QualificationProfile::Core, None),
        evidence(QualificationProfile::Core),
        Vec::new(),
        now(),
        Some(now()),
    );

    assert!(result.is_err());
}

#[test]
fn duplicate_hard_gate_evidence_is_rejected() {
    let profile = QualificationProfile::Core;
    let mut duplicate_evidence = evidence(profile);
    duplicate_evidence.push(duplicate_evidence[0].clone());

    let result = QualificationBundle::from_conformance_report(
        QualificationLifecycle::Release,
        profile,
        "manifest://target",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        report(profile, None),
        duplicate_evidence,
        Vec::new(),
        now(),
        Some(now()),
    );

    assert!(result.is_err());
}

#[test]
fn legacy_bundle_without_report_is_incomplete() {
    let profile = QualificationProfile::Core;
    let bundle = QualificationBundle::new(
        profile,
        "manifest://target",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        "suite-v1",
        evidence(profile),
        Vec::new(),
        now(),
        None,
    )
    .expect("valid incomplete bundle");

    assert_eq!(bundle.status(), QualificationStatus::Incomplete);
    assert!(bundle.conformance_report().is_none());
}
