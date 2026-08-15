use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, ExternalEffectRecoveryCandidate, ExternalEffectRepository, RequestContext,
    UndeliveredOutcome,
};
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation, ReconciliationOutcome,
};
use vestrace_domain::{
    ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId, Timestamp,
};

use super::PgStore;

/// Durable external-effect intents, receipts and reconciliations.
///
/// # Why this holds a store and not a pool
///
/// These three tables had no row level security at all until migration 0144 —
/// not enabled, not forced, no policy — and the adapter read all three by id
/// alone. An `ExternalEffectId` from any tenant returned that tenant's intent,
/// and a receipt id returned the external resource id and response digest of a
/// call made on another tenant's behalf.
///
/// A receipt and a reconciliation carry no workspace in the domain: they belong
/// to an effect, and the effect belongs to a tenant. 0144 gives their tables
/// the column, and a composite foreign key onto `(id, workspace_id)` of the
/// parent keeps the two from ever disagreeing — so this adapter can bind the
/// workspace from the request context without that being an assertion it makes
/// alone.
#[derive(Clone, Debug)]
pub struct PgExternalEffectRepository {
    store: PgStore,
}

impl PgExternalEffectRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
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
struct UndeliveredOutcomeRow {
    reconciliation_payload: Value,
    intent_payload: Value,
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
    async fn insert_intent(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError> {
        if intent.workspace_id() != context.workspace_id {
            return Err(ApplicationError::Policy(
                "an external effect intent cannot be recorded into another workspace".into(),
            ));
        }

        let payload = json(intent)?;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let result = sqlx::query(
            "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(intent.id().as_uuid())
        .bind(intent.workspace_id().as_uuid())
        .bind(intent.adapter())
        .bind(payload)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_intent(context, intent.id()),
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
        context: &RequestContext,
        id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query_as::<_, IntentRow>(
            "SELECT id, workspace_id, adapter, payload
             FROM external_effect_intents WHERE id = $1 AND workspace_id = $2",
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

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
        context: &RequestContext,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        let outcome_status = enum_name(receipt.outcome_status())?;
        let payload = json(receipt)?;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The workspace is bound from the context, and the composite foreign
        // key onto `(id, workspace_id)` of the intent refuses a receipt filed
        // against an effect belonging to a different tenant. The adapter does
        // not have to check that itself, and could not check it reliably: the
        // receipt carries no workspace to compare.
        let result = sqlx::query(
            "INSERT INTO external_effect_receipts (id, effect_id, workspace_id, outcome_status, payload)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(receipt.id().as_uuid())
        .bind(receipt.effect_id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(outcome_status)
        .bind(payload)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_receipt(context, receipt.id()),
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
        context: &RequestContext,
        id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query_as::<_, ReceiptRow>(
            "SELECT id, effect_id, outcome_status, payload
             FROM external_effect_receipts WHERE id = $1 AND workspace_id = $2",
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

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
        context: &RequestContext,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError> {
        let outcome = enum_name(reconciliation.outcome())?;
        let evidence_strength = enum_name(reconciliation.evidence_strength())?;
        let payload = json(reconciliation)?;
        let effect_id = indexed_uuid(&payload, "effect_id")?;
        let receipt_id = indexed_uuid(&payload, "receipt_id")?;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let result = sqlx::query(
            "INSERT INTO external_reconciliations (
                 id, effect_id, receipt_id, workspace_id, outcome, evidence_strength,
                 reconciled_at, payload
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(reconciliation.id().as_uuid())
        .bind(effect_id)
        .bind(receipt_id)
        .bind(context.workspace_id.as_uuid())
        .bind(outcome)
        .bind(evidence_strength)
        // The observation's own time, not the insert's: the sweep backs off from
        // when somebody looked, and a row written later for an earlier look
        // would otherwise be asked about again too late.
        .bind(reconciliation.reconciled_at())
        .bind(payload)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        if result.rows_affected() == 1 {
            return Ok(());
        }
        verify_insert(
            self.find_reconciliation(context, reconciliation.id()),
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
        context: &RequestContext,
        id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query_as::<_, ReconciliationRow>(
            "SELECT id, effect_id, receipt_id, outcome, evidence_strength, payload
             FROM external_reconciliations WHERE id = $1 AND workspace_id = $2",
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

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

    async fn find_undelivered_outcomes(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<UndeliveredOutcome>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query_as::<_, UndeliveredOutcomeRow>(
            "SELECT x.payload AS reconciliation_payload,
                    i.payload AS intent_payload
             FROM external_reconciliations x
             JOIN external_effect_intents i
               ON i.id = x.effect_id AND i.workspace_id = x.workspace_id
             WHERE x.workspace_id = $1
               AND x.notified_at IS NULL
               AND x.outcome = ANY($2)
             ORDER BY x.reconciled_at ASC, x.id ASC
             LIMIT $3",
        )
        .bind(context.workspace_id.as_uuid())
        // Only settled outcomes are owed. An inconclusive answer is not a fact
        // about the run — it is the absence of one — and telling a run "we still
        // do not know" on every sweep would be noise, not history.
        .bind(ReconciliationOutcome::settled_names())
        .bind(i64::from(limit))
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(UndeliveredOutcome {
                    reconciliation: decode(row.reconciliation_payload)?,
                    intent: decode(row.intent_payload)?,
                })
            })
            .collect()
    }

    async fn mark_outcome_delivered(
        &self,
        context: &RequestContext,
        reconciliation_id: ExternalReconciliationId,
        at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // `notified_at IS NULL` in the predicate, so a concurrent sweep that
        // already paid this debt does not have its timestamp overwritten by a
        // later one — the record says when the run was told, not when somebody
        // last thought about telling it.
        sqlx::query(
            "UPDATE external_reconciliations
                SET notified_at = $3
              WHERE id = $1 AND workspace_id = $2 AND notified_at IS NULL",
        )
        .bind(reconciliation_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        Ok(())
    }

    /// # Why this is not "effects with no reconciliation row"
    ///
    /// It was, and that made a recorded attempt indistinguishable from a settled
    /// outcome. An `Inconclusive` reconciliation — the provider answered and
    /// could not tell us — removed the effect from this set permanently. Its
    /// receipt stayed `unknown`, nothing ever asked again, and the sweep looked
    /// clean because the thing it should have been asking about was no longer in
    /// the set it swept.
    ///
    /// The set is now effects whose *most recent* reconciliation settled
    /// nothing, and whose most recent attempt is older than the caller's cutoff.
    /// The settled names come from the domain rather than being literals here,
    /// so renaming a variant cannot leave this matching nothing — which would
    /// put every already-confirmed effect back in the sweep.
    async fn find_reconciliation_candidates(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

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
             JOIN external_effect_receipts r
               ON r.effect_id = i.id AND r.workspace_id = i.workspace_id
             LEFT JOIN LATERAL (
                 SELECT x.outcome, x.reconciled_at
                 FROM external_reconciliations x
                 WHERE x.effect_id = r.effect_id
                   AND x.receipt_id = r.id
                   AND x.workspace_id = r.workspace_id
                 ORDER BY x.reconciled_at DESC, x.id DESC
                 LIMIT 1
             ) last ON TRUE
             WHERE i.workspace_id = $1
               AND r.outcome_status = 'unknown'
               AND (
                     last.outcome IS NULL
                  OR (last.outcome <> ALL($2) AND last.reconciled_at < $3)
               )
             ORDER BY r.created_at ASC, r.id ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(ReconciliationOutcome::settled_names())
        .bind(retry_unsettled_before)
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

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
