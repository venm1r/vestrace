use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use vestrace_application::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultSuiteEvidence,
    ExternalEffectFaultSuiteService,
};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

#[derive(Default)]
struct MockFaultExecutor {
    unsafe_retry: bool,
    points: Mutex<Vec<EffectFaultPoint>>,
}

#[async_trait]
impl EffectFaultScenarioExecutor for MockFaultExecutor {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError> {
        self.points.lock().unwrap().push(point);
        let mut observation = FaultObservation::expected(point);
        observation.retry_attempted = self.unsafe_retry;
        Ok(observation)
    }
}

#[tokio::test]
async fn evidence_snapshot_preserves_target_and_all_fault_observations() {
    let executor = Arc::new(MockFaultExecutor::default());
    let report = ExternalEffectFaultSuiteService::new(executor)
        .run()
        .await
        .unwrap();

    let evidence =
        ExternalEffectFaultSuiteEvidence::from_report("sha256:deployment-target", &report, at(40))
            .unwrap();

    assert_eq!(evidence.target_digest(), "sha256:deployment-target");
    assert_eq!(evidence.observations().len(), 5);
    assert!(evidence.is_passed());
    assert!(evidence.failures().is_empty());
    assert_ne!(
        evidence.id().to_string(),
        "00000000-0000-0000-0000-000000000000"
    );
}

#[tokio::test]
async fn evidence_snapshot_retains_failed_decision_instead_of_promoting_it() {
    let report = ExternalEffectFaultSuiteService::new(Arc::new(MockFaultExecutor {
        unsafe_retry: true,
        ..Default::default()
    }))
    .run()
    .await
    .unwrap();

    let evidence =
        ExternalEffectFaultSuiteEvidence::from_report("sha256:deployment-target", &report, at(40))
            .unwrap();

    assert!(!evidence.is_passed());
    assert!(
        evidence
            .failures()
            .iter()
            .any(|failure| failure.contains("unsafe retry"))
    );
}

#[test]
fn evidence_snapshot_rejects_blank_target_identity() {
    let report = futures_placeholder_report();
    let error = ExternalEffectFaultSuiteEvidence::from_report(" ", &report, at(40)).unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message))
            if message.contains("target")
    ));
}

fn futures_placeholder_report() -> vestrace_application::ExternalEffectFaultSuiteReport {
    let report = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(
            ExternalEffectFaultSuiteService::new(Arc::new(MockFaultExecutor::default())).run(),
        )
        .unwrap();
    report
}
