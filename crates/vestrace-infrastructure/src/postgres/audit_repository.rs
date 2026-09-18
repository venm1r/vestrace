use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, AuditRepository, RequestContext, UnitOfWork};
use vestrace_domain::AuditEvent;

use super::PgStore;

/// The audit trail.
///
/// # Why this holds a store and not a pool
///
/// `record` took a `RequestContext` and ignored it, writing
/// `event.workspace_id` through an unscoped pool. Nothing checked that the two
/// agreed, so an audit event could be written into another tenant's trail — and
/// the audit trail is the one place where that is least recoverable, because a
/// wrong entry there is indistinguishable from a real one afterwards.
///
/// Scoped now, and the workspace is checked before the write rather than left
/// to the policy: an explicit refusal names the problem, where a policy
/// violation surfaces as a storage error.
pub struct PgAuditRepository {
    store: PgStore,
}

impl PgAuditRepository {
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
impl AuditRepository for PgAuditRepository {
    async fn record(
        &self,
        context: &RequestContext,
        event: &AuditEvent,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        self.record_in(context, &mut scoped, event).await?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn record_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        event: &AuditEvent,
    ) -> Result<(), ApplicationError> {
        if event.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "an audit event cannot be recorded into another workspace".into(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO audit_events (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.id.as_uuid())
        .bind(event.workspace_id.as_uuid())
        .bind(event.principal_id.as_uuid())
        .bind(&event.action)
        .bind(&event.resource_type)
        .bind(event.resource_id)
        .bind(&event.payload)
        .bind(event.created_at)
        .execute(transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;

        Ok(())
    }

    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AuditEvent>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, principal_id, action, resource_type, resource_id,
                   payload, created_at
            FROM audit_events
            WHERE workspace_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(AuditEvent {
                    id: vestrace_domain::id::AuditEventId::from_uuid(
                        row.try_get("id").map_err(storage_error)?,
                    ),
                    workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                        row.try_get("workspace_id").map_err(storage_error)?,
                    ),
                    principal_id: vestrace_domain::PrincipalId::from_uuid(
                        row.try_get("principal_id").map_err(storage_error)?,
                    ),
                    action: row.try_get("action").map_err(storage_error)?,
                    resource_type: row.try_get("resource_type").map_err(storage_error)?,
                    resource_id: row.try_get("resource_id").map_err(storage_error)?,
                    payload: row.try_get("payload").map_err(storage_error)?,
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                })
            })
            .collect::<Result<Vec<_>, ApplicationError>>()?;

        scoped.commit().await.map_err(storage_error)?;
        Ok(events)
    }
}
