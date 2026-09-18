use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use vestrace_application::{
    ApplicationError, ExternalEffectRecoveryCandidate, ExternalEffectRepository,
    LostDispatchAdoption, ProviderResultReceiptEvidence, RequestContext, UndeliveredOutcome,
    UnitOfWork, WORKER_PRESENCE_LAPSE_AFTER,
};
use vestrace_domain::external_effects::{
    EffectLifecycleStatus, ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation,
    ReconciliationOutcome,
};
use vestrace_domain::{
    ExternalEffectId, ExternalEffectLifecycleTransitionId, ExternalEffectReceiptId,
    ExternalReconciliationId, PolicyDecision, PolicyDecisionId, Timestamp, WorkerId,
};

use super::{PgScopedTransaction, PgStore};

// The recovery query is deliberately one shared artifact: planner evidence must
// explain the same CTE, joins, predicates, union and ordering production runs.
const RECONCILIATION_CANDIDATES_SQL: &str = r#"WITH candidates AS (
    SELECT i.id AS intent_id, i.workspace_id, i.adapter, i.payload AS intent_payload,
           r.id AS receipt_id, r.effect_id AS receipt_effect_id, r.outcome_status,
           r.payload AS receipt_payload, NULL::UUID AS dispatch_transition_id,
           FALSE AS dispatch_already_adopted, r.created_at AS candidate_at
    FROM external_effect_receipts r
    JOIN external_effect_intents i ON i.id = r.effect_id AND i.workspace_id = r.workspace_id
    LEFT JOIN LATERAL (
        SELECT x.outcome, x.reconciled_at FROM external_reconciliations x
        WHERE x.effect_id = i.id AND x.receipt_id = r.id AND x.workspace_id = i.workspace_id
        ORDER BY x.reconciled_at DESC, x.id DESC LIMIT 1
    ) last ON TRUE
    LEFT JOIN LATERAL (
        SELECT attempt.attempted_at FROM external_effect_recovery_attempts attempt
        WHERE attempt.effect_id = i.id AND attempt.workspace_id = i.workspace_id
        ORDER BY attempt.attempted_at DESC, attempt.id DESC LIMIT 1
    ) failed ON TRUE
    WHERE i.workspace_id = $1 AND r.workspace_id = $1
      AND r.outcome_status IN ('unknown', 'acknowledged')
      AND (last.outcome IS NULL OR (last.outcome <> ALL($2) AND last.reconciled_at < $3))
      AND (failed.attempted_at IS NULL
           OR (last.reconciled_at IS NOT NULL AND last.reconciled_at > failed.attempted_at)
           OR failed.attempted_at < $4)
    UNION ALL
    SELECT i.id AS intent_id, i.workspace_id, i.adapter, i.payload AS intent_payload,
           NULL::UUID AS receipt_id, NULL::UUID AS receipt_effect_id,
           NULL::TEXT AS outcome_status, NULL::JSONB AS receipt_payload,
           CASE WHEN dispatch.cause = 'dispatch_lost' THEN dispatch.cause_ref::UUID ELSE dispatch.id END AS dispatch_transition_id,
           dispatch.cause = 'dispatch_lost' AS dispatch_already_adopted,
           dispatch.recorded_at AS candidate_at
    FROM external_effect_lifecycle_transitions dispatch
    JOIN external_effect_intents i ON i.id = dispatch.effect_id AND i.workspace_id = dispatch.workspace_id
    LEFT JOIN external_effect_worker_presence owner_presence
      ON owner_presence.workspace_id = dispatch.workspace_id
     AND owner_presence.worker_id = dispatch.dispatch_owner
    LEFT JOIN LATERAL (
        SELECT x.outcome, x.reconciled_at FROM external_reconciliations x
        WHERE x.effect_id = i.id AND x.receipt_id IS NULL AND x.workspace_id = i.workspace_id
        ORDER BY x.reconciled_at DESC, x.id DESC LIMIT 1
    ) last ON TRUE
    LEFT JOIN LATERAL (
        SELECT attempt.attempted_at FROM external_effect_recovery_attempts attempt
        WHERE attempt.effect_id = i.id AND attempt.workspace_id = i.workspace_id
        ORDER BY attempt.attempted_at DESC, attempt.id DESC LIMIT 1
    ) failed ON TRUE
    WHERE dispatch.workspace_id = $1 AND i.workspace_id = $1
      AND ((dispatch.status = 'dispatching' AND dispatch.dispatch_expires_at IS NOT NULL
            AND (dispatch.dispatch_expires_at < $5
                 OR (owner_presence.worker_id IS NOT NULL
                     AND (owner_presence.stopped_at IS NOT NULL
                          OR owner_presence.last_reported_at < $6))))
           OR (dispatch.status = 'unknown' AND dispatch.cause = 'dispatch_lost' AND dispatch.recorded_at < $3))
      AND NOT EXISTS (SELECT 1 FROM external_effect_receipts r WHERE r.effect_id = i.id AND r.workspace_id = i.workspace_id)
      AND NOT EXISTS (SELECT 1 FROM external_effect_lifecycle_transitions newer WHERE newer.effect_id = dispatch.effect_id AND newer.workspace_id = dispatch.workspace_id AND newer.ordinal > dispatch.ordinal)
      AND (last.outcome IS NULL OR (last.outcome <> ALL($2) AND last.reconciled_at < $3))
      AND (failed.attempted_at IS NULL
           OR (last.reconciled_at IS NOT NULL AND last.reconciled_at > failed.attempted_at)
           OR failed.attempted_at < $4)
)
SELECT intent_id, workspace_id, adapter, intent_payload, receipt_id,
       receipt_effect_id, outcome_status, receipt_payload, dispatch_transition_id,
       dispatch_already_adopted
