use std::sync::Arc;

use vestrace_domain::Timestamp;

use crate::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultSuiteEvidence,
    ExternalEffectFaultSuiteService, FaultSuiteEvidenceRepository,
};

pub struct ExternalEffectFaultQualificationService {
    suite: ExternalEffectFaultSuiteService,
    evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>,
}

impl ExternalEffectFaultQualificationService {
    pub fn new(
        executor: Arc<dyn EffectFaultScenarioExecutor>,
        evidence_repository: Arc<dyn FaultSuiteEvidenceRepository>,
    ) -> Self {
        Self {
            suite: ExternalEffectFaultSuiteService::new(executor),
            evidence_repository,
        }
    }

    pub async fn run(
        &self,
        target_digest: impl Into<String>,
        created_at: Timestamp,
    ) -> Result<ExternalEffectFaultSuiteEvidence, ApplicationError> {
        let report = self.suite.run().await?;
        let evidence =
            ExternalEffectFaultSuiteEvidence::from_report(target_digest, &report, created_at)?;
        self.evidence_repository.insert(&evidence).await?;
        Ok(evidence)
    }
}
