use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ExternalEffectFaultSuiteEvidence, FaultSuiteEvidenceRepository,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgFaultSuiteEvidenceRepository {
    pool: PgPool,
}

impl PgFaultSuiteEvidenceRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct FaultSuiteEvidenceRow {
    id: Uuid,
    target_digest: String,
    passed: bool,
    payload: Value,
    created_at: DateTime<Utc>,
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn conflict(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Conflict(message.into())
}

fn decode(
    row: FaultSuiteEvidenceRow,
) -> Result<ExternalEffectFaultSuiteEvidence, ApplicationError> {
    let evidence: ExternalEffectFaultSuiteEvidence =
        serde_json::from_value(row.payload).map_err(storage_error)?;
    if evidence.id() != row.id
        || evidence.target_digest() != row.target_digest
        || evidence.is_passed() != row.passed
        || evidence.created_at() != row.created_at
    {
        return Err(storage_error(
            "fault-suite evidence indexed metadata does not match payload",
        ));
    }
    Ok(evidence)
}

#[async_trait]
impl FaultSuiteEvidenceRepository for PgFaultSuiteEvidenceRepository {
    async fn insert(
        &self,
        evidence: &ExternalEffectFaultSuiteEvidence,
    ) -> Result<(), ApplicationError> {
        let payload = serde_json::to_value(evidence).map_err(storage_error)?;
        let result = sqlx::query(
            "INSERT INTO external_effect_fault_suite_evidence (
                 id, target_digest, passed, payload, created_at
             ) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(evidence.id())
        .bind(evidence.target_digest())
        .bind(evidence.is_passed())
        .bind(payload)
        .bind(evidence.created_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }

        match self.find_by_id(evidence.id()).await? {
            Some(existing) if existing == *evidence => Ok(()),
            Some(_) => Err(conflict(format!(
                "fault-suite evidence id {} already contains different evidence",
                evidence.id()
            ))),
            None => Err(storage_error(
                "fault-suite evidence conflict row disappeared before verification",
            )),
        }
    }

    async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ExternalEffectFaultSuiteEvidence>, ApplicationError> {
        let row = sqlx::query_as::<_, FaultSuiteEvidenceRow>(
            "SELECT id, target_digest, passed, payload, created_at
             FROM external_effect_fault_suite_evidence
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(decode).transpose()
    }
}
