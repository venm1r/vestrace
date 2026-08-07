pub mod status;

pub use status::ExecutionStatus;

use crate::{
    DomainError,
    id::{
        AgentId, ExecutionArtifactId, ExecutionOutcomeId, ModelExecutionAttemptId, StepExecutionId,
        ToolInvocationId, WorkflowExecutionId, WorkflowId, WorkflowNodeId, WorkflowRevisionId,
        WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowExecution {
    pub id: WorkflowExecutionId,
    pub workspace_id: WorkspaceId,
    pub workflow_id: WorkflowId,
    pub workflow_revision: u32,
    pub workflow_revision_id: WorkflowRevisionId,
    pub status: ExecutionStatus,
    pub attempt: u32,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
}

impl WorkflowExecution {
    pub fn transition_to(
        &mut self,
        next: ExecutionStatus,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        self.status.transition_to(next)?;
        self.status = next;
        if next.is_terminal() {
            self.completed_at = Some(at);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Agent,
    Skill,
    Tool,
    Decision,
    Parallel,
    Join,
    HumanApproval,
    SubWorkflow,
    End,
}

impl From<&crate::cognitive::WorkflowNodeKind> for StepKind {
    fn from(k: &crate::cognitive::WorkflowNodeKind) -> Self {
        use crate::cognitive::WorkflowNodeKind::*;
        match k {
            Agent => Self::Agent,
            Skill => Self::Skill,
            Tool => Self::Tool,
            Decision => Self::Decision,
            Parallel => Self::Parallel,
            Join => Self::Join,
            HumanApproval => Self::HumanApproval,
            SubWorkflow => Self::SubWorkflow,
            End => Self::End,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StepExecution {
    pub id: StepExecutionId,
    pub workflow_execution_id: WorkflowExecutionId,
    pub workspace_id: WorkspaceId,
    pub node_id: WorkflowNodeId,
    pub node_label: String,
    pub kind: StepKind,
    pub agent_ref: Option<AgentId>,
    pub attempt: u32,
    pub status: ExecutionStatus,
    pub input_artifact_id: Option<ExecutionArtifactId>,
    pub output_artifact_id: Option<ExecutionArtifactId>,
    pub model_attempt_id: Option<ModelExecutionAttemptId>,
    pub tool_invocation_id: Option<ToolInvocationId>,
    pub error_message: Option<String>,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

impl StepExecution {
    pub fn transition_to(
        &mut self,
        next: ExecutionStatus,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        self.status.transition_to(next)?;
        self.status = next;
        if next.is_terminal() {
            self.completed_at = Some(at);
        }
        Ok(())
    }

    pub fn validate_node_membership(
        &self,
        known_node_ids: &[WorkflowNodeId],
    ) -> Result<(), DomainError> {
        if !known_node_ids.contains(&self.node_id) {
            return Err(DomainError::InvalidArgument(format!(
                "step references node {} which is not in the workflow revision",
                self.node_id
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Input,
    Output,
    Intermediate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecutionArtifact {
    pub id: ExecutionArtifactId,
    pub workspace_id: WorkspaceId,
    pub step_execution_id: StepExecutionId,
    pub kind: ArtifactKind,
    pub content_ref: String,
    pub content_type: String,
    pub byte_size: u64,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Success,
    Failure,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecutionOutcome {
    pub id: ExecutionOutcomeId,
    pub workspace_id: WorkspaceId,
    pub workflow_execution_id: WorkflowExecutionId,
    pub step_execution_id: Option<StepExecutionId>,
    pub outcome_kind: OutcomeKind,
    pub summary: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub created_at: Timestamp,
}

pub fn validate_attempt_uniqueness(
    existing_attempts: &[u32],
    new_attempt: u32,
) -> Result<(), DomainError> {
    if existing_attempts.contains(&new_attempt) {
        return Err(DomainError::InvalidArgument(format!(
            "duplicate attempt number {} for this step",
            new_attempt
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::*;

    fn wf_execution(status: ExecutionStatus) -> WorkflowExecution {
        WorkflowExecution {
            id: WorkflowExecutionId::new(),
            workspace_id: WorkspaceId::new(),
            workflow_id: WorkflowId::new(),
            workflow_revision: 1,
            workflow_revision_id: WorkflowRevisionId::new(),
            status,
            attempt: 1,
            started_at: crate::time::now(),
            completed_at: None,
            correlation_id: None,
            causation_id: None,
        }
    }

    fn step(status: ExecutionStatus, attempt: u32) -> StepExecution {
        StepExecution {
            id: StepExecutionId::new(),
            workflow_execution_id: WorkflowExecutionId::new(),
            workspace_id: WorkspaceId::new(),
            node_id: WorkflowNodeId::new(),
            node_label: "step1".to_owned(),
            kind: StepKind::Agent,
            agent_ref: None,
            attempt,
            status,
            input_artifact_id: None,
            output_artifact_id: None,
            model_attempt_id: None,
            tool_invocation_id: None,
            error_message: None,
            started_at: crate::time::now(),
            completed_at: None,
        }
    }

    #[test]
    fn workflow_transition_sets_completed_at_for_terminal() {
        let mut exec = wf_execution(ExecutionStatus::Running);
        let at = crate::time::now();
        exec.transition_to(ExecutionStatus::Succeeded, at).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Succeeded);
        assert_eq!(exec.completed_at, Some(at));
    }

    #[test]
    fn workflow_transition_rejects_terminal_to_running() {
        let mut exec = wf_execution(ExecutionStatus::Succeeded);
        assert!(
            exec.transition_to(ExecutionStatus::Running, crate::time::now())
                .is_err()
        );
    }

    #[test]
    fn step_transition_rejects_succeeded_to_running() {
        let mut s = step(ExecutionStatus::Succeeded, 1);
        assert!(
            s.transition_to(ExecutionStatus::Running, crate::time::now())
                .is_err()
        );
    }

    #[test]
    fn step_validates_node_membership() {
        let s = step(ExecutionStatus::Running, 1);
        let wrong_node = WorkflowNodeId::new();
        assert!(s.validate_node_membership(&[wrong_node]).is_err());
    }

    #[test]
    fn step_accepts_known_node() {
        let s = step(ExecutionStatus::Running, 1);
        assert!(s.validate_node_membership(&[s.node_id]).is_ok());
    }

    #[test]
    fn duplicate_attempt_rejected() {
        assert!(validate_attempt_uniqueness(&[1, 2], 1).is_err());
        assert!(validate_attempt_uniqueness(&[1, 2], 3).is_ok());
    }
}
