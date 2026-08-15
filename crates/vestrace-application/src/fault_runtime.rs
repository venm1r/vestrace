use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};

use crate::{ApplicationError, EffectFaultScenarioExecutor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultInjectionEnvironment {
    Ephemeral,
    DesignatedNonProduction,
    ProviderSandbox,
}

/// Where a fault scenario executes. The driver is configuration, not something
/// a runtime infers: a runtime that does not match the configured driver
/// refuses to run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FaultInjectionDriver {
    /// A child process on the host, isolated only by the process boundary.
    #[default]
    Process,
    /// A disposable container, isolated from the host network.
    Container,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultInjectionSettings {
    enabled: bool,
    target_digest: String,
    environment: FaultInjectionEnvironment,
    driver: FaultInjectionDriver,
}

impl FaultInjectionSettings {
    pub fn new(
        enabled: bool,
        target_digest: impl Into<String>,
        environment: FaultInjectionEnvironment,
    ) -> Result<Self, ApplicationError> {
        let target_digest = target_digest.into();
        if target_digest.trim().is_empty() {
            return Err(vestrace_domain::DomainError::InvalidArgument(
                "fault injection target digest must not be blank".into(),
            )
            .into());
        }
        Ok(Self {
            enabled,
            target_digest,
            environment,
            driver: FaultInjectionDriver::Process,
        })
    }

    pub fn with_driver(mut self, driver: FaultInjectionDriver) -> Result<Self, ApplicationError> {
        self.driver = driver;
        Ok(self)
    }

    pub fn driver(&self) -> FaultInjectionDriver {
        self.driver
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn target_digest(&self) -> &str {
        &self.target_digest
    }

    pub fn environment(&self) -> FaultInjectionEnvironment {
        self.environment
    }
}

#[async_trait]
pub trait FaultInjectionRuntime: Send + Sync {
    async fn execute(
        &self,
        target_digest: &str,
        point: EffectFaultPoint,
    ) -> Result<FaultObservation, ApplicationError>;
}

pub struct ConfiguredEffectFaultScenarioExecutor {
    settings: FaultInjectionSettings,
    runtime: Arc<dyn FaultInjectionRuntime>,
}

pub struct ProcessFaultInjectionRuntime {
    settings: FaultInjectionSettings,
    program: String,
    args: Vec<String>,
    timeout: Duration,
}

impl ProcessFaultInjectionRuntime {
    pub fn new<I, S>(
        settings: FaultInjectionSettings,
        program: impl Into<String>,
        args: I,
        timeout: Duration,
    ) -> Result<Self, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let program = program.into();
        if program.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "fault runtime program must not be blank".into(),
            ));
        }
        if timeout.is_zero() {
            return Err(ApplicationError::InvalidConfiguration(
                "fault runtime timeout must be positive".into(),
            ));
        }
        Ok(Self {
            settings,
            program,
            args: args.into_iter().map(Into::into).collect(),
            timeout,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessFaultObservation {
    point: String,
    status: String,
    retry_attempted: bool,
    reconciliation_started: bool,
    receipt_persisted: bool,
}

impl ProcessFaultObservation {
    fn parse(
        self,
        requested_point: EffectFaultPoint,
    ) -> Result<FaultObservation, ApplicationError> {
        let point = parse_fault_point(&self.point)?;
        if point != requested_point {
            return Err(vestrace_domain::DomainError::InvalidArgument(format!(
                "fault process returned observation for {:?} while {:?} was requested",
                point, requested_point
            ))
            .into());
        }
        let status = parse_effect_status(&self.status)?;
        Ok(FaultObservation {
            point,
            status,
            retry_attempted: self.retry_attempted,
            reconciliation_started: self.reconciliation_started,
            receipt_persisted: self.receipt_persisted,
        })
    }
}

fn parse_fault_point(value: &str) -> Result<EffectFaultPoint, ApplicationError> {
    match value {
        "after_intent_persistence" => Ok(EffectFaultPoint::AfterIntentPersistence),
        "after_authorization_before_dispatch" => {
            Ok(EffectFaultPoint::AfterAuthorizationBeforeDispatch)
        }
        "after_dispatch_before_receipt" => Ok(EffectFaultPoint::AfterDispatchBeforeReceipt),
        "after_receipt_before_outcome_confirmation" => {
            Ok(EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation)
        }
        "after_outcome_before_run_commit" => Ok(EffectFaultPoint::AfterOutcomeBeforeRunCommit),
        other => Err(vestrace_domain::DomainError::InvalidArgument(format!(
            "fault process returned unknown point {other:?}"
        ))
        .into()),
    }
}

fn parse_effect_status(
    value: &str,
) -> Result<vestrace_domain::external_effects::EffectLifecycleStatus, ApplicationError> {
    match value {
        "prepared" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Prepared),
        "authorized" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Authorized),
        "dispatching" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Dispatching),
        "acknowledged" => {
            Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Acknowledged)
        }
        "failed" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Failed),
        "unknown" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Unknown),
        "confirmed" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Confirmed),
        "reconciling" => Ok(vestrace_domain::external_effects::EffectLifecycleStatus::Reconciling),
        other => Err(vestrace_domain::DomainError::InvalidArgument(format!(
            "fault process returned unknown lifecycle status {other:?}"
        ))
        .into()),
    }
}

