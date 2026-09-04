//! The single transaction boundary for a governed mutation and its evidence.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    AuditEvent,
    id::{AuditEventId, OutboxId},
};

use crate::{ApplicationError, IdempotencyRecord, OutboxMessage, RequestContext, UnitOfWork};

/// The audit entry paired with a governed mutation.
pub type AuditEntry = AuditEvent;

/// Applies the business mutation using the transaction opened by the authority.
#[async_trait]
pub trait GovernedMutationApply: Send + Sync {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError>;
}

/// A governed mutation and all durable evidence it must commit with.
pub struct GovernedMutation<T> {
    pub context: RequestContext,
    pub audit: AuditEntry,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub apply: T,
}

/// Identifiers committed by a governed mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedMutationReceipt {
    pub audit_event_id: AuditEventId,
    pub idempotency_key: Option<String>,
    pub outbox_message_ids: Vec<OutboxId>,
}

/// Commits a concrete mutation and its audit, idempotency and outbox evidence
/// through one caller-owned unit of work.
#[async_trait]
pub trait GovernedMutationRepository<T: GovernedMutationApply + 'static>: Send + Sync {
    async fn commit(
        &self,
        mutation: GovernedMutation<T>,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;

    /// Commit through the transaction and installation permit already owned by
    /// the caller. Implementations must not open or commit another transaction.
    async fn commit_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        mutation: GovernedMutation<T>,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;
}

pub type SharedGovernedMutationRepository<T> = Arc<dyn GovernedMutationRepository<T>>;

#[cfg(test)]
mod transaction_bound_api_contract {
    use super::*;

    struct Mutation;

    #[async_trait]
    impl GovernedMutationApply for Mutation {
        async fn apply(
            &self,
            _context: &RequestContext,
            _unit_of_work: &mut dyn UnitOfWork,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[test]
    fn governed_mutation_repository_exposes_caller_owned_commit() {
        async fn type_check(
            repository: &dyn GovernedMutationRepository<Mutation>,
            unit_of_work: &mut dyn UnitOfWork,
            mutation: GovernedMutation<Mutation>,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            repository.commit_in(unit_of_work, mutation).await
        }

        let _ = type_check;
    }
}
