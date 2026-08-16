use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    QualificationProfile, RequirementFamily, RequirementId, registry, runner::profile_requirements,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateEvidenceStatus {
    Pass,
    Fail,
    NotApplicable,
    Skipped,
    Inconclusive,
}

/// Where a piece of gate evidence came from, which decides how much it is
/// worth.
///
/// # Why there are five of these and not four
///
/// There were three, and `CaseOrigin::Attested` was mapped onto
/// `RemoteSelfAsserted` — so this installation's own written reading of its own
/// code was filed under the same heading as a federation peer's claim about
/// itself, and failed the hard gate for the same reason.
///
/// Those are not the same thing, and the codebase already said so in the other
/// direction: `ConformanceReport::attested_but_should_execute` exempts
/// `VerificationClass::Static`, because "the architecture separates
/// authoritative from derived state" is a claim about shape that no runtime
/// assertion observes. One place called an attestation the right evidence for
/// such a requirement; the other called every attestation a self-assertion.
/// Both were in the build, and they cannot both be right.
///
/// The distinction that matters is the trust boundary, not the word
/// "attested". A remote self-assertion is untrustworthy because the subject and
/// the source are the same party and the reader cannot check. A local
/// attestation is published *with the thing it describes*: the source is in the
/// bundle, and a reader who doubts it can read the code it points at.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOrigin {
    /// A case ran here and produced this result.
    LocalExecutable,
    /// Compilation established a type-level invariant in the local domain
    /// crate. Admissible only for `Domain` requirements.
    LocalBuildVerified,
    /// A written claim about this build, published beside it. Admissible only
    /// where the requirement's class is `Static` — see
    /// [`GovernanceFederationGate::evaluate`].
    LocalAttested,
    /// A third party vouched for the subject.
    RemoteAttested,
    /// The subject vouched for itself.
    RemoteSelfAsserted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HardGateEvidence {
    requirement_id: RequirementId,
    status: GateEvidenceStatus,
    evidence_ref: Option<String>,
    policy_version: Option<String>,
    origin: EvidenceOrigin,
}

impl HardGateEvidence {
    pub fn new(
        requirement_id: RequirementId,
        status: GateEvidenceStatus,
        evidence_ref: Option<String>,
        policy_version: Option<String>,
        origin: EvidenceOrigin,
    ) -> Self {
        Self {
            requirement_id,
            status,
            evidence_ref,
            policy_version,
            origin,
        }
    }

    pub fn pass(
        requirement_id: RequirementId,
        evidence_ref: impl Into<String>,
        policy_version: Option<String>,
        origin: EvidenceOrigin,
    ) -> Self {
        Self::new(
            requirement_id,
            GateEvidenceStatus::Pass,
            Some(evidence_ref.into()),
            policy_version,
            origin,
        )
    }

    pub fn requirement_id(&self) -> RequirementId {
        self.requirement_id
    }

    pub fn status(&self) -> GateEvidenceStatus {
        self.status
    }

    pub fn evidence_ref(&self) -> Option<&str> {
        self.evidence_ref.as_deref()
    }

    pub fn policy_version(&self) -> Option<&str> {
        self.policy_version.as_deref()
    }

