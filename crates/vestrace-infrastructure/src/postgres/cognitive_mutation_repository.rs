use async_trait::async_trait;
use chrono::Duration;
use sqlx::{PgConnection, Row};
use vestrace_application::{
    ApplicationError, CognitiveMutationRepository, CognitiveMutationResult, RequestContext,
};
use vestrace_domain::{
    AuditEvent, Claim, ClaimId, CognitiveMutation, Conflict, ConflictId, DomainError, MutationKind,
    MutationTargetKind, ReconciliationOutcome, ReconciliationRecord, WorkspaceId, id::AuditEventId,
    time::Timestamp,
};

use super::PgStore;

pub struct PgCognitiveMutationRepository {
    store: PgStore,
}

impl PgCognitiveMutationRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl CognitiveMutationRepository for PgCognitiveMutationRepository {
    async fn apply_transactional(
        &self,
        context: &RequestContext,
        mutation: CognitiveMutation,
        reconciliation: Option<ReconciliationRecord>,
        request_hash: String,
        idempotency_key: String,
    ) -> Result<CognitiveMutationResult, ApplicationError> {
        if mutation.workspace_id != context.workspace_id || mutation.actor != context.principal_id {
            return Err(ApplicationError::Policy(
                "mutation actor or workspace does not match request context".into(),
            ));
        }

        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        lock_idempotency_key(
            transaction.connection(),
            context.workspace_id,
            &idempotency_key,
        )
        .await?;

        if let Some(result) = find_idempotent_result(
            transaction.connection(),
            context.workspace_id,
            &idempotency_key,
            &request_hash,
        )
        .await?
        {
            transaction.commit().await.map_err(storage_error)?;
            return Ok(result);
        }

        let (result_mutation, target_uuid) = match mutation.target_kind {
            MutationTargetKind::Claim => {
                let claim_id = parse_uuid::<ClaimId>(&mutation.target_id, "claim target_id")?;
                let claim = load_claim(transaction.connection(), context.workspace_id, claim_id)
                    .await?
                    .ok_or_else(|| not_found("claim does not exist"))?;
                let updated = apply_claim_mutation(claim, &mutation)?;
                persist_claim(
                    transaction.connection(),
                    context.workspace_id,
                    &updated,
                    mutation.expected_state_revision,
                )
                .await?;
                (
                    mutation.with_resulting_revision(format!(
                        "{}:{}",
                        updated.claim_id, updated.state_revision
                    )),
                    updated.claim_id.as_uuid(),
                )
            }
            MutationTargetKind::Conflict => {
                let conflict_id =
                    parse_uuid::<ConflictId>(&mutation.target_id, "conflict target_id")?;
                let conflict =
                    load_conflict(transaction.connection(), context.workspace_id, conflict_id)
                        .await?
                        .ok_or_else(|| not_found("conflict does not exist"))?;
                let updated =
                    apply_conflict_mutation(conflict, &mutation, reconciliation.as_ref())?;
                persist_conflict(
                    transaction.connection(),
                    context.workspace_id,
                    &updated,
                    mutation.expected_state_revision,
                )
                .await?;
                (
                    mutation.with_resulting_revision(format!(
                        "{}:{}",
                        updated.conflict_id, updated.state_revision
                    )),
                    updated.conflict_id.as_uuid(),
                )
            }
            _ => {
                return Err(ApplicationError::Domain(DomainError::PolicyViolation(
                    "cognitive mutation target is not supported by the C4 repository".into(),
                )));
            }
        };

        if let Some(record) = reconciliation.as_ref() {
            if record.workspace_id != context.workspace_id
                || record.resolved_by != context.principal_id
                || record.conflict_id.as_uuid() != target_uuid
            {
                return Err(ApplicationError::Policy(
                    "reconciliation identity does not match mutation context or target".into(),
                ));
            }
            persist_reconciliation(transaction.connection(), record).await?;
        }

        persist_mutation(transaction.connection(), &result_mutation).await?;

        let at = result_mutation.created_at;
        let audit = AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "cognitive.mutation.applied",
            match result_mutation.target_kind {
                MutationTargetKind::Claim => "claim",
                MutationTargetKind::Conflict => "conflict",
                MutationTargetKind::Memory => "memory",
                MutationTargetKind::MemoryRevision => "memory_revision",
            },
            target_uuid,
            serde_json::json!({
                "mutation_id": result_mutation.mutation_id,
                "target_id": result_mutation.target_id,
                "expected_state_revision": result_mutation.expected_state_revision,
                "resulting_revision_id": result_mutation.resulting_revision_id,
                "reconciliation_id": reconciliation.as_ref().map(|record| &record.reconciliation_id),
            }),
            at,
        )?;
        persist_audit(transaction.connection(), &audit).await?;

