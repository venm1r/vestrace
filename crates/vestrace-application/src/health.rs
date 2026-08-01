use crate::ApplicationError;

#[async_trait::async_trait]
pub trait HealthRepository: Send + Sync {
    async fn check(&self) -> Result<(), ApplicationError>;
}
