use vestrace_application::{
    ReleaseApprovalDecision, ReleaseApprovalService, ReleaseSignatureEvidence,
};
use vestrace_domain::conformance::gate::GovernanceFederationGate;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
};
use vestrace_domain::trust::{
    KeyPurpose, KeyReference, QualificationBaseline, QualificationBundle, QualificationLifecycle,
    SignatureAlgorithm, SignatureRecord, SignerTrustPolicy, SignerTrustRule, TrustState,
    TrustStateRecord,
};
use vestrace_domain::{HealthScope, VestraceCapabilityManifest, WorkspaceId, now};

fn manifest() -> VestraceCapabilityManifest {
    let manifest = VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release-test",
        vec!["schema-1"],
        vec![QualificationProfile::Trusted],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-key-provider"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["provider-specific wording remains bounded"],
    )
    .unwrap();
    let signature = SignatureRecord::new(
        manifest.unsigned_signing_digest().unwrap(),
        "issuer://release",
        signing_key(),
        SignatureAlgorithm::Ed25519,
        "manifest-signature",
        now(),
    )
    .unwrap();
    manifest.attach_signature(signature).unwrap()
}

fn signing_key() -> KeyReference {
    KeyReference::new(
        "local-file",
        "release-signing",
        "v1",
        KeyPurpose::Signing,
        "release",
        "ed25519",
    )
    .unwrap()
}

fn signer_policy() -> SignerTrustPolicy {
    SignerTrustPolicy::new(vec![
        SignerTrustRule::new(
            "issuer://release",
            "local-file",
            "release-signing",
            "v1",
            "release",
            SignatureAlgorithm::Ed25519,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn bundle(manifest: &VestraceCapabilityManifest) -> QualificationBundle {
    let profile = QualificationProfile::Trusted;
    let report = ConformanceReport::from_results(
        Some(profile),
        profile_requirements(profile)
            .into_iter()
            .map(|requirement_id| ConformanceCaseResult {
                case_id: format!("release-{requirement_id}"),
                requirement_ids: vec![requirement_id],
                status: CaseStatus::Pass,
                message: "release qualification passed".into(),
                evidence: Some(format!("test://release/{requirement_id}")),
            })
            .collect(),
    );
    let evidence = GovernanceFederationGate::required_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::pass(
                requirement_id,
                format!("test://release/evidence/{requirement_id}"),
                Some("policy-v1".into()),
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect();
    let bundle = QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::Release,
        profile,
        manifest,
        "suite-v1",
        report,
        evidence,
        vec!["provider-specific wording remains bounded".into()],
        now(),
        Some(now()),
    )
    .unwrap();
    let signature = SignatureRecord::new(
        bundle.unsigned_signing_digest().unwrap(),
        "issuer://release",
        signing_key(),
        SignatureAlgorithm::Ed25519,
        "bundle-signature",
        now(),
    )
    .unwrap();
    bundle.attach_signature(signature).unwrap()
}

fn scope() -> HealthScope {
    HealthScope::workspace(WorkspaceId::new())
}

fn trusted_state(scope: HealthScope) -> TrustStateRecord {
    TrustStateRecord::new(scope, TrustState::Trusted, "qualified release", None, now()).unwrap()
}

fn approval() -> ReleaseApprovalDecision {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    let scope = scope();
    ReleaseApprovalService::evaluate(
        &scope,
        &manifest,
        &bundle,
        &baseline,
        &trusted_state(scope.clone()),
        &signer_policy(),
        ReleaseSignatureEvidence::new(true, true),
    )
}

#[test]
fn trusted_release_is_approved_only_after_all_evidence_closes() {
    let decision = approval();

    assert!(
        decision.is_approved(),
        "failures: {:?}",
        decision.failures()
    );
    assert!(decision.failures().is_empty());
}

#[test]
fn release_approval_rejects_untrusted_state_and_unsigned_artifacts() {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    let untrusted = TrustStateRecord::new(
        scope(),
        TrustState::DegradedTrust,
        "incident remains open",
        None,
        now(),
    )
    .unwrap();
    let unsigned_manifest = VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release-test",
        vec!["schema-1"],
        vec![QualificationProfile::Trusted],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-key-provider"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["provider-specific wording remains bounded"],
    )
    .unwrap();

    let decision = ReleaseApprovalService::evaluate(
        &scope(),
        &unsigned_manifest,
        &bundle,
        &baseline,
        &untrusted,
        &signer_policy(),
        ReleaseSignatureEvidence::new(false, false),
    );

    assert!(!decision.is_approved());
    assert!(decision.failures().len() >= 2);
}

#[test]
fn release_approval_rejects_scope_drift_and_blank_limitations() {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    let expected_scope = scope();
    let decision = ReleaseApprovalService::evaluate(
        &expected_scope,
        &manifest,
        &bundle,
        &baseline,
        &trusted_state(scope()),
        &signer_policy(),
        ReleaseSignatureEvidence::new(true, true),
    );

    assert!(!decision.is_approved());
    assert!(!decision.failures().is_empty());
}
