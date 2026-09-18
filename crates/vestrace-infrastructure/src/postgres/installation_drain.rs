use sqlx::Row;
use vestrace_application::{ApplicationError, DrainMutationPermitRepository, RequestContext};
use vestrace_domain::{InstallationDrainRequest, InstallationDrainRequestId};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgDrainMutationPermitRepository {
    store: PgStore,
}

impl PgDrainMutationPermitRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait::async_trait]
impl DrainMutationPermitRepository for PgDrainMutationPermitRepository {
    async fn request(
        &self,
        context: &RequestContext,
    ) -> Result<InstallationDrainRequest, ApplicationError> {
        let id = InstallationDrainRequestId::new();
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let result = sqlx::query("SELECT vestrace_request_installation_drain($1)")
            .bind(id.as_uuid())
            .execute(transaction.connection())
            .await;
        match result {
            Ok(_) => {
                transaction.commit().await.map_err(storage_error)?;
                Ok(InstallationDrainRequest::request(id))
            }
            Err(error) if sqlstate(&error).as_deref() == Some("55000") => Err(
                ApplicationError::Conflict("INSTALLATION_DRAIN_ALREADY_ACTIVE".to_owned()),
            ),
            Err(error) => Err(storage_error(error)),
        }
    }

    async fn current_state(
        &self,
        context: &RequestContext,
    ) -> Result<Option<InstallationDrainRequest>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT id, completed_at FROM installation_drain_requests \
             ORDER BY requested_at DESC LIMIT 1",
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(row.map(|row| {
            let id = InstallationDrainRequestId::from_uuid(row.get("id"));
            let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("completed_at");
            if completed_at.is_some() {
                InstallationDrainRequest::Frozen(id)
            } else {
                InstallationDrainRequest::Draining(id)
            }
        }))
    }

    async fn reconcile(
        &self,
        context: &RequestContext,
        id: InstallationDrainRequestId,
    ) -> Result<InstallationDrainRequest, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let frozen: bool = sqlx::query_scalar("SELECT vestrace_reconcile_installation_drain($1)")
            .bind(id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(if frozen {
            InstallationDrainRequest::Frozen(id)
        } else {
            InstallationDrainRequest::Draining(id)
        })
    }
}
