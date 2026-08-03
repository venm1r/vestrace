#![forbid(unsafe_code)]

pub mod api;
mod health;
mod router;

pub use router::{AppState, build_router};
