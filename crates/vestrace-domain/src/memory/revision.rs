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
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub change_reason: Option<String>,
    pub canonical_hash: Option<String>,
    pub classification: Option<String>,
}

impl MemoryRevision {
    pub fn validate_temporal_range(&self) -> Result<(), DomainError> {
        if let (Some(from), Some(until)) = (self.valid_from, self.valid_until) {
            if until < from {
                return Err(DomainError::InvalidArgument(format!(
                    "valid_until ({}) must not precede valid_from ({})",
                    until, from
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Memory {
    pub id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub active_revision_id: Option<MemoryRevisionId>,
    pub state_revision: u32,
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
            state_revision: 0,
            created_at: at,
            updated_at: at,
        }
    }

    /// Make `revision` the active one.
    ///
    /// Takes the revision rather than its id so that ownership can be checked.
    /// The previous signature accepted a bare `MemoryRevisionId` and could not
    /// verify anything about it, so a memory could be pointed at a revision
    /// belonging to another memory — or another workspace — and the domain
    /// would accept it. Reading that memory would then return content that was
    /// never written to it.
    pub fn activate(
        mut self,
        revision: &MemoryRevision,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if revision.memory_id != self.id {
            return Err(DomainError::InvalidArgument(
                "active revision must belong to the same memory".into(),
            ));
        }
        if revision.workspace_id != self.workspace_id {
            return Err(DomainError::PolicyViolation(
                "active revision must belong to the same workspace".into(),
            ));
        }

        match self.status {
            MemoryStatus::Candidate | MemoryStatus::Active => {
                self.status = MemoryStatus::Active;
                self.active_revision_id = Some(revision.id);
                self.state_revision += 1;
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
            self.state_revision += 1;
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
            self.state_revision += 1;
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
            self.state_revision += 1;
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
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        }
    }
}
