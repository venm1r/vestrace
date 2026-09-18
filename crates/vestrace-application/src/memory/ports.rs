//! Ports for the memory graph.
//!
//! # Why every method takes a request context
//!
//! They did not. Each entity carries its own `workspace_id`, so an adapter
//! could have scoped itself from the value it was handed — and that is worth
//! naming as the wrong answer rather than leaving it implicit. Row level
//! security enforces the scope it is given, faithfully; it cannot know the
//! scope was wrong. Setting the scope from the entity would make every policy
//! pass by construction and check nothing at all.
//!
//! The context is the workspace authentication resolved, which the caller
//! cannot assert. Taking both lets the adapter refuse an entity that claims to
//! belong somewhere else — the only version of this check that means anything.

use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource,
    id::{EventId, MemoryId, MemoryRevisionId},
};

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save(&self, context: &RequestContext, event: &Event) -> Result<(), ApplicationError>;
    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: EventId,
    ) -> Result<Option<Event>, ApplicationError>;
}

#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Set a memory's active revision, with the source that justifies it.
    ///
    /// Used for both the first revision and every later one, because they are
    /// the same operation: the memory now points at this revision, and this
    /// evidence is why.
    ///
    /// # Why one method and not three calls
    ///
    /// These writes are one fact. The schema says so twice —
    /// `tr_active_memory_has_source` is a deferred constraint trigger, and
    /// `fk_memories_active_revision` is a deferred foreign key. Both defer
    /// precisely so that the order *within a transaction* does not matter.
    /// Written separately, the memory commits on its own and both deferred
    /// checks run against a database that has neither the revision nor the
    /// source yet, so the write is refused. That is why creating or revising a
    /// memory through this path had never once succeeded.
    ///
    /// A method rather than a transaction handle threaded through three calls:
    /// the port stays free of storage types, and atomicity becomes a property
    /// of the operation instead of something each caller has to remember.
    ///
    /// The search index is written here too. It is a projection of the active
    /// revision, so it has to move in the same step — an index updated
    /// afterwards is an index that is wrong whenever the step after fails.
    async fn save_memory_with_revision(
        &self,
        context: &RequestContext,
        memory: &Memory,
        revision: &MemoryRevision,
        source: &MemorySource,
    ) -> Result<(), ApplicationError>;

    async fn save_memory(
        &self,
        context: &RequestContext,
        memory: &Memory,
    ) -> Result<(), ApplicationError>;
    async fn save_revision(
        &self,
        context: &RequestContext,
        revision: &MemoryRevision,
    ) -> Result<(), ApplicationError>;
    async fn find_memory_by_id(
        &self,
        context: &RequestContext,
        id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError>;
    async fn find_revision_by_id(
        &self,
        context: &RequestContext,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError>;
}

#[async_trait]
pub trait ProvenanceRepository: Send + Sync {
    async fn save_source(
        &self,
        context: &RequestContext,
        source: &MemorySource,
    ) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait RelationRepository: Send + Sync {
    async fn save_relation(
        &self,
        context: &RequestContext,
        relation: &KnowledgeRelation,
    ) -> Result<(), ApplicationError>;
}
