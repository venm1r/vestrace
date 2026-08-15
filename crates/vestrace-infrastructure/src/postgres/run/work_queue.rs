use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::run::ports::{LeaseWorkRequest, WorkItem, WorkItemKind, WorkQueuePort};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::id::{AgentRunId, WorkItemId};
use vestrace_domain::run::RunFailure;
use vestrace_domain::time::Timestamp;

use super::super::PgStore;

/// The run work queue.
///
/// # Why this holds a store and not a pool
///
/// Unlike most of the adapters converted in these batches, every statement here
/// already carried `workspace_id = $n`. What it lacked was the connection: the
/// queries went through a bare pool, so no `vestrace.workspace_id` was ever set
/// and the policy on `run_work_items` had nothing to compare against. The
/// predicate was doing all of the work alone, correctly, with no second line of
/// defence if a future query forgot it.
#[derive(Clone)]
pub struct PgWorkQueuePort {
    store: PgStore,
}

impl PgWorkQueuePort {
    pub fn new(store: &PgStore) -> Self {
        Self {
            store: store.clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct WorkItemRow {
    id: uuid::Uuid,
    run_id: uuid::Uuid,
    kind: String,
    step_id: Option<uuid::Uuid>,
    attempt: i32,
    idempotency_key: String,
    expected_run_version: i64,
    available_at: Timestamp,
}

impl TryFrom<WorkItemRow> for WorkItem {
    type Error = ApplicationError;

    fn try_from(row: WorkItemRow) -> Result<Self, Self::Error> {
        let kind = match row.kind.as_str() {
            "advance_run" => WorkItemKind::AdvanceRun,
            "resume_run" => WorkItemKind::ResumeRun,
            "execute_step" => {
                let step_id = row.step_id.ok_or_else(|| {
                    ApplicationError::Storage("execute_step item missing step_id".into())
                })?;
                WorkItemKind::ExecuteStep {
                    step_id: vestrace_domain::id::RunStepId::from_uuid(step_id),
                }
            }
            other => {
                return Err(ApplicationError::Storage(format!(
                    "unknown work item kind: {other}"
                )));
            }
        };

        let version_num = u64::try_from(row.expected_run_version)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let expected_run_version = vestrace_domain::run::RunVersion::new(version_num)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: WorkItemId::from_uuid(row.id),
            run_id: AgentRunId::from_uuid(row.run_id),
            kind,
            expected_run_version,
            available_at: row.available_at,
            idempotency_key: row.idempotency_key,
            attempt: u32::try_from(row.attempt).unwrap_or(0),
        })
    }
}

#[async_trait]
impl WorkQueuePort for PgWorkQueuePort {
    async fn lease_next(
        &self,
        context: &RequestContext,
        request: LeaseWorkRequest,
    ) -> Result<Option<WorkItem>, ApplicationError> {
        let workspace_id = context.workspace_id.as_uuid();
        let worker_id = request.worker_id.to_string();
        let now = request.now;
        let lease_until = request.lease_until;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let row: Option<WorkItemRow> = sqlx::query_as::<_, WorkItemRow>(
            r#"
            WITH next_item AS (
                SELECT id
                FROM run_work_items
                WHERE workspace_id = $1
                  AND available_at <= $2
                  AND (
                        status = 'ready'
                        -- An item whose lease has expired is reclaimable. Without
                        -- this a worker that stopped between leasing and
                        -- completing an item stranded it permanently, because
                        -- nothing ever moved it back to 'ready' — which defeats
                        -- the purpose of the lease having an expiry at all.
                        OR (status = 'leased' AND lease_until IS NOT NULL AND lease_until <= $2)
                      )
                ORDER BY priority DESC, available_at, created_at, id
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE run_work_items
            SET status = 'leased',
                lease_owner = $3,
                lease_until = $4,
                attempt = attempt + 1,
                updated_at = $2
            FROM next_item
            WHERE run_work_items.id = next_item.id
            RETURNING run_work_items.id, run_work_items.run_id, run_work_items.kind,
                      run_work_items.step_id, run_work_items.attempt,
                      run_work_items.idempotency_key, run_work_items.expected_run_version,
                      run_work_items.available_at
            "#,
        )
        .bind(workspace_id)
        .bind(now)
        .bind(&worker_id)
        .bind(lease_until)
        .fetch_optional(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        // `FOR UPDATE SKIP LOCKED` holds its row lock until this commit, which
        // is what keeps two workers from leasing the same item. It used to be
        // released by the implicit commit of a single autocommit statement; the
        // window is the same length here, just explicit.
        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        match row {
            Some(row) => Ok(Some(WorkItem::try_from(row)?)),
            None => Ok(None),
        }
    }

    async fn complete(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let id = item.id.as_uuid();
        let workspace_id = context.workspace_id.as_uuid();

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE run_work_items
            SET status = 'completed',
                updated_at = $3,
                lease_owner = NULL,
                lease_until = NULL
            WHERE id = $1
              AND workspace_id = $2
              AND status = 'leased'
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(at)
        .execute(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "work item not leased or not found".into(),
            ));
        }
        Ok(())
    }

    async fn retry(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        available_at: Timestamp,
        error: RunFailure,
    ) -> Result<(), ApplicationError> {
        let id = item.id.as_uuid();
        let workspace_id = context.workspace_id.as_uuid();
        let error_json =
            serde_json::to_value(&error).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE run_work_items
            SET status = CASE
                    WHEN attempt >= max_attempts THEN 'dead_letter'
                    ELSE 'ready'
                END,
                available_at = $4,
                last_error = $5,
                lease_owner = NULL,
                lease_until = NULL,
                updated_at = $4
            WHERE id = $1
              AND workspace_id = $2
              AND status = 'leased'
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(item.attempt as i32)
        .bind(available_at)
        .bind(error_json)
        .execute(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "work item not leased or not found".into(),
            ));
        }
        Ok(())
    }

    async fn cancel_for_run(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        at: Timestamp,
    ) -> Result<u64, ApplicationError> {
        let run_id = run_id.as_uuid();
        let workspace_id = context.workspace_id.as_uuid();

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE run_work_items
            SET status = 'cancelled',
                updated_at = $3,
                lease_owner = NULL,
                lease_until = NULL
            WHERE run_id = $1
              AND workspace_id = $2
              AND (status = 'ready' OR (status = 'leased' AND lease_until < $3))
            "#,
        )
        .bind(run_id)
        .bind(workspace_id)
        .bind(at)
        .execute(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn dead_letter(
        &self,
        context: &RequestContext,
        item: &WorkItem,
        error: RunFailure,
        at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let id = item.id.as_uuid();
        let workspace_id = context.workspace_id.as_uuid();
        let error_json =
            serde_json::to_value(&error).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE run_work_items
            SET status = 'dead_letter',
                last_error = $4,
                updated_at = $5,
                lease_owner = NULL,
                lease_until = NULL
            WHERE id = $1
              AND workspace_id = $2
              AND idempotency_key = $3
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(&item.idempotency_key)
        .bind(error_json)
        .bind(at)
        .execute(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict("work item not found".into()));
        }
        Ok(())
    }
}
