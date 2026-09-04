//! Authenticating a caller and attributing what they do.
//!
//! # The two halves are deliberately separate
//!
//! [`AccessTokenAuthenticator`] resolves a presented credential and is the only
//! port that may run **outside** a workspace scope — it cannot be workspace
//! scoped, because the workspace is what the credential tells us. It is
//! therefore kept as narrow as possible: one method, taking a hash, returning
//! three identifiers.
//!
//! [`AccessTokenStore`] administers credentials and is workspace scoped like
//! every other store, because by the time a caller is minting or revoking one
//! they have already authenticated and their workspace is known.
//!
//! Splitting them means the unscoped path cannot be used to list, mint or
//! revoke anything. A single combined port would have made the exception to
//! workspace scoping much wider than it needs to be.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::identity::AccessToken;
use vestrace_domain::{AccessTokenId, PrincipalId, Timestamp, WorkspaceId};

use crate::{ApplicationError, GovernedMutationApply, RequestContext, UnitOfWork};

/// Who a presented credential turned out to be.
///
/// Carries identifiers and nothing else — not the label, not the timestamps,
/// not the hash. An authentication result that carried the credential record
/// would put it within reach of every handler downstream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedPrincipal {
    pub token_id: AccessTokenId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
}

/// Resolves a credential to the principal it belongs to.
#[async_trait]
pub trait AccessTokenAuthenticator: Send + Sync {
    /// Resolve a **hash**, never a token.
    ///
    /// Taking the hash rather than the plaintext keeps the credential out of
    /// the adapter, out of any query log, and out of the error path. Returns
    /// `None` for an unknown, expired or revoked credential — the three are not
    /// distinguished, because telling a caller which one applies tells them
    /// whether they guessed a real token.
    async fn resolve(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedPrincipal>, ApplicationError>;

    /// Record that the credential was used.
    ///
    /// Separate from `resolve` and allowed to fail quietly at the call site: a
    /// failure to update a usage timestamp must never turn a valid credential
    /// into a rejected one.
    async fn record_use(&self, token_id: AccessTokenId) -> Result<(), ApplicationError>;
}

pub type SharedAccessTokenAuthenticator = Arc<dyn AccessTokenAuthenticator>;

/// Administers the credentials of one workspace.
#[async_trait]
pub trait AccessTokenStore: Send + Sync {
    /// Persist a freshly minted credential. The plaintext never reaches here.
    async fn put(
        &self,
        context: &RequestContext,
        token: &AccessToken,
    ) -> Result<(), ApplicationError>;

    /// Persist a freshly minted credential inside a transaction the caller
    /// already owns.
    async fn put_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        token: &AccessToken,
    ) -> Result<(), ApplicationError>;

    /// Every credential in the workspace, revoked ones included.
    ///
    /// Revoked rows are returned rather than filtered: an operator reviewing
    /// access needs to see what was withdrawn and when, and a listing that
    /// silently omits them reads as though the credential never existed.
    async fn list(&self, context: &RequestContext) -> Result<Vec<AccessToken>, ApplicationError>;

    async fn find(
        &self,
        context: &RequestContext,
        id: AccessTokenId,
    ) -> Result<Option<AccessToken>, ApplicationError>;

    /// Mark a credential revoked. There is no delete.
    async fn revoke(
        &self,
        context: &RequestContext,
        id: AccessTokenId,
        at: Timestamp,
    ) -> Result<(), ApplicationError>;
}

pub type SharedAccessTokenStore = Arc<dyn AccessTokenStore>;

/// The mutation half of access-token creation. The surrounding
/// [`crate::GovernedMutation`] supplies the audit, idempotency and outbox
/// records so credential issuance cannot succeed without its audit event.
#[derive(Clone)]
pub struct AccessTokenMutation {
    store: SharedAccessTokenStore,
    token: AccessToken,
}

impl AccessTokenMutation {
    pub fn new(store: SharedAccessTokenStore, token: AccessToken) -> Self {
        Self { store, token }
    }
}

#[async_trait]
impl GovernedMutationApply for AccessTokenMutation {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        self.store.put_in(context, unit_of_work, &self.token).await
    }
}

/// Draws the entropy a new credential is built from.
///
/// A port rather than a direct call to the OS so that the application layer
/// stays testable without weakening what production uses, and so a build that
/// accidentally supplies a weak source is a wiring mistake visible in one
/// place rather than a subtle property of a function buried in a handler.
pub trait TokenEntropySource: Send + Sync {
    /// Fill `buffer` with cryptographically secure random bytes.
    fn fill(&self, buffer: &mut [u8]) -> Result<(), ApplicationError>;
}

pub type SharedTokenEntropySource = Arc<dyn TokenEntropySource>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_authentication_result_carries_identifiers_and_nothing_else() {
        // If this ever grows a field holding the credential record, every
        // handler downstream gains reach it should not have.
        let resolved = AuthenticatedPrincipal {
            token_id: AccessTokenId::new(),
            workspace_id: WorkspaceId::new(),
            principal_id: PrincipalId::new(),
        };
        let rendered = format!("{resolved:?}");
        assert!(!rendered.contains("token_hash"));
        assert!(!rendered.contains("label"));
    }
}
