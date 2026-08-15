use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use vestrace_domain::{
    AuthorizationRequest, Capability, CapabilityGrant, HierarchicalBudget, PolicyDecision,
    PolicyDecisionId, PolicyDecisionReason, PolicyDecisionResult, ResourceScope, RiskCategory,
    WorkspaceId, evaluate_capability_grants, now,
};

#[async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait PolicyDecisionEngine: Send + Sync {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError>;
}

pub type SharedPolicyDecisionEngine = Arc<dyn PolicyDecisionEngine>;

#[derive(Clone)]
pub struct AuthorizationBoundary {
    engine: SharedPolicyDecisionEngine,
}

impl AuthorizationBoundary {
    pub fn new(engine: SharedPolicyDecisionEngine) -> Self {
        Self { engine }
    }

    pub async fn evaluate(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        self.engine.decide(context, request).await
    }

    pub async fn require(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        let decision = self.evaluate(context, request).await?;
        if decision.is_allowed() {
            Ok(decision)
        } else {
            Err(ApplicationError::Policy(format!(
                "authorization denied: {:?}",
                decision.reason
            )))
        }
    }
}

pub struct GrantPolicyEngine {
    policy_version: String,
    grants: Vec<CapabilityGrant>,
}

impl GrantPolicyEngine {
    pub fn new(
        policy_version: impl Into<String>,
        grants: impl IntoIterator<Item = CapabilityGrant>,
    ) -> Result<Self, ApplicationError> {
        let policy_version = policy_version.into();
        if policy_version.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "policy version must not be empty".into(),
            ));
        }
        Ok(Self {
            policy_version,
            grants: grants.into_iter().collect(),
        })
    }
}

#[async_trait]
impl PolicyDecisionEngine for GrantPolicyEngine {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        Ok(evaluate_capability_grants(
            PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            &self.policy_version,
            &request,
            &self.grants,
            now(),
        )?)
    }
}

#[async_trait]
impl PolicyEngine for GrantPolicyEngine {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        let request = AuthorizationRequest::new(
            capability.clone(),
            capability.to_string(),
            // `"unscoped"` until the scope vocabulary was made explicit: a bare
            // word in no vocabulary, so the only grant that could authorise a
            // capability check here was one whose scope was the literal string
            // `unscoped`. `workspace://` is what this is actually asking about
            // — the workspace, not any resource in it — and it is contained by
            // a grant an operator can write.
            ResourceScope::workspace().to_string(),
            RiskCategory::Low,
        );
        let decision = self.decide(context, request).await?;
        if decision.is_allowed() {
            Ok(())
        } else {
            Err(ApplicationError::Policy(format!(
                "capability denied: {:?}",
                decision.reason
            )))
        }
    }
}

pub type SharedHierarchicalBudget = Arc<Mutex<HierarchicalBudget>>;

pub struct BudgetPolicyEngine {
    inner: GrantPolicyEngine,
    budget: SharedHierarchicalBudget,
}

impl BudgetPolicyEngine {
    pub fn new(
        policy_version: impl Into<String>,
        grants: impl IntoIterator<Item = CapabilityGrant>,
        budget: SharedHierarchicalBudget,
    ) -> Result<Self, ApplicationError> {
        Ok(Self {
            inner: GrantPolicyEngine::new(policy_version, grants)?,
            budget,
        })
    }

    pub fn budget(&self) -> &SharedHierarchicalBudget {
        &self.budget
    }
}

#[async_trait]
impl PolicyDecisionEngine for BudgetPolicyEngine {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        let requested_budget_units = request.requested_budget_units;
        let mut decision = self.inner.decide(context, request).await?;
        if decision.is_allowed() && requested_budget_units > 0 {
            let charged = decision
                .matched_grant_id
                .and_then(|id| {
                    self.budget
                        .lock()
                        .ok()
                        .map(|mut budget| budget.charge(id, requested_budget_units).is_ok())
                })
                .unwrap_or(false);
            if !charged {
                decision.result = PolicyDecisionResult::Deny;
                decision.reason = PolicyDecisionReason::BudgetExceeded;
                decision.matched_grant_id = None;
            }
        }
        Ok(decision)
    }
}

