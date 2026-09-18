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

    assert_eq!(observation.status, EffectLifecycleStatus::Reconciling);
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
$status = if ($env:VESTRACE_FAULT_POINT -eq 'after_dispatch_before_receipt') { 'reconciling' } else { 'prepared' }
$reconciliation = $env:VESTRACE_FAULT_POINT -eq 'after_dispatch_before_receipt'
$o = @{ point = $env:VESTRACE_FAULT_POINT; status = $status; retry_attempted = $false; reconciliation_started = $reconciliation; receipt_persisted = $false }
$o | ConvertTo-Json -Compress
"#;
    #[cfg(not(windows))]
    let script = r#"test "$VESTRACE_FAULT_TARGET_DIGEST" = "sha256:deployment-target" || exit 7; if [ "$VESTRACE_FAULT_POINT" = "after_dispatch_before_receipt" ]; then printf '%s' '{"point":"after_dispatch_before_receipt","status":"reconciling","retry_attempted":false,"reconciliation_started":true,"receipt_persisted":false}'; else printf '%s' '{"point":"after_intent_persistence","status":"prepared","retry_attempted":false,"reconciliation_started":false,"receipt_persisted":false}'; fi"#;

    let runtime = process_runtime(script, Duration::from_secs(10));
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

    let error = process_runtime(script, Duration::from_secs(10))
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

    let error = process_runtime(script, Duration::from_secs(10))
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

// ─── Docker-backed qualification execution ───────────────────────────────────

fn container_settings() -> vestrace_application::FaultInjectionSettings {
    settings(true)
        .with_driver(vestrace_application::FaultInjectionDriver::Container)
        .unwrap()
}

/// The driver a deployment selects is part of its configuration, so a container
/// runtime must refuse to run under settings that select the process driver.
#[tokio::test]
async fn container_runtime_refuses_process_driver_settings() {
    let runtime = vestrace_application::DockerFaultInjectionRuntime::new(
        settings(true),
        "vestrace/fault-helper:1",
        std::iter::empty::<&str>(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = runtime
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterDispatchBeforeReceipt,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ApplicationError::Policy(message) if message.contains("container")),
        "container runtime accepted process-driver settings"
    );
}

/// The container must be isolated and disposable, and it must receive the exact
/// target/point contract the process runtime uses.
#[tokio::test]
async fn container_runtime_runs_an_isolated_disposable_container() {
    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let argv_dump = std::env::temp_dir().join(format!("vestrace-container-argv-{unique}.txt"));

    // The stub stands in for the container command: it records the argv it was
    // invoked with, then emits the observation the real helper image would.
    #[cfg(windows)]
    let (program, args) = {
        let script = std::env::temp_dir().join(format!("vestrace-container-stub-{unique}.ps1"));
        std::fs::write(
            &script,
            format!(
                "$args -join ' ' | Set-Content -LiteralPath '{}' -Encoding utf8\n\
                 if ($env:VESTRACE_FAULT_TARGET_DIGEST -ne 'sha256:deployment-target') {{ exit 24 }}\n\
                 $o = @{{ point = $env:VESTRACE_FAULT_POINT; status = 'unknown'; retry_attempted = $false; reconciliation_started = $true; receipt_persisted = $false }}\n\
                 $o | ConvertTo-Json -Compress\n",
                argv_dump.display()
            ),
        )
        .unwrap();
        (
            "powershell.exe".to_owned(),
            vec![
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-File".to_owned(),
                script.to_string_lossy().into_owned(),
            ],
        )
    };
    #[cfg(not(windows))]
    let (program, args) = {
        let script = std::env::temp_dir().join(format!("vestrace-container-stub-{unique}.sh"));
        std::fs::write(
            &script,
            format!(
                "printf '%s' \"$*\" > '{}'\n\
                 test \"$VESTRACE_FAULT_TARGET_DIGEST\" = \"sha256:deployment-target\" || exit 24\n\
                 printf '%s' \"{{\\\"point\\\":\\\"$VESTRACE_FAULT_POINT\\\",\\\"status\\\":\\\"unknown\\\",\\\"retry_attempted\\\":false,\\\"reconciliation_started\\\":true,\\\"receipt_persisted\\\":false}}\"\n",
                argv_dump.display()
            ),
        )
        .unwrap();
        ("sh".to_owned(), vec![script.to_string_lossy().into_owned()])
    };

    let runtime = vestrace_application::DockerFaultInjectionRuntime::new(
        container_settings(),
        "vestrace/fault-helper:1",
        std::iter::empty::<&str>(),
        Duration::from_secs(10),
    )
    .unwrap()
    .with_container_command(program, args)
    .unwrap();

    let observation = runtime
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterDispatchBeforeReceipt,
        )
        .await
        .unwrap();

    assert_eq!(
        observation.point,
        EffectFaultPoint::AfterDispatchBeforeReceipt
    );
    assert_eq!(observation.status, EffectLifecycleStatus::Unknown);
    assert!(observation.reconciliation_started);
    assert!(!observation.retry_attempted);

    let argv = std::fs::read_to_string(&argv_dump).unwrap();
    assert!(argv.contains("run"), "argv: {argv}");
    assert!(argv.contains("--rm"), "container is not disposable: {argv}");
    assert!(
        argv.contains("--network none"),
        "container is not network-isolated: {argv}"
    );
    assert!(
        argv.contains("vestrace/fault-helper:1"),
        "configured image was not used: {argv}"
    );
    for forwarded in [
        "VESTRACE_FAULT_TARGET_DIGEST",
        "VESTRACE_FAULT_POINT",
        "VESTRACE_FAULT_ISOLATION",
    ] {
        assert!(
            argv.contains(forwarded),
            "{forwarded} is not forwarded into the container: {argv}"
        );
    }
    let _ = std::fs::remove_file(&argv_dump);
}

