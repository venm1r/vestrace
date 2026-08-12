use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, ConfiguredEffectFaultScenarioExecutor, EffectFaultScenarioExecutor,
    FaultInjectionEnvironment, FaultInjectionRuntime, FaultInjectionSettings,
    ProcessFaultInjectionRuntime,
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

fn process_runtime(script: &str, timeout: Duration) -> ProcessFaultInjectionRuntime {
    #[cfg(windows)]
    {
        ProcessFaultInjectionRuntime::new(
            settings(true),
            "powershell.exe",
            ["-NoProfile", "-NonInteractive", "-Command", script],
            timeout,
        )
        .unwrap()
    }
    #[cfg(not(windows))]
    {
        ProcessFaultInjectionRuntime::new(settings(true), "sh", ["-c", script], timeout).unwrap()
    }
}

#[tokio::test]
async fn process_runtime_passes_target_and_point_to_an_isolated_child() {
    #[cfg(windows)]
    let script = r#"
if ($env:VESTRACE_FAULT_TARGET_DIGEST -ne 'sha256:deployment-target') { exit 7 }
$status = if ($env:VESTRACE_FAULT_POINT -eq 'after_dispatch_before_receipt') { 'unknown' } else { 'prepared' }
$reconciliation = $env:VESTRACE_FAULT_POINT -eq 'after_dispatch_before_receipt'
$o = @{ point = $env:VESTRACE_FAULT_POINT; status = $status; retry_attempted = $false; reconciliation_started = $reconciliation; receipt_persisted = $false }
$o | ConvertTo-Json -Compress
"#;
    #[cfg(not(windows))]
    let script = r#"test "$VESTRACE_FAULT_TARGET_DIGEST" = "sha256:deployment-target" || exit 7; if [ "$VESTRACE_FAULT_POINT" = "after_dispatch_before_receipt" ]; then printf '%s' '{"point":"after_dispatch_before_receipt","status":"unknown","retry_attempted":false,"reconciliation_started":true,"receipt_persisted":false}'; else printf '%s' '{"point":"after_intent_persistence","status":"prepared","retry_attempted":false,"reconciliation_started":false,"receipt_persisted":false}'; fi"#;

    let runtime = process_runtime(script, Duration::from_secs(2));
    let observation = runtime
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterDispatchBeforeReceipt,
        )
        .await
        .unwrap();

    assert_eq!(
        observation,
        FaultObservation::expected(EffectFaultPoint::AfterDispatchBeforeReceipt)
    );
}

#[tokio::test]
async fn process_runtime_fails_closed_on_non_zero_child_exit() {
    #[cfg(windows)]
    let script = "exit 23";
    #[cfg(not(windows))]
    let script = "exit 23";

    let error = process_runtime(script, Duration::from_secs(2))
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterIntentPersistence,
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Unavailable(message) if message.contains("exit")));
}

#[tokio::test]
async fn process_runtime_fails_closed_on_timeout() {
    #[cfg(windows)]
    let script = "Start-Sleep -Seconds 2";
    #[cfg(not(windows))]
    let script = "sleep 2";

    let error = process_runtime(script, Duration::from_millis(20))
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterIntentPersistence,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ApplicationError::Unavailable(message) if message.contains("timed out"))
    );
}

#[tokio::test]
async fn process_runtime_rejects_malformed_observation_output() {
    #[cfg(windows)]
    let script = "Write-Output not-json";
    #[cfg(not(windows))]
    let script = "printf '%s' not-json";

    let error = process_runtime(script, Duration::from_secs(2))
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterIntentPersistence,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message)) if message.contains("JSON"))
    );
}

#[test]
fn process_runtime_rejects_invalid_process_configuration() {
    assert!(matches!(
        ProcessFaultInjectionRuntime::new(
            settings(true),
            "   ",
            std::iter::empty::<&str>(),
            Duration::from_secs(1),
        ),
        Err(ApplicationError::InvalidConfiguration(message)) if message.contains("program")
    ));
    assert!(matches!(
        ProcessFaultInjectionRuntime::new(
            settings(true),
            "fault-helper",
            std::iter::empty::<&str>(),
            Duration::ZERO,
        ),
        Err(ApplicationError::InvalidConfiguration(message)) if message.contains("timeout")
    ));
}
