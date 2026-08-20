use std::collections::HashSet;

use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::QualificationProfile;
use vestrace_domain::external_effects::FaultSuiteDecision;
use vestrace_domain::trust::{KeyReference, RecoveryQualificationDecision};

use crate::ApplicationError;
use crate::capability_restoration::CapabilityRestorationDecision;
use crate::crypto_qualification::CryptoQualificationDecision;
use crate::qualification::RuntimeQualificationDecision;
use crate::release_approval::ReleaseApprovalDecision;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactEnvironmentReleaseTarget {
    release_version: String,
    source_revision: String,
    build_digest: String,
    configuration_digest: String,
    environment_manifest: String,
    manifest_digest: String,
    schema_versions: Vec<String>,
    profile: QualificationProfile,
    signer_keys: Vec<KeyReference>,
}

impl ExactEnvironmentReleaseTarget {
    /// Record a key that signed this release.
    ///
    /// Every signer must be the key the crypto qualification examined. Two
    /// artifacts of one release signed by two different keys cannot both be the
    /// qualified custody, so that disagreement is a mismatch rather than a
    /// choice the gate makes for the reader.
    pub fn with_signer_key(mut self, key: KeyReference) -> Self {
        self.signer_keys.push(key);
        self
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        release_version: impl Into<String>,
        source_revision: impl Into<String>,
        build_digest: impl Into<String>,
        configuration_digest: impl Into<String>,
        environment_manifest: impl Into<String>,
        manifest_digest: impl Into<String>,
        schema_versions: Vec<impl Into<String>>,
        profile: QualificationProfile,
    ) -> Result<Self, ApplicationError> {
        let schema_versions = schema_versions
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        if schema_versions.is_empty()
            || schema_versions
                .iter()
                .any(|version| version.trim().is_empty())
            || has_duplicates(&schema_versions)
        {
            return Err(ApplicationError::InvalidConfiguration(
                "exact release target requires unique non-blank schema versions".into(),
            ));
        }
        Ok(Self {
            release_version: non_blank("release version", release_version.into())?,
            source_revision: non_blank("source revision", source_revision.into())?,
            build_digest: non_blank("build digest", build_digest.into())?,
            configuration_digest: non_blank("configuration digest", configuration_digest.into())?,
            environment_manifest: non_blank("environment manifest", environment_manifest.into())?,
            manifest_digest: non_blank("manifest digest", manifest_digest.into())?,
            schema_versions,
            profile,
            signer_keys: Vec::new(),
        })
    }

