use std::collections::{BTreeMap, HashMap};

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::{
    Capability, DomainError, HealthFindingId, HealthOccurrenceId, PrincipalId, RepairExecutionId,
    RepairPlanId, Timestamp, VerificationRunId, WorkspaceId,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Repairability {
    Auto,
    Manual,
    NotRepairable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthProjectionState {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl From<HealthState> for HealthProjectionState {
    fn from(value: HealthState) -> Self {
        match value {
            HealthState::Healthy => Self::Healthy,
            HealthState::Degraded => Self::Degraded,
            HealthState::Unhealthy => Self::Unhealthy,
            HealthState::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HealthScope {
    Workspace(WorkspaceId),
    Resource {
        workspace_id: WorkspaceId,
        resource_type: String,
        resource_id: String,
    },
}

impl HealthScope {
    pub fn workspace(workspace_id: WorkspaceId) -> Self {
        Self::Workspace(workspace_id)
    }

    fn validate(&self) -> Result<(), DomainError> {
        if let Self::Resource {
            resource_type,
            resource_id,
            ..
        } = self
        {
            if resource_type.trim().is_empty() || resource_id.trim().is_empty() {
                return Err(DomainError::InvalidArgument(
                    "health resource scope must identify a resource".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvariantDefinition {
    invariant_id: String,
    version: String,
    description: String,
    /// What an operator should do when this invariant is violated.
    ///
    /// It belongs to the invariant rather than to each observation: the action
    /// follows from what was violated, not from which run noticed. Keeping it
    /// per-observation is how eight checks came to carry eight hand-written
    /// remediation strings with nothing holding them to a registry.
    remediation: String,
    default_severity: HealthSeverity,
    repairability: Repairability,
    dependencies: Vec<String>,
}

impl InvariantDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invariant_id: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
        remediation: impl Into<String>,
        default_severity: HealthSeverity,
        repairability: Repairability,
        dependencies: Vec<String>,
    ) -> Result<Self, DomainError> {
        let invariant_id = invariant_id.into();
        let version = version.into();
        let description = description.into();
        let remediation = remediation.into();
        if invariant_id.trim().is_empty() || version.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "invariant id and version are required".into(),
            ));
        }
        if description.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "invariant description is required".into(),
            ));
        }
        // A violation an operator cannot act on is a notification, not a
        // finding. Requiring the text here means the gap shows up when the
        // invariant is defined rather than when somebody is paged.
        if remediation.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "invariant remediation is required".into(),
            ));
        }
        if dependencies
            .iter()
            .any(|dependency| dependency.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "invariant dependencies must not be empty".into(),
            ));
        }
        Ok(Self {
            invariant_id,
            version,
            description,
            remediation,
            default_severity,
            repairability,
            dependencies,
        })
    }

    pub fn invariant_id(&self) -> &str {
        &self.invariant_id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn remediation(&self) -> &str {
        &self.remediation
    }

    pub fn default_severity(&self) -> HealthSeverity {
        self.default_severity
    }

    pub fn repairability(&self) -> Repairability {
        self.repairability
    }

    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvariantRegistry {
    definitions: BTreeMap<String, InvariantDefinition>,
}

impl InvariantRegistry {
    pub fn new(
        definitions: impl IntoIterator<Item = InvariantDefinition>,
    ) -> Result<Self, DomainError> {
        let mut registry = Self::default();
        for definition in definitions {
            registry.register(definition)?;
        }
        Ok(registry)
    }

    pub fn register(&mut self, definition: InvariantDefinition) -> Result<(), DomainError> {
        if self
            .definitions
            .insert(definition.invariant_id.clone(), definition)
            .is_some()
        {
            return Err(DomainError::InvalidArgument(
                "invariant id must be unique in the registry".into(),
            ));
        }
        Ok(())
    }

    pub fn get(&self, invariant_id: &str) -> Option<&InvariantDefinition> {
        self.definitions.get(invariant_id)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &InvariantDefinition> {
        self.definitions.values()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingLifecycleStatus {
    Open,
    Reopened,
    Resolved,
    Suppressed,
    AcceptedRisk,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FindingDisposition {
    AutoRepair,
    ManualRequired,
    NotRepairable,
    Suppressed {
        actor_id: PrincipalId,
        reason: String,
        expires_at: Option<Timestamp>,
        policy_version: String,
        audit_ref: String,
    },
    AcceptedRisk {
        actor_id: PrincipalId,
        reason: String,
        expires_at: Option<Timestamp>,
        policy_version: String,
        audit_ref: String,
    },
}

impl FindingDisposition {
    pub fn accepted_risk(
        actor_id: PrincipalId,
        reason: impl Into<String>,
        expires_at: Option<Timestamp>,
        policy_version: impl Into<String>,
        audit_ref: impl Into<String>,
    ) -> Self {
        Self::AcceptedRisk {
            actor_id,
            reason: reason.into(),
            expires_at,
            policy_version: policy_version.into(),
            audit_ref: audit_ref.into(),
        }
    }

    pub fn suppressed(
        actor_id: PrincipalId,
        reason: impl Into<String>,
        expires_at: Option<Timestamp>,
        policy_version: impl Into<String>,
        audit_ref: impl Into<String>,
    ) -> Self {
        Self::Suppressed {
            actor_id,
            reason: reason.into(),
            expires_at,
            policy_version: policy_version.into(),
            audit_ref: audit_ref.into(),
        }
    }

    /// When this disposition stops applying, if its author bounded it.
    pub fn expires_at(&self) -> Option<Timestamp> {
        match self {
            Self::Suppressed { expires_at, .. } | Self::AcceptedRisk { expires_at, .. } => {
                *expires_at
            }
            _ => None,
        }
    }

    /// Whether the disposition still applies at `at`.
    ///
    /// # Why this exists
    ///
    /// `expires_at` was recorded in four places and read in none. A suppression
    /// or an accepted risk therefore silenced its finding for ever, whatever
    /// bound its author put on it — "until Friday" and "permanently" were the
    /// same value, and only one of them was what anybody agreed to.
    ///
    /// Only the two silencing dispositions can lapse. `AutoRepair`,
    /// `ManualRequired` and `NotRepairable` are classifications rather than
    /// decisions to look away, so they have nothing to expire.
    pub fn has_lapsed(&self, at: Timestamp) -> bool {
        self.expires_at().is_some_and(|expires_at| at > expires_at)
    }

    /// Whether this disposition silences the finding it is attached to.
    pub fn silences(&self) -> bool {
        matches!(self, Self::Suppressed { .. } | Self::AcceptedRisk { .. })
    }

    fn validate(&self) -> Result<(), DomainError> {
        let (reason, policy_version, audit_ref) = match self {
            Self::Suppressed {
                reason,
                policy_version,
                audit_ref,
                ..
            }
            | Self::AcceptedRisk {
                reason,
                policy_version,
                audit_ref,
                ..
            } => (reason, policy_version, audit_ref),
            _ => return Ok(()),
        };
        if reason.trim().is_empty()
            || policy_version.trim().is_empty()
            || audit_ref.trim().is_empty()
        {
            return Err(DomainError::InvalidArgument(
                "finding disposition requires reason, policy version and audit reference".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthFinding {
    id: HealthFindingId,
    invariant_id: String,
    invariant_version: String,
    fingerprint: String,
    scope: HealthScope,
    severity: HealthSeverity,
    repair_risk: RepairRisk,
    repairability: Repairability,
    evidence_refs: Vec<String>,
    observed_state: HealthState,
    lifecycle_status: FindingLifecycleStatus,
    disposition: Option<FindingDisposition>,
    first_seen_at: Timestamp,
    last_seen_at: Timestamp,
    occurrence_count: u64,
}

impl HealthFinding {
    pub fn new(
        definition: &InvariantDefinition,
        scope: HealthScope,
        fingerprint: impl Into<String>,
        observed_state: HealthState,
        evidence_refs: Vec<String>,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        scope.validate()?;
        let fingerprint = fingerprint.into();
        if fingerprint.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "health finding fingerprint is required".into(),
            ));
        }
        if evidence_refs.is_empty()
            || evidence_refs
                .iter()
                .any(|reference| reference.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "health finding requires non-empty evidence references".into(),
            ));
        }
        Ok(Self {
            id: HealthFindingId::new(),
            invariant_id: definition.invariant_id.clone(),
            invariant_version: definition.version.clone(),
            fingerprint,
            scope,
            severity: definition.default_severity,
            repair_risk: match definition.repairability {
                Repairability::Auto => RepairRisk::Low,
                Repairability::Manual => RepairRisk::Medium,
                Repairability::NotRepairable => RepairRisk::High,
            },
            repairability: definition.repairability,
            evidence_refs,
            observed_state,
            lifecycle_status: FindingLifecycleStatus::Open,
            disposition: None,
            first_seen_at: at,
            last_seen_at: at,
            occurrence_count: 0,
        })
    }

    pub fn id(&self) -> HealthFindingId {
        self.id
    }

    pub fn invariant_id(&self) -> &str {
        &self.invariant_id
    }

    pub fn invariant_version(&self) -> &str {
        &self.invariant_version
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn scope(&self) -> &HealthScope {
        &self.scope
    }

    pub fn severity(&self) -> HealthSeverity {
        self.severity
    }

    pub fn repair_risk(&self) -> RepairRisk {
        self.repair_risk
    }

    pub fn repairability(&self) -> Repairability {
        self.repairability
    }

    pub fn observed_state(&self) -> HealthState {
        self.observed_state
    }

    /// The status as stored, without regard to whether a silencing disposition
    /// has since lapsed. Prefer [`Self::lifecycle_status_at`] when deciding
    /// whether a finding is visible now.
    pub fn lifecycle_status(&self) -> FindingLifecycleStatus {
        self.lifecycle_status
    }

    /// The status as it stands at `at`.
    ///
    /// A suppression or accepted risk that has passed its expiry stops
    /// silencing the finding, which returns to `Open`. Reading the stored field
    /// alone would report a decision nobody is still making: the expiry was
    /// recorded and never consulted, so "until Friday" and "for ever" were the
    /// same value.
    pub fn lifecycle_status_at(&self, at: Timestamp) -> FindingLifecycleStatus {
        match &self.disposition {
            Some(disposition) if disposition.silences() && disposition.has_lapsed(at) => {
                FindingLifecycleStatus::Open
            }
            _ => self.lifecycle_status,
        }
    }

    pub fn disposition(&self) -> Option<&FindingDisposition> {
        self.disposition.as_ref()
    }

    /// The disposition if it still applies at `at`.
    ///
    /// A lapsed one is returned by neither this nor [`Self::lifecycle_status_at`],
    /// while [`Self::disposition`] still reports it — the decision was made and
    /// belongs in the record even once it has stopped taking effect.
    pub fn disposition_in_force_at(&self, at: Timestamp) -> Option<&FindingDisposition> {
        match &self.disposition {
            Some(disposition) if !disposition.has_lapsed(at) => Some(disposition),
            _ => None,
        }
    }

    pub fn occurrence_count(&self) -> u64 {
        self.occurrence_count
    }

    /// What supports the claim that the invariant was violated.
    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    /// When this problem was first observed, across every run that has seen it.
    pub fn first_seen_at(&self) -> Timestamp {
        self.first_seen_at
    }

    /// When it was last observed.
    pub fn last_seen_at(&self) -> Timestamp {
        self.last_seen_at
    }

    /// Record that the invariant now holds a different version than when the
    /// finding was opened.
    ///
    /// The version is not part of a finding's identity — splitting the history
    /// of a problem because somebody edited the check that watches it would lose
    /// exactly the continuity recurrence detection depends on — so a finding
    /// carries the version that last observed it and keeps everything else.
    pub fn observed_by_version(&mut self, version: impl Into<String>) {
        self.invariant_version = version.into();
    }

    pub fn record_occurrence(
        &mut self,
        observed_state: HealthState,
        evidence_refs: Vec<String>,
        at: Timestamp,
    ) -> Result<HealthOccurrence, DomainError> {
        if evidence_refs.is_empty()
            || evidence_refs
                .iter()
                .any(|reference| reference.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "health occurrence requires non-empty evidence references".into(),
            ));
        }
        self.last_seen_at = at;
        self.observed_state = observed_state;
        self.occurrence_count += 1;
        if self.lifecycle_status == FindingLifecycleStatus::Resolved {
            self.lifecycle_status = FindingLifecycleStatus::Reopened;
        }
        // Observing the problem again after its silence has expired is the
        // moment the finding comes back into view. Leaving the stored status at
        // `Suppressed` here would mean the lapse only ever showed up in a read
        // that remembered to ask for it.
        if self
            .disposition
            .as_ref()
            .is_some_and(|disposition| disposition.silences() && disposition.has_lapsed(at))
        {
            self.lifecycle_status = FindingLifecycleStatus::Open;
        }
        Ok(HealthOccurrence {
            id: HealthOccurrenceId::new(),
            finding_id: self.id,
            observed_state,
            evidence_refs,
            observed_at: at,
            sequence: self.occurrence_count,
        })
    }

    pub fn set_disposition(&mut self, disposition: FindingDisposition) -> Result<(), DomainError> {
        disposition.validate()?;
        self.lifecycle_status = match disposition {
            FindingDisposition::Suppressed { .. } => FindingLifecycleStatus::Suppressed,
            FindingDisposition::AcceptedRisk { .. } => FindingLifecycleStatus::AcceptedRisk,
            _ => FindingLifecycleStatus::Open,
        };
        self.disposition = Some(disposition);
        Ok(())
    }

    pub fn apply_verification(&self, verification: &VerificationRun) -> Result<Self, DomainError> {
        if !verification.finding_ids.contains(&self.id) {
            return Err(DomainError::InvalidArgument(
                "verification run does not cover the finding".into(),
            ));
        }
        let mut next = self.clone();
        match verification.result {
            VerificationResult::Passed => next.lifecycle_status = FindingLifecycleStatus::Resolved,
            VerificationResult::Failed | VerificationResult::Inconclusive => {
                next.lifecycle_status = FindingLifecycleStatus::Reopened
            }
        }
        Ok(next)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthOccurrence {
    id: HealthOccurrenceId,
    finding_id: HealthFindingId,
    observed_state: HealthState,
    evidence_refs: Vec<String>,
    observed_at: Timestamp,
    sequence: u64,
}

impl HealthOccurrence {
    pub fn id(&self) -> HealthOccurrenceId {
        self.id
    }

    pub fn finding_id(&self) -> HealthFindingId {
        self.finding_id
    }

    pub fn observed_state(&self) -> HealthState {
        self.observed_state
    }

    pub fn observed_at(&self) -> Timestamp {
        self.observed_at
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthProjection {
    state: HealthProjectionState,
    target_invariant_id: String,
    contributing_invariants: Vec<String>,
}

impl HealthProjection {
    pub fn state(&self) -> HealthProjectionState {
        self.state
    }

    pub fn target_invariant_id(&self) -> &str {
        &self.target_invariant_id
    }

    pub fn contributing_invariants(&self) -> &[String] {
        &self.contributing_invariants
    }
}

pub fn project_health(
    target_invariant_id: &str,
    findings: &[HealthFinding],
    dependencies: &HashMap<String, Vec<String>>,
) -> HealthProjection {
    let mut contributing_invariants = Vec::new();
    let state = project_node(
        target_invariant_id,
        findings,
        dependencies,
        &mut Vec::new(),
        &mut contributing_invariants,
    );
    HealthProjection {
        state,
        target_invariant_id: target_invariant_id.to_owned(),
        contributing_invariants,
    }
}

fn project_node(
    target: &str,
    findings: &[HealthFinding],
    dependencies: &HashMap<String, Vec<String>>,
    path: &mut Vec<String>,
    contributing_invariants: &mut Vec<String>,
) -> HealthProjectionState {
    if path.iter().any(|item| item == target) {
        return HealthProjectionState::Unknown;
    }
    path.push(target.to_owned());

    let active = findings
        .iter()
        .filter(|finding| {
            finding.invariant_id() == target
                && finding.lifecycle_status() != FindingLifecycleStatus::Resolved
        })
        .map(|finding| {
            if !contributing_invariants.iter().any(|item| item == target) {
                contributing_invariants.push(target.to_owned());
            }
            HealthProjectionState::from(finding.observed_state())
        })
        .collect::<Vec<_>>();
    let result = if active.is_empty() {
        let child_states = dependencies
            .get(target)
            .into_iter()
            .flatten()
            .map(|dependency| {
                project_node(
                    dependency,
                    findings,
                    dependencies,
                    path,
                    contributing_invariants,
                )
            })
            .collect::<Vec<_>>();
        combine_projection_states(&child_states)
    } else {
        combine_projection_states(&active)
    };
    path.pop();
    result
}

fn combine_projection_states(states: &[HealthProjectionState]) -> HealthProjectionState {
    if states.is_empty() {
        return HealthProjectionState::Unknown;
    }
    if states.contains(&HealthProjectionState::Unhealthy) {
        HealthProjectionState::Unhealthy
    } else if states.contains(&HealthProjectionState::Degraded) {
        HealthProjectionState::Degraded
    } else if states.contains(&HealthProjectionState::Unknown) {
        HealthProjectionState::Unknown
    } else {
        HealthProjectionState::Healthy
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    Restartable,
    Reversible,
    Irreversible,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairPlan {
    id: RepairPlanId,
    finding_ids: Vec<HealthFindingId>,
    input_state_ref: String,
    scope: HealthScope,
    preconditions: Vec<String>,
    operations: Vec<String>,
    expected_postconditions: Vec<String>,
    verification_checks: Vec<String>,
    risk: RepairRisk,
    reversibility: Reversibility,
    required_capability: Capability,
    created_at: Timestamp,
    expires_at: Option<Timestamp>,
}

impl RepairPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        finding_ids: Vec<HealthFindingId>,
        input_state_ref: impl Into<String>,
        scope: HealthScope,
        preconditions: Vec<String>,
        operations: Vec<String>,
        expected_postconditions: Vec<String>,
        verification_checks: Vec<String>,
        risk: RepairRisk,
        reversibility: Reversibility,
        required_capability: Capability,
        created_at: Timestamp,
        expires_at: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        let input_state_ref = input_state_ref.into();
        scope.validate()?;
        if finding_ids.is_empty()
            || input_state_ref.trim().is_empty()
            || preconditions.is_empty()
            || operations.is_empty()
            || expected_postconditions.is_empty()
            || verification_checks.is_empty()
        {
            return Err(DomainError::InvalidArgument(
                "repair plan requires findings, state, preconditions, operations, postconditions and verification"
                    .into(),
            ));
        }
        if expires_at.is_some_and(|expires_at| expires_at < created_at) {
            return Err(DomainError::InvalidArgument(
                "repair plan expiry must not precede creation".into(),
            ));
        }
        Ok(Self {
            id: RepairPlanId::new(),
            finding_ids,
            input_state_ref,
            scope,
            preconditions,
            operations,
            expected_postconditions,
            verification_checks,
            risk,
            reversibility,
            required_capability,
            created_at,
            expires_at,
        })
    }

    pub fn id(&self) -> RepairPlanId {
        self.id
    }

    pub fn finding_ids(&self) -> &[HealthFindingId] {
        &self.finding_ids
    }

    pub fn input_state_ref(&self) -> &str {
        &self.input_state_ref
    }

    pub fn scope(&self) -> &HealthScope {
        &self.scope
    }

    pub fn required_capability(&self) -> Capability {
        self.required_capability.clone()
    }

    pub fn is_expired(&self, at: Timestamp) -> bool {
        self.expires_at.is_some_and(|expires_at| at > expires_at)
    }

    pub fn start_execution(
        &self,
        authorization: &RepairAuthorization,
        current_state_ref: &str,
        at: Timestamp,
    ) -> Result<RepairExecution, RepairExecutionError> {
        if !authorization.is_allowed() || authorization.capability() != self.required_capability {
            return Err(RepairExecutionError::Unauthorized);
        }
        if self.is_expired(at) {
            return Err(RepairExecutionError::ExpiredPlan);
        }
        if current_state_ref != self.input_state_ref {
            return Err(RepairExecutionError::StalePlan);
        }
        Ok(RepairExecution {
            id: RepairExecutionId::new(),
            plan_id: self.id,
            result: None,
            started_at: at,
            completed_at: None,
        })
    }

    pub fn preconditions(&self) -> &[String] {
        &self.preconditions
    }

    pub fn operations(&self) -> &[String] {
        &self.operations
    }

    pub fn expected_postconditions(&self) -> &[String] {
        &self.expected_postconditions
    }

    pub fn verification_checks(&self) -> &[String] {
        &self.verification_checks
    }

    pub fn risk(&self) -> &RepairRisk {
        &self.risk
    }

    pub fn reversibility(&self) -> &Reversibility {
        &self.reversibility
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn expires_at(&self) -> Option<Timestamp> {
        self.expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairAuthorization {
    allowed: bool,
    capability: Capability,
    policy_version: String,
    evidence_ref: String,
    reason: String,
}

impl RepairAuthorization {
    pub fn allow(
        capability: Capability,
        policy_version: impl Into<String>,
        evidence_ref: impl Into<String>,
    ) -> Self {
        Self {
            allowed: true,
            capability,
            policy_version: policy_version.into(),
            evidence_ref: evidence_ref.into(),
            reason: "allowed".into(),
        }
    }

    pub fn deny(
        capability: Capability,
        policy_version: impl Into<String>,
        evidence_ref: impl Into<String>,
    ) -> Self {
        Self {
            allowed: false,
            capability,
            policy_version: policy_version.into(),
            evidence_ref: evidence_ref.into(),
            reason: "denied".into(),
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn capability(&self) -> Capability {
        self.capability.clone()
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn evidence_ref(&self) -> &str {
        &self.evidence_ref
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairExecutionResult {
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairExecutionError {
    Unauthorized,
    StalePlan,
    ExpiredPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairExecution {
    id: RepairExecutionId,
    plan_id: RepairPlanId,
    result: Option<RepairExecutionResult>,
    started_at: Timestamp,
    completed_at: Option<Timestamp>,
}

impl RepairExecution {
    pub fn id(&self) -> RepairExecutionId {
        self.id
    }

    pub fn plan_id(&self) -> RepairPlanId {
        self.plan_id
    }

    pub fn result(&self) -> Option<RepairExecutionResult> {
        self.result
    }

    pub fn started_at(&self) -> Timestamp {
        self.started_at
    }

    pub fn completed_at(&self) -> Option<Timestamp> {
        self.completed_at
    }

    /// Record how the repair ended.
    ///
    /// # Why this refuses a second verdict
    ///
    /// It used to overwrite whatever was there, so
    /// `execution.complete(Failed, t1).complete(Succeeded, t2)` left a record
    /// saying the repair succeeded. An execution record exists to be evidence
    /// about a change to production state; one whose verdict can be rewritten
    /// after the fact is evidence of nothing, and the rewrite is invisible
    /// because only the final value is stored.
    ///
    /// A repeat of the *same* verdict is accepted, so a retried write is not an
    /// error — only a contradiction is.
    pub fn complete(
        mut self,
        result: RepairExecutionResult,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if let Some(existing) = self.result {
            if existing != result {
                return Err(DomainError::PolicyViolation(format!(
                    "repair execution already completed as {existing:?} and cannot be \
                     recorded as {result:?}"
                )));
            }
            return Ok(self);
        }
        self.result = Some(result);
        self.completed_at = Some(at);
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult {
    Passed,
    Failed,
    Inconclusive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerificationRun {
    id: VerificationRunId,
    plan_id: RepairPlanId,
    finding_ids: Vec<HealthFindingId>,
    result: VerificationResult,
    checks: Vec<String>,
    evidence_refs: Vec<String>,
    completed_at: Timestamp,
}

impl VerificationRun {
    pub fn new(
        plan_id: RepairPlanId,
        finding_ids: Vec<HealthFindingId>,
        result: VerificationResult,
        checks: Vec<String>,
        evidence_refs: Vec<String>,
        completed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if finding_ids.is_empty() || checks.is_empty() || evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "verification run requires findings, checks and evidence".into(),
            ));
        }
        Ok(Self {
            id: VerificationRunId::new(),
            plan_id,
            finding_ids,
            result,
            checks,
            evidence_refs,
            completed_at,
        })
    }

    pub fn id(&self) -> VerificationRunId {
        self.id
    }

    pub fn plan_id(&self) -> RepairPlanId {
        self.plan_id
    }

    pub fn result(&self) -> VerificationResult {
        self.result
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn finding_ids(&self) -> &[HealthFindingId] {
        &self.finding_ids
    }

    pub fn checks(&self) -> &[String] {
        &self.checks
    }

    pub fn completed_at(&self) -> Timestamp {
        self.completed_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairBudget {
    max_attempts: usize,
    window: Duration,
    cooldown: Duration,
    attempts: Vec<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairBudgetError {
    CooldownActive { until: Timestamp },
    AttemptBudgetExceeded,
}

impl RepairBudget {
    pub fn new(max_attempts: usize, window: Duration, cooldown: Duration) -> Self {
        Self {
            max_attempts,
            window,
            cooldown,
            attempts: Vec::new(),
        }
    }

    pub fn reserve(&mut self, at: Timestamp) -> Result<(), RepairBudgetError> {
        self.attempts.retain(|attempt| *attempt >= at - self.window);
        if let Some(last_attempt) = self.attempts.last().copied() {
            let until = last_attempt + self.cooldown;
            if at < until {
                return Err(RepairBudgetError::CooldownActive { until });
            }
        }
        if self.attempts.len() >= self.max_attempts {
            return Err(RepairBudgetError::AttemptBudgetExceeded);
        }
        self.attempts.push(at);
        Ok(())
    }

    pub fn attempts_in_window(&self) -> usize {
        self.attempts.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurrenceAssessment {
    occurrence_count: usize,
    state_transitions: usize,
    recurrent: bool,
    flapping: bool,
}

impl RecurrenceAssessment {
    pub fn occurrence_count(&self) -> usize {
        self.occurrence_count
    }

    pub fn state_transitions(&self) -> usize {
        self.state_transitions
    }

    pub fn is_recurrent(&self) -> bool {
        self.recurrent
    }

    pub fn is_flapping(&self) -> bool {
        self.flapping
    }
}

pub fn assess_recurrence(
    occurrences: &[HealthOccurrence],
    flap_threshold: usize,
) -> RecurrenceAssessment {
    let mut sorted = occurrences.to_vec();
    sorted.sort_by_key(HealthOccurrence::sequence);
    let state_transitions = sorted
        .windows(2)
        .filter(|window| window[0].observed_state != window[1].observed_state)
        .count();
    RecurrenceAssessment {
        occurrence_count: sorted.len(),
        state_transitions,
        recurrent: sorted.len() > 1,
        flapping: sorted.len() >= flap_threshold && state_transitions >= 2,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairOperatorPhase {
    Inspect,
    Plan,
    Repair,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairOperatorCommand {
    Inspect,
    Plan {
        finding_ids: Vec<HealthFindingId>,
    },
    Repair {
        plan_id: RepairPlanId,
        current_state_ref: String,
        authorization: RepairAuthorization,
    },
}

impl RepairOperatorCommand {
    pub fn inspect() -> Self {
        Self::Inspect
    }

    pub fn plan(finding_ids: Vec<HealthFindingId>) -> Self {
        Self::Plan { finding_ids }
    }

    pub fn repair(
        plan_id: RepairPlanId,
        current_state_ref: impl Into<String>,
        authorization: RepairAuthorization,
    ) -> Result<Self, DomainError> {
        let current_state_ref = current_state_ref.into();
        if current_state_ref.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "repair command requires current state reference".into(),
            ));
        }
        Ok(Self::Repair {
            plan_id,
            current_state_ref,
            authorization,
        })
    }

    pub fn phase(&self) -> RepairOperatorPhase {
        match self {
            Self::Inspect => RepairOperatorPhase::Inspect,
            Self::Plan { .. } => RepairOperatorPhase::Plan,
            Self::Repair { .. } => RepairOperatorPhase::Repair,
        }
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Inspect | Self::Plan { .. })
    }

    pub fn authorization(&self) -> Option<&RepairAuthorization> {
        match self {
            Self::Repair { authorization, .. } => Some(authorization),
            _ => None,
        }
    }

    pub fn plan_id(&self) -> Option<RepairPlanId> {
        match self {
            Self::Repair { plan_id, .. } => Some(*plan_id),
            _ => None,
        }
    }
}

pub struct RepairOperatorContract;

impl RepairOperatorContract {
    pub fn validate(command: &RepairOperatorCommand) -> Result<(), DomainError> {
        match command {
            RepairOperatorCommand::Inspect => Ok(()),
            RepairOperatorCommand::Plan { finding_ids } if finding_ids.is_empty() => Err(
                DomainError::InvalidArgument("plan command requires finding ids".into()),
            ),
            RepairOperatorCommand::Plan { .. } => Ok(()),
            RepairOperatorCommand::Repair {
                current_state_ref,
                authorization,
                ..
            } if current_state_ref.trim().is_empty() || !authorization.is_allowed() => {
                Err(DomainError::PolicyViolation(
                    "repair command requires an authorized immutable plan".into(),
                ))
            }
            RepairOperatorCommand::Repair { .. } => Ok(()),
        }
    }
}
