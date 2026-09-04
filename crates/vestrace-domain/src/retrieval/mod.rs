pub mod hydration;

pub use hydration::{
    ClassificationPolicy, HydratedRevision, HydrationOutcome, RevisionRef, WithheldRevision,
    WithholdingReason,
};

use crate::{
    DomainError, EvidenceRef, MemoryKind, MemoryStatus,
    id::{ContextPackId, MemoryId, MemoryRevisionId, PrincipalId, RetrievalRunId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalIntent {
    SemanticRecall,
    CurrentState,
    DecisionRecall,
    Timeline,
    TaskResume,
    ProcedureLookup,
    UserPreferences,
    ErrorRecovery,
    ModelSelection,
    WorkflowContext,
    Exploration,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimePerspective {
    Current,
    /// Validity-window reconstruction at the requested subject time.
    /// Known-as-of and reconstructed-with-late-evidence modes remain distinct
    /// future variants and must not be inferred from this one.
    AsOf(Timestamp),
    Timeline,
    AllHistory,
}

impl Default for TimePerspective {
    fn default() -> Self {
        Self::Current
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalChannel {
    Exact,
    FullText,
    Vector,
    Structured,
    Graph,
    ExecutionHistory,
}

impl std::fmt::Display for RetrievalChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::FullText => write!(f, "full_text"),
            Self::Vector => write!(f, "vector"),
            Self::Structured => write!(f, "structured"),
            Self::Graph => write!(f, "graph"),
            Self::ExecutionHistory => write!(f, "execution_history"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetrievalRequest {
    pub workspace_id: WorkspaceId,
    pub actor: PrincipalId,
    pub intent: RetrievalIntent,
    pub query: String,
    pub scopes: Vec<String>,
    pub temporal_perspective: TimePerspective,
    pub token_budget: u32,
    pub retrieval_policy_version: String,
    pub explanation_required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChannelResult {
    pub channel: RetrievalChannel,
    pub candidates: Vec<RetrievalCandidate>,
    pub degraded: bool,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetrievalCandidate {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
    pub kind: MemoryKind,
    pub memory_status: MemoryStatus,
    pub revision_number: u32,
    pub content: String,
    /// Set only after the exact revision has crossed the hydration policy
    /// boundary. Search adapters are finders, not classification authorities.
    pub classification: Option<String>,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub revision_created_at: Timestamp,
    pub source_generation: u32,
    pub score: f32,
    pub channel_rank: u32,
    pub channel: String,
    pub explanation: String,
    pub conflict_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreComponents {
    pub fused_rank: f32,
    pub scope_match: f32,
    pub kind_match: f32,
    pub importance: f32,
    pub confidence: f32,
    pub recency: f32,
    pub provenance_quality: f32,
    pub redundancy_penalty: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RepresentationLevel {
    Full,
    Summary,
    Atomic,
    Reference,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextItem {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
    pub memory_status: MemoryStatus,
    pub revision_number: u32,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub revision_created_at: Timestamp,
    pub source_generation: u32,
    pub representation: RepresentationLevel,
    pub rendered_text: String,
    pub accounted_tokens: u32,
    pub provenance_refs: Vec<EvidenceRef>,
    pub inclusion_explanation: String,
    pub source_classification: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConflictWarning {
    pub conflict_id: String,
    pub participant_memory_ids: Vec<MemoryId>,
    pub conflict_kind: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextSection {
    pub label: String,
    pub items: Vec<ContextItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextPack {
    pub id: ContextPackId,
    pub retrieval_run_id: RetrievalRunId,
    pub workspace_id: WorkspaceId,
    pub actor: PrincipalId,
    pub temporal_perspective: TimePerspective,
    pub token_budget: u32,
    pub used_tokens: u32,
    pub candidate_ids: Vec<MemoryId>,
    pub sections: Vec<ContextSection>,
    pub degraded: bool,
    pub degraded_channels: Vec<String>,
    pub warnings: Vec<String>,
    pub withheld: Vec<WithheldRevision>,
    pub conflict_warnings: Vec<ConflictWarning>,
    pub retrieval_policy_version: String,
    pub authorization_checked: bool,
    pub created_at: Timestamp,
}

impl ContextPack {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        actor: PrincipalId,
        temporal_perspective: TimePerspective,
        token_budget: u32,
        used_tokens: u32,
        candidate_ids: Vec<MemoryId>,
        sections: Vec<ContextSection>,
        degraded: bool,
        degraded_channels: Vec<String>,
        warnings: Vec<String>,
        conflict_warnings: Vec<ConflictWarning>,
        retrieval_policy_version: String,
        authorization_checked: bool,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if used_tokens > token_budget {
            return Err(DomainError::InvalidArgument(
                "used_tokens cannot exceed token_budget".into(),
            ));
        }
        if !authorization_checked {
            return Err(DomainError::PolicyViolation(
                "authorization must be checked before creating ContextPack".into(),
            ));
        }

        // Every included item must say where it came from.
        //
        // Nothing enforced this before, so a pack could carry text with no
        // reference back to the revision it was rendered from. That is the
        // difference between context a reader can check and context they have
        // to take on faith — and a model given unattributable context produces
        // unattributable output.
        for section in &sections {
            for item in &section.items {
                if item.provenance_refs.is_empty() {
                    return Err(DomainError::InvalidArgument(format!(
                        "context item for memory {} carries no provenance reference",
                        item.memory_id
                    )));
                }
                if item.revision_number == 0 {
                    return Err(DomainError::InvalidArgument(format!(
                        "context item for memory {} names no revision",
                        item.memory_id
                    )));
                }
            }
        }

        // The accounted tokens must add up. A per-item count that disagrees
        // with the total is how a budget is honoured in the summary and
        // exceeded in the payload.
        let accounted: u32 = sections
            .iter()
            .flat_map(|section| section.items.iter())
            .map(|item| item.accounted_tokens)
            .sum();
        if accounted != used_tokens {
            return Err(DomainError::InvalidArgument(format!(
                "used_tokens is {used_tokens} but the included items account for {accounted}"
            )));
        }
        Ok(Self {
            id,
            retrieval_run_id,
            workspace_id,
            actor,
            temporal_perspective,
            token_budget,
            used_tokens,
            candidate_ids,
            sections,
            degraded,
            degraded_channels,
            warnings,
            withheld: Vec::new(),
            conflict_warnings,
            retrieval_policy_version,
            authorization_checked,
            created_at: at,
        })
    }

    pub fn with_withholding(mut self, withheld: Vec<WithheldRevision>) -> Self {
        self.withheld = withheld;
        self
    }

    /// Record that one or more channels degraded during retrieval.
    ///
    /// A method rather than two public fields a caller sets by hand. The
    /// service used to assign `degraded` and `degraded_channels` directly after
    /// construction, so forgetting either line produced a pack that looked
    /// complete while some of its channels had failed — which is precisely the
    /// silent substitution the requirement forbids.
    ///
    /// The two cannot disagree here: naming channels marks the pack degraded,
    /// and there is no way to mark it degraded without naming them.
    pub fn with_degradation(
        mut self,
        degraded_channels: Vec<String>,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if degraded_channels.iter().any(|name| name.trim().is_empty()) {
            return Err(DomainError::InvalidArgument(
                "a degraded channel must be named".into(),
            ));
        }
        self.degraded = !degraded_channels.is_empty();
        self.degraded_channels = degraded_channels;
        self.warnings = warnings;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::PrincipalId;

    /// One item accounting for exactly `tokens`.
    ///
    /// The fixture used to pass no sections at all while claiming a non-zero
    /// `used_tokens`, which the accounting invariant now refuses: a pack that
    /// says it consumed a hundred tokens while carrying nothing is incoherent,
    /// and it is the shape a budget-honouring summary takes when the payload
    /// disagrees with it.
    fn sections_for(tokens: u32) -> Vec<ContextSection> {
        if tokens == 0 {
            return Vec::new();
        }
        let memory_id = MemoryId::new();
        let revision_id = MemoryRevisionId::new();
        vec![ContextSection {
            label: "facts".to_string(),
            items: vec![ContextItem {
                memory_id,
                revision_id,
                memory_status: MemoryStatus::Active,
                revision_number: 1,
                valid_from: None,
                valid_until: None,
                revision_created_at: crate::now(),
                source_generation: 1,
                representation: RepresentationLevel::Full,
                rendered_text: "rendered".to_string(),
                accounted_tokens: tokens,
                provenance_refs: vec![EvidenceRef::MemoryRevisionRef {
                    memory_id,
                    revision_id,
                }],
                inclusion_explanation: "fixture".to_string(),
                source_classification: None,
            }],
        }]
    }

    fn make_pack(used: u32, budget: u32) -> Result<ContextPack, DomainError> {
        ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            TimePerspective::Current,
            budget,
            used,
            Vec::new(),
            sections_for(used),
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "v1".to_string(),
            true,
            crate::now(),
        )
    }

    #[test]
    fn context_pack_rejects_used_tokens_above_budget() {
        let result = make_pack(101, 100);
        assert!(result.is_err());
    }

    #[test]
    fn context_pack_accepts_used_tokens_equal_to_budget() {
        let result = make_pack(100, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn context_pack_rejects_missing_authorization() {
        let result = ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            TimePerspective::Current,
            100,
            50,
            Vec::new(),
            Vec::new(),
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "v1".to_string(),
            false,
            crate::now(),
        );
        assert!(result.is_err());
    }
}
