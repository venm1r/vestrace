use std::sync::Arc;

use uuid::Uuid;
use vestrace_domain::DomainError;
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::{RequirementFamily, RequirementId};

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
                "fault-suite evidence {evidence_id} target does not match requested target"
            )));
        }

        let status = if evidence.is_passed() {
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
}
