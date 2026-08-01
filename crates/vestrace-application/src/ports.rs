use crate::{ApplicationError, RequestContext};

#[async_trait::async_trait]
pub trait UnitOfWork: Send {
    async fn commit(self: Box<Self>) -> Result<(), ApplicationError>;
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError>;
}

#[async_trait::async_trait]
pub trait TransactionManager: Send + Sync {
    async fn begin(
        &self,
        context: &RequestContext,
    ) -> Result<Box<dyn UnitOfWork>, ApplicationError>;
}
