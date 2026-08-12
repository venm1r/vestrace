use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, ConfiguredEffectFaultScenarioExecutor, EffectFaultScenarioExecutor,
    FaultInjectionEnvironment, FaultInjectionRuntime, FaultInjectionSettings,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, FaultObservation,
};

#[derive(Default)]
struct MockRuntime {
    calls: Mutex<Vec<(String, EffectFaultPoint)>>,
    fail: bool,
    wrong_point: bool,
}

#[async_trait]
impl FaultInjectionRuntime for MockRuntime {
    async fn execute(
        &self,
        target_digest: &str,
        point: EffectFaultPoint,
    ) -> Result<FaultObservation, ApplicationError> {
        self.calls
            .lock()
            .unwrap()
            .push((target_digest.to_owned(), point));
        if self.fail {
            return Err(ApplicationError::Unavailable(
                "fault runtime unavailable".into(),
            ));
        }
        let returned_point = if self.wrong_point {
            EffectFaultPoint::AfterIntentPersistence
        } else {
            point
        };
        Ok(FaultObservation::expected(returned_point))
    }
}

fn settings(enabled: bool) -> FaultInjectionSettings {
    FaultInjectionSettings::new(
        enabled,
        "sha256:deployment-target",
        FaultInjectionEnvironment::Ephemeral,
    )
    .unwrap()
}

#[tokio::test]
async fn configured_executor_rejects_disabled_fault_injection_before_runtime() {
    let runtime = Arc::new(MockRuntime::default());
    let executor = ConfiguredEffectFaultScenarioExecutor::new(settings(false), runtime.clone());

    let error = executor
        .execute(EffectFaultPoint::AfterIntentPersistence)
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Policy(message) if message.contains("disabled")));
    assert!(runtime.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn configured_executor_passes_exact_target_and_fault_point_to_runtime() {
    let runtime = Arc::new(MockRuntime::default());
    let executor = ConfiguredEffectFaultScenarioExecutor::new(settings(true), runtime.clone());

    let observation = executor
        .execute(EffectFaultPoint::AfterDispatchBeforeReceipt)
        .await
        .unwrap();

    assert_eq!(observation.status, EffectLifecycleStatus::Unknown);
    assert_eq!(
        runtime.calls.lock().unwrap().as_slice(),
        &[(
            "sha256:deployment-target".to_owned(),
            EffectFaultPoint::AfterDispatchBeforeReceipt
        )]
    );
}

#[tokio::test]
async fn configured_executor_propagates_runtime_failure() {
    let runtime = Arc::new(MockRuntime {
        fail: true,
        ..Default::default()
    });
    let executor = ConfiguredEffectFaultScenarioExecutor::new(settings(true), runtime);

    let error = executor
        .execute(EffectFaultPoint::AfterIntentPersistence)
        .await
        .unwrap_err();

    assert!(
        matches!(error, ApplicationError::Unavailable(message) if message.contains("unavailable"))
    );
}

#[tokio::test]
async fn configured_executor_rejects_observation_for_another_fault_point() {
    let runtime = Arc::new(MockRuntime {
        wrong_point: true,
        ..Default::default()
    });
    let executor = ConfiguredEffectFaultScenarioExecutor::new(settings(true), runtime);

    let error = executor
        .execute(EffectFaultPoint::AfterDispatchBeforeReceipt)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message))
            if message.contains("returned observation")
    ));
}
