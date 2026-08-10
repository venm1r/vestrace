use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::run::ports::{AcquireRunLease, RunLease, RunLeasePort};
use vestrace_application::{ApplicationError, RequestContext};

use super::super::PgStore;

#[derive(Clone)]
pub struct PgRunLeasePort {
    pool: PgPool,
}

impl PgRunLeasePort {
    pub fn new(store: &PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
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
            RETURNING generation, acquired_at, heartbeat_at, lease_until
            "#,
        )
        .bind(run_id)
        .bind(workspace_id)
        .bind(&worker_id)
        .bind(now)
        .bind(lease_until)
        .fetch_optional(&self.pool)
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
        _context: &RequestContext,
        lease: &RunLease,
        extend_until: vestrace_domain::time::Timestamp,
    ) -> Result<RunLease, ApplicationError> {
        let run_id = lease.run_id.as_uuid();
        let worker_id = lease.worker_id.to_string();
        let generation = lease.generation as i64;

        let row = sqlx::query(
            r#"
            UPDATE run_leases
            SET heartbeat_at = $4,
                lease_until = $5
            WHERE run_id = $1
              AND worker_id = $2
              AND generation = $3
              AND lease_until > $4
            RETURNING generation, acquired_at, heartbeat_at, lease_until
            "#,
        )
        .bind(run_id)
        .bind(&worker_id)
        .bind(generation)
        .bind(lease.heartbeat_at)
        .bind(extend_until)
        .fetch_optional(&self.pool)
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
        _context: &RequestContext,
        lease: &RunLease,
    ) -> Result<(), ApplicationError> {
        let run_id = lease.run_id.as_uuid();
        let worker_id = lease.worker_id.to_string();
        let generation = lease.generation as i64;

        let result = sqlx::query(
            r#"
            DELETE FROM run_leases
            WHERE run_id = $1
              AND worker_id = $2
              AND generation = $3
            "#,
        )
        .bind(run_id)
        .bind(&worker_id)
        .bind(generation)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict("lease lost".into()));
        }
        Ok(())
    }
}
