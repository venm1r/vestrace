use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, ConnectionListing, ConnectionRepository, RequestContext,
};
use vestrace_domain::connection::{Connection, ConnectionStatus};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgConnectionRepository {
    store: PgStore,
}

impl PgConnectionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn parse_status(value: &str) -> Result<ConnectionStatus, ApplicationError> {
    match value {
        "active" => Ok(ConnectionStatus::Active),
        "revoked" => Ok(ConnectionStatus::Revoked),
        "expired" => Ok(ConnectionStatus::Expired),
        other => Err(ApplicationError::Storage(format!(
            "unknown connection status {other:?}"
        ))),
    }
}

#[async_trait]
impl ConnectionRepository for PgConnectionRepository {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ConnectionListing>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        // Only identity and state are selected. The schema holds no credential
        // column, and none is introduced here.
        let rows = sqlx::query(
            "SELECT c.id, c.connector_id, c.principal_id, c.name, c.status, c.created_at,
                    k.name AS connector_name, k.provider_type
             FROM connections AS c
             JOIN connectors AS k ON k.id = c.connector_id AND k.workspace_id = $1
             WHERE c.workspace_id = $1
             ORDER BY c.created_at DESC, c.id DESC
             LIMIT $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(ConnectionListing {
                    connection: Connection {
                        id: vestrace_domain::id::ConnectionId::from_uuid(
                            row.try_get("id").map_err(storage_error)?,
                        ),
                        connector_id: vestrace_domain::id::ConnectorId::from_uuid(
                            row.try_get("connector_id").map_err(storage_error)?,
                        ),
                        workspace_id: context.workspace_id,
                        principal_id: vestrace_domain::PrincipalId::from_uuid(
                            row.try_get("principal_id").map_err(storage_error)?,
                        ),
                        name: row.try_get("name").map_err(storage_error)?,
                        status: parse_status(
                            &row.try_get::<String, _>("status").map_err(storage_error)?,
                        )?,
                        created_at: row.try_get("created_at").map_err(storage_error)?,
                    },
                    connector_name: row.try_get("connector_name").map_err(storage_error)?,
                    provider_type: row.try_get("provider_type").map_err(storage_error)?,
                })
            })
            .collect()
    }
}