#[async_trait]
impl PolicyEngine for BudgetPolicyEngine {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        let request = AuthorizationRequest::new(
            capability.clone(),
            capability.to_string(),
            // `"unscoped"` until the scope vocabulary was made explicit: a bare
            // word in no vocabulary, so the only grant that could authorise a
            // capability check here was one whose scope was the literal string
            // `unscoped`. `workspace://` is what this is actually asking about
            // — the workspace, not any resource in it — and it is contained by
            // a grant an operator can write.
            ResourceScope::workspace().to_string(),
            RiskCategory::Low,
        );
        let decision = self.decide(context, request).await?;
        if decision.is_allowed() {
            Ok(())
        } else {
            Err(ApplicationError::Policy(format!(
                "capability denied: {:?}",
                decision.reason
            )))
        }
    }
}

pub struct DenyAllPolicyEngine;

#[async_trait]
impl PolicyDecisionEngine for DenyAllPolicyEngine {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        Ok(evaluate_capability_grants(
            PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            "deny-all",
            &request,
            &[],
            now(),
        )?)
    }
}

#[async_trait]
impl PolicyEngine for DenyAllPolicyEngine {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        _capability: Capability,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Policy(
            "default policy denies capability".into(),
        ))
    }
}

#[cfg(test)]
pub struct AllowAllPolicyEngine;

#[cfg(test)]
#[async_trait]
impl PolicyEngine for AllowAllPolicyEngine {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        _capability: Capability,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

/// Authorizes a fixed set of capabilities declared in deployment configuration.
///
/// This is a stopgap for deployments that have no capability-grant store yet:
/// without it every governed route is denied and no console can show anything.
/// It is intentionally weaker than a grant and says so in its decisions —
/// [`PolicyDecisionReason::ConfiguredAllowance`], never `GrantMatched`.
///
/// What it does **not** provide: per-subject scoping, validity windows,
/// revocation, budgets, conditions, or an audit trail tying a decision to an
/// issued grant. The only constraint it keeps is the risk ceiling, so a
/// read-oriented allowance cannot authorize a critical operation.
#[derive(Clone, Debug)]
pub struct ConfiguredCapabilityPolicyEngine {
    policy_version: String,
    capabilities: HashSet<Capability>,
    risk_ceiling: RiskCategory,
}

impl ConfiguredCapabilityPolicyEngine {
    pub fn new(
        policy_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = Capability>,
        risk_ceiling: RiskCategory,
    ) -> Result<Self, ApplicationError> {
        let policy_version = policy_version.into();
        if policy_version.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "policy version must not be empty".into(),
            ));
        }
        Ok(Self {
            policy_version,
            capabilities: capabilities.into_iter().collect(),
            risk_ceiling,
        })
    }

    pub fn from_strings(
        policy_version: impl Into<String>,
        capabilities: &[&str],
        risk_ceiling: RiskCategory,
    ) -> Result<Self, ApplicationError> {
        let mut parsed = Vec::with_capacity(capabilities.len());
        for capability in capabilities {
            // Deployment configuration arrives as a delimited string, so entries
            // can carry surrounding whitespace from list folding.
            let capability = capability.trim();
            if capability.is_empty() {
                continue;
            }
            parsed.push(
                capability
                    .parse::<Capability>()
                    .map_err(ApplicationError::Domain)?,
            );
        }
        Self::new(policy_version, parsed, risk_ceiling)
    }

    fn decision(
        &self,
        context: &RequestContext,
        request: &AuthorizationRequest,
        result: PolicyDecisionResult,
        reason: PolicyDecisionReason,
    ) -> PolicyDecision {
        PolicyDecision {
            id: PolicyDecisionId::new(),
            policy_id: None,
            policy_version: self.policy_version.clone(),
            workspace_id: context.workspace_id,
            subject_id: context.principal_id,
            capability: request.capability.clone(),
            operation: request.operation.clone(),
            resource_scope: request.resource_scope.clone(),
            result,
            reason,
            input_state: vestrace_domain::PolicyInputState::from_request(
                context.workspace_id,
                context.principal_id,
                request,
            ),
            matched_grant_id: None,
            decided_at: now(),
        }
    }
}

