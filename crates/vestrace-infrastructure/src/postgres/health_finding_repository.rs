use async_trait::async_trait;
use serde_json::Value;
use sqlx::Row;
use vestrace_application::{ApplicationError, HealthFindingRepository, RequestContext};
use vestrace_domain::HealthFindingId;
use vestrace_domain::health::{HealthFinding, HealthOccurrence};

use super::PgStore;

/// Durable health findings.
///
/// # Identity
///
/// A finding is keyed by `(workspace_id, invariant_id, fingerprint)`, so the
/// upsert on that constraint is what turns the same problem seen on two runs
/// into one finding with two occurrences. The row id is preserved across
/// updates — an occurrence references it, and a new id per run would orphan
/// every occurrence written before.
///
/// # Payload and columns
///
/// The JSONB payload is authoritative; the columns beside it exist to be
/// queried and are checked against it on read. A row whose indexed columns have
/// drifted from its payload is refused rather than believed, following the
/// qualification and recovery repositories.
pub struct PgHealthFindingRepository {
    store: PgStore,
}

impl PgHealthFindingRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value, ApplicationError> {
    serde_json::to_value(value).map_err(storage_error)
}

fn decode<T: serde::de::DeserializeOwned>(payload: Value) -> Result<T, ApplicationError> {
    serde_json::from_value(payload).map_err(storage_error)
}

/// The stored status, spelled the way the domain serialises it.
fn enum_name<T: serde::Serialize>(value: T) -> Result<String, ApplicationError> {
    serde_json::to_value(value)
        .map_err(storage_error)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| storage_error("a health enum did not serialize as a string"))
}

fn check_indexed(
    finding: &HealthFinding,
    row_invariant: &str,
    row_fingerprint: &str,
) -> Result<(), ApplicationError> {
    if finding.invariant_id() != row_invariant || finding.fingerprint() != row_fingerprint {
        return Err(storage_error(
            "health finding indexed columns do not match the stored payload",
        ));
    }
    Ok(())
}

#[async_trait]
impl HealthFindingRepository for PgHealthFindingRepository {
    async fn find_by_fingerprint(
        &self,
        context: &RequestContext,
        invariant_id: &str,
        fingerprint: &str,
    ) -> Result<Option<HealthFinding>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            "SELECT invariant_id, fingerprint, payload
             FROM health_findings
             WHERE workspace_id = $1 AND invariant_id = $2 AND fingerprint = $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(invariant_id)
        .bind(fingerprint)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        row.map(|row| {
            let stored_invariant: String = row.try_get("invariant_id").map_err(storage_error)?;
            let stored_fingerprint: String = row.try_get("fingerprint").map_err(storage_error)?;
            let payload: Value = row.try_get("payload").map_err(storage_error)?;
            let finding: HealthFinding = decode(payload)?;
            check_indexed(&finding, &stored_invariant, &stored_fingerprint)?;
            Ok(finding)
        })
        .transpose()
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: HealthFindingId,
    ) -> Result<Option<HealthFinding>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row =
            sqlx::query("SELECT payload FROM health_findings WHERE id = $1 AND workspace_id = $2")
                .bind(id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .fetch_optional(scoped.connection())
                .await
                .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.map(|row| {
            let payload: Value = row.try_get("payload").map_err(storage_error)?;
            decode(payload)
        })
        .transpose()
    }

    async fn save(
        &self,
        context: &RequestContext,
        finding: &HealthFinding,
        occurrence: Option<&HealthOccurrence>,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The id is kept from whatever row already holds this identity. An
        // upsert that took `EXCLUDED.id` would give the finding a new id on
        // every run and orphan its occurrences.
        sqlx::query(
            "INSERT INTO health_findings (
                 id, workspace_id, invariant_id, invariant_version, fingerprint,
                 lifecycle_status, severity, observed_state, occurrence_count,
                 first_seen_at, last_seen_at, payload
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (workspace_id, invariant_id, fingerprint) DO UPDATE SET
                 invariant_version = EXCLUDED.invariant_version,
                 lifecycle_status = EXCLUDED.lifecycle_status,
                 severity = EXCLUDED.severity,
                 observed_state = EXCLUDED.observed_state,
                 occurrence_count = EXCLUDED.occurrence_count,
                 last_seen_at = EXCLUDED.last_seen_at,
                 payload = EXCLUDED.payload",
        )
        .bind(finding.id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(finding.invariant_id())
        .bind(finding.invariant_version())
        .bind(finding.fingerprint())
        .bind(enum_name(finding.lifecycle_status())?)
        .bind(enum_name(finding.severity())?)
        .bind(enum_name(finding.observed_state())?)
        .bind(i64::try_from(finding.occurrence_count()).map_err(storage_error)?)
        .bind(finding.first_seen_at())
        .bind(finding.last_seen_at())
        .bind(json(finding)?)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        if let Some(occurrence) = occurrence {
            // The occurrence has to reference the row that actually holds this
            // identity, which is not necessarily the id the caller's in-memory
            // finding carries: a finding opened by one run and re-observed by
            // another keeps the stored id.
            let stored_id: uuid::Uuid = sqlx::query_scalar(
                "SELECT id FROM health_findings
                 WHERE workspace_id = $1 AND invariant_id = $2 AND fingerprint = $3",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(finding.invariant_id())
            .bind(finding.fingerprint())
            .fetch_one(scoped.connection())
            .await
            .map_err(storage_error)?;

            sqlx::query(
                "INSERT INTO health_occurrences (
                     id, finding_id, workspace_id, sequence, observed_state, observed_at, payload
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (finding_id, sequence) DO NOTHING",
            )
            .bind(occurrence.id().as_uuid())
            .bind(stored_id)
            .bind(context.workspace_id.as_uuid())
            .bind(i64::try_from(occurrence.sequence()).map_err(storage_error)?)
            .bind(enum_name(occurrence.observed_state())?)
            .bind(occurrence.observed_at())
            .bind(json(occurrence)?)
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;
        }

        scoped.commit().await.map_err(storage_error)
    }

    async fn list(&self, context: &RequestContext) -> Result<Vec<HealthFinding>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            "SELECT invariant_id, fingerprint, payload
             FROM health_findings
             WHERE workspace_id = $1
             ORDER BY last_seen_at DESC, invariant_id ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let stored_invariant: String =
                    row.try_get("invariant_id").map_err(storage_error)?;
                let stored_fingerprint: String =
                    row.try_get("fingerprint").map_err(storage_error)?;
                let payload: Value = row.try_get("payload").map_err(storage_error)?;
                let finding: HealthFinding = decode(payload)?;
                check_indexed(&finding, &stored_invariant, &stored_fingerprint)?;
                Ok(finding)
            })
            .collect()
    }

    async fn occurrences(
        &self,
        context: &RequestContext,
        finding_id: HealthFindingId,
    ) -> Result<Vec<HealthOccurrence>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            "SELECT payload FROM health_occurrences
             WHERE workspace_id = $1 AND finding_id = $2
             ORDER BY sequence ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(finding_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let payload: Value = row.try_get("payload").map_err(storage_error)?;
                decode(payload)
            })
            .collect()
    }
}
