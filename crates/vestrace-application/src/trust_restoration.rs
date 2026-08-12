use std::sync::Arc;

use vestrace_domain::trust::{
    QualificationBaseline, QualificationBundle, QualificationLifecycle, QualificationStatus,
    RevalidationResult, TrustState, TrustStateRecord, TrustedQualificationGate,
};
use vestrace_domain::{
    HealthScope, QualificationBundleId, RevalidationRunId, Timestamp, VestraceCapabilityManifest,
    conformance::QualificationProfile,
};

use crate::{ApplicationError, QualificationRepository, RecoveryRepository};

pub struct ProgressiveTrustRestorationService {
    recovery_repository: Arc<dyn RecoveryRepository>,
    qualification_repository: Arc<dyn QualificationRepository>,
}

impl ProgressiveTrustRestorationService {
    pub fn new(
        recovery_repository: Arc<dyn RecoveryRepository>,
        qualification_repository: Arc<dyn QualificationRepository>,
    ) -> Self {
        Self {
            recovery_repository,
            qualification_repository,
        }
    }

    pub async fn restore(
        &self,
        scope: &HealthScope,
        revalidation_run_id: RevalidationRunId,
        qualification_bundle_id: QualificationBundleId,
        manifest: &VestraceCapabilityManifest,
        baseline: Option<&QualificationBaseline>,
        at: Timestamp,
    ) -> Result<TrustStateRecord, ApplicationError> {
        let mut trust = self
            .recovery_repository
            .find_latest_trust_state(scope)
            .await?
            .ok_or_else(|| {
                ApplicationError::Policy(
                    "trust restoration requires an existing persisted trust state".into(),
                )
            })?;
        let run = self
            .recovery_repository
            .find_revalidation_run(revalidation_run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Policy(format!(
                    "revalidation run {revalidation_run_id} was not found"
                ))
            })?;
        if run.scope() != scope {
            return Err(ApplicationError::Policy(
                "revalidation run scope does not match trust scope".into(),
            ));
        }

        let bundle = self
            .qualification_repository
            .find_by_id(qualification_bundle_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Policy(format!(
                    "qualification bundle {qualification_bundle_id} was not found"
                ))
            })?;
        bundle.validate_manifest_identity(
            manifest,
            QualificationProfile::Trusted,
            QualificationLifecycle::PostIncident,
        )?;
        validate_post_incident_evidence(&trust, &run, &bundle)?;

        trust.begin_revalidation(run.id(), at)?;
        self.recovery_repository.insert_trust_state(&trust).await?;

        let final_state = match run.result() {
            RevalidationResult::Passed => {
                let gate_passed = bundle.status() == QualificationStatus::Passed
                    && baseline.is_some_and(|baseline| {
                        TrustedQualificationGate::evaluate(&bundle, baseline).is_passed()
                    });
                if gate_passed {
                    trust.apply_revalidation(&run)?;
                    trust
                } else {
                    TrustStateRecord::new(
                        scope.clone(),
                        TrustState::Untrusted,
                        "trust restoration blocked by incomplete Trusted qualification closure",
                        trust.incident_id(),
                        at,
                    )?
                }
            }
            RevalidationResult::PassedWithDegradation => {
                trust.apply_revalidation(&run)?;
                trust
            }
            RevalidationResult::Failed => TrustStateRecord::new(
                scope.clone(),
                TrustState::Untrusted,
                "trust restoration failed during revalidation",
                trust.incident_id(),
                at,
            )?,
            RevalidationResult::Inconclusive => {
                trust.apply_revalidation(&run)?;
                trust
            }
        };

        self.recovery_repository
            .insert_trust_state(&final_state)
            .await?;
        Ok(final_state)
    }
}

fn validate_post_incident_evidence(
    trust: &TrustStateRecord,
    run: &vestrace_domain::RevalidationRun,
    bundle: &QualificationBundle,
) -> Result<(), ApplicationError> {
    if bundle.lifecycle() != QualificationLifecycle::PostIncident
        || bundle.profile() != QualificationProfile::Trusted
    {
        return Err(ApplicationError::Policy(
            "trust restoration requires a Trusted post-incident qualification bundle".into(),
        ));
    }
    let evidence = bundle.post_incident_evidence().ok_or_else(|| {
        ApplicationError::Policy(
            "trust restoration requires post-incident qualification evidence".into(),
        )
    })?;
    if Some(evidence.incident_id()) != trust.incident_id()
        || evidence.revalidation_run_id() != run.id()
        || evidence.revalidation_result() != run.result()
    {
        return Err(ApplicationError::Policy(
            "post-incident qualification evidence does not match revalidation".into(),
        ));
    }
    Ok(())
}
