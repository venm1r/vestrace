use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultSuiteEvidence,
    ExternalEffectFaultSuiteService, ExternalEffectQualificationBundleService,
    FaultSuiteEvidenceRepository, QualificationRepository,
};
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile, RequirementId,
};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle, QualificationStatus};
use vestrace_domain::{QualificationBundleId as BundleId, VestraceCapabilityManifest};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn manifest() -> VestraceCapabilityManifest {
    VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "0.2.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://test",
        vec!["schema-1"],
        vec![QualificationProfile::Core],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-key-provider"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
    )
    .unwrap()
}

fn report(profile: QualificationProfile) -> ConformanceReport {
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("bundle-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Pass,
            message: "base conformance passed".into(),
            evidence: Some(format!("test://bundle/{requirement_id}")),
        })
        .collect();
    ConformanceReport::from_results(Some(profile), results)
}

fn base_evidence(profile: QualificationProfile) -> Vec<HardGateEvidence> {
    profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::new(
                requirement_id,
                GateEvidenceStatus::Pass,
                Some(format!("test://bundle/{requirement_id}")),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}

#[derive(Default)]
struct MemoryFaultEvidenceRepository {
    evidence: Mutex<BTreeMap<Uuid, ExternalEffectFaultSuiteEvidence>>,
}

#[async_trait]
impl FaultSuiteEvidenceRepository for MemoryFaultEvidenceRepository {
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
        Ok(self.evidence.lock().unwrap().get(&id).cloned())
    }
}

#[derive(Default)]
struct MemoryQualificationRepository {
    inserted: Mutex<Vec<QualificationBundle>>,
    fail_insert: bool,
}

