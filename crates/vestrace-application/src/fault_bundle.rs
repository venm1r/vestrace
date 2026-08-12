use std::sync::Arc;

use uuid::Uuid;
use vestrace_domain::conformance::gate::{GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile, RequirementFamily,
    RequirementId,
};
use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle};
use vestrace_domain::{DomainError, Timestamp, VestraceCapabilityManifest};

use crate::{
    ApplicationError, ExternalEffectFaultGateEvidenceService, FaultSuiteEvidenceRepository,
    QualificationRepository,
};

pub struct ExternalEffectQualificationBundleService {
    fault_gate: ExternalEffectFaultGateEvidenceService,
    qualification_repository: Arc<dyn QualificationRepository>,
}

impl ExternalEffectQualificationBundleService {
    pub fn new(
        fault_repository: Arc<dyn FaultSuiteEvidenceRepository>,
        qualification_repository: Arc<dyn QualificationRepository>,
    ) -> Self {
        Self {
            fault_gate: ExternalEffectFaultGateEvidenceService::new(fault_repository),
            qualification_repository,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn assemble_and_persist(
        &self,
        lifecycle: QualificationLifecycle,
        profile: QualificationProfile,
        manifest: &VestraceCapabilityManifest,
        suite_version: impl Into<String>,
        report: ConformanceReport,
        mut evidence: Vec<HardGateEvidence>,
        known_limitations: Vec<String>,
        started_at: Timestamp,
        completed_at: Option<Timestamp>,
        fault_evidence_id: Uuid,
    ) -> Result<QualificationBundle, ApplicationError> {
        let fault_requirement = RequirementId::new(RequirementFamily::Qual, 8);
        if report.profile != Some(profile) {
            return Err(DomainError::InvalidArgument(
                "qualification report profile must match bundle profile".into(),
            )
            .into());
        }
        if report
            .results
            .iter()
            .any(|result| result.requirement_ids.contains(&fault_requirement))
        {
            return Err(ApplicationError::Conflict(
                "qualification report already contains QUAL-008 fault evidence".into(),
            ));
        }
        if evidence
            .iter()
            .any(|item| item.requirement_id() == fault_requirement)
        {
            return Err(ApplicationError::Conflict(
                "qualification evidence already contains QUAL-008 fault evidence".into(),
            ));
        }

        let fault_evidence = self
            .fault_gate
            .load(fault_evidence_id, manifest.manifest_digest())
            .await?;
        let fault_status = fault_evidence.status();
        let fault_evidence_ref = fault_evidence.evidence_ref().map(str::to_owned);
        evidence.push(fault_evidence);

        let mut results = report.results;
        results.push(ConformanceCaseResult {
            case_id: format!("external-effect-fault-suite/{fault_evidence_id}"),
            requirement_ids: vec![fault_requirement],
            status: match fault_status {
                GateEvidenceStatus::Pass => CaseStatus::Pass,
                GateEvidenceStatus::Fail => CaseStatus::Fail,
                _ => {
                    return Err(ApplicationError::Internal(
                        "fault gate emitted unsupported evidence status".into(),
                    ));
                }
            },
            message: match fault_status {
                GateEvidenceStatus::Pass => "external-effect fault suite passed".into(),
                GateEvidenceStatus::Fail => "external-effect fault suite failed".into(),
                _ => unreachable!(),
            },
            evidence: fault_evidence_ref,
        });
        let report = ConformanceReport::from_results(report.profile, results);
        let bundle = QualificationBundle::from_conformance_report_for_manifest(
            lifecycle,
            profile,
            manifest,
            suite_version,
            report,
            evidence,
            known_limitations,
            started_at,
            completed_at,
        )?;
        self.qualification_repository.insert(&bundle).await?;
        Ok(bundle)
    }
}