        let result = CognitiveMutationResult {
            mutation: result_mutation,
            reconciliation,
            replayed: false,
        };
        let result_payload = serde_json::to_value(&result).map_err(internal_error)?;
        persist_outbox(
            transaction.connection(),
            context.workspace_id,
            &result_payload,
            at,
        )
        .await?;
        persist_idempotency(
            transaction.connection(),
            context.workspace_id,
            &idempotency_key,
            &request_hash,
            &result_payload,
            at,
        )
        .await?;

        transaction.commit().await.map_err(storage_error)?;
        Ok(result)
    }
}

async fn lock_idempotency_key(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    key: &str,
) -> Result<(), ApplicationError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("{}:{key}", workspace_id))
        .execute(&mut *connection)
        .await
        .map_err(storage_error)
        .map(|_| ())
}

async fn find_idempotent_result(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    key: &str,
    request_hash: &str,
) -> Result<Option<CognitiveMutationResult>, ApplicationError> {
    let row = sqlx::query(
        "SELECT request_hash, response_payload
         FROM idempotency_keys
         WHERE workspace_id = $1 AND idempotency_key = $2
         FOR UPDATE",
    )
    .bind(workspace_id.as_uuid())
    .bind(key)
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;

    let Some(row) = row else { return Ok(None) };
    let stored_hash: String = row.try_get("request_hash").map_err(storage_error)?;
    if stored_hash != request_hash {
        return Err(ApplicationError::Conflict(
            "idempotency key reused with different request".into(),
        ));
    }
    let payload: Option<serde_json::Value> =
        row.try_get("response_payload").map_err(storage_error)?;
    let Some(payload) = payload else {
        return Err(ApplicationError::Conflict(
            "idempotent mutation is still in progress".into(),
        ));
    };
    let mut result: CognitiveMutationResult =
        serde_json::from_value(payload).map_err(internal_error)?;
    result.replayed = true;
    Ok(Some(result))
}

