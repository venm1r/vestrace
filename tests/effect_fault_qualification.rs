use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultQualificationService,
    ExternalEffectFaultSuiteEvidence, FaultSuiteEvidenceRepository,
};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

#[derive(Default)]
struct MemoryEvidenceRepository {
    inserted: Mutex<Vec<ExternalEffectFaultSuiteEvidence>>,
}

#[async_trait]
impl FaultSuiteEvidenceRepository for MemoryEvidenceRepository {
    async fn insert(
        &self,
        evidence: &ExternalEffectFaultSuiteEvidence,
    ) -> Result<(), ApplicationError> {
        self.inserted.lock().unwrap().push(evidence.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<ExternalEffectFaultSuiteEvidence>, ApplicationError> {
        Ok(None)
    }
}

struct MockExecutor {
    unsafe_retry: bool,
    fail: bool,
}

#[async_trait]
impl EffectFaultScenarioExecutor for MockExecutor {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError> {
        if self.fail {
            return Err(ApplicationError::Internal(
                "fault executor unavailable".into(),
            ));
        }
        let mut observation = FaultObservation::expected(point);
        observation.retry_attempted = self.unsafe_retry;
        Ok(observation)
    }
}

#[tokio::test]
async fn qualification_service_persists_passed_evidence_for_exact_target() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let service = ExternalEffectFaultQualificationService::new(
        Arc::new(MockExecutor {
            unsafe_retry: false,
            fail: false,
        }),
        repository.clone(),
    );

    let evidence = service
        .run("sha256:deployment-target", at(50))
        .await
        .unwrap();

    assert!(evidence.is_passed());
    assert_eq!(evidence.target_digest(), "sha256:deployment-target");
    assert_eq!(repository.inserted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn qualification_service_persists_failed_evidence_without_promoting_it() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let service = ExternalEffectFaultQualificationService::new(
        Arc::new(MockExecutor {
            unsafe_retry: true,
            fail: false,
        }),
        repository.clone(),
    );

    let evidence = service
        .run("sha256:deployment-target", at(50))
        .await
        .unwrap();

    assert!(!evidence.is_passed());
    assert!(!evidence.failures().is_empty());
    assert!(!repository.inserted.lock().unwrap()[0].is_passed());
}

#[tokio::test]
async fn qualification_service_does_not_persist_when_executor_fails() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let service = ExternalEffectFaultQualificationService::new(
        Arc::new(MockExecutor {
            unsafe_retry: false,
            fail: true,
        }),
        repository.clone(),
    );

    let error = service
        .run("sha256:deployment-target", at(50))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Internal(message) if message == "fault executor unavailable"
    ));
    assert!(repository.inserted.lock().unwrap().is_empty());
}
