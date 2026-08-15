//! Operator-changeable workspace settings.
//!
//! This module holds only what the runtime genuinely consults and an operator
//! can genuinely change. Environment facts — whether row-level security is
//! enforced, the database pool size, the process log filter — are deliberately
//! absent: they are observed and reported elsewhere, because storing them here
//! would present an operator with controls that change nothing.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{DomainError, WorkspaceId};

/// Upper bound on concurrency, so a typo cannot ask the runtime for unbounded
/// parallelism.
const MAX_CONCURRENT_RUNS_CEILING: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LogLevel {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            other => Err(DomainError::InvalidArgument(format!(
                "unknown log level {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceSettings {
    workspace_id: WorkspaceId,
    max_concurrent_runs: u32,
    /// Budget ceiling per run in micro-units of currency. Zero means no cap.
    run_budget_cap_micros: u64,
    log_level: LogLevel,
    /// Optimistic-concurrency token. Every accepted change produces a new one.
    version: u64,
}

impl WorkspaceSettings {
    pub fn new(
        workspace_id: WorkspaceId,
        max_concurrent_runs: u32,
        run_budget_cap_micros: u64,
        log_level: LogLevel,
        version: u64,
    ) -> Result<Self, DomainError> {
        if max_concurrent_runs == 0 || max_concurrent_runs > MAX_CONCURRENT_RUNS_CEILING {
            return Err(DomainError::InvalidArgument(format!(
                "max_concurrent_runs must be between 1 and {MAX_CONCURRENT_RUNS_CEILING}"
            )));
        }
        if version == 0 {
            return Err(DomainError::InvalidArgument(
                "settings version must be positive".to_owned(),
            ));
        }
        Ok(Self {
            workspace_id,
            max_concurrent_runs,
            run_budget_cap_micros,
            log_level,
            version,
        })
    }

    /// Conservative starting point for a workspace that has never been
    /// configured: one run at a time and no budget cap expressed.
    pub fn defaults(workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id,
            max_concurrent_runs: 1,
            run_budget_cap_micros: 0,
            log_level: LogLevel::Info,
            version: 1,
        }
    }

    /// Produce the next revision. Validation runs before anything is replaced,
    /// so a rejected change cannot leave a half-applied value behind.
    pub fn apply(
        self,
        max_concurrent_runs: u32,
        run_budget_cap_micros: u64,
        log_level: LogLevel,
    ) -> Result<Self, DomainError> {
        Self::new(
            self.workspace_id,
            max_concurrent_runs,
            run_budget_cap_micros,
            log_level,
            self.version.saturating_add(1),
        )
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn max_concurrent_runs(&self) -> u32 {
        self.max_concurrent_runs
    }

    pub fn run_budget_cap_micros(&self) -> u64 {
        self.run_budget_cap_micros
    }

    pub fn budget_is_unlimited(&self) -> bool {
        self.run_budget_cap_micros == 0
    }

    pub fn log_level(&self) -> LogLevel {
        self.log_level
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}
