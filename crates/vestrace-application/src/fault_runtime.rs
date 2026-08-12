use std::sync::Arc;

use async_trait::async_trait;
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
