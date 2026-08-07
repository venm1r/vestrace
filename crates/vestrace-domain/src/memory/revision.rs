use crate::{
    Confidence, DomainError, Importance, MemoryKind, MemoryStatus, StructuredMemory,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
    time::Timestamp,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MemoryRevision {
    pub id: MemoryRevisionId,
    pub memory_id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub content: String,
    pub structured: Option<StructuredMemory>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Memory {
    pub id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub active_revision_id: Option<MemoryRevisionId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Memory {
    pub fn new(id: MemoryId, workspace_id: WorkspaceId, kind: MemoryKind, at: Timestamp) -> Self {
        Self {
            id,
            workspace_id,
            kind,
            status: MemoryStatus::Candidate,
            active_revision_id: None,
            created_at: at,
            updated_at: at,
        }
    }

    pub fn activate(
        mut self,
        revision_id: MemoryRevisionId,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        match self.status {
            MemoryStatus::Candidate | MemoryStatus::Active => {
                self.status = MemoryStatus::Active;
                self.active_revision_id = Some(revision_id);
                self.updated_at = at;
                Ok(self)
            }
            MemoryStatus::Superseded
            | MemoryStatus::Rejected
            | MemoryStatus::Expired
            | MemoryStatus::Deleted => Err(DomainError::PolicyViolation(format!(
                "cannot activate memory in status {:?}",
                self.status
            ))),
        }
    }

    pub fn reject(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Candidate {
            self.status = MemoryStatus::Rejected;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot reject memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn supersede(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Active {
            self.status = MemoryStatus::Superseded;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot supersede memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn expire(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Active {
            self.status = MemoryStatus::Expired;
            self.active_revision_id = None;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot expire memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn soft_delete(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Deleted {
            Err(DomainError::PolicyViolation(
                "memory is already deleted".into(),
            ))
        } else {
            self.status = MemoryStatus::Deleted;
            self.active_revision_id = None;
            self.updated_at = at;
            Ok(self)
        }
    }
}
