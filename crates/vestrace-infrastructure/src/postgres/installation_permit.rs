//! PostgreSQL-backed installation mutation permits.

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, InstallationMutationPermit, PermitHandle, PermitMode, RequestContext,
};

use super::PgStore;

const INSTALLATION_MUTATION_PERMIT_LOCK: &str = "vestrace-installation-mutation-permit-v1";

/// Holds PostgreSQL advisory transaction locks for installation-wide mutations.
#[derive(Clone, Debug)]
pub struct PgInstallationMutationPermit {
    store: PgStore,
}

impl PgInstallationMutationPermit {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl InstallationMutationPermit for PgInstallationMutationPermit {
    async fn acquire(
        &self,
        mode: PermitMode,
        context: &RequestContext,
    ) -> Result<PermitHandle, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let lock = match mode {
            PermitMode::Shared => "SELECT pg_advisory_xact_lock_shared(hashtext($1))",
            PermitMode::Exclusive => "SELECT pg_advisory_xact_lock(hashtext($1))",
        };
        sqlx::query(lock)
            .bind(INSTALLATION_MUTATION_PERMIT_LOCK)
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;

        Ok(PermitHandle::new(Box::new(transaction)))
    }
}
