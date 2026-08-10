use crate::{
    DomainError,
    id::{
        AgentRunId, AgentRuntimeSnapshotId, BudgetSnapshotId, PlanRevisionId, PrincipalId,
        ResourceUsageSnapshotId, RunCheckpointId, RunReferenceId, RunStepId, WorkerId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::RunVersion;
use super::status::{RunExecutionMode, RunStatus, RunStepStatus};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunTerminalResult {
    Succeeded {
        summary: Option<String>,
    },
    SucceededWithWarnings {
        summary: String,
        warnings: Vec<String>,
    },
    Partial {
        summary: String,
        remaining_work: Vec<String>,
    },
    Failed {
        code: String,
        message: String,
        retryable: bool,
    },
    Cancelled {
        reason: Option<String>,
    },
    Expired,
}

impl RunTerminalResult {
    pub fn from_failure(failure: RunFailure) -> Self {
        Self::Failed {
            code: failure.code,
            message: failure.message,
            retryable: failure.retryable,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunActorRef {
    Principal(PrincipalId),
    AgentSnapshot(AgentRuntimeSnapshotId),
    Worker(WorkerId),
    System,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunReferenceKind {
    Context,
    Artifact,
    Approval,
    ModelInvocation,
    ToolInvocation,
    SubRun,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunReference {
    pub id: RunReferenceId,
    pub kind: RunReferenceKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParentRunLink {
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NewAgentRun {
    pub id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub execution_mode: RunExecutionMode,
    pub parent: Option<ParentRunLink>,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
    pub resource_usage_snapshot_id: Option<ResourceUsageSnapshotId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentRun {
    pub id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub active_plan_revision_id: Option<PlanRevisionId>,
    pub execution_mode: RunExecutionMode,
    pub status: RunStatus,
    pub current_step_id: Option<RunStepId>,
    pub checkpoint_id: Option<RunCheckpointId>,
    pub parent: Option<ParentRunLink>,
    pub root_run_id: AgentRunId,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
    pub resource_usage_snapshot_id: Option<ResourceUsageSnapshotId>,
    pub version: RunVersion,
    pub result: Option<RunTerminalResult>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunStatusChange {
    pub from: RunStatus,
    pub to: RunStatus,
    pub new_version: RunVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunPlanChange {
    pub plan_revision_id: PlanRevisionId,
    pub new_version: RunVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCurrentStepChange {
    pub step_id: Option<RunStepId>,
    pub new_version: RunVersion,
}

const MAX_OBJECTIVE_LEN: usize = 32 * 1024;
const MAX_FIELD_LEN: usize = 32 * 1024;
const MAX_PLAN_REF_LEN: usize = 256;

impl AgentRun {
    pub fn create(input: NewAgentRun, at: Timestamp) -> Result<Self, DomainError> {
        let objective = input.objective.trim();
        if objective.is_empty() {
            return Err(DomainError::InvalidArgument(
                "objective must not be blank".into(),
            ));
        }
        if objective.len() > MAX_OBJECTIVE_LEN {
            return Err(DomainError::InvalidArgument(format!(
                "objective exceeds {MAX_OBJECTIVE_LEN} bytes"
            )));
        }
        if let Some(parent) = &input.parent {
            if parent.parent_run_id == input.id {
                return Err(DomainError::InvalidArgument(
                    "parent run id must differ from run id".into(),
                ));
            }
        }
        let root_run_id = input.id;
        Ok(Self {
            id: input.id,
            workspace_id: input.workspace_id,
            objective: objective.to_owned(),
            coordinator_snapshot_id: input.coordinator_snapshot_id,
            active_plan_revision_id: None,
            execution_mode: input.execution_mode,
            status: RunStatus::Created,
            current_step_id: None,
            checkpoint_id: None,
            parent: input.parent,
            root_run_id,
            budget_snapshot_id: input.budget_snapshot_id,
            resource_usage_snapshot_id: input.resource_usage_snapshot_id,
            version: RunVersion::INITIAL,
            result: None,
            created_at: at,
            updated_at: at,
            finished_at: None,
        })
    }

    pub fn transition(
        &mut self,
        expected: RunVersion,
        target: RunStatus,
        result: Option<RunTerminalResult>,
        at: Timestamp,
    ) -> Result<RunStatusChange, DomainError> {
        if expected != self.version {
            return Err(DomainError::RevisionConflict {
                expected: expected.value(),
                current: self.version.value(),
            });
        }
        if target.is_terminal() && result.is_none() {
            return Err(DomainError::InvalidArgument(format!(
                "terminal target {target:?} requires a result"
            )));
        }
        if !target.is_terminal() && result.is_some() {
            return Err(DomainError::InvalidArgument(format!(
                "non-terminal target {target:?} rejects a result"
            )));
        }
        self.status.transition_to(target)?;
        let from = self.status;
        self.status = target;
        if target.is_terminal() {
            self.result = result;
            self.finished_at = Some(at);
        }
        self.version = self.version.next()?;
        self.updated_at = at;
        Ok(RunStatusChange {
            from,
            to: target,
            new_version: self.version,
        })
    }

    pub fn attach_plan(
        &mut self,
        expected: RunVersion,
        plan: PlanRevisionId,
        at: Timestamp,
    ) -> Result<RunPlanChange, DomainError> {
        if expected != self.version {
            return Err(DomainError::RevisionConflict {
                expected: expected.value(),
                current: self.version.value(),
            });
        }
        self.active_plan_revision_id = Some(plan);
        self.version = self.version.next()?;
        self.updated_at = at;
        Ok(RunPlanChange {
            plan_revision_id: plan,
            new_version: self.version,
        })
    }

    pub fn select_current_step(
        &mut self,
        expected: RunVersion,
        step: Option<RunStepId>,
        at: Timestamp,
    ) -> Result<RunCurrentStepChange, DomainError> {
        if expected != self.version {
            return Err(DomainError::RevisionConflict {
                expected: expected.value(),
                current: self.version.value(),
            });
        }
        self.current_step_id = step;
        self.version = self.version.next()?;
        self.updated_at = at;
        Ok(RunCurrentStepChange {
            step_id: step,
            new_version: self.version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NewRunStep {
    pub id: RunStepId,
    pub run_id: AgentRunId,
    pub plan_step_reference: Option<String>,
    pub assigned_actor: RunActorRef,
    pub input_references: Vec<RunReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunStep {
    pub id: RunStepId,
    pub run_id: AgentRunId,
    pub plan_step_reference: Option<String>,
    pub assigned_actor: RunActorRef,
    pub input_references: Vec<RunReference>,
    pub status: RunStepStatus,
    pub attempt: u32,
    pub output_references: Vec<RunReference>,
    pub error: Option<RunFailure>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

impl RunStep {
    pub fn create(input: NewRunStep, at: Timestamp) -> Result<Self, DomainError> {
        if let Some(ref reference) = input.plan_step_reference {
            let trimmed = reference.trim();
            if trimmed.is_empty() {
                return Err(DomainError::InvalidArgument(
                    "plan_step_reference must not be blank if present".into(),
                ));
            }
            if trimmed.len() > MAX_PLAN_REF_LEN {
                return Err(DomainError::InvalidArgument(format!(
                    "plan_step_reference exceeds {MAX_PLAN_REF_LEN} bytes"
                )));
            }
        }
        Ok(Self {
            id: input.id,
            run_id: input.run_id,
            plan_step_reference: input
                .plan_step_reference
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty()),
            assigned_actor: input.assigned_actor,
            input_references: input.input_references,
            status: RunStepStatus::Pending,
            attempt: 1,
            output_references: Vec::new(),
            error: None,
            created_at: at,
            started_at: None,
            finished_at: None,
        })
    }

    pub fn transition_to(&mut self, next: RunStepStatus, at: Timestamp) -> Result<(), DomainError> {
        self.status.transition_to(next)?;
        let was_terminal = self.status.is_terminal();
        self.status = next;
        if next == RunStepStatus::Running && self.started_at.is_none() {
            self.started_at = Some(at);
        }
        if next.is_terminal() && !was_terminal {
            self.finished_at = Some(at);
        }
        if next.is_terminal() {
            self.error = None;
        }
        Ok(())
    }

    pub fn retry(&mut self, at: Timestamp) -> Result<(), DomainError> {
        if self.status != RunStepStatus::Failed {
            return Err(DomainError::InvalidArgument(
                "retry is only allowed from Failed status".into(),
            ));
        }
        self.attempt = self
            .attempt
            .checked_add(1)
            .ok_or_else(|| DomainError::InvalidArgument("attempt counter overflow".into()))?;
        self.status = RunStepStatus::Ready;
        self.started_at = None;
        self.finished_at = None;
        self.error = None;
        let _ = at;
        Ok(())
    }

    pub fn fail(&mut self, failure: RunFailure, at: Timestamp) -> Result<(), DomainError> {
        let code = failure.code.trim();
        let message = failure.message.trim();
        if code.is_empty() {
            return Err(DomainError::InvalidArgument(
                "failure code must not be blank".into(),
            ));
        }
        if code.len() > MAX_FIELD_LEN {
            return Err(DomainError::InvalidArgument(format!(
                "failure code exceeds {MAX_FIELD_LEN} bytes"
            )));
        }
        if message.len() > MAX_FIELD_LEN {
            return Err(DomainError::InvalidArgument(format!(
                "failure message exceeds {MAX_FIELD_LEN} bytes"
            )));
        }
        self.status.transition_to(RunStepStatus::Failed)?;
        self.status = RunStepStatus::Failed;
        self.error = Some(RunFailure {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable: failure.retryable,
        });
        self.finished_at = Some(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AgentRuntimeSnapshotId, WorkspaceId};

    fn new_run_fixture() -> NewAgentRun {
        NewAgentRun {
            id: AgentRunId::new(),
            workspace_id: WorkspaceId::new(),
            objective: "Summarize the latest changes".into(),
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            execution_mode: RunExecutionMode::Autopilot,
            parent: None,
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
        }
    }

    fn now() -> Timestamp {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn running_run_fixture() -> AgentRun {
        let mut run = AgentRun::create(new_run_fixture(), now()).unwrap();
        run.transition(run.version, RunStatus::Preparing, None, now())
            .unwrap();
        run.transition(run.version, RunStatus::Running, None, now())
            .unwrap();
        run
    }

    #[test]
    fn objective_must_not_be_blank() {
        let mut input = new_run_fixture();
        input.objective = "  ".into();
        let result = AgentRun::create(input, now());
        assert!(result.is_err());
    }

    #[test]
    fn objective_is_trimmed() {
        let mut input = new_run_fixture();
        input.objective = "  Summarize  ".into();
        let run = AgentRun::create(input, now()).unwrap();
        assert_eq!(run.objective, "Summarize");
    }

    #[test]
    fn objective_capped_at_32kib() {
        let mut input = new_run_fixture();
        input.objective = "x".repeat(32 * 1024 + 1);
        let result = AgentRun::create(input, now());
        assert!(result.is_err());
    }

    #[test]
    fn create_sets_version_one_and_status_created() {
        let run = AgentRun::create(new_run_fixture(), now()).unwrap();
        assert_eq!(run.version, RunVersion::INITIAL);
        assert_eq!(run.status, RunStatus::Created);
        assert_eq!(run.root_run_id, run.id);
    }

    #[test]
    fn parent_id_must_differ_from_run_id() {
        let id = AgentRunId::new();
        let mut input = new_run_fixture();
        input.id = id;
        input.parent = Some(ParentRunLink {
            parent_run_id: id,
            parent_step_id: RunStepId::new(),
        });
        let result = AgentRun::create(input, now());
        assert!(result.is_err());
    }

    #[test]
    fn stale_expected_version_is_rejected() {
        let run = running_run_fixture();
        let stale = RunVersion::new(run.version.value() + 1).unwrap();
        let mut run = run;
        let result = run.transition(stale, RunStatus::Paused, None, now());
        assert!(matches!(result, Err(DomainError::RevisionConflict { .. })));
    }

    #[test]
    fn terminal_target_requires_result() {
        let mut run = running_run_fixture();
        let result = run.transition(run.version, RunStatus::Succeeded, None, now());
        assert!(result.is_err());
    }

    #[test]
    fn non_terminal_target_rejects_result() {
        let mut run = running_run_fixture();
        let result = run.transition(
            run.version,
            RunStatus::Paused,
            Some(RunTerminalResult::Expired),
            now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn transition_increments_version() {
        let mut run = running_run_fixture();
        let version_before = run.version;
        run.transition(run.version, RunStatus::Paused, None, now())
            .unwrap();
        assert_eq!(run.version.value(), version_before.value() + 1);
    }

    #[test]
    fn transition_sets_finished_at_for_terminal() {
        let mut run = running_run_fixture();
        let at = now();
        run.transition(
            run.version,
            RunStatus::Succeeded,
            Some(RunTerminalResult::Succeeded { summary: None }),
            at,
        )
        .unwrap();
        assert_eq!(run.finished_at, Some(at));
        assert!(run.result.is_some());
    }

    #[test]
    fn attach_plan_increments_version() {
        let mut run = running_run_fixture();
        let version_before = run.version;
        let plan = PlanRevisionId::new();
        run.attach_plan(run.version, plan, now()).unwrap();
        assert_eq!(run.active_plan_revision_id, Some(plan));
        assert_eq!(run.version.value(), version_before.value() + 1);
    }

    #[test]
    fn select_current_step_increments_version() {
        let mut run = running_run_fixture();
        let version_before = run.version;
        let step = RunStepId::new();
        run.select_current_step(run.version, Some(step), now())
            .unwrap();
        assert_eq!(run.current_step_id, Some(step));
        assert_eq!(run.version.value(), version_before.value() + 1);
    }

    #[test]
    fn step_create_defaults_to_pending_attempt_one() {
        let input = NewRunStep {
            id: RunStepId::new(),
            run_id: AgentRunId::new(),
            plan_step_reference: Some("node-a".into()),
            assigned_actor: RunActorRef::System,
            input_references: Vec::new(),
        };
        let step = RunStep::create(input, now()).unwrap();
        assert_eq!(step.status, RunStepStatus::Pending);
        assert_eq!(step.attempt, 1);
        assert!(step.started_at.is_none());
        assert!(step.finished_at.is_none());
    }

    #[test]
    fn step_plan_reference_trimmed_and_validated() {
        let input = NewRunStep {
            id: RunStepId::new(),
            run_id: AgentRunId::new(),
            plan_step_reference: Some("  ".into()),
            assigned_actor: RunActorRef::System,
            input_references: Vec::new(),
        };
        let result = RunStep::create(input, now());
        assert!(result.is_err());
    }

    #[test]
    fn step_plan_reference_capped_at_256() {
        let input = NewRunStep {
            id: RunStepId::new(),
            run_id: AgentRunId::new(),
            plan_step_reference: Some("x".repeat(257)),
            assigned_actor: RunActorRef::System,
            input_references: Vec::new(),
        };
        let result = RunStep::create(input, now());
        assert!(result.is_err());
    }

    #[test]
    fn step_retry_only_from_failed() {
        let mut step = RunStep::create(
            NewRunStep {
                id: RunStepId::new(),
                run_id: AgentRunId::new(),
                plan_step_reference: None,
                assigned_actor: RunActorRef::System,
                input_references: Vec::new(),
            },
            now(),
        )
        .unwrap();
        assert!(step.retry(now()).is_err());
        step.status = RunStepStatus::Failed;
        assert!(step.retry(now()).is_ok());
        assert_eq!(step.status, RunStepStatus::Ready);
        assert_eq!(step.attempt, 2);
    }

    #[test]
    fn step_fail_requires_non_blank_code() {
        let mut step = RunStep::create(
            NewRunStep {
                id: RunStepId::new(),
                run_id: AgentRunId::new(),
                plan_step_reference: None,
                assigned_actor: RunActorRef::System,
                input_references: Vec::new(),
            },
            now(),
        )
        .unwrap();
        step.status = RunStepStatus::Running;
        let result = step.fail(
            RunFailure {
                code: "  ".into(),
                message: "something went wrong".into(),
                retryable: false,
            },
            now(),
        );
        assert!(result.is_err());
    }
}
