use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::Duration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::conformance::gate::{GovernanceFederationGate, HardGateEvidence, HardGateFailure};
use crate::conformance::runner::profile_requirements;
use crate::conformance::{ConformanceReport, QualificationProfile, RequirementId};
use crate::release::VestraceCapabilityManifest;
use crate::{
    Capability, DataDestination, DomainError, HealthFindingId, HealthScope, IncidentId,
    PrincipalId, RevalidationRunId, Timestamp, WorkspaceId,
};

fn required_text(field: &str, value: impl Into<String>) -> Result<String, DomainError> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(DomainError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn required_refs(field: &str, values: &[String]) -> Result<(), DomainError> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(DomainError::InvalidArgument(format!(
            "{field} requires non-empty references"
        )));
    }
    Ok(())
}

fn sha256_digest(parts: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentType {
    CriticalInvariantFailure,
    MultiDomainImpact,
    RecoveryRequired,
    UnknownExternalEffect,
    TrustBoundaryViolation,
    CryptoIntegrityFailure,
    RepairFailure,
    BroadCorruption,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Containing,
    Contained,
    Recovering,
    Revalidating,
    Resolved,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentState {
    None,
    Active,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryState {
    NotStarted,
    InProgress,
    Recovered,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationState {
    NotStarted,
    InProgress,
    Passed,
    Failed,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    Trusted,
    DegradedTrust,
    Untrusted,
    Revalidating,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentActionKind {
    FreezeWrites,
    SuspendCapabilities,
    IsolateScope,
    PauseWorkers,
    StopExternalEffects,
    RevokeLeases,
    QuarantineResources,
    ForceReadOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainmentAction {
    kind: ContainmentActionKind,
    scope_ref: String,
    actor_id: PrincipalId,
    reason: String,
    policy_version: String,
    at: Timestamp,
}

impl ContainmentAction {
    pub fn new(
        kind: ContainmentActionKind,
        scope_ref: impl Into<String>,
        actor_id: PrincipalId,
        reason: impl Into<String>,
        policy_version: impl Into<String>,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            kind,
            scope_ref: required_text("containment scope", scope_ref)?,
            actor_id,
            reason: required_text("containment reason", reason)?,
            policy_version: required_text("containment policy version", policy_version)?,
            at,
        })
    }

    pub fn kind(&self) -> ContainmentActionKind {
        self.kind
    }

    pub fn scope_ref(&self) -> &str {
        &self.scope_ref
    }

    pub fn actor_id(&self) -> PrincipalId {
        self.actor_id
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn at(&self) -> Timestamp {
        self.at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    id: IncidentId,
    incident_type: IncidentType,
    severity: IncidentSeverity,
    scope: HealthScope,
    triggering_findings: Vec<HealthFindingId>,
    affected_resources: Vec<String>,
    triggering_evidence: Vec<String>,
    opened_at: Timestamp,
    status: IncidentStatus,
    containment_state: ContainmentState,
    recovery_state: RecoveryState,
    revalidation_state: RevalidationState,
    containment_actions: Vec<ContainmentAction>,
    revalidation_run_id: Option<crate::RevalidationRunId>,
    disposition: Option<String>,
    closure_evidence: Vec<String>,
    closed_by: Option<PrincipalId>,
    closed_at: Option<Timestamp>,
}

impl Incident {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        incident_type: IncidentType,
        severity: IncidentSeverity,
        scope: HealthScope,
        triggering_findings: Vec<HealthFindingId>,
        affected_resources: Vec<String>,
        triggering_evidence: Vec<String>,
        opened_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if triggering_findings.is_empty() && affected_resources.is_empty() {
            return Err(DomainError::InvalidArgument(
                "incident requires a finding or affected resource".into(),
            ));
        }
        required_refs("incident evidence", &triggering_evidence)?;
        if affected_resources
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "incident affected resources must not be empty".into(),
            ));
        }
        Ok(Self {
            id: crate::IncidentId::new(),
            incident_type,
            severity,
            scope,
            triggering_findings,
            affected_resources,
            triggering_evidence,
            opened_at,
            status: IncidentStatus::Open,
            containment_state: ContainmentState::None,
            recovery_state: RecoveryState::NotStarted,
            revalidation_state: RevalidationState::NotStarted,
            containment_actions: Vec::new(),
            revalidation_run_id: None,
            disposition: None,
            closure_evidence: Vec::new(),
            closed_by: None,
            closed_at: None,
        })
    }

    pub fn id(&self) -> crate::IncidentId {
        self.id
    }

    pub fn incident_type(&self) -> IncidentType {
        self.incident_type
    }

    pub fn severity(&self) -> IncidentSeverity {
        self.severity
    }

    pub fn scope(&self) -> &HealthScope {
        &self.scope
    }

    pub fn status(&self) -> IncidentStatus {
        self.status
    }

    pub fn containment_state(&self) -> ContainmentState {
        self.containment_state
    }

    pub fn recovery_state(&self) -> RecoveryState {
        self.recovery_state
    }

    pub fn revalidation_state(&self) -> RevalidationState {
        self.revalidation_state
    }

    /// The findings this incident was opened from.
    ///
    /// Stored since the type was written and unreadable until now, which is the
    /// same defect the export plan had: a field that exists, is validated on the
    /// way in, and can never be looked at. REC-001 requires a finding and an
    /// incident to stay distinct concepts, and that cannot be checked by
    /// anything that cannot see the link between them.
    pub fn triggering_findings(&self) -> &[HealthFindingId] {
        &self.triggering_findings
    }

    pub fn affected_resources(&self) -> &[String] {
        &self.affected_resources
    }

    /// How the incident was disposed of, and by whom. Written by `close` and,
    /// until now, readable by nobody — so "recovery must not rewrite history"
    /// was a requirement about a record that could not be inspected.
    pub fn disposition(&self) -> Option<&str> {
        self.disposition.as_deref()
    }

    pub fn closed_by(&self) -> Option<PrincipalId> {
        self.closed_by
    }

    pub fn closed_at(&self) -> Option<Timestamp> {
        self.closed_at
    }

    pub fn triggering_evidence(&self) -> &[String] {
        &self.triggering_evidence
    }

    pub fn opened_at(&self) -> Timestamp {
        self.opened_at
    }

    pub fn revalidation_run_id(&self) -> Option<crate::RevalidationRunId> {
        self.revalidation_run_id
    }

    pub fn containment_actions(&self) -> &[ContainmentAction] {
        &self.containment_actions
    }

    pub fn closure_evidence(&self) -> &[String] {
        &self.closure_evidence
    }

    pub fn begin_containment(&mut self, action: ContainmentAction) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Open {
            return Err(DomainError::PolicyViolation(
                "incident containment requires an open incident".into(),
            ));
        }
        self.containment_actions.push(action);
        self.containment_state = ContainmentState::Active;
        self.status = IncidentStatus::Containing;
        Ok(())
    }

    pub fn mark_contained(&mut self, _at: Timestamp) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Containing
            || self.containment_state != ContainmentState::Active
        {
            return Err(DomainError::PolicyViolation(
                "incident must be actively containing before it is contained".into(),
            ));
        }
        self.status = IncidentStatus::Contained;
        Ok(())
    }

    pub fn begin_recovery(&mut self, _at: Timestamp) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Contained {
            return Err(DomainError::PolicyViolation(
                "recovery requires contained incident".into(),
            ));
        }
        self.status = IncidentStatus::Recovering;
        self.recovery_state = RecoveryState::InProgress;
        Ok(())
    }

    pub fn begin_revalidation(
        &mut self,
        run_id: crate::RevalidationRunId,
        _at: Timestamp,
    ) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Recovering {
            return Err(DomainError::PolicyViolation(
                "revalidation requires recovering incident".into(),
            ));
        }
        self.status = IncidentStatus::Revalidating;
        self.revalidation_state = RevalidationState::InProgress;
        self.revalidation_run_id = Some(run_id);
        Ok(())
    }

    pub fn apply_revalidation(&mut self, run: &RevalidationRun) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Revalidating
            || self.revalidation_run_id != Some(run.id())
            || self.scope != *run.scope()
        {
            return Err(DomainError::PolicyViolation(
                "incident revalidation run does not match lifecycle state".into(),
            ));
        }
        self.apply_revalidation_result(run.result());
        Ok(())
    }

    pub fn mark_resolved(&mut self, _at: Timestamp) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Revalidating
            || self.revalidation_state != RevalidationState::Passed
        {
            return Err(DomainError::PolicyViolation(
                "incident resolution requires passed revalidation".into(),
            ));
        }
        self.status = IncidentStatus::Resolved;
        self.recovery_state = RecoveryState::Recovered;
        Ok(())
    }

    pub fn close(
        &mut self,
        actor_id: PrincipalId,
        disposition: impl Into<String>,
        evidence_refs: Vec<String>,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        if self.status != IncidentStatus::Resolved {
            return Err(DomainError::PolicyViolation(
                "incident must be resolved before close".into(),
            ));
        }
        required_refs("incident closure evidence", &evidence_refs)?;
        self.disposition = Some(required_text("incident disposition", disposition)?);
        self.closure_evidence = evidence_refs;
        self.closed_by = Some(actor_id);
        self.closed_at = Some(at);
        self.status = IncidentStatus::Closed;
        Ok(())
    }

    fn apply_revalidation_result(&mut self, result: RevalidationResult) {
        self.revalidation_state = match result {
            RevalidationResult::Passed | RevalidationResult::PassedWithDegradation => {
                RevalidationState::Passed
            }
            RevalidationResult::Failed => RevalidationState::Failed,
            RevalidationResult::Inconclusive => RevalidationState::Inconclusive,
        };
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrustStateRecord {
    scope: HealthScope,
    state: TrustState,
    reason: String,
    incident_id: Option<IncidentId>,
    updated_at: Timestamp,
    revalidation_run_id: Option<crate::RevalidationRunId>,
}

impl TrustStateRecord {
    pub fn new(
        scope: HealthScope,
        state: TrustState,
        reason: impl Into<String>,
        incident_id: Option<IncidentId>,
        updated_at: Timestamp,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            scope,
            state,
            reason: required_text("trust state reason", reason)?,
            incident_id,
            updated_at,
            revalidation_run_id: None,
        })
    }

    pub fn from_incident(
        scope: HealthScope,
        incident_id: IncidentId,
        severity: IncidentSeverity,
        at: Timestamp,
    ) -> Self {
        let state = match severity {
            IncidentSeverity::Low | IncidentSeverity::Medium => TrustState::DegradedTrust,
            IncidentSeverity::High | IncidentSeverity::Critical => TrustState::Untrusted,
        };
        Self {
            scope,
            state,
            reason: "incident requires explicit trust restoration".into(),
            incident_id: Some(incident_id),
            updated_at: at,
            revalidation_run_id: None,
        }
    }

    pub fn scope(&self) -> &HealthScope {
        &self.scope
    }

    pub fn state(&self) -> TrustState {
        self.state
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn incident_id(&self) -> Option<IncidentId> {
        self.incident_id
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn revalidation_run_id(&self) -> Option<crate::RevalidationRunId> {
        self.revalidation_run_id
    }

    pub fn begin_revalidation(
        &mut self,
        run_id: crate::RevalidationRunId,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        if self.state == TrustState::Trusted {
            return Err(DomainError::PolicyViolation(
                "trusted scope does not need recovery revalidation".into(),
            ));
        }
        self.state = TrustState::Revalidating;
        self.revalidation_run_id = Some(run_id);
        self.updated_at = at;
        Ok(())
    }

    pub fn apply_revalidation(&mut self, run: &RevalidationRun) -> Result<(), DomainError> {
        if run.scope() != &self.scope {
            return Err(DomainError::PolicyViolation(
                "revalidation scope does not match trust scope".into(),
            ));
        }
        if self.revalidation_run_id != Some(run.id()) {
            return Err(DomainError::PolicyViolation(
                "trust restoration requires the registered revalidation run".into(),
            ));
        }
        self.state = match run.result() {
            RevalidationResult::Passed => TrustState::Trusted,
            RevalidationResult::PassedWithDegradation => TrustState::DegradedTrust,
            RevalidationResult::Failed => TrustState::Untrusted,
            RevalidationResult::Inconclusive => TrustState::Revalidating,
        };
        self.reason = format!("revalidation result: {:?}", run.result());
        self.updated_at = run.completed_at();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryTarget {
    RunningExecution,
    DispatchingExternalEffect,
    VerifyingRepair,
    StaleLease,
    UnfinishedWorkflow,
    OrphanTemporaryState,
    UnknownOutcome,
    DivergentHistory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Resume,
    Retry,
    Reconcile,
    Abort,
    HumanReview,
}

impl RecoveryTarget {
    pub fn required_targets() -> [Self; 8] {
        [
            Self::RunningExecution,
            Self::DispatchingExternalEffect,
            Self::VerifyingRepair,
            Self::StaleLease,
            Self::UnfinishedWorkflow,
            Self::OrphanTemporaryState,
            Self::UnknownOutcome,
            Self::DivergentHistory,
        ]
    }
}

impl RecoveryClassification {
    fn expected_action(self) -> RecoveryAction {
        match self {
            Self::SafeToResume => RecoveryAction::Resume,
            Self::SafeToRetry => RecoveryAction::Retry,
            Self::MustReconcile => RecoveryAction::Reconcile,
            Self::MustAbort => RecoveryAction::Abort,
            Self::HumanRequired => RecoveryAction::HumanReview,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryQualificationObservation {
    pub target: RecoveryTarget,
    pub classification: RecoveryClassification,
    pub action: RecoveryAction,
    pub evidence_ref: String,
}

impl RecoveryQualificationObservation {
    pub fn new(
        target: RecoveryTarget,
        classification: RecoveryClassification,
        action: RecoveryAction,
        evidence_ref: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            target,
            classification,
            action,
            evidence_ref: required_text("recovery qualification evidence", evidence_ref)?,
        })
    }

    pub fn expected(target: RecoveryTarget, evidence_ref: impl Into<String>) -> Self {
        let classification = classify_recovery(target);
        Self::new(
            target,
            classification,
            classification.expected_action(),
            evidence_ref,
        )
        .expect("expected recovery qualification observation is valid")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForensicEvidenceSnapshot {
    pub target_ref: String,
    pub capture_profile: crate::state_engine::CaptureProfile,
    pub evidence_ref: String,
    pub captured_at: Timestamp,
}

impl ForensicEvidenceSnapshot {
    pub fn new(
        target_ref: impl Into<String>,
        capture_profile: crate::state_engine::CaptureProfile,
        evidence_ref: impl Into<String>,
        captured_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let target_ref = required_text("forensic snapshot target reference", target_ref)?;
        let evidence_ref = required_text("forensic snapshot evidence reference", evidence_ref)?;
        if capture_profile != crate::state_engine::CaptureProfile::Forensic {
            return Err(DomainError::PolicyViolation(
                "forensic evidence snapshot requires CaptureProfile::Forensic".into(),
            ));
        }
        Ok(Self {
            target_ref,
            capture_profile,
            evidence_ref,
            captured_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DestructiveRecoveryExecution {
    target_state_ref: String,
    target: RecoveryTarget,
    forensic_snapshot: Option<ForensicEvidenceSnapshot>,
    executed_at: Timestamp,
}

impl DestructiveRecoveryExecution {
    pub fn new(
        target_state_ref: impl Into<String>,
        target: RecoveryTarget,
        forensic_snapshot: Option<ForensicEvidenceSnapshot>,
        executed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let target_state_ref = required_text(
            "destructive recovery target state reference",
            target_state_ref,
        )?;
        let execution = Self {
            target_state_ref,
            target,
            forensic_snapshot,
            executed_at,
        };
        execution.validate()?;
        Ok(execution)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        let snapshot = self.forensic_snapshot.as_ref().ok_or_else(|| {
            DomainError::PolicyViolation(
                "destructive recovery requires preserving forensic evidence before execution"
                    .into(),
            )
        })?;
        if snapshot.target_ref != self.target_state_ref {
            return Err(DomainError::PolicyViolation(
                "forensic evidence snapshot target does not match destructive recovery target state"
                    .into(),
            ));
        }
        if snapshot.capture_profile != crate::state_engine::CaptureProfile::Forensic {
            return Err(DomainError::PolicyViolation(
                "destructive recovery requires a forensic profile capture".into(),
            ));
        }
        Ok(())
    }

    pub fn target_state_ref(&self) -> &str {
        &self.target_state_ref
    }

    pub fn target(&self) -> RecoveryTarget {
        self.target
    }

    pub fn forensic_snapshot(&self) -> &ForensicEvidenceSnapshot {
        self.forensic_snapshot
            .as_ref()
            .expect("validated forensic snapshot")
    }

    pub fn executed_at(&self) -> Timestamp {
        self.executed_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryQualificationDecision {
    failures: Vec<String>,
}

impl RecoveryQualificationDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }
}

pub fn evaluate_recovery_qualification(
    observations: &[RecoveryQualificationObservation],
) -> RecoveryQualificationDecision {
    let mut failures = Vec::new();
    for target in RecoveryTarget::required_targets() {
        let matches = observations
            .iter()
            .filter(|observation| observation.target == target)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            failures.push(format!(
                "recovery target {target:?} must have exactly one observation"
            ));
            continue;
        }

        let observation = matches[0];
        let classification = classify_recovery(target);
        if observation.classification != classification {
            failures.push(format!(
                "recovery target {target:?} has incorrect classification"
            ));
        }
        if observation.action != classification.expected_action() {
            failures.push(format!("recovery target {target:?} has unsafe action"));
        }
        if observation.evidence_ref.trim().is_empty() {
            failures.push(format!(
                "recovery target {target:?} has no evidence reference"
            ));
        }
    }
    RecoveryQualificationDecision { failures }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClassification {
    SafeToResume,
    SafeToRetry,
    MustReconcile,
    MustAbort,
    HumanRequired,
}

pub fn classify_recovery(target: RecoveryTarget) -> RecoveryClassification {
    match target {
        RecoveryTarget::RunningExecution | RecoveryTarget::UnfinishedWorkflow => {
            RecoveryClassification::SafeToResume
        }
        RecoveryTarget::StaleLease | RecoveryTarget::OrphanTemporaryState => {
            RecoveryClassification::SafeToRetry
        }
        RecoveryTarget::DispatchingExternalEffect | RecoveryTarget::UnknownOutcome => {
            RecoveryClassification::MustReconcile
        }
        RecoveryTarget::VerifyingRepair => RecoveryClassification::MustAbort,
        RecoveryTarget::DivergentHistory => RecoveryClassification::HumanRequired,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityStatus {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPoint {
    id: crate::RecoveryPointId,
    state_ref: String,
    sequence_position: u64,
    consistency_scope: String,
    integrity_status: IntegrityStatus,
    provenance: String,
    created_at: Timestamp,
}

impl RecoveryPoint {
    pub fn new(
        state_ref: impl Into<String>,
        sequence_position: u64,
        consistency_scope: impl Into<String>,
        integrity_status: IntegrityStatus,
        provenance: impl Into<String>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: crate::RecoveryPointId::new(),
            state_ref: required_text("recovery point state reference", state_ref)?,
            sequence_position,
            consistency_scope: required_text(
                "recovery point consistency scope",
                consistency_scope,
            )?,
            integrity_status,
            provenance: required_text("recovery point provenance", provenance)?,
            created_at,
        })
    }

    pub fn id(&self) -> crate::RecoveryPointId {
        self.id
    }

    pub fn can_restore(&self) -> bool {
        self.integrity_status == IntegrityStatus::Valid
    }

    /// The state this point refers to, where in the sequence it sits, and what
    /// it is consistent across.
    ///
    /// REC-007 requires a recovery point to reference a *provably consistent*
    /// position. All three of the fields that say which position it is were
    /// unreadable, so the only observable thing about a recovery point was
    /// whether it claimed to be valid.
    pub fn state_ref(&self) -> &str {
        &self.state_ref
    }

    pub fn sequence_position(&self) -> u64 {
        self.sequence_position
    }

    pub fn consistency_scope(&self) -> &str {
        &self.consistency_scope
    }

    /// Where this point came from — which is what makes it evidence rather than
    /// an assertion.
    pub fn recovery_provenance(&self) -> &str {
        &self.provenance
    }

    pub fn integrity_status(&self) -> IntegrityStatus {
        self.integrity_status
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationLevel {
    Local,
    Domain,
    Workspace,
    Installation,
    FullTrust,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationResult {
    Passed,
    PassedWithDegradation,
    Failed,
    Inconclusive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RevalidationCheck {
    name: String,
    passed: bool,
    evidence_refs: Vec<String>,
}

impl RevalidationCheck {
    pub fn new(
        name: impl Into<String>,
        passed: bool,
        evidence_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            name: required_text("revalidation check name", name)?,
            passed,
            evidence_refs: {
                required_refs("revalidation check evidence", &evidence_refs)?;
                evidence_refs
            },
        })
    }

    pub fn passing(name: impl Into<String>, evidence_refs: Vec<String>) -> Self {
        Self::new(name, true, evidence_refs).expect("passing revalidation check is valid")
    }

    pub fn passed(&self) -> bool {
        self.passed
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RevalidationRun {
    id: crate::RevalidationRunId,
    incident_id: Option<IncidentId>,
    scope: HealthScope,
    level: RevalidationLevel,
    baseline_state_ref: String,
    checks: Vec<RevalidationCheck>,
    evidence_refs: Vec<String>,
    result: RevalidationResult,
    completed_at: Timestamp,
}

impl RevalidationRun {
    #[allow(clippy::too_many_arguments)]
    pub fn complete(
        incident_id: Option<IncidentId>,
        scope: HealthScope,
        level: RevalidationLevel,
        baseline_state_ref: impl Into<String>,
        checks: Vec<RevalidationCheck>,
        evidence_refs: Vec<String>,
        result: RevalidationResult,
        completed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if checks.is_empty() {
            return Err(DomainError::InvalidArgument(
                "revalidation requires checks".into(),
            ));
        }
        required_refs("revalidation evidence", &evidence_refs)?;
        Ok(Self {
            id: crate::RevalidationRunId::new(),
            incident_id,
            scope,
            level,
            baseline_state_ref: required_text("revalidation baseline state", baseline_state_ref)?,
            checks,
            evidence_refs,
            result,
            completed_at,
        })
    }

    pub fn id(&self) -> crate::RevalidationRunId {
        self.id
    }

    pub fn scope(&self) -> &HealthScope {
        &self.scope
    }

    pub fn level(&self) -> RevalidationLevel {
        self.level
    }

    pub fn result(&self) -> RevalidationResult {
        self.result
    }

    pub fn completed_at(&self) -> Timestamp {
        self.completed_at
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    /// The checks the run performed.
    ///
    /// REC-012 requires a revalidation run to preserve its scope, level,
    /// checks, evidence and result. Four of the five were readable; `checks`
    /// was validated as non-empty on construction and then invisible, so the
    /// requirement was unverifiable in the one place it is about.
    pub fn checks(&self) -> &[RevalidationCheck] {
        &self.checks
    }

    /// The state the run measured against — what "revalidated" was relative to.
    pub fn baseline_state_ref(&self) -> &str {
        &self.baseline_state_ref
    }

    pub fn incident_id(&self) -> Option<IncidentId> {
        self.incident_id
    }

    pub fn is_successful(&self) -> bool {
        matches!(
            self.result,
            RevalidationResult::Passed | RevalidationResult::PassedWithDegradation
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PostIncidentQualificationEvidence {
    incident_id: IncidentId,
    revalidation_run_id: RevalidationRunId,
    revalidation_result: RevalidationResult,
    evidence_refs: Vec<String>,
}

impl PostIncidentQualificationEvidence {
    pub fn new(
        incident_id: IncidentId,
        revalidation_run_id: RevalidationRunId,
        revalidation_result: RevalidationResult,
        evidence_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        required_refs("post-incident qualification evidence", &evidence_refs)?;
        Ok(Self {
            incident_id,
            revalidation_run_id,
            revalidation_result,
            evidence_refs,
        })
    }

    pub fn incident_id(&self) -> IncidentId {
        self.incident_id
    }

    pub fn revalidation_run_id(&self) -> RevalidationRunId {
        self.revalidation_run_id
    }

    pub fn revalidation_result(&self) -> RevalidationResult {
        self.revalidation_result
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn is_successful(&self) -> bool {
        matches!(
            self.revalidation_result,
            RevalidationResult::Passed | RevalidationResult::PassedWithDegradation
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyPurpose {
    Storage,
    Export,
    Signing,
    Federation,
    Backup,
    Provider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyLifecycleState {
    Active,
    Rotating,
    Retired,
    Revoked,
    Destroyed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretRef {
    id: crate::SecretRefId,
    opaque_uri: String,
    provider: String,
    workspace_id: WorkspaceId,
    purpose: String,
    metadata: BTreeMap<String, String>,
    version: Option<String>,
}

impl SecretRef {
    pub fn new(
        opaque_uri: impl Into<String>,
        provider: impl Into<String>,
        workspace_id: WorkspaceId,
        purpose: impl Into<String>,
        metadata: BTreeMap<String, String>,
        version: Option<String>,
    ) -> Result<Self, DomainError> {
        let opaque_uri = required_text("secret reference URI", opaque_uri)?;
        if !opaque_uri.starts_with("secret://") || opaque_uri.contains("plaintext=") {
            return Err(DomainError::InvalidArgument(
                "secret reference must be an opaque secret:// URI".into(),
            ));
        }
        Ok(Self {
            id: crate::SecretRefId::new(),
            opaque_uri,
            provider: required_text("secret provider", provider)?,
            workspace_id,
            purpose: required_text("secret purpose", purpose)?,
            metadata,
            version,
        })
    }

    /// Rebuild a reference that already exists in storage, keeping its
    /// identity.
    ///
    /// [`Self::new`] mints a fresh id, which is right when a secret is first
    /// created and wrong when one is read back: the id is what a lease names,
    /// and a repository that minted a new one on every read would hand out
    /// leases that resolve to nothing. Validation is identical to `new` — a
    /// stored row is not trusted to be well-formed just because it is stored.
    pub fn rehydrate(
        id: crate::SecretRefId,
        opaque_uri: impl Into<String>,
        provider: impl Into<String>,
        workspace_id: WorkspaceId,
        purpose: impl Into<String>,
        metadata: BTreeMap<String, String>,
        version: Option<String>,
    ) -> Result<Self, DomainError> {
        let mut reference = Self::new(
            opaque_uri,
            provider,
            workspace_id,
            purpose,
            metadata,
            version,
        )?;
        reference.id = id;
        Ok(reference)
    }

    pub fn id(&self) -> crate::SecretRefId {
        self.id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn authorize_resolution(
        &self,
        request: &SecretResolutionRequest,
    ) -> Result<SecretLease, DomainError> {
        if request.workspace_id != self.workspace_id || request.purpose != self.purpose {
            return Err(DomainError::PolicyViolation(
                "secret resolution scope does not match SecretRef".into(),
            ));
        }
        required_text(
            "secret authorization reference",
            request.authorization_ref.clone(),
        )?;
        Ok(SecretLease {
            secret_ref_id: self.id,
            authorization_ref: request.authorization_ref.clone(),
            issued_at: request.issued_at,
        })
    }

    pub fn opaque_uri(&self) -> &str {
        &self.opaque_uri
    }

    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretResolutionRequest {
    workspace_id: WorkspaceId,
    purpose: String,
    authorization_ref: String,
    issued_at: Timestamp,
}

impl SecretResolutionRequest {
    pub fn new(
        workspace_id: WorkspaceId,
        purpose: impl Into<String>,
        authorization_ref: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id,
            purpose: purpose.into(),
            authorization_ref: authorization_ref.into(),
            issued_at: crate::now(),
        }
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn authorization_ref(&self) -> &str {
        &self.authorization_ref
    }

    pub fn issued_at(&self) -> Timestamp {
        self.issued_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretLease {
    secret_ref_id: crate::SecretRefId,
    authorization_ref: String,
    issued_at: Timestamp,
}

impl SecretLease {
    pub fn secret_ref_id(&self) -> crate::SecretRefId {
        self.secret_ref_id
    }

    pub fn authorization_ref(&self) -> &str {
        &self.authorization_ref
    }

    pub fn issued_at(&self) -> Timestamp {
        self.issued_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct KeyReference {
    id: crate::KeyReferenceId,
    provider: String,
    key_id: String,
    version: String,
    purpose: KeyPurpose,
    scope: String,
    algorithm_suite: String,
    lifecycle_state: KeyLifecycleState,
}

impl KeyReference {
    pub fn new(
        provider: impl Into<String>,
        key_id: impl Into<String>,
        version: impl Into<String>,
        purpose: KeyPurpose,
        scope: impl Into<String>,
        algorithm_suite: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: crate::KeyReferenceId::new(),
            provider: required_text("key provider", provider)?,
            key_id: required_text("key id", key_id)?,
            version: required_text("key version", version)?,
            purpose,
            scope: required_text("key scope", scope)?,
            algorithm_suite: required_text("key algorithm suite", algorithm_suite)?,
            lifecycle_state: KeyLifecycleState::Active,
        })
    }

    pub fn lifecycle_state(&self) -> KeyLifecycleState {
        self.lifecycle_state
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn purpose(&self) -> KeyPurpose {
        self.purpose
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn algorithm_suite(&self) -> &str {
        &self.algorithm_suite
    }

    pub fn is_usable(&self) -> bool {
        matches!(
            self.lifecycle_state,
            KeyLifecycleState::Active | KeyLifecycleState::Rotating
        )
    }

    pub fn begin_rotation(&mut self) -> Result<(), DomainError> {
        if self.lifecycle_state != KeyLifecycleState::Active {
            return Err(DomainError::PolicyViolation(
                "only active key can enter rotation".into(),
            ));
        }
        self.lifecycle_state = KeyLifecycleState::Rotating;
        Ok(())
    }

    pub fn retire(&mut self) -> Result<(), DomainError> {
        if !matches!(
            self.lifecycle_state,
            KeyLifecycleState::Active | KeyLifecycleState::Rotating
        ) {
            return Err(DomainError::PolicyViolation(
                "only active or rotating key can retire".into(),
            ));
        }
        self.lifecycle_state = KeyLifecycleState::Retired;
        Ok(())
    }

    pub fn revoke(&mut self) -> Result<(), DomainError> {
        if !matches!(
            self.lifecycle_state,
            KeyLifecycleState::Active | KeyLifecycleState::Rotating | KeyLifecycleState::Retired
        ) {
            return Err(DomainError::PolicyViolation(
                "key cannot be revoked from its current lifecycle state".into(),
            ));
        }
        self.lifecycle_state = KeyLifecycleState::Revoked;
        Ok(())
    }

    pub fn destroy(&mut self) -> Result<(), DomainError> {
        if self.lifecycle_state != KeyLifecycleState::Revoked {
            return Err(DomainError::PolicyViolation(
                "key must be revoked before destruction".into(),
            ));
        }
        self.lifecycle_state = KeyLifecycleState::Destroyed;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignatureAlgorithm {
    Ed25519,
}

impl SignatureAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignatureRecord {
    object_digest: String,
    signer_identity: String,
    key_ref: KeyReference,
    algorithm: SignatureAlgorithm,
    signature: String,
    signed_at: Timestamp,
}

#[derive(Serialize)]
struct SignaturePayload<'a, T> {
    object: &'a T,
    object_digest: &'a str,
    signer_identity: &'a str,
    key_ref: &'a KeyReference,
    algorithm: SignatureAlgorithm,
    signed_at: Timestamp,
}

impl SignatureRecord {
    pub fn new(
        object_digest: impl Into<String>,
        signer_identity: impl Into<String>,
        key_ref: KeyReference,
        algorithm: SignatureAlgorithm,
        signature: impl Into<String>,
        signed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let object_digest = required_text("signature object digest", object_digest)?;
        let signer_identity = required_text("signature signer identity", signer_identity)?;
        let signature = required_text("signature bytes", signature)?;
        if key_ref.purpose() != KeyPurpose::Signing {
            return Err(DomainError::PolicyViolation(
                "signature key reference must use signing purpose".into(),
            ));
        }
        if key_ref.algorithm_suite() != algorithm.as_str() {
            return Err(DomainError::InvalidArgument(
                "signature algorithm does not match key reference algorithm suite".into(),
            ));
        }
        Ok(Self {
            object_digest,
            signer_identity,
            key_ref,
            algorithm,
            signature,
            signed_at,
        })
    }

    pub fn object_digest(&self) -> &str {
        &self.object_digest
    }

    pub fn signer_identity(&self) -> &str {
        &self.signer_identity
    }

    pub fn key_ref(&self) -> &KeyReference {
        &self.key_ref
    }

    pub fn algorithm(&self) -> SignatureAlgorithm {
        self.algorithm
    }

    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn signed_at(&self) -> Timestamp {
        self.signed_at
    }

    pub fn validate_for(&self, object_digest: &str) -> Result<(), DomainError> {
        if self.object_digest != object_digest {
            return Err(DomainError::InvalidArgument(
                "signature object digest does not match artifact".into(),
            ));
        }
        if self.key_ref.purpose() != KeyPurpose::Signing {
            return Err(DomainError::PolicyViolation(
                "signature key reference must use signing purpose".into(),
            ));
        }
        if self.key_ref.algorithm_suite() != self.algorithm.as_str() {
            return Err(DomainError::InvalidArgument(
                "signature algorithm does not match key reference algorithm suite".into(),
            ));
        }
        required_text("signature signer identity", self.signer_identity.clone())?;
        required_text("signature bytes", self.signature.clone())?;
        Ok(())
    }

    pub fn payload_for<T: Serialize>(&self, object: &T) -> Result<Vec<u8>, DomainError> {
        serde_json::to_vec(&SignaturePayload {
            object,
            object_digest: &self.object_digest,
            signer_identity: &self.signer_identity,
            key_ref: &self.key_ref,
            algorithm: self.algorithm,
            signed_at: self.signed_at,
        })
        .map_err(|error| {
            DomainError::InvalidArgument(format!("signature payload is invalid: {error}"))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignerTrustRule {
    signer_identity: String,
    provider: String,
    key_id: String,
    key_version: String,
    key_scope: String,
    algorithm: SignatureAlgorithm,
}

impl SignerTrustRule {
    pub fn new(
        signer_identity: impl Into<String>,
        provider: impl Into<String>,
        key_id: impl Into<String>,
        key_version: impl Into<String>,
        key_scope: impl Into<String>,
        algorithm: SignatureAlgorithm,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            signer_identity: required_text("trusted signer identity", signer_identity)?,
            provider: required_text("trusted key provider", provider)?,
            key_id: required_text("trusted key id", key_id)?,
            key_version: required_text("trusted key version", key_version)?,
            key_scope: required_text("trusted key scope", key_scope)?,
            algorithm,
        })
    }

    fn matches(&self, signature: &SignatureRecord) -> bool {
        self.signer_identity == signature.signer_identity()
            && self.provider == signature.key_ref().provider()
            && self.key_id == signature.key_ref().key_id()
            && self.key_version == signature.key_ref().version()
            && self.key_scope == signature.key_ref().scope()
            && self.algorithm == signature.algorithm()
    }

    pub fn signer_identity(&self) -> &str {
        &self.signer_identity
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key_version(&self) -> &str {
        &self.key_version
    }

    pub fn key_scope(&self) -> &str {
        &self.key_scope
    }

    pub fn algorithm(&self) -> &SignatureAlgorithm {
        &self.algorithm
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignerTrustFailure {
    NoMatchingRule,
    KeyNotUsable,
    KeyPurposeNotSigning,
    AlgorithmMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignerTrustDecision {
    failures: Vec<SignerTrustFailure>,
}

impl SignerTrustDecision {
    pub fn is_trusted(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[SignerTrustFailure] {
        &self.failures
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignerTrustPolicy {
    rules: Vec<SignerTrustRule>,
}

impl SignerTrustPolicy {
    pub fn new(rules: Vec<SignerTrustRule>) -> Result<Self, DomainError> {
        if rules.is_empty() {
            return Err(DomainError::InvalidArgument(
                "signer trust policy requires at least one rule".into(),
            ));
        }
        Ok(Self { rules })
    }

    pub fn rules(&self) -> &[SignerTrustRule] {
        &self.rules
    }

    pub fn evaluate(&self, signature: &SignatureRecord) -> SignerTrustDecision {
        let mut failures = Vec::new();
        if signature.key_ref().purpose() != KeyPurpose::Signing {
            failures.push(SignerTrustFailure::KeyPurposeNotSigning);
        }
        if !signature.key_ref().is_usable() {
            failures.push(SignerTrustFailure::KeyNotUsable);
        }
        if signature.key_ref().algorithm_suite() != signature.algorithm().as_str() {
            failures.push(SignerTrustFailure::AlgorithmMismatch);
        }
        if !self.rules.iter().any(|rule| rule.matches(signature)) {
            failures.push(SignerTrustFailure::NoMatchingRule);
        }
        SignerTrustDecision { failures }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyProviderError {
    #[error("key provider unavailable: {0}")]
    Unavailable(String),
    #[error("key reference is not usable")]
    NotUsable,
    #[error("key resolution denied: {0}")]
    Denied(String),
}

pub struct ResolvedKeyMaterial {
    bytes: Vec<u8>,
}

impl std::fmt::Debug for ResolvedKeyMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The byte count is safe to show; the material itself is never
        // rendered, so a caller matching on a `Result<ResolvedKeyMaterial, _>`
        // with `expect_err`/`{:?}` cannot leak it through this impl.
        formatter
            .debug_struct("ResolvedKeyMaterial")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl ResolvedKeyMaterial {
    pub fn from_ephemeral(bytes: Vec<u8>) -> Result<Self, KeyProviderError> {
        if bytes.is_empty() {
            return Err(KeyProviderError::Denied("empty key material".into()));
        }
        Ok(Self { bytes })
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub trait KeyProvider: Send + Sync {
    fn resolve(
        &self,
        key: &KeyReference,
        request: &SecretResolutionRequest,
    ) -> Result<ResolvedKeyMaterial, KeyProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataClassification {
    sensitivity: crate::Sensitivity,
    categories: Vec<String>,
    jurisdiction_tags: Vec<String>,
    handling_requirements: Vec<String>,
    classification_source: String,
    provenance: String,
}

impl DataClassification {
    pub fn new(
        sensitivity: crate::Sensitivity,
        categories: Vec<String>,
        jurisdiction_tags: Vec<String>,
        handling_requirements: Vec<String>,
        classification_source: impl Into<String>,
        provenance: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            sensitivity,
            categories,
            jurisdiction_tags,
            handling_requirements,
            classification_source: required_text("classification source", classification_source)?,
            provenance: required_text("classification provenance", provenance)?,
        })
    }

    pub fn source(
        sensitivity: crate::Sensitivity,
        source_ref: impl Into<String>,
        provenance: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::new(
            sensitivity,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            source_ref,
            provenance,
        )
    }

    pub fn sensitivity(&self) -> crate::Sensitivity {
        self.sensitivity
    }

    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    pub fn classification_source(&self) -> &str {
        &self.classification_source
    }

    pub fn provenance(&self) -> &str {
        &self.provenance
    }

    pub fn jurisdiction_tags(&self) -> &[String] {
        &self.jurisdiction_tags
    }

    pub fn handling_requirements(&self) -> &[String] {
        &self.handling_requirements
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClassificationLineage {
    id: crate::ClassificationLineageId,
    source_refs: Vec<String>,
    source_classifications: Vec<DataClassification>,
    effective_classification: DataClassification,
    classification_policy_version: String,
    derivation_ref: String,
}

impl ClassificationLineage {
    pub fn derive(
        source_refs: Vec<String>,
        source_classifications: Vec<DataClassification>,
        classification_policy_version: impl Into<String>,
        derivation_ref: impl Into<String>,
    ) -> Result<Self, DomainError> {
        required_refs("classification source references", &source_refs)?;
        if source_classifications.is_empty() {
            return Err(DomainError::InvalidArgument(
                "classification lineage requires source classifications".into(),
            ));
        }
        let mut effective = source_classifications[0].clone();
        effective.sensitivity = source_classifications
            .iter()
            .map(DataClassification::sensitivity)
            .max()
            .unwrap_or(crate::Sensitivity::Public);
        Ok(Self {
            id: crate::ClassificationLineageId::new(),
            source_refs,
            source_classifications,
            effective_classification: effective,
            classification_policy_version: required_text(
                "classification policy version",
                classification_policy_version,
            )?,
            derivation_ref: required_text("classification derivation reference", derivation_ref)?,
        })
    }

    pub fn effective_classification(&self) -> &DataClassification {
        &self.effective_classification
    }

    pub fn source_refs(&self) -> &[String] {
        &self.source_refs
    }

    pub fn policy_version(&self) -> &str {
        &self.classification_policy_version
    }

    pub fn source_classifications(&self) -> &[DataClassification] {
        &self.source_classifications
    }

    pub fn classification_policy_version(&self) -> &str {
        &self.classification_policy_version
    }

    pub fn derivation_ref(&self) -> &str {
        &self.derivation_ref
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataPolicy {
    id: crate::DataPolicyId,
    version: String,
    maximum_sensitivity: crate::Sensitivity,
    allowed_destinations: BTreeSet<DataDestination>,
    required_capability: Option<Capability>,
}

impl DataPolicy {
    pub fn new(
        id: crate::DataPolicyId,
        version: impl Into<String>,
        maximum_sensitivity: crate::Sensitivity,
        allowed_destinations: BTreeSet<DataDestination>,
        required_capability: Option<Capability>,
    ) -> Result<Self, DomainError> {
        if allowed_destinations.is_empty() {
            return Err(DomainError::InvalidArgument(
                "data policy requires allowed destinations".into(),
            ));
        }
        Ok(Self {
            id,
            version: required_text("data policy version", version)?,
            maximum_sensitivity,
            allowed_destinations,
            required_capability,
        })
    }

    pub fn id(&self) -> crate::DataPolicyId {
        self.id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn maximum_sensitivity(&self) -> crate::Sensitivity {
        self.maximum_sensitivity
    }

    pub fn evaluate(
        &self,
        classification: &DataClassification,
        destination: DataDestination,
        capability_granted: bool,
    ) -> DataPolicyDecision {
        if classification.sensitivity > self.maximum_sensitivity {
            return DataPolicyDecision::denied(
                &self.version,
                "classification exceeds policy ceiling",
            );
        }
        if !self.allowed_destinations.contains(&destination) {
            return DataPolicyDecision::denied(&self.version, "destination is not allowed");
        }
        if self.required_capability.is_some() && !capability_granted {
            return DataPolicyDecision::denied(&self.version, "required capability is absent");
        }
        DataPolicyDecision::allowed(&self.version)
    }

    pub fn allowed_destinations(&self) -> &BTreeSet<DataDestination> {
        &self.allowed_destinations
    }

    pub fn required_capability(&self) -> Option<&Capability> {
        self.required_capability.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataPolicyDecision {
    allowed: bool,
    policy_version: String,
    reason: String,
}

impl DataPolicyDecision {
    fn allowed(policy_version: &str) -> Self {
        Self {
            allowed: true,
            policy_version: policy_version.to_owned(),
            reason: "allowed".into(),
        }
    }

    fn denied(policy_version: &str, reason: &str) -> Self {
        Self {
            allowed: false,
            policy_version: policy_version.to_owned(),
            reason: reason.into(),
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

pub fn evaluate_model_boundary(
    policy: &DataPolicy,
    classification: &DataClassification,
    destination: DataDestination,
    capability_granted: bool,
) -> DataPolicyDecision {
    policy.evaluate(classification, destination, capability_granted)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeclassificationDecision {
    id: crate::DeclassificationDecisionId,
    target_ref: String,
    previous_classification: crate::Sensitivity,
    new_classification: crate::Sensitivity,
    transformation: String,
    evidence_refs: Vec<String>,
    policy_version: String,
    actor_id: PrincipalId,
    approver_id: PrincipalId,
    decided_at: Timestamp,
}

impl DeclassificationDecision {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_ref: impl Into<String>,
        previous_classification: crate::Sensitivity,
        new_classification: crate::Sensitivity,
        transformation: impl Into<String>,
        evidence_refs: Vec<String>,
        policy_version: impl Into<String>,
        actor_id: PrincipalId,
        approver_id: PrincipalId,
        decided_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if new_classification >= previous_classification {
            return Err(DomainError::PolicyViolation(
                "declassification must lower sensitivity".into(),
            ));
        }
        required_refs("declassification evidence", &evidence_refs)?;
        Ok(Self {
            id: crate::DeclassificationDecisionId::new(),
            target_ref: required_text("declassification target", target_ref)?,
            previous_classification,
            new_classification,
            transformation: required_text("declassification transformation", transformation)?,
            evidence_refs,
            policy_version: required_text("declassification policy version", policy_version)?,
            actor_id,
            approver_id,
            decided_at,
        })
    }

    pub fn apply(
        &self,
        classification: &DataClassification,
    ) -> Result<DataClassification, DomainError> {
        if classification.sensitivity != self.previous_classification {
            return Err(DomainError::PolicyViolation(
                "declassification source does not match decision".into(),
            ));
        }
        let mut next = classification.clone();
        next.sensitivity = self.new_classification;
        next.provenance = format!("{};declassified:{}", next.provenance, self.id);
        Ok(next)
    }

    pub fn previous_classification(&self) -> &crate::Sensitivity {
        &self.previous_classification
    }

    pub fn new_classification(&self) -> &crate::Sensitivity {
        &self.new_classification
    }

    pub fn transformation(&self) -> &str {
        &self.transformation
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn actor_id(&self) -> PrincipalId {
        self.actor_id
    }

    pub fn approver_id(&self) -> PrincipalId {
        self.approver_id
    }

    pub fn decided_at(&self) -> Timestamp {
        self.decided_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionTrigger {
    CreatedAt,
    LastUsedAt,
    ExecutionClosedAt,
    WorkspaceClosedAt,
    PolicyEvent,
    ExplicitDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisposalMethod {
    LogicalDelete,
    PhysicalDelete,
    CryptoErasure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionState {
    Active,
    Expired,
    PendingDisposal,
    Disposed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    minimum_retention: Option<Duration>,
    maximum_retention: Option<Duration>,
    trigger: RetentionTrigger,
    disposal_method: DisposalMethod,
    policy_version: String,
}

impl RetentionPolicy {
    pub fn new(
        minimum_retention: Option<Duration>,
        maximum_retention: Option<Duration>,
        trigger: RetentionTrigger,
        disposal_method: DisposalMethod,
        policy_version: impl Into<String>,
    ) -> Result<Self, DomainError> {
        if minimum_retention.is_none() && maximum_retention.is_none() {
            return Err(DomainError::InvalidArgument(
                "retention policy requires a retention boundary".into(),
            ));
        }
        if minimum_retention
            .zip(maximum_retention)
            .is_some_and(|(min, max)| min > max)
        {
            return Err(DomainError::InvalidArgument(
                "minimum retention must not exceed maximum retention".into(),
            ));
        }
        Ok(Self {
            minimum_retention,
            maximum_retention,
            trigger,
            disposal_method,
            policy_version: required_text("retention policy version", policy_version)?,
        })
    }

    pub fn state_at(&self, created_at: Timestamp, at: Timestamp) -> RetentionState {
        if self
            .maximum_retention
            .is_some_and(|maximum| at >= created_at + maximum)
        {
            RetentionState::Expired
        } else {
            RetentionState::Active
        }
    }

    pub fn disposal_method(&self) -> DisposalMethod {
        self.disposal_method
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn minimum_retention(&self) -> Option<Duration> {
        self.minimum_retention
    }

    pub fn maximum_retention(&self) -> Option<Duration> {
        self.maximum_retention
    }

    pub fn trigger(&self) -> RetentionTrigger {
        self.trigger
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataHold {
    id: crate::DataHoldId,
    scope: String,
    reason: String,
    authority: PrincipalId,
    created_at: Timestamp,
    expires_at: Option<Timestamp>,
    policy_ref: String,
}

impl DataHold {
    pub fn new(
        scope: impl Into<String>,
        reason: impl Into<String>,
        authority: PrincipalId,
        policy_ref: impl Into<String>,
        created_at: Timestamp,
        expires_at: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        if expires_at.is_some_and(|expires| expires < created_at) {
            return Err(DomainError::InvalidArgument(
                "data hold expiry must not precede creation".into(),
            ));
        }
        Ok(Self {
            id: crate::DataHoldId::new(),
            scope: required_text("data hold scope", scope)?,
            reason: required_text("data hold reason", reason)?,
            authority,
            created_at,
            expires_at,
            policy_ref: required_text("data hold policy reference", policy_ref)?,
        })
    }

    pub fn id(&self) -> crate::DataHoldId {
        self.id
    }

    pub fn is_active(&self, at: Timestamp) -> bool {
        at >= self.created_at && self.expires_at.is_none_or(|expires| at <= expires)
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn expires_at(&self) -> Option<Timestamp> {
        self.expires_at
    }

    pub fn policy_ref(&self) -> &str {
        &self.policy_ref
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionSemantics {
    LogicalDelete,
    PhysicalDelete,
    CryptoErasure,
}

impl From<DeletionSemantics> for DisposalMethod {
    fn from(value: DeletionSemantics) -> Self {
        match value {
            DeletionSemantics::LogicalDelete => Self::LogicalDelete,
            DeletionSemantics::PhysicalDelete => Self::PhysicalDelete,
            DeletionSemantics::CryptoErasure => Self::CryptoErasure,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeletionRequest {
    id: crate::DeletionRequestId,
    requester: PrincipalId,
    authority: PrincipalId,
    selector: String,
    semantics: DeletionSemantics,
    plan_ref: String,
    execution_ref: String,
    requested_at: Timestamp,
}

impl DeletionRequest {
    pub fn new(
        requester: PrincipalId,
        authority: PrincipalId,
        selector: impl Into<String>,
        semantics: DeletionSemantics,
        plan_ref: impl Into<String>,
        execution_ref: impl Into<String>,
        requested_at: Timestamp,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: crate::DeletionRequestId::new(),
            requester,
            authority,
            selector: required_text("deletion selector", selector)?,
            semantics,
            plan_ref: required_text("deletion plan reference", plan_ref)?,
            execution_ref: required_text("deletion execution reference", execution_ref)?,
            requested_at,
        })
    }

    pub fn id(&self) -> crate::DeletionRequestId {
        self.id
    }

    pub fn semantics(&self) -> DeletionSemantics {
        self.semantics
    }

    pub fn requester(&self) -> PrincipalId {
        self.requester
    }

    pub fn selector(&self) -> &str {
        &self.selector
    }

    pub fn plan_ref(&self) -> &str {
        &self.plan_ref
    }

    pub fn execution_ref(&self) -> &str {
        &self.execution_ref
    }

    pub fn requested_at(&self) -> Timestamp {
        self.requested_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeletionPlan {
    id: crate::DeletionPlanId,
    request_id: crate::DeletionRequestId,
    target_refs: Vec<String>,
    dependency_refs: Vec<String>,
    hold_ids: Vec<crate::DataHoldId>,
    created_at: Timestamp,
}

impl DeletionPlan {
    pub fn new(
        request_id: crate::DeletionRequestId,
        target_refs: Vec<String>,
        dependency_refs: Vec<String>,
        hold_ids: Vec<crate::DataHoldId>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        required_refs("deletion target references", &target_refs)?;
        if dependency_refs.iter().any(|value| value.trim().is_empty()) {
            return Err(DomainError::InvalidArgument(
                "deletion dependency references must not be empty".into(),
            ));
        }
        Ok(Self {
            id: crate::DeletionPlanId::new(),
            request_id,
            target_refs,
            dependency_refs,
            hold_ids,
            created_at,
        })
    }

    pub fn id(&self) -> crate::DeletionPlanId {
        self.id
    }

    pub fn request_id(&self) -> crate::DeletionRequestId {
        self.request_id
    }

    pub fn target_refs(&self) -> &[String] {
        &self.target_refs
    }

    pub fn dependency_refs(&self) -> &[String] {
        &self.dependency_refs
    }

    pub fn hold_ids(&self) -> &[crate::DataHoldId] {
        &self.hold_ids
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionVerificationOutcome {
    Verified,
    Incomplete,
    BlockedByHold,
    Inconclusive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeletionVerification {
    id: crate::DeletionVerificationId,
    request_id: crate::DeletionRequestId,
    semantics: DeletionSemantics,
    checked_refs: Vec<String>,
    remaining_copies: Vec<String>,
    evidence_refs: Vec<String>,
    outcome: DeletionVerificationOutcome,
    verified_at: Timestamp,
}

impl DeletionVerification {
    pub fn is_complete(&self) -> bool {
        self.outcome == DeletionVerificationOutcome::Verified && self.remaining_copies.is_empty()
    }

    pub fn outcome(&self) -> DeletionVerificationOutcome {
        self.outcome
    }

    pub fn remaining_copies(&self) -> &[String] {
        &self.remaining_copies
    }

    pub fn request_id(&self) -> crate::DeletionRequestId {
        self.request_id
    }

    pub fn semantics(&self) -> DeletionSemantics {
        self.semantics
    }

    pub fn checked_refs(&self) -> &[String] {
        &self.checked_refs
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn verified_at(&self) -> Timestamp {
        self.verified_at
    }
}

pub fn verify_deletion(
    request: &DeletionRequest,
    plan: &DeletionPlan,
    holds: &[DataHold],
    checked_refs: Vec<String>,
    remaining_copies: Vec<String>,
    evidence_refs: Vec<String>,
    verified_at: Timestamp,
) -> Result<DeletionVerification, DomainError> {
    if request.id() != plan.request_id() {
        return Err(DomainError::PolicyViolation(
            "deletion plan does not belong to request".into(),
        ));
    }
    required_refs("deletion verification checked references", &checked_refs)?;
    required_refs("deletion verification evidence", &evidence_refs)?;
    let mut all_planned_refs = plan.target_refs.iter().chain(plan.dependency_refs.iter());
    if all_planned_refs.any(|reference| !checked_refs.iter().any(|checked| checked == reference)) {
        return Err(DomainError::PolicyViolation(
            "deletion verification does not cover every planned dependency".into(),
        ));
    }
    let outcome = if holds.iter().any(|hold| hold.is_active(verified_at)) {
        DeletionVerificationOutcome::BlockedByHold
    } else if remaining_copies.is_empty() {
        DeletionVerificationOutcome::Verified
    } else {
        DeletionVerificationOutcome::Incomplete
    };
    Ok(DeletionVerification {
        id: crate::DeletionVerificationId::new(),
        request_id: request.id(),
        semantics: request.semantics(),
        checked_refs,
        remaining_copies,
        evidence_refs,
        outcome,
        verified_at,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataExportPlan {
    id: crate::DataExportPlanId,
    workspace_id: WorkspaceId,
    exact_scope: String,
    purpose: String,
    recipient: String,
    classification: DataClassification,
    object_revisions: Vec<String>,
    redaction_refs: Vec<String>,
    format_schema: String,
    encryption_key_ref: Option<String>,
    signing_key_ref: Option<String>,
    expires_at: Option<Timestamp>,
    policy_version: String,
    required_capability: Capability,
    created_at: Timestamp,
}

impl DataExportPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_id: WorkspaceId,
        exact_scope: impl Into<String>,
        purpose: impl Into<String>,
        recipient: impl Into<String>,
        classification: DataClassification,
        object_revisions: Vec<String>,
        redaction_refs: Vec<String>,
        format_schema: impl Into<String>,
        encryption_key_ref: Option<String>,
        signing_key_ref: Option<String>,
        expires_at: Option<Timestamp>,
        policy_version: impl Into<String>,
        required_capability: Capability,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        required_refs("export object revisions", &object_revisions)?;
        if redaction_refs
            .iter()
            .any(|reference| reference.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "export redaction references must not be empty".into(),
            ));
        }
        Ok(Self {
            id: crate::DataExportPlanId::new(),
            workspace_id,
            exact_scope: required_text("export scope", exact_scope)?,
            purpose: required_text("export purpose", purpose)?,
            recipient: required_text("export recipient", recipient)?,
            classification,
            object_revisions,
            redaction_refs,
            format_schema: required_text("export format schema", format_schema)?,
            encryption_key_ref,
            signing_key_ref,
            expires_at,
            policy_version: required_text("export policy version", policy_version)?,
            required_capability,
            created_at,
        })
    }

    /// The exact revisions this export carries.
    ///
    /// A reader needs them to check that an export named a subset rather than a
    /// selector that could widen later; they had no accessor, so the field was
    /// stored and unreadable.
    pub fn object_revisions(&self) -> &[String] {
        &self.object_revisions
    }

    pub fn exact_scope(&self) -> &str {
        &self.exact_scope
    }

    pub fn authorize(
        &self,
        policy: &DataPolicy,
        capability_granted: bool,
        at: Timestamp,
    ) -> Result<AuthorizedDataExport, DomainError> {
        if self.expires_at.is_some_and(|expires| at > expires) {
            return Err(DomainError::PolicyViolation("export plan expired".into()));
        }
        let decision = policy.evaluate(
            &self.classification,
            DataDestination::ExportBundle,
            capability_granted,
        );
        if !decision.is_allowed() {
            return Err(DomainError::PolicyViolation(decision.reason().into()));
        }
        if policy.version() != self.policy_version {
            return Err(DomainError::PolicyViolation(
                "export plan policy version is stale".into(),
            ));
        }
        Ok(AuthorizedDataExport {
            plan: self.clone(),
            authorized_at: at,
            policy_version: policy.version().to_owned(),
        })
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    pub fn classification(&self) -> &DataClassification {
        &self.classification
    }

    pub fn redaction_refs(&self) -> &[String] {
        &self.redaction_refs
    }

    pub fn format_schema(&self) -> &str {
        &self.format_schema
    }

    pub fn encryption_key_ref(&self) -> Option<&str> {
        self.encryption_key_ref.as_deref()
    }

    pub fn signing_key_ref(&self) -> Option<&str> {
        self.signing_key_ref.as_deref()
    }

    pub fn expires_at(&self) -> Option<Timestamp> {
        self.expires_at
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn required_capability(&self) -> &Capability {
        &self.required_capability
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorizedDataExport {
    plan: DataExportPlan,
    authorized_at: Timestamp,
    policy_version: String,
}

impl AuthorizedDataExport {
    /// The plan this export was authorized to carry out.
    pub fn plan(&self) -> &DataExportPlan {
        &self.plan
    }

    pub fn authorized_at(&self) -> Timestamp {
        self.authorized_at
    }

    /// The policy version that permitted it — which is what makes the
    /// authorization attributable to a decision rather than to a moment.
    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportBundle {
    id: crate::ExportBundleId,
    manifest_digest: String,
    schema: String,
    object_revisions: Vec<String>,
    provenance_refs: Vec<String>,
    classification: DataClassification,
    object_hashes: BTreeMap<String, String>,
    signature: Option<String>,
    encryption_metadata: Option<String>,
    authority_transferred: bool,
    created_at: Timestamp,
}

impl ExportBundle {
    pub fn from_authorized(
        authorized: &AuthorizedDataExport,
        provenance_refs: Vec<String>,
        object_hashes: BTreeMap<String, String>,
        signature: Option<String>,
        encryption_metadata: Option<String>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        required_refs("export provenance references", &provenance_refs)?;
        if object_hashes.is_empty() {
            return Err(DomainError::InvalidArgument(
                "export bundle requires object integrity hashes".into(),
            ));
        }
        let plan = &authorized.plan;
        let manifest_digest = sha256_digest([
            plan.exact_scope.clone(),
            plan.purpose.clone(),
            plan.recipient.clone(),
            plan.format_schema.clone(),
            authorized.policy_version.clone(),
            plan.object_revisions.join(","),
            provenance_refs.join(","),
        ]);
        Ok(Self {
            id: crate::ExportBundleId::new(),
            manifest_digest,
            schema: plan.format_schema.clone(),
            object_revisions: plan.object_revisions.clone(),
            provenance_refs,
            classification: plan.classification.clone(),
            object_hashes,
            signature,
            encryption_metadata,
            authority_transferred: false,
            created_at,
        })
    }

    pub fn manifest_digest(&self) -> &str {
        &self.manifest_digest
    }

    pub fn authority_transferred(&self) -> bool {
        self.authority_transferred
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn object_revisions(&self) -> &[String] {
        &self.object_revisions
    }

    pub fn provenance_refs(&self) -> &[String] {
        &self.provenance_refs
    }

    pub fn classification(&self) -> &DataClassification {
        &self.classification
    }

    pub fn object_hashes(&self) -> &BTreeMap<String, String> {
        &self.object_hashes
    }

    pub fn encryption_metadata(&self) -> Option<&str> {
        self.encryption_metadata.as_deref()
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditIntegrityEntry {
    sequence: u64,
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    action: String,
    resource_ref: String,
    content_digest: String,
    previous_digest: String,
    entry_digest: String,
    recorded_at: Timestamp,
}

impl AuditIntegrityEntry {
    /// Everything an audit entry records, readable.
    ///
    /// The entry existed with nine private fields and no `impl` block at all:
    /// a hash-chained audit record whose digests, actor, action and position in
    /// the chain could be written and never read. Verification of the chain
    /// lived entirely inside `AuditIntegrityChain`, so nothing outside this file
    /// could check the chain, reconstruct it, or say what an entry was about.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn resource_ref(&self) -> &str {
        &self.resource_ref
    }

    /// The digest of what happened.
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }

    /// The digest of the entry before this one — the link that makes the chain
    /// a chain rather than a list.
    pub fn previous_digest(&self) -> &str {
        &self.previous_digest
    }

    pub fn entry_digest(&self) -> &str {
        &self.entry_digest
    }

    pub fn recorded_at(&self) -> Timestamp {
        self.recorded_at
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditIntegrityChain {
    entries: Vec<AuditIntegrityEntry>,
}

impl AuditIntegrityChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(
        &mut self,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        action: impl Into<String>,
        resource_ref: impl Into<String>,
        content_digest: impl Into<String>,
        recorded_at: Timestamp,
    ) {
        let action = action.into();
        let resource_ref = resource_ref.into();
        let content_digest = content_digest.into();
        let sequence = self.entries.len() as u64 + 1;
        let previous_digest = self
            .entries
            .last()
            .map(|entry| entry.entry_digest.clone())
            .unwrap_or_else(|| "genesis".into());
        let entry_digest = sha256_digest([
            sequence.to_string(),
            workspace_id.to_string(),
            principal_id.to_string(),
            action.clone(),
            resource_ref.clone(),
            content_digest.clone(),
            previous_digest.clone(),
            recorded_at.to_rfc3339(),
        ]);
        self.entries.push(AuditIntegrityEntry {
            sequence,
            workspace_id,
            principal_id,
            action,
            resource_ref,
            content_digest,
            previous_digest,
            entry_digest,
            recorded_at,
        });
    }

    pub fn entries(&self) -> &[AuditIntegrityEntry] {
        &self.entries
    }

    pub fn verify(&self) -> Result<(), DomainError> {
        let mut previous = "genesis".to_owned();
        for (index, entry) in self.entries.iter().enumerate() {
            let expected_sequence = index as u64 + 1;
            if entry.sequence != expected_sequence || entry.previous_digest != previous {
                return Err(DomainError::PolicyViolation(
                    "audit integrity chain continuity failed".into(),
                ));
            }
            let expected_digest = sha256_digest([
                entry.sequence.to_string(),
                entry.workspace_id.to_string(),
                entry.principal_id.to_string(),
                entry.action.clone(),
                entry.resource_ref.clone(),
                entry.content_digest.clone(),
                entry.previous_digest.clone(),
                entry.recorded_at.to_rfc3339(),
            ]);
            if entry.entry_digest != expected_digest {
                return Err(DomainError::PolicyViolation(
                    "audit integrity chain digest failed".into(),
                ));
            }
            previous = entry.entry_digest.clone();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationBaselineState {
    Qualified,
    Stale,
    Invalidated,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationLifecycle {
    PreMerge,
    Release,
    Deployment,
    Periodic,
    PostIncident,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStatus {
    Passed,
    Failed,
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QualificationBundle {
    id: crate::QualificationBundleId,
    lifecycle: QualificationLifecycle,
    profile: QualificationProfile,
    target_manifest: String,
    source_revision: String,
    build_digest: String,
    configuration_digest: String,
    environment_manifest: String,
    suite_version: String,
    evidence: Vec<HardGateEvidence>,
    conformance_report: Option<ConformanceReport>,
    known_limitations: Vec<String>,
    target_digest: String,
    started_at: Timestamp,
    completed_at: Option<Timestamp>,
    #[serde(default)]
    signature: Option<SignatureRecord>,
    #[serde(default)]
    post_incident_evidence: Option<PostIncidentQualificationEvidence>,
}

impl QualificationBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile: QualificationProfile,
        target_manifest: impl Into<String>,
        source_revision: impl Into<String>,
        build_digest: impl Into<String>,
        configuration_digest: impl Into<String>,
        environment_manifest: impl Into<String>,
        suite_version: impl Into<String>,
        evidence: Vec<HardGateEvidence>,
        known_limitations: Vec<String>,
        started_at: Timestamp,
        completed_at: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        if completed_at.is_some_and(|completed| completed < started_at) {
            return Err(DomainError::InvalidArgument(
                "qualification completion cannot precede start".into(),
            ));
        }
        let target_manifest = required_text("qualification target manifest", target_manifest)?;
        let source_revision = required_text("qualification source revision", source_revision)?;
        let build_digest = required_text("qualification build digest", build_digest)?;
        let configuration_digest =
            required_text("qualification configuration digest", configuration_digest)?;
        let environment_manifest =
            required_text("qualification environment manifest", environment_manifest)?;
        let suite_version = required_text("qualification suite version", suite_version)?;
        let mut seen = HashSet::new();
        for item in &evidence {
            if !seen.insert(item.requirement_id()) {
                return Err(DomainError::InvalidArgument(
                    "qualification evidence must have unique requirement IDs".into(),
                ));
            }
        }
        let target_digest = sha256_digest([
            target_manifest.clone(),
            source_revision.clone(),
            build_digest.clone(),
            configuration_digest.clone(),
            environment_manifest.clone(),
            suite_version.clone(),
        ]);
        Ok(Self {
            id: crate::QualificationBundleId::new(),
            lifecycle: QualificationLifecycle::Release,
            profile,
            target_manifest,
            source_revision,
            build_digest,
            configuration_digest,
            environment_manifest,
            suite_version,
            evidence,
            conformance_report: None,
            known_limitations,
            target_digest,
            started_at,
            completed_at,
            signature: None,
            post_incident_evidence: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_conformance_report(
        lifecycle: QualificationLifecycle,
        profile: QualificationProfile,
        target_manifest: impl Into<String>,
        source_revision: impl Into<String>,
        build_digest: impl Into<String>,
        configuration_digest: impl Into<String>,
        environment_manifest: impl Into<String>,
        suite_version: impl Into<String>,
        conformance_report: ConformanceReport,
        evidence: Vec<HardGateEvidence>,
        known_limitations: Vec<String>,
        started_at: Timestamp,
        completed_at: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        if conformance_report.profile != Some(profile) {
            return Err(DomainError::InvalidArgument(
                "qualification report profile must match bundle profile".into(),
            ));
        }

        let covered: HashSet<RequirementId> = conformance_report
            .results
            .iter()
            .flat_map(|result| result.requirement_ids.iter().copied())
            .collect();
        let missing: Vec<_> = profile_requirements(profile)
            .into_iter()
            .filter(|requirement_id| !covered.contains(requirement_id))
            .collect();
        if !missing.is_empty() {
            return Err(DomainError::InvalidArgument(format!(
                "qualification report is missing {} profile requirement result(s)",
                missing.len()
            )));
        }

        let mut bundle = Self::new(
            profile,
            target_manifest,
            source_revision,
            build_digest,
            configuration_digest,
            environment_manifest,
            suite_version,
            evidence,
            known_limitations,
            started_at,
            completed_at,
        )?;
        bundle.lifecycle = lifecycle;
        bundle.conformance_report = Some(conformance_report);
        Ok(bundle)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_conformance_report_for_manifest(
        lifecycle: QualificationLifecycle,
        profile: QualificationProfile,
        manifest: &VestraceCapabilityManifest,
        suite_version: impl Into<String>,
        conformance_report: ConformanceReport,
        evidence: Vec<HardGateEvidence>,
        known_limitations: Vec<String>,
        started_at: Timestamp,
        completed_at: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        manifest.validate()?;
        if !manifest.supported_profiles().contains(&profile) {
            return Err(DomainError::InvalidArgument(format!(
                "qualification profile {profile} is not listed in capability manifest supported profiles"
            )));
        }

        Self::from_conformance_report(
            lifecycle,
            profile,
            manifest.manifest_digest(),
            manifest.source_revision(),
            manifest.build_digest(),
            manifest.configuration_digest(),
            manifest.environment_manifest(),
            suite_version,
            conformance_report,
            evidence,
            known_limitations,
            started_at,
            completed_at,
        )
    }

    pub fn id(&self) -> crate::QualificationBundleId {
        self.id
    }

    pub fn lifecycle(&self) -> QualificationLifecycle {
        self.lifecycle
    }

    pub fn profile(&self) -> QualificationProfile {
        self.profile
    }

    pub fn evidence(&self) -> &[HardGateEvidence] {
        &self.evidence
    }

    pub fn conformance_report(&self) -> Option<&ConformanceReport> {
        self.conformance_report.as_ref()
    }

    pub fn target_digest(&self) -> &str {
        &self.target_digest
    }

    pub fn target_manifest(&self) -> &str {
        &self.target_manifest
    }

    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }

    pub fn build_digest(&self) -> &str {
        &self.build_digest
    }

    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }

    pub fn environment_manifest(&self) -> &str {
        &self.environment_manifest
    }

    pub fn validate_manifest_identity(
        &self,
        manifest: &VestraceCapabilityManifest,
        profile: QualificationProfile,
        lifecycle: QualificationLifecycle,
    ) -> Result<(), DomainError> {
        manifest.validate()?;
        if self.profile != profile {
            return Err(DomainError::InvalidArgument(
                "qualification bundle profile does not match requested deployment profile".into(),
            ));
        }
        if self.lifecycle != lifecycle {
            return Err(DomainError::InvalidArgument(
                "qualification bundle lifecycle does not match requested deployment lifecycle"
                    .into(),
            ));
        }
        if !manifest.supported_profiles().contains(&profile) {
            return Err(DomainError::InvalidArgument(format!(
                "qualification profile {profile} is not listed in capability manifest supported profiles"
            )));
        }

        let identity_matches = [
            (
                "target manifest",
                self.target_manifest.as_str(),
                manifest.manifest_digest(),
            ),
            (
                "source revision",
                self.source_revision.as_str(),
                manifest.source_revision(),
            ),
            (
                "build digest",
                self.build_digest.as_str(),
                manifest.build_digest(),
            ),
            (
                "configuration digest",
                self.configuration_digest.as_str(),
                manifest.configuration_digest(),
            ),
            (
                "environment manifest",
                self.environment_manifest.as_str(),
                manifest.environment_manifest(),
            ),
        ];
        if let Some((field, _, _)) = identity_matches
            .into_iter()
            .find(|(_, actual, expected)| actual != expected)
        {
            return Err(DomainError::InvalidArgument(format!(
                "qualification bundle {field} does not match capability manifest"
            )));
        }
        Ok(())
    }

    pub fn validate_manifest_binding(
        &self,
        manifest: &VestraceCapabilityManifest,
        profile: QualificationProfile,
        lifecycle: QualificationLifecycle,
    ) -> Result<(), DomainError> {
        self.validate_manifest_identity(manifest, profile, lifecycle)?;
        if self.status() != QualificationStatus::Passed {
            return Err(DomainError::InvalidArgument(
                "qualification bundle status is not passed".into(),
            ));
        }
        Ok(())
    }

    pub fn signature(&self) -> Option<&SignatureRecord> {
        self.signature.as_ref()
    }

    pub fn attach_post_incident_evidence(
        mut self,
        evidence: PostIncidentQualificationEvidence,
    ) -> Result<Self, DomainError> {
        if self.lifecycle != QualificationLifecycle::PostIncident {
            return Err(DomainError::PolicyViolation(
                "post-incident evidence requires post-incident qualification lifecycle".into(),
            ));
        }
        if self.post_incident_evidence.is_some() {
            return Err(DomainError::PolicyViolation(
                "qualification bundle already has post-incident evidence".into(),
            ));
        }
        self.post_incident_evidence = Some(evidence);
        Ok(self)
    }

    pub fn post_incident_evidence(&self) -> Option<&PostIncidentQualificationEvidence> {
        self.post_incident_evidence.as_ref()
    }

    pub fn unsigned_signing_payload(&self) -> Result<Vec<u8>, DomainError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        serde_json::to_vec(&unsigned).map_err(|error| {
            DomainError::InvalidArgument(format!(
                "qualification signing payload is invalid: {error}"
            ))
        })
    }

    pub fn unsigned_signing_digest(&self) -> Result<String, DomainError> {
        Ok(sha256_bytes(&self.unsigned_signing_payload()?))
    }

    pub fn signing_payload_for(&self, signature: &SignatureRecord) -> Result<Vec<u8>, DomainError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        signature.payload_for(&unsigned)
    }

    pub fn attach_signature(mut self, signature: SignatureRecord) -> Result<Self, DomainError> {
        if self.signature.is_some() {
            return Err(DomainError::PolicyViolation(
                "qualification bundle already has a signature".into(),
            ));
        }
        signature.validate_for(&self.unsigned_signing_digest()?)?;
        self.signature = Some(signature);
        Ok(self)
    }

    pub fn validate_signature(&self) -> Result<(), DomainError> {
        let Some(signature) = self.signature.as_ref() else {
            return Err(DomainError::PolicyViolation(
                "qualification bundle has no signature".into(),
            ));
        };
        signature.validate_for(&self.unsigned_signing_digest()?)
    }

    pub fn is_complete(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn started_at(&self) -> Timestamp {
        self.started_at
    }

    pub fn completed_at(&self) -> Option<Timestamp> {
        self.completed_at
    }

    pub fn status(&self) -> QualificationStatus {
        if self.lifecycle == QualificationLifecycle::PostIncident {
            let Some(evidence) = self.post_incident_evidence.as_ref() else {
                return QualificationStatus::Incomplete;
            };
            if !evidence.is_successful() {
                return QualificationStatus::Failed;
            }
        }
        let Some(report) = &self.conformance_report else {
            return QualificationStatus::Incomplete;
        };
        if report.is_pass() {
            QualificationStatus::Passed
        } else {
            QualificationStatus::Failed
        }
    }

    pub fn known_limitations(&self) -> &[String] {
        &self.known_limitations
    }

    pub fn suite_version(&self) -> &str {
        &self.suite_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QualificationBaseline {
    id: crate::QualificationBaselineId,
    profile: QualificationProfile,
    target_digest: String,
    state: QualificationBaselineState,
    published_at: Timestamp,
    invalidation_reason: Option<String>,
}

impl QualificationBaseline {
    pub fn from_bundle(
        bundle: &QualificationBundle,
        published_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if !bundle.is_complete() {
            return Err(DomainError::PolicyViolation(
                "incomplete qualification bundle cannot publish a baseline".into(),
            ));
        }
        Ok(Self {
            id: crate::QualificationBaselineId::new(),
            profile: bundle.profile(),
            target_digest: bundle.target_digest().to_owned(),
            state: QualificationBaselineState::Qualified,
            published_at,
            invalidation_reason: None,
        })
    }

    pub fn id(&self) -> crate::QualificationBaselineId {
        self.id
    }

    pub fn state(&self) -> QualificationBaselineState {
        self.state
    }

    pub fn target_digest(&self) -> &str {
        &self.target_digest
    }

    pub fn profile(&self) -> QualificationProfile {
        self.profile
    }

    pub fn published_at(&self) -> Timestamp {
        self.published_at
    }

    /// Why the baseline stopped qualifying anything.
    ///
    /// Required on the way in by `mark_stale` and `invalidate`, and unreadable
    /// until now — so a withdrawn qualification recorded its reason and could
    /// not be asked for it. The same shape as the recovery types' unreadable
    /// evidence, found the same way: by trying to write a case about it.
    pub fn invalidation_reason(&self) -> Option<&str> {
        self.invalidation_reason.as_deref()
    }

    pub fn matches_bundle(&self, bundle: &QualificationBundle) -> bool {
        self.profile == bundle.profile()
            && self.target_digest == bundle.target_digest()
            && self.state == QualificationBaselineState::Qualified
    }

    pub fn mark_stale(
        &mut self,
        reason: impl Into<String>,
        _at: Timestamp,
    ) -> Result<(), DomainError> {
        self.state = QualificationBaselineState::Stale;
        self.invalidation_reason = Some(required_text("baseline stale reason", reason)?);
        Ok(())
    }

    pub fn invalidate(
        &mut self,
        reason: impl Into<String>,
        _at: Timestamp,
    ) -> Result<(), DomainError> {
        self.state = QualificationBaselineState::Invalidated;
        self.invalidation_reason = Some(required_text("baseline invalidation reason", reason)?);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrustedGateFailure {
    WrongProfile,
    IncompleteBundle,
    BaselineNotQualified,
    BaselineMismatch,
    HardGate(HardGateFailure),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrustedQualificationGateDecision {
    failures: Vec<TrustedGateFailure>,
}

impl TrustedQualificationGateDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[TrustedGateFailure] {
        &self.failures
    }
}

pub struct TrustedQualificationGate;

impl TrustedQualificationGate {
    pub fn required_requirements() -> Vec<RequirementId> {
        GovernanceFederationGate::required_requirements(QualificationProfile::Trusted)
    }

    pub fn evaluate(
        bundle: &QualificationBundle,
        baseline: &QualificationBaseline,
    ) -> TrustedQualificationGateDecision {
        let mut failures = Vec::new();
        if bundle.profile() != QualificationProfile::Trusted {
            failures.push(TrustedGateFailure::WrongProfile);
        }
        if !bundle.is_complete() {
            failures.push(TrustedGateFailure::IncompleteBundle);
        }
        if baseline.state() != QualificationBaselineState::Qualified {
            failures.push(TrustedGateFailure::BaselineNotQualified);
        }
        if !baseline.matches_bundle(bundle) {
            failures.push(TrustedGateFailure::BaselineMismatch);
        }
        if failures.is_empty() {
            let gate = GovernanceFederationGate::evaluate(
                QualificationProfile::Trusted,
                bundle.evidence(),
            );
            failures.extend(
                gate.failures()
                    .iter()
                    .cloned()
                    .map(TrustedGateFailure::HardGate),
            );
        }
        TrustedQualificationGateDecision { failures }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned_bundle() -> QualificationBundle {
        QualificationBundle::new(
            QualificationProfile::Core,
            "sha256:manifest",
            "source-revision",
            "sha256:build",
            "sha256:config",
            "environment://test",
            "suite-v1",
            Vec::new(),
            Vec::new(),
            crate::now(),
            Some(crate::now()),
        )
        .unwrap()
    }

    fn signing_key() -> KeyReference {
        KeyReference::new(
            "local-file",
            "release-signing",
            "v1",
            KeyPurpose::Signing,
            "release",
            "ed25519",
        )
        .unwrap()
    }

    #[test]
    fn audit_chain_digest_is_reproducible() {
        let mut chain = AuditIntegrityChain::new();
        chain.append(
            WorkspaceId::new(),
            PrincipalId::new(),
            "test",
            "resource",
            "digest",
            crate::now(),
        );
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn trusted_gate_requires_the_full_registered_requirement_closure() {
        assert!(TrustedQualificationGate::required_requirements().len() > 100);
        assert!(
            TrustedQualificationGate::required_requirements()
                .iter()
                .any(|id| id.family == crate::conformance::RequirementFamily::Rec)
        );
    }

    #[test]
    fn policy_decision_rejects_disallowed_destination() {
        let policy = DataPolicy::new(
            crate::DataPolicyId::new(),
            "p1",
            crate::Sensitivity::Internal,
            BTreeSet::from([DataDestination::LocalModel]),
            None,
        )
        .unwrap();
        let classification =
            DataClassification::source(crate::Sensitivity::Internal, "source", "provenance")
                .unwrap();
        assert!(
            !policy
                .evaluate(&classification, DataDestination::RemoteProvider, true)
                .is_allowed()
        );
    }

    #[test]
    fn signed_qualification_attaches_structured_metadata_and_validates_digest() {
        let bundle = unsigned_bundle();
        let record = SignatureRecord::new(
            bundle.unsigned_signing_digest().unwrap(),
            "issuer://release",
            signing_key(),
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();

        let signed = bundle.clone().attach_signature(record).unwrap();
        assert!(signed.signature().is_some());
        assert!(signed.validate_signature().is_ok());
        assert_eq!(
            signed.unsigned_signing_digest().unwrap(),
            bundle.unsigned_signing_digest().unwrap()
        );
    }

    #[test]
    fn signed_qualification_rejects_wrong_digest_and_tampered_payload() {
        let bundle = unsigned_bundle();
        let record = SignatureRecord::new(
            "sha256:wrong",
            "issuer://release",
            signing_key(),
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();
        assert!(bundle.clone().attach_signature(record).is_err());

        let record = SignatureRecord::new(
            bundle.unsigned_signing_digest().unwrap(),
            "issuer://release",
            signing_key(),
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();
        let signed = bundle.attach_signature(record).unwrap();
        let mut tampered: QualificationBundle =
            serde_json::from_value(serde_json::to_value(&signed).unwrap()).unwrap();
        tampered.source_revision = "tampered-revision".into();
        assert!(tampered.validate_signature().is_err());
    }

    #[test]
    fn signature_requires_signing_key_and_non_blank_issuer() {
        let bundle = unsigned_bundle();
        let mut non_signing_key = signing_key();
        non_signing_key.purpose = KeyPurpose::Export;
        assert!(
            SignatureRecord::new(
                bundle.unsigned_signing_digest().unwrap(),
                "issuer://release",
                non_signing_key,
                SignatureAlgorithm::Ed25519,
                "base64-signature",
                crate::now(),
            )
            .is_err()
        );
        assert!(
            SignatureRecord::new(
                bundle.unsigned_signing_digest().unwrap(),
                " ",
                signing_key(),
                SignatureAlgorithm::Ed25519,
                "base64-signature",
                crate::now(),
            )
            .is_err()
        );
    }

    #[test]
    fn unsigned_qualification_json_without_signature_remains_readable() {
        let bundle = unsigned_bundle();
        let mut json = serde_json::to_value(&bundle).unwrap();
        json.as_object_mut().unwrap().remove("signature");
        let decoded: QualificationBundle = serde_json::from_value(json).unwrap();
        assert!(decoded.signature().is_none());
    }

    #[test]
    fn signer_trust_policy_matches_the_complete_signer_and_key_identity() {
        let bundle = unsigned_bundle();
        let signature = SignatureRecord::new(
            bundle.unsigned_signing_digest().unwrap(),
            "issuer://release",
            signing_key(),
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();
        let policy = SignerTrustPolicy::new(vec![
            SignerTrustRule::new(
                "issuer://release",
                "local-file",
                "release-signing",
                "v1",
                "release",
                SignatureAlgorithm::Ed25519,
            )
            .unwrap(),
        ])
        .unwrap();

        let decision = policy.evaluate(&signature);
        assert!(decision.is_trusted());
        assert!(decision.failures().is_empty());
    }

    #[test]
    fn signer_trust_policy_rejects_metadata_drift_even_when_signature_is_valid() {
        let bundle = unsigned_bundle();
        let signature = SignatureRecord::new(
            bundle.unsigned_signing_digest().unwrap(),
            "issuer://unapproved",
            signing_key(),
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();
        let policy = SignerTrustPolicy::new(vec![
            SignerTrustRule::new(
                "issuer://release",
                "local-file",
                "release-signing",
                "v1",
                "release",
                SignatureAlgorithm::Ed25519,
            )
            .unwrap(),
        ])
        .unwrap();

        let decision = policy.evaluate(&signature);
        assert!(!decision.is_trusted());
        assert!(
            decision
                .failures()
                .contains(&SignerTrustFailure::NoMatchingRule)
        );
    }

    #[test]
    fn signer_trust_policy_rejects_revoked_key_fail_closed() {
        let bundle = unsigned_bundle();
        let mut key = signing_key();
        key.revoke().unwrap();
        let signature = SignatureRecord::new(
            bundle.unsigned_signing_digest().unwrap(),
            "issuer://release",
            key,
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();
        let policy = SignerTrustPolicy::new(vec![
            SignerTrustRule::new(
                "issuer://release",
                "local-file",
                "release-signing",
                "v1",
                "release",
                SignatureAlgorithm::Ed25519,
            )
            .unwrap(),
        ])
        .unwrap();

        let decision = policy.evaluate(&signature);
        assert!(!decision.is_trusted());
        assert!(
            decision
                .failures()
                .contains(&SignerTrustFailure::KeyNotUsable)
        );
    }
}