#[test]
fn container_runtime_rejects_invalid_configuration() {
    assert!(matches!(
        vestrace_application::DockerFaultInjectionRuntime::new(
            container_settings(),
            "   ",
            std::iter::empty::<&str>(),
            Duration::from_secs(1),
        ),
        Err(ApplicationError::InvalidConfiguration(message)) if message.contains("image")
    ));
    assert!(matches!(
        vestrace_application::DockerFaultInjectionRuntime::new(
            container_settings(),
            "vestrace/fault-helper:1",
            std::iter::empty::<&str>(),
            Duration::ZERO,
        ),
        Err(ApplicationError::InvalidConfiguration(message)) if message.contains("timeout")
    ));
}

#[test]
fn fault_injection_settings_default_to_the_process_driver() {
    assert_eq!(
        settings(true).driver(),
        vestrace_application::FaultInjectionDriver::Process
    );
    assert_eq!(
        container_settings().driver(),
        vestrace_application::FaultInjectionDriver::Container
    );
}

/// `ProviderSandbox` names an isolation guarantee that no driver in this
/// repository provides. A host process and a local container are not a
/// provider's sandbox, so claiming that isolation while executing on either one
/// would be a false claim. Until a provider sandbox driver exists, both
/// runtimes refuse it rather than silently running somewhere else.
#[tokio::test]
async fn runtimes_refuse_an_unbacked_provider_sandbox_claim() {
    let settings = FaultInjectionSettings::new(
        true,
        "sha256:deployment-target",
        FaultInjectionEnvironment::ProviderSandbox,
    )
    .unwrap();

    let process = ProcessFaultInjectionRuntime::new(
        settings.clone(),
        "fault-helper",
        std::iter::empty::<&str>(),
        Duration::from_secs(2),
    )
    .unwrap();
    let process_error = process
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterDispatchBeforeReceipt,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(process_error, ApplicationError::Policy(ref message) if message.contains("provider sandbox")),
        "unexpected process outcome: {process_error:?}"
    );

    let container = vestrace_application::DockerFaultInjectionRuntime::new(
        settings
            .with_driver(vestrace_application::FaultInjectionDriver::Container)
            .unwrap(),
        "vestrace/fault-helper:1",
        std::iter::empty::<&str>(),
        Duration::from_secs(2),
    )
    .unwrap();
    let container_error = container
        .execute(
            "sha256:deployment-target",
            EffectFaultPoint::AfterDispatchBeforeReceipt,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(container_error, ApplicationError::Policy(ref message) if message.contains("provider sandbox")),
        "unexpected container outcome: {container_error:?}"
    );
}
