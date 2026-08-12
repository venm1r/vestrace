use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use vestrace_application::{ApplicationError, RecoveryRepository};
use vestrace_domain::{
    HealthScope, Incident, IncidentId, RecoveryPoint, RecoveryPointId, RevalidationRun,
    RevalidationRunId, TrustStateRecord,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRecoveryRepository {
    pool: PgPool,
}

impl PgRecoveryRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct IncidentRow {
    id: uuid::Uuid,
    status: String,
    scope: Value,
    payload: Value,
    opened_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct RevalidationRunRow {
    id: uuid::Uuid,
    incident_id: Option<uuid::Uuid>,
    result: String,
    scope: Value,
    payload: Value,
    completed_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct TrustStateRow {
    scope_key: String,
    scope: Value,
    state: String,
    payload: Value,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct RecoveryPointRow {
    id: uuid::Uuid,
    integrity_status: String,
    payload: Value,
    created_at: DateTime<Utc>,
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn conflict(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Conflict(message.into())
}

fn enum_name<T: Serialize>(value: T) -> Result<String, ApplicationError> {
    serde_json::to_value(value)
        .map_err(storage_error)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| storage_error("recovery enum did not serialize as a string"))
}

fn json<T: Serialize>(value: &T) -> Result<Value, ApplicationError> {
    serde_json::to_value(value).map_err(storage_error)
}

fn decode<T: DeserializeOwned>(payload: Value) -> Result<T, ApplicationError> {
    serde_json::from_value(payload).map_err(storage_error)
}

fn scope_key(scope: &HealthScope) -> Result<String, ApplicationError> {
    serde_json::to_string(scope).map_err(storage_error)
}

async fn verify_insert<T, F>(find: F, expected: &T, message: &str) -> Result<(), ApplicationError>
where
    T: Eq,
    F: std::future::Future<Output = Result<Option<T>, ApplicationError>>,
{
    match find.await? {
        Some(existing) if existing == *expected => Ok(()),
        Some(_) => Err(conflict(message)),
        None => Err(storage_error(
            "recovery conflict row disappeared before verification",
        )),
    }
}

#[async_trait]
impl RecoveryRepository for PgRecoveryRepository {
    async fn insert_incident(&self, incident: &Incident) -> Result<(), ApplicationError> {
        let status = enum_name(incident.status())?;
        let scope = json(incident.scope())?;
        let payload = json(incident)?;
        let result = sqlx::query(
            "INSERT INTO incidents (id, status, scope, payload, opened_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(incident.id().as_uuid())
        .bind(status)
        .bind(scope)
        .bind(payload)
        .bind(incident.opened_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_incident(incident.id()),
            incident,
            &format!(
                "incident id {} already contains different evidence",
                incident.id()
            ),
        )
        .await
    }

    async fn find_incident(&self, id: IncidentId) -> Result<Option<Incident>, ApplicationError> {
        let row = sqlx::query_as::<_, IncidentRow>(
            "SELECT id, status, scope, payload, opened_at
             FROM incidents WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let incident: Incident = decode(row.payload)?;
            let expected_status = enum_name(incident.status())?;
            let expected_scope = json(incident.scope())?;
            if incident.id().as_uuid() != row.id
                || expected_status != row.status
                || expected_scope != row.scope
                || incident.opened_at() != row.opened_at
            {
                return Err(storage_error(
                    "incident indexed metadata does not match payload",
                ));
            }
            Ok(incident)
        })
        .transpose()
    }

    async fn insert_revalidation_run(&self, run: &RevalidationRun) -> Result<(), ApplicationError> {
        let result_name = enum_name(run.result())?;
        let scope = json(run.scope())?;
        let payload = json(run)?;
        let result = sqlx::query(
            "INSERT INTO revalidation_runs (
                 id, incident_id, result, scope, payload, completed_at
             ) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(run.id().as_uuid())
        .bind(run.incident_id().map(|id| id.as_uuid()))
        .bind(result_name)
        .bind(scope)
        .bind(payload)
        .bind(run.completed_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_revalidation_run(run.id()),
            run,
            &format!(
                "revalidation run id {} already contains different evidence",
                run.id()
            ),
        )
        .await
    }

    async fn find_revalidation_run(
        &self,
        id: RevalidationRunId,
    ) -> Result<Option<RevalidationRun>, ApplicationError> {
        let row = sqlx::query_as::<_, RevalidationRunRow>(
            "SELECT id, incident_id, result, scope, payload, completed_at
             FROM revalidation_runs WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let run: RevalidationRun = decode(row.payload)?;
            let expected_result = enum_name(run.result())?;
            let expected_scope = json(run.scope())?;
            if run.id().as_uuid() != row.id
                || run.incident_id().map(|value| value.as_uuid()) != row.incident_id
                || expected_result != row.result
                || expected_scope != row.scope
                || run.completed_at() != row.completed_at
            {
                return Err(storage_error(
                    "revalidation run indexed metadata does not match payload",
                ));
            }
            Ok(run)
        })
        .transpose()
    }

    async fn insert_trust_state(&self, state: &TrustStateRecord) -> Result<(), ApplicationError> {
        let scope_key = scope_key(state.scope())?;
        let scope = json(state.scope())?;
        let state_name = enum_name(state.state())?;
        let payload = json(state)?;
        sqlx::query(
            "INSERT INTO trust_state_records (
                 scope_key, scope, state, payload, updated_at
             ) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (scope_key, updated_at, payload) DO NOTHING",
        )
        .bind(scope_key)
        .bind(scope)
        .bind(state_name)
        .bind(payload)
        .bind(state.updated_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn find_latest_trust_state(
        &self,
        scope: &HealthScope,
    ) -> Result<Option<TrustStateRecord>, ApplicationError> {
        let key = scope_key(scope)?;
        let row = sqlx::query_as::<_, TrustStateRow>(
            "SELECT scope_key, scope, state, payload, updated_at
             FROM trust_state_records
             WHERE scope_key = $1
             ORDER BY updated_at DESC, created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let state: TrustStateRecord = decode(row.payload)?;
            let expected_scope = json(state.scope())?;
            let expected_key = scope_key(state.scope())?;
            let expected_state = enum_name(state.state())?;
            if expected_key != row.scope_key
                || expected_scope != row.scope
                || expected_state != row.state
                || state.updated_at() != row.updated_at
            {
                return Err(storage_error(
                    "trust state indexed metadata does not match payload",
                ));
            }
            Ok(state)
        })
        .transpose()
    }

    async fn insert_recovery_point(&self, point: &RecoveryPoint) -> Result<(), ApplicationError> {
        let integrity_status = enum_name(point.integrity_status())?;
        let payload = json(point)?;
        let result = sqlx::query(
            "INSERT INTO recovery_points (id, integrity_status, payload, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(point.id().as_uuid())
        .bind(integrity_status)
        .bind(payload)
        .bind(point.created_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_recovery_point(point.id()),
            point,
            &format!(
                "recovery point id {} already contains different evidence",
                point.id()
            ),
        )
        .await
    }

    async fn find_recovery_point(
        &self,
        id: RecoveryPointId,
    ) -> Result<Option<RecoveryPoint>, ApplicationError> {
        let row = sqlx::query_as::<_, RecoveryPointRow>(
            "SELECT id, integrity_status, payload, created_at
             FROM recovery_points WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let point: RecoveryPoint = decode(row.payload)?;
            let expected_integrity = enum_name(point.integrity_status())?;
            if point.id().as_uuid() != row.id
                || expected_integrity != row.integrity_status
                || point.created_at() != row.created_at
            {
                return Err(storage_error(
                    "recovery point indexed metadata does not match payload",
                ));
            }
            Ok(point)
        })
        .transpose()
    }
}
