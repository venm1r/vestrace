use sqlx::{PgConnection, Postgres, Transaction};
use vestrace_application::{ApplicationError, RequestContext, TransactionManager, UnitOfWork};

use super::PgStore;
use crate::InfrastructureError;

pub struct PgScopedTransaction {
    transaction: Transaction<'static, Postgres>,
}

#[derive(Clone, Debug)]
pub struct PgTransactionManager {
    store: PgStore,
}

impl PgTransactionManager {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

impl PgScopedTransaction {
    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }

    pub async fn commit(self) -> Result<(), InfrastructureError> {
        self.transaction.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<(), InfrastructureError> {
        self.transaction.rollback().await?;
        Ok(())
    }
}

impl PgStore {
    pub async fn begin_scoped(
        &self,
        context: &RequestContext,
    ) -> Result<PgScopedTransaction, InfrastructureError> {
        let mut transaction = self.pool.begin().await?;

        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.workspace_id")
            .bind(context.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.principal_id")
            .bind(context.principal_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;

        Ok(PgScopedTransaction { transaction })
    }
}

fn application_storage_error(error: InfrastructureError) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait::async_trait]
impl UnitOfWork for PgScopedTransaction {
    async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
        PgScopedTransaction::commit(*self)
            .await
            .map_err(application_storage_error)
    }

    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
        PgScopedTransaction::rollback(*self)
            .await
            .map_err(application_storage_error)
    }
}

#[async_trait::async_trait]
impl TransactionManager for PgTransactionManager {
    async fn begin(
        &self,
        context: &RequestContext,
    ) -> Result<Box<dyn UnitOfWork>, ApplicationError> {
        self.store
            .begin_scoped(context)
            .await
            .map(|transaction| Box::new(transaction) as Box<dyn UnitOfWork>)
            .map_err(application_storage_error)
    }
}