    pub fn from_manifest(
        manifest: &VestraceCapabilityManifest,
        profile: QualificationProfile,
    ) -> Result<Self, ApplicationError> {
        Self::new(
            manifest.product_version(),
            manifest.source_revision(),
            manifest.build_digest(),
            manifest.configuration_digest(),
            manifest.environment_manifest(),
            manifest.manifest_digest(),
            manifest.schema_versions().to_vec(),
            profile,
        )
        .map(|target| match manifest.signature() {
            // The signer is read from the artifact rather than supplied
            // alongside it: a caller who could name the signer could name the
            // one whose custody it had qualified.
            Some(signature) => target.with_signer_key(signature.key_ref().clone()),
            None => target,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactEnvironmentReleaseEvidence {
    release_version: String,
    source_revision: String,
    build_digest: String,
    configuration_digest: String,
    environment_manifest: String,
    manifest_digest: String,
    schema_versions: Vec<String>,
    profile: QualificationProfile,
    pub(crate) release_approval: Option<ReleaseApprovalDecision>,
    pub(crate) runtime_qualification: Option<RuntimeQualificationDecision>,
    pub(crate) crypto_qualification: Option<CryptoQualificationDecision>,
    pub(crate) recovery_qualification: Option<RecoveryQualificationDecision>,
    pub(crate) fault_suite: Option<FaultSuiteDecision>,
    pub(crate) capability_restoration: Vec<CapabilityRestorationDecision>,
    evidence_refs: Vec<String>,
    known_limitations: Vec<String>,
}

impl ExactEnvironmentReleaseEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        release_version: impl Into<String>,
        source_revision: impl Into<String>,
        build_digest: impl Into<String>,
        configuration_digest: impl Into<String>,
        environment_manifest: impl Into<String>,
        manifest_digest: impl Into<String>,
        schema_versions: Vec<String>,
        profile: QualificationProfile,
        release_approval: Option<ReleaseApprovalDecision>,
        runtime_qualification: Option<RuntimeQualificationDecision>,
        crypto_qualification: Option<CryptoQualificationDecision>,
        recovery_qualification: Option<RecoveryQualificationDecision>,
        fault_suite: Option<FaultSuiteDecision>,
        capability_restoration: Vec<CapabilityRestorationDecision>,
        evidence_refs: Vec<String>,
        known_limitations: Vec<String>,
    ) -> Self {
        Self {
            release_version: release_version.into(),
            source_revision: source_revision.into(),
            build_digest: build_digest.into(),
            configuration_digest: configuration_digest.into(),
            environment_manifest: environment_manifest.into(),
            manifest_digest: manifest_digest.into(),
            schema_versions,
            profile,
            release_approval,
            runtime_qualification,
            crypto_qualification,
            recovery_qualification,
            fault_suite,
            capability_restoration,
            evidence_refs,
            known_limitations,
        }
    }

    pub fn unavailable() -> Self {
        Self::new(
            "",
            "",
            "",
            "",
            "",
            "",
            Vec::new(),
            QualificationProfile::Trusted,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactEnvironmentReleaseFailure {
    ReleaseIdentityMismatch,
    SchemaVersionsMismatch,
    ProfileMismatch,
    ReleaseApprovalMissing,
    ReleaseApprovalFailed,
    RuntimeQualificationMissing,
    RuntimeQualificationFailed,
    RuntimeManifestMismatch,
    CryptoQualificationMissing,
    CryptoQualificationFailed,
    CryptoSignerMismatch,
    RecoveryQualificationMissing,
    RecoveryQualificationFailed,
    FaultSuiteMissing,
    FaultSuiteFailed,
    CapabilityRestorationMissing,
    CapabilityRestorationFailed,
    EvidenceReferencesMissing,
    KnownLimitationsMissing,
    KnownLimitationBlank,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactEnvironmentReleaseDecision {
    failures: Vec<ExactEnvironmentReleaseFailure>,
}

impl ExactEnvironmentReleaseDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[ExactEnvironmentReleaseFailure] {
        &self.failures
    }
}

pub trait V1ReleaseEvidenceProbe {
    fn collect(
        &self,
        target: &ExactEnvironmentReleaseTarget,
    ) -> Result<ExactEnvironmentReleaseEvidence, ApplicationError>;
}

/// Whether a signer reference and a qualified key name the same custody.
///
/// The reference's `id` is a fresh identifier minted per construction, so
/// comparing whole references would never match. What has to agree is the
/// custody: which provider holds the key, which key, which version, for what
/// scope, under which algorithm.
fn signs_with(signer: &KeyReference, observed: &KeyReference) -> bool {
    signer.provider() == observed.provider()
        && signer.key_id() == observed.key_id()
        && signer.version() == observed.version()
        && signer.scope() == observed.scope()
        && signer.algorithm_suite() == observed.algorithm_suite()
        && signer.purpose() == observed.purpose()
}

pub struct V1ReleaseEvidenceService;

impl V1ReleaseEvidenceService {
    pub fn evaluate_with_probe<P: V1ReleaseEvidenceProbe>(
        target: &ExactEnvironmentReleaseTarget,
        probe: &P,
    ) -> Result<ExactEnvironmentReleaseDecision, ApplicationError> {
        Ok(Self::evaluate(target, &probe.collect(target)?))
    }

    pub fn evaluate(
        target: &ExactEnvironmentReleaseTarget,
        evidence: &ExactEnvironmentReleaseEvidence,
    ) -> ExactEnvironmentReleaseDecision {
        let mut failures = Vec::new();
        if evidence.release_version != target.release_version
            || evidence.source_revision != target.source_revision
            || evidence.build_digest != target.build_digest
            || evidence.configuration_digest != target.configuration_digest
            || evidence.environment_manifest != target.environment_manifest
            || evidence.manifest_digest != target.manifest_digest
        {
            failures.push(ExactEnvironmentReleaseFailure::ReleaseIdentityMismatch);
        }
        if evidence.schema_versions != target.schema_versions {
            failures.push(ExactEnvironmentReleaseFailure::SchemaVersionsMismatch);
        }
        if evidence.profile != target.profile {
            failures.push(ExactEnvironmentReleaseFailure::ProfileMismatch);
        }

        match &evidence.release_approval {
            Some(decision) if decision.is_approved() => {}
            Some(_) => failures.push(ExactEnvironmentReleaseFailure::ReleaseApprovalFailed),
            None => failures.push(ExactEnvironmentReleaseFailure::ReleaseApprovalMissing),
        }
        match &evidence.runtime_qualification {
            Some(decision) if !decision.is_passed() => {
                failures.push(ExactEnvironmentReleaseFailure::RuntimeQualificationFailed)
            }
            Some(decision) if decision.target_manifest != target.manifest_digest => {
                failures.push(ExactEnvironmentReleaseFailure::RuntimeManifestMismatch)
            }
            Some(_) => {}
            None => failures.push(ExactEnvironmentReleaseFailure::RuntimeQualificationMissing),
        }
        match &evidence.crypto_qualification {
            Some(decision) if !decision.is_passed() => {
                failures.push(ExactEnvironmentReleaseFailure::CryptoQualificationFailed)
            }
            // A custody qualification for a key that signed nothing here is a
            // true statement about some other key. Runtime qualification has
            // always been bound to the build it examined; this binds crypto to
            // the artifact it is offered for.
            Some(decision)
                if !target.signer_keys.is_empty()
                    && target
                        .signer_keys
                        .iter()
                        .any(|signer| !signs_with(signer, decision.observed_key())) =>
            {
                failures.push(ExactEnvironmentReleaseFailure::CryptoSignerMismatch)
            }
            Some(_) => {}
            None => failures.push(ExactEnvironmentReleaseFailure::CryptoQualificationMissing),
        }
        match &evidence.recovery_qualification {
            Some(decision) if decision.is_passed() => {}
            Some(_) => failures.push(ExactEnvironmentReleaseFailure::RecoveryQualificationFailed),
            None => failures.push(ExactEnvironmentReleaseFailure::RecoveryQualificationMissing),
        }
        match &evidence.fault_suite {
            Some(decision) if decision.is_passed() => {}
            Some(_) => failures.push(ExactEnvironmentReleaseFailure::FaultSuiteFailed),
            None => failures.push(ExactEnvironmentReleaseFailure::FaultSuiteMissing),
        }
        if evidence.capability_restoration.is_empty() {
            failures.push(ExactEnvironmentReleaseFailure::CapabilityRestorationMissing);
        } else if evidence
            .capability_restoration
            .iter()
            .any(|decision| !decision.is_allowed())
        {
            failures.push(ExactEnvironmentReleaseFailure::CapabilityRestorationFailed);
        }

        let mut references = HashSet::new();
        if evidence.evidence_refs.is_empty()
            || evidence
                .evidence_refs
                .iter()
                .any(|reference| reference.trim().is_empty() || !references.insert(reference))
        {
            failures.push(ExactEnvironmentReleaseFailure::EvidenceReferencesMissing);
        }
        if evidence.known_limitations.is_empty() {
            failures.push(ExactEnvironmentReleaseFailure::KnownLimitationsMissing);
        } else if evidence
            .known_limitations
            .iter()
            .any(|limitation| limitation.trim().is_empty())
        {
            failures.push(ExactEnvironmentReleaseFailure::KnownLimitationBlank);
        }

        ExactEnvironmentReleaseDecision { failures }
    }
}

fn has_duplicates(values: &[String]) -> bool {
    let mut seen = HashSet::new();
    values.iter().any(|value| !seen.insert(value))
}

fn non_blank(label: &str, value: String) -> Result<String, ApplicationError> {
    if value.trim().is_empty() {
        return Err(ApplicationError::InvalidConfiguration(format!(
            "{label} must not be blank"
        )));
    }
    Ok(value)
}
