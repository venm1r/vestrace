use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    id::{AgentRunId, RunCheckpointId, RunStepId, WorkItemId, WorkspaceId},
    run::RunVersion,
    time::Timestamp,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    AdvanceRun,
    ResumeRun,
    CreateCheckpoint,
    ExpireRun,
    ExecuteStep,
}

impl WorkItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdvanceRun => "advance_run",
            Self::ResumeRun => "resume_run",
            Self::CreateCheckpoint => "create_checkpoint",
            Self::ExpireRun => "expire_run",
            Self::ExecuteStep => "execute_step",
        }
    }

    pub fn parse_kind(s: &str) -> Option<Self> {
        match s {
            "advance_run" => Some(Self::AdvanceRun),
            "resume_run" => Some(Self::ResumeRun),
            "create_checkpoint" => Some(Self::CreateCheckpoint),
            "expire_run" => Some(Self::ExpireRun),
            "execute_step" => Some(Self::ExecuteStep),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Ready,
    Leased,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

impl WorkItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Leased => "leased",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::DeadLetter => "dead_letter",
        }
    }

    pub fn parse_status(s: &str) -> Option<Self> {
        match s {
            "ready" => Some(Self::Ready),
            "leased" => Some(Self::Leased),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "dead_letter" => Some(Self::DeadLetter),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunWorkPayload {
    Advance,
    Resume {
        checkpoint_id: Option<RunCheckpointId>,
    },
    CreateCheckpoint,
    Expire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkItem {
    pub id: WorkItemId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub step_id: Option<RunStepId>,
    pub expected_run_version: RunVersion,
    pub kind: WorkItemKind,
    pub payload: RunWorkPayload,
    pub status: WorkItemStatus,
    pub priority: i16,
    pub available_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub idempotency_key: String,
}

impl WorkItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: WorkItemId,
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
        step_id: Option<RunStepId>,
        expected_run_version: RunVersion,
        kind: WorkItemKind,
        payload: RunWorkPayload,
        priority: i16,
        available_at: Timestamp,
        deadline: Option<Timestamp>,
        max_attempts: u32,
        idempotency_key: String,
    ) -> Result<Self, DomainError> {
        validate_kind_payload_agreement(kind, &payload)?;
        if idempotency_key.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "idempotency_key must not be blank".into(),
            ));
        }
        if max_attempts < 1 {
            return Err(DomainError::InvalidArgument(
                "max_attempts must be >= 1".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            run_id,
            step_id,
            expected_run_version,
            kind,
            payload,
            status: WorkItemStatus::Ready,
            priority,
            available_at,
            deadline,
            attempt: 0,
            max_attempts,
            idempotency_key,
        })
    }
}

fn validate_kind_payload_agreement(
    kind: WorkItemKind,
    payload: &RunWorkPayload,
) -> Result<(), DomainError> {
    let ok = matches!(
        (kind, payload),
        (WorkItemKind::AdvanceRun, RunWorkPayload::Advance)
            | (WorkItemKind::ResumeRun, RunWorkPayload::Resume { .. })
            | (
                WorkItemKind::CreateCheckpoint,
                RunWorkPayload::CreateCheckpoint
            )
            | (WorkItemKind::ExpireRun, RunWorkPayload::Expire)
            | (WorkItemKind::ExecuteStep, RunWorkPayload::Advance)
    );
    if !ok {
        return Err(DomainError::InvalidArgument(format!(
            "work item kind {kind:?} does not match payload {payload:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AgentRunId, WorkItemId, WorkspaceId};
    use crate::run::RunVersion;
    use crate::time::Timestamp;
    use chrono::TimeZone;

    fn ts() -> Timestamp {
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn advance_run_item_valid() {
        let item = WorkItem::new(
            WorkItemId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            None,
            RunVersion::INITIAL,
            WorkItemKind::AdvanceRun,
            RunWorkPayload::Advance,
            0,
            ts(),
            None,
            3,
            "key-1".into(),
        );
        assert!(item.is_ok());
    }

    #[test]
    fn kind_payload_mismatch_rejected() {
        let item = WorkItem::new(
            WorkItemId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            None,
            RunVersion::INITIAL,
            WorkItemKind::AdvanceRun,
            RunWorkPayload::Expire,
            0,
            ts(),
            None,
            3,
            "key-1".into(),
        );
        assert!(item.is_err());
    }

    #[test]
    fn blank_idempotency_key_rejected() {
        let item = WorkItem::new(
            WorkItemId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            None,
            RunVersion::INITIAL,
            WorkItemKind::AdvanceRun,
            RunWorkPayload::Advance,
            0,
            ts(),
            None,
            3,
            "".into(),
        );
        assert!(item.is_err());
    }

    #[test]
    fn zero_max_attempts_rejected() {
        let item = WorkItem::new(
            WorkItemId::new(),
            WorkspaceId::new(),
            AgentRunId::new(),
            None,
            RunVersion::INITIAL,
            WorkItemKind::AdvanceRun,
            RunWorkPayload::Advance,
            0,
            ts(),
            None,
            0,
            "key-1".into(),
        );
        assert!(item.is_err());
    }

    #[test]
    fn round_trip_kind_status_strings() {
        for kind in [
            WorkItemKind::AdvanceRun,
            WorkItemKind::ResumeRun,
            WorkItemKind::CreateCheckpoint,
            WorkItemKind::ExpireRun,
            WorkItemKind::ExecuteStep,
        ] {
            assert_eq!(WorkItemKind::parse_kind(kind.as_str()), Some(kind));
        }
        for status in [
            WorkItemStatus::Ready,
            WorkItemStatus::Leased,
            WorkItemStatus::Completed,
            WorkItemStatus::Failed,
            WorkItemStatus::Cancelled,
            WorkItemStatus::DeadLetter,
        ] {
            assert_eq!(WorkItemStatus::parse_status(status.as_str()), Some(status));
        }
    }
}
