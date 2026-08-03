#![forbid(unsafe_code)]

mod health;
mod router;
pub mod api;

pub use router::{AppState, build_router};
