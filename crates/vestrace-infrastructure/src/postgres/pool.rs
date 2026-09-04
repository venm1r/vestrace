use std::time::Duration;

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

    pub async fn migrations_are_compatible(&self) -> Result<bool, InfrastructureError> {
        let applied =
            sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&self.pool)
                .await?;
        let expected: Vec<_> = MIGRATOR.iter().collect();

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
