use vestrace_application::{
    CryptoAdapterQualificationEvidence, CryptoAdapterQualificationProbe,
    CryptoAdapterQualificationService, CryptoAdapterQualificationTarget, CryptoCustody,
    CryptoQualificationCheck, CryptoQualificationDecision,
};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SignatureAlgorithm};

fn key(provider: &str, scope: &str) -> KeyReference {
    KeyReference::new(
        provider,
        "release-signing",
        "v1",
        KeyPurpose::Signing,
        scope,
        SignatureAlgorithm::Ed25519.as_str(),
    )
    .unwrap()
}

fn target() -> CryptoAdapterQualificationTarget {
    CryptoAdapterQualificationTarget::new(
        "kms",
        CryptoCustody::Kms,
        "release-signing",
        "v1",
        SignatureAlgorithm::Ed25519.as_str(),
        KeyPurpose::Signing,
        "release",
    )
    .unwrap()
}

fn complete_evidence() -> CryptoAdapterQualificationEvidence {
    CryptoAdapterQualificationEvidence::new(
        key("kms", "release"),
        [
            CryptoQualificationCheck::Resolution,
            CryptoQualificationCheck::ScopeIsolation,
            CryptoQualificationCheck::CryptographicRoundTrip,
            CryptoQualificationCheck::Lifecycle,
            CryptoQualificationCheck::Rotation,
            CryptoQualificationCheck::SecretNonDisclosure,
        ],
        vec![
            "evidence:key-resolution".into(),
            "evidence:scope-denial".into(),
            "evidence:sign-verify".into(),
            "evidence:key-lifecycle".into(),
            "evidence:key-rotation".into(),
            "evidence:non-disclosure".into(),
        ],
    )
}

struct FixtureProbe(CryptoAdapterQualificationEvidence);

impl CryptoAdapterQualificationProbe for FixtureProbe {
    fn collect(
        &self,
        _target: &CryptoAdapterQualificationTarget,
    ) -> Result<CryptoAdapterQualificationEvidence, vestrace_application::ApplicationError> {
        Ok(self.0.clone())
    }
}

#[test]
fn production_crypto_adapter_requires_complete_exact_evidence() {
    let decision = CryptoAdapterQualificationService::evaluate(&target(), &complete_evidence());

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());
}

#[test]
fn qualification_can_be_run_through_provider_probe_boundary() {
    let decision = CryptoAdapterQualificationService::evaluate_with_probe(
        &target(),
        &FixtureProbe(complete_evidence()),
    )
    .unwrap();

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());
}

#[test]
fn local_file_custody_is_not_accepted_as_production_qualification() {
    let target = CryptoAdapterQualificationTarget::new(
        "local-file",
        CryptoCustody::LocalDevelopment,
        "release-signing",
        "v1",
        SignatureAlgorithm::Ed25519.as_str(),
        KeyPurpose::Signing,
        "release",
    )
    .unwrap();

    let decision = CryptoAdapterQualificationService::evaluate(&target, &complete_evidence());

    assert!(!decision.is_passed());
}

#[test]
fn crypto_qualification_rejects_metadata_drift_and_incomplete_proofs() {
    let evidence = CryptoAdapterQualificationEvidence::new(
        key("kms", "other-scope"),
        [
            CryptoQualificationCheck::Resolution,
            CryptoQualificationCheck::CryptographicRoundTrip,
        ],
        Vec::new(),
    );
    let decision = CryptoAdapterQualificationService::evaluate(&target(), &evidence);

    assert!(matches!(decision, CryptoQualificationDecision { .. }));
    assert!(!decision.is_passed());
    assert!(decision.failures().len() >= 5);
}

#[test]
fn crypto_qualification_rejects_provider_custody_and_key_version_drift() {
    let local_provider_target = CryptoAdapterQualificationTarget::new(
        "local-file",
        CryptoCustody::Kms,
        "release-signing",
        "v1",
        SignatureAlgorithm::Ed25519.as_str(),
        KeyPurpose::Signing,
        "release",
    )
    .unwrap();
    let rotated_key = KeyReference::new(
        "kms",
        "other-key",
        "v2",
        KeyPurpose::Signing,
        "release",
        SignatureAlgorithm::Ed25519.as_str(),
    )
    .unwrap();
    let decision = CryptoAdapterQualificationService::evaluate(
        &local_provider_target,
        &CryptoAdapterQualificationEvidence::new(
            rotated_key,
            [
                CryptoQualificationCheck::Resolution,
                CryptoQualificationCheck::ScopeIsolation,
                CryptoQualificationCheck::CryptographicRoundTrip,
                CryptoQualificationCheck::Lifecycle,
                CryptoQualificationCheck::Rotation,
                CryptoQualificationCheck::SecretNonDisclosure,
            ],
            vec!["evidence:complete".into()],
        ),
    );

    assert!(!decision.is_passed());
    assert!(decision.failures().len() >= 3);
}

#[test]
fn crypto_qualification_rejects_blank_and_duplicate_evidence_refs() {
    let decision = CryptoAdapterQualificationService::evaluate(
        &target(),
        &CryptoAdapterQualificationEvidence::new(
            key("kms", "release"),
            [
                CryptoQualificationCheck::Resolution,
                CryptoQualificationCheck::ScopeIsolation,
                CryptoQualificationCheck::CryptographicRoundTrip,
                CryptoQualificationCheck::Lifecycle,
                CryptoQualificationCheck::Rotation,
                CryptoQualificationCheck::SecretNonDisclosure,
            ],
            vec![
                " ".into(),
                "evidence:duplicate".into(),
                "evidence:duplicate".into(),
            ],
        ),
    );

    assert!(!decision.is_passed());
}
