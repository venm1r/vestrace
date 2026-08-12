use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use vestrace_application::{
    ApplicationError, ExternalEffectRecoveryCandidate, ExternalEffectRepository,
};
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation,
};
use vestrace_domain::{ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgExternalEffectRepository {
    pool: PgPool,
}

impl PgExternalEffectRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct IntentRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    adapter: String,
    payload: Value,
}

#[derive(Debug, FromRow)]
struct ReceiptRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    outcome_status: String,
    payload: Value,
}

#[derive(Debug, FromRow)]
struct ReconciliationRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    receipt_id: uuid::Uuid,
    outcome: String,
    evidence_strength: String,
    payload: Value,
}

#[derive(Debug, FromRow)]
struct RecoveryCandidateRow {
    intent_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    adapter: String,
    intent_payload: Value,
    receipt_id: uuid::Uuid,
    receipt_effect_id: uuid::Uuid,
    outcome_status: String,
    receipt_payload: Value,
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
        .ok_or_else(|| storage_error("external-effect enum did not serialize as a string"))
}

fn json<T: Serialize>(value: &T) -> Result<Value, ApplicationError> {
    serde_json::to_value(value).map_err(storage_error)
}

fn decode<T: DeserializeOwned>(payload: Value) -> Result<T, ApplicationError> {
    serde_json::from_value(payload).map_err(storage_error)
}

fn indexed_uuid(payload: &Value, field: &str) -> Result<uuid::Uuid, ApplicationError> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            storage_error(format!(
                "external reconciliation payload is missing {field}"
            ))
        })
        .and_then(|value| uuid::Uuid::parse_str(value).map_err(storage_error))
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
            "external-effect conflict row disappeared before verification",
        )),
    }
}

#[async_trait]
impl ExternalEffectRepository for PgExternalEffectRepository {
    async fn insert_intent(&self, intent: &ExternalEffectIntent) -> Result<(), ApplicationError> {
        let payload = json(intent)?;
        let result = sqlx::query(
            "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(intent.id().as_uuid())
        .bind(intent.workspace_id().as_uuid())
        .bind(intent.adapter())
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_intent(intent.id()),
            intent,
            &format!(
                "external effect intent id {} already contains different evidence",
                intent.id()
            ),
        )
        .await
    }

    async fn find_intent(
        &self,
        id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError> {
        let row = sqlx::query_as::<_, IntentRow>(
            "SELECT id, workspace_id, adapter, payload
             FROM external_effect_intents WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let intent: ExternalEffectIntent = decode(row.payload)?;
            if intent.id().as_uuid() != row.id
                || intent.workspace_id().as_uuid() != row.workspace_id
                || intent.adapter() != row.adapter
            {
                return Err(storage_error(
                    "external effect intent indexed metadata does not match payload",
                ));
            }
            Ok(intent)
        })
        .transpose()
    }

    async fn insert_receipt(
        &self,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        let outcome_status = enum_name(receipt.outcome_status())?;
        let payload = json(receipt)?;
        let result = sqlx::query(
            "INSERT INTO external_effect_receipts (id, effect_id, outcome_status, payload)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(receipt.id().as_uuid())
        .bind(receipt.effect_id().as_uuid())
        .bind(outcome_status)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_receipt(receipt.id()),
            receipt,
            &format!(
                "external effect receipt id {} already contains different evidence",
                receipt.id()
            ),
        )
        .await
    }

