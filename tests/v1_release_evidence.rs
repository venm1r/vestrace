use vestrace_application::{
    CapabilityRestorationPolicy, CapabilityRestorationService, CryptoAdapterQualificationEvidence,
    CryptoAdapterQualificationService, CryptoAdapterQualificationTarget, CryptoCustody,
    CryptoQualificationCheck, ExactEnvironmentReleaseEvidence, ExactEnvironmentReleaseFailure,
    ExactEnvironmentReleaseTarget, ReleaseApprovalService, ReleaseSignatureEvidence,
    RestorationEvidence, RestorationStage, V1ReleaseEvidenceProbe, V1ReleaseEvidenceService,
};
use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::conformance::gate::GovernanceFederationGate;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation, evaluate_fault_suite};
use vestrace_domain::security::Capability;
use vestrace_domain::trust::{
    KeyPurpose, KeyReference, QualificationBaseline, QualificationBundle, QualificationLifecycle,
    RecoveryQualificationObservation, RecoveryTarget, SignatureAlgorithm, SignatureRecord,
    SignerTrustPolicy, SignerTrustRule, TrustState, TrustStateRecord,
    evaluate_recovery_qualification,
};
use vestrace_domain::{HealthScope, VestraceCapabilityManifest, WorkspaceId, now};

#[test]
fn unavailable_exact_environment_evidence_cannot_pass_v1_gate() {
    let target = ExactEnvironmentReleaseTarget::new(
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release",
        "sha256:manifest",
        vec!["schema-1"],
        QualificationProfile::Trusted,
    )
    .unwrap();

    let decision = V1ReleaseEvidenceService::evaluate(
        &target,
        &ExactEnvironmentReleaseEvidence::unavailable(),
    );

    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&ExactEnvironmentReleaseFailure::ReleaseApprovalMissing)
    );
    assert!(
        decision
            .failures()
            .contains(&ExactEnvironmentReleaseFailure::RuntimeQualificationMissing)
    );
    assert!(
        decision
            .failures()
            .contains(&ExactEnvironmentReleaseFailure::EvidenceReferencesMissing)
    );
}

