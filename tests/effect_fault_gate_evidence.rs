use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultGateEvidenceService,
    ExternalEffectFaultQualificationService, ExternalEffectFaultSuiteEvidence,
    FaultSuiteEvidenceRepository,
};
use vestrace_domain::conformance::gate::GateEvidenceStatus;
use vestrace_domain::conformance::{RequirementFamily, RequirementId};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

#[derive(Default)]
struct MemoryEvidenceRepository {
    evidence: Mutex<BTreeMap<Uuid, ExternalEffectFaultSuiteEvidence>>,
    fail_reads: bool,
}

#[async_trait]
impl FaultSuiteEvidenceRepository for MemoryEvidenceRepository {
    async fn insert(
        &self,
        evidence: &ExternalEffectFaultSuiteEvidence,
    ) -> Result<(), ApplicationError> {
        self.evidence
            .lock()
            .unwrap()
            .insert(evidence.id(), evidence.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ExternalEffectFaultSuiteEvidence>, ApplicationError> {
        if self.fail_reads {
            return Err(ApplicationError::Storage(
                "fault evidence read unavailable".into(),
            ));
        }
        Ok(self.evidence.lock().unwrap().get(&id).cloned())
    }
}

struct MockExecutor {
    unsafe_retry: bool,
}

#[async_trait]
impl EffectFaultScenarioExecutor for MockExecutor {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError> {
        let mut observation = FaultObservation::expected(point);
        observation.retry_attempted = self.unsafe_retry;
        Ok(observation)
    }
}

async fn create_evidence(
    repository: Arc<MemoryEvidenceRepository>,
    unsafe_retry: bool,
) -> ExternalEffectFaultSuiteEvidence {
    ExternalEffectFaultQualificationService::new(
        Arc::new(MockExecutor { unsafe_retry }),
        repository,
    )
    .run("sha256:deployment-target", at(70))
    .await
    .unwrap()
}

#[tokio::test]
async fn gate_adapter_maps_passed_evidence_to_local_qual008_pass() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let evidence = create_evidence(repository.clone(), false).await;
    let service = ExternalEffectFaultGateEvidenceService::new(repository);

    let gate_evidence = service
        .load(evidence.id(), "sha256:deployment-target")
        .await
        .unwrap();

    assert_eq!(
        gate_evidence.requirement_id(),
        RequirementId::new(RequirementFamily::Qual, 8)
    );
    assert_eq!(gate_evidence.status(), GateEvidenceStatus::Pass);
    assert!(
        gate_evidence
            .evidence_ref()
            .unwrap()
            .starts_with("fault-suite://")
    );
}

#[tokio::test]
async fn gate_adapter_preserves_failed_evidence_as_fail() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let evidence = create_evidence(repository.clone(), true).await;
    let service = ExternalEffectFaultGateEvidenceService::new(repository);

    let gate_evidence = service
        .load(evidence.id(), "sha256:deployment-target")
        .await
        .unwrap();

    assert_eq!(gate_evidence.status(), GateEvidenceStatus::Fail);
    assert!(
        gate_evidence
            .evidence_ref()
            .unwrap()
            .starts_with("fault-suite://")
    );
}

#[tokio::test]
async fn gate_adapter_rejects_target_mismatch_and_missing_evidence() {
    let repository = Arc::new(MemoryEvidenceRepository::default());
    let evidence = create_evidence(repository.clone(), false).await;
    let service = ExternalEffectFaultGateEvidenceService::new(repository);

    let mismatch = service
        .load(evidence.id(), "sha256:another-target")
        .await
        .unwrap_err();
    assert!(matches!(mismatch, ApplicationError::Policy(message) if message.contains("target")));

    let missing = service
        .load(Uuid::now_v7(), "sha256:deployment-target")
        .await
        .unwrap_err();
    assert!(matches!(missing, ApplicationError::Policy(message) if message.contains("not found")));
}

#[tokio::test]
async fn gate_adapter_propagates_repository_failure() {
    let repository = Arc::new(MemoryEvidenceRepository {
        fail_reads: true,
        ..Default::default()
    });
    let service = ExternalEffectFaultGateEvidenceService::new(repository);

    let error = service
        .load(Uuid::now_v7(), "sha256:deployment-target")
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Storage(message) if message.contains("unavailable")));
}
