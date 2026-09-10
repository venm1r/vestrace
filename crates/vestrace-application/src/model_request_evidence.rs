use async_trait::async_trait;
use vestrace_domain::{ModelRequestEvidenceId, WorkspaceId};

use crate::{ApplicationError, EffectiveModelRequest, Q1ProbeRequest, UnitOfWork};

pub const MODEL_REQUEST_EVIDENCE_CONFLICT: &str = "MODEL_REQUEST_EVIDENCE_CONFLICT";
pub const MODEL_REQUEST_EVIDENCE_INCOMPLETE: &str = "MODEL_REQUEST_EVIDENCE_INCOMPLETE";

/// Closed, non-content q1 source facts.  A q1 MRE is rebuilt from the pinned
/// manifest, target binding and this compact structural tuple; it never needs
/// to retain a rendered body, prompt, tool argument blob or provider body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1SafeMessageLayout {
    PlainText,
    MultipartImageMarker,
    AssistantToolCallReplay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1SafeToolChoice {
    None,
    NamedProbe,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1SafeResponseFormat {
    None,
    StrictNonceJsonSchema,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Q1AssistantToolCallReplay {
    call_id: String,
}

impl Q1AssistantToolCallReplay {
    pub fn new(call_id: impl Into<String>) -> Result<Self, ApplicationError> {
        let call_id = call_id.into();
        if call_id.trim().is_empty() || call_id.len() > 256 {
            return Err(ApplicationError::Policy(
                "q1 assistant tool-call identity is malformed".into(),
            ));
        }
        Ok(Self { call_id })
    }

    pub fn call_id(&self) -> &str {
        &self.call_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Q1MreSource {
    ordinal: String,
    message_layout: Q1SafeMessageLayout,
    tool_choice: Q1SafeToolChoice,
    parallel_tool_calls: bool,
    response_format: Q1SafeResponseFormat,
    stream: bool,
    stream_include_usage: bool,
    assistant_tool_call: Option<Q1AssistantToolCallReplay>,
}

impl Q1MreSource {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ordinal: impl Into<String>,
        message_layout: Q1SafeMessageLayout,
        tool_choice: Q1SafeToolChoice,
        parallel_tool_calls: bool,
        response_format: Q1SafeResponseFormat,
        stream: bool,
        stream_include_usage: bool,
        assistant_tool_call: Option<Q1AssistantToolCallReplay>,
    ) -> Result<Self, ApplicationError> {
        let ordinal = ordinal.into();
        let valid = match ordinal.as_str() {
            "10" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "20" => plain_nonstream(
                message_layout,
                tool_choice,
                parallel_tool_calls,
                response_format,
                stream,
                stream_include_usage,
                &assistant_tool_call,
            ),
            "30" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "35" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && stream
                    && stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "40" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::NamedProbe
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "50" => {
                message_layout == Q1SafeMessageLayout::AssistantToolCallReplay
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_some()
            }
            "60" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::Required
                    && parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "70" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::StrictNonceJsonSchema
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "80" => {
                message_layout == Q1SafeMessageLayout::MultipartImageMarker
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            "90" => {
                message_layout == Q1SafeMessageLayout::PlainText
                    && tool_choice == Q1SafeToolChoice::None
                    && !parallel_tool_calls
                    && response_format == Q1SafeResponseFormat::None
                    && !stream
                    && !stream_include_usage
                    && assistant_tool_call.is_none()
            }
            _ => false,
        };
        if !valid {
            return Err(ApplicationError::Policy(
                "q1 MRE source does not match its frozen ordinal".into(),
            ));
        }
        Ok(Self {
            ordinal,
            message_layout,
            tool_choice,
            parallel_tool_calls,
            response_format,
            stream,
            stream_include_usage,
            assistant_tool_call,
        })
    }

    pub fn ordinal(&self) -> &str {
        &self.ordinal
    }
    pub const fn message_layout(&self) -> Q1SafeMessageLayout {
        self.message_layout
    }
    pub const fn tool_choice(&self) -> Q1SafeToolChoice {
        self.tool_choice
    }
    pub const fn parallel_tool_calls(&self) -> bool {
        self.parallel_tool_calls
    }
    pub const fn response_format(&self) -> Q1SafeResponseFormat {
        self.response_format
    }
    pub const fn stream(&self) -> bool {
        self.stream
    }
    pub const fn stream_include_usage(&self) -> bool {
        self.stream_include_usage
    }
    pub fn assistant_tool_call(&self) -> Option<&Q1AssistantToolCallReplay> {
        self.assistant_tool_call.as_ref()
    }
}

fn plain_nonstream(
    message_layout: Q1SafeMessageLayout,
    tool_choice: Q1SafeToolChoice,
    parallel_tool_calls: bool,
    response_format: Q1SafeResponseFormat,
    stream: bool,
    stream_include_usage: bool,
    assistant_tool_call: &Option<Q1AssistantToolCallReplay>,
) -> bool {
    message_layout == Q1SafeMessageLayout::PlainText
        && tool_choice == Q1SafeToolChoice::None
        && !parallel_tool_calls
        && response_format == Q1SafeResponseFormat::None
        && !stream
        && !stream_include_usage
        && assistant_tool_call.is_none()
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRequestReconstructionStatus {
    Complete,
    Incomplete,
    Expired,
}

impl ModelRequestReconstructionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::Expired => "expired",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRequestCauseKind {
    RunStep,
    QualificationProbe,
    /// A governed embedding job. Like a run step it is bound to an immutable
    /// binding snapshot and carries no qualification target, which is exactly
    /// what `model_request_evidence_roots` has always required of this cause;
    /// only the vocabulary above the column was missing.
    EmbeddingJob,
}

impl ModelRequestCauseKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStep => "run_step",
            Self::QualificationProbe => "qualification_probe",
            Self::EmbeddingJob => "embedding_job",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRequestNodeKind {
    ConnectionRevision,
    ConnectionQualificationRevision,
    ModelRevision,
    ModelQualificationRevision,
    BindingSnapshot,
    QualificationTarget,
    ExternalEffect,
    QualificationProbe,
    GovernedInputMaterial,
    ToolSchemaRevision,
    SamplingRevision,
    LimitsRevision,
    RequestShapeRevision,
}

impl ModelRequestNodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectionRevision => "connection_revision",
            Self::ConnectionQualificationRevision => "connection_qualification_revision",
            Self::ModelRevision => "model_revision",
            Self::ModelQualificationRevision => "model_qualification_revision",
            Self::BindingSnapshot => "binding_snapshot",
            Self::QualificationTarget => "qualification_target",
            Self::ExternalEffect => "external_effect",
            Self::QualificationProbe => "qualification_probe",
            Self::GovernedInputMaterial => "governed_input_material",
            Self::ToolSchemaRevision => "tool_schema_revision",
            Self::SamplingRevision => "sampling_revision",
            Self::LimitsRevision => "limits_revision",
            Self::RequestShapeRevision => "request_shape_revision",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequestEvidenceNodeInput {
    pub kind: ModelRequestNodeKind,
    pub reference_id: uuid::Uuid,
    pub reference_version: Option<u64>,
    pub safe_ordinal: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RequestShapeRevisionInput {
    pub id: uuid::Uuid,
    pub version: u64,
    pub request_kind: crate::EffectiveRequestKind,
    pub stream: bool,
    pub input_roles: Vec<crate::EffectiveChatRole>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SamplingRevisionInput {
    pub id: uuid::Uuid,
    pub version: u64,
    pub sampling: crate::EffectiveSampling,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitsRevisionInput {
    pub id: uuid::Uuid,
    pub version: u64,
    pub limits: crate::EffectiveRequestLimits,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolSchemaRevisionInput {
    pub id: uuid::Uuid,
    pub version: u64,
    pub name: String,
    pub description: Option<String>,
    pub parameters_schema: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalRequestRevisions {
    pub request_shape: RequestShapeRevisionInput,
    pub sampling: Option<SamplingRevisionInput>,
    pub limits: LimitsRevisionInput,
    pub tools: Vec<ToolSchemaRevisionInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateModelRequestEvidence {
    root_id: uuid::Uuid,
    workspace_id: WorkspaceId,
    external_effect_id: uuid::Uuid,
    cause_kind: ModelRequestCauseKind,
    cause_id: uuid::Uuid,
    binding_snapshot_id: Option<uuid::Uuid>,
    qualification_target_binding_id: Option<uuid::Uuid>,
    canonical: CanonicalRequestRevisions,
    nodes: Vec<ModelRequestEvidenceNodeInput>,
    q1_source: Option<Q1MreSource>,
}

impl CreateModelRequestEvidence {
    pub fn new(
        identity: ModelRequestEvidenceIdentity,
        canonical: CanonicalRequestRevisions,
        nodes: Vec<ModelRequestEvidenceNodeInput>,
    ) -> Result<Self, ApplicationError> {
        let cause_is_valid = matches!(
            (
                identity.cause_kind,
                identity.binding_snapshot_id,
                identity.qualification_target_binding_id,
            ),
            (ModelRequestCauseKind::RunStep, Some(_), None)
                | (ModelRequestCauseKind::QualificationProbe, None, Some(_))
                | (ModelRequestCauseKind::EmbeddingJob, Some(_), None)
        );
        if !cause_is_valid || nodes.is_empty() || !canonical_nodes_are_exact(&canonical, &nodes) {
            return Err(ApplicationError::Policy(
                "model request evidence has an invalid cause or canonical node set".into(),
            ));
        }
        Ok(Self {
            root_id: identity.root_id,
            workspace_id: identity.workspace_id,
            external_effect_id: identity.external_effect_id,
            cause_kind: identity.cause_kind,
            cause_id: identity.cause_id,
            binding_snapshot_id: identity.binding_snapshot_id,
            qualification_target_binding_id: identity.qualification_target_binding_id,
            canonical,
            nodes,
            q1_source: None,
        })
    }

    /// Attaches the closed q1 structural source to exactly the qualification
    /// probe node that it describes.  The profile supplies all fixed request
    /// fields; the optional assistant replay identity is bounded metadata, not
    /// a retained tool-arguments/body copy.
    pub fn with_q1_source(mut self, source: Q1MreSource) -> Result<Self, ApplicationError> {
        let exact_probe_ordinal = self.nodes.iter().find_map(|node| {
            (node.kind == ModelRequestNodeKind::QualificationProbe)
                .then_some(node.safe_ordinal.as_deref())
                .flatten()
        });
        let expected_kind = match source.ordinal() {
            "10" => crate::EffectiveRequestKind::ModelsList,
            "90" => crate::EffectiveRequestKind::Embeddings,
            _ => crate::EffectiveRequestKind::ChatCompletions,
        };
        if self.cause_kind != ModelRequestCauseKind::QualificationProbe
            || exact_probe_ordinal != Some(source.ordinal())
            || self.canonical.request_shape.request_kind != expected_kind
        {
            return Err(ApplicationError::Policy(
                "q1 MRE source does not bind its exact qualification evidence node".into(),
            ));
        }
        self.q1_source = Some(source);
        Ok(self)
    }

    pub fn models_list_probe(
        root_id: uuid::Uuid,
        workspace_id: WorkspaceId,
        external_effect_id: uuid::Uuid,
        connection_revision_id: uuid::Uuid,
        qualification_target_binding_id: uuid::Uuid,
        qualification_job_id: uuid::Uuid,
        probe_ordinal: &str,
    ) -> Result<Self, ApplicationError> {
        vestrace_domain::QualificationProbeOrdinal::parse(probe_ordinal)
            .map_err(ApplicationError::from)?;
        let request_shape_id = uuid::Uuid::now_v7();
        let limits_id = uuid::Uuid::now_v7();
        let nodes = vec![
            node(ModelRequestNodeKind::ExternalEffect, external_effect_id),
            node(
                ModelRequestNodeKind::ConnectionRevision,
                connection_revision_id,
            ),
            node(
                ModelRequestNodeKind::QualificationTarget,
                qualification_target_binding_id,
            ),
            ModelRequestEvidenceNodeInput {
                kind: ModelRequestNodeKind::QualificationProbe,
                reference_id: qualification_job_id,
                reference_version: None,
                safe_ordinal: Some(probe_ordinal.to_owned()),
            },
            versioned_node(
                ModelRequestNodeKind::RequestShapeRevision,
                request_shape_id,
                1,
            ),
            versioned_node(ModelRequestNodeKind::LimitsRevision, limits_id, 1),
        ];
        Self::new(
            ModelRequestEvidenceIdentity {
                root_id,
                workspace_id,
                external_effect_id,
                cause_kind: ModelRequestCauseKind::QualificationProbe,
                cause_id: qualification_job_id,
                binding_snapshot_id: None,
                qualification_target_binding_id: Some(qualification_target_binding_id),
            },
            CanonicalRequestRevisions {
                request_shape: RequestShapeRevisionInput {
                    id: request_shape_id,
                    version: 1,
                    request_kind: crate::EffectiveRequestKind::ModelsList,
                    stream: false,
                    input_roles: Vec::new(),
                },
                sampling: None,
                limits: LimitsRevisionInput {
                    id: limits_id,
                    version: 1,
                    limits: crate::EffectiveRequestLimits::new(1, 1, 1).map_err(|error| {
                        ApplicationError::Internal(format!(
                            "fixed models-list evidence limits are invalid: {error}"
                        ))
                    })?,
                },
                tools: Vec::new(),
            },
            nodes,
        )
    }

    /// The evidence for one governed agent Run step.
    ///
    /// The three canonical revisions are authored here rather than looked up.
    /// Nothing in the pinned binding names them, and nothing should: they
    /// describe the shape of the request about to be sent, not a property of
    /// the binding. Migration 0183's creators are idempotent on `(id, value)`,
    /// so a replay carrying these same identities and values converges, while
    /// one whose values differ conflicts instead of rewriting the evidence.
    /// This mirrors [`Self::models_list_probe`], which authors its own shape
    /// and limits the same way.
    ///
    /// No parameter carries the step's confidential input. The input reaches
    /// the evidence only as the opaque `input_material_id` of its already
    /// sealed content material, which is what allows a downstream argument
    /// digest to be independent of the plaintext.
    #[allow(clippy::too_many_arguments)]
    pub fn for_governed_run_step(
        root_id: uuid::Uuid,
        workspace_id: WorkspaceId,
        external_effect_id: uuid::Uuid,
        binding_snapshot_id: uuid::Uuid,
        connection_revision_id: uuid::Uuid,
        connection_qualification_revision_id: uuid::Uuid,
        model_revision_id: uuid::Uuid,
        model_qualification_revision_id: uuid::Uuid,
        run_id: uuid::Uuid,
        input_material_id: uuid::Uuid,
        sampling: crate::EffectiveSampling,
        limits: crate::EffectiveRequestLimits,
    ) -> Result<Self, ApplicationError> {
        let request_shape_id = uuid::Uuid::now_v7();
        let sampling_id = uuid::Uuid::now_v7();
        let limits_id = uuid::Uuid::now_v7();
        let nodes = vec![
            node(ModelRequestNodeKind::ExternalEffect, external_effect_id),
            node(
                ModelRequestNodeKind::ConnectionRevision,
                connection_revision_id,
            ),
            node(
                ModelRequestNodeKind::ConnectionQualificationRevision,
                connection_qualification_revision_id,
            ),
            node(ModelRequestNodeKind::BindingSnapshot, binding_snapshot_id),
            node(ModelRequestNodeKind::ModelRevision, model_revision_id),
            node(
                ModelRequestNodeKind::ModelQualificationRevision,
                model_qualification_revision_id,
            ),
            versioned_node(
                ModelRequestNodeKind::RequestShapeRevision,
                request_shape_id,
                1,
            ),
            versioned_node(ModelRequestNodeKind::SamplingRevision, sampling_id, 1),
            versioned_node(ModelRequestNodeKind::LimitsRevision, limits_id, 1),
            // Migration 0183 requires ordered kinds to carry a contiguous
            // ordinal from zero. One governed input means exactly "0".
            ModelRequestEvidenceNodeInput {
                kind: ModelRequestNodeKind::GovernedInputMaterial,
                reference_id: input_material_id,
                reference_version: None,
                safe_ordinal: Some("0".to_owned()),
            },
        ];
        Self::new(
            ModelRequestEvidenceIdentity {
                root_id,
                workspace_id,
                external_effect_id,
                cause_kind: ModelRequestCauseKind::RunStep,
                cause_id: run_id,
                binding_snapshot_id: Some(binding_snapshot_id),
                qualification_target_binding_id: None,
            },
            CanonicalRequestRevisions {
                request_shape: RequestShapeRevisionInput {
                    id: request_shape_id,
                    version: 1,
                    request_kind: crate::EffectiveRequestKind::ChatCompletions,
                    // A governed agent step sends one user message and does not
                    // stream: the result is published atomically from a single
                    // durable receipt, which a streamed response cannot supply.
                    stream: false,
                    input_roles: vec![crate::EffectiveChatRole::User],
                },
                sampling: Some(SamplingRevisionInput {
                    id: sampling_id,
                    version: 1,
                    sampling,
                }),
                limits: LimitsRevisionInput {
                    id: limits_id,
                    version: 1,
                    limits,
                },
                tools: Vec::new(),
            },
            nodes,
        )
    }

    /// The evidence a governed embedding job is dispatched under.
    ///
    /// Shaped by what the durable authority already demands of an `embeddings`
    /// request: exactly one model revision, at least one governed input
    /// material, and no sampling or tool-schema revision, because an embedding
    /// request has no sampling to make and no tools to offer. That is why
    /// `sampling` is `None` here and present for a chat completion.
    ///
    /// The sources are the content materials being embedded, in order. They
    /// become the delivery source memberships that bind each projection to what
    /// it was computed from, so an empty list is refused rather than producing a
    /// job whose output depends on nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn for_embedding_job(
        root_id: uuid::Uuid,
        workspace_id: WorkspaceId,
        external_effect_id: uuid::Uuid,
        binding_snapshot_id: uuid::Uuid,
        connection_revision_id: uuid::Uuid,
        connection_qualification_revision_id: uuid::Uuid,
        model_revision_id: uuid::Uuid,
        model_qualification_revision_id: uuid::Uuid,
        embedding_job_id: uuid::Uuid,
        source_material_ids: &[uuid::Uuid],
        limits: crate::EffectiveRequestLimits,
    ) -> Result<Self, ApplicationError> {
        if source_material_ids.is_empty() {
            return Err(ApplicationError::Policy(
                "a governed embedding job requires at least one source material".to_owned(),
            ));
        }
        let request_shape_id = uuid::Uuid::now_v7();
        let limits_id = uuid::Uuid::now_v7();
        let mut nodes = vec![
            node(ModelRequestNodeKind::ExternalEffect, external_effect_id),
            node(
                ModelRequestNodeKind::ConnectionRevision,
                connection_revision_id,
            ),
            node(
                ModelRequestNodeKind::ConnectionQualificationRevision,
                connection_qualification_revision_id,
            ),
            node(ModelRequestNodeKind::BindingSnapshot, binding_snapshot_id),
            node(ModelRequestNodeKind::ModelRevision, model_revision_id),
            node(
                ModelRequestNodeKind::ModelQualificationRevision,
                model_qualification_revision_id,
            ),
            versioned_node(
                ModelRequestNodeKind::RequestShapeRevision,
                request_shape_id,
                1,
            ),
            versioned_node(ModelRequestNodeKind::LimitsRevision, limits_id, 1),
        ];
        // Migration 0183 requires ordered kinds to carry a contiguous ordinal
        // from zero, and the delivery authority reads exactly this order back
        // as the job's source ordinals.
        for (ordinal, material_id) in source_material_ids.iter().enumerate() {
            nodes.push(ModelRequestEvidenceNodeInput {
                kind: ModelRequestNodeKind::GovernedInputMaterial,
                reference_id: *material_id,
                reference_version: None,
                safe_ordinal: Some(ordinal.to_string()),
            });
        }
        Self::new(
            ModelRequestEvidenceIdentity {
                root_id,
                workspace_id,
                external_effect_id,
                cause_kind: ModelRequestCauseKind::EmbeddingJob,
                cause_id: embedding_job_id,
                binding_snapshot_id: Some(binding_snapshot_id),
                qualification_target_binding_id: None,
            },
            CanonicalRequestRevisions {
                request_shape: RequestShapeRevisionInput {
                    id: request_shape_id,
                    version: 1,
                    request_kind: crate::EffectiveRequestKind::Embeddings,
                    stream: false,
                    input_roles: Vec::new(),
                },
                sampling: None,
                limits: LimitsRevisionInput {
                    id: limits_id,
                    version: 1,
                    limits,
                },
                tools: Vec::new(),
            },
            nodes,
        )
    }

    pub const fn root_id(&self) -> uuid::Uuid {
        self.root_id
    }
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    pub const fn external_effect_id(&self) -> uuid::Uuid {
        self.external_effect_id
    }
    pub const fn cause_kind(&self) -> ModelRequestCauseKind {
        self.cause_kind
    }
    pub const fn cause_id(&self) -> uuid::Uuid {
        self.cause_id
    }
    pub const fn binding_snapshot_id(&self) -> Option<uuid::Uuid> {
        self.binding_snapshot_id
    }
    pub const fn qualification_target_binding_id(&self) -> Option<uuid::Uuid> {
        self.qualification_target_binding_id
    }
    pub fn canonical(&self) -> &CanonicalRequestRevisions {
        &self.canonical
    }
    pub fn nodes(&self) -> &[ModelRequestEvidenceNodeInput] {
        &self.nodes
    }

    pub fn q1_source(&self) -> Option<&Q1MreSource> {
        self.q1_source.as_ref()
    }
}

fn canonical_nodes_are_exact(
    canonical: &CanonicalRequestRevisions,
    nodes: &[ModelRequestEvidenceNodeInput],
) -> bool {
    let exact_versioned_count = |kind, id, version| {
        nodes
            .iter()
            .filter(|node| {
                node.kind == kind
                    && node.reference_id == id
                    && node.reference_version == Some(version)
            })
            .count()
            == 1
            && nodes.iter().filter(|node| node.kind == kind).count() == 1
    };
    if !exact_versioned_count(
        ModelRequestNodeKind::RequestShapeRevision,
        canonical.request_shape.id,
        canonical.request_shape.version,
    ) || !exact_versioned_count(
        ModelRequestNodeKind::LimitsRevision,
        canonical.limits.id,
        canonical.limits.version,
    ) {
        return false;
    }
    let sampling_is_exact = match &canonical.sampling {
        Some(sampling) => exact_versioned_count(
            ModelRequestNodeKind::SamplingRevision,
            sampling.id,
            sampling.version,
        ),
        None => nodes
            .iter()
            .all(|node| node.kind != ModelRequestNodeKind::SamplingRevision),
    };
    if !sampling_is_exact {
        return false;
    }
    let tool_nodes: Vec<_> = nodes
        .iter()
        .filter(|node| node.kind == ModelRequestNodeKind::ToolSchemaRevision)
        .collect();
    if tool_nodes.len() != canonical.tools.len()
        || tool_nodes
            .iter()
            .zip(&canonical.tools)
            .enumerate()
            .any(|(ordinal, (node, tool))| {
                node.reference_id != tool.id
                    || node.reference_version != Some(tool.version)
                    || node.safe_ordinal.as_deref() != Some(ordinal.to_string().as_str())
            })
    {
        return false;
    }
    let governed_inputs = nodes
        .iter()
        .filter(|node| node.kind == ModelRequestNodeKind::GovernedInputMaterial)
        .count();
    match canonical.request_shape.request_kind {
        crate::EffectiveRequestKind::ModelsList => {
            !canonical.request_shape.stream
                && canonical.request_shape.input_roles.is_empty()
                && canonical.sampling.is_none()
                && canonical.tools.is_empty()
                && governed_inputs == 0
        }
        crate::EffectiveRequestKind::ChatCompletions => {
            canonical.sampling.is_some()
                && !canonical.request_shape.input_roles.is_empty()
                && canonical.request_shape.input_roles.len() == governed_inputs
        }
        crate::EffectiveRequestKind::Embeddings => {
            !canonical.request_shape.stream
                && canonical.request_shape.input_roles.is_empty()
                && canonical.sampling.is_none()
                && canonical.tools.is_empty()
                && governed_inputs > 0
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRequestEvidenceIdentity {
    pub root_id: uuid::Uuid,
    pub workspace_id: WorkspaceId,
    pub external_effect_id: uuid::Uuid,
    pub cause_kind: ModelRequestCauseKind,
    pub cause_id: uuid::Uuid,
    pub binding_snapshot_id: Option<uuid::Uuid>,
    pub qualification_target_binding_id: Option<uuid::Uuid>,
}

fn node(kind: ModelRequestNodeKind, reference_id: uuid::Uuid) -> ModelRequestEvidenceNodeInput {
    ModelRequestEvidenceNodeInput {
        kind,
        reference_id,
        reference_version: None,
        safe_ordinal: None,
    }
}

fn versioned_node(
    kind: ModelRequestNodeKind,
    reference_id: uuid::Uuid,
    reference_version: u64,
) -> ModelRequestEvidenceNodeInput {
    ModelRequestEvidenceNodeInput {
        kind,
        reference_id,
        reference_version: Some(reference_version),
        safe_ordinal: None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissingModelRequestReference {
    pub kind: ModelRequestNodeKind,
    pub reference_id: uuid::Uuid,
    pub erasure_preparation_id: Option<uuid::Uuid>,
}

pub enum ModelRequestReconstruction {
    Complete(EffectiveModelRequest),
    Incomplete(Vec<MissingModelRequestReference>),
    Expired,
}

impl ModelRequestReconstruction {
    pub const fn status(&self) -> ModelRequestReconstructionStatus {
        match self {
            Self::Complete(_) => ModelRequestReconstructionStatus::Complete,
            Self::Incomplete(_) => ModelRequestReconstructionStatus::Incomplete,
            Self::Expired => ModelRequestReconstructionStatus::Expired,
        }
    }
}

#[async_trait]
pub trait ModelRequestEvidenceRepository: Send + Sync {
    async fn create_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        creation: &CreateModelRequestEvidence,
    ) -> Result<ModelRequestEvidenceId, ApplicationError>;

    async fn reconstruct_current_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
    ) -> Result<ModelRequestReconstruction, ApplicationError>;

    /// Qualification is deliberately reconstructed through a closed request
    /// vocabulary rather than the backwards-compatible effective-request
    /// carrier. Implementations that cannot prove the durable q1 tuple must
    /// refuse it before shared dispatch can reach an adapter.
    async fn reconstruct_q1_in(
        &self,
        _unit_of_work: &mut dyn UnitOfWork,
        _workspace_id: WorkspaceId,
        _evidence_id: ModelRequestEvidenceId,
    ) -> Result<Q1ProbeRequest, ApplicationError> {
        Err(ApplicationError::Policy(
            "qualification q1 reconstruction is unavailable for this evidence repository".into(),
        ))
    }
}

#[cfg(test)]
mod q1_source_tests {
    use super::*;

    #[test]
    fn q1_source_rejects_each_frozen_structural_mutation() {
        let replay = Q1AssistantToolCallReplay::new("call_q1").unwrap();
        assert!(
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                Some(replay.clone())
            )
            .is_ok()
        );

        for mutation in [
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                Some(replay.clone()),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::NamedProbe,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                Some(replay.clone()),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                true,
                Q1SafeResponseFormat::None,
                false,
                false,
                Some(replay.clone()),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::StrictNonceJsonSchema,
                false,
                false,
                Some(replay.clone()),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                true,
                false,
                Some(replay.clone()),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                true,
                Some(replay),
            ),
            Q1MreSource::new(
                "50",
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
        ] {
            assert!(mutation.is_err());
        }
    }

    #[test]
    fn q1_source_covers_multipart_tools_schema_and_stream_usage() {
        assert!(
            Q1MreSource::new(
                "40",
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::NamedProbe,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None
            )
            .is_ok()
        );
        assert!(
            Q1MreSource::new(
                "60",
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::Required,
                true,
                Q1SafeResponseFormat::None,
                false,
                false,
                None
            )
            .is_ok()
        );
        assert!(
            Q1MreSource::new(
                "70",
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::StrictNonceJsonSchema,
                false,
                false,
                None
            )
            .is_ok()
        );
        assert!(
            Q1MreSource::new(
                "80",
                Q1SafeMessageLayout::MultipartImageMarker,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None
            )
            .is_ok()
        );
        assert!(
            Q1MreSource::new(
                "35",
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                true,
                true,
                None
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod governed_run_step_evidence {
    use super::*;
    use crate::{EffectiveRequestLimits, EffectiveSampling};

    fn limits() -> EffectiveRequestLimits {
        EffectiveRequestLimits::new(256, 4, 32_768).unwrap()
    }

    fn sampling() -> EffectiveSampling {
        EffectiveSampling::new(0.2, 0.9).unwrap()
    }

    fn creation() -> Result<CreateModelRequestEvidence, ApplicationError> {
        CreateModelRequestEvidence::for_governed_run_step(
            uuid::Uuid::now_v7(),
            WorkspaceId::new(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            sampling(),
            limits(),
        )
    }

    /// The three canonical revisions are authored here, not selected. Nothing
    /// in the pinned binding names them, and nothing should: they describe the
    /// request about to be sent, so the command that records the evidence is
    /// the only honest place to allocate their identities.
    #[test]
    fn it_authors_the_three_canonical_revisions_at_version_one() {
        let creation = creation().expect("a governed Run step has complete evidence");
        let canonical = creation.canonical();

        assert_eq!(canonical.request_shape.version, 1);
        assert_eq!(
            canonical.request_shape.request_kind,
            crate::EffectiveRequestKind::ChatCompletions
        );
        assert!(!canonical.request_shape.stream);
        assert_eq!(canonical.limits.version, 1);
        assert_eq!(canonical.limits.limits, limits());

        let sampling_input = canonical
            .sampling
            .as_ref()
            .expect("a chat completion pins exactly one sampling revision");
        assert_eq!(sampling_input.version, 1);
        assert_eq!(sampling_input.sampling, sampling());
        assert!(
            canonical.tools.is_empty(),
            "a governed Run step declares no tool schema"
        );
    }

    /// Migration 0183 refuses a `chat_completions` root that does not carry
    /// exactly one of each of these, plus at least one governed input.
    #[test]
    fn it_carries_the_exact_node_set_a_chat_completion_requires() {
        let creation = creation().expect("a governed Run step has complete evidence");
        let count = |kind| {
            creation
                .nodes()
                .iter()
                .filter(|node| node.kind == kind)
                .count()
        };

        assert_eq!(count(ModelRequestNodeKind::ExternalEffect), 1);
        assert_eq!(count(ModelRequestNodeKind::ConnectionRevision), 1);
        assert_eq!(count(ModelRequestNodeKind::BindingSnapshot), 1);
        assert_eq!(count(ModelRequestNodeKind::ModelRevision), 1);
        // Migration 0183's run-step cause matrix requires exactly one of each.
        assert_eq!(
            count(ModelRequestNodeKind::ConnectionQualificationRevision),
            1
        );
        assert_eq!(count(ModelRequestNodeKind::ModelQualificationRevision), 1);
        assert_eq!(count(ModelRequestNodeKind::QualificationTarget), 0);
        assert_eq!(count(ModelRequestNodeKind::QualificationProbe), 0);
        assert_eq!(count(ModelRequestNodeKind::RequestShapeRevision), 1);
        assert_eq!(count(ModelRequestNodeKind::SamplingRevision), 1);
        assert_eq!(count(ModelRequestNodeKind::LimitsRevision), 1);
        assert_eq!(count(ModelRequestNodeKind::GovernedInputMaterial), 1);
        assert_eq!(
            creation
                .nodes()
                .iter()
                .find(|node| node.kind == ModelRequestNodeKind::GovernedInputMaterial)
                .and_then(|node| node.safe_ordinal.as_deref()),
            Some("0"),
            "an ordered kind must carry a contiguous ordinal from zero"
        );
        assert_eq!(
            count(ModelRequestNodeKind::ToolSchemaRevision),
            0,
            "an unexpected tool node would be refused by the guarded creator"
        );
    }

    /// The evidence root is bound to the Run that caused it and to the binding
    /// it was authorised against. `CreateModelRequestEvidence::new` refuses the
    /// other combination, so this pins which one a Run step uses.
    #[test]
    fn it_is_caused_by_the_run_and_bound_to_its_pinned_snapshot() {
        let root_id = uuid::Uuid::now_v7();
        let workspace_id = WorkspaceId::new();
        let external_effect_id = uuid::Uuid::now_v7();
        let snapshot_id = uuid::Uuid::now_v7();
        let run_id = uuid::Uuid::now_v7();

        let creation = CreateModelRequestEvidence::for_governed_run_step(
            root_id,
            workspace_id,
            external_effect_id,
            snapshot_id,
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            run_id,
            uuid::Uuid::now_v7(),
            sampling(),
            limits(),
        )
        .expect("a governed Run step has complete evidence");

        assert_eq!(creation.root_id(), root_id);
        assert_eq!(creation.external_effect_id(), external_effect_id);
        assert_eq!(creation.cause_kind(), ModelRequestCauseKind::RunStep);
        assert_eq!(creation.cause_id(), run_id);
        assert_eq!(creation.binding_snapshot_id(), Some(snapshot_id));
        assert_eq!(creation.qualification_target_binding_id(), None);
    }

    /// The node identities must be the canonical ones. `canonical_nodes_are_exact`
    /// already refuses a mismatch, so this proves the constructor feeds it a
    /// consistent pair rather than two independently allocated sets.
    #[test]
    fn its_canonical_nodes_reference_the_revisions_it_authored() {
        let creation = creation().expect("a governed Run step has complete evidence");
        let canonical = creation.canonical();
        let referenced = |kind| {
            creation
                .nodes()
                .iter()
                .find(|node| node.kind == kind)
                .map(|node| (node.reference_id, node.reference_version))
        };

        assert_eq!(
            referenced(ModelRequestNodeKind::RequestShapeRevision),
            Some((canonical.request_shape.id, Some(1)))
        );
        assert_eq!(
            referenced(ModelRequestNodeKind::LimitsRevision),
            Some((canonical.limits.id, Some(1)))
        );
        assert_eq!(
            referenced(ModelRequestNodeKind::SamplingRevision),
            Some((canonical.sampling.as_ref().unwrap().id, Some(1)))
        );
    }

    /// Two accepted steps differing only in their confidential input must not
    /// differ here. The constructor never sees the input, which is what makes
    /// an input-independent argument digest possible downstream.
    #[test]
    fn it_never_accepts_the_confidential_input() {
        // A compile-time statement of the rule: every argument is an opaque id
        // or an already-validated request-shape value. If a future change adds
        // an input parameter, this call stops compiling.
        //
        // The argument count is the assertion. Collapsing these into a struct
        // to satisfy `too_many_arguments` would let a new field be added
        // without this test noticing, which is the one thing it exists to
        // prevent.
        #[allow(clippy::too_many_arguments)]
        fn signature_is_input_free(
            root_id: uuid::Uuid,
            workspace_id: WorkspaceId,
            external_effect_id: uuid::Uuid,
            binding_snapshot_id: uuid::Uuid,
            connection_revision_id: uuid::Uuid,
            connection_qualification_revision_id: uuid::Uuid,
            model_revision_id: uuid::Uuid,
            model_qualification_revision_id: uuid::Uuid,
            run_id: uuid::Uuid,
            input_material_id: uuid::Uuid,
            sampling: EffectiveSampling,
            limits: EffectiveRequestLimits,
        ) -> Result<CreateModelRequestEvidence, ApplicationError> {
            CreateModelRequestEvidence::for_governed_run_step(
                root_id,
                workspace_id,
                external_effect_id,
                binding_snapshot_id,
                connection_revision_id,
                connection_qualification_revision_id,
                model_revision_id,
                model_qualification_revision_id,
                run_id,
                input_material_id,
                sampling,
                limits,
            )
        }

        let _ = signature_is_input_free;
    }
}

#[cfg(test)]
mod embedding_job_evidence {
    use super::*;
    use crate::EffectiveRequestLimits;

    fn limits() -> EffectiveRequestLimits {
        EffectiveRequestLimits::new(256, 4, 32_768).unwrap()
    }

    fn build(sources: &[uuid::Uuid]) -> Result<CreateModelRequestEvidence, ApplicationError> {
        CreateModelRequestEvidence::for_embedding_job(
            uuid::Uuid::now_v7(),
            WorkspaceId::new(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            sources,
            limits(),
        )
    }

    fn count(evidence: &CreateModelRequestEvidence, kind: ModelRequestNodeKind) -> usize {
        evidence
            .nodes()
            .iter()
            .filter(|node| node.kind == kind)
            .count()
    }

    /// The shape `model_request_evidence_roots` and the delivery authority have
    /// always required of an `embeddings` request: one model revision, sources
    /// present, and no sampling or tool schema, because an embedding request
    /// makes no sampling choice and offers no tools.
    #[test]
    fn the_embedding_shape_is_what_the_durable_authority_requires() {
        let sources = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];
        let evidence = build(&sources).expect("a two-source embedding job is well formed");

        assert_eq!(evidence.cause_kind(), ModelRequestCauseKind::EmbeddingJob);
        assert_eq!(evidence.cause_kind().as_str(), "embedding_job");
        assert!(evidence.binding_snapshot_id().is_some());
        assert_eq!(count(&evidence, ModelRequestNodeKind::ModelRevision), 1);
        assert_eq!(count(&evidence, ModelRequestNodeKind::SamplingRevision), 0);
        assert_eq!(
            count(&evidence, ModelRequestNodeKind::ToolSchemaRevision),
            0
        );
        assert_eq!(
            count(&evidence, ModelRequestNodeKind::RequestShapeRevision),
            1
        );
        assert_eq!(count(&evidence, ModelRequestNodeKind::LimitsRevision), 1);
        assert_eq!(
            count(&evidence, ModelRequestNodeKind::GovernedInputMaterial),
            sources.len()
        );
    }

    /// The delivery authority reads these back as the job's source ordinals, so
    /// they must be contiguous from zero and in the order given.
    #[test]
    fn source_ordinals_are_contiguous_from_zero_in_the_order_supplied() {
        let sources = [
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
            uuid::Uuid::now_v7(),
        ];
        let evidence = build(&sources).expect("three sources are well formed");
        let ordered: Vec<(String, uuid::Uuid)> = evidence
            .nodes()
            .iter()
            .filter(|node| node.kind == ModelRequestNodeKind::GovernedInputMaterial)
            .map(|node| {
                (
                    node.safe_ordinal.clone().expect("an ordered kind"),
                    node.reference_id,
                )
            })
            .collect();
        assert_eq!(
            ordered,
            vec![
                ("0".to_owned(), sources[0]),
                ("1".to_owned(), sources[1]),
                ("2".to_owned(), sources[2]),
            ]
        );
    }

    /// A job whose output depends on nothing is refused here rather than at the
    /// delivery authority, which would report it as a malformed source tuple.
    #[test]
    fn an_embedding_job_without_a_source_is_refused() {
        let error = build(&[]).expect_err("no source is not a job");
        assert!(matches!(error, ApplicationError::Policy(_)), "{error:?}");
    }
}