fn manifest() -> VestraceCapabilityManifest {
    let manifest = VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release",
        vec!["schema-1"],
        vec![QualificationProfile::Trusted],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-key-provider"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["provider limitations are published"],
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
    let report = vestrace_domain::conformance::ConformanceReport::from_results(
        Some(profile),
        profile_requirements(profile)
            .into_iter()
            .map(
                |requirement_id| vestrace_domain::conformance::ConformanceCaseResult {
                    case_id: format!("release-{requirement_id}"),
                    requirement_ids: vec![requirement_id],
                    status: vestrace_domain::conformance::CaseStatus::Pass,
                    message: "release qualification passed".into(),
                    evidence: Some(format!("test://release/{requirement_id}")),
                    // A fixture standing in for a run that happened.
                    origin: vestrace_domain::conformance::CaseOrigin::Executed,
                },
            )
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
        vec!["provider limitations are published".into()],
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

fn approval() -> vestrace_application::ReleaseApprovalDecision {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    let scope = HealthScope::workspace(WorkspaceId::new());
    let trust = TrustStateRecord::new(scope.clone(), TrustState::Trusted, "qualified", None, now())
        .unwrap();
    ReleaseApprovalService::evaluate(
        &scope,
        &manifest,
        &bundle,
        &baseline,
        &trust,
        &signer_policy(),
        ReleaseSignatureEvidence::new(true, true),
    )
}

fn crypto_qualification() -> vestrace_application::CryptoQualificationDecision {
    let target = CryptoAdapterQualificationTarget::new(
        "kms",
        CryptoCustody::Kms,
        "release-signing",
        "v1",
        "ed25519",
        KeyPurpose::Signing,
        "release",
    )
    .unwrap();
    let key = KeyReference::new(
        "kms",
        "release-signing",
        "v1",
        KeyPurpose::Signing,
        "release",
        "ed25519",
    )
    .unwrap();
    CryptoAdapterQualificationService::evaluate(
        &target,
        &CryptoAdapterQualificationEvidence::new(
            key,
            [
                CryptoQualificationCheck::Resolution,
                CryptoQualificationCheck::ScopeIsolation,
                CryptoQualificationCheck::CryptographicRoundTrip,
                CryptoQualificationCheck::Lifecycle,
                CryptoQualificationCheck::Rotation,
                CryptoQualificationCheck::SecretNonDisclosure,
            ],
            vec!["evidence:crypto".into()],
        ),
    )
}

fn runtime_qualification(
    manifest: &VestraceCapabilityManifest,
    bundle: &QualificationBundle,
) -> vestrace_application::RuntimeQualificationDecision {
    vestrace_application::evaluate_runtime_qualification(
        vestrace_application::QualificationRuntime::Server,
        manifest,
        bundle,
        QualificationProfile::Trusted,
        QualificationLifecycle::Release,
        &vestrace_application::RuntimeQualificationEvidence {
            migration_history_compatible: true,
            runtime_role: "vestrace_runtime".into(),
            is_superuser: false,
            bypasses_rls: false,
            inherits_bootstrap: false,
        },
    )
}

fn recovery_qualification() -> vestrace_domain::trust::RecoveryQualificationDecision {
    evaluate_recovery_qualification(
        &RecoveryTarget::required_targets()
            .into_iter()
            .map(|target| RecoveryQualificationObservation::expected(target, "evidence:recovery"))
            .collect::<Vec<_>>(),
    )
}

fn fault_qualification() -> vestrace_domain::external_effects::FaultSuiteDecision {
    evaluate_fault_suite(
        &EffectFaultPoint::required_points()
            .into_iter()
            .map(FaultObservation::expected)
            .collect::<Vec<_>>(),
    )
}

fn capability_restoration() -> vestrace_application::CapabilityRestorationDecision {
    let policy = CapabilityRestorationPolicy::new([(
        Capability::MemoryRead,
        RestorationStage::DiagnosticsReadOnly,
    )])
    .unwrap();
    CapabilityRestorationService::evaluate(
        TrustState::Trusted,
        Capability::MemoryRead,
        &policy,
        &RestorationEvidence::new(false, false, Vec::new()),
    )
}

fn complete_evidence(
    manifest: &VestraceCapabilityManifest,
    bundle: &QualificationBundle,
) -> ExactEnvironmentReleaseEvidence {
    ExactEnvironmentReleaseEvidence::new(
        manifest.product_version(),
        manifest.source_revision(),
        manifest.build_digest(),
        manifest.configuration_digest(),
        manifest.environment_manifest(),
        manifest.manifest_digest(),
        manifest.schema_versions().to_vec(),
        QualificationProfile::Trusted,
        Some(approval()),
        Some(runtime_qualification(manifest, bundle)),
        Some(crypto_qualification()),
        Some(recovery_qualification()),
        Some(fault_qualification()),
        vec![capability_restoration()],
        vec!["evidence:exact-environment".into()],
        vec!["provider limitations are published".into()],
    )
}

struct FixtureProbe(ExactEnvironmentReleaseEvidence);

impl V1ReleaseEvidenceProbe for FixtureProbe {
    fn collect(
        &self,
        _target: &ExactEnvironmentReleaseTarget,
    ) -> Result<ExactEnvironmentReleaseEvidence, vestrace_application::ApplicationError> {
        Ok(self.0.clone())
    }
}

#[test]
fn exact_environment_gate_passes_only_after_all_typed_decisions_close() {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let target =
        ExactEnvironmentReleaseTarget::from_manifest(&manifest, QualificationProfile::Trusted)
            .unwrap();
    let decision =
        V1ReleaseEvidenceService::evaluate(&target, &complete_evidence(&manifest, &bundle));

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());
}

#[test]
fn exact_environment_gate_exposes_a_collection_probe_boundary() {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let target =
        ExactEnvironmentReleaseTarget::from_manifest(&manifest, QualificationProfile::Trusted)
            .unwrap();
    let decision = V1ReleaseEvidenceService::evaluate_with_probe(
        &target,
        &FixtureProbe(complete_evidence(&manifest, &bundle)),
    )
    .unwrap();

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());
}

#[test]
fn exact_environment_identity_drift_blocks_a_ready_dependency_set() {
    let manifest = manifest();
    let bundle = bundle(&manifest);
    let target =
        ExactEnvironmentReleaseTarget::from_manifest(&manifest, QualificationProfile::Trusted)
            .unwrap();
    let evidence = ExactEnvironmentReleaseEvidence::new(
        "1.0.0",
        "source-revision",
        "sha256:tampered-build",
        "sha256:config",
        "environment://release",
        manifest.manifest_digest(),
        vec!["schema-1".into()],
        QualificationProfile::Trusted,
        Some(approval()),
        Some(runtime_qualification(&manifest, &bundle)),
        Some(crypto_qualification()),
        Some(recovery_qualification()),
        Some(fault_qualification()),
        vec![capability_restoration()],
        vec!["evidence:exact-environment".into()],
        vec!["provider limitations are published".into()],
    );

    let decision = V1ReleaseEvidenceService::evaluate(&target, &evidence);

    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&ExactEnvironmentReleaseFailure::ReleaseIdentityMismatch)
    );
}
