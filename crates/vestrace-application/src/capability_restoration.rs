use std::collections::HashMap;

use vestrace_domain::security::Capability;
use vestrace_domain::trust::TrustState;

use crate::ApplicationError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RestorationStage {
    DiagnosticsReadOnly,
    InternalDeterministicWrites,
    SemanticMutation,
    ReversibleExternalEffects,
    IrreversibleExternalEffects,
}

impl RestorationStage {
    fn requires_qualification(self) -> bool {
        self > Self::DiagnosticsReadOnly
    }

    fn requires_revalidation(self) -> bool {
        self >= Self::SemanticMutation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRestorationPolicy {
    stages: HashMap<Capability, RestorationStage>,
}

impl CapabilityRestorationPolicy {
    pub fn new(
        entries: impl IntoIterator<Item = (Capability, RestorationStage)>,
    ) -> Result<Self, ApplicationError> {
        let mut stages = HashMap::new();
        for (capability, stage) in entries {
            if stages.insert(capability, stage).is_some() {
                return Err(ApplicationError::InvalidConfiguration(
                    "capability restoration policy cannot contain duplicate capabilities".into(),
                ));
            }
        }
        if stages.is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "capability restoration policy requires at least one capability".into(),
            ));
        }
        Ok(Self { stages })
    }

    pub fn stage_for(&self, capability: &Capability) -> Option<RestorationStage> {
        self.stages.get(capability).copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestorationEvidence {
    qualification_passed: bool,
    revalidation_passed: bool,
    evidence_refs: Vec<String>,
}

impl RestorationEvidence {
    pub fn new(
        qualification_passed: bool,
        revalidation_passed: bool,
        evidence_refs: Vec<String>,
    ) -> Self {
        Self {
            qualification_passed,
            revalidation_passed,
            evidence_refs,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestorationBlockReason {
    CapabilityNotDeclared,
    TrustBarrier,
    QualificationEvidenceMissing,
    RevalidationEvidenceMissing,
    EvidenceReferencesMissing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityRestorationDecision {
    Allowed {
        capability: Capability,
        stage: RestorationStage,
    },
    Blocked {
        capability: Capability,
        stage: Option<RestorationStage>,
        reason: RestorationBlockReason,
    },
}

impl CapabilityRestorationDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    pub fn stage(&self) -> Option<RestorationStage> {
        match self {
            Self::Allowed { stage, .. }
            | Self::Blocked {
                stage: Some(stage), ..
            } => Some(*stage),
            Self::Blocked { stage: None, .. } => None,
        }
    }
}

pub struct CapabilityRestorationService;

impl CapabilityRestorationService {
    pub fn evaluate(
        trust_state: TrustState,
        capability: Capability,
        policy: &CapabilityRestorationPolicy,
        evidence: &RestorationEvidence,
    ) -> CapabilityRestorationDecision {
        let Some(stage) = policy.stage_for(&capability) else {
            return CapabilityRestorationDecision::Blocked {
                capability,
                stage: None,
                reason: RestorationBlockReason::CapabilityNotDeclared,
            };
        };

        if trust_state != TrustState::Trusted && stage > RestorationStage::DiagnosticsReadOnly {
            if trust_state == TrustState::DegradedTrust
                && stage == RestorationStage::InternalDeterministicWrites
                && evidence.qualification_passed
                && !evidence.evidence_refs.is_empty()
            {
                return CapabilityRestorationDecision::Allowed { capability, stage };
            }
            return CapabilityRestorationDecision::Blocked {
                capability,
                stage: Some(stage),
                reason: RestorationBlockReason::TrustBarrier,
            };
        }

        if stage.requires_qualification() && !evidence.qualification_passed {
            return CapabilityRestorationDecision::Blocked {
                capability,
                stage: Some(stage),
                reason: RestorationBlockReason::QualificationEvidenceMissing,
            };
        }
        if stage.requires_revalidation() && !evidence.revalidation_passed {
            return CapabilityRestorationDecision::Blocked {
                capability,
                stage: Some(stage),
                reason: RestorationBlockReason::RevalidationEvidenceMissing,
            };
        }
        if stage.requires_qualification() && evidence.evidence_refs.is_empty() {
            return CapabilityRestorationDecision::Blocked {
                capability,
                stage: Some(stage),
                reason: RestorationBlockReason::EvidenceReferencesMissing,
            };
        }

        CapabilityRestorationDecision::Allowed { capability, stage }
    }
}
