use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use vestrace_domain::{
    QualificationBaselineId, QualificationBundle, QualificationBundleId, QualificationLifecycle,
    VestraceCapabilityManifest, conformance::QualificationProfile, trust::QualificationBaseline,
};

use crate::ApplicationError;

/// Durable control-plane boundary for target-bound qualification evidence.
///
/// Qualification bundles are not workspace-owned business records. They bind
/// evidence to a release/deployment target and therefore intentionally do not
/// accept a workspace-scoped RequestContext.
#[async_trait]
pub trait QualificationRepository: Send + Sync {
    async fn insert(&self, bundle: &QualificationBundle) -> Result<(), ApplicationError>;

    async fn find_by_id(
        &self,
        id: QualificationBundleId,
    ) -> Result<Option<QualificationBundle>, ApplicationError>;

    async fn find_latest(
        &self,
        profile: QualificationProfile,
        lifecycle: QualificationLifecycle,
        target_digest: &str,
    ) -> Result<Option<QualificationBundle>, ApplicationError>;
}

pub type SharedQualificationRepository = Arc<dyn QualificationRepository>;

/// Durable boundary for an operator-published qualification baseline.
///
/// Like the bundle it summarizes, a baseline describes a release target rather
/// than workspace-owned data and therefore has no workspace request context.
#[async_trait]
pub trait QualificationBaselineRepository: Send + Sync {
    async fn insert(&self, baseline: &QualificationBaseline) -> Result<(), ApplicationError>;

    async fn find_by_id(
        &self,
        id: QualificationBaselineId,
    ) -> Result<Option<QualificationBaseline>, ApplicationError>;

    async fn find_by_target_digest(
        &self,
        target_digest: &str,
    ) -> Result<Option<QualificationBaseline>, ApplicationError>;
}

pub type SharedQualificationBaselineRepository = Arc<dyn QualificationBaselineRepository>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationRuntime {
    Server,
    Worker,
}