    async fn find_receipt(
        &self,
        id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        let row = sqlx::query_as::<_, ReceiptRow>(
            "SELECT id, effect_id, outcome_status, payload
             FROM external_effect_receipts WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let receipt: ExternalEffectReceipt = decode(row.payload)?;
            let expected_status = enum_name(receipt.outcome_status())?;
            if receipt.id().as_uuid() != row.id
                || receipt.effect_id().as_uuid() != row.effect_id
                || expected_status != row.outcome_status
            {
                return Err(storage_error(
                    "external effect receipt indexed metadata does not match payload",
                ));
            }
            Ok(receipt)
        })
        .transpose()
    }

    async fn insert_reconciliation(
        &self,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError> {
        let outcome = enum_name(reconciliation.outcome())?;
        let evidence_strength = enum_name(reconciliation.evidence_strength())?;
        let payload = json(reconciliation)?;
        let effect_id = indexed_uuid(&payload, "effect_id")?;
        let receipt_id = indexed_uuid(&payload, "receipt_id")?;
        let result = sqlx::query(
            "INSERT INTO external_reconciliations (
                 id, effect_id, receipt_id, outcome, evidence_strength, payload
             ) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(reconciliation.id().as_uuid())
        .bind(effect_id)
        .bind(receipt_id)
        .bind(outcome)
        .bind(evidence_strength)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_reconciliation(reconciliation.id()),
            reconciliation,
            &format!(
                "external reconciliation id {} already contains different evidence",
                reconciliation.id()
            ),
        )
        .await
    }

    async fn find_reconciliation(
        &self,
        id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError> {
        let row = sqlx::query_as::<_, ReconciliationRow>(
            "SELECT id, effect_id, receipt_id, outcome, evidence_strength, payload
             FROM external_reconciliations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            let payload = row.payload;
            let payload_effect_id = indexed_uuid(&payload, "effect_id")?;
            let payload_receipt_id = indexed_uuid(&payload, "receipt_id")?;
            let reconciliation: ExternalReconciliation = decode(payload)?;
            let expected_outcome = enum_name(reconciliation.outcome())?;
            let expected_strength = enum_name(reconciliation.evidence_strength())?;
            if reconciliation.id().as_uuid() != row.id
                || payload_effect_id != row.effect_id
                || payload_receipt_id != row.receipt_id
                || expected_outcome != row.outcome
                || expected_strength != row.evidence_strength
            {
                return Err(storage_error(
                    "external reconciliation indexed metadata does not match payload",
                ));
            }
            Ok(reconciliation)
        })
        .transpose()
    }

    async fn find_reconciliation_candidates(
        &self,
        workspace_id: vestrace_domain::WorkspaceId,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let rows = sqlx::query_as::<_, RecoveryCandidateRow>(
            "SELECT i.id AS intent_id,
                    i.workspace_id,
                    i.adapter,
                    i.payload AS intent_payload,
                    r.id AS receipt_id,
                    r.effect_id AS receipt_effect_id,
                    r.outcome_status,
                    r.payload AS receipt_payload
             FROM external_effect_intents i
             JOIN external_effect_receipts r ON r.effect_id = i.id
             LEFT JOIN external_reconciliations x
               ON x.effect_id = r.effect_id AND x.receipt_id = r.id
             WHERE i.workspace_id = $1
               AND r.outcome_status = 'unknown'
               AND x.id IS NULL
             ORDER BY r.created_at ASC, r.id ASC",
        )
        .bind(workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let intent: ExternalEffectIntent = decode(row.intent_payload)?;
                if intent.id().as_uuid() != row.intent_id
                    || intent.workspace_id().as_uuid() != row.workspace_id
                    || intent.adapter() != row.adapter
                {
                    return Err(storage_error(
                        "external effect recovery intent indexed metadata does not match payload",
                    ));
                }

                let receipt: ExternalEffectReceipt = decode(row.receipt_payload)?;
                if receipt.id().as_uuid() != row.receipt_id
                    || receipt.effect_id().as_uuid() != row.receipt_effect_id
                    || enum_name(receipt.outcome_status())? != row.outcome_status
                {
                    return Err(storage_error(
                        "external effect recovery receipt indexed metadata does not match payload",
                    ));
                }

                ExternalEffectRecoveryCandidate::new(intent, receipt)
            })
            .collect()
    }
}
