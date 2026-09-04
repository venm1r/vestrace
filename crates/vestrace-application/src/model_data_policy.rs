use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::id::{AgentRunId, RunStepId};
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{DataDestination, Sensitivity, time::Timestamp};

use crate::{ApplicationError, UnitOfWork};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelDataPolicyMode {
    Enforce,
    Observe,
}

#[derive(Clone, Debug)]
pub struct ModelDataPolicySettings {
    pub policy: DataPolicy,
    /// The deployment-declared sensitivity floor for every run objective.
    pub classification: Sensitivity,
    pub mode: ModelDataPolicyMode,
}

/// The decision committed before a model request is allowed to leave.
///
/// This is intentionally richer than the domain's binary decision: observe
/// mode still needs to preserve a denied verdict together with the fact that
/// the deployment proceeded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelDataPolicyDecisionRecord {
    pub id: Uuid,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub destination: DataDestination,
    pub classification: Sensitivity,
    pub allowed: bool,
    pub reason: String,
    pub policy_version: String,
    pub mode: ModelDataPolicyMode,
    pub decided_at: Timestamp,
}

#[async_trait]
pub trait ModelDataPolicyDecisionRepository: Send + Sync {
    async fn record(&self, record: &ModelDataPolicyDecisionRecord) -> Result<(), ApplicationError>;

    /// Persist the decision in a caller-owned transaction. This is the only
    /// entry point used while preparing an atomic provider dispatch.
    async fn record_in(
        &self,
        _unit_of_work: &mut dyn UnitOfWork,
        _record: &ModelDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Internal(
            "transaction-bound model-data-policy persistence is unsupported".to_owned(),
        ))
    }
}

pub type SharedModelDataPolicyDecisionRepository = Arc<dyn ModelDataPolicyDecisionRepository>;

#[cfg(test)]
mod transaction_bound_api_contract {
    use super::*;
    #[test]
    fn model_data_policy_repository_exposes_caller_owned_record() {
        async fn type_check(
            repository: &dyn ModelDataPolicyDecisionRepository,
            unit_of_work: &mut dyn UnitOfWork,
            record: &ModelDataPolicyDecisionRecord,
        ) -> Result<(), ApplicationError> {
            repository.record_in(unit_of_work, record).await
        }

        let _ = type_check;
    }
}
