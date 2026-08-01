#![forbid(unsafe_code)]

mod health;
mod router;

pub use router::{AppState, build_router};
