use super::{Capability, CapabilityGrant, CapabilityGrantSpec, RiskCategory};
use crate::{DomainError, id::CapabilityGrantId, time::Timestamp};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DELEGATION_OPERATION: &str = "capability.delegate";
pub const MAX_DELEGATION_DEPTH: u8 = 8;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DelegationContract {
    pub capability: Capability,
    pub resource_scope: String,
    pub max_depth: u8,
    pub max_lifetime_seconds: Option<u64>,
    pub max_risk: RiskCategory,
    pub max_budget_units: u64,
}

impl DelegationContract {
    pub fn new(
        capability: Capability,
        resource_scope: impl Into<String>,
        max_depth: u8,
        max_lifetime_seconds: Option<u64>,
        max_risk: RiskCategory,
        max_budget_units: u64,
    ) -> Result<Self, DomainError> {
        let resource_scope = resource_scope.into();
        if resource_scope.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "delegation resource scope must not be empty".into(),
            ));
        }
        if resource_scope.contains('*') {
            return Err(DomainError::PolicyViolation(
                "delegation wildcard resource scope is not allowed".into(),
            ));
        }
        if max_depth == 0 || max_depth > MAX_DELEGATION_DEPTH {
            return Err(DomainError::PolicyViolation(format!(
                "delegation depth must be between 1 and {MAX_DELEGATION_DEPTH}"
            )));
        }
        if max_lifetime_seconds.is_some_and(|seconds| seconds > i64::MAX as u64) {
            return Err(DomainError::InvalidArgument(
                "delegation lifetime is too large".into(),
            ));
        }
        if max_budget_units == 0 {
            return Err(DomainError::InvalidArgument(
                "delegation budget must be positive".into(),
            ));
        }
        Ok(Self {
            capability,
            resource_scope,
            max_depth,
            max_lifetime_seconds,
            max_risk,
            max_budget_units,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct DelegatedCapability {
    grant: CapabilityGrant,
    parent_grant_id: CapabilityGrantId,
    delegation_grant_id: CapabilityGrantId,
    depth: u8,
}

impl DelegatedCapability {
    pub fn grant(&self) -> &CapabilityGrant {
        &self.grant
    }

    pub const fn parent_grant_id(&self) -> CapabilityGrantId {
        self.parent_grant_id
    }

    pub const fn delegation_grant_id(&self) -> CapabilityGrantId {
        self.delegation_grant_id
    }

    pub const fn depth(&self) -> u8 {
        self.depth
    }

    pub fn into_grant(self) -> CapabilityGrant {
        self.grant
    }

    pub fn from_root(
        parent: &CapabilityGrant,
        delegation_permission: &CapabilityGrant,
        child_spec: CapabilityGrantSpec,
        contract: &DelegationContract,
        budget: &mut HierarchicalBudget,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        let parent_budget = parent
            .budget
            .map(|constraint| constraint.max_units)
            .unwrap_or(contract.max_budget_units);
        validate_attenuation(parent, delegation_permission, &child_spec, contract, 0, at)?;
        let child_budget = child_spec.budget.ok_or_else(|| {
            DomainError::PolicyViolation("delegated child requires a budget".into())
        })?;
        if child_budget.max_units > parent_budget {
            return Err(DomainError::PolicyViolation(
                "delegated budget exceeds parent allocation".into(),
            ));
        }
        let grant = CapabilityGrant::issue(child_spec, at)?;
        budget.register_root_with_child(
            parent.id,
            parent_budget,
            grant.id,
            child_budget.max_units,
        )?;
        Ok(DelegatedCapability {
            grant,
            parent_grant_id: parent.id,
            delegation_grant_id: delegation_permission.id,
            depth: 1,
        })
    }

    pub fn delegate(
        &self,
        delegation_permission: &CapabilityGrant,
        child_spec: CapabilityGrantSpec,
        contract: &DelegationContract,
        budget: &mut HierarchicalBudget,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        validate_attenuation(
            &self.grant,
            delegation_permission,
            &child_spec,
            contract,
            self.depth,
            at,
        )?;
        attenuate(
            &self.grant,
            delegation_permission,
            child_spec,
            self.depth,
            budget,
            at,
        )
    }
}

fn attenuate(
    parent: &CapabilityGrant,
    delegation_permission: &CapabilityGrant,
    child_spec: CapabilityGrantSpec,
    parent_depth: u8,
    budget: &mut HierarchicalBudget,
    at: Timestamp,
) -> Result<DelegatedCapability, DomainError> {
    let child_budget = child_spec
        .budget
        .ok_or_else(|| DomainError::PolicyViolation("delegated child requires a budget".into()))?;
    let child_id = child_spec.id;
    let grant = CapabilityGrant::issue(child_spec, at)?;
    budget.reserve_child(parent.id, child_id, child_budget.max_units)?;
    Ok(DelegatedCapability {
        grant,
        parent_grant_id: parent.id,
        delegation_grant_id: delegation_permission.id,
        depth: parent_depth + 1,
    })
}

fn validate_attenuation(
    parent: &CapabilityGrant,
    delegation_permission: &CapabilityGrant,
    child: &CapabilityGrantSpec,
    contract: &DelegationContract,
    parent_depth: u8,
    at: Timestamp,
) -> Result<(), DomainError> {
    if !parent.is_active_at(at) {
        return Err(DomainError::PolicyViolation(
            "delegation parent grant is not active".into(),
        ));
    }
    if !delegation_permission.is_active_at(at) {
        return Err(DomainError::PolicyViolation(
            "delegation permission is not active".into(),
        ));
    }
    if delegation_permission.capability != Capability::CapabilityDelegate
        || delegation_permission.operation != DELEGATION_OPERATION
    {
        return Err(DomainError::PolicyViolation(
            "explicit capability.delegate permission is required".into(),
        ));
    }
    if parent.workspace_id != delegation_permission.workspace_id
        || parent.workspace_id != child.workspace_id
    {
        return Err(DomainError::PolicyViolation(
            "delegation workspace mismatch".into(),
        ));
    }
    if delegation_permission.subject_id != parent.subject_id || child.issuer_id != parent.subject_id
    {
        return Err(DomainError::PolicyViolation(
            "delegation subject/issuer mismatch".into(),
        ));
    }
    if child.subject_id == parent.subject_id {
        return Err(DomainError::PolicyViolation(
            "delegation cannot self-escalate".into(),
        ));
    }
    for required_condition in parent
        .conditions
        .iter()
        .chain(delegation_permission.conditions.iter())
    {
        if !child
            .conditions
            .iter()
            .any(|condition| condition == required_condition)
        {
            return Err(DomainError::PolicyViolation(
                "delegated grant cannot drop a parent condition".into(),
            ));
        }
    }
    if child.capability != parent.capability || child.capability != contract.capability {
        return Err(DomainError::PolicyViolation(
            "delegated capability exceeds parent authority".into(),
        ));
    }
    if !selector_is_within(&child.operation, &parent.operation)
        || !scope_is_within(&child.resource_scope, &parent.resource_scope)
        || !scope_is_within(&contract.resource_scope, &parent.resource_scope)
        || !scope_is_within(&child.resource_scope, &contract.resource_scope)
        || !scope_is_within(
            &contract.resource_scope,
            &delegation_permission.resource_scope,
        )
    {
        return Err(DomainError::PolicyViolation(
            "delegated selector exceeds parent or delegation scope".into(),
        ));
    }
    if child.risk_ceiling > parent.risk_ceiling
        || child.risk_ceiling > contract.max_risk
        || child.risk_ceiling > delegation_permission.risk_ceiling
    {
        return Err(DomainError::PolicyViolation(
            "delegated risk exceeds parent ceiling".into(),
        ));
    }
    let child_budget = child
        .budget
        .ok_or_else(|| DomainError::PolicyViolation("delegated child requires a budget".into()))?;
    if child_budget.max_units > contract.max_budget_units
        || delegation_permission
            .budget
            .is_some_and(|permission| contract.max_budget_units > permission.max_units)
    {
        return Err(DomainError::PolicyViolation(
            "delegated budget exceeds explicit delegation ceiling".into(),
        ));
    }
    if parent_depth >= MAX_DELEGATION_DEPTH || parent_depth + 1 > contract.max_depth {
        return Err(DomainError::PolicyViolation(
            "delegation depth limit exceeded".into(),
        ));
    }
    if child.valid_from < at || child.valid_from < parent.valid_from {
        return Err(DomainError::PolicyViolation(
            "delegated validity cannot begin before delegation".into(),
        ));
    }
    if let Some(parent_until) = parent.valid_until {
        if child
            .valid_until
            .is_none_or(|child_until| child_until > parent_until)
        {
            return Err(DomainError::PolicyViolation(
                "delegated validity exceeds parent interval".into(),
            ));
        }
    }
    if let Some(permission_until) = delegation_permission.valid_until {
        if child
            .valid_until
            .is_none_or(|child_until| child_until > permission_until)
        {
            return Err(DomainError::PolicyViolation(
                "delegated validity exceeds delegation permission interval".into(),
            ));
        }
    }
    if let Some(max_lifetime_seconds) = contract.max_lifetime_seconds {
        let max_until = child
            .valid_from
            .checked_add_signed(Duration::seconds(max_lifetime_seconds as i64))
            .ok_or_else(|| {
                DomainError::InvalidArgument("delegation lifetime overflows timestamp".into())
            })?;
        if child
            .valid_until
            .is_none_or(|child_until| child_until > max_until)
        {
            return Err(DomainError::PolicyViolation(
                "delegated validity exceeds contract lifetime".into(),
            ));
        }
    }
    Ok(())
}

/// Whether `child` names the same operation as `parent` or one beneath it.
///
/// Operations form a dotted hierarchy — `http` contains `http.get`, and
/// `memory.write` contains `memory.write.metadata`. The boundary must be a
/// separator: `memory.writer` is **not** beneath `memory.write`, or a grant
/// would silently cover neighbours whose names merely start the same way.
///
/// A wildcard on either side is refused rather than interpreted. Nothing in the
/// system defines what `*` means here, and guessing would make a grant's reach
/// depend on a convention no code enforces.
pub fn selector_is_within(child: &str, parent: &str) -> bool {
    if child.contains('*') || parent.contains('*') {
        return false;
    }
    child == parent || child.starts_with(&format!("{parent}."))
}

/// Whether `child` names the same resource as `parent` or one inside it.
///
/// The slash-separated counterpart of [`selector_is_within`], and the rule that
/// makes a grant usable for anything hierarchical: `/v1` contains
/// `/v1/memories/0198…`, while `/v1-admin` is not inside `/v1`.
///
/// # Why a plain prefix test was not enough
///
/// This was `child == parent || child.starts_with("{parent}/")`, which is
/// correct for paths and silently useless for the URI-shaped scopes the
/// resource checks use. `memory://` could not contain `memory://0198…`, because
/// the test looked for `memory:///` — so **"may purge any memory in this
/// workspace" was not expressible**, only "may purge this one". An operator who
/// cannot say what they mean says something broader instead: in practice, a
/// grant on `/v1` with the `http` operation, which covers far more than the one
/// resource they had in mind.
///
/// The rule now is: `child` is inside `parent` when it continues past it at a
/// boundary. A parent that already ends in a separator needs only something to
/// follow it; one that does not needs the next character to be `/`. That keeps
/// the property the prefix test existed for — `memory://abc` is **not** inside
/// `memory://ab`, and `/v1-admin` is **not** inside `/v1` — while letting a
/// scheme-shaped scope contain the resources under it.
pub fn scope_is_within(child: &str, parent: &str) -> bool {
    if child.contains('*') || parent.contains('*') {
        return false;
    }
    // An empty parent would contain everything, which is a grant nobody wrote
    // deliberately. Grants validate their scope as non-blank; this refuses it
    // again rather than trusting that.
    if parent.is_empty() {
        return child.is_empty();
    }
    if child == parent {
        return true;
    }
    let Some(rest) = child.strip_prefix(parent) else {
        return false;
    };
    if parent.ends_with('/') {
        // `memory://`, `finding://`, `/v1/` — anything beneath is inside.
        !rest.is_empty()
    } else {
        // `/v1` contains `/v1/memories`; it does not contain `/v1-admin`.
        rest.starts_with('/')
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BudgetReservation {
    pub grant_id: CapabilityGrantId,
    pub parent_grant_id: Option<CapabilityGrantId>,
    pub allocated_units: u64,
    pub consumed_units: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HierarchicalBudget {
    pub hard_limit: u64,
    pub reservations: HashMap<CapabilityGrantId, BudgetReservation>,
}

impl HierarchicalBudget {
    pub fn new(hard_limit: u64) -> Self {
        Self {
            hard_limit,
            reservations: HashMap::new(),
        }
    }

    pub fn register_root(
        &mut self,
        grant_id: CapabilityGrantId,
        allocated_units: u64,
    ) -> Result<(), DomainError> {
        if allocated_units > self.hard_limit {
            return Err(DomainError::PolicyViolation(
                "root budget exceeds hard limit".into(),
            ));
        }
        if self
            .reservations
            .values()
            .any(|reservation| reservation.parent_grant_id.is_none())
        {
            return Err(DomainError::PolicyViolation(
                "hierarchical budget already has a root".into(),
            ));
        }
        if self.reservations.contains_key(&grant_id) {
            return Err(DomainError::PolicyViolation(
                "budget grant is already registered".into(),
            ));
        }
        self.reservations.insert(
            grant_id,
            BudgetReservation {
                grant_id,
                parent_grant_id: None,
                allocated_units,
                consumed_units: 0,
            },
        );
        Ok(())
    }

    pub fn register_root_with_child(
        &mut self,
        root_grant_id: CapabilityGrantId,
        root_allocated_units: u64,
        child_grant_id: CapabilityGrantId,
        child_allocated_units: u64,
    ) -> Result<(), DomainError> {
        if child_allocated_units == 0 {
            return Err(DomainError::InvalidArgument(
                "budget reservation must be positive".into(),
            ));
        }
        if child_allocated_units > root_allocated_units {
            return Err(DomainError::PolicyViolation(
                "hierarchical budget allocation exceeded".into(),
            ));
        }
        if root_grant_id == child_grant_id {
            return Err(DomainError::PolicyViolation(
                "budget root and child must be distinct".into(),
            ));
        }
        if self.reservations.contains_key(&root_grant_id)
            || self.reservations.contains_key(&child_grant_id)
        {
            return Err(DomainError::PolicyViolation(
                "budget grant is already registered".into(),
            ));
        }
        if root_allocated_units > self.hard_limit {
            return Err(DomainError::PolicyViolation(
                "root budget exceeds hard limit".into(),
            ));
        }
        if self
            .reservations
            .values()
            .any(|reservation| reservation.parent_grant_id.is_none())
        {
            return Err(DomainError::PolicyViolation(
                "hierarchical budget already has a root".into(),
            ));
        }
        self.reservations.insert(
            root_grant_id,
            BudgetReservation {
                grant_id: root_grant_id,
                parent_grant_id: None,
                allocated_units: root_allocated_units,
                consumed_units: 0,
            },
        );
        self.reservations.insert(
            child_grant_id,
            BudgetReservation {
                grant_id: child_grant_id,
                parent_grant_id: Some(root_grant_id),
                allocated_units: child_allocated_units,
                consumed_units: 0,
            },
        );
        Ok(())
    }

    pub fn reserve_child(
        &mut self,
        parent_grant_id: CapabilityGrantId,
        child_grant_id: CapabilityGrantId,
        allocated_units: u64,
    ) -> Result<(), DomainError> {
        if allocated_units == 0 {
            return Err(DomainError::InvalidArgument(
                "budget reservation must be positive".into(),
            ));
        }
        if self.reservations.contains_key(&child_grant_id) {
            return Err(DomainError::PolicyViolation(
                "budget grant is already reserved".into(),
            ));
        }
        if self
            .remaining(parent_grant_id)
            .is_none_or(|remaining| remaining < allocated_units)
        {
            return Err(DomainError::PolicyViolation(
                "hierarchical budget allocation exceeded".into(),
            ));
        }
        if !self.reservations.contains_key(&parent_grant_id) {
            return Err(DomainError::NotFound(format!(
                "budget parent {parent_grant_id}"
            )));
        }
        self.reservations.insert(
            child_grant_id,
            BudgetReservation {
                grant_id: child_grant_id,
                parent_grant_id: Some(parent_grant_id),
                allocated_units,
                consumed_units: 0,
            },
        );
        Ok(())
    }

    pub fn charge(&mut self, grant_id: CapabilityGrantId, amount: u64) -> Result<(), DomainError> {
        let remaining = self
            .remaining(grant_id)
            .ok_or_else(|| DomainError::NotFound(format!("budget grant {grant_id}")))?;
        if amount > remaining {
            return Err(DomainError::PolicyViolation(
                "budget charge exceeds reservation".into(),
            ));
        }
        let reservation = self
            .reservations
            .get_mut(&grant_id)
            .expect("remaining checked reservation presence");
        reservation.consumed_units += amount;
        Ok(())
    }

    pub fn remaining(&self, grant_id: CapabilityGrantId) -> Option<u64> {
        let reservation = self.reservations.get(&grant_id)?;
        let children_reserved = self
            .reservations
            .values()
            .filter(|child| child.parent_grant_id == Some(grant_id))
            .map(|child| child.allocated_units)
            .sum::<u64>();
        reservation
            .allocated_units
            .checked_sub(reservation.consumed_units.saturating_add(children_reserved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_and_scope_helpers_only_accept_narrow_descendants() {
        assert!(selector_is_within("memory.write.metadata", "memory.write"));
        assert!(!selector_is_within("memory.purge", "memory.write"));
        assert!(scope_is_within("memory:one/item", "memory:one"));
        assert!(!scope_is_within("memory:one-other", "memory:one"));
    }
}

#[cfg(test)]
mod scope_matching_tests {
    use super::scope_is_within;

    /// The property the plain prefix test was written for, kept.
    #[test]
    fn a_path_contains_what_is_under_it_and_nothing_beside_it() {
        assert!(scope_is_within("/v1", "/v1"));
        assert!(scope_is_within("/v1/memories/0198", "/v1"));
        assert!(scope_is_within("/v1/memories/0198", "/v1/memories"));

        // Sharing a prefix is not being inside.
        assert!(!scope_is_within("/v1-admin", "/v1"));
        assert!(!scope_is_within("/v11", "/v1"));
        // And the other direction never holds.
        assert!(!scope_is_within("/v1", "/v1/memories"));
    }

    /// The case that could not be expressed: a scheme-shaped scope containing
    /// the resources under it.
    #[test]
    fn a_scheme_contains_the_resources_under_it() {
        assert!(scope_is_within("memory://0198", "memory://"));
        assert!(scope_is_within("finding://0198", "finding://"));
        assert!(scope_is_within("memory://0198", "memory://0198"));
    }

    /// The dangerous direction. A grant for one resource must not quietly cover
    /// another whose identifier merely starts with the same characters.
    #[test]
    fn one_resource_does_not_contain_its_neighbour() {
        assert!(!scope_is_within("memory://abc", "memory://ab"));
        assert!(!scope_is_within("memory://0198x", "memory://0198"));
        assert!(!scope_is_within("finding://0198x", "finding://0198"));
        // A different scheme is a different world.
        assert!(!scope_is_within("memory://0198", "finding://"));
        assert!(!scope_is_within("memoryx://0198", "memory://"));
    }

    /// A wildcard is refused rather than interpreted, on either side.
    #[test]
    fn wildcards_are_not_a_scope_language() {
        assert!(!scope_is_within("memory://*", "memory://"));
        assert!(!scope_is_within("memory://0198", "memory://*"));
        assert!(!scope_is_within("*", "*"));
    }

    /// An empty parent would contain everything.
    #[test]
    fn an_empty_scope_contains_nothing() {
        assert!(!scope_is_within("memory://0198", ""));
        assert!(!scope_is_within("/v1", ""));
    }
}
