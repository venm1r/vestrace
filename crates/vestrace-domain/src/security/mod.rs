mod capability;
mod delegation;
mod scope;

pub use capability::*;
pub use delegation::*;
pub use scope::*;

use crate::id::AuditEventId;
use crate::{
    DomainError,
    id::{ApprovalRecordId, PrincipalId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    MemoryRead,
    MemoryWrite,
    MemoryPurge,
    EventRead,
    EventWrite,
    ContextRetrieve,
    AgentRead,
    AgentWrite,
    SkillRead,
    SkillWrite,
    WorkflowRead,
    WorkflowWrite,
    ExecutionRead,
    ExecutionWrite,
    ModelRead,
    ModelWrite,
    ProviderRead,
    ProviderWrite,
    EvaluationRead,
    EvaluationWrite,
    LearningRead,
    LearningWrite,
    AuditRead,
    ExportRead,
    CapabilityDelegate,
    WorkspaceAdmin,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::MemoryRead => "memory.read",
            Self::MemoryWrite => "memory.write",
            Self::MemoryPurge => "memory.purge",
            Self::EventRead => "event.read",
            Self::EventWrite => "event.write",
            Self::ContextRetrieve => "context.retrieve",
            Self::AgentRead => "agent.read",
            Self::AgentWrite => "agent.write",
            Self::SkillRead => "skill.read",
            Self::SkillWrite => "skill.write",
            Self::WorkflowRead => "workflow.read",
            Self::WorkflowWrite => "workflow.write",
            Self::ExecutionRead => "execution.read",
            Self::ExecutionWrite => "execution.write",
            Self::ModelRead => "model.read",
            Self::ModelWrite => "model.write",
            Self::ProviderRead => "provider.read",
            Self::ProviderWrite => "provider.write",
            Self::EvaluationRead => "evaluation.read",
            Self::EvaluationWrite => "evaluation.write",
            Self::LearningRead => "learning.read",
            Self::LearningWrite => "learning.write",
            Self::AuditRead => "audit.read",
            Self::ExportRead => "export.read",
            Self::CapabilityDelegate => "capability.delegate",
            Self::WorkspaceAdmin => "workspace.admin",
        };
        write!(f, "{}", name)
    }
}