fn fault_point_name(point: EffectFaultPoint) -> &'static str {
    match point {
        EffectFaultPoint::AfterIntentPersistence => "after_intent_persistence",
        EffectFaultPoint::AfterAuthorizationBeforeDispatch => "after_authorization_before_dispatch",
        EffectFaultPoint::AfterDispatchBeforeReceipt => "after_dispatch_before_receipt",
        EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation => {
            "after_receipt_before_outcome_confirmation"
        }
        EffectFaultPoint::AfterOutcomeBeforeRunCommit => "after_outcome_before_run_commit",
    }
}

/// `ProviderSandbox` names an isolation guarantee that only a provider can
/// give. Neither a host process nor a locally started container is a provider's
/// sandbox, so executing there while claiming that environment would record a
/// false isolation claim in the qualification evidence. Until a provider
/// sandbox driver exists, the claim is refused rather than approximated.
fn reject_unbacked_provider_sandbox(
    settings: &FaultInjectionSettings,
) -> Result<(), ApplicationError> {
    if settings.environment() == FaultInjectionEnvironment::ProviderSandbox {
        return Err(ApplicationError::Policy(
            "no provider sandbox fault driver is available; refusing to claim provider sandbox isolation"
                .into(),
        ));
    }
    Ok(())
}

fn isolation_name(environment: FaultInjectionEnvironment) -> &'static str {
    match environment {
        FaultInjectionEnvironment::Ephemeral => "ephemeral",
        FaultInjectionEnvironment::DesignatedNonProduction => "designated_non_production",
        FaultInjectionEnvironment::ProviderSandbox => "provider_sandbox",
    }
}

/// Run a fault command under the shared contract: a cleared environment
/// carrying only the fault variables, a hard timeout, and strict structured
/// output. Both the process and container runtimes go through here so their
/// observation contract cannot drift apart.
async fn execute_fault_command(
    program: &str,
    args: &[String],
    settings: &FaultInjectionSettings,
    target_digest: &str,
    point: EffectFaultPoint,
    deadline: Duration,
    preserve_env: &[&str],
) -> Result<FaultObservation, ApplicationError> {
    let mut command = Command::new(program);
    command.args(args).env_clear();
    for name in preserve_env {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let child = command
        .env("VESTRACE_FAULT_TARGET_DIGEST", target_digest)
        .env("VESTRACE_FAULT_POINT", fault_point_name(point))
        .env(
            "VESTRACE_FAULT_ISOLATION",
            isolation_name(settings.environment()),
        )
        .kill_on_drop(true)
        .output();
    let output = match timeout(deadline, child).await {
        Ok(result) => result.map_err(|error| {
            ApplicationError::Unavailable(format!("fault process could not be started: {error}"))
        })?,
        Err(_) => {
            return Err(ApplicationError::Unavailable(
                "fault process timed out".into(),
            ));
        }
    };
    if !output.status.success() {
        return Err(ApplicationError::Unavailable(format!(
            "fault process exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let observation: ProcessFaultObservation =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            vestrace_domain::DomainError::InvalidArgument(format!(
                "fault process output is not valid JSON: {error}"
            ))
        })?;
    observation.parse(point)
}

#[cfg(windows)]
const HOST_ENV_ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "Path"];
#[cfg(not(windows))]
const HOST_ENV_ALLOWLIST: &[&str] = &[];

/// The container runtime additionally needs enough host environment to locate
/// and reach the container daemon.
#[cfg(windows)]
const CONTAINER_ENV_ALLOWLIST: &[&str] = &[
    "SystemRoot",
    "WINDIR",
    "Path",
    "USERPROFILE",
    "DOCKER_HOST",
    "DOCKER_CONFIG",
];
#[cfg(not(windows))]
const CONTAINER_ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "DOCKER_HOST", "DOCKER_CONFIG"];

