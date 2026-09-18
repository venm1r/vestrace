use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, IdempotencyRecord, IdempotencyRepository, RequestContext, UnitOfWork,
};
use vestrace_domain::{WorkspaceId, time::Timestamp};

use super::PgStore;

/// Idempotency keys, scoped like everything else.
///
/// This ran on a bare pool while `idempotency_keys` carried a workspace
/// isolation policy that was enabled and never forced — so the policy was inert
/// and the only thing keeping one workspace's replayed responses out of
/// another's was the `WHERE workspace_id = $1` in each statement. Forcing the
/// policy and giving the port a scope are two halves of one change.
pub struct PgIdempotencyRepository {
    store: PgStore,
}

impl PgIdempotencyRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut super::PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<super::PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected a PostgreSQL unit of work".to_owned()))
}

#[async_trait]
impl IdempotencyRepository for PgIdempotencyRepository {
    async fn find_by_key(
        &self,
        context: &RequestContext,
        key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT idempotency_key, workspace_id, request_hash, response_payload, status, created_at, expires_at
            FROM idempotency_keys
            WHERE workspace_id = $1 AND idempotency_key = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(key)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.map(parse_record).transpose()
    }

    async fn save(
        &self,
        context: &RequestContext,
        record: &IdempotencyRecord,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        self.save_in(context, &mut scoped, record).await?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn save_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        record: &IdempotencyRecord,
    ) -> Result<(), ApplicationError> {
        if record.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "an idempotency record cannot be saved into another workspace".into(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO idempotency_keys (idempotency_key, workspace_id, request_hash, response_payload, status, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (workspace_id, idempotency_key) DO NOTHING
            "#,
        )
        .bind(&record.idempotency_key)
        .bind(record.workspace_id.as_uuid())
        .bind(&record.request_hash)
        .bind(&record.response_payload)
        .bind(&record.status)
        .bind(record.created_at)
        .bind(record.expires_at)
        .execute(transaction(unit_of_work)?.connection())
        .await
        .map_err(|error| ApplicationError::Conflict(error.to_string()))?;

        Ok(())
    }
}

fn parse_record(row: sqlx::postgres::PgRow) -> Result<IdempotencyRecord, ApplicationError> {
    let idempotency_key: String = row.try_get("idempotency_key").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let request_hash: String = row.try_get("request_hash").map_err(storage_error)?;
    let response_payload: Option<serde_json::Value> =
        row.try_get("response_payload").map_err(storage_error)?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let created_at: Timestamp = row.try_get("created_at").map_err(storage_error)?;
    let expires_at: Timestamp = row.try_get("expires_at").map_err(storage_error)?;

    Ok(IdempotencyRecord {
        idempotency_key,
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        request_hash,
        response_payload,
        status,
        created_at,
        expires_at,
    })
}
