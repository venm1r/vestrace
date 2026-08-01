#![forbid(unsafe_code)]

pub mod config;
mod error;
mod postgres;

pub use config::{AppConfig, ConfigOverrides, DatabaseConfig, HttpConfig, ObservabilityConfig};
pub use error::{InfrastructureError, InfrastructureErrorKind};
pub use postgres::{PgScopedTransaction, PgStore, PgTransactionManager};
