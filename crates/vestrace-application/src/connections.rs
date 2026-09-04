use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    AuditEvent, ConnectionAdmissionPolicyId, ConnectionAuthMode, ConnectionId, ConnectionKind,
    ConnectionRevisionId, ConnectionTransportPolicy, CredentialSlotId, connection::Connection,
    models::ConnectionAdmissionLimits,
};

use crate::{
    ApplicationError, GovernedMutationReceipt, IdempotencyRecord, OutboxMessage, RequestContext,
};

/// A connection together with the connector it belongs to.
///
/// **No credentials.** The `connections` table stores identity and state only;
/// there is no secret column and this port deliberately exposes none. Storing
/// credentials requires envelope encryption under a key that does not live
/// beside the ciphertext, which this system does not yet have.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionListing {
    pub connection: Connection,
    pub connector_name: String,
    pub provider_type: String,
}

/// A non-routing Connection read model.  Stable legacy identities remain
/// visible only as explicitly non-executable rows; all status and
/// qualification facts come from immutable revision heads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedConnectionProjection {
    pub id: ConnectionId,
    pub revision_id: Option<ConnectionRevisionId>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
}

#[async_trait]
pub trait ConnectionRepository: Send + Sync {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ConnectionListing>, ApplicationError>;
}

pub type SharedConnectionRepository = Arc<dyn ConnectionRepository>;

/// The complete immutable revision to create or publish under the stable
/// connection identity. The `connection` projection carries the connector,
/// principal, and display identity required to create a new stable row.
#[derive(Clone, Debug)]
pub struct CreateConnectionRevision {
    pub connection: Connection,
    pub revision_id: ConnectionRevisionId,
    pub execution_guard_id: uuid::Uuid,
    pub kind: ConnectionKind,
    pub logical_base_url: String,
    pub runtime_base_url: String,
    pub adapter_profile_revision: String,
    pub transport_policy: ConnectionTransportPolicy,
    pub auth_mode: ConnectionAuthMode,
    pub credential_slot_id: Option<CredentialSlotId>,
    pub expected_head_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// One connection admission policy revision, published under compare-and-swap
/// against the policy head.
///
/// This exists because P03 shipped the policy's vocabulary and its consumer
/// without a producer: `ConnectionAdmissionPolicy` is a domain type and
/// `vestrace_try_admit_provider_dispatch` refuses a dispatch that has no
/// current revision, but nothing wrote one. Every test that needed an
/// admissible Connection inserted the rows itself as the guarded owner, which
/// no deployment can do, so provider dispatch was unreachable in production.
///
/// `expected_head_version` is stated by the caller and never looked up here.
/// Reading the current version and then writing would let two operators each
/// believe they had published the policy that governs the next dispatch.
/// Version zero means "this Connection has no policy yet" and is the only value
/// that may create the first revision.
#[derive(Clone, Debug)]
pub struct PublishConnectionAdmissionPolicy {
    pub connection_id: ConnectionId,
    pub policy_revision_id: ConnectionAdmissionPolicyId,
    pub limits: ConnectionAdmissionLimits,
    pub expected_head_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// Publishes immutable connection revisions through the governed-mutation
/// boundary. Creation also writes the stable connection identity; revision
/// changes only advance its guarded immutable-revision head.
#[async_trait]
pub trait ConnectionRevisionRepository: Send + Sync {
    async fn create_governed(
        &self,
        context: RequestContext,
        command: CreateConnectionRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;

    async fn revise_governed(
        &self,
        context: RequestContext,
        command: CreateConnectionRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError>;

    /// Publishes the admission policy that governs every dispatch on this
    /// Connection.
    ///
    /// The default refuses. An implementation that has no policy authority must
    /// say so rather than report a publication that never happened: a caller
    /// that believed this succeeded would go on to dispatch against a policy
    /// nobody wrote.
    async fn publish_admission_policy_governed(
        &self,
        _context: RequestContext,
        _command: PublishConnectionAdmissionPolicy,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "connection admission policy authority is not configured".to_owned(),
        ))
    }

    /// Lists stable Connections without returning their legacy routing or
    /// display fields.  A caller must not infer executability from the
    /// compatibility identity alone.
    async fn list_safe_connections(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<GovernedConnectionProjection>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed connection projection storage is not configured".to_owned(),
        ))
    }
}

pub type SharedConnectionRevisionRepository = Arc<dyn ConnectionRevisionRepository>;