impl std::fmt::Display for QualificationRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Server => formatter.write_str("server"),
            Self::Worker => formatter.write_str("worker"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeQualificationEvidence {
    pub migration_history_compatible: bool,
    pub runtime_role: String,
    pub is_superuser: bool,
    pub bypasses_rls: bool,
    pub inherits_bootstrap: bool,
}

impl RuntimeQualificationEvidence {
    pub fn unavailable() -> Self {
        Self {
            migration_history_compatible: false,
            runtime_role: String::new(),
            is_superuser: false,
            bypasses_rls: false,
            inherits_bootstrap: false,
        }
    }

    pub fn runtime_database_is_restricted(&self) -> bool {
        !self.runtime_role.trim().is_empty()
            && !self.is_superuser
            && !self.bypasses_rls
            && !self.inherits_bootstrap
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeQualificationDecision {
    pub component: QualificationRuntime,
    pub target_manifest: String,
    pub status: String,
    pub checks: BTreeMap<String, String>,
}

impl RuntimeQualificationDecision {
    pub fn is_passed(&self) -> bool {
        self.status == "passed"
    }
}

pub fn evaluate_runtime_qualification(
    component: QualificationRuntime,
    manifest: &VestraceCapabilityManifest,
    bundle: &QualificationBundle,
    profile: QualificationProfile,
    lifecycle: QualificationLifecycle,
    runtime: &RuntimeQualificationEvidence,
) -> RuntimeQualificationDecision {
    let manifest_integrity = manifest.validate().is_ok();
    let bundle_target_binding = manifest_integrity
        && bundle
            .validate_manifest_identity(manifest, profile, lifecycle)
            .is_ok();
    let checks = BTreeMap::from([
        (
            "manifest_integrity".to_owned(),
            if manifest_integrity {
                "passed"
            } else {
                "failed"
            }
            .to_owned(),
        ),
        (
            "bundle_target_binding".to_owned(),
            if bundle_target_binding {
                "passed"
            } else {
                "failed"
            }
            .to_owned(),
        ),
        (
            "bundle_status".to_owned(),
            if bundle.status() == vestrace_domain::QualificationStatus::Passed {
                "passed"
            } else {
                "failed"
            }
            .to_owned(),
        ),
        (
            "migration_history".to_owned(),
            if runtime.migration_history_compatible {
                "passed"
            } else {
                "failed"
            }
            .to_owned(),
        ),
        (
            "runtime_database".to_owned(),
            if runtime.runtime_database_is_restricted() {
                "passed"
            } else {
                "failed"
            }
            .to_owned(),
        ),
    ]);
    let status = if checks.values().all(|value| value == "passed") {
        "passed"
    } else {
        "failed"
    };

    RuntimeQualificationDecision {
        component,
        target_manifest: manifest.manifest_digest().to_owned(),
        status: status.to_owned(),
        checks,
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::{
        QualificationRuntime, RuntimeQualificationEvidence, evaluate_runtime_qualification,
    };
    use vestrace_domain::{
        QualificationBundle, QualificationLifecycle, VestraceCapabilityManifest,
        conformance::QualificationProfile,
    };

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

    fn bundle(manifest: &VestraceCapabilityManifest) -> QualificationBundle {
        let results =
            vestrace_domain::conformance::runner::profile_requirements(QualificationProfile::Core)
                .into_iter()
                .map(
                    |requirement_id| vestrace_domain::conformance::ConformanceCaseResult {
                        case_id: format!("test-{requirement_id}"),
                        requirement_ids: vec![requirement_id],
                        status: vestrace_domain::conformance::CaseStatus::Pass,
                        message: "passed".into(),
                        evidence: Some(format!("test://{requirement_id}")),
                        origin: vestrace_domain::conformance::CaseOrigin::Executed,
                    },
                )
                .collect();
        QualificationBundle::from_conformance_report_for_manifest(
            QualificationLifecycle::Release,
            QualificationProfile::Core,
            manifest,
            "suite-v1",
            vestrace_domain::conformance::ConformanceReport::from_results(
                Some(QualificationProfile::Core),
                results,
            ),
            Vec::new(),
            Vec::new(),
            vestrace_domain::now(),
            Some(vestrace_domain::now()),
        )
        .unwrap()
    }

    fn runtime() -> RuntimeQualificationEvidence {
        RuntimeQualificationEvidence {
            migration_history_compatible: true,
            runtime_role: "vestrace".into(),
            is_superuser: false,
            bypasses_rls: false,
            inherits_bootstrap: false,
        }
    }

    #[test]
    fn runtime_qualification_passes_only_for_a_restricted_exact_target() {
        let manifest = manifest();
        let decision = evaluate_runtime_qualification(
            QualificationRuntime::Server,
            &manifest,
            &bundle(&manifest),
            QualificationProfile::Core,
            QualificationLifecycle::Release,
            &runtime(),
        );

        assert!(decision.is_passed());
        assert_eq!(decision.component, QualificationRuntime::Server);
        assert_eq!(decision.checks["runtime_database"], "passed");
    }

    #[test]
    fn runtime_qualification_fails_for_superuser_or_rls_bypass() {
        let manifest = manifest();
        let mut evidence = runtime();
        evidence.is_superuser = true;
        evidence.bypasses_rls = true;

        let decision = evaluate_runtime_qualification(
            QualificationRuntime::Worker,
            &manifest,
            &bundle(&manifest),
            QualificationProfile::Core,
            QualificationLifecycle::Release,
            &evidence,
        );

        assert!(!decision.is_passed());
        assert_eq!(decision.checks["runtime_database"], "failed");
    }

    #[test]
    fn unavailable_runtime_evidence_is_fail_closed() {
        let manifest = manifest();
        let decision = evaluate_runtime_qualification(
            QualificationRuntime::Server,
            &manifest,
            &bundle(&manifest),
            QualificationProfile::Core,
            QualificationLifecycle::Release,
            &RuntimeQualificationEvidence::unavailable(),
        );

        assert!(!decision.is_passed());
        assert_eq!(decision.checks["migration_history"], "failed");
        assert_eq!(decision.checks["runtime_database"], "failed");
    }
}
