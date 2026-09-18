use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, RecoveryQualificationEvidence, RecoveryQualificationEvidenceRepository,
};
use vestrace_domain::{AgentRunId, RecoveryAction, RecoveryClassification, RecoveryTarget};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRecoveryQualificationEvidenceRepository {
    pool: PgPool,
}

impl PgRecoveryQualificationEvidenceRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct RecoveryQualificationEvidenceRow {
    id: Uuid,
    run_id: Uuid,
    target: String,
    classification: String,
    action: String,
    observed_at: DateTime<Utc>,
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn encode<T: Serialize>(value: T) -> Result<String, ApplicationError> {
    match serde_json::to_value(value).map_err(storage_error)? {
        serde_json::Value::String(value) => Ok(value),
        _ => Err(storage_error(
            "recovery qualification enum was not a string",
        )),
    }
}

fn decode<T: DeserializeOwned>(value: String) -> Result<T, ApplicationError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(storage_error)
}

fn decode_row(
    row: RecoveryQualificationEvidenceRow,
) -> Result<RecoveryQualificationEvidence, ApplicationError> {
    Ok(RecoveryQualificationEvidence::from_persisted(
        row.id,
        AgentRunId::from_uuid(row.run_id),
        decode::<RecoveryTarget>(row.target)?,
        decode::<RecoveryClassification>(row.classification)?,
        decode::<RecoveryAction>(row.action)?,
        row.observed_at,
    ))
}

#[async_trait]
impl RecoveryQualificationEvidenceRepository for PgRecoveryQualificationEvidenceRepository {
    async fn insert(
        &self,
        evidence: &RecoveryQualificationEvidence,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            "INSERT INTO recovery_qualification_observations (
                 id, run_id, target, classification, action, observed_at
             ) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(evidence.id())
        .bind(evidence.run_id().as_uuid())
        .bind(encode(evidence.target())?)
        .bind(encode(evidence.classification())?)
        .bind(encode(evidence.action())?)
        .bind(evidence.observed_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<RecoveryQualificationEvidence>, ApplicationError> {
        sqlx::query_as::<_, RecoveryQualificationEvidenceRow>(
            "SELECT id, run_id, target, classification, action, observed_at
             FROM recovery_qualification_observations
             ORDER BY observed_at, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(decode_row)
        .collect()
    }
}
