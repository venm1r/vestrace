use vestrace_application::{ApplicationError, HealthRepository};

use super::PgStore;

#[async_trait::async_trait]
impl HealthRepository for PgStore {
    async fn check(&self) -> Result<(), ApplicationError> {
        match self.migrations_are_compatible().await {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(ApplicationError::Unavailable(
                "database is not ready".to_owned(),
            )),
        }
    }
}
