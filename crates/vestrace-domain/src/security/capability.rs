use super::{Capability, scope_is_within, selector_is_within};
use crate::{
    DomainError,
    id::{CapabilityGrantId, PolicyDecisionId, PolicyId, PrincipalId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for RiskCategory {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BudgetConstraint {
    pub max_units: u64,
}

impl BudgetConstraint {
    pub const fn new(max_units: u64) -> Self {
        Self { max_units }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GrantCondition {
    pub name: String,
    pub expected_value: String,
}

impl GrantCondition {
    pub fn new(
        name: impl Into<String>,
        expected_value: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let expected_value = expected_value.into();
        if name.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "grant condition name must not be empty".into(),
            ));
        }
        if expected_value.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "grant condition value must not be empty".into(),
            ));
        }
        Ok(Self {
            name,
            expected_value,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityGrantStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityGrantSpec {
    pub id: CapabilityGrantId,
    pub workspace_id: WorkspaceId,
    pub subject_id: PrincipalId,
    pub issuer_id: PrincipalId,
    pub capability: Capability,
    pub operation: String,
    pub resource_scope: String,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub budget: Option<BudgetConstraint>,
    pub risk_ceiling: RiskCategory,
    pub conditions: Vec<GrantCondition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityGrant {
    pub id: CapabilityGrantId,
    pub workspace_id: WorkspaceId,
    pub subject_id: PrincipalId,
    pub issuer_id: PrincipalId,
    pub capability: Capability,
    pub operation: String,
    pub resource_scope: String,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub budget: Option<BudgetConstraint>,
    pub risk_ceiling: RiskCategory,
    pub conditions: Vec<GrantCondition>,
    pub status: CapabilityGrantStatus,
    pub created_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

impl CapabilityGrant {
    pub fn issue(spec: CapabilityGrantSpec, created_at: Timestamp) -> Result<Self, DomainError> {
        if spec.operation.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "capability grant operation must not be empty".into(),
            ));
        }
        // Parsed, not merely non-blank. A scope in no vocabulary this system
        // checks is not a narrow grant, it is a grant that denies everything
        // forever, and the only signal an operator got was a `ResourceMismatch`
        // on every later request. See [`ResourceScope`](super::ResourceScope).
        super::ResourceScope::parse(&spec.resource_scope)?;
        if let Some(valid_until) = spec.valid_until {
            if valid_until <= spec.valid_from {
                return Err(DomainError::InvalidArgument(
                    "capability grant valid_until must be after valid_from".into(),
                ));
            }
        }
        for condition in &spec.conditions {
            if condition.name.trim().is_empty() {
                return Err(DomainError::InvalidArgument(
                    "capability grant condition name must not be empty".into(),
                ));
            }
            if condition.expected_value.trim().is_empty() {
                return Err(DomainError::InvalidArgument(
                    "capability grant condition value must not be empty".into(),
                ));
            }
        }

        Ok(Self {
            id: spec.id,
            workspace_id: spec.workspace_id,
            subject_id: spec.subject_id,
            issuer_id: spec.issuer_id,
            capability: spec.capability,
            operation: spec.operation,
            resource_scope: spec.resource_scope,
            valid_from: spec.valid_from,
            valid_until: spec.valid_until,
            budget: spec.budget,
            risk_ceiling: spec.risk_ceiling,
            conditions: spec.conditions,
            status: CapabilityGrantStatus::Active,
            created_at,
            revoked_at: None,
        })
    }

    pub fn revoke(&mut self, at: Timestamp) -> Result<(), DomainError> {
        if self.status == CapabilityGrantStatus::Revoked {
            return Err(DomainError::PolicyViolation(
                "capability grant is already revoked".into(),
            ));
        }
        self.status = CapabilityGrantStatus::Revoked;
        self.revoked_at = Some(at);
        Ok(())
    }

    /// Whether this grant may be relied on for an action judged at `at`.
    ///
    /// # Revocation is not time-travel-exempt
    ///
    /// The validity window is evaluated against `at`, and revocation is **not**:
    /// a revoked grant is unusable at every instant, including ones before
    /// `revoked_at`. That asymmetry is deliberate and is the safe direction. The
    /// alternative — answering "was it active then" faithfully — would mean a
    /// caller presenting an older instant could still be authorized by a grant
    /// that has since been revoked, which is precisely the stale-authority hole
    /// CAP-014 is about.
    ///
    /// So this does not reconstruct history. `revoked_at` is recorded for a
    /// reader that wants to; nothing that decides authority should use it that
    /// way.
    pub fn is_active_at(&self, at: Timestamp) -> bool {
        self.status == CapabilityGrantStatus::Active
            && at >= self.valid_from
            && self.valid_until.map(|until| at < until).unwrap_or(true)
    }

    fn failure_reason(
        &self,
        workspace_id: WorkspaceId,
        subject_id: PrincipalId,
        request: &AuthorizationRequest,
        at: Timestamp,
    ) -> Option<PolicyDecisionReason> {
        if self.workspace_id != workspace_id {
            return Some(PolicyDecisionReason::WorkspaceMismatch);
        }
        if self.subject_id != subject_id {
            return Some(PolicyDecisionReason::SubjectMismatch);
        }
        if self.capability != request.capability {
            return Some(PolicyDecisionReason::CapabilityMismatch);
        }
        // A grant covers what it names and what lies beneath it. Exact equality
        // here made grants unusable for anything hierarchical: the HTTP boundary
        // scopes each request to its own path, so authorizing a deployment would
        // have needed one grant per URL — an access control list rather than a
        // capability model.
        //
        // The rule is the one delegation already uses to decide whether a child
        // grant stays inside its parent, so "what a grant covers" means the same
        // thing in both places. It only ever widens *downward*: a grant for
        // `/v1/memories` never covers `/v1`, and the match must land on a
        // separator so `/v1-admin` is not inside `/v1`.
        if !selector_is_within(&request.operation, &self.operation) {
            return Some(PolicyDecisionReason::OperationMismatch);
        }
        if !scope_is_within(&request.resource_scope, &self.resource_scope) {
            return Some(PolicyDecisionReason::ResourceMismatch);
        }
        if self.status == CapabilityGrantStatus::Revoked {
            return Some(PolicyDecisionReason::Revoked);
        }
        if at < self.valid_from {
            return Some(PolicyDecisionReason::NotYetValid);
        }
        if self.valid_until.map(|until| at >= until).unwrap_or(false) {
            return Some(PolicyDecisionReason::Expired);
        }
        if let Some(budget) = self.budget {
            if request.requested_budget_units > budget.max_units {
                return Some(PolicyDecisionReason::BudgetExceeded);
            }
        }
        if request.effective_risk() > self.risk_ceiling {
            return Some(PolicyDecisionReason::RiskExceedsCeiling);
        }
        if !self
            .conditions
            .iter()
            .all(|expected| request.conditions.iter().any(|actual| actual == expected))
        {
            return Some(PolicyDecisionReason::ConditionNotSatisfied);
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuthorizationRequest {
    pub capability: Capability,
    pub operation: String,
    pub resource_scope: String,
    pub requested_risk: RiskCategory,
    pub context_risk: RiskCategory,
    pub requested_budget_units: u64,
    pub conditions: Vec<GrantCondition>,
}

impl AuthorizationRequest {
    pub fn new(
        capability: Capability,
        operation: impl Into<String>,
        resource_scope: impl Into<String>,
        requested_risk: RiskCategory,
    ) -> Self {
        Self {
            capability,
            operation: operation.into(),
            resource_scope: resource_scope.into(),
            requested_risk,
            context_risk: RiskCategory::Low,
            requested_budget_units: 0,
            conditions: Vec::new(),
        }
    }

    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = operation.into();
        self
    }

    pub fn with_resource_scope(mut self, resource_scope: impl Into<String>) -> Self {
        self.resource_scope = resource_scope.into();
        self
    }

    pub fn with_budget_units(mut self, requested_budget_units: u64) -> Self {
        self.requested_budget_units = requested_budget_units;
        self
    }

    pub fn with_context_risk(mut self, context_risk: RiskCategory) -> Self {
        self.context_risk = context_risk;
        self
    }

    pub fn with_condition(mut self, condition: GrantCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn effective_risk(&self) -> RiskCategory {
        self.requested_risk.max(self.context_risk)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PolicyInputState {
    pub workspace_id: WorkspaceId,
    pub subject_id: PrincipalId,
    pub capability: Capability,
    pub operation: String,
    pub resource_scope: String,
    pub requested_risk: RiskCategory,
    pub context_risk: RiskCategory,
    pub effective_risk: RiskCategory,
    pub requested_budget_units: u64,
    pub conditions: Vec<GrantCondition>,
}

impl PolicyInputState {
    /// Records exactly what a decision was made about. Engines outside the
    /// domain need this so every decision they emit carries the same evidence
    /// as a grant-based one.
    pub fn from_request(
        workspace_id: WorkspaceId,
        subject_id: PrincipalId,
        request: &AuthorizationRequest,
    ) -> Self {
        Self {
            workspace_id,
            subject_id,
            capability: request.capability.clone(),
            operation: request.operation.clone(),
            resource_scope: request.resource_scope.clone(),
            requested_risk: request.requested_risk,
            context_risk: request.context_risk,
            effective_risk: request.effective_risk(),
            requested_budget_units: request.requested_budget_units,
            conditions: request.conditions.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionResult {
    Deny,
    Allow,
    PrepareOnly,
    RequireApproval,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionReason {
    DefaultDeny,
    GrantMatched,
    /// A deployment-configured static allowance permitted the request. This is
    /// deliberately distinct from `GrantMatched`: no capability grant was
    /// consulted, so the decision carries no grant identity, no subject scope
    /// and no revocation path.
    ConfiguredAllowance,
    WorkspaceMismatch,
    SubjectMismatch,
    CapabilityMismatch,
    OperationMismatch,
    ResourceMismatch,
    NotYetValid,
    Expired,
    Revoked,
    BudgetExceeded,
    RiskExceedsCeiling,
    ConditionNotSatisfied,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PolicyDecision {
    pub id: PolicyDecisionId,
    pub policy_id: Option<PolicyId>,
    pub policy_version: String,
    pub workspace_id: WorkspaceId,
    pub subject_id: PrincipalId,
    pub capability: Capability,
    pub operation: String,
    pub resource_scope: String,
    pub result: PolicyDecisionResult,
    pub reason: PolicyDecisionReason,
    pub input_state: PolicyInputState,
    pub matched_grant_id: Option<CapabilityGrantId>,
    pub decided_at: Timestamp,
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        self.result == PolicyDecisionResult::Allow
    }
}

pub fn evaluate_capability_grants(
    decision_id: PolicyDecisionId,
    workspace_id: WorkspaceId,
    subject_id: PrincipalId,
    policy_version: impl Into<String>,
    request: &AuthorizationRequest,
    grants: &[CapabilityGrant],
    decided_at: Timestamp,
) -> Result<PolicyDecision, DomainError> {
    let policy_version = policy_version.into();
    if policy_version.trim().is_empty() {
        return Err(DomainError::InvalidArgument(
            "policy version must not be empty".into(),
        ));
    }
    if request.operation.trim().is_empty() {
        return Err(DomainError::InvalidArgument(
            "authorization operation must not be empty".into(),
        ));
    }
    // A request in a private vocabulary is a bug in the caller, not a denial:
    // no grant an operator could write would ever match it, so reporting it as
    // `ResourceMismatch` would blame the operator for the caller's spelling.
    // Refusing here is what stops a new call site inventing a sixth vocabulary
    // and finding out in production.
    super::ResourceScope::parse(&request.resource_scope)?;
    let input_state = PolicyInputState::from_request(workspace_id, subject_id, request);
    let mut first_failure = None;

    for grant in grants {
        if grant
            .failure_reason(workspace_id, subject_id, request, decided_at)
            .is_none()
        {
            return Ok(PolicyDecision {
                id: decision_id,
                policy_id: None,
                policy_version,
                workspace_id,
                subject_id,
                capability: request.capability.clone(),
                operation: request.operation.clone(),
                resource_scope: request.resource_scope.clone(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::GrantMatched,
                input_state,
                matched_grant_id: Some(grant.id),
                decided_at,
            });
        }

        if first_failure.is_none() {
            first_failure = grant.failure_reason(workspace_id, subject_id, request, decided_at);
        }
    }

    Ok(PolicyDecision {
        id: decision_id,
        policy_id: None,
        policy_version,
        workspace_id,
        subject_id,
        capability: request.capability.clone(),
        operation: request.operation.clone(),
        resource_scope: request.resource_scope.clone(),
        result: PolicyDecisionResult::Deny,
        reason: first_failure.unwrap_or(PolicyDecisionReason::DefaultDeny),
        input_state,
        matched_grant_id: None,
        decided_at,
    })
}
