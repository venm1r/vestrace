use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, EffectFaultScenarioExecutor, ExternalEffectFaultSuiteService,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, FaultObservation,
};

#[derive(Default)]
struct MockFaultExecutor {
    points: Mutex<Vec<EffectFaultPoint>>,
    unsafe_retry: bool,
    failure: Option<String>,
}

#[async_trait]
impl EffectFaultScenarioExecutor for MockFaultExecutor {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError> {
        self.points.lock().unwrap().push(point);
        if let Some(message) = &self.failure {
            return Err(ApplicationError::Internal(message.clone()));
        }
        let mut observation = FaultObservation::expected(point);
        observation.retry_attempted = self.unsafe_retry;
        Ok(observation)
    }
}

#[tokio::test]
async fn fault_suite_runs_every_required_point_and_evaluates_safe_evidence() {
    let executor = Arc::new(MockFaultExecutor::default());
    let service = ExternalEffectFaultSuiteService::new(executor.clone());

    let report = service.run().await.unwrap();

    assert_eq!(
        executor.points.lock().unwrap().as_slice(),
        &EffectFaultPoint::required_points()
    );
    assert_eq!(report.observations().len(), 5);
    assert!(report.decision().is_passed());
}

#[tokio::test]
async fn fault_suite_preserves_failed_decision_for_unsafe_retry_evidence() {
    let executor = Arc::new(MockFaultExecutor {
        unsafe_retry: true,
        ..Default::default()
    });
    let report = ExternalEffectFaultSuiteService::new(executor)
        .run()
        .await
        .unwrap();

    assert!(!report.decision().is_passed());
    assert!(
        report
            .decision()
            .failures()
            .iter()
            .any(|failure| failure.contains("unsafe retry"))
    );
}

#[tokio::test]
async fn fault_suite_fails_closed_on_executor_error() {
    let executor = Arc::new(MockFaultExecutor {
        failure: Some("fault harness unavailable".into()),
        ..Default::default()
    });

    let error = ExternalEffectFaultSuiteService::new(executor)
        .run()
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Internal(message) if message == "fault harness unavailable"
    ));
}

#[test]
fn required_fault_points_include_recovered_lost_dispatch() {
    let expected = FaultObservation::expected(EffectFaultPoint::AfterDispatchBeforeReceipt);

    assert_eq!(expected.status, EffectLifecycleStatus::Reconciling);
    assert!(expected.reconciliation_started);
    assert!(!expected.retry_attempted);
}
