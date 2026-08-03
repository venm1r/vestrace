#![forbid(unsafe_code)]

pub mod a2a;
pub mod ag_ui;
pub mod artifact;
pub mod budget;
pub mod cognitive;
pub mod connection;
pub mod conversation;
pub mod enterprise;
pub mod error;
pub mod event;
pub mod id;
pub mod job;
pub mod memory;
pub mod models;
pub mod observability;
pub mod package;
pub mod planning;
pub mod policy;
pub mod product;
pub mod provenance;
pub mod relation;
pub mod release;
pub mod retrieval;
pub mod run;
pub mod security;
pub mod state_engine;
pub mod time;
pub mod tool;
pub mod webhook;

pub use error::DomainError;
pub use event::{ActorRef, Event, SubjectRef};
pub use id::*;
pub use job::{Job, JobState};
pub use memory::*;
pub use policy::{ActivationDecision, MemoryWritePolicy};
pub use provenance::{Derivation, DerivationMethod, EvidenceRole, MemorySource};
pub use relation::{KnowledgeRelation, RelationType};
pub use retrieval::{ContextPack, RetrievalCandidate, RetrievalIntent};
pub use time::{Timestamp, now};

pub fn crate_name() -> &'static str {
    "vestrace-domain"
}

#[cfg(test)]
mod tests {
    use super::WorkspaceId;

    #[test]
    fn crate_is_linkable() {
        assert_eq!(super::crate_name(), "vestrace-domain");
    }

    #[test]
    fn workspace_id_round_trips_as_string() {
        let id = WorkspaceId::new();

        assert_eq!(id.to_string().parse::<WorkspaceId>().unwrap(), id);
    }
}
