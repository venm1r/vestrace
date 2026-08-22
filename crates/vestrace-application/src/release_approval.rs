use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::trust::{
    QualificationBaseline, QualificationBundle, QualificationLifecycle, TrustState,
    TrustStateRecord, TrustedQualificationGate,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseSignatureEvidence {
    manifest_cryptographically_verified: bool,
    bundle_cryptographically_verified: bool,
}

impl ReleaseSignatureEvidence {
    pub fn new(
        manifest_cryptographically_verified: bool,
        bundle_cryptographically_verified: bool,
    ) -> Self {
        Self {
            manifest_cryptographically_verified,
            bundle_cryptographically_verified,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseApprovalFailure {
    ManifestInvalid,
    ManifestNotSigned,
    ManifestSignatureInvalid,
    ManifestSignatureNotCryptographicallyVerified,
    BundleLifecycleOrProfileMismatch,
    BundleManifestBindingInvalid,
    BundleNotPassed,
    BundleNotSigned,
    BundleSignatureInvalid,
    BundleSignatureNotCryptographicallyVerified,
    BaselineNotQualified,
    BaselineMismatch,
    TrustScopeMismatch,
    TrustStateNotTrusted,
    TrustedQualificationGateFailed,
    ManifestSignerNotTrusted,
    BundleSignerNotTrusted,
    KnownLimitationsMissing,
    KnownLimitationBlank,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseApprovalDecision {
    failures: Vec<ReleaseApprovalFailure>,
}

impl ReleaseApprovalDecision {
    /// A requested approval whose required persisted input could not be found
    /// is still an approval decision: it is explicitly non-approved, rather
    /// than an absent producer that the release gate would label `missing`.
    pub fn rejected(failure: ReleaseApprovalFailure) -> Self {
        Self {
            failures: vec![failure],
        }
    }

    pub fn is_approved(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[ReleaseApprovalFailure] {
        &self.failures
    }
}

pub struct ReleaseApprovalService;

impl ReleaseApprovalService {
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        expected_scope: &vestrace_domain::HealthScope,
        manifest: &VestraceCapabilityManifest,
        bundle: &QualificationBundle,
        baseline: &QualificationBaseline,
        trust_state: &TrustStateRecord,
        signer_policy: &vestrace_domain::trust::SignerTrustPolicy,
        signature_evidence: ReleaseSignatureEvidence,
    ) -> ReleaseApprovalDecision {
        let mut failures = Vec::new();

        if manifest.validate().is_err() {
            failures.push(ReleaseApprovalFailure::ManifestInvalid);
        }
        if manifest.signature().is_none() {
            failures.push(ReleaseApprovalFailure::ManifestNotSigned);
        } else if manifest.validate_signature().is_err() {
            failures.push(ReleaseApprovalFailure::ManifestSignatureInvalid);
        }
        if !signature_evidence.manifest_cryptographically_verified {
            failures.push(ReleaseApprovalFailure::ManifestSignatureNotCryptographicallyVerified);
        }

        if bundle.lifecycle() != QualificationLifecycle::Release
            || bundle.profile() != QualificationProfile::Trusted
        {
            failures.push(ReleaseApprovalFailure::BundleLifecycleOrProfileMismatch);
        }
        if bundle
            .validate_manifest_binding(
                manifest,
                QualificationProfile::Trusted,
                QualificationLifecycle::Release,
            )
            .is_err()
        {
            failures.push(ReleaseApprovalFailure::BundleManifestBindingInvalid);
        }
        if bundle.status() != vestrace_domain::trust::QualificationStatus::Passed {
            failures.push(ReleaseApprovalFailure::BundleNotPassed);
        }
        if bundle.signature().is_none() {
            failures.push(ReleaseApprovalFailure::BundleNotSigned);
        } else if bundle.validate_signature().is_err() {
            failures.push(ReleaseApprovalFailure::BundleSignatureInvalid);
        }
        if !signature_evidence.bundle_cryptographically_verified {
            failures.push(ReleaseApprovalFailure::BundleSignatureNotCryptographicallyVerified);
        }

        if baseline.state() != vestrace_domain::trust::QualificationBaselineState::Qualified {
            failures.push(ReleaseApprovalFailure::BaselineNotQualified);
        }
        if !baseline.matches_bundle(bundle) {
            failures.push(ReleaseApprovalFailure::BaselineMismatch);
        }

        if trust_state.scope() != expected_scope {
            failures.push(ReleaseApprovalFailure::TrustScopeMismatch);
        }
        if trust_state.state() != TrustState::Trusted {
            failures.push(ReleaseApprovalFailure::TrustStateNotTrusted);
        }
        if !TrustedQualificationGate::evaluate(bundle, baseline).is_passed() {
            failures.push(ReleaseApprovalFailure::TrustedQualificationGateFailed);
        }

        if let Some(signature) = manifest.signature() {
            if !signer_policy.evaluate(signature).is_trusted() {
                failures.push(ReleaseApprovalFailure::ManifestSignerNotTrusted);
            }
        }
        if let Some(signature) = bundle.signature() {
            if !signer_policy.evaluate(signature).is_trusted() {
                failures.push(ReleaseApprovalFailure::BundleSignerNotTrusted);
            }
        }

        if bundle.known_limitations().is_empty() {
            failures.push(ReleaseApprovalFailure::KnownLimitationsMissing);
        } else if bundle
            .known_limitations()
            .iter()
            .any(|limitation| limitation.trim().is_empty())
        {
            failures.push(ReleaseApprovalFailure::KnownLimitationBlank);
        }

        ReleaseApprovalDecision { failures }
    }
}
