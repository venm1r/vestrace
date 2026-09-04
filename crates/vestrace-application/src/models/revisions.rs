use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    AuditEvent, ConnectionId, ModelId, ModelRevision, ModelRevisionId, ProviderId, id::WorkspaceId,
};

use crate::{
    ApplicationError, GovernedMutationReceipt, IdempotencyRecord, OutboxMessage, RequestContext,
};

use super::ModelRecord;

/// An immutable Model revision published under its stable compatibility row.
#[derive(Clone, Debug)]
pub struct CreateModelRevision {
    pub model: ModelRecord,
    pub revision: ModelRevision,
    pub connection_id: ConnectionId,
    pub execution_guard_id: uuid::Uuid,
    pub expected_head_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// A workspace's mutable default pointer, independently versioned from Model heads.
#[derive(Clone, Debug)]
pub struct SetWorkspaceModelDefault {
    pub default_id: uuid::Uuid,
    pub workspace_id: WorkspaceId,
    pub purpose: String,
    pub model_id: ModelId,
    pub required_capabilities: Vec<String>,
    pub expected_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// A non-routing read model derived only from the current immutable Model and
/// Connection revision heads plus their current qualification evidence.  It
/// deliberately contains no wire model name, URL, credential shape, or legacy
/// catalog facts that could become an alternate execution authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedModelProjection {
    pub id: ModelId,
    pub revision_id: Option<ModelRevisionId>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
}

/// A provider registry compatibility projection.  A provider appears here
/// only when at least one of its stable Models has a current qualified
/// Model/Connection tuple.  The row is descriptive, never dispatch authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedProviderProjection {
    pub id: ProviderId,
    pub state: String,
    pub blockers: Vec<String>,
}

/// Publishes immutable Model revisions and the separately versioned workspace default.
#[async_trait]
pub trait ModelRevisionRepository: Send + Sync {
    async fn create_governed(
        &self,
        context: RequestContext,
        command: CreateModelRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;

    async fn set_workspace_default_governed(
        &self,
        context: RequestContext,
        command: SetWorkspaceModelDefault,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;

    /// Lists stable Models with an explicit non-executable state when they
    /// lack a current governed tuple.  This is intentionally separate from
    /// the legacy catalog port used by historical callers.
    async fn list_safe_models(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<GovernedModelProjection>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed model projection storage is not configured".to_owned(),
        ))
    }

    /// Lists only Provider rows backed by at least one current, enabled, and
    /// unexpired qualified Model/Connection tuple.
    async fn list_safe_providers(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<GovernedProviderProjection>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed provider projection storage is not configured".to_owned(),
        ))
    }
}

pub type SharedModelRevisionRepository = Arc<dyn ModelRevisionRepository>;
