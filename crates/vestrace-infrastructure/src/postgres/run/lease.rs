use async_trait::async_trait;
use vestrace_application::run::ports::{AcquireRunLease, RunLease, RunLeasePort};
use vestrace_application::{ApplicationError, RequestContext};

use super::super::PgStore;

/// The run lease — the mutual-exclusion primitive that decides which worker may
/// advance a run.
///
/// # Why this holds a store and not a pool
///
/// `heartbeat` and `release` took a `RequestContext`, ignored it, and matched on
/// `run_id` alone. A worker serving one workspace could extend or delete the
/// lease on another workspace's run, given the worker id and generation it would
/// have if it held one. `acquire`'s `ON CONFLICT (run_id) DO UPDATE` had no
/// workspace predicate either, so an expired lease on any tenant's run was
/// takeable by any worker — the row kept its own `workspace_id` while its
/// `worker_id` came to belong to somebody else.
///
/// That is not a read leak; it is worse in kind. The lease is what stops two
/// workers advancing the same run, so a cross-tenant acquire lets one workspace
/// deny another the ability to execute its runs at all.
#[derive(Clone)]
pub struct PgRunLeasePort {
    store: PgStore,
}

impl PgRunLeasePort {
    pub fn new(store: &PgStore) -> Self {
        Self {
            store: store.clone(),
        }
    }
}

#[async_trait]
impl RunLeasePort for PgRunLeasePort {
    async fn acquire(
        &self,
        context: &RequestContext,
        request: AcquireRunLease,
    ) -> Result<RunLease, ApplicationError> {
        let run_id = request.run_id.as_uuid();
        let workspace_id = context.workspace_id.as_uuid();
        let worker_id = request.worker_id.to_string();
        let now = request.now;
        let lease_until = request.lease_until;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        // The takeover clause matches the workspace as well as the expiry. An
        // expired lease belonging to another tenant is not this worker's to
        // reclaim, and the row would have kept its original `workspace_id`
        // while answering to a worker from somewhere else.
        let row = sqlx::query(
            r#"
            INSERT INTO run_leases (run_id, workspace_id, worker_id, generation, acquired_at, heartbeat_at, lease_until)
            VALUES ($1, $2, $3, 1, $4, $4, $5)
            ON CONFLICT (run_id) DO UPDATE
            SET worker_id = EXCLUDED.worker_id,
                generation = run_leases.generation + 1,
                acquired_at = EXCLUDED.acquired_at,
                heartbeat_at = EXCLUDED.heartbeat_at,
                lease_until = EXCLUDED.lease_until
            WHERE run_leases.lease_until < $4
              AND run_leases.workspace_id = $2
            RETURNING generation, acquired_at, heartbeat_at, lease_until
            "#,
        )
        .bind(run_id)
        .bind(workspace_id)
        .bind(&worker_id)
        .bind(now)
        .bind(lease_until)
        .fetch_optional(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        match row {
            Some(row) => {
                let generation: i64 = sqlx::Row::get(&row, "generation");
                let acquired_at: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "acquired_at");
                let heartbeat_at: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "heartbeat_at");
                let lease_until: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "lease_until");
                Ok(RunLease {
                    run_id: request.run_id,
                    worker_id: request.worker_id,
                    generation: u64::try_from(generation).unwrap_or(1),
                    acquired_at,
                    heartbeat_at,
                    lease_until,
                })
            }
            None => Err(ApplicationError::Conflict(
                "run lease held by another worker".into(),
            )),
        }
    }

    async fn heartbeat(
        &self,
        context: &RequestContext,
        lease: &RunLease,
        extend_until: vestrace_domain::time::Timestamp,
    ) -> Result<RunLease, ApplicationError> {
        let run_id = lease.run_id.as_uuid();
        let worker_id = lease.worker_id.to_string();
        let generation = lease.generation as i64;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let row = sqlx::query(
            r#"
            UPDATE run_leases
            SET heartbeat_at = $5,
                lease_until = $6
            WHERE run_id = $1
              AND workspace_id = $2
              AND worker_id = $3
              AND generation = $4
              AND lease_until > $5
            RETURNING generation, acquired_at, heartbeat_at, lease_until
            "#,
        )
        .bind(run_id)
        .bind(context.workspace_id.as_uuid())
        .bind(&worker_id)
        .bind(generation)
        .bind(lease.heartbeat_at)
        .bind(extend_until)
        .fetch_optional(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        match row {
            Some(row) => {
                let generation: i64 = sqlx::Row::get(&row, "generation");
                let acquired_at: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "acquired_at");
                let heartbeat_at: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "heartbeat_at");
                let lease_until: vestrace_domain::time::Timestamp =
                    sqlx::Row::get(&row, "lease_until");
                Ok(RunLease {
                    run_id: lease.run_id,
                    worker_id: lease.worker_id,
                    generation: u64::try_from(generation).unwrap_or(lease.generation),
                    acquired_at,
                    heartbeat_at,
                    lease_until,
                })
            }
            None => Err(ApplicationError::Conflict("lease lost".into())),
        }
    }

    async fn release(
        &self,
        context: &RequestContext,
        lease: &RunLease,
    ) -> Result<(), ApplicationError> {
        let run_id = lease.run_id.as_uuid();
        let worker_id = lease.worker_id.to_string();
        let generation = lease.generation as i64;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result = sqlx::query(
            r#"
            DELETE FROM run_leases
            WHERE run_id = $1
              AND workspace_id = $2
              AND worker_id = $3
              AND generation = $4
            "#,
        )
        .bind(run_id)
        .bind(context.workspace_id.as_uuid())
        .bind(&worker_id)
        .bind(generation)
        .execute(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict("lease lost".into()));
        }
        Ok(())
    }
}
