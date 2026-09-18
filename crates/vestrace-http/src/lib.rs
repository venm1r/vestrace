#![forbid(unsafe_code)]

pub mod api;
pub mod auth;
mod health;
mod metrics;
pub mod route_inventory;
mod router;

pub use metrics::MetricsRegistry;
pub use router::{AppState, build_router, inventory_lookup_for_test};
