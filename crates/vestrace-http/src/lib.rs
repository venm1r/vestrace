#![forbid(unsafe_code)]

pub mod api;
pub mod auth;
mod health;
mod metrics;
mod router;

pub use metrics::MetricsRegistry;
pub use router::{AppState, build_router, http_capability_for_test};
