use crate::{
    DomainError,
    id::{BudAccountId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BudgetAccount {
    pub id: BudAccountId,
    pub workspace_id: WorkspaceId,
    pub account_name: String,
    pub currency: String,
    pub hard_limit: f32,
    pub balance: f32,
    pub created_at: Timestamp,
}

impl BudgetAccount {
    pub fn new(
        id: BudAccountId,
        workspace_id: WorkspaceId,
        account_name: impl Into<String>,
        hard_limit: f32,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if hard_limit < 0.0 {
            return Err(DomainError::InvalidArgument(
                "budget hard limit cannot be negative".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            account_name: account_name.into(),
            currency: "USD".to_string(),
            hard_limit,
            balance: 0.0,
            created_at: at,
        })
    }

    pub fn reserve(&mut self, amount: f32) -> Result<(), DomainError> {
        if amount < 0.0 {
            return Err(DomainError::InvalidArgument(
                "reservation amount must be positive".into(),
            ));
        }
        if self.balance + amount > self.hard_limit {
            return Err(DomainError::PolicyViolation(format!(
                "budget limit exceeded: current {}, requested {}",
                self.balance, amount
            )));
        }
        self.balance += amount;
        Ok(())
    }
}
