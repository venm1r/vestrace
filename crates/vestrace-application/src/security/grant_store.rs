//! Durable capability grants.
//!
//! # What this makes possible
//!
//! Authority in this system has been a configuration file: an operator lists
//! capability names, every principal gets all of them, and there is no subject
//! scoping, expiry or revocation. That is not a partial implementation of the
//! capability model — it is a different model wearing its vocabulary, which is
//! why `ConfiguredCapabilityPolicyEngine` reports `ConfiguredAllowance` rather
//! than `GrantMatched` and warns at startup.
//!
//! With a store, the verified domain kernel becomes reachable:
//! [`StoredGrantPolicyEngine`] loads the active grants for the requesting
//! subject and hands them to `evaluate_capability_grants`, which is the function
//! CAP-002..CAP-014 exercise.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::security::{
    AuthorizationRequest, CapabilityGrant, PolicyDecision, evaluate_capability_grants,
};
use vestrace_domain::{CapabilityGrantId, PolicyDecisionId, PrincipalId, time::now};

use crate::security::policy_engine::PolicyDecisionEngine;
use crate::{ApplicationError, RequestContext};

#[async_trait]
pub trait CapabilityGrantRepository: Send + Sync {
    /// Record a newly issued grant.
    async fn insert(
        &self,
        context: &RequestContext,
        grant: &CapabilityGrant,
    ) -> Result<(), ApplicationError>;

    /// The grants that could authorize a request from `subject`.
    ///
    /// Only active ones: a revoked grant cannot contribute to a decision, and
    /// loading it so the domain can reject it again would put revocation on the
    /// hot path of every request for no benefit. The domain still refuses a
    /// revoked grant if one reaches it — this is an optimisation of the query,
    /// not a relocation of the rule.
    async fn active_for_subject(
        &self,
        context: &RequestContext,
        subject: PrincipalId,
    ) -> Result<Vec<CapabilityGrant>, ApplicationError>;

    /// Every grant in the workspace, for administration.
    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<CapabilityGrant>, ApplicationError>;

    async fn find(
        &self,
        context: &RequestContext,
        id: CapabilityGrantId,
    ) -> Result<Option<CapabilityGrant>, ApplicationError>;

    /// Persist a revocation. The grant carries its own revoked status and time;
    /// this writes what the domain decided rather than deciding it here.
    async fn save_revocation(
        &self,
        context: &RequestContext,
        grant: &CapabilityGrant,
    ) -> Result<(), ApplicationError>;
}

pub type SharedCapabilityGrantRepository = Arc<dyn CapabilityGrantRepository>;

/// The authorization engine that consults stored grants.
pub struct StoredGrantPolicyEngine {
    repository: SharedCapabilityGrantRepository,
    policy_version: String,
}

impl StoredGrantPolicyEngine {
    pub fn new(
        repository: SharedCapabilityGrantRepository,
        policy_version: impl Into<String>,
    ) -> Result<Self, ApplicationError> {
        let policy_version = policy_version.into();
        if policy_version.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "policy version must not be empty".into(),
            ));
        }
        Ok(Self {
            repository,
            policy_version,
        })
    }
}

#[async_trait]
impl PolicyDecisionEngine for StoredGrantPolicyEngine {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        // The subject is the authenticated principal from the request context,
        // never anything the caller supplied. Authentication decides who is
        // asking; this decides what they may do.
        let grants = self
            .repository
            .active_for_subject(context, context.principal_id)
            .await?;

        Ok(evaluate_capability_grants(
            PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            &self.policy_version,
            &request,
            &grants,
            now(),
        )?)
    }
}
