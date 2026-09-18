use vestrace_domain::{
    HealthFinding, RepairExecution, RepairPlan, Repairability, Timestamp,
    health::{RepairAuthorization, RepairExecutionError},
};

use crate::{ApplicationError, InspectedFinding};

#[derive(Clone, Debug)]
pub struct RepairPlanRequest {
    pub finding: HealthFinding,
    pub input_state_ref: String,
    pub operations: Vec<String>,
    pub expected_postconditions: Vec<String>,
    pub verification_checks: Vec<String>,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

#[derive(Clone, Debug)]
pub struct RepairRequest {
    pub plan: RepairPlan,
    pub authorization: RepairAuthorization,
    pub current_state_ref: String,
    pub at: Timestamp,
}

#[derive(Default)]
pub struct HealthOperatorService;

impl HealthOperatorService {
    /// The inspection phase: what was found, unchanged.
    ///
    /// It reads as a no-op because it is one — the phase exists to be a phase.
    /// Naming it keeps the boundary HLT-012 requires visible in the type system:
    /// inspection returns findings and touches nothing, and a caller that wants
    /// to act has to move to `plan` and then `repair`.
    pub fn inspect(&self, findings: Vec<InspectedFinding>) -> Vec<InspectedFinding> {
        findings
    }

    pub fn plan(&self, request: RepairPlanRequest) -> Result<RepairPlan, ApplicationError> {
        if request.finding.repairability() == Repairability::NotRepairable {
            return Err(ApplicationError::Policy("finding is not repairable".into()));
        }
        if request.finding.lifecycle_status() == vestrace_domain::FindingLifecycleStatus::Resolved {
            return Err(ApplicationError::Conflict(
                "resolved finding cannot produce a new repair plan".into(),
            ));
        }
        let required_capability = request.authorization_capability();
        RepairPlan::new(
            vec![request.finding.id()],
            request.input_state_ref,
            request.finding.scope().clone(),
            vec![request.finding.fingerprint().to_owned()],
            request.operations,
            request.expected_postconditions,
            request.verification_checks,
            request.finding.repair_risk(),
            vestrace_domain::health::Reversibility::Restartable,
            required_capability,
            request.created_at,
            request.expires_at,
        )
        .map_err(ApplicationError::from)
    }

    pub fn repair(&self, request: RepairRequest) -> Result<RepairExecution, ApplicationError> {
        request
            .plan
            .start_execution(
                &request.authorization,
                &request.current_state_ref,
                request.at,
            )
            .map_err(|error| match error {
                RepairExecutionError::Unauthorized => {
                    ApplicationError::Policy("repair authorization denied".into())
                }
                RepairExecutionError::StalePlan => ApplicationError::Conflict("STALE_PLAN".into()),
                RepairExecutionError::ExpiredPlan => {
                    ApplicationError::Conflict("repair plan expired".into())
                }
            })
    }
}

impl RepairPlanRequest {
    pub fn authorization_capability(&self) -> vestrace_domain::Capability {
        match self.finding.repairability() {
            Repairability::Auto => vestrace_domain::Capability::MemoryWrite,
            Repairability::Manual => vestrace_domain::Capability::WorkspaceAdmin,
            Repairability::NotRepairable => vestrace_domain::Capability::WorkspaceAdmin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vestrace_domain::{
        Capability, HealthScope, HealthState, InvariantDefinition, RepairRisk, WorkspaceId,
        health::HealthSeverity,
    };

    fn finding() -> HealthFinding {
        let definition = InvariantDefinition::new(
            "health.operator",
            "v1",
            "operator fixture",
            "rebuild the derived state from its source",
            HealthSeverity::Error,
            Repairability::Auto,
            vec![],
        )
        .unwrap();
        HealthFinding::new(
            &definition,
            HealthScope::workspace(WorkspaceId::new()),
            "state-v1",
            HealthState::Unhealthy,
            vec!["evidence:operator".into()],
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn plan_is_derived_from_an_open_finding_and_repair_is_stale_safe() {
        let service = HealthOperatorService;
        let finding = finding();
        let plan = service
            .plan(RepairPlanRequest {
                finding,
                input_state_ref: "state-v1".into(),
                operations: vec!["repair".into()],
                expected_postconditions: vec!["state-v2".into()],
                verification_checks: vec!["verify".into()],
                created_at: Utc::now(),
                expires_at: None,
            })
            .unwrap();
        let error = service
            .repair(RepairRequest {
                plan,
                authorization: RepairAuthorization::allow(
                    Capability::MemoryWrite,
                    "policy-v1",
                    "evidence:operator",
                ),
                current_state_ref: "state-v2".into(),
                at: Utc::now(),
            })
            .unwrap_err();
        assert!(matches!(error, ApplicationError::Conflict(message) if message == "STALE_PLAN"));
        let _ = RepairRisk::Low;
    }
}
