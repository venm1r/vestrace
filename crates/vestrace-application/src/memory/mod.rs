mod commands;
mod extraction;
mod ports;
mod purge;
mod services;
mod shared_read;

use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision,
    id::{MemoryId, MemoryRevisionId},
};

pub use commands::*;
pub use extraction::*;
pub use ports::*;
pub use purge::*;
pub use services::*;
pub use shared_read::*;

pub type SharedMemoryUseCases = std::sync::Arc<dyn MemoryUseCases>;

#[async_trait]
pub trait MemoryUseCases: Send + Sync {
    async fn record_event(
        &self,
        ctx: &RequestContext,
        cmd: RecordEventCommand,
    ) -> Result<Event, ApplicationError>;

    async fn remember_memory(
        &self,
        ctx: &RequestContext,
        cmd: RememberMemoryCommand,
    ) -> Result<Memory, ApplicationError>;

    async fn revise_memory(
        &self,
        ctx: &RequestContext,
        cmd: ReviseMemoryCommand,
    ) -> Result<Memory, ApplicationError>;

    async fn link_knowledge(
        &self,
        ctx: &RequestContext,
        cmd: LinkKnowledgeCommand,
    ) -> Result<KnowledgeRelation, ApplicationError>;

    async fn find_memory(
        &self,
        ctx: &RequestContext,
        id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError>;

    async fn find_revision(
        &self,
        ctx: &RequestContext,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError>;
}
