use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::trust::{
    PostIncidentQualificationEvidence, QualificationBundle, RecoveryAction,
    RecoveryQualificationObservation, RecoveryTarget, evaluate_recovery_qualification,
};
use vestrace_domain::{IncidentId, RevalidationResult, RevalidationRunId, now};

#[test]
fn post_incident_evidence_is_typed_and_requires_proof_refs() {
    let incident_id = IncidentId::new();
    let revalidation_run_id = RevalidationRunId::new();
    let evidence = PostIncidentQualificationEvidence::new(
        incident_id,
        revalidation_run_id,
        RevalidationResult::Passed,
        vec!["evidence:revalidation".into()],
    )
    .unwrap();

    assert!(evidence.is_successful());
    assert!(
        PostIncidentQualificationEvidence::new(
            incident_id,
            revalidation_run_id,
            RevalidationResult::Passed,
            Vec::new(),
        )
        .is_err()
    );

    let bundle = QualificationBundle::new(
        QualificationProfile::Trusted,
        "manifest",
        "source",
        "build",
        "config",
        "environment",
        "suite",
        Vec::new(),
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap();
    assert!(bundle.attach_post_incident_evidence(evidence).is_err());
}

#[test]
fn recovery_qualification_requires_one_safe_observation_per_target() {
    let observations = RecoveryTarget::required_targets()
        .into_iter()
        .map(|target| {
            RecoveryQualificationObservation::expected(target, format!("evidence:{target:?}"))
        })
        .collect::<Vec<_>>();

    let decision = evaluate_recovery_qualification(&observations);
    assert!(decision.is_passed(), "failures={:?}", decision.failures());

    let mut unsafe_retry = observations.clone();
    unsafe_retry[0].action = RecoveryAction::Retry;
    let decision = evaluate_recovery_qualification(&unsafe_retry);
    assert!(!decision.is_passed());
}
