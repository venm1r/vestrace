use crate::{
    id::{
        ArtifactId, ArtifactRevisionId, DerivationId, EvaluationId, EventId, MemoryId,
        MemoryRevisionId, MemorySourceId, ModelExecutionAttemptId, ToolInvocationId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    DirectSource,
    SupportingContext,
    ContradictingEvidence,
    Primary,
    Supporting,
    Contradicting,
    Contextual,
    DerivedFrom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRef {
    EventRef {
        event_id: EventId,
    },
    ArtifactRevisionRef {
        artifact_id: ArtifactId,
        revision_id: ArtifactRevisionId,
    },
    DocumentRef {
        uri: String,
        version: Option<String>,
    },
    MemoryRevisionRef {
        memory_id: MemoryId,
        revision_id: MemoryRevisionId,
    },
    /// A memory belonging to another workspace, reached through a share.
    ///
    /// Distinct from `MemoryRevisionRef` because the source workspace is the
    /// part that must not be lost: a derivation from mounted content that
    /// recorded only the memory and revision would look, a week later, exactly
    /// like a derivation from something local. The grant revision is here for
    /// the same reason — it says *under what* the content was reachable, which
    /// is the difference between a provenance record and a pair of identifiers.
    SharedMemoryRevisionRef {
        source_workspace_id: WorkspaceId,
        memory_id: MemoryId,
        revision_id: MemoryRevisionId,
        grant_revision_id: crate::id::MemoryShareGrantRevisionId,
        source_generation: String,
    },
    ModelExecutionRef {
        attempt_id: ModelExecutionAttemptId,
    },
    ToolResultRef {
        invocation_id: ToolInvocationId,
    },
    EvaluationRef {
        evaluation_id: EvaluationId,
    },
    ExternalEffectReceiptRef {
        receipt_id: String,
        receipt_uri: Option<String>,
    },
    HumanFeedbackRef {
        feedback_id: String,
        session_id: Option<String>,
    },
    FederatedEvidenceRef {
        federation_id: String,
        remote_evidence_id: String,
        remote_workspace_id: String,
    },
    ExternalReference {
        uri: String,
        label: Option<String>,
        integrity_hash: Option<String>,
    },
}

impl EvidenceRef {
    pub fn event(event_id: EventId) -> Self {
        Self::EventRef { event_id }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DerivationMethod {
    Extraction,
    Summarization,
    Inference,
    Consolidation,
    ConflictResolution,
    RuleBased {
        rule_id: String,
    },
    HumanAuthored,
    Import,
    FederatedDerivation,
    LlmExtraction {
        model: String,
        prompt_version: String,
    },
    ManualConsolidation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Derivation {
    pub id: DerivationId,
    pub workspace_id: WorkspaceId,
    pub method: DerivationMethod,
    pub input_refs: Vec<EvidenceRef>,
    pub output_ref: Option<EvidenceRef>,
    pub execution_ref: Option<EvidenceRef>,
    pub model_ref: Option<String>,
    pub policy_version: Option<String>,
    pub created_by: Option<crate::id::PrincipalId>,
    pub created_at: Timestamp,
}

impl Derivation {
    pub fn new(
        id: DerivationId,
        workspace_id: WorkspaceId,
        method: DerivationMethod,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            workspace_id,
            method,
            input_refs: Vec::new(),
            output_ref: None,
            execution_ref: None,
            model_ref: None,
            policy_version: None,
            created_by: None,
            created_at: at,
        }
    }

    pub fn with_input(mut self, evidence: EvidenceRef) -> Self {
        self.input_refs.push(evidence);
        self
    }

    pub fn with_output(mut self, evidence: EvidenceRef) -> Self {
        self.output_ref = Some(evidence);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemorySource {
    pub id: MemorySourceId,
    pub memory_id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub event_id: EventId,
    pub role: EvidenceRole,
    pub derivation_id: Option<DerivationId>,
    pub evidence_ref: Option<EvidenceRef>,
    pub created_at: Timestamp,
}

impl MemorySource {
    pub fn new_direct(
        id: MemorySourceId,
        memory_id: MemoryId,
        workspace_id: WorkspaceId,
        event_id: EventId,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            memory_id,
            workspace_id,
            event_id,
            role: EvidenceRole::DirectSource,
            derivation_id: None,
            evidence_ref: Some(EvidenceRef::event(event_id)),
            created_at: at,
        }
    }

    pub fn new_derived(
        id: MemorySourceId,
        memory_id: MemoryId,
        workspace_id: WorkspaceId,
        event_id: EventId,
        role: EvidenceRole,
        derivation_id: DerivationId,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            memory_id,
            workspace_id,
            event_id,
            role,
            derivation_id: Some(derivation_id),
            evidence_ref: Some(EvidenceRef::event(event_id)),
            created_at: at,
        }
    }

    pub fn with_evidence_ref(mut self, evidence_ref: EvidenceRef) -> Self {
        self.evidence_ref = Some(evidence_ref);
        self
    }
}
