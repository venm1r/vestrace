use crate::{
    DomainError,
    id::{ClaimId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Proposed,
    Supported,
    Contested,
    Superseded,
    Expired,
    Rejected,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Claim {
    pub claim_id: ClaimId,
    pub workspace_id: WorkspaceId,
    pub semantic_key: String,
    pub subject: String,
    pub predicate: String,
    pub value: String,
    pub lifecycle_status: ClaimStatus,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub state_revision: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Claim {
    pub fn new(
        claim_id: ClaimId,
        workspace_id: WorkspaceId,
        semantic_key: String,
        subject: String,
        predicate: String,
        value: String,
        at: Timestamp,
    ) -> Self {
        Self {
            claim_id,
            workspace_id,
            semantic_key,
            subject,
            predicate,
            value,
            lifecycle_status: ClaimStatus::Proposed,
            valid_from: None,
            valid_until: None,
            state_revision: 0,
            created_at: at,
            updated_at: at,
        }
    }

    pub fn support(mut self, at: Timestamp) -> Result<Self, DomainError> {
        match self.lifecycle_status {
            ClaimStatus::Proposed | ClaimStatus::Contested => {
                self.lifecycle_status = ClaimStatus::Supported;
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot support claim in status {:?}",
                self.lifecycle_status
            ))),
        }
    }

    pub fn contest(mut self, at: Timestamp) -> Result<Self, DomainError> {
        match self.lifecycle_status {
            ClaimStatus::Proposed | ClaimStatus::Supported => {
                self.lifecycle_status = ClaimStatus::Contested;
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot contest claim in status {:?}",
                self.lifecycle_status
            ))),
        }
    }

    pub fn supersede(mut self, at: Timestamp) -> Result<Self, DomainError> {
        match self.lifecycle_status {
            ClaimStatus::Proposed | ClaimStatus::Supported | ClaimStatus::Contested => {
                self.lifecycle_status = ClaimStatus::Superseded;
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot supersede claim in status {:?}",
                self.lifecycle_status
            ))),
        }
    }

    pub fn expire(mut self, at: Timestamp) -> Result<Self, DomainError> {
        match self.lifecycle_status {
            ClaimStatus::Proposed | ClaimStatus::Supported | ClaimStatus::Contested => {
                self.lifecycle_status = ClaimStatus::Expired;
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot expire claim in status {:?}",
                self.lifecycle_status
            ))),
        }
    }

    pub fn reject(mut self, at: Timestamp) -> Result<Self, DomainError> {
        match self.lifecycle_status {
            ClaimStatus::Proposed | ClaimStatus::Contested => {
                self.lifecycle_status = ClaimStatus::Rejected;
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot reject claim in status {:?}",
                self.lifecycle_status
            ))),
        }
    }

    pub fn delete(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.lifecycle_status == ClaimStatus::Deleted {
            Err(DomainError::PolicyViolation(
                "claim is already deleted".into(),
            ))
        } else {
            self.lifecycle_status = ClaimStatus::Deleted;
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        }
    }

    pub fn set_valid_range(
        mut self,
        valid_from: Option<Timestamp>,
        valid_until: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        if let (Some(from), Some(until)) = (valid_from, valid_until) {
            if until < from {
                return Err(DomainError::InvalidArgument(format!(
                    "valid_until ({}) must not precede valid_from ({})",
                    until, from
                )));
            }
        }
        self.valid_from = valid_from;
        self.valid_until = valid_until;
        self.state_revision += 1;
        Ok(self)
    }
}
