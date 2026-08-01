#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfrastructureErrorKind {
    Configuration,
    Database,
    Migration,
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct InfrastructureError(InfrastructureErrorSource);

#[derive(Debug, thiserror::Error)]
enum InfrastructureErrorSource {
    #[error("invalid infrastructure configuration: {0}")]
    Configuration(String),
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
}

impl InfrastructureError {
    pub fn kind(&self) -> InfrastructureErrorKind {
        match self.0 {
            InfrastructureErrorSource::Configuration(_) => InfrastructureErrorKind::Configuration,
            InfrastructureErrorSource::Database(_) => InfrastructureErrorKind::Database,
            InfrastructureErrorSource::Migration(_) => InfrastructureErrorKind::Migration,
        }
    }

    pub(crate) fn configuration(message: impl Into<String>) -> Self {
        Self(InfrastructureErrorSource::Configuration(message.into()))
    }
}

impl From<sqlx::Error> for InfrastructureError {
    fn from(error: sqlx::Error) -> Self {
        Self(InfrastructureErrorSource::Database(error))
    }
}

impl From<sqlx::migrate::MigrateError> for InfrastructureError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self(InfrastructureErrorSource::Migration(error))
    }
}