FROM candidates
ORDER BY candidate_at ASC, intent_id ASC, receipt_id ASC NULLS FIRST
LIMIT $7"#;

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
struct AuthorizationRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    policy_id: Option<uuid::Uuid>,
    policy_version: String,
    subject_id: uuid::Uuid,
    capability: String,
    operation: String,
    resource_scope: String,
    result: String,
    reason: String,
    input_state: Value,
    matched_grant_id: Option<uuid::Uuid>,
    decided_at: Timestamp,
    payload: Value,
}

#[derive(Debug, FromRow)]
struct ReconciliationRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    receipt_id: Option<uuid::Uuid>,
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
    receipt_id: Option<uuid::Uuid>,
    receipt_effect_id: Option<uuid::Uuid>,
    outcome_status: Option<String>,
    receipt_payload: Option<Value>,
    dispatch_transition_id: Option<uuid::Uuid>,
    dispatch_already_adopted: bool,
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

fn indexed_optional_uuid(
    payload: &Value,
    field: &str,
) -> Result<Option<uuid::Uuid>, ApplicationError> {
    match payload.get(field) {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) => uuid::Uuid::parse_str(value)
            .map(Some)
            .map_err(storage_error),
        Some(_) => Err(storage_error(format!(
            "external reconciliation payload has invalid {field}"
        ))),
    }
}

fn lifecycle_status(value: &str) -> Result<EffectLifecycleStatus, ApplicationError> {
    match value {
        "prepared" => Ok(EffectLifecycleStatus::Prepared),
        "authorized" => Ok(EffectLifecycleStatus::Authorized),
        "dispatching" => Ok(EffectLifecycleStatus::Dispatching),
        "acknowledged" => Ok(EffectLifecycleStatus::Acknowledged),
        "failed" => Ok(EffectLifecycleStatus::Failed),
        "unknown" => Ok(EffectLifecycleStatus::Unknown),
        "confirmed" => Ok(EffectLifecycleStatus::Confirmed),
        "reconciling" => Ok(EffectLifecycleStatus::Reconciling),
        other => Err(storage_error(format!(
            "external effect lifecycle contains unknown status {other:?}"
        ))),
    }
}

fn decode_authorization(row: AuthorizationRow) -> Result<PolicyDecision, ApplicationError> {
    let decision: PolicyDecision = decode(row.payload)?;
    // Named rather than one long disjunction: a projection that disagrees with
    // its payload is a storage fault somebody has to find, and "does not match"
    // without saying which column costs whoever reads it an afternoon.
    let disagreement = [
        ("id", decision.id.as_uuid() != row.id),
        (
            "workspace_id",
            decision.workspace_id.as_uuid() != row.workspace_id,
        ),
        (
            "policy_id",
            decision.policy_id.map(|id| id.as_uuid()) != row.policy_id,
        ),
        (
            "policy_version",
            decision.policy_version != row.policy_version,
        ),
        (
            "subject_id",
            decision.subject_id.as_uuid() != row.subject_id,
        ),
        (
            "capability",
            decision.capability.to_string() != row.capability,
        ),
        ("operation", decision.operation != row.operation),
        (
            "resource_scope",
            decision.resource_scope != row.resource_scope,
        ),
        ("result", enum_name(decision.result)? != row.result),
        ("reason", enum_name(decision.reason)? != row.reason),
        (
            "input_state",
            json(&decision.input_state)? != row.input_state,
        ),
        (
            "matched_grant_id",
            decision.matched_grant_id.map(|id| id.as_uuid()) != row.matched_grant_id,
        ),
        // PostgreSQL TIMESTAMPTZ stores microseconds while chrono carries
        // nanoseconds, and it **rounds** rather than truncating: a decision
        // made at .123456789 is stored as .123457, whereas `timestamp_micros`
        // on the payload truncates to .123456. Comparing the two directly
        // rejects a faithfully persisted decision one time in two.
        //
        // A microsecond is exactly the granularity the column has, so a
        // difference within it is the storage boundary and anything larger is
        // a projection describing a different decision — which is what this
        // check exists to catch.
        (
            "decided_at",
            (decision.decided_at.timestamp_micros() - row.decided_at.timestamp_micros()).abs() > 1,
        ),
    ]
    .into_iter()
    .find_map(|(field, differs)| differs.then_some(field));

    if let Some(field) = disagreement {
        return Err(storage_error(format!(
            "external effect authorization column `{field}` does not match its payload"
        )));
    }
    Ok(decision)
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

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL effect transaction".to_owned())
        })
}

