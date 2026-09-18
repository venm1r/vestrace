//! Transaction-scoped authority for installation-wide mutations.

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext, UnitOfWork};

/// The mutually compatible classes of installation mutation permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermitMode {
    /// Concurrent governed mutations are allowed to proceed.
    Shared,
    /// Restore and other installation-exclusive work excludes shared holders.
    Exclusive,
}

/// A permit owns the transaction that holds the underlying database lock.
///
/// Dropping the handle drops its unit of work, so PostgreSQL rolls the
/// transaction back and releases the transaction-scoped lock without a reaper.
pub struct PermitHandle {
    unit_of_work: Box<dyn UnitOfWork>,
}

impl PermitHandle {
    /// Constructs a handle around the transaction that acquired the permit.
    pub fn new(unit_of_work: Box<dyn UnitOfWork>) -> Self {
        Self { unit_of_work }
    }

    /// Borrows the transaction for the mutation protected by this permit.
    pub fn unit_of_work_mut(&mut self) -> &mut dyn UnitOfWork {
        self.unit_of_work.as_mut()
    }

    /// Commits the protected mutation and releases the permit with its transaction.
    pub async fn commit(self) -> Result<(), ApplicationError> {
        self.unit_of_work.commit().await
    }

    /// Rolls the protected mutation back and releases the permit with its transaction.
    pub async fn rollback(self) -> Result<(), ApplicationError> {
        self.unit_of_work.rollback().await
    }
}

/// Opens a transaction and holds an installation mutation permit for its lifetime.
#[async_trait]
pub trait InstallationMutationPermit: Send + Sync {
    async fn acquire(
        &self,
        mode: PermitMode,
        context: &RequestContext,
    ) -> Result<PermitHandle, ApplicationError>;
}