async fn load_claim(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    claim_id: ClaimId,
) -> Result<Option<Claim>, ApplicationError> {
    let row = sqlx::query(
        "SELECT workspace_id, semantic_key, subject, predicate, value,
                lifecycle_status, valid_from, valid_until, state_revision,
                created_at, updated_at
         FROM claims
         WHERE id = $1 AND workspace_id = $2
         FOR UPDATE",
    )
    .bind(claim_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;

    row.map(|row| {
        Ok(Claim {
            claim_id,
            workspace_id: WorkspaceId::from_uuid(
                row.try_get("workspace_id").map_err(storage_error)?,
            ),
            semantic_key: row.try_get("semantic_key").map_err(storage_error)?,
            subject: row.try_get("subject").map_err(storage_error)?,
            predicate: row.try_get("predicate").map_err(storage_error)?,
            value: row.try_get("value").map_err(storage_error)?,
            lifecycle_status: enum_from_text(
                row.try_get("lifecycle_status").map_err(storage_error)?,
            )?,
            valid_from: row.try_get("valid_from").map_err(storage_error)?,
            valid_until: row.try_get("valid_until").map_err(storage_error)?,
            state_revision: row
                .try_get::<i32, _>("state_revision")
                .map_err(storage_error)? as u32,
            created_at: row.try_get("created_at").map_err(storage_error)?,
            updated_at: row.try_get("updated_at").map_err(storage_error)?,
        })
    })
    .transpose()
}

async fn load_conflict(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    conflict_id: ConflictId,
) -> Result<Option<Conflict>, ApplicationError> {
    let row = sqlx::query(
        "SELECT workspace_id, conflict_kind, participant_refs, status,
                state_revision, detected_by, evidence_refs, reconciliation_ref, created_at
         FROM conflicts
         WHERE id = $1 AND workspace_id = $2
         FOR UPDATE",
    )
    .bind(conflict_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;

    row.map(|row| {
        Ok(Conflict {
            conflict_id,
            workspace_id: WorkspaceId::from_uuid(
                row.try_get("workspace_id").map_err(storage_error)?,
            ),
            conflict_kind: enum_from_text(row.try_get("conflict_kind").map_err(storage_error)?)?,
            participant_refs: json_column(&row, "participant_refs")?,
            status: enum_from_text(row.try_get("status").map_err(storage_error)?)?,
            state_revision: row
                .try_get::<i32, _>("state_revision")
                .map_err(storage_error)? as u32,
            detected_by: vestrace_domain::PrincipalId::from_uuid(
                row.try_get("detected_by").map_err(storage_error)?,
            ),
            evidence_refs: json_column(&row, "evidence_refs")?,
            reconciliation_ref: row.try_get("reconciliation_ref").map_err(storage_error)?,
            created_at: row.try_get("created_at").map_err(storage_error)?,
        })
    })
    .transpose()
}

fn apply_claim_mutation(
    claim: Claim,
    mutation: &CognitiveMutation,
) -> Result<Claim, ApplicationError> {
    if claim.state_revision != mutation.expected_state_revision {
        return Err(ApplicationError::Domain(DomainError::RevisionConflict {
            expected: mutation.expected_state_revision as u64,
            current: claim.state_revision as u64,
        }));
    }
    let at = mutation.created_at;
    match mutation.mutation_kind {
        MutationKind::Supersede => claim.supersede(at),
        MutationKind::Expire => claim.expire(at),
        MutationKind::Reject => claim.reject(at),
        MutationKind::Delete => claim.delete(at),
        _ => Err(DomainError::PolicyViolation(
            "mutation kind is not supported for a claim in C4".into(),
        )),
    }
    .map_err(ApplicationError::Domain)
}

fn apply_conflict_mutation(
    conflict: Conflict,
    mutation: &CognitiveMutation,
    reconciliation: Option<&ReconciliationRecord>,
) -> Result<Conflict, ApplicationError> {
    if mutation.mutation_kind != MutationKind::ResolveConflict {
        return Err(ApplicationError::Domain(DomainError::PolicyViolation(
            "only ResolveConflict mutations may change a conflict".into(),
        )));
    }
    if conflict.state_revision != mutation.expected_state_revision {
        return Err(ApplicationError::Domain(DomainError::RevisionConflict {
            expected: mutation.expected_state_revision as u64,
            current: conflict.state_revision as u64,
        }));
    }
    let Some(record) = reconciliation else {
        return Err(DomainError::InvalidArgument(
            "conflict mutation requires a reconciliation record".into(),
        )
        .into());
    };
    let conflict = conflict.propose_reconciliation(record.reconciliation_id.clone())?;
    match record.outcome {
        ReconciliationOutcome::Resolved => conflict.resolve(),
        ReconciliationOutcome::AcceptedAmbiguity => conflict.accept_ambiguity(),
        ReconciliationOutcome::HumanDeferred => Ok(conflict),
        ReconciliationOutcome::Obsolete => conflict.obsolete(),
    }
    .map_err(ApplicationError::Domain)
}

async fn persist_claim(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    claim: &Claim,
    expected_state_revision: u32,
) -> Result<(), ApplicationError> {
    let result = sqlx::query(
        "UPDATE claims
         SET lifecycle_status = $1, valid_from = $2, valid_until = $3,
             state_revision = $4, updated_at = $5
         WHERE id = $6 AND workspace_id = $7 AND state_revision = $8",
    )
    .bind(enum_text(&claim.lifecycle_status))
    .bind(claim.valid_from)
    .bind(claim.valid_until)
    .bind(claim.state_revision as i32)
    .bind(claim.updated_at)
    .bind(claim.claim_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(expected_state_revision as i32)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationError::Conflict(
            "claim changed while mutation was being applied".into(),
        ));
    }
    Ok(())
}

async fn persist_conflict(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    conflict: &Conflict,
    expected_state_revision: u32,
) -> Result<(), ApplicationError> {
    let evidence_refs = json_value(&conflict.evidence_refs)?;
    let result = sqlx::query(
        "UPDATE conflicts
         SET status = $1, state_revision = $2, reconciliation_ref = $3,
             evidence_refs = $4
         WHERE id = $5 AND workspace_id = $6 AND state_revision = $7",
    )
    .bind(enum_text(&conflict.status))
    .bind(conflict.state_revision as i32)
    .bind(&conflict.reconciliation_ref)
    .bind(evidence_refs)
    .bind(conflict.conflict_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(expected_state_revision as i32)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationError::Conflict(
            "conflict changed while mutation was being applied".into(),
        ));
    }
    Ok(())
}

