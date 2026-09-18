use std::sync::Arc;

use uuid::Uuid;
use vestrace_domain::DomainError;

use crate::{ApplicationError, ExternalEffectFaultSuiteEvidence, FaultSuiteEvidenceRepository};

pub struct ExternalEffectFaultEvidenceAdmissionService {
    evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>,
}

impl ExternalEffectFaultEvidenceAdmissionService {
    pub fn new(evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>) -> Self {
        Self {
            evidence_repository,
        }
    }

    pub async fn require_passed(
        &self,
        evidence_id: Uuid,
        target_digest: impl Into<String>,
    ) -> Result<ExternalEffectFaultSuiteEvidence, ApplicationError> {
        let target_digest = target_digest.into();
        if target_digest.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "fault-suite admission target digest must not be blank".into(),
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
        if !evidence.is_passed() {
            return Err(ApplicationError::Policy(format!(
                "fault-suite evidence {evidence_id} did not pass"
            )));
        }

        Ok(evidence)
    }
}
