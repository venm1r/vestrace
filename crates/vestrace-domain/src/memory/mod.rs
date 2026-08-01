pub mod kind;
pub mod revision;
pub mod structured;

pub use kind::{Confidence, Importance, MemoryKind, MemoryStatus};
pub use revision::{Memory, MemoryRevision};
pub use structured::StructuredMemory;