async fn insert_intent_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    intent: &ExternalEffectIntent,
) -> Result<(), ApplicationError> {
    if intent.workspace_id() != context.workspace_id {
        return Err(ApplicationError::Policy(
            "an external effect intent cannot be recorded into another workspace".into(),
        ));
    }

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
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;

    if result.rows_affected() == 1 {
        sqlx::query(
            "INSERT INTO external_effect_lifecycle_transitions \
                 (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
             VALUES ($1, $2, 'prepared', 'intent_recorded', $3, $4)",
        )
        .bind(intent.id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(intent.id().to_string())
        .bind(intent.created_at())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
        return Ok(());
    }

    let row = sqlx::query_as::<_, IntentRow>(
        "SELECT id, workspace_id, adapter, payload
         FROM external_effect_intents WHERE id = $1 AND workspace_id = $2",
    )
    .bind(intent.id().as_uuid())
    .bind(context.workspace_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;
    let existing = row
        .map(|row| {
            let existing: ExternalEffectIntent = decode(row.payload)?;
            if existing.id().as_uuid() != row.id
                || existing.workspace_id().as_uuid() != row.workspace_id
                || existing.adapter() != row.adapter
            {
                return Err(storage_error(
                    "external effect intent indexed metadata does not match payload",
                ));
            }
            Ok(existing)
        })
        .transpose()?;
    verify_insert(
        std::future::ready(Ok(existing)),
        intent,
        &format!(
            "external effect intent id {} already contains different evidence",
            intent.id()
        ),
    )
    .await
}

async fn record_authorization_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    effect_id: ExternalEffectId,
    decision: &PolicyDecision,
) -> Result<(), ApplicationError> {
    if decision.workspace_id != context.workspace_id {
        return Err(ApplicationError::Policy(
            "an external effect authorization cannot be recorded into another workspace".into(),
        ));
    }

    let policy_id = decision.policy_id.map(|id| id.as_uuid());
    let capability = decision.capability.to_string();
    let result = enum_name(decision.result)?;
    let reason = enum_name(decision.reason)?;
    let input_state = json(&decision.input_state)?;
    let matched_grant_id = decision.matched_grant_id.map(|id| id.as_uuid());
    let payload = json(decision)?;
    let inserted = sqlx::query(
        "INSERT INTO external_effect_authorizations ( \
             id, effect_id, workspace_id, policy_id, policy_version, subject_id, \
             capability, operation, resource_scope, result, reason, input_state, \
             matched_grant_id, decided_at, payload \
         ) \
         SELECT $1, id, workspace_id, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                $13, $14, $15 \
         FROM external_effect_intents \
         WHERE id = $2 AND workspace_id = $3",
    )
    .bind(decision.id.as_uuid())
    .bind(effect_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(policy_id)
    .bind(&decision.policy_version)
    .bind(decision.subject_id.as_uuid())
    .bind(capability)
    .bind(&decision.operation)
    .bind(&decision.resource_scope)
    .bind(&result)
    .bind(&reason)
    .bind(input_state)
    .bind(matched_grant_id)
    .bind(decision.decided_at)
    .bind(payload)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;

    if inserted.rows_affected() != 1 {
        return Err(storage_error(
            "external effect authorization could not be recorded",
        ));
    }

    if decision.is_allowed() {
        sqlx::query(
            "INSERT INTO external_effect_lifecycle_transitions \
                 (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
             VALUES ($1, $2, 'authorized', 'authorization_recorded', $3, $4)",
        )
        .bind(effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(decision.id.to_string())
        .bind(decision.decided_at)
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

async fn insert_receipt_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    receipt: &ExternalEffectReceipt,
    provider_result_evidence: Option<ProviderResultReceiptEvidence>,
) -> Result<(), ApplicationError> {
    let outcome_status = enum_name(receipt.outcome_status())?;
    let mut payload = json(receipt)?;
    if let Some(evidence) = provider_result_evidence {
        let object = payload.as_object_mut().ok_or_else(|| {
            storage_error("external effect receipt did not serialize as an object")
        })?;
        object.insert(
            "provider_result_recovery".into(),
            serde_json::json!({
                "advance_work_item_id": evidence.advance_work_item_id(),
                "finish_reason": evidence.finish_reason(),
                "usage_known": evidence.usage_known(),
                "prompt_tokens": evidence.prompt_tokens(),
                "completion_tokens": evidence.completion_tokens(),
            }),
        );
    }
    let expected_provider_result_evidence = payload.get("provider_result_recovery").cloned();
    let result = sqlx::query(
        "INSERT INTO external_effect_receipts (id, effect_id, workspace_id, outcome_status, payload)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(receipt.id().as_uuid())
    .bind(receipt.effect_id().as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(&outcome_status)
    .bind(&payload)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;

    if result.rows_affected() == 1 {
        sqlx::query(
            "INSERT INTO external_effect_lifecycle_transitions \
                 (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
             SELECT $1, $2, $3, 'receipt_recorded', $4, $5 \
             FROM external_effect_intents \
             WHERE id = $1 AND workspace_id = $2",
        )
        .bind(receipt.effect_id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(&outcome_status)
        .bind(receipt.id().to_string())
        .bind(receipt.recorded_at())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
        return Ok(());
    }

    let row = sqlx::query_as::<_, ReceiptRow>(
        "SELECT id, effect_id, outcome_status, payload
         FROM external_effect_receipts WHERE id = $1 AND workspace_id = $2",
    )
    .bind(receipt.id().as_uuid())
    .bind(context.workspace_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;
    let existing = row
        .map(|row| {
            if provider_result_evidence.is_some()
                && row.payload.get("provider_result_recovery")
                    != expected_provider_result_evidence.as_ref()
            {
                return Err(conflict(
                    "external effect receipt already contains different provider-result evidence",
                ));
            }
            let existing: ExternalEffectReceipt = decode(row.payload)?;
            if existing.id().as_uuid() != row.id
                || existing.effect_id().as_uuid() != row.effect_id
                || enum_name(existing.outcome_status())? != row.outcome_status
            {
                return Err(storage_error(
                    "external effect receipt indexed metadata does not match payload",
                ));
            }
            Ok(existing)
        })
        .transpose()?;
    verify_insert(
        std::future::ready(Ok(existing)),
        receipt,
        &format!(
            "external effect receipt id {} already contains different evidence",
            receipt.id()
        ),
    )
    .await
}

async fn record_dispatch_started_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    effect_id: ExternalEffectId,
    dispatch_owner: WorkerId,
    dispatch_expires_at: Timestamp,
    recorded_at: Timestamp,
) -> Result<ExternalEffectLifecycleTransitionId, ApplicationError> {
    let transition_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at, \
              dispatch_owner, dispatch_expires_at) \
         SELECT id, workspace_id, 'dispatching', 'dispatch_started', id::text, $5, $3, $4 \
         FROM external_effect_intents \
         WHERE id = $1 AND workspace_id = $2 \
         RETURNING id",
    )
    .bind(effect_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(dispatch_owner.to_string())
    .bind(dispatch_expires_at)
    .bind(recorded_at)
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?
    .ok_or_else(|| storage_error("external effect dispatch could not be recorded"))?;
    Ok(ExternalEffectLifecycleTransitionId::from_uuid(
        transition_id,
    ))
}

async fn adopt_lost_dispatch_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    effect_id: ExternalEffectId,
    dispatch_transition_id: ExternalEffectLifecycleTransitionId,
    recorded_at: Timestamp,
) -> Result<LostDispatchAdoption, ApplicationError> {
    let inserted = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         SELECT dispatch.effect_id, dispatch.workspace_id, \
                'unknown', 'dispatch_lost', dispatch.id::text, $4 \
           FROM external_effect_lifecycle_transitions dispatch \
         WHERE dispatch.id = $3 \
            AND dispatch.effect_id = $1 \
            AND dispatch.workspace_id = $2 \
            AND dispatch.status = 'dispatching' \
         ON CONFLICT (workspace_id, effect_id, cause_ref) \
             WHERE cause = 'dispatch_lost' DO NOTHING \
         RETURNING id",
    )
    .bind(effect_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(dispatch_transition_id.as_uuid())
    .bind(recorded_at)
    .fetch_optional(&mut *connection)
    .await;
    match inserted {
        Ok(Some(_)) => Ok(LostDispatchAdoption::Adopted),
        Ok(None) => {
            let already_adopted = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (
                    SELECT 1 FROM external_effect_lifecycle_transitions
                     WHERE workspace_id=$1 AND effect_id=$2
                       AND status='unknown' AND cause='dispatch_lost'
                       AND cause_ref=$3
                )",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(effect_id.as_uuid())
            .bind(dispatch_transition_id.as_uuid().to_string())
            .fetch_one(&mut *connection)
            .await
            .map_err(storage_error)?;
            if already_adopted {
                Ok(LostDispatchAdoption::AlreadyAdopted)
            } else {
                Err(storage_error(
                    "external effect dispatch could not be adopted",
                ))
            }
        }
        Err(error) => Err(storage_error(error)),
    }
}

#[async_trait]
impl ExternalEffectRepository for PgExternalEffectRepository {
    async fn insert_intent(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        insert_intent_on(scoped.connection(), context, intent).await?;
        scoped.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn save_intent_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        insert_intent_on(transaction.connection(), context, intent).await
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

    async fn record_authorization(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        decision: &PolicyDecision,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        record_authorization_on(scoped.connection(), context, effect_id, decision).await?;
        scoped.commit().await.map_err(storage_error)
    }

    async fn record_authorization_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        effect_id: ExternalEffectId,
        decision: &PolicyDecision,
    ) -> Result<(), ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        record_authorization_on(transaction.connection(), context, effect_id, decision).await
    }

    async fn find_authorization(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        id: PolicyDecisionId,
    ) -> Result<Option<PolicyDecision>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, AuthorizationRow>(
            "SELECT id, workspace_id, policy_id, policy_version, subject_id, \
                    capability, operation, resource_scope, result, reason, input_state, \
                    matched_grant_id, decided_at, payload \
             FROM external_effect_authorizations \
             WHERE id = $1 AND effect_id = $2 AND workspace_id = $3",
        )
        .bind(id.as_uuid())
        .bind(effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        row.map(decode_authorization).transpose()
    }

    async fn insert_receipt(
        &self,
        context: &RequestContext,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        insert_receipt_on(scoped.connection(), context, receipt, None).await?;
        scoped.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn insert_receipt_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        insert_receipt_on(transaction.connection(), context, receipt, None).await
    }

    async fn insert_provider_result_receipt_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        receipt: &ExternalEffectReceipt,
        evidence: ProviderResultReceiptEvidence,
    ) -> Result<(), ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        insert_receipt_on(transaction.connection(), context, receipt, Some(evidence)).await
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

    async fn find_receipt_by_effect(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, ReceiptRow>(
            "SELECT r.id, r.effect_id, r.outcome_status, r.payload \
             FROM external_effect_receipts r \
             LEFT JOIN external_effect_lifecycle_transitions t \
               ON t.effect_id = r.effect_id \
              AND t.workspace_id = r.workspace_id \
              AND t.cause = 'receipt_recorded' \
              AND t.cause_ref = r.id::text \
             WHERE r.effect_id = $1 AND r.workspace_id = $2 \
             ORDER BY (t.id IS NOT NULL) DESC, t.ordinal DESC, \
                      r.created_at DESC, r.id DESC \
             LIMIT 1",
        )
        .bind(effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        row.map(|row| {
            let receipt: ExternalEffectReceipt = decode(row.payload)?;
            if receipt.id().as_uuid() != row.id
                || receipt.effect_id().as_uuid() != row.effect_id
                || enum_name(receipt.outcome_status())? != row.outcome_status
            {
                return Err(storage_error(
                    "external effect receipt indexed metadata does not match payload",
                ));
            }
            Ok(receipt)
        })
        .transpose()
    }

    async fn record_worker_presence(
        &self,
        context: &RequestContext,
        worker_id: WorkerId,
        started_at: Timestamp,
        last_reported_at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO external_effect_worker_presence \
                 (workspace_id, worker_id, started_at, last_reported_at, stopped_at) \
             VALUES ($1, $2, $3, $4, NULL) \
             ON CONFLICT (workspace_id, worker_id) DO UPDATE \
             SET started_at = EXCLUDED.started_at, \
                 last_reported_at = EXCLUDED.last_reported_at, \
                 stopped_at = NULL \
             WHERE external_effect_worker_presence.workspace_id = $1 \
               AND external_effect_worker_presence.worker_id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(worker_id.to_string())
        .bind(started_at)
        .bind(last_reported_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)
    }

    async fn clear_worker_presence(
        &self,
        context: &RequestContext,
        worker_id: WorkerId,
        stopped_at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let cleared = sqlx::query_scalar::<_, String>(
            "UPDATE external_effect_worker_presence \
             SET stopped_at = $3 \
             WHERE workspace_id = $1 AND worker_id = $2 \
             RETURNING worker_id",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(worker_id.to_string())
        .bind(stopped_at)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        if cleared.is_none() {
            scoped.rollback().await.map_err(storage_error)?;
            return Err(storage_error(
                "external effect worker presence could not be cleared",
            ));
        }
        scoped.commit().await.map_err(storage_error)
    }

    async fn record_dispatch_started(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        dispatch_owner: WorkerId,
        dispatch_expires_at: Timestamp,
        recorded_at: Timestamp,
    ) -> Result<ExternalEffectLifecycleTransitionId, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let transition_id = record_dispatch_started_on(
            scoped.connection(),
            context,
            effect_id,
            dispatch_owner,
            dispatch_expires_at,
            recorded_at,
        )
        .await?;
        scoped.commit().await.map_err(storage_error)?;
        Ok(transition_id)
    }

    async fn record_dispatch_started_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        effect_id: ExternalEffectId,
        dispatch_owner: WorkerId,
        dispatch_expires_at: Timestamp,
        recorded_at: Timestamp,
    ) -> Result<ExternalEffectLifecycleTransitionId, ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        record_dispatch_started_on(
            transaction.connection(),
            context,
            effect_id,
            dispatch_owner,
            dispatch_expires_at,
            recorded_at,
        )
        .await
    }

    async fn adopt_lost_dispatch(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        dispatch_transition_id: ExternalEffectLifecycleTransitionId,
        recorded_at: Timestamp,
    ) -> Result<LostDispatchAdoption, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let adoption = adopt_lost_dispatch_on(
            scoped.connection(),
            context,
            effect_id,
            dispatch_transition_id,
            recorded_at,
        )
        .await;
        match adoption {
            Ok(adoption) => {
                scoped.commit().await.map_err(storage_error)?;
                Ok(adoption)
            }
            Err(error) => {
                scoped.rollback().await.map_err(storage_error)?;
                Err(error)
            }
        }
    }

    async fn adopt_lost_dispatch_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        effect_id: ExternalEffectId,
        dispatch_transition_id: ExternalEffectLifecycleTransitionId,
        recorded_at: Timestamp,
    ) -> Result<LostDispatchAdoption, ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        adopt_lost_dispatch_on(
            transaction.connection(),
            context,
            effect_id,
            dispatch_transition_id,
            recorded_at,
        )
        .await
    }

    async fn count_deadline_less_dispatching_transitions(
        &self,
        context: &RequestContext,
    ) -> Result<u64, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE workspace_id = $1 \
               AND status = 'dispatching' \
               AND dispatch_expires_at IS NULL",
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        u64::try_from(count)
            .map_err(|_| storage_error("external effect lifecycle count was negative"))
    }

    async fn find_lifecycle_status(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<Option<EffectLifecycleStatus>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT transition.status \
             FROM external_effect_lifecycle_transitions transition \
             WHERE transition.effect_id = $1 AND transition.workspace_id = $2 \
               AND ( \
                    transition.cause <> 'dispatch_lost' \
                    OR NOT EXISTS ( \
                        SELECT 1 FROM external_effect_lifecycle_transitions receipt \
                        WHERE receipt.effect_id = transition.effect_id \
                          AND receipt.workspace_id = transition.workspace_id \
                          AND receipt.cause = 'receipt_recorded' \
                    ) \
               ) \
             ORDER BY transition.ordinal DESC LIMIT 1",
        )
        .bind(effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        status.as_deref().map(lifecycle_status).transpose()
    }

    async fn record_failed_recovery_attempt(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        attempted_at: Timestamp,
        failure_reason: &str,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let inserted = sqlx::query(
            "INSERT INTO external_effect_recovery_attempts \
                 (effect_id, workspace_id, attempted_at, failure_reason) \
             SELECT $1, $2, $3, $4 \
             FROM external_effect_intents \
             WHERE id = $1 AND workspace_id = $2",
        )
        .bind(effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(attempted_at)
        .bind(failure_reason)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        if inserted.rows_affected() == 1 {
            Ok(())
        } else {
            Err(storage_error("effect evidence not found"))
        }
    }

    async fn insert_reconciliation(
        &self,
        context: &RequestContext,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError> {
        let outcome = enum_name(reconciliation.outcome())?;
        let evidence_strength = enum_name(reconciliation.evidence_strength())?;
        let payload = json(reconciliation)?;
        let effect_id = reconciliation.effect_id().as_uuid();
        let receipt_id = reconciliation.receipt_id().map(|id| id.as_uuid());
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

        if result.rows_affected() == 1 && reconciliation.outcome().is_settled() {
            sqlx::query(
                "INSERT INTO external_effect_lifecycle_transitions \
                     (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
                 SELECT $1, $2, 'reconciling', 'outcome_settled', $3, $4 \
                 FROM external_effect_intents WHERE id = $1 AND workspace_id = $2",
            )
            .bind(reconciliation.effect_id().as_uuid())
            .bind(context.workspace_id.as_uuid())
            .bind(reconciliation.id().to_string())
            .bind(reconciliation.reconciled_at())
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;
        }

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
            let payload_receipt_id = indexed_optional_uuid(&payload, "receipt_id")?;
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
        let delivered = sqlx::query_as::<_, (uuid::Uuid, String)>(
            "UPDATE external_reconciliations \
                SET notified_at = $3 \
              WHERE id = $1 AND workspace_id = $2 AND notified_at IS NULL \
              RETURNING effect_id, outcome",
        )
        .bind(reconciliation_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(at)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        if let Some((effect_id, outcome)) = delivered {
            let status = match outcome.as_str() {
                "confirmed" => "confirmed",
                "not_applied" => "failed",
                _ => {
                    return Err(storage_error(
                        "only a settled external effect outcome can be delivered",
                    ));
                }
            };
            sqlx::query(
                "INSERT INTO external_effect_lifecycle_transitions \
                     (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
                 SELECT $1, $2, $3, 'outcome_delivered', $4, $5 \
                 FROM external_effect_intents WHERE id = $1 AND workspace_id = $2",
            )
            .bind(effect_id)
            .bind(context.workspace_id.as_uuid())
            .bind(status)
            .bind(reconciliation_id.to_string())
            .bind(at)
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;
        }

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
    /// nothing, whose most recent inconclusive answer is older than its cutoff,
    /// and whose most recent failed attempt is older than its own cutoff. A
    /// later reconciliation supersedes a failed attempt without deleting
    /// evidence. The settled names come from the domain rather than being
    /// literals here, so renaming a variant cannot leave this matching nothing
    /// — which would put every already-confirmed effect back in the sweep.
    async fn find_reconciliation_candidates(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
        retry_failed_before: Timestamp,
        dispatch_expired_before: Timestamp,
        limit: u32,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let worker_lapsed_before = dispatch_expired_before - WORKER_PRESENCE_LAPSE_AFTER;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query_as::<_, RecoveryCandidateRow>(RECONCILIATION_CANDIDATES_SQL)
            .bind(context.workspace_id.as_uuid())
            .bind(ReconciliationOutcome::settled_names())
            .bind(retry_unsettled_before)
            .bind(retry_failed_before)
            .bind(dispatch_expired_before)
            .bind(worker_lapsed_before)
            .bind(i64::from(limit))
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

                let receipt = row
                    .receipt_payload
                    .map(decode::<ExternalEffectReceipt>)
                    .transpose()?;
                if let Some(receipt) = receipt.as_ref() {
                    if Some(receipt.id().as_uuid()) != row.receipt_id
                        || Some(receipt.effect_id().as_uuid()) != row.receipt_effect_id
                        || Some(enum_name(receipt.outcome_status())?) != row.outcome_status
                    {
                        return Err(storage_error(
                            "external effect recovery receipt indexed metadata does not match payload",
                        ));
                    }
                }

                match receipt {
                    Some(receipt) => ExternalEffectRecoveryCandidate::new(intent, receipt),
                    None => {
                        let dispatch_transition_id = row.dispatch_transition_id.ok_or_else(|| {
                            storage_error(
                                "receipt-less recovery candidate is missing dispatch transition id",
                            )
                        })?;
                        Ok(ExternalEffectRecoveryCandidate::for_lost_dispatch(
                            intent,
                            ExternalEffectLifecycleTransitionId::from_uuid(
                                dispatch_transition_id,
                            ),
                            row.dispatch_already_adopted,
                        ))
                    }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod planner_tests {
    use super::{RECONCILIATION_CANDIDATES_SQL, WORKER_PRESENCE_LAPSE_AFTER};
    use sqlx::PgPool;
    use vestrace_domain::external_effects::ReconciliationOutcome;
    use vestrace_domain::time::now;

    /// Whether the plan reaches `external_effect_receipts` through the widened
    /// partial index.
    ///
    /// # Why the two names are not looked for on one node
    ///
    /// They were, and the assertion could never hold. An index scan carries both
    /// `Relation Name` and `Index Name`, but a **bitmap** scan splits them: the
    /// `Bitmap Heap Scan` names the relation and its child `Bitmap Index Scan`
    /// names the index. PostgreSQL chose the bitmap shape here, so a conjunction
    /// on one node reported "no index" about a plan whose first line is
    /// `Index Name: idx_external_effect_receipts_reconciliation_candidates`.
    ///
    /// So the relation is found first and the index looked for beneath it, which
    /// is where the planner actually puts it. Matching the index name alone
    /// would also pass today — the name belongs to one table — but it would stop
    /// checking the thing the test is named for the moment an index of that name
    /// existed elsewhere.
    fn plan_uses_receipt_index(value: &serde_json::Value) -> bool {
        fn names_the_index(value: &serde_json::Value) -> bool {
            match value {
                serde_json::Value::Object(fields) => {
                    fields
                        .get("Index Name")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|index| {
                            index == "idx_external_effect_receipts_reconciliation_candidates"
                        })
                        || fields.values().any(names_the_index)
                }
                serde_json::Value::Array(values) => values.iter().any(names_the_index),
                _ => false,
            }
        }

        match value {
            serde_json::Value::Object(fields) => {
                let reaches_receipts = fields
                    .get("Relation Name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|relation| relation == "external_effect_receipts")
                    && names_the_index(value);
                reaches_receipts || fields.values().any(plan_uses_receipt_index)
            }
            serde_json::Value::Array(values) => values.iter().any(plan_uses_receipt_index),
            _ => false,
        }
    }

    fn has_receipt_seq_scan(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => {
                let is_receipt_scan = fields
                    .get("Node Type")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|node| node == "Seq Scan")
                    && fields
                        .get("Relation Name")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|relation| relation == "external_effect_receipts");
                is_receipt_scan || fields.values().any(has_receipt_seq_scan)
            }
            serde_json::Value::Array(values) => values.iter().any(has_receipt_seq_scan),
            _ => false,
        }
    }

    /// Mutant caught: changing the actual recovery CTE so its receipt branch
    /// cannot use the widened partial index forces a workspace receipt scan.
    #[sqlx::test(migrations = "../../migrations")]
    async fn recovery_candidate_query_uses_the_widened_receipt_partial_index(pool: PgPool) {
        let workspace_id = uuid::Uuid::now_v7();
        sqlx::query("SELECT set_config('vestrace.workspace_id', $1, false)")
            .bind(workspace_id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "WITH inserted AS (
                 INSERT INTO external_effect_intents (id, workspace_id, adapter, payload)
                 SELECT gen_random_uuid(), $1, 'planner', '{}'::JSONB
                 FROM generate_series(1, 4000)
                 RETURNING id, workspace_id
             )
             INSERT INTO external_effect_receipts
                 (id, effect_id, workspace_id, outcome_status, payload, created_at)
             SELECT gen_random_uuid(), id, workspace_id,
                    CASE WHEN row_number() OVER () % 8 = 0 THEN 'acknowledged' ELSE 'failed' END,
                    '{}'::JSONB, NOW()
             FROM inserted",
        )
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("ANALYZE external_effect_receipts")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("ANALYZE external_effect_intents")
            .execute(&pool)
            .await
            .unwrap();

        let cutoff = now();
        let plan = sqlx::query_scalar::<_, serde_json::Value>(&format!(
            "EXPLAIN (FORMAT JSON, COSTS OFF) {RECONCILIATION_CANDIDATES_SQL}"
        ))
        .bind(workspace_id)
        .bind(ReconciliationOutcome::settled_names())
        .bind(cutoff)
        .bind(cutoff)
        .bind(cutoff)
        .bind(cutoff - WORKER_PRESENCE_LAPSE_AFTER)
        .bind(8_i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            plan_uses_receipt_index(&plan),
            "the exact recovery query did not use its receipt partial index: {plan}"
        );
        assert!(
            !has_receipt_seq_scan(&plan),
            "the exact recovery query scanned external_effect_receipts: {plan}"
        );
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;

    struct ForeignUnitOfWork;

    #[async_trait]
    impl UnitOfWork for ForeignUnitOfWork {
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[test]
    fn a_foreign_unit_of_work_is_a_typed_error() {
        let mut unit_of_work = ForeignUnitOfWork;
        match postgres_transaction(&mut unit_of_work) {
            Err(ApplicationError::Internal(message)) => {
                assert_eq!(message, "expected PostgreSQL effect transaction")
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("foreign unit of work was accepted"),
        }
    }
}
