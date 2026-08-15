use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, RequestContext, WorkspaceSettingsRepository};
use vestrace_domain::{LogLevel, WorkspaceSettings};

use super::PgStore;

/// Workspace-scoped settings storage.
///
/// Every statement runs inside the caller's scoped transaction, so row-level
/// security applies exactly as it does to the rest of the system.
#[derive(Clone, Debug)]
pub struct PgWorkspaceSettingsRepository {
    store: PgStore,
}

impl PgWorkspaceSettingsRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl WorkspaceSettingsRepository for PgWorkspaceSettingsRepository {
    async fn load(&self, context: &RequestContext) -> Result<WorkspaceSettings, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT max_concurrent_runs, run_budget_cap_micros, log_level, version
             FROM workspace_settings
             WHERE workspace_id = $1",
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        let Some(row) = row else {
            // A workspace that was never configured reads as the conservative
            // defaults rather than as an error, so callers need no special case.
            return Ok(WorkspaceSettings::defaults(context.workspace_id));
        };

        let max_concurrent_runs: i32 = row.try_get("max_concurrent_runs").map_err(storage_error)?;
        let run_budget_cap_micros: i64 = row
            .try_get("run_budget_cap_micros")
            .map_err(storage_error)?;
        let log_level: String = row.try_get("log_level").map_err(storage_error)?;
        let version: i64 = row.try_get("version").map_err(storage_error)?;

        let max_concurrent_runs = u32::try_from(max_concurrent_runs).map_err(storage_error)?;
        let run_budget_cap_micros = u64::try_from(run_budget_cap_micros).map_err(storage_error)?;
        let version = u64::try_from(version).map_err(storage_error)?;
        let log_level = log_level
            .parse::<LogLevel>()
            .map_err(ApplicationError::Domain)?;

        WorkspaceSettings::new(
            context.workspace_id,
            max_concurrent_runs,
            run_budget_cap_micros,
            log_level,
            version,
        )
        .map_err(ApplicationError::Domain)
    }

    async fn save(
        &self,
        context: &RequestContext,
        settings: &WorkspaceSettings,
        expected_version: u64,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // One statement decides the outcome: insert only when the caller
        // expected the unconfigured default revision, update only when the
        // stored revision still matches. Anything else affects no rows and is
        // reported as a conflict.
        let result = sqlx::query(
            "INSERT INTO workspace_settings (
                 workspace_id, max_concurrent_runs, run_budget_cap_micros, log_level, version
             ) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (workspace_id) DO UPDATE
             SET max_concurrent_runs = EXCLUDED.max_concurrent_runs,
                 run_budget_cap_micros = EXCLUDED.run_budget_cap_micros,
                 log_level = EXCLUDED.log_level,
                 version = EXCLUDED.version,
                 updated_at = NOW()
             WHERE workspace_settings.version = $6",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i32::try_from(settings.max_concurrent_runs()).map_err(storage_error)?)
        .bind(i64::try_from(settings.run_budget_cap_micros()).map_err(storage_error)?)
        .bind(settings.log_level().as_str())
        .bind(i64::try_from(settings.version()).map_err(storage_error)?)
        .bind(i64::try_from(expected_version).map_err(storage_error)?)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "workspace settings were modified concurrently".to_owned(),
            ));
        }
        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }
}
