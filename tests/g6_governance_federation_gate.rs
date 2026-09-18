use vestrace_domain::conformance::{
    QualificationProfile, RequirementFamily, RequirementId,
    gate::{
        EvidenceOrigin, GateEvidenceStatus, GovernanceFederationGate, HardGateEvidence,
        HardGateFailure,
    },
};

fn complete_evidence(profile: QualificationProfile) -> Vec<HardGateEvidence> {
    GovernanceFederationGate::required_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::pass(
                requirement_id,
                format!("evidence:g6:{requirement_id}"),
                Some("governance-policy-v1".into()),
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}

#[test]
fn g6_federation_gate_requires_the_complete_dependency_closure() {
    let profile = QualificationProfile::Federation;
    let evidence = complete_evidence(profile);
    let decision = GovernanceFederationGate::evaluate(profile, &evidence);

    assert!(decision.is_passed());
    assert!(
        GovernanceFederationGate::required_requirements(profile)
            .contains(&RequirementId::new(RequirementFamily::Arc, 1,))
    );
    assert!(
        GovernanceFederationGate::required_requirements(profile)
            .contains(&RequirementId::new(RequirementFamily::Idw, 6,))
    );
    assert!(
        GovernanceFederationGate::required_requirements(profile)
            .contains(&RequirementId::new(RequirementFamily::Gov, 20,))
    );
    assert!(
        GovernanceFederationGate::required_requirements(profile)
            .contains(&RequirementId::new(RequirementFamily::Qual, 4,))
    );
}

#[test]
fn g6_missing_dependency_or_hard_gate_evidence_denies_release() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    evidence.retain(|item| {
        item.requirement_id() != RequirementId::new(RequirementFamily::Mem, 1)
            && item.requirement_id() != RequirementId::new(RequirementFamily::Idw, 6)
    });

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::MissingEvidence {
                requirement_id: RequirementId::new(RequirementFamily::Mem, 1),
            })
    );
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::MissingEvidence {
                requirement_id: RequirementId::new(RequirementFamily::Idw, 6),
            })
    );
}

#[test]
fn g6_skipped_inconclusive_and_failed_security_evidence_are_hard_denies() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    let skipped_id = RequirementId::new(RequirementFamily::Idw, 7);
    let inconclusive_id = RequirementId::new(RequirementFamily::Gov, 20);
    let failed_id = RequirementId::new(RequirementFamily::Idw, 8);
    evidence.push(HardGateEvidence::new(
        skipped_id,
        GateEvidenceStatus::Skipped,
        Some("evidence:g6:skipped".into()),
        Some("governance-policy-v1".into()),
        EvidenceOrigin::LocalExecutable,
    ));
    evidence.push(HardGateEvidence::new(
        inconclusive_id,
        GateEvidenceStatus::Inconclusive,
        Some("evidence:g6:inconclusive".into()),
        Some("governance-policy-v1".into()),
        EvidenceOrigin::LocalExecutable,
    ));
    evidence.push(HardGateEvidence::new(
        failed_id,
        GateEvidenceStatus::Fail,
        Some("evidence:g6:failed".into()),
        Some("governance-policy-v1".into()),
        EvidenceOrigin::LocalExecutable,
    ));

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(decision.failures().contains(&HardGateFailure::Skipped {
        requirement_id: skipped_id,
    }));
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::Inconclusive {
                requirement_id: inconclusive_id,
            })
    );
    assert!(decision.failures().contains(&HardGateFailure::Failed {
        requirement_id: failed_id,
    }));
}

#[test]
fn g6_remote_self_assertion_cannot_satisfy_a_federation_requirement() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    let requirement_id = RequirementId::new(RequirementFamily::Idw, 7);
    evidence.push(HardGateEvidence::pass(
        requirement_id,
        "remote:self-asserted:trusted",
        Some("governance-policy-v1".into()),
        EvidenceOrigin::RemoteSelfAsserted,
    ));

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::RemoteSelfAssertion { requirement_id })
    );
}

#[test]
fn g6_exact_policy_version_evidence_is_required_for_governance_decisions() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    let requirement_id = RequirementId::new(RequirementFamily::Gov, 24);
    evidence.push(HardGateEvidence::pass(
        requirement_id,
        "evidence:g6:policy-version",
        None,
        EvidenceOrigin::LocalExecutable,
    ));

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::MissingPolicyVersion { requirement_id })
    );
}

#[test]
fn g6_blank_evidence_reference_and_policy_version_are_not_evidence() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    let evidence_id = RequirementId::new(RequirementFamily::Idw, 6);
    let policy_id = RequirementId::new(RequirementFamily::Gov, 24);
    evidence.push(HardGateEvidence::pass(
        evidence_id,
        "   ",
        Some("governance-policy-v1".into()),
        EvidenceOrigin::LocalExecutable,
    ));
    evidence.push(HardGateEvidence::pass(
        policy_id,
        "evidence:g6:blank-policy",
        Some("  ".into()),
        EvidenceOrigin::LocalExecutable,
    ));

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::MissingEvidenceReference {
                requirement_id: evidence_id,
            })
    );
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::MissingPolicyVersion {
                requirement_id: policy_id,
            })
    );
}

#[test]
fn g6_trusted_claim_cannot_be_widened_from_federation_evidence() {
    let federation_evidence = complete_evidence(QualificationProfile::Federation);
    let decision =
        GovernanceFederationGate::evaluate(QualificationProfile::Trusted, &federation_evidence);
    assert!(!decision.is_passed());
    assert!(decision.failures().iter().any(|failure| matches!(
        failure,
        HardGateFailure::MissingEvidence { requirement_id }
            if requirement_id.family == RequirementFamily::Rec
    )));
}

#[test]
fn g6_duplicate_requirement_evidence_is_rejected_as_ambiguous() {
    let profile = QualificationProfile::Federation;
    let mut evidence = complete_evidence(profile);
    let requirement_id = RequirementId::new(RequirementFamily::Idw, 6);
    evidence.push(HardGateEvidence::pass(
        requirement_id,
        "evidence:g6:duplicate",
        Some("governance-policy-v1".into()),
        EvidenceOrigin::LocalExecutable,
    ));

    let decision = GovernanceFederationGate::evaluate(profile, &evidence);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&HardGateFailure::DuplicateEvidence { requirement_id })
    );
}

#[test]
fn g6_fixture_scope_is_machine_readable_and_does_not_claim_more_than_the_gate() {
    let required =
        GovernanceFederationGate::required_requirements(QualificationProfile::Federation);
    assert!(required.contains(&RequirementId::new(RequirementFamily::Qual, 6)));
    assert!(!required.contains(&RequirementId::new(RequirementFamily::Rec, 1)));
}