#[async_trait]
impl QualificationRepository for MemoryQualificationRepository {
    async fn insert(&self, bundle: &QualificationBundle) -> Result<(), ApplicationError> {
        if self.fail_insert {
            return Err(ApplicationError::Storage(
                "qualification bundle unavailable".into(),
            ));
        }
        self.inserted.lock().unwrap().push(bundle.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: BundleId,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        Ok(self
            .inserted
            .lock()
            .unwrap()
            .iter()
            .find(|bundle| bundle.id() == id)
            .cloned())
    }

    async fn find_latest(
        &self,
        _profile: QualificationProfile,
        _lifecycle: QualificationLifecycle,
        _target_digest: &str,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        Ok(self.inserted.lock().unwrap().last().cloned())
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

async fn create_fault_evidence(
    repository: Arc<MemoryFaultEvidenceRepository>,
    target_digest: &str,
    unsafe_retry: bool,
) -> ExternalEffectFaultSuiteEvidence {
    let report = ExternalEffectFaultSuiteService::new(Arc::new(MockExecutor { unsafe_retry }))
        .run()
        .await
        .unwrap();
    let evidence =
        ExternalEffectFaultSuiteEvidence::from_report(target_digest, &report, at(80)).unwrap();
    repository.insert(&evidence).await.unwrap();
    evidence
}

fn assembled_service(
    fault_repository: Arc<MemoryFaultEvidenceRepository>,
    qualification_repository: Arc<MemoryQualificationRepository>,
) -> ExternalEffectQualificationBundleService {
    ExternalEffectQualificationBundleService::new(fault_repository, qualification_repository)
}

#[tokio::test]
async fn bundle_assembly_persists_passed_fault_evidence_as_qual008() {
    let manifest = manifest();
    let fault_repository = Arc::new(MemoryFaultEvidenceRepository::default());
    let qualification_repository = Arc::new(MemoryQualificationRepository::default());
    let fault_evidence =
        create_fault_evidence(fault_repository.clone(), manifest.manifest_digest(), false).await;

    let bundle = assembled_service(fault_repository, qualification_repository.clone())
        .assemble_and_persist(
            QualificationLifecycle::Deployment,
            QualificationProfile::Core,
            &manifest,
            "suite-v1",
            report(QualificationProfile::Core),
            base_evidence(QualificationProfile::Core),
            Vec::new(),
            at(81),
            Some(at(82)),
            fault_evidence.id(),
        )
        .await
        .unwrap();

    assert_eq!(bundle.status(), QualificationStatus::Passed);
    assert!(bundle.evidence().iter().any(|evidence| {
        evidence.requirement_id()
            == RequirementId::new(vestrace_domain::conformance::RequirementFamily::Qual, 8)
            && evidence.status() == GateEvidenceStatus::Pass
    }));
    assert_eq!(qualification_repository.inserted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn bundle_assembly_persists_failed_bundle_for_failed_fault_evidence() {
    let manifest = manifest();
    let fault_repository = Arc::new(MemoryFaultEvidenceRepository::default());
    let qualification_repository = Arc::new(MemoryQualificationRepository::default());
    let fault_evidence =
        create_fault_evidence(fault_repository.clone(), manifest.manifest_digest(), true).await;

    let bundle = assembled_service(fault_repository, qualification_repository.clone())
        .assemble_and_persist(
            QualificationLifecycle::Deployment,
            QualificationProfile::Core,
            &manifest,
            "suite-v1",
            report(QualificationProfile::Core),
            base_evidence(QualificationProfile::Core),
            Vec::new(),
            at(81),
            Some(at(82)),
            fault_evidence.id(),
        )
        .await
        .unwrap();

    assert_eq!(bundle.status(), QualificationStatus::Failed);
    assert_eq!(qualification_repository.inserted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn bundle_assembly_rejects_target_mismatch_and_storage_failure() {
    let manifest = manifest();
    let fault_repository = Arc::new(MemoryFaultEvidenceRepository::default());
    let mismatch_evidence =
        create_fault_evidence(fault_repository.clone(), "sha256:other-target", false).await;
    let qualification_repository = Arc::new(MemoryQualificationRepository::default());

    let mismatch = assembled_service(fault_repository, qualification_repository)
        .assemble_and_persist(
            QualificationLifecycle::Deployment,
            QualificationProfile::Core,
            &manifest,
            "suite-v1",
            report(QualificationProfile::Core),
            base_evidence(QualificationProfile::Core),
            Vec::new(),
            at(81),
            Some(at(82)),
            mismatch_evidence.id(),
        )
        .await
        .unwrap_err();
    assert!(matches!(mismatch, ApplicationError::Policy(message) if message.contains("target")));

    let fault_repository = Arc::new(MemoryFaultEvidenceRepository::default());
    let fault_evidence =
        create_fault_evidence(fault_repository.clone(), manifest.manifest_digest(), false).await;
    let qualification_repository = Arc::new(MemoryQualificationRepository {
        fail_insert: true,
        ..Default::default()
    });
    let storage_error = assembled_service(fault_repository, qualification_repository)
        .assemble_and_persist(
            QualificationLifecycle::Deployment,
            QualificationProfile::Core,
            &manifest,
            "suite-v1",
            report(QualificationProfile::Core),
            base_evidence(QualificationProfile::Core),
            Vec::new(),
            at(81),
            Some(at(82)),
            fault_evidence.id(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(storage_error, ApplicationError::Storage(message) if message.contains("unavailable"))
    );
}

#[tokio::test]
async fn bundle_assembly_rejects_duplicate_qual008_result_before_loading_evidence() {
    let manifest = manifest();
    let fault_repository = Arc::new(MemoryFaultEvidenceRepository::default());
    let qualification_repository = Arc::new(MemoryQualificationRepository::default());
    let duplicate_report = ConformanceReport::from_results(
        Some(QualificationProfile::Core),
        vec![ConformanceCaseResult {
            case_id: "duplicate-qual008".into(),
            requirement_ids: vec![RequirementId::new(
                vestrace_domain::conformance::RequirementFamily::Qual,
                8,
            )],
            status: CaseStatus::Pass,
            message: "duplicate".into(),
            evidence: Some("test://duplicate".into()),
        }],
    );

    let error = assembled_service(fault_repository, qualification_repository)
        .assemble_and_persist(
            QualificationLifecycle::Deployment,
            QualificationProfile::Core,
            &manifest,
            "suite-v1",
            duplicate_report,
            Vec::new(),
            Vec::new(),
            at(81),
            Some(at(82)),
            Uuid::nil(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Conflict(message) if message.contains("QUAL-008")));
}