#[async_trait]
impl FaultInjectionRuntime for ProcessFaultInjectionRuntime {
    async fn execute(
        &self,
        target_digest: &str,
        point: EffectFaultPoint,
    ) -> Result<FaultObservation, ApplicationError> {
        if target_digest != self.settings.target_digest() {
            return Err(ApplicationError::Policy(
                "fault process target does not match configured target".into(),
            ));
        }
        reject_unbacked_provider_sandbox(&self.settings)?;

        execute_fault_command(
            &self.program,
            &self.args,
            &self.settings,
            target_digest,
            point,
            self.timeout,
            HOST_ENV_ALLOWLIST,
        )
        .await
    }
}

/// Executes a fault scenario inside a disposable, network-isolated container.
///
/// The container is started with `--rm` and `--network none`, and the fault
/// contract reaches it through forwarded environment variables rather than
/// arguments, so the same strict observation format applies as for the process
/// runtime.
pub struct DockerFaultInjectionRuntime {
    settings: FaultInjectionSettings,
    program: String,
    program_args: Vec<String>,
    image: String,
    image_args: Vec<String>,
    timeout: Duration,
}

impl DockerFaultInjectionRuntime {
    pub fn new<I, S>(
        settings: FaultInjectionSettings,
        image: impl Into<String>,
        image_args: I,
        timeout: Duration,
    ) -> Result<Self, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let image = image.into();
        if image.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "fault container image must not be blank".into(),
            ));
        }
        if timeout.is_zero() {
            return Err(ApplicationError::InvalidConfiguration(
                "fault runtime timeout must be positive".into(),
            ));
        }
        Ok(Self {
            settings,
            program: "docker".to_owned(),
            program_args: Vec::new(),
            image,
            image_args: image_args.into_iter().map(Into::into).collect(),
            timeout,
        })
    }

    /// Override the container command. Deployments use this to select
    /// `podman` or an absolute `docker` path; tests use it to substitute a stub.
    pub fn with_container_command<I, S>(
        mut self,
        program: impl Into<String>,
        args: I,
    ) -> Result<Self, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let program = program.into();
        if program.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "fault container command must not be blank".into(),
            ));
        }
        self.program = program;
        self.program_args = args.into_iter().map(Into::into).collect();
        Ok(self)
    }

    fn container_args(&self) -> Vec<String> {
        let mut args = self.program_args.clone();
        args.extend(
            [
                "run",
                "--rm",
                "--network",
                "none",
                "-e",
                "VESTRACE_FAULT_TARGET_DIGEST",
                "-e",
                "VESTRACE_FAULT_POINT",
                "-e",
                "VESTRACE_FAULT_ISOLATION",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        args.push(self.image.clone());
        args.extend(self.image_args.iter().cloned());
        args
    }
}

#[async_trait]
impl FaultInjectionRuntime for DockerFaultInjectionRuntime {
    async fn execute(
        &self,
        target_digest: &str,
        point: EffectFaultPoint,
    ) -> Result<FaultObservation, ApplicationError> {
        if self.settings.driver() != FaultInjectionDriver::Container {
            return Err(ApplicationError::Policy(
                "configured fault driver is not container execution".into(),
            ));
        }
        if target_digest != self.settings.target_digest() {
            return Err(ApplicationError::Policy(
                "fault container target does not match configured target".into(),
            ));
        }
        reject_unbacked_provider_sandbox(&self.settings)?;

        execute_fault_command(
            &self.program,
            &self.container_args(),
            &self.settings,
            target_digest,
            point,
            self.timeout,
            CONTAINER_ENV_ALLOWLIST,
        )
        .await
    }
}

impl ConfiguredEffectFaultScenarioExecutor {
    pub fn new(settings: FaultInjectionSettings, runtime: Arc<dyn FaultInjectionRuntime>) -> Self {
        Self { settings, runtime }
    }
}

#[async_trait]
impl EffectFaultScenarioExecutor for ConfiguredEffectFaultScenarioExecutor {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError> {
        if !self.settings.enabled() {
            return Err(ApplicationError::Policy(
                "deterministic fault injection is disabled".into(),
            ));
        }

        let observation = self
            .runtime
            .execute(self.settings.target_digest(), point)
            .await?;
        if observation.point != point {
            return Err(vestrace_domain::DomainError::InvalidArgument(format!(
                "fault runtime returned observation for {:?} while {:?} was requested",
                observation.point, point
            ))
            .into());
        }
        Ok(observation)
    }
}
