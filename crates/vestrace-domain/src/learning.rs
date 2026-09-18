use crate::{
    DomainError, EvaluationFact, EvaluationResult, EvidenceRef,
    id::{
        AgentRevisionId, EvaluationId, LearningProjectionId, LearningProposalId, ModelId,
        PrincipalId, SkillRevisionId, ToolDefinitionId, WorkflowRevisionId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A learned result is an explicitly advisory projection over immutable raw facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionAuthority {
    Advisory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    PerformanceSummary,
    Recommendation,
    Trend,
    Anomaly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProjectionGenerator {
    Deterministic { algorithm_version: String },
    Model { model_id: ModelId, revision: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LearningTarget {
    AgentRevision { revision_id: AgentRevisionId },
    SkillRevision { revision_id: SkillRevisionId },
    WorkflowRevision { revision_id: WorkflowRevisionId },
    Model { model_id: ModelId },
    ToolDefinition { definition_id: ToolDefinitionId },
    RoutingPolicy { policy_key: String },
}

impl LearningTarget {
    fn validate(&self) -> Result<(), DomainError> {
        if let Self::RoutingPolicy { policy_key } = self {
            validate_non_blank(policy_key, "learning routing policy key")?;
        }
        Ok(())
    }
}

/// Changes are deliberately limited to versioned cognitive/routing assets.
/// Capability, permission, security-policy, and workspace-admin changes have no
/// representable variant in the L2 proposal contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LearningChange {
    AgentInstructions { instructions: String },
    SkillImplementation { implementation: serde_json::Value },
    WorkflowDefinition { definition: serde_json::Value },
    ModelRouting { configuration: serde_json::Value },
    ToolConfiguration { configuration: serde_json::Value },
}

impl LearningChange {
    fn validate(&self) -> Result<(), DomainError> {
        if let Self::AgentInstructions { instructions } = self {
            validate_non_blank(instructions, "agent instructions")?;
        }
        let payload = match self {
            Self::AgentInstructions { .. } => None,
            Self::SkillImplementation { implementation } => Some(implementation),
            Self::WorkflowDefinition { definition } => Some(definition),
            Self::ModelRouting { configuration } => Some(configuration),
            Self::ToolConfiguration { configuration } => Some(configuration),
        };
        if let Some(payload) = payload {
            validate_no_governance_keys(payload)?;
        }
        Ok(())
    }

    fn matches_target(&self, target: &LearningTarget) -> bool {
        matches!(
            (self, target),
            (
                Self::AgentInstructions { .. },
                LearningTarget::AgentRevision { .. }
            ) | (
                Self::SkillImplementation { .. },
                LearningTarget::SkillRevision { .. }
            ) | (
                Self::WorkflowDefinition { .. },
                LearningTarget::WorkflowRevision { .. }
            ) | (Self::ModelRouting { .. }, LearningTarget::Model { .. })
                | (
                    Self::ModelRouting { .. },
                    LearningTarget::RoutingPolicy { .. }
                )
                | (
                    Self::ToolConfiguration { .. },
                    LearningTarget::ToolDefinition { .. }
                )
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LearningProposalStatus {
    Draft,
    Submitted,
    Rejected,
    Withdrawn,
    /// The change was applied to its target, producing a new revision and an
    /// [`AppliedLearningChange`] that records what happened.
    Applied,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LearnedProjection {
    pub id: LearningProjectionId,
    pub workspace_id: WorkspaceId,
    pub kind: ProjectionKind,
    pub target: LearningTarget,
    pub generator: ProjectionGenerator,
    pub authority: ProjectionAuthority,
    pub source_generation: u32,
    pub source_evaluation_fact_ids: Vec<EvaluationId>,
    pub source_evidence_refs: Vec<EvidenceRef>,
    pub content: serde_json::Value,
    pub created_at: Timestamp,
}

impl LearnedProjection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: LearningProjectionId,
        workspace_id: WorkspaceId,
        kind: ProjectionKind,
        target: LearningTarget,
        generator: ProjectionGenerator,
        source_generation: u32,
        source_evaluation_fact_ids: Vec<EvaluationId>,
        source_evidence_refs: Vec<EvidenceRef>,
        content: serde_json::Value,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if source_generation == 0 {
            return Err(DomainError::InvalidArgument(
                "learned projection source_generation must be > 0".into(),
            ));
        }
        if source_evaluation_fact_ids.is_empty() {
            return Err(DomainError::InvalidArgument(
                "learned projection requires raw evaluation fact references".into(),
            ));
        }
        if source_evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "learned projection requires evidence provenance".into(),
            ));
        }
        for evidence_ref in &source_evidence_refs {
            if let EvidenceRef::EvaluationRef { evaluation_id } = evidence_ref {
                if !source_evaluation_fact_ids.contains(evaluation_id) {
                    return Err(DomainError::InvalidArgument(
                        "evaluation evidence ref must be declared in source_evaluation_fact_ids"
                            .into(),
                    ));
                }
            }
        }
        target.validate()?;
        match &generator {
            ProjectionGenerator::Deterministic { algorithm_version } => {
                validate_non_blank(algorithm_version, "projection algorithm version")?;
            }
            ProjectionGenerator::Model { revision, .. } => {
                validate_non_blank(revision, "projection generator revision")?;
            }
        }

        Ok(Self {
            id,
            workspace_id,
            kind,
            target,
            generator,
            authority: ProjectionAuthority::Advisory,
            source_generation,
            source_evaluation_fact_ids,
            source_evidence_refs,
            content,
            created_at,
        })
    }

    /// Rebuild a projection using only canonical evaluation facts.
    ///
    /// The rebuild contract is deliberately deterministic: facts are ordered by
    /// their stable IDs, only a deterministic generator is accepted, and the
    /// generated content is a versioned aggregate of raw measurements. This
    /// keeps derived learning state rebuildable without treating it as a new
    /// source of truth.
    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_from_facts(
        id: LearningProjectionId,
        workspace_id: WorkspaceId,
        kind: ProjectionKind,
        target: LearningTarget,
        algorithm_version: impl Into<String>,
        source_generation: u32,
        facts: &[EvaluationFact],
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if facts.is_empty() {
            return Err(DomainError::InvalidArgument(
                "learned projection rebuild requires canonical evaluation facts".into(),
            ));
        }

        let algorithm_version = algorithm_version.into();
        validate_non_blank(&algorithm_version, "projection algorithm version")?;

        let mut ordered_facts = facts.to_vec();
        ordered_facts.sort_by_key(|fact| fact.id.to_string());
        if ordered_facts
            .windows(2)
            .any(|pair| pair[0].id == pair[1].id)
        {
            return Err(DomainError::InvalidArgument(
                "learned projection rebuild cannot contain duplicate evaluation facts".into(),
            ));
        }

        for fact in &ordered_facts {
            fact.validate()?;
            if fact.workspace_id != workspace_id {
                return Err(DomainError::PolicyViolation(
                    "learned projection rebuild cannot cross workspace boundaries".into(),
                ));
            }
        }

        let source_evaluation_fact_ids = ordered_facts.iter().map(|fact| fact.id).collect();
        let source_evidence_refs = ordered_facts
            .iter()
            .flat_map(|fact| fact.evidence_refs.iter().cloned())
            .collect();

        let highest_authority = ordered_facts
            .iter()
            .map(|fact| fact.authority.priority())
            .max()
            .expect("non-empty facts were checked above");
        let contributing_facts: Vec<&EvaluationFact> = ordered_facts
            .iter()
            .filter(|fact| fact.authority.priority() == highest_authority)
            .collect();
        let ignored_lower_authority_fact_ids: Vec<String> = ordered_facts
            .iter()
            .filter(|fact| fact.authority.priority() < highest_authority)
            .map(|fact| fact.id.to_string())
            .collect();

        let mut result_counts =
            BTreeMap::from([("fail", 0_u64), ("inconclusive", 0_u64), ("pass", 0_u64)]);
        let mut metric_totals: BTreeMap<(String, Option<String>), (u64, f64)> = BTreeMap::new();
        for fact in &contributing_facts {
            let result_key = match fact.result {
                EvaluationResult::Pass => "pass",
                EvaluationResult::Fail => "fail",
                EvaluationResult::Inconclusive => "inconclusive",
            };
            *result_counts
                .get_mut(result_key)
                .expect("result key is predefined") += 1;

            let entry = metric_totals
                .entry((fact.metric.kind.clone(), fact.metric.unit.clone()))
                .or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += fact.metric.value;
            if !entry.1.is_finite() {
                return Err(DomainError::InvalidArgument(
                    "learned projection rebuild metric aggregate overflowed".into(),
                ));
            }
        }

        let metric_kinds: Vec<String> =
            metric_totals.keys().map(|(kind, _)| kind.clone()).collect();
        let mut metric_units = BTreeMap::new();
        let metric_averages: BTreeMap<String, f64> = metric_totals
            .into_iter()
            .map(|((kind, unit), (count, total))| {
                let key = match &unit {
                    Some(unit) => format!("{kind} [{unit}]"),
                    None => kind.clone(),
                };
                metric_units.insert(key.clone(), unit.unwrap_or_else(|| "unitless".to_owned()));
                let average = total / count as f64;
                (key, average)
            })
            .collect();
        let content = serde_json::json!({
            "algorithm_version": algorithm_version,
            "aggregated_fact_count": contributing_facts.len(),
            "fact_count": ordered_facts.len(),
            "ignored_lower_authority_fact_ids": ignored_lower_authority_fact_ids,
            "metric_averages": metric_averages,
            "metric_kinds": metric_kinds,
            "metric_units": metric_units,
            "result_counts": result_counts,
            "source_generation": source_generation,
            "aggregation_authority_tier": highest_authority,
        });

        Self::new(
            id,
            workspace_id,
            kind,
            target,
            ProjectionGenerator::Deterministic { algorithm_version },
            source_generation,
            source_evaluation_fact_ids,
            source_evidence_refs,
            content,
            created_at,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LearningProposal {
    pub id: LearningProposalId,
    pub workspace_id: WorkspaceId,
    pub source_projection_ids: Vec<LearningProjectionId>,
    pub source_evaluation_fact_ids: Vec<EvaluationId>,
    pub target: LearningTarget,
    pub change: LearningChange,
    pub expected_target_revision: u32,
    pub rationale: String,
    pub policy_version: String,
    pub created_by: PrincipalId,
    pub status: LearningProposalStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl LearningProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: LearningProposalId,
        workspace_id: WorkspaceId,
        source_projection_ids: Vec<LearningProjectionId>,
        source_evaluation_fact_ids: Vec<EvaluationId>,
        target: LearningTarget,
        change: LearningChange,
        expected_target_revision: u32,
        rationale: impl Into<String>,
        policy_version: impl Into<String>,
        created_by: PrincipalId,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if source_projection_ids.is_empty() || source_evaluation_fact_ids.is_empty() {
            return Err(DomainError::InvalidArgument(
                "learning proposal requires projection and raw fact provenance".into(),
            ));
        }
        if expected_target_revision == 0 {
            return Err(DomainError::InvalidArgument(
                "learning proposal expected_target_revision must be > 0".into(),
            ));
        }
        target.validate()?;
        change.validate()?;
        if !change.matches_target(&target) {
            return Err(DomainError::InvalidArgument(
                "learning proposal change does not match its target".into(),
            ));
        }
        let rationale = rationale.into();
        validate_non_blank(&rationale, "learning proposal rationale")?;
        let policy_version = policy_version.into();
        validate_non_blank(&policy_version, "learning proposal policy version")?;

        Ok(Self {
            id,
            workspace_id,
            source_projection_ids,
            source_evaluation_fact_ids,
            target,
            change,
            expected_target_revision,
            rationale,
            policy_version,
            created_by,
            status: LearningProposalStatus::Draft,
            created_at,
            updated_at: created_at,
        })
    }

    pub fn submit(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status != LearningProposalStatus::Draft {
            return Err(DomainError::PolicyViolation(format!(
                "cannot submit learning proposal in status {:?}",
                self.status
            )));
        }
        self.status = LearningProposalStatus::Submitted;
        self.updated_at = at;
        Ok(self)
    }

    /// Apply the proposal to its target, producing the record of what changed.
    ///
    /// # The two refusals
    ///
    /// Only a **submitted** proposal can be applied. A draft was never put
    /// forward and a rejected or withdrawn one was decided against; applying
    /// either would mean a change to a cognitive asset that nobody reviewed.
    ///
    /// The target must be on the revision the proposal expected. A proposal is
    /// written against a state of the world — these instructions, this routing
    /// configuration — and applying it to a target that has since moved would
    /// silently overwrite whatever happened in between. This is the same
    /// optimistic check `CognitiveMutation` makes for the same reason, and it
    /// names both revisions so the caller can re-read rather than guess.
    pub fn apply(
        mut self,
        current_target_revision: u32,
        approved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<(Self, AppliedLearningChange), DomainError> {
        if self.status != LearningProposalStatus::Submitted {
            return Err(DomainError::PolicyViolation(format!(
                "cannot apply learning proposal in status {:?}",
                self.status
            )));
        }
        if current_target_revision != self.expected_target_revision {
            // The same shape `CognitiveMutation` uses: both revisions named, so
            // the caller can re-read rather than guess.
            return Err(DomainError::RevisionConflict {
                expected: u64::from(self.expected_target_revision),
                current: u64::from(current_target_revision),
            });
        }

        let applied = AppliedLearningChange {
            id: crate::id::LearningChangeId::new(),
            workspace_id: self.workspace_id,
            proposal_id: self.id,
            target: self.target.clone(),
            change: self.change.clone(),
            from_revision: current_target_revision,
            to_revision: current_target_revision + 1,
            source_projection_ids: self.source_projection_ids.clone(),
            source_evaluation_fact_ids: self.source_evaluation_fact_ids.clone(),
            policy_version: self.policy_version.clone(),
            proposed_by: self.created_by,
            approved_by,
            applied_at: at,
        };

        self.status = LearningProposalStatus::Applied;
        self.updated_at = at;
        Ok((self, applied))
    }
}

fn validate_non_blank(value: &str, field: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        return Err(DomainError::InvalidArgument(format!(
            "{field} must not be blank"
        )));
    }
    Ok(())
}

fn validate_no_governance_keys(value: &serde_json::Value) -> Result<(), DomainError> {
    const FORBIDDEN_KEYS: &[&str] = &[
        "capability",
        "capabilities",
        "permission",
        "permissions",
        "requested_capabilities",
        "required_capabilities",
        "security_policy",
        "workspace_admin",
        "grants",
    ];

    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let normalized_key = key.to_ascii_lowercase();
                if FORBIDDEN_KEYS
                    .iter()
                    .any(|forbidden| normalized_key == *forbidden)
                    || normalized_key.contains("capabilit")
                    || normalized_key.contains("permission")
                    || normalized_key.contains("privilege")
                    || normalized_key.contains("grant")
                    || normalized_key.contains("elevation")
                {
                    return Err(DomainError::PolicyViolation(format!(
                        "learning change cannot modify governance field {key}"
                    )));
                }
                validate_no_governance_keys(child)?;
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                validate_no_governance_keys(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_change_cannot_target_a_different_asset_kind() {
        let result = LearningProposal::new(
            LearningProposalId::new(),
            WorkspaceId::new(),
            vec![LearningProjectionId::new()],
            vec![EvaluationId::new()],
            LearningTarget::SkillRevision {
                revision_id: SkillRevisionId::new(),
            },
            LearningChange::WorkflowDefinition {
                definition: serde_json::json!({}),
            },
            1,
            "test",
            "policy-v1",
            PrincipalId::new(),
            crate::now(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn proposal_change_rejects_nested_capability_fields() {
        let result = LearningProposal::new(
            LearningProposalId::new(),
            WorkspaceId::new(),
            vec![LearningProjectionId::new()],
            vec![EvaluationId::new()],
            LearningTarget::SkillRevision {
                revision_id: SkillRevisionId::new(),
            },
            LearningChange::SkillImplementation {
                implementation: serde_json::json!({
                    "implementation": {
                        "required_capabilities": ["workspace.admin"]
                    }
                }),
            },
            1,
            "test",
            "policy-v1",
            PrincipalId::new(),
            crate::now(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn projection_rejects_an_undeclared_evaluation_evidence_ref() {
        let result = LearnedProjection::new(
            LearningProjectionId::new(),
            WorkspaceId::new(),
            ProjectionKind::PerformanceSummary,
            LearningTarget::Model {
                model_id: ModelId::new(),
            },
            ProjectionGenerator::Deterministic {
                algorithm_version: "bench-v1".to_owned(),
            },
            1,
            vec![EvaluationId::new()],
            vec![EvidenceRef::EvaluationRef {
                evaluation_id: EvaluationId::new(),
            }],
            serde_json::json!({}),
            crate::now(),
        );

        assert!(result.is_err());
    }
}

/// What a learning proposal did when it was applied.
///
/// # Why this type exists
///
/// LRN-005 requires a change to a cognitive asset made by learning to be
/// versioned and provenanced, and a `LearningProposal` could only be drafted,
/// submitted, rejected or withdrawn. There was no way to apply one, so nothing
/// produced a versioned change and the requirement had nothing to be true or
/// false about.
///
/// A record of the application is not the same as the proposal. The proposal
/// says what somebody wanted; this says what the system did: which revision it
/// moved the target from and to, under which policy, by whose approval, and
/// carrying forward the projections and raw evaluation facts the proposal rested
/// on — so a later reader of the agent's instructions can find not only that
/// they changed but what measurements the change came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AppliedLearningChange {
    pub id: crate::id::LearningChangeId,
    pub workspace_id: WorkspaceId,
    pub proposal_id: LearningProposalId,
    pub target: LearningTarget,
    pub change: LearningChange,
    /// The revision the target was on when the change was applied.
    pub from_revision: u32,
    /// The revision it is on afterwards — always exactly one more, because a
    /// learning change is one step and a gap would be a step nobody recorded.
    pub to_revision: u32,
    pub source_projection_ids: Vec<LearningProjectionId>,
    pub source_evaluation_fact_ids: Vec<EvaluationId>,
    pub policy_version: String,
    pub proposed_by: PrincipalId,
    pub approved_by: PrincipalId,
    pub applied_at: Timestamp,
}

impl AppliedLearningChange {
    /// Whether this change carries provenance back to measurements.
    ///
    /// A change with no projections and no facts behind it is somebody editing
    /// an agent through the learning path, which is the thing the proposal's own
    /// constructor already refuses; this is the same guarantee, checkable on the
    /// applied record without going back to the proposal.
    pub fn is_provenanced(&self) -> bool {
        !self.source_projection_ids.is_empty() && !self.source_evaluation_fact_ids.is_empty()
    }
}
