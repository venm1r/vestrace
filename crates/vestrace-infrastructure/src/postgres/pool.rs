use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use secrecy::ExposeSecret;
use sqlx::{Row, postgres::PgPoolOptions};
use vestrace_application::{
    ApplicationError, AuditRepository, GovernedMutation, GovernedMutationApply,
    GovernedMutationReceipt, GovernedMutationRepository, IdempotencyRepository,
    InstallationMutationPermit, OutboxRepository, PermitMode, RuntimeQualificationEvidence,
};
use vestrace_domain::time::now;

use crate::{DatabaseConfig, InfrastructureError};

use super::{
    audit_repository::PgAuditRepository, idempotency_repository::PgIdempotencyRepository,
    outbox_repository::PgOutboxRepository,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);
const P05_HISTORY_PREFIX_VERSION: i64 = 208;
const P05_SAFETY_ASSERTION_MIGRATION_VERSION: i64 = 209;
const P05_ARCHIVE_ASSERTION_MIGRATION_VERSION: i64 = 210;
const P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION: i64 = 211;
const P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION: i64 = 212;
const P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION: i64 = 213;
const P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION: i64 = 214;
const P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION: i64 = 215;
const P05_MIGRATION_ADVISORY_LOCK: i64 = 0x5030_3509;

#[derive(Clone, Debug)]
pub struct PgStore {
    pub(crate) pool: sqlx::PgPool,
}

/// The PostgreSQL implementation of the one governed-mutation transaction.
/// It deliberately owns no business mutation: the typed `apply` value does
/// that work through a transaction-bound application port.
pub struct PgGovernedMutationRepository {
    audit: PgAuditRepository,
    idempotency: PgIdempotencyRepository,
    outbox: PgOutboxRepository,
    permit: super::installation_permit::PgInstallationMutationPermit,
}

impl PgGovernedMutationRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            audit: PgAuditRepository::new(store.clone()),
            idempotency: PgIdempotencyRepository::new(store.clone()),
            outbox: PgOutboxRepository::new(store.clone()),
            permit: super::installation_permit::PgInstallationMutationPermit::new(store),
        }
    }

    async fn commit_on<T>(
        &self,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        mutation: GovernedMutation<T>,
    ) -> Result<GovernedMutationReceipt, ApplicationError>
    where
        T: GovernedMutationApply + 'static,
    {
        let GovernedMutation {
            context,
            audit,
            idempotency,
            outbox,
            apply,
        } = mutation;
        let audit_event_id = audit.id;
        let idempotency_key = idempotency
            .as_ref()
            .map(|record| record.idempotency_key.clone());
        let outbox_message_ids = outbox.iter().map(|message| message.id).collect();

        // A reused key carrying a different request is refused before anything
        // is applied. `save_in` writes the key with `ON CONFLICT DO NOTHING`,
        // which silently discards the second request's hash: without this
        // check, one key could smuggle a different mutation through and the
        // only thing standing in its way would be the mutation's own guard.
        //
        // The mutual exclusion is an advisory transaction lock rather than
        // `SELECT ... FOR UPDATE`. A locking read needs `UPDATE` on
        // `idempotency_keys`, which the restricted runtime role does not have
        // and must not be granted; taking that privilege back is exactly the
        // trap Task 10C already walked into on `model_request_evidence`.
        if let Some(record) = idempotency.as_ref() {
            let transaction = unit_of_work
                .as_any_mut()
                .downcast_mut::<super::transaction::PgScopedTransaction>()
                .ok_or_else(|| {
                    ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
                })?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(format!(
                    "governed-mutation-idempotency:{}:{}",
                    context.workspace_id, record.idempotency_key
                ))
                .execute(transaction.connection())
                .await
                .map_err(application_storage_error)?;
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT request_hash FROM idempotency_keys                  WHERE workspace_id = $1 AND idempotency_key = $2",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(&record.idempotency_key)
            .fetch_optional(transaction.connection())
            .await
            .map_err(application_storage_error)?;
            if existing.is_some_and(|hash| hash != record.request_hash) {
                return Err(ApplicationError::Conflict(
                    "IDEMPOTENCY_KEY_REUSE_CONFLICT".to_owned(),
                ));
            }
        }

        apply.apply(&context, unit_of_work).await?;
        self.audit.record_in(&context, unit_of_work, &audit).await?;

        if let Some(record) = idempotency.as_ref() {
            self.idempotency
                .save_in(&context, unit_of_work, record)
                .await?;
        }

        for message in &outbox {
            self.outbox.save_in(&context, unit_of_work, message).await?;
        }

        let transaction = unit_of_work
            .as_any_mut()
            .downcast_mut::<super::transaction::PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
            })?;
        sqlx::query(
            "SELECT vestrace_record_governed_mutation_audit_mark_and_advance($1, $2, $3, $4)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(context.workspace_id.as_uuid())
        .bind(audit_event_id.as_uuid())
        .bind(now())
        .execute(transaction.connection())
        .await
        .map_err(application_storage_error)?;

        Ok(GovernedMutationReceipt {
            audit_event_id,
            idempotency_key,
            outbox_message_ids,
        })
    }
}