async fn persist_mutation(
    connection: &mut PgConnection,
    mutation: &CognitiveMutation,
) -> Result<(), ApplicationError> {
    let provenance_refs = json_value(&mutation.provenance_refs)?;
    sqlx::query(
        "INSERT INTO cognitive_mutations
            (id, workspace_id, actor, target_kind, target_id,
             expected_state_revision, mutation_kind, reason, provenance_refs,
             resulting_revision_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(mutation.mutation_id.as_uuid())
    .bind(mutation.workspace_id.as_uuid())
    .bind(mutation.actor.as_uuid())
    .bind(enum_text(&mutation.target_kind))
    .bind(&mutation.target_id)
    .bind(mutation.expected_state_revision as i32)
    .bind(enum_text(&mutation.mutation_kind))
    .bind(&mutation.reason)
    .bind(provenance_refs)
    .bind(&mutation.resulting_revision_id)
    .bind(mutation.created_at)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn persist_reconciliation(
    connection: &mut PgConnection,
    record: &ReconciliationRecord,
) -> Result<(), ApplicationError> {
    let input_evidence_refs = json_value(&record.input_evidence_refs)?;
    let basis_refs = json_value(&record.basis_refs)?;
    sqlx::query(
        "INSERT INTO reconciliation_records
            (id, workspace_id, conflict_id, reconciliation_class, outcome,
             input_evidence_refs, basis_refs, policy_version, human_decision,
             resolved_by, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(&record.reconciliation_id)
    .bind(record.workspace_id.as_uuid())
    .bind(record.conflict_id.as_uuid())
    .bind(enum_text(&record.reconciliation_class))
    .bind(enum_text(&record.outcome))
    .bind(input_evidence_refs)
    .bind(basis_refs)
    .bind(&record.policy_version)
    .bind(&record.human_decision)
    .bind(record.resolved_by.as_uuid())
    .bind(record.created_at)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn persist_audit(
    connection: &mut PgConnection,
    event: &AuditEvent,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO audit_events
            (id, workspace_id, principal_id, action, resource_type,
             resource_id, payload, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(event.id.as_uuid())
    .bind(event.workspace_id.as_uuid())
    .bind(event.principal_id.as_uuid())
    .bind(&event.action)
    .bind(&event.resource_type)
    .bind(event.resource_id)
    .bind(&event.payload)
    .bind(event.created_at)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn persist_outbox(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    payload: &serde_json::Value,
    at: Timestamp,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO outbox (id, workspace_id, topic, payload, processed_at, created_at)
         VALUES ($1, $2, $3, $4, NULL, $5)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(workspace_id.as_uuid())
    .bind("cognitive.mutation.applied")
    .bind(payload)
    .bind(at)
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn persist_idempotency(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    key: &str,
    request_hash: &str,
    payload: &serde_json::Value,
    at: Timestamp,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO idempotency_keys
            (idempotency_key, workspace_id, request_hash, response_payload,
             status, created_at, expires_at)
         VALUES ($1, $2, $3, $4, 'completed', $5, $6)",
    )
    .bind(key)
    .bind(workspace_id.as_uuid())
    .bind(request_hash)
    .bind(payload)
    .bind(at)
    .bind(at + Duration::hours(24))
    .execute(&mut *connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

fn parse_uuid<T>(value: &str, field: &str) -> Result<T, ApplicationError>
where
    T: std::str::FromStr<Err = uuid::Error>,
{
    value.parse().map_err(|_| {
        ApplicationError::Domain(DomainError::InvalidArgument(format!(
            "{field} must be a UUID"
        )))
    })
}

fn enum_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("domain enum serialization must be infallible")
        .as_str()
        .expect("domain enum serialization must produce a string")
        .to_owned()
}

fn enum_from_text<T: serde::de::DeserializeOwned>(value: String) -> Result<T, ApplicationError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(internal_error)
}

fn json_value<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, ApplicationError> {
    serde_json::to_value(value).map_err(internal_error)
}

fn json_column<T: serde::de::DeserializeOwned>(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<T, ApplicationError> {
    let value: serde_json::Value = row.try_get(column).map_err(storage_error)?;
    serde_json::from_value(value).map_err(internal_error)
}

fn not_found(message: &str) -> ApplicationError {
    ApplicationError::Domain(DomainError::NotFound(message.into()))
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn internal_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Internal(error.to_string())
}
