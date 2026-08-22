use std::sync::Arc;

use uuid::Uuid;
use vestrace_domain::DomainError;
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::{RequirementFamily, RequirementId};
use vestrace_domain::external_effects::{
    FaultObservation, FaultSuiteDecision, evaluate_fault_suite,
};

use crate::{ApplicationError, FaultSuiteEvidenceRepository};

pub struct ExternalEffectFaultGateEvidenceService {
    evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>,
}

impl ExternalEffectFaultGateEvidenceService {
    pub fn new(evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>) -> Self {
        Self {
            evidence_repository,
        }
    }

    pub async fn load(
        &self,
        evidence_id: Uuid,
        target_digest: impl Into<String>,
    ) -> Result<HardGateEvidence, ApplicationError> {
        let decision = self.load_decision(evidence_id, target_digest).await?;
        let status = if decision.is_passed() {
            GateEvidenceStatus::Pass
        } else {
            GateEvidenceStatus::Fail
        };
        Ok(HardGateEvidence::new(
            RequirementId::new(RequirementFamily::Qual, 8),
            status,
            Some(format!("fault-suite://{evidence_id}")),
            None,
            EvidenceOrigin::LocalExecutable,
        ))
    }

    pub async fn load_decision(
        &self,
        evidence_id: Uuid,
        target_digest: impl Into<String>,
    ) -> Result<FaultSuiteDecision, ApplicationError> {
        let target_digest = target_digest.into();
        if target_digest.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "fault-suite gate target digest must not be blank".into(),
            )
            .into());
        }

        let evidence = self
            .evidence_repository
            .find_by_id(evidence_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Policy(format!(
                    "fault-suite evidence {evidence_id} was not found"
                ))
            })?;

        if evidence.target_digest() != target_digest {
            return Err(ApplicationError::Policy(format!(
                "fault-suite evidence {evidence_id} target '{}' does not match requested target '{}'",
                evidence.target_digest(),
                target_digest,
            )));
        }

        // The stored verdict records the evaluator used when the suite ran.
        // Release admission is governed by the evaluator that exists now, so
        // reconstruct its input from the immutable observations instead of
        // allowing an older `passed` flag to outrank the current contract.
        let observations = evidence
            .observations()
            .iter()
            .map(FaultObservation::from)
            .collect::<Vec<_>>();
        Ok(evaluate_fault_suite(&observations))
    }
}
