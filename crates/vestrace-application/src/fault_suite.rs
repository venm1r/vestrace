use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::external_effects::{
    EffectFaultPoint, FaultObservation, FaultSuiteDecision, evaluate_fault_suite,
};

use crate::ApplicationError;

#[async_trait]
pub trait EffectFaultScenarioExecutor: Send + Sync {
    async fn execute(&self, point: EffectFaultPoint) -> Result<FaultObservation, ApplicationError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalEffectFaultSuiteReport {
    observations: Vec<FaultObservation>,
    decision: FaultSuiteDecision,
}

impl ExternalEffectFaultSuiteReport {
    pub fn observations(&self) -> &[FaultObservation] {
        &self.observations
    }

    pub fn decision(&self) -> &FaultSuiteDecision {
        &self.decision
    }
}

pub struct ExternalEffectFaultSuiteService {
    executor: Arc<dyn EffectFaultScenarioExecutor>,
}

impl ExternalEffectFaultSuiteService {
    pub fn new(executor: Arc<dyn EffectFaultScenarioExecutor>) -> Self {
        Self { executor }
    }

    pub async fn run(&self) -> Result<ExternalEffectFaultSuiteReport, ApplicationError> {
        let mut observations = Vec::with_capacity(EffectFaultPoint::required_points().len());
        for point in EffectFaultPoint::required_points() {
            let observation = self.executor.execute(point).await?;
            if observation.point != point {
                return Err(vestrace_domain::DomainError::InvalidArgument(format!(
                    "fault executor returned observation for {:?} while {:?} was requested",
                    observation.point, point
                ))
                .into());
            }
            observations.push(observation);
        }

        let decision = evaluate_fault_suite(&observations);
        Ok(ExternalEffectFaultSuiteReport {
            observations,
            decision,
        })
    }
}
