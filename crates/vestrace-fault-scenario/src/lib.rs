//! The program that breaks the external-effect lifecycle on purpose.
//!
//! Its whole value is that the observation it prints is *found* rather than
//! *stated*: every field is read back from persisted state or counted by a
//! party outside the process under test. A build of this crate that constructs
//! the suite's expected answer would clear the release gate and prove nothing,
//! which is why a test forbids it by name.

pub mod adapter_stub;
pub mod settings;

pub use adapter_stub::AdapterStub;
pub use settings::ScenarioSettings;
