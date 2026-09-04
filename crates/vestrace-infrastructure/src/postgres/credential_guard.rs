use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::{ConnectionId, CredentialSlotId};

use super::PgStore;

/// PostgreSQL authority for the permanent credential guard and its one-live
/// preparing/Candidate occupancy. It exposes no separately acquirable locks,
/// so callers cannot invert the database's canonical lock chain.
#[derive(Clone, Debug)]
pub struct PgCredentialGuardRepository {
    store: PgStore,
}

impl PgCredentialGuardRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }

    pub async fn reserve_preparing(
        &self,
        context: &RequestContext,
        connection_id: ConnectionId,
        slot_id: CredentialSlotId,
        occupancy_id: uuid::Uuid,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let result =
            sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
                .bind(occupancy_id)
                .bind(context.workspace_id.as_uuid())
                .bind(connection_id.as_uuid())
                .bind(slot_id.as_uuid())
                .execute(transaction.connection())
                .await;

        match result {
            Ok(_) => transaction.commit().await.map_err(storage_error),
            Err(error) if sqlstate(&error).as_deref() == Some("23505") => Err(
                ApplicationError::Conflict("CREDENTIAL_GUARD_OCCUPIED".to_owned()),
            ),
            Err(error) => Err(storage_error(error)),
        }
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
