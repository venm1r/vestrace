use async_trait::async_trait;
use serde::Serialize;
use sqlx::{Executor, PgPool, Postgres};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    UnitOfWork,
};

use super::{PgScopedTransaction, PgStore};

#[derive(Clone, Debug)]
pub struct PgEmbeddingDataPolicyDecisionRepository {
    pool: PgPool,
}

impl PgEmbeddingDataPolicyDecisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal(
                "expected PostgreSQL embedding data-policy transaction".to_owned(),
            )
        })
}

fn encode<T: Serialize>(value: T, column: &str) -> Result<String, ApplicationError> {
    match serde_json::to_value(value).map_err(storage_error)? {
        serde_json::Value::String(value) => Ok(value),
        _ => Err(storage_error(format!(
            "embedding_data_policy_decisions.{column} was not encoded as a string"
        ))),
    }
}

async fn record_on<'e, E>(
    executor: E,
    record: &EmbeddingDataPolicyDecisionRecord,
) -> Result<(), ApplicationError>
where
    E: Executor<'e, Database = Postgres>,
{
    let delivery_attempt = record
        .delivery_attempt
        .map(i32::try_from)
        .transpose()
        .map_err(|_| {
            storage_error("embedding_data_policy_decisions.delivery_attempt does not fit INTEGER")
        })?;
    let batch_ordinal = record
        .batch_ordinal
        .map(i32::try_from)
        .transpose()
        .map_err(|_| {
            storage_error("embedding_data_policy_decisions.batch_ordinal does not fit INTEGER")
        })?;
    let unclassified_count = i32::try_from(record.unclassified_count).map_err(|_| {
        storage_error("embedding_data_policy_decisions.unclassified_count does not fit INTEGER")
    })?;
    let input_count = i32::try_from(record.input_count).map_err(|_| {
        storage_error("embedding_data_policy_decisions.input_count does not fit INTEGER")
    })?;

    sqlx::query(
        "INSERT INTO embedding_data_policy_decisions (
             id, purpose, causal_reference_id, delivery_attempt, batch_ordinal,
             destination, classification, classification_labels,
             unclassified_count, input_count, classification_allowed,
             destination_allowed, verdict, reason, policy_version, mode, decided_at
         ) VALUES (
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
             $15, $16, $17
         )",
    )
    .bind(record.id)
    .bind(encode(record.purpose, "purpose")?)
    .bind(record.causal_reference_id)
    .bind(delivery_attempt)
    .bind(batch_ordinal)
    .bind(encode(record.destination, "destination")?)
    .bind(encode(record.classification, "classification")?)
    .bind(&record.classification_labels)
    .bind(unclassified_count)
    .bind(input_count)
    .bind(record.classification_allowed)
    .bind(record.destination_allowed)
    .bind(if record.allowed { "allowed" } else { "denied" })
    .bind(&record.reason)
    .bind(&record.policy_version)
    .bind(encode(record.mode, "mode")?)
    .bind(record.decided_at)
    .execute(executor)
    .await
    .map_err(storage_error)?;
    Ok(())
}

#[async_trait]
impl EmbeddingDataPolicyDecisionRepository for PgEmbeddingDataPolicyDecisionRepository {
    async fn record(
        &self,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        record_on(&self.pool, record).await
    }

    async fn record_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        record_on(postgres_transaction(unit_of_work)?.connection(), record).await
    }
}
