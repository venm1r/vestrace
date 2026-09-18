use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use vestrace_application::{ApplicationError, QualificationRepository};
use vestrace_domain::{
    QualificationBundle, QualificationBundleId, QualificationLifecycle,
    conformance::QualificationProfile,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgQualificationRepository {
    pool: PgPool,
}

impl PgQualificationRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct QualificationBundleRow {
    id: uuid::Uuid,
    lifecycle: String,
    profile: String,
    status: String,
    target_digest: String,
    payload: Value,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
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
        .ok_or_else(|| storage_error("qualification enum did not serialize as a string"))
}

fn decode_row(row: QualificationBundleRow) -> Result<QualificationBundle, ApplicationError> {
    let bundle: QualificationBundle = serde_json::from_value(row.payload).map_err(storage_error)?;
    let expected_lifecycle = enum_name(bundle.lifecycle())?;
    let expected_profile = enum_name(bundle.profile())?;
    let expected_status = enum_name(bundle.status())?;

    if bundle.id().as_uuid() != row.id
        || expected_lifecycle != row.lifecycle
        || expected_profile != row.profile
        || expected_status != row.status
        || bundle.target_digest() != row.target_digest
        || bundle.started_at() != row.started_at
        || bundle.completed_at() != row.completed_at
    {
        return Err(storage_error(
            "qualification bundle indexed metadata does not match payload",
        ));
    }

    Ok(bundle)
}

#[async_trait]
impl QualificationRepository for PgQualificationRepository {
    async fn insert(&self, bundle: &QualificationBundle) -> Result<(), ApplicationError> {
        let lifecycle = enum_name(bundle.lifecycle())?;
        let profile = enum_name(bundle.profile())?;
        let status = enum_name(bundle.status())?;
        let payload = serde_json::to_value(bundle).map_err(storage_error)?;

        let result = sqlx::query(
            "INSERT INTO qualification_bundles (
                 id, lifecycle, profile, status, target_digest, payload,
                 started_at, completed_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(bundle.id().as_uuid())
        .bind(lifecycle)
        .bind(profile)
        .bind(status)
        .bind(bundle.target_digest())
        .bind(payload)
        .bind(bundle.started_at())
        .bind(bundle.completed_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }

        match self.find_by_id(bundle.id()).await? {
            Some(existing) if existing == *bundle => Ok(()),
            Some(_) => Err(conflict(format!(
                "qualification bundle id {} already contains different evidence",
                bundle.id()
            ))),
            None => Err(storage_error(
                "qualification bundle conflict row disappeared before verification",
            )),
        }
    }

    async fn find_by_id(
        &self,
        id: QualificationBundleId,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        let row = sqlx::query_as::<_, QualificationBundleRow>(
            "SELECT id, lifecycle, profile, status, target_digest, payload,
                    started_at, completed_at
             FROM qualification_bundles
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(decode_row).transpose()
    }

    async fn find_latest(
        &self,
        profile: QualificationProfile,
        lifecycle: QualificationLifecycle,
        target_digest: &str,
    ) -> Result<Option<QualificationBundle>, ApplicationError> {
        let profile = enum_name(profile)?;
        let lifecycle = enum_name(lifecycle)?;
        let row = sqlx::query_as::<_, QualificationBundleRow>(
            "SELECT id, lifecycle, profile, status, target_digest, payload,
                    started_at, completed_at
             FROM qualification_bundles
             WHERE profile = $1
               AND lifecycle = $2
               AND target_digest = $3
             ORDER BY started_at DESC, created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(profile)
        .bind(lifecycle)
        .bind(target_digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(decode_row).transpose()
    }
}
