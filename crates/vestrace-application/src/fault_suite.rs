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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct RecordingExecutor {
        requested: Mutex<Vec<EffectFaultPoint>>,
    }

    #[async_trait]
    impl EffectFaultScenarioExecutor for RecordingExecutor {
        async fn execute(
            &self,
            point: EffectFaultPoint,
        ) -> Result<FaultObservation, ApplicationError> {
            self.requested.lock().unwrap().push(point);
            Ok(FaultObservation::expected(point))
        }
    }

    #[tokio::test]
    async fn external_effect_suite_executes_exactly_the_five_required_points() {
        let executor = Arc::new(RecordingExecutor {
            requested: Mutex::new(Vec::new()),
        });
        let suite = ExternalEffectFaultSuiteService::new(executor.clone());

        let report = suite.run().await.unwrap();

        assert_eq!(report.observations().len(), 5);
        assert_eq!(
            report
                .observations()
                .iter()
                .map(|observation| observation.point)
                .collect::<Vec<_>>(),
            EffectFaultPoint::required_points()
        );
        assert_eq!(
            *executor.requested.lock().unwrap(),
            EffectFaultPoint::required_points()
        );
        assert!(report.decision().is_passed());
    }
}
