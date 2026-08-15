use async_trait::async_trait;
use serde_json::Value;
use sqlx::Row;
use vestrace_application::{ApplicationError, CapabilityGrantRepository, RequestContext};
use vestrace_domain::security::{CapabilityGrant, CapabilityGrantStatus};
use vestrace_domain::{CapabilityGrantId, PrincipalId};

use super::PgStore;

/// Durable capability grants.
///
/// # What the columns are for
///
/// The payload is authoritative. The columns exist so that "the active grants
/// for this subject" is a query rather than a scan-and-decode, because that is
/// the question every authorized request asks. They are checked against the
/// payload on read; a row whose status column disagrees with its payload is
/// refused rather than believed, since the two disagreeing is exactly how a
/// revoked grant would keep authorizing.
pub struct PgCapabilityGrantRepository {
    store: PgStore,
}

impl PgCapabilityGrantRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn status_name(status: CapabilityGrantStatus) -> &'static str {
    match status {
        CapabilityGrantStatus::Active => "active",
        CapabilityGrantStatus::Revoked => "revoked",
    }
}

fn decode_grant(row: &sqlx::postgres::PgRow) -> Result<CapabilityGrant, ApplicationError> {
    let payload: Value = row.try_get("payload").map_err(storage_error)?;
    let stored_status: String = row.try_get("status").map_err(storage_error)?;
    let stored_subject: uuid::Uuid = row.try_get("subject_id").map_err(storage_error)?;
    let grant: CapabilityGrant = serde_json::from_value(payload).map_err(storage_error)?;

    if status_name(grant.status) != stored_status {
        return Err(storage_error(
            "capability grant status column disagrees with its payload",
        ));
    }
    if grant.subject_id.as_uuid() != stored_subject {
        return Err(storage_error(
            "capability grant subject column disagrees with its payload",
        ));
    }
    Ok(grant)
}

/// The columns and payload of one grant, in the order both statements bind them.
macro_rules! bind_grant {
    ($query:expr, $context:expr, $grant:expr, $payload:expr) => {
        $query
            .bind($grant.id.as_uuid())
            .bind($context.workspace_id.as_uuid())
            .bind($grant.subject_id.as_uuid())
            .bind($grant.issuer_id.as_uuid())
            .bind($grant.capability.to_string())
            .bind(&$grant.operation)
            .bind(&$grant.resource_scope)
            .bind(status_name($grant.status))
            .bind(format!("{:?}", $grant.risk_ceiling).to_lowercase())
            .bind(
                $grant
                    .budget
                    .map(|budget| i64::try_from(budget.max_units))
                    .transpose()
                    .map_err(storage_error)?,
            )
            .bind($grant.valid_from)
            .bind($grant.valid_until)
            .bind($grant.created_at)
            .bind($grant.revoked_at)
            .bind($payload)
    };
}

#[async_trait]
impl CapabilityGrantRepository for PgCapabilityGrantRepository {
    async fn insert(
        &self,
        context: &RequestContext,
        grant: &CapabilityGrant,
    ) -> Result<(), ApplicationError> {
        if grant.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a capability grant cannot be issued into another workspace".into(),
            ));
        }

        let payload = serde_json::to_value(grant).map_err(storage_error)?;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let query = sqlx::query(
            "INSERT INTO capability_grants (
                 id, workspace_id, subject_id, issuer_id, capability, operation,
                 resource_scope, status, risk_ceiling, budget_max_units,
                 valid_from, valid_until, created_at, revoked_at, payload
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        );
        bind_grant!(query, context, grant, payload)
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn active_for_subject(
        &self,
        context: &RequestContext,
        subject: PrincipalId,
    ) -> Result<Vec<CapabilityGrant>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            "SELECT status, subject_id, payload
             FROM capability_grants
             WHERE workspace_id = $1 AND subject_id = $2 AND status = 'active'
             ORDER BY created_at ASC, id ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(subject.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        rows.iter().map(decode_grant).collect()
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<CapabilityGrant>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            "SELECT status, subject_id, payload
             FROM capability_grants
             WHERE workspace_id = $1
             ORDER BY created_at DESC, id DESC
             LIMIT 200",
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        rows.iter().map(decode_grant).collect()
    }

    async fn find(
        &self,
        context: &RequestContext,
        id: CapabilityGrantId,
    ) -> Result<Option<CapabilityGrant>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            "SELECT status, subject_id, payload
             FROM capability_grants
             WHERE workspace_id = $1 AND id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.as_ref().map(decode_grant).transpose()
    }

    async fn save_revocation(
        &self,
        context: &RequestContext,
        grant: &CapabilityGrant,
    ) -> Result<(), ApplicationError> {
        if grant.status != CapabilityGrantStatus::Revoked || grant.revoked_at.is_none() {
            return Err(ApplicationError::Policy(
                "save_revocation was given a grant that is not revoked".into(),
            ));
        }

        let payload = serde_json::to_value(grant).map_err(storage_error)?;
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // Only an active grant can be revoked, in the statement as well as in
        // the domain: two concurrent revocations would otherwise both succeed
        // and the second would move `revoked_at` forward.
        let result = sqlx::query(
            "UPDATE capability_grants
             SET status = 'revoked', revoked_at = $3, payload = $4
             WHERE workspace_id = $1 AND id = $2 AND status = 'active'",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(grant.id.as_uuid())
        .bind(grant.revoked_at)
        .bind(payload)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "the capability grant is not active in this workspace".into(),
            ));
        }

        scoped.commit().await.map_err(storage_error)
    }
}