fn application_storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait::async_trait]
impl<T> GovernedMutationRepository<T> for PgGovernedMutationRepository
where
    T: GovernedMutationApply + 'static,
{
    async fn commit(
        &self,
        mutation: GovernedMutation<T>,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        // Task 3 governed writes all take the shared installation permit. The
        // exclusive permit is deliberately left for P05's restore path.
        let context = mutation.context.clone();
        let mut permit = self.permit.acquire(PermitMode::Shared, &context).await?;
        let receipt = self.commit_on(permit.unit_of_work_mut(), mutation).await?;
        permit.commit().await?;
        Ok(receipt)
    }

    async fn commit_in(
        &self,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        mutation: GovernedMutation<T>,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        self.commit_on(unit_of_work, mutation).await
    }
}

impl PgStore {
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self, InfrastructureError> {
        if config.max_connections == 0 {
            return Err(InfrastructureError::configuration(
                "database.max_connections must be greater than zero".to_owned(),
            ));
        }

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(config.url.expose_secret())
            .await?;

        Ok(Self::from_pool(pool))
    }

    pub fn from_pool(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> Result<(), InfrastructureError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// Applies only the embedded migration prefix through `version`.
    pub async fn migrate_through_version(&self, version: i64) -> Result<(), InfrastructureError> {
        let migrator = bounded_migrator(version)?;
        migrator.run(&self.pool).await?;
        Ok(())
    }

    /// Applies a fixed P05 assertion migration only after its predecessor is exact.
    pub async fn migrate_only_version_after(
        &self,
        version: i64,
        required_prefix: i64,
    ) -> Result<(), InfrastructureError> {
        let migration = p05_assertion_migration(version, required_prefix)?;
        if migration.no_tx {
            return Err(InfrastructureError::configuration(
                "the P05 assertion migration must be transactional",
            ));
        }

        // SQLx's `Migrator::run` always calls `CREATE TABLE IF NOT EXISTS
        // _sqlx_migrations`. Phase three intentionally runs after the runtime
        // role lost CREATE on public, so execute the one fixed embedded
        // assertion migration and maintain the existing SQLx ledger ourselves.
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(P05_MIGRATION_ADVISORY_LOCK)
            .execute(&mut *transaction)
            .await?;

        // A completed, checksum-matching 0209 is an idempotent retry. Any
        // unexpected, dirty, stale, or later row fails both exact checks.
        if migration_ledger_matches_through(&mut *transaction, version).await? {
            transaction.commit().await?;
            return Ok(());
        }
        if !migration_ledger_matches_through(&mut *transaction, required_prefix).await? {
            return Err(InfrastructureError::configuration(
                "migration ledger is not the required successful historical prefix",
            ));
        }

        let started = Instant::now();
        sqlx::raw_sql(&migration.sql)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES ($1, $2, TRUE, $3, -1)",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref())
        .execute(&mut *transaction)
        .await?;
        if !migration_ledger_matches_through(&mut *transaction, version).await? {
            return Err(InfrastructureError::configuration(
                "migration ledger is incompatible after the P05 assertion migration",
            ));
        }
        transaction.commit().await?;

        #[allow(clippy::cast_possible_truncation)]
        sqlx::query("UPDATE _sqlx_migrations SET execution_time = $1 WHERE version = $2")
            .bind(started.elapsed().as_nanos() as i64)
            .bind(migration.version)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn migrations_are_compatible(&self) -> Result<bool, InfrastructureError> {
        self.migrations_are_compatible_through(i64::MAX).await
    }

    /// Returns whether the successful migration ledger exactly matches the
    /// embedded prefix through `through`, with no later migration present.
    ///
    /// This is intentionally stricter than merely checking that the prefix is
    /// present: P05 phase one must reject a stale, partial, or out-of-order
    /// ledger before the guarded bootstrap transition.
    pub async fn migrations_are_compatible_through(
        &self,
        through: i64,
    ) -> Result<bool, InfrastructureError> {
        migration_ledger_matches_through(&self.pool, through).await
    }

    pub async fn deployment_qualification_evidence(
        &self,
    ) -> Result<RuntimeQualificationEvidence, InfrastructureError> {
        let role = sqlx::query(
            "SELECT current_user::text AS runtime_role,
                    runtime_role_record.rolsuper AS is_superuser,
                    runtime_role_record.rolbypassrls AS bypasses_rls,
                    COALESCE(
                        pg_has_role(current_user, bootstrap_role.rolname, 'MEMBER'),
                        false
                    ) AS inherits_bootstrap
             FROM pg_roles AS runtime_role_record
             LEFT JOIN pg_roles AS bootstrap_role
               ON bootstrap_role.rolname = 'vestrace_bootstrap'
             WHERE runtime_role_record.rolname = current_user",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(RuntimeQualificationEvidence {
            migration_history_compatible: self.migrations_are_compatible().await?,
            runtime_role: role.try_get("runtime_role")?,
            is_superuser: role.try_get("is_superuser")?,
            bypasses_rls: role.try_get("bypasses_rls")?,
            inherits_bootstrap: role.try_get("inherits_bootstrap")?,
        })
    }
}

fn p05_assertion_migration(
    version: i64,
    required_prefix: i64,
) -> Result<&'static sqlx::migrate::Migration, InfrastructureError> {
    let permitted = matches!(
        (version, required_prefix),
        (
            P05_SAFETY_ASSERTION_MIGRATION_VERSION,
            P05_HISTORY_PREFIX_VERSION
        ) | (
            P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
            P05_SAFETY_ASSERTION_MIGRATION_VERSION
        ) | (
            P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
            P05_ARCHIVE_ASSERTION_MIGRATION_VERSION
        ) | (
            P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
            P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION
        ) | (
            P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION
        ) | (
            P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION
        ) | (
            P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION
        )
    );
    if !permitted {
        return Err(InfrastructureError::configuration(
            "the P05 migration path only permits its next assertion after the exact predecessor",
        ));
    }
    MIGRATOR
        .iter()
        .find(|migration| migration.version == version)
        .ok_or_else(|| {
            InfrastructureError::configuration(
                "the required P05 assertion migration is not embedded",
            )
        })
}