    pub fn origin(&self) -> EvidenceOrigin {
        self.origin
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum HardGateFailure {
    UnsupportedProfile {
        profile: QualificationProfile,
    },
    MissingEvidence {
        requirement_id: RequirementId,
    },
    DuplicateEvidence {
        requirement_id: RequirementId,
    },
    Failed {
        requirement_id: RequirementId,
    },
    NotApplicable {
        requirement_id: RequirementId,
    },
    Skipped {
        requirement_id: RequirementId,
    },
    Inconclusive {
        requirement_id: RequirementId,
    },
    MissingEvidenceReference {
        requirement_id: RequirementId,
    },
    MissingPolicyVersion {
        requirement_id: RequirementId,
    },
    RemoteSelfAssertion {
        requirement_id: RequirementId,
    },
    /// A written claim standing in for a requirement that describes behaviour.
    ///
    /// Admissible for `Static` requirements and for nothing else: everything
    /// with another class describes something that can be made to happen, and a
    /// sentence about it is not evidence that it does.
    AttestationWhereExecutionIsRequired {
        requirement_id: RequirementId,
    },
    /// A compiler-verified claim was supplied for a requirement whose class
    /// requires runtime evidence.
    BuildVerificationWhereRuntimeIsRequired {
        requirement_id: RequirementId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HardGateDecision {
    profile: QualificationProfile,
    failures: Vec<HardGateFailure>,
}

impl HardGateDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn profile(&self) -> QualificationProfile {
        self.profile
    }

    pub fn failures(&self) -> &[HardGateFailure] {
        &self.failures
    }
}

pub struct GovernanceFederationGate;

impl GovernanceFederationGate {
    pub fn required_requirements(profile: QualificationProfile) -> Vec<RequirementId> {
        let mut required = Vec::new();
        for requirement_id in profile_requirements(profile) {
            let is_must = registry::find(&requirement_id)
                .map(|requirement| requirement.level == super::RequirementLevel::Must)
                .unwrap_or(false);
            if is_must && !required.contains(&requirement_id) {
                required.push(requirement_id);
            }
        }

        for requirement_id in Self::hard_gate_requirements() {
            if !required.contains(&requirement_id) {
                required.push(requirement_id);
            }
        }
        required
    }

    pub fn evaluate(
        profile: QualificationProfile,
        evidence: &[HardGateEvidence],
    ) -> HardGateDecision {
        let mut failures = Vec::new();
        if !matches!(
            profile,
            QualificationProfile::Federation | QualificationProfile::Trusted
        ) {
            failures.push(HardGateFailure::UnsupportedProfile { profile });
            return HardGateDecision { profile, failures };
        }

        let required = Self::required_requirements(profile);
        let mut by_requirement = HashMap::new();
        for item in evidence {
            if by_requirement.insert(item.requirement_id(), item).is_some() {
                failures.push(HardGateFailure::DuplicateEvidence {
                    requirement_id: item.requirement_id(),
                });
            }
        }

        for requirement_id in required {
            let Some(item) = by_requirement.get(&requirement_id) else {
                failures.push(HardGateFailure::MissingEvidence { requirement_id });
                continue;
            };

            match item.origin() {
                EvidenceOrigin::RemoteSelfAsserted => {
                    failures.push(HardGateFailure::RemoteSelfAssertion { requirement_id });
                }
                EvidenceOrigin::LocalAttested => {
                    // The class comes from the registry, never from the
                    // evidence, so a bundle cannot admit an attestation by
                    // relabelling what the requirement is.
                    let is_static = registry::find(&requirement_id)
                        .map(|requirement| requirement.class == super::VerificationClass::Static)
                        .unwrap_or(false);
                    if !is_static {
                        failures.push(HardGateFailure::AttestationWhereExecutionIsRequired {
                            requirement_id,
                        });
                    }
                }
                EvidenceOrigin::LocalBuildVerified => {
                    let is_domain = registry::find(&requirement_id)
                        .map(|requirement| requirement.class == super::VerificationClass::Domain)
                        .unwrap_or(false);
                    if !is_domain {
                        failures.push(HardGateFailure::BuildVerificationWhereRuntimeIsRequired {
                            requirement_id,
                        });
                    }
                }
                EvidenceOrigin::LocalExecutable | EvidenceOrigin::RemoteAttested => {}
            }
            match item.status() {
                GateEvidenceStatus::Pass => {
                    if item
                        .evidence_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        failures.push(HardGateFailure::MissingEvidenceReference { requirement_id });
                    }
                    if Self::requires_policy_version(requirement_id)
                        && item
                            .policy_version()
                            .map(|value| value.trim().is_empty())
                            .unwrap_or(true)
                    {
                        failures.push(HardGateFailure::MissingPolicyVersion { requirement_id });
                    }
                }
                GateEvidenceStatus::Fail => {
                    failures.push(HardGateFailure::Failed { requirement_id })
                }
                GateEvidenceStatus::NotApplicable => {
                    failures.push(HardGateFailure::NotApplicable { requirement_id })
                }
                GateEvidenceStatus::Skipped => {
                    failures.push(HardGateFailure::Skipped { requirement_id })
                }
                GateEvidenceStatus::Inconclusive => {
                    failures.push(HardGateFailure::Inconclusive { requirement_id })
                }
            }
        }

        HardGateDecision { profile, failures }
    }

    fn hard_gate_requirements() -> [RequirementId; 14] {
        [
            RequirementId::new(RequirementFamily::Idw, 6),
            RequirementId::new(RequirementFamily::Idw, 7),
            RequirementId::new(RequirementFamily::Idw, 8),
            RequirementId::new(RequirementFamily::Idw, 13),
            RequirementId::new(RequirementFamily::Gov, 20),
            RequirementId::new(RequirementFamily::Gov, 24),
            RequirementId::new(RequirementFamily::Gov, 25),
            RequirementId::new(RequirementFamily::Qual, 2),
            RequirementId::new(RequirementFamily::Qual, 4),
            RequirementId::new(RequirementFamily::Qual, 6),
            RequirementId::new(RequirementFamily::Qual, 7),
            RequirementId::new(RequirementFamily::Qual, 13),
            RequirementId::new(RequirementFamily::Qual, 17),
            RequirementId::new(RequirementFamily::Qual, 18),
        ]
    }

    fn requires_policy_version(requirement_id: RequirementId) -> bool {
        matches!(
            (requirement_id.family, requirement_id.number),
            (RequirementFamily::Gov, 24) | (RequirementFamily::Gov, 25)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnderstandMilestone {
    Understand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum UnderstandGateFailure {
    MissingEvidence { requirement_id: RequirementId },
    DuplicateEvidence { requirement_id: RequirementId },
    Failed { requirement_id: RequirementId },
    NotApplicable { requirement_id: RequirementId },
    Skipped { requirement_id: RequirementId },
    Inconclusive { requirement_id: RequirementId },
    MissingEvidenceReference { requirement_id: RequirementId },
    RemoteSelfAssertion { requirement_id: RequirementId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnderstandGateDecision {
    milestone: UnderstandMilestone,
    failures: Vec<UnderstandGateFailure>,
}

impl UnderstandGateDecision {
    pub fn milestone(&self) -> UnderstandMilestone {
        self.milestone
    }

    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[UnderstandGateFailure] {
        &self.failures
    }
}

pub struct UnderstandGate;

impl UnderstandGate {
    pub fn required_requirements() -> Vec<RequirementId> {
        (1..=19)
            .map(|number| RequirementId::new(RequirementFamily::Hlt, number))
            .collect()
    }

    pub fn evaluate(evidence: &[HardGateEvidence]) -> UnderstandGateDecision {
        let mut failures = Vec::new();
        let mut by_requirement = HashMap::new();
        for item in evidence {
            if by_requirement.insert(item.requirement_id(), item).is_some() {
                failures.push(UnderstandGateFailure::DuplicateEvidence {
                    requirement_id: item.requirement_id(),
                });
            }
        }

        for requirement_id in Self::required_requirements() {
            let Some(item) = by_requirement.get(&requirement_id) else {
                failures.push(UnderstandGateFailure::MissingEvidence { requirement_id });
                continue;
            };
            if item.origin() == EvidenceOrigin::RemoteSelfAsserted {
                failures.push(UnderstandGateFailure::RemoteSelfAssertion { requirement_id });
            }
            match item.status() {
                GateEvidenceStatus::Pass => {
                    if item
                        .evidence_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        failures.push(UnderstandGateFailure::MissingEvidenceReference {
                            requirement_id,
                        });
                    }
                }
                GateEvidenceStatus::Fail => {
                    failures.push(UnderstandGateFailure::Failed { requirement_id })
                }
                GateEvidenceStatus::NotApplicable => {
                    failures.push(UnderstandGateFailure::NotApplicable { requirement_id })
                }
                GateEvidenceStatus::Skipped => {
                    failures.push(UnderstandGateFailure::Skipped { requirement_id })
                }
                GateEvidenceStatus::Inconclusive => {
                    failures.push(UnderstandGateFailure::Inconclusive { requirement_id })
                }
            }
        }

        UnderstandGateDecision {
            milestone: UnderstandMilestone::Understand,
            failures,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectMilestone {
    Connect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ConnectGateFailure {
    MissingEvidence { requirement_id: RequirementId },
    DuplicateEvidence { requirement_id: RequirementId },
    Failed { requirement_id: RequirementId },
    NotApplicable { requirement_id: RequirementId },
    Skipped { requirement_id: RequirementId },
    Inconclusive { requirement_id: RequirementId },
    MissingEvidenceReference { requirement_id: RequirementId },
    RemoteSelfAssertion { requirement_id: RequirementId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConnectGateDecision {
    milestone: ConnectMilestone,
    failures: Vec<ConnectGateFailure>,
}

impl ConnectGateDecision {
    pub fn milestone(&self) -> ConnectMilestone {
        self.milestone
    }

    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[ConnectGateFailure] {
        &self.failures
    }
}

pub struct ConnectGate;

impl ConnectGate {
    pub fn required_requirements() -> Vec<RequirementId> {
        let mut requirements = (1..=18)
            .map(|number| RequirementId::new(RequirementFamily::Ext, number))
            .collect::<Vec<_>>();
        requirements.push(RequirementId::new(RequirementFamily::Qual, 8));
        requirements
    }

    pub fn evaluate(evidence: &[HardGateEvidence]) -> ConnectGateDecision {
        let mut failures = Vec::new();
        let mut by_requirement = HashMap::new();
        for item in evidence {
            if by_requirement.insert(item.requirement_id(), item).is_some() {
                failures.push(ConnectGateFailure::DuplicateEvidence {
                    requirement_id: item.requirement_id(),
                });
            }
        }
        for requirement_id in Self::required_requirements() {
            let Some(item) = by_requirement.get(&requirement_id) else {
                failures.push(ConnectGateFailure::MissingEvidence { requirement_id });
                continue;
            };
            if item.origin() == EvidenceOrigin::RemoteSelfAsserted {
                failures.push(ConnectGateFailure::RemoteSelfAssertion { requirement_id });
            }
            match item.status() {
                GateEvidenceStatus::Pass => {
                    if item
                        .evidence_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        failures
                            .push(ConnectGateFailure::MissingEvidenceReference { requirement_id });
                    }
                }
                GateEvidenceStatus::Fail => {
                    failures.push(ConnectGateFailure::Failed { requirement_id })
                }
                GateEvidenceStatus::NotApplicable => {
                    failures.push(ConnectGateFailure::NotApplicable { requirement_id })
                }
                GateEvidenceStatus::Skipped => {
                    failures.push(ConnectGateFailure::Skipped { requirement_id })
                }
                GateEvidenceStatus::Inconclusive => {
                    failures.push(ConnectGateFailure::Inconclusive { requirement_id })
                }
            }
        }
        ConnectGateDecision {
            milestone: ConnectMilestone::Connect,
            failures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceOrigin, GateEvidenceStatus, GovernanceFederationGate, HardGateEvidence,
        HardGateFailure,
    };
    use crate::conformance::{
        QualificationProfile, RequirementFamily, RequirementId, VerificationClass, registry,
    };

    fn trusted_evidence() -> Vec<HardGateEvidence> {
        GovernanceFederationGate::required_requirements(QualificationProfile::Trusted)
            .into_iter()
            .map(|id| {
                let policy_version = matches!(
                    (id.family, id.number),
                    (RequirementFamily::Gov, 24) | (RequirementFamily::Gov, 25)
                )
                .then(|| "policy-v1".to_owned());
                HardGateEvidence::pass(
                    id,
                    format!("evidence:{id}"),
                    policy_version,
                    EvidenceOrigin::LocalExecutable,
                )
            })
            .collect()
    }

    #[test]
    fn federation_gate_rejects_missing_required_evidence() {
        let id = RequirementId::new(RequirementFamily::Idw, 6);
        let evidence = vec![HardGateEvidence::new(
            id,
            GateEvidenceStatus::Skipped,
            Some("evidence:skip".into()),
            None,
            EvidenceOrigin::LocalExecutable,
        )];

        let decision =
            GovernanceFederationGate::evaluate(QualificationProfile::Federation, &evidence);
        assert!(!decision.is_passed());
        assert!(
            decision
                .failures()
                .contains(&HardGateFailure::Skipped { requirement_id: id })
        );
    }

    #[test]
    fn trusted_gate_accepts_build_verified_domain_evidence() {
        let id = RequirementId::new(RequirementFamily::Mem, 1);
        let requirement = registry::find(&id).expect("MEM-001 must be registered");
        assert_eq!(requirement.class, VerificationClass::Domain);

        let evidence = trusted_evidence()
            .into_iter()
            .map(|item| {
                if item.requirement_id() == id {
                    HardGateEvidence::pass(
                        id,
                        "evidence:MEM-001",
                        None,
                        EvidenceOrigin::LocalBuildVerified,
                    )
                } else {
                    item
                }
            })
            .collect::<Vec<_>>();

        let decision = GovernanceFederationGate::evaluate(QualificationProfile::Trusted, &evidence);
        assert!(
            decision.is_passed(),
            "unexpected failures: {:?}",
            decision.failures()
        );
    }

    #[test]
    fn trusted_gate_rejects_build_verified_non_domain_evidence() {
        let id = RequirementId::new(RequirementFamily::Arc, 2);
        let requirement = registry::find(&id).expect("ARC-002 must be registered");
        assert_ne!(requirement.class, VerificationClass::Domain);

        let evidence = trusted_evidence()
            .into_iter()
            .map(|item| {
                if item.requirement_id() == id {
                    HardGateEvidence::pass(
                        id,
                        "evidence:ARC-002",
                        None,
                        EvidenceOrigin::LocalBuildVerified,
                    )
                } else {
                    item
                }
            })
            .collect::<Vec<_>>();

        let decision = GovernanceFederationGate::evaluate(QualificationProfile::Trusted, &evidence);
        assert!(!decision.is_passed());
        assert!(decision.failures().contains(
            &HardGateFailure::BuildVerificationWhereRuntimeIsRequired { requirement_id: id }
        ));
    }
}