#[async_trait]
impl PolicyDecisionEngine for ConfiguredCapabilityPolicyEngine {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        if self.capabilities.is_empty() {
            return Ok(self.decision(
                context,
                &request,
                PolicyDecisionResult::Deny,
                PolicyDecisionReason::DefaultDeny,
            ));
        }
        if !self.capabilities.contains(&request.capability) {
            return Ok(self.decision(
                context,
                &request,
                PolicyDecisionResult::Deny,
                PolicyDecisionReason::CapabilityMismatch,
            ));
        }
        if request.effective_risk() > self.risk_ceiling {
            return Ok(self.decision(
                context,
                &request,
                PolicyDecisionResult::Deny,
                PolicyDecisionReason::RiskExceedsCeiling,
            ));
        }
        Ok(self.decision(
            context,
            &request,
            PolicyDecisionResult::Allow,
            PolicyDecisionReason::ConfiguredAllowance,
        ))
    }
}

pub struct CapabilitySetPolicyEngine {
    capabilities: HashSet<Capability>,
}

impl CapabilitySetPolicyEngine {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }

    pub fn from_strings(strings: &[&str]) -> Result<Self, ApplicationError> {
        let mut caps = HashSet::new();
        for s in strings {
            caps.insert(s.parse::<Capability>().map_err(ApplicationError::Domain)?);
        }
        Ok(Self { capabilities: caps })
    }
}

#[async_trait]
impl PolicyEngine for CapabilitySetPolicyEngine {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(ApplicationError::Policy(format!(
                "missing required capability: {capability}"
            )))
        }
    }
}

pub struct WorkspaceScopedPolicyEngine {
    inner: CapabilitySetPolicyEngine,
    workspace_id: WorkspaceId,
}

impl WorkspaceScopedPolicyEngine {
    pub fn new(
        workspace_id: WorkspaceId,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            inner: CapabilitySetPolicyEngine::new(capabilities),
            workspace_id,
        }
    }
}

#[async_trait]
impl PolicyEngine for WorkspaceScopedPolicyEngine {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        if context.workspace_id != self.workspace_id {
            return Err(ApplicationError::Policy("workspace mismatch".to_owned()));
        }
        self.inner.evaluate(context, capability).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::{PrincipalId, WorkspaceId};

    fn ctx() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    #[tokio::test]
    async fn allow_all_permits_everything() {
        let engine = AllowAllPolicyEngine;
        let result = engine.evaluate(&ctx(), Capability::MemoryPurge).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn capability_set_denies_missing() {
        let engine = CapabilitySetPolicyEngine::new(vec![Capability::MemoryRead]);
        let result = engine.evaluate(&ctx(), Capability::MemoryPurge).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn capability_set_allows_present() {
        let engine =
            CapabilitySetPolicyEngine::new(vec![Capability::MemoryRead, Capability::MemoryWrite]);
        let result = engine.evaluate(&ctx(), Capability::MemoryRead).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn workspace_scoped_rejects_mismatch() {
        let ws = WorkspaceId::new();
        let engine = WorkspaceScopedPolicyEngine::new(ws, vec![Capability::MemoryRead]);
        let result = engine.evaluate(&ctx(), Capability::MemoryRead).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn workspace_scoped_allows_match() {
        let ws = WorkspaceId::new();
        let principal = PrincipalId::new();
        let context = RequestContext::new(ws, principal);
        let engine = WorkspaceScopedPolicyEngine::new(ws, vec![Capability::MemoryRead]);
        let result = engine.evaluate(&context, Capability::MemoryRead).await;
        assert!(result.is_ok());
    }
}
