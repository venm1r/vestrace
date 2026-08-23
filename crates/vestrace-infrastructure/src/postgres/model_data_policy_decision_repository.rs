use async_trait::async_trait;
use serde::Serialize;
use sqlx::PgPool;
use vestrace_application::{
    ApplicationError, ModelDataPolicyDecisionRecord, ModelDataPolicyDecisionRepository,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgModelDataPolicyDecisionRepository {
    pool: PgPool,
}

impl PgModelDataPolicyDecisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn encode<T: Serialize>(value: T, column: &str) -> Result<String, ApplicationError> {
    match serde_json::to_value(value).map_err(storage_error)? {
        serde_json::Value::String(value) => Ok(value),
        _ => Err(storage_error(format!(
            "model_data_policy_decisions.{column} was not encoded as a string"
        ))),
    }
}

#[async_trait]
impl ModelDataPolicyDecisionRepository for PgModelDataPolicyDecisionRepository {
    async fn record(&self, record: &ModelDataPolicyDecisionRecord) -> Result<(), ApplicationError> {
        sqlx::query(
            "INSERT INTO model_data_policy_decisions (
                 id, run_id, step_id, destination, classification, verdict,
                 reason, policy_version, mode, decided_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(record.id)
        .bind(record.run_id.as_uuid())
        .bind(record.step_id.as_uuid())
        .bind(encode(record.destination, "destination")?)
        .bind(encode(record.classification, "classification")?)
        .bind(if record.allowed { "allowed" } else { "denied" })
        .bind(&record.reason)
        .bind(&record.policy_version)
        .bind(encode(record.mode, "mode")?)
        .bind(record.decided_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }
}
