use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{LogLevel, WorkspaceSettings};

use crate::{ApplicationError, RequestContext};

/// Durable storage for operator-changeable workspace settings.
///
/// `load` returns conservative defaults for a workspace that has never been
/// configured, so a caller never has to distinguish "absent" from "unset".
#[async_trait]
pub trait WorkspaceSettingsRepository: Send + Sync {
    async fn load(&self, context: &RequestContext) -> Result<WorkspaceSettings, ApplicationError>;

    /// Persist the next revision. `expected_version` is the revision the caller
    /// read; a mismatch is a [`ApplicationError::Conflict`] and nothing is
    /// written.
    async fn save(
        &self,
        context: &RequestContext,
        settings: &WorkspaceSettings,
        expected_version: u64,
    ) -> Result<(), ApplicationError>;
}

pub type SharedWorkspaceSettingsRepository = Arc<dyn WorkspaceSettingsRepository>;

pub struct WorkspaceSettingsService {
    repository: SharedWorkspaceSettingsRepository,
}

impl WorkspaceSettingsService {
    pub fn new(repository: SharedWorkspaceSettingsRepository) -> Self {
        Self { repository }
    }

    pub async fn get(
        &self,
        context: &RequestContext,
    ) -> Result<WorkspaceSettings, ApplicationError> {
        self.repository.load(context).await
    }

    /// Apply a change against the revision the caller observed.
    ///
    /// The stored revision is re-read and compared before anything is written,
    /// so a concurrent change is reported rather than silently overwritten.
    pub async fn update(
        &self,
        context: &RequestContext,
        expected_version: u64,
        max_concurrent_runs: u32,
        run_budget_cap_micros: u64,
        log_level: LogLevel,
    ) -> Result<WorkspaceSettings, ApplicationError> {
        let current = self.repository.load(context).await?;
        if current.version() != expected_version {
            return Err(ApplicationError::Conflict(format!(
                "settings were modified concurrently: expected version {expected_version}, found {}",
                current.version()
            )));
        }
        let updated = current
            .apply(max_concurrent_runs, run_budget_cap_micros, log_level)
            .map_err(ApplicationError::Domain)?;
        self.repository
            .save(context, &updated, expected_version)
            .await?;
        Ok(updated)
    }
}
