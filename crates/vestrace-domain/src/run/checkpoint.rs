use crate::{
    DomainError,
    id::{
        AgentRunId, BudgetSnapshotId, PlanRevisionId, RunCheckpointId, RunReferenceId, RunStepId,
        WorkItemId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::step::RunReference;
use super::{ResumeCursor, RunVersion};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCheckpointPayloadV1 {
    pub completed_steps: Vec<RunStepId>,
    pub ready_steps: Vec<RunStepId>,
    pub pending_work_items: Vec<WorkItemId>,
    pub pending_tool_calls: Vec<RunReferenceId>,
    pub pending_approvals: Vec<RunReferenceId>,
    pub active_subruns: Vec<AgentRunId>,
    pub context_references: Vec<RunReference>,
    pub artifact_references: Vec<RunReference>,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "version")]
pub enum RunCheckpointPayload {
    V1(RunCheckpointPayloadV1),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCheckpoint {
    pub id: RunCheckpointId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub run_version: RunVersion,
    pub active_plan_revision_id: Option<PlanRevisionId>,
    pub resume_cursor: ResumeCursor,
    pub payload: RunCheckpointPayload,
    pub created_at: Timestamp,
}

impl RunCheckpoint {
    pub fn new(
        id: RunCheckpointId,
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
        run_version: RunVersion,
        active_plan_revision_id: Option<PlanRevisionId>,
        payload: RunCheckpointPayload,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let resume_cursor = ResumeCursor::from_version(run_version);
        Self::validate_payload(&payload)?;
        Ok(Self {
            id,
            workspace_id,
            run_id,
            run_version,
            active_plan_revision_id,
            resume_cursor,
            payload,
            created_at,
        })
    }

    fn validate_payload(payload: &RunCheckpointPayload) -> Result<(), DomainError> {
        match payload {
            RunCheckpointPayload::V1(v1) => {
                check_no_duplicates("completed_steps", &v1.completed_steps)?;
                check_no_duplicates("ready_steps", &v1.ready_steps)?;
                check_no_duplicates("pending_work_items", &v1.pending_work_items)?;
                check_no_duplicates("pending_tool_calls", &v1.pending_tool_calls)?;
                check_no_duplicates("pending_approvals", &v1.pending_approvals)?;
                check_no_duplicates("active_subruns", &v1.active_subruns)?;
                Ok(())
            }
        }
    }

    pub fn resume_cursor(&self) -> ResumeCursor {
        self.resume_cursor
    }
}

fn check_no_duplicates<T: PartialEq + std::fmt::Debug>(
    field_name: &str,
    items: &[T],
) -> Result<(), DomainError> {
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            if items[i] == items[j] {
                return Err(DomainError::InvalidArgument(format!(
                    "checkpoint {field_name} contains duplicate IDs"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn valid_payload() -> RunCheckpointPayload {
        RunCheckpointPayload::V1(RunCheckpointPayloadV1 {
            completed_steps: vec![RunStepId::new()],
            ready_steps: vec![],
            pending_work_items: vec![],
            pending_tool_calls: vec![],
            pending_approvals: vec![],
            active_subruns: vec![],
            context_references: vec![],
            artifact_references: vec![],
            budget_snapshot_id: None,
        })
    }

    #[test]
    fn valid_checkpoint_is_accepted() {
        let result = RunCheckpoint::new(
            RunCheckpointId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            RunVersion::new(3).unwrap(),
            None,
            valid_payload(),
            now(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn duplicate_completed_steps_rejected() {
        let id = RunStepId::new();
        let mut payload = valid_payload();
        if let RunCheckpointPayload::V1(v1) = &mut payload {
            v1.completed_steps = vec![id, id];
        }
        let result = RunCheckpoint::new(
            RunCheckpointId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            RunVersion::new(3).unwrap(),
            None,
            payload,
            now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn resume_cursor_equals_version() {
        let checkpoint = RunCheckpoint::new(
            RunCheckpointId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            RunVersion::new(5).unwrap(),
            None,
            valid_payload(),
            now(),
        )
        .unwrap();
        assert_eq!(checkpoint.resume_cursor.value(), 5);
    }
}
