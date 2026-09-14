//! Host-only P05-C custody for a fresh PostgreSQL restore target.

mod file_target;

pub use file_target::{RestoreTargetCustody, RestoreTargetError, RestoreTool};