async fn migration_ledger_matches_through<'e, E>(
    executor: E,
    through: i64,
) -> Result<bool, InfrastructureError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let applied =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(executor)
            .await?;
    let expected: Vec<_> = MIGRATOR
        .iter()
        .filter(|migration| migration.version <= through)
        .collect();

    if applied.len() != expected.len() {
        return Ok(false);
    }

    for (row, migration) in applied.iter().zip(expected) {
        let version: i64 = row.try_get("version")?;
        let success: bool = row.try_get("success")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;

        if version != migration.version
            || !success
            || checksum.as_slice() != migration.checksum.as_ref()
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn bounded_migrator(version: i64) -> Result<sqlx::migrate::Migrator, InfrastructureError> {
    if !MIGRATOR.version_exists(version) {
        return Err(InfrastructureError::configuration(
            "requested bounded migration version is not embedded",
        ));
    }
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    })
}

#[async_trait::async_trait]
impl vestrace_application::RuntimeEvidenceProvider for PgStore {
    async fn runtime_evidence(
        &self,
    ) -> Result<
        vestrace_application::RuntimeQualificationEvidence,
        vestrace_application::ApplicationError,
    > {
        self.deployment_qualification_evidence()
            .await
            .map_err(|error| vestrace_application::ApplicationError::Storage(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        P05_ARCHIVE_ASSERTION_MIGRATION_VERSION, P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
        P05_HISTORY_PREFIX_VERSION, P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
        P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
        P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
        P05_SAFETY_ASSERTION_MIGRATION_VERSION, P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
        p05_assertion_migration,
    };

    #[test]
    fn p05_only_paths_are_pinned_to_their_transactional_predecessors() {
        let migration = p05_assertion_migration(
            P05_SAFETY_ASSERTION_MIGRATION_VERSION,
            P05_HISTORY_PREFIX_VERSION,
        )
        .expect("0209 after 0208 must be embedded");

        assert_eq!(migration.version, 209);
        assert!(!migration.no_tx, "0209 must commit with its ledger row");
        assert!(
            !migration.sql.to_ascii_uppercase().contains("CREATE TABLE"),
            "0209 must only assert the provisioned safety catalog"
        );
        let archive = p05_assertion_migration(
            P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
            P05_SAFETY_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0210 after 0209 must be embedded");
        assert_eq!(archive.version, 210);
        assert!(!archive.no_tx, "0210 must commit with its ledger row");
        let base_capture = p05_assertion_migration(
            P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
            P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0211 after 0210 must be embedded");
        assert_eq!(base_capture.version, 211);
        assert!(!base_capture.no_tx, "0211 must commit with its ledger row");
        let restore = p05_assertion_migration(
            P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
            P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0212 after 0211 must be embedded");
        assert_eq!(restore.version, 212);
        let refusal = p05_assertion_migration(
            P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0213 after 0212 must be embedded");
        assert_eq!(refusal.version, 213);
        let restore_events = p05_assertion_migration(
            P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0214 after 0213 must be embedded");
        assert_eq!(restore_events.version, 214);
        let readiness = p05_assertion_migration(
            P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
            P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
        )
        .expect("0215 after 0214 must be embedded");
        assert_eq!(readiness.version, 215);
        assert!(!readiness.no_tx, "0215 must commit with its ledger row");
        assert!(
            !readiness.sql.to_ascii_uppercase().contains("CREATE TABLE"),
            "0215 must only assert the provisioned readiness read surface"
        );
        assert!(p05_assertion_migration(208, 208).is_err());
        assert!(p05_assertion_migration(210, 208).is_err());
        assert!(p05_assertion_migration(211, 209).is_err());
        assert!(p05_assertion_migration(209, 207).is_err());
        // 0215 is pinned to 0214 alone; skipping the restore-event mirror would
        // install a read surface over a catalog that never gained its guard.
        assert!(p05_assertion_migration(215, 213).is_err());
        assert!(p05_assertion_migration(215, 208).is_err());
    }
}
