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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultInjectionSettings {
    enabled: bool,
    target_digest: String,
    environment: FaultInjectionEnvironment,
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
        })
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

        let mut command = Command::new(&self.program);
        command.args(&self.args).env_clear();
        #[cfg(windows)]
        {
            for name in ["SystemRoot", "WINDIR", "Path"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
        let child = command
            .env("VESTRACE_FAULT_TARGET_DIGEST", target_digest)
            .env("VESTRACE_FAULT_POINT", fault_point_name(point))
            .env(
                "VESTRACE_FAULT_ISOLATION",
                match self.settings.environment() {
                    FaultInjectionEnvironment::Ephemeral => "ephemeral",
                    FaultInjectionEnvironment::DesignatedNonProduction => {
                        "designated_non_production"
                    }
                    FaultInjectionEnvironment::ProviderSandbox => "provider_sandbox",
                },
            )
            .kill_on_drop(true)
            .output();
        let output = match timeout(self.timeout, child).await {
            Ok(result) => result.map_err(|error| {
                ApplicationError::Unavailable(format!(
                    "fault process could not be started: {error}"
                ))
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
