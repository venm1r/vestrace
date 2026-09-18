use std::collections::HashSet;

use vestrace_domain::trust::{KeyPurpose, KeyReference};

use crate::ApplicationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CryptoCustody {
    LocalDevelopment,
    OsKeyring,
    Kms,
    Hsm,
    Vault,
    MountedSecretStore,
}

impl CryptoCustody {
    fn is_production(self) -> bool {
        !matches!(self, Self::LocalDevelopment)
    }

    fn accepts_provider(self, provider: &str) -> bool {
        match self {
            Self::LocalDevelopment => provider == "local-file",
            _ => provider != "local-file",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CryptoAdapterQualificationTarget {
    provider: String,
    custody: CryptoCustody,
    key_id: String,
    key_version: String,
    algorithm_suite: String,
    purpose: KeyPurpose,
    scope: String,
}

impl CryptoAdapterQualificationTarget {
    pub fn new(
        provider: impl Into<String>,
        custody: CryptoCustody,
        key_id: impl Into<String>,
        key_version: impl Into<String>,
        algorithm_suite: impl Into<String>,
        purpose: KeyPurpose,
        scope: impl Into<String>,
    ) -> Result<Self, ApplicationError> {
        let provider = non_blank("crypto provider", provider.into())?;
        let key_id = non_blank("crypto key id", key_id.into())?;
        let key_version = non_blank("crypto key version", key_version.into())?;
        let algorithm_suite = non_blank("crypto algorithm suite", algorithm_suite.into())?;
        let scope = non_blank("crypto key scope", scope.into())?;
        Ok(Self {
            provider,
            custody,
            key_id,
            key_version,
            algorithm_suite,
            purpose,
            scope,
        })
    }

    /// The key custody arrangement being qualified, in full.
    ///
    /// A crypto qualification that cannot say which provider, key, version and
    /// algorithm it examined is an assertion about nothing in particular.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn custody(&self) -> CryptoCustody {
        self.custody
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key_version(&self) -> &str {
        &self.key_version
    }

    pub fn algorithm_suite(&self) -> &str {
        &self.algorithm_suite
    }

    pub fn purpose(&self) -> KeyPurpose {
        self.purpose
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CryptoAdapterQualificationEvidence {
    observed_key: KeyReference,
    passed_checks: HashSet<CryptoQualificationCheck>,
    evidence_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CryptoQualificationCheck {
    Resolution,
    ScopeIsolation,
    CryptographicRoundTrip,
    Lifecycle,
    Rotation,
    SecretNonDisclosure,
}

impl CryptoAdapterQualificationEvidence {
    pub fn new(
        observed_key: KeyReference,
        passed_checks: impl IntoIterator<Item = CryptoQualificationCheck>,
        evidence_refs: Vec<String>,
    ) -> Self {
        Self {
            observed_key,
            passed_checks: passed_checks.into_iter().collect(),
            evidence_refs,
        }
    }

    pub fn observed_key(&self) -> &KeyReference {
        &self.observed_key
    }

    /// Which checks actually passed, rather than only whether all of them did.
    pub fn passed_checks(&self) -> &HashSet<CryptoQualificationCheck> {
        &self.passed_checks
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CryptoQualificationFailure {
    ProductionCustodyRequired,
    ProviderMismatch,
    AlgorithmMismatch,
    PurposeMismatch,
    ScopeMismatch,
    CustodyProviderMismatch,
    KeyIdMismatch,
    KeyVersionMismatch,
    KeyNotUsable,
    ResolutionNotProven,
    ScopeIsolationNotProven,
    CryptographicRoundTripNotProven,
    LifecycleNotProven,
    RotationNotProven,
    SecretNonDisclosureNotProven,
    EvidenceReferencesMissing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CryptoQualificationDecision {
    failures: Vec<CryptoQualificationFailure>,
    observed_key: KeyReference,
}

pub trait CryptoAdapterQualificationProbe {
    fn collect(
        &self,
        target: &CryptoAdapterQualificationTarget,
    ) -> Result<CryptoAdapterQualificationEvidence, ApplicationError>;
}

impl CryptoQualificationDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[CryptoQualificationFailure] {
        &self.failures
    }

    /// The key this qualification actually examined.
    ///
    /// A decision that cannot say which key it looked at can be held up beside
    /// any release, which is how a custody qualification comes to answer a
    /// question nobody asked about the artifact in hand.
    pub fn observed_key(&self) -> &KeyReference {
        &self.observed_key
    }
}

pub struct CryptoAdapterQualificationService;

impl CryptoAdapterQualificationService {
    pub fn evaluate_with_probe<P: CryptoAdapterQualificationProbe>(
        target: &CryptoAdapterQualificationTarget,
        probe: &P,
    ) -> Result<CryptoQualificationDecision, ApplicationError> {
        let evidence = probe.collect(target)?;
        Ok(Self::evaluate(target, &evidence))
    }

    pub fn evaluate(
        target: &CryptoAdapterQualificationTarget,
        evidence: &CryptoAdapterQualificationEvidence,
    ) -> CryptoQualificationDecision {
        let mut failures = Vec::new();
        if !target.custody.is_production() {
            failures.push(CryptoQualificationFailure::ProductionCustodyRequired);
        }
        if !target.custody.accepts_provider(&target.provider) {
            failures.push(CryptoQualificationFailure::CustodyProviderMismatch);
        }
        if evidence.observed_key.provider() != target.provider {
            failures.push(CryptoQualificationFailure::ProviderMismatch);
        }
        if evidence.observed_key.algorithm_suite() != target.algorithm_suite {
            failures.push(CryptoQualificationFailure::AlgorithmMismatch);
        }
        if evidence.observed_key.purpose() != target.purpose {
            failures.push(CryptoQualificationFailure::PurposeMismatch);
        }
        if evidence.observed_key.scope() != target.scope {
            failures.push(CryptoQualificationFailure::ScopeMismatch);
        }
        if evidence.observed_key.key_id() != target.key_id {
            failures.push(CryptoQualificationFailure::KeyIdMismatch);
        }
        if evidence.observed_key.version() != target.key_version {
            failures.push(CryptoQualificationFailure::KeyVersionMismatch);
        }
        if !evidence.observed_key.is_usable() {
            failures.push(CryptoQualificationFailure::KeyNotUsable);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::Resolution)
        {
            failures.push(CryptoQualificationFailure::ResolutionNotProven);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::ScopeIsolation)
        {
            failures.push(CryptoQualificationFailure::ScopeIsolationNotProven);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::CryptographicRoundTrip)
        {
            failures.push(CryptoQualificationFailure::CryptographicRoundTripNotProven);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::Lifecycle)
        {
            failures.push(CryptoQualificationFailure::LifecycleNotProven);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::Rotation)
        {
            failures.push(CryptoQualificationFailure::RotationNotProven);
        }
        if !evidence
            .passed_checks
            .contains(&CryptoQualificationCheck::SecretNonDisclosure)
        {
            failures.push(CryptoQualificationFailure::SecretNonDisclosureNotProven);
        }
        let mut references = HashSet::new();
        if evidence.evidence_refs.is_empty()
            || evidence
                .evidence_refs
                .iter()
                .any(|reference| reference.trim().is_empty() || !references.insert(reference))
        {
            failures.push(CryptoQualificationFailure::EvidenceReferencesMissing);
        }
        CryptoQualificationDecision {
            failures,
            observed_key: evidence.observed_key.clone(),
        }
    }
}

fn non_blank(label: &str, value: String) -> Result<String, ApplicationError> {
    if value.trim().is_empty() {
        return Err(ApplicationError::InvalidConfiguration(format!(
            "{label} must not be blank"
        )));
    }
    Ok(value)
}
