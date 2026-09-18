use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use vestrace_application::{
    ApplicationError, ProgressiveTrustRestorationService, QualificationRepository,
    RecoveryRepository,
};
use vestrace_domain::conformance::gate::GovernanceFederationGate;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::runner::profile_requirements;
use vestrace_domain::conformance::{
    CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
};
use vestrace_domain::trust::{
    PostIncidentQualificationEvidence, QualificationBundle, QualificationLifecycle,
    RevalidationLevel, RevalidationResult, RevalidationRun, TrustState, TrustStateRecord,
};
use vestrace_domain::{
    HealthScope, IncidentId, QualificationBundleId, RecoveryPoint, RecoveryPointId,
    RevalidationRunId, VestraceCapabilityManifest,
};
use vestrace_domain::{Incident, IncidentId as DomainIncidentId};

fn at(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn scope() -> HealthScope {
    HealthScope::workspace(vestrace_domain::WorkspaceId::new())
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
        vec![QualificationProfile::Trusted],
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

fn incident_id() -> IncidentId {
    IncidentId::new()
}

fn make_run(
    incident_id: IncidentId,
    scope: HealthScope,
    result: RevalidationResult,
) -> RevalidationRun {
    RevalidationRun::complete(
        Some(incident_id),
        scope,
        RevalidationLevel::Workspace,
        "state:workspace:recovered",
        vec![vestrace_domain::RevalidationCheck::passing(
            "canonical-history",
            vec!["evidence:history".into()],
        )],
        vec!["evidence:revalidation".into()],
        result,
        at(20),
    )
    .unwrap()
}

fn bundle(
    run: &RevalidationRun,
    incident_id: IncidentId,
    result: RevalidationResult,
    force_failed_conformance: bool,
) -> QualificationBundle {
    let profile = QualificationProfile::Trusted;
    let required = GovernanceFederationGate::required_requirements(profile);
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("post-incident-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: if force_failed_conformance
                && requirement_id == profile_requirements(profile)[0]
            {
                CaseStatus::Fail
            } else {
                CaseStatus::Pass
            },
            message: "trusted gate passed".into(),
            evidence: Some(format!("test://{requirement_id}")),
            // A fixture standing in for a run that happened.
            origin: vestrace_domain::conformance::CaseOrigin::Executed,
        })
        .collect();
    let report = ConformanceReport::from_results(Some(profile), results);
    let evidence = PostIncidentQualificationEvidence::new(
        incident_id,
        run.id(),
        result,
        vec!["evidence:post-incident".into()],
    )
    .unwrap();
    QualificationBundle::from_conformance_report_for_manifest(
        QualificationLifecycle::PostIncident,
        profile,
        &manifest(),
        "suite-v1",
        report,
        required
            .into_iter()
            .map(|requirement_id| {
                HardGateEvidence::pass(
                    requirement_id,
                    format!("test://evidence/{requirement_id}"),
                    Some("policy-v1".into()),
                    EvidenceOrigin::LocalExecutable,
                )
            })
            .collect(),
        Vec::new(),
        at(19),
        Some(at(21)),
    )
    .unwrap()
    .attach_post_incident_evidence(evidence)
    .unwrap()
}

#[derive(Default)]
struct MemoryRecoveryRepository {
    latest: Mutex<Option<TrustStateRecord>>,
    runs: Mutex<HashMap<RevalidationRunId, RevalidationRun>>,
    inserted: Mutex<Vec<TrustStateRecord>>,
}

#[async_trait]
impl RecoveryRepository for MemoryRecoveryRepository {
    async fn insert_incident(&self, _incident: &Incident) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_incident(
        &self,
        _id: DomainIncidentId,
    ) -> Result<Option<Incident>, ApplicationError> {
        Ok(None)
    }

    async fn insert_revalidation_run(&self, run: &RevalidationRun) -> Result<(), ApplicationError> {
        self.runs.lock().unwrap().insert(run.id(), run.clone());
        Ok(())
    }

    async fn find_revalidation_run(
        &self,
        id: RevalidationRunId,
    ) -> Result<Option<RevalidationRun>, ApplicationError> {
        Ok(self.runs.lock().unwrap().get(&id).cloned())
    }

    async fn insert_trust_state(&self, state: &TrustStateRecord) -> Result<(), ApplicationError> {
        *self.latest.lock().unwrap() = Some(state.clone());
        self.inserted.lock().unwrap().push(state.clone());
        Ok(())
    }

    async fn find_latest_trust_state(
        &self,
        _scope: &HealthScope,
    ) -> Result<Option<TrustStateRecord>, ApplicationError> {
        Ok(self.latest.lock().unwrap().clone())
    }

    async fn insert_recovery_point(&self, _point: &RecoveryPoint) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_recovery_point(
        &self,
        _id: RecoveryPointId,
    ) -> Result<Option<RecoveryPoint>, ApplicationError> {
        Ok(None)
    }
}

#[derive(Default)]
struct MemoryQualificationRepository {
    bundles: Mutex<HashMap<QualificationBundleId, QualificationBundle>>,
}

#[async_trait]
impl QualificationRepository for MemoryQualificationRepository {
    async fn insert(&self, bundle: &QualificationBundle) -> Result<(), ApplicationError> {
        self.bundles
            .lock()
            .unwrap()
            .insert(bundle.id(), bundle.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: QualificationBundleId,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        Ok(self.bundles.lock().unwrap().get(&id).cloned())
    }

    async fn find_latest(
        &self,
        _profile: QualificationProfile,
        _lifecycle: QualificationLifecycle,
        _target_digest: &str,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        Ok(self.bundles.lock().unwrap().values().next().cloned())
    }
}

fn service(
    recovery: Arc<MemoryRecoveryRepository>,
    qualification: Arc<MemoryQualificationRepository>,
) -> ProgressiveTrustRestorationService {
    ProgressiveTrustRestorationService::new(recovery, qualification)
}

fn baseline(bundle: &QualificationBundle) -> vestrace_domain::QualificationBaseline {
    vestrace_domain::QualificationBaseline::from_bundle(bundle, at(22)).unwrap()
}

#[tokio::test]
async fn passed_revalidation_and_post_incident_bundle_restore_trust() {
    let scope = scope();
    let incident_id = incident_id();
    let run = make_run(incident_id, scope.clone(), RevalidationResult::Passed);
    let bundle = bundle(&run, incident_id, RevalidationResult::Passed, false);
    let recovery = Arc::new(MemoryRecoveryRepository::default());
    recovery
        .insert_trust_state(&TrustStateRecord::from_incident(
            scope.clone(),
            incident_id,
            vestrace_domain::IncidentSeverity::Critical,
            at(10),
        ))
        .await
        .unwrap();
    recovery.insert_revalidation_run(&run).await.unwrap();
    let qualification = Arc::new(MemoryQualificationRepository::default());
    qualification.insert(&bundle).await.unwrap();
    let blocked = service(recovery.clone(), qualification.clone())
        .restore(&scope, run.id(), bundle.id(), &manifest(), None, at(25))
        .await
        .unwrap();
    assert_eq!(blocked.state(), TrustState::Untrusted);

    let restored = service(recovery.clone(), qualification)
        .restore(
            &scope,
            run.id(),
            bundle.id(),
            &manifest(),
            Some(&baseline(&bundle)),
            at(30),
        )
        .await
        .unwrap();

    assert_eq!(
        restored.state(),
        TrustState::Trusted,
        "gate failures: {:?}",
        vestrace_domain::trust::TrustedQualificationGate::evaluate(&bundle, &baseline(&bundle))
            .failures()
    );
    assert_eq!(restored.revalidation_run_id(), Some(run.id()));
    assert_eq!(
        recovery.inserted.lock().unwrap().last().unwrap().state(),
        TrustState::Trusted
    );
}

#[tokio::test]
async fn passed_revalidation_without_a_passed_bundle_stays_untrusted() {
    let scope = scope();
    let incident_id = incident_id();
    let run = make_run(incident_id, scope.clone(), RevalidationResult::Passed);
    let failed_bundle = bundle(&run, incident_id, RevalidationResult::Passed, true);
    let recovery = Arc::new(MemoryRecoveryRepository::default());
    recovery
        .insert_trust_state(&TrustStateRecord::from_incident(
            scope.clone(),
            incident_id,
            vestrace_domain::IncidentSeverity::Critical,
            at(10),
        ))
        .await
        .unwrap();
    recovery.insert_revalidation_run(&run).await.unwrap();
    let qualification = Arc::new(MemoryQualificationRepository::default());
    qualification.insert(&failed_bundle).await.unwrap();

    let restored = service(recovery, qualification)
        .restore(
            &scope,
            run.id(),
            failed_bundle.id(),
            &manifest(),
            Some(&baseline(&failed_bundle)),
            at(30),
        )
        .await
        .unwrap();

    assert_eq!(restored.state(), TrustState::Untrusted);
}

#[tokio::test]
async fn inconclusive_revalidation_remains_revalidating() {
    let scope = scope();
    let incident_id = incident_id();
    let run = make_run(incident_id, scope.clone(), RevalidationResult::Inconclusive);
    let bundle = bundle(&run, incident_id, RevalidationResult::Inconclusive, false);
    let recovery = Arc::new(MemoryRecoveryRepository::default());
    recovery
        .insert_trust_state(&TrustStateRecord::from_incident(
            scope.clone(),
            incident_id,
            vestrace_domain::IncidentSeverity::Critical,
            at(10),
        ))
        .await
        .unwrap();
    recovery.insert_revalidation_run(&run).await.unwrap();
    let qualification = Arc::new(MemoryQualificationRepository::default());
    qualification.insert(&bundle).await.unwrap();

    let restored = service(recovery, qualification)
        .restore(
            &scope,
            run.id(),
            bundle.id(),
            &manifest(),
            Some(&baseline(&bundle)),
            at(30),
        )
        .await
        .unwrap();

    assert_eq!(restored.state(), TrustState::Revalidating);
}

#[tokio::test]
async fn mismatched_post_incident_evidence_fails_closed_before_trust_promotion() {
    let scope = scope();
    let incident_id = incident_id();
    let run = make_run(incident_id, scope.clone(), RevalidationResult::Passed);
    let wrong_run = make_run(incident_id, scope.clone(), RevalidationResult::Passed);
    let bundle = bundle(&wrong_run, incident_id, RevalidationResult::Passed, false);
    let recovery = Arc::new(MemoryRecoveryRepository::default());
    recovery
        .insert_trust_state(&TrustStateRecord::from_incident(
            scope.clone(),
            incident_id,
            vestrace_domain::IncidentSeverity::Critical,
            at(10),
        ))
        .await
        .unwrap();
    recovery.insert_revalidation_run(&run).await.unwrap();
    let qualification = Arc::new(MemoryQualificationRepository::default());
    qualification.insert(&bundle).await.unwrap();

    let error = service(recovery.clone(), qualification)
        .restore(
            &scope,
            run.id(),
            bundle.id(),
            &manifest(),
            Some(&baseline(&bundle)),
            at(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Policy(message) if message.contains("revalidation")));
    assert_eq!(recovery.inserted.lock().unwrap().len(), 1);
}