impl FromStr for Capability {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "memory.read" => Ok(Self::MemoryRead),
            "memory.write" => Ok(Self::MemoryWrite),
            "memory.purge" => Ok(Self::MemoryPurge),
            "event.read" => Ok(Self::EventRead),
            "event.write" => Ok(Self::EventWrite),
            "context.retrieve" => Ok(Self::ContextRetrieve),
            "agent.read" => Ok(Self::AgentRead),
            "agent.write" => Ok(Self::AgentWrite),
            "skill.read" => Ok(Self::SkillRead),
            "skill.write" => Ok(Self::SkillWrite),
            "workflow.read" => Ok(Self::WorkflowRead),
            "workflow.write" => Ok(Self::WorkflowWrite),
            "execution.read" => Ok(Self::ExecutionRead),
            "execution.write" => Ok(Self::ExecutionWrite),
            "model.read" => Ok(Self::ModelRead),
            "model.write" => Ok(Self::ModelWrite),
            "provider.read" => Ok(Self::ProviderRead),
            "provider.write" => Ok(Self::ProviderWrite),
            "evaluation.read" => Ok(Self::EvaluationRead),
            "evaluation.write" => Ok(Self::EvaluationWrite),
            "learning.read" => Ok(Self::LearningRead),
            "learning.write" => Ok(Self::LearningWrite),
            "audit.read" => Ok(Self::AuditRead),
            "export.read" => Ok(Self::ExportRead),
            "capability.delegate" => Ok(Self::CapabilityDelegate),
            "workspace.admin" => Ok(Self::WorkspaceAdmin),
            _ => Err(DomainError::InvalidArgument(format!(
                "unknown capability {}",
                s
            ))),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl Default for Sensitivity {
    fn default() -> Self {
        Self::Confidential
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DataDestination {
    LocalModel,
    RemoteProvider,
    LogOutput,
    AuditStore,
    ExportBundle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    HardPurge,
    PermissionChange,
    ProviderEnablement,
    RestrictedTransfer,
    ExportBundle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Requested,
    AwaitingApproval,
    Approved,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApprovalRecord {
    pub id: ApprovalRecordId,
    pub workspace_id: WorkspaceId,
    pub requestor_id: PrincipalId,
    pub approver_id: Option<PrincipalId>,
    pub kind: ApprovalKind,
    pub status: ApprovalStatus,
    pub reason: String,
    pub operation_hash: Option<String>,
    pub expires_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ApprovalRecord {
    pub fn new(
        id: ApprovalRecordId,
        workspace_id: WorkspaceId,
        requestor_id: PrincipalId,
        kind: ApprovalKind,
        reason: impl Into<String>,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            workspace_id,
            requestor_id,
            approver_id: None,
            kind,
            status: ApprovalStatus::Requested,
            reason: reason.into(),
            operation_hash: None,
            expires_at: None,
            created_at: at,
            updated_at: at,
        }
    }

    pub fn approve(
        mut self,
        approver_id: PrincipalId,
        operation_hash: Option<String>,
        expires_at: Option<Timestamp>,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if self.status != ApprovalStatus::Requested
            && self.status != ApprovalStatus::AwaitingApproval
        {
            return Err(DomainError::PolicyViolation(format!(
                "cannot approve record in status {:?}",
                self.status
            )));
        }
        self.status = ApprovalStatus::Approved;
        self.approver_id = Some(approver_id);
        self.operation_hash = operation_hash;
        self.expires_at = expires_at;
        self.updated_at = at;
        Ok(self)
    }

    pub fn reject(mut self, approver_id: PrincipalId, at: Timestamp) -> Result<Self, DomainError> {
        if self.status != ApprovalStatus::Requested
            && self.status != ApprovalStatus::AwaitingApproval
        {
            return Err(DomainError::PolicyViolation(format!(
                "cannot reject record in status {:?}",
                self.status
            )));
        }
        self.status = ApprovalStatus::Rejected;
        self.approver_id = Some(approver_id);
        self.updated_at = at;
        Ok(self)
    }

    pub fn is_valid(&self, at: Timestamp) -> bool {
        if self.status != ApprovalStatus::Approved {
            return false;
        }
        if let Some(expires) = self.expires_at {
            return at <= expires;
        }
        true
    }

    /// Whether this approval authorises the operation that hashes to
    /// `operation_hash`.
    ///
    /// # Why validity alone was not enough
    ///
    /// `operation_hash` has been recorded at approval time since the type
    /// existed, and **nothing ever compared it**. [`Self::is_valid`] checks
    /// status and expiry and says nothing about *what* was approved, so an
    /// approval obtained for one operation authorised any other — a caller could
    /// get "yes" for a small transfer and present that approval for a large one,
    /// which is the whole of what an approval is supposed to prevent.
    ///
    /// # Why a missing hash covers nothing
    ///
    /// An approval that recorded no operation hash is a blank cheque: there is
    /// no operation it was about, so there is none it can be checked against.
    /// Treating that as "covers everything" is the failure this method exists to
    /// close, so it covers nothing instead. An approver who wants an approval to
    /// be usable has to say what it is for.
    pub fn covers(&self, operation_hash: &str, at: Timestamp) -> bool {
        if !self.is_valid(at) {
            return false;
        }
        match self.operation_hash.as_deref() {
            Some(approved) => !approved.trim().is_empty() && approved == operation_hash,
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub action: String,
    pub resource_type: String,
    pub resource_id: uuid::Uuid,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}

impl AuditEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AuditEventId,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: uuid::Uuid,
        payload: serde_json::Value,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        let action = action.into();
        let resource_type = resource_type.into();
        if action.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "audit action must not be empty".into(),
            ));
        }
        if resource_type.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "audit resource_type must not be empty".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            principal_id,
            action,
            resource_type,
            resource_id,
            payload,
            created_at: at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_round_trips_to_stable_name() {
        let cap: Capability = "memory.read".parse().unwrap();
        assert_eq!(cap.to_string(), "memory.read");
    }

    #[test]
    fn unknown_capability_is_rejected() {
        assert!("root.everything".parse::<Capability>().is_err());
    }

    #[test]
    fn extended_capabilities_round_trip() {
        for name in [
            "agent.read",
            "agent.write",
            "skill.read",
            "skill.write",
            "workflow.read",
            "workflow.write",
            "execution.read",
            "execution.write",
            "model.read",
            "model.write",
            "provider.read",
            "provider.write",
            "evaluation.read",
            "evaluation.write",
            "learning.read",
            "learning.write",
            "audit.read",
            "export.read",
            "capability.delegate",
            "workspace.admin",
        ] {
            let cap: Capability = name.parse().unwrap();
            assert_eq!(cap.to_string(), name);
        }
    }

    #[test]
    fn approval_lifecycle_transitions() {
        let id = ApprovalRecordId::new();
        let ws = WorkspaceId::new();
        let principal = PrincipalId::new();
        let approver = PrincipalId::new();
        let at = crate::now();

        let record = ApprovalRecord::new(id, ws, principal, ApprovalKind::HardPurge, "test", at);
        assert_eq!(record.status, ApprovalStatus::Requested);

        let approved = record
            .approve(approver, Some("hash".to_owned()), None, at)
            .unwrap();
        assert_eq!(approved.status, ApprovalStatus::Approved);
        assert!(approved.is_valid(at));

        let expired = ApprovalRecord::new(id, ws, principal, ApprovalKind::HardPurge, "test", at);
        let expired = expired
            .approve(approver, None, Some(at - chrono::Duration::seconds(1)), at)
            .unwrap();
        assert!(!expired.is_valid(at));
    }

    #[test]
    fn cannot_approve_already_approved() {
        let id = ApprovalRecordId::new();
        let ws = WorkspaceId::new();
        let principal = PrincipalId::new();
        let approver = PrincipalId::new();
        let at = crate::now();

        let record = ApprovalRecord::new(id, ws, principal, ApprovalKind::HardPurge, "test", at);
        let approved = record.approve(approver, None, None, at).unwrap();
        let result = approved.approve(approver, None, None, at);
        assert!(result.is_err());
    }

    #[test]
    fn sensitivity_ordering() {
        assert!(Sensitivity::Restricted > Sensitivity::Confidential);
        assert!(Sensitivity::Confidential > Sensitivity::Internal);
        assert!(Sensitivity::Internal > Sensitivity::Public);
    }

    #[test]
    fn audit_event_rejects_empty_action() {
        let result = AuditEvent::new(
            AuditEventId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "",
            "memory",
            uuid::Uuid::nil(),
            serde_json::Value::Null,
            crate::now(),
        );
        assert!(result.is_err());
    }
}
