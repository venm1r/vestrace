use async_trait::async_trait;
use serde::Serialize;
use sqlx::{Executor, Postgres};
use vestrace_application::{
    ApplicationError, ModelDataPolicyDecisionRecord, ModelDataPolicyDecisionRepository, UnitOfWork,
};

use super::{PgScopedTransaction, PgStore};

#[derive(Clone, Debug)]
pub struct PgModelDataPolicyDecisionRepository {
    store: PgStore,
}

impl PgModelDataPolicyDecisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal(
                "expected PostgreSQL model-data-policy transaction".to_owned(),
            )
        })
}

async fn record_on<'e, E>(
    executor: E,
    record: &ModelDataPolicyDecisionRecord,
) -> Result<(), ApplicationError>
where
    E: Executor<'e, Database = Postgres>,
{
    let recorded_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT public.vestrace_record_model_data_policy_decision(
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
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
    .fetch_one(executor)
    .await
    .map_err(storage_error)?;
    if recorded_id != record.id {
        return Err(storage_error(
            "guarded model-data-policy decision returned another identity",
        ));
    }
    Ok(())
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
        record_on(self.store.pool(), record).await
    }

    async fn record_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        record: &ModelDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        record_on(postgres_transaction(unit_of_work)?.connection(), record).await
    }
}
