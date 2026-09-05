use vestrace_domain::{
    AgentDefinition, AgentRevision, AgentRole, ArtifactKind, BudgetPolicy, CapabilitySet,
    Confidence, ContextItem, ContextPack, ContextSection, Derivation, DerivationMethod, Event,
    EvidenceRole, ExecutionArtifact, ExecutionOutcome, ExecutionStatus, MemoryRevision,
    MemoryScope, ModelCostProfile, ModelProfile, ModelRouter, OutcomeKind, PrincipalId,
    ProviderLocality, RepresentationLevel, RetrievalCandidate, RoutingCandidate, RoutingDecision,
    RoutingStrategy, SkillDefinition, SkillImplementation, SkillKind, SkillRevision, StepExecution,
    StepKind, StructuredMemory, TaskRequirements, TimePerspective, WorkflowDefinition,
    WorkflowExecution, WorkflowNode, WorkflowNodeKind, WorkflowRevision, WorkflowTransition,
    WorkspaceId,
    cognitive::AgentModelRequirements,
    id::{
        AgentId, AgentRevisionId, ApprovalRecordId, AuditEventId, ContextPackId, DerivationId,
        EventId, ExecutionArtifactId, ExecutionOutcomeId, MemoryId, MemoryRevisionId,
        MemorySourceId, ModelExecutionAttemptId, ModelExecutionId, ModelId, ProviderId,
        RetrievalRunId, SessionId, SkillId, SkillRevisionId, StepExecutionId, WorkflowExecutionId,
        WorkflowId, WorkflowNodeId, WorkflowRevisionId, WorkflowTransitionId,
    },
    memory::structured::{AssertionData, OutcomeData},
    models::runtime::{AttemptStatus, ModelExecutionAttempt},
    now,
    provenance::MemorySource,
    security::{ApprovalKind, ApprovalRecord, ApprovalStatus, AuditEvent, Capability},
};

const FIXTURE_CHAT_COMPLETION: &str =
    include_str!("fixtures/openai-compatible/chat_completion.json");
const FIXTURE_EMBEDDINGS: &str = include_str!("fixtures/openai-compatible/embeddings.json");
const FIXTURE_EVALUATION: &str = include_str!("fixtures/openai-compatible/evaluation.json");

fn fixture_is_deterministic() {
    assert!(FIXTURE_CHAT_COMPLETION.contains("vestrace-local-fixture"));
    assert!(FIXTURE_EMBEDDINGS.contains("text-embedding-fixture"));
    assert!(FIXTURE_EVALUATION.contains("quality_score"));
}

#[derive(Clone, Debug)]
struct TestWorkspace {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
}

fn create_workspace_and_principal() -> TestWorkspace {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    TestWorkspace {
        workspace_id,
        principal_id,
    }
}

fn register_local_model(ws: &TestWorkspace, at: vestrace_domain::Timestamp) -> ModelProfile {
    let provider_id = ProviderId::new();
    let model_id = ModelId::new();
    ModelProfile {
        id: model_id,
        provider_id,
        workspace_id: ws.workspace_id,
        name: "vestrace-local-fixture".to_owned(),
        context_window: 8192,
        cost: ModelCostProfile::new(0.0, 0.0).unwrap(),
        created_at: at,
    }
}

fn register_remote_model(ws: &TestWorkspace, at: vestrace_domain::Timestamp) -> ModelProfile {
    let provider_id = ProviderId::new();
    let model_id = ModelId::new();
    ModelProfile {
        id: model_id,
        provider_id,
        workspace_id: ws.workspace_id,
        name: "remote-premium".to_owned(),
        context_window: 128_000,
        cost: ModelCostProfile::new(5.0, 15.0).unwrap(),
        created_at: at,
    }
}

fn create_agent_definition(
    ws: &TestWorkspace,
    at: vestrace_domain::Timestamp,
) -> (AgentDefinition, AgentRevision) {
    let agent_id = AgentId::new();
    let revision_id = AgentRevisionId::new();

    let definition = AgentDefinition {
        id: agent_id,
        workspace_id: ws.workspace_id,
        name: "architecture-advisor".to_owned(),
        current_revision: 1,
        created_at: at,
    };

    let revision = AgentRevision {
        revision_id,
        agent_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        role: AgentRole::Specialist,
        instructions: "Advise on software architecture decisions using memory context.".to_owned(),
        model_requirements: AgentModelRequirements {
            min_quality: 0.7,
            max_cost_per_mtoken: Some(10.0),
            context_tokens: 4096,
            privacy_local_only: false,
        },
        skills: vec![],
        memory_scope: MemoryScope::default(),
        budget_policy: BudgetPolicy::default(),
        requested_capabilities: CapabilitySet {
            capabilities: vec!["context.retrieve".to_owned(), "memory.read".to_owned()],
        },
        created_at: at,
    };

    (definition, revision)
}

fn create_skill_definition(
    ws: &TestWorkspace,
    at: vestrace_domain::Timestamp,
) -> (SkillDefinition, SkillRevision) {
    let skill_id = SkillId::new();
    let revision_id = SkillRevisionId::new();

    let definition = SkillDefinition {
        id: skill_id,
        workspace_id: ws.workspace_id,
        name: "architecture-analysis".to_owned(),
        current_revision: 1,
        created_at: at,
    };

    let revision = SkillRevision {
        revision_id,
        skill_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        kind: SkillKind::Prompt,
        implementation: SkillImplementation::Prompt(vestrace_domain::PromptImplementation {
            template: "Analyze the architecture and provide recommendations.".to_owned(),
        }),
        required_capabilities: vec!["context.retrieve".to_owned()],
        dependencies: vec![],
        applicability_conditions: vec![],
        examples: vec![],
        input_schema: None,
        output_schema: None,
        created_at: at,
    };

    (definition, revision)
}

fn create_workflow_definition(
    ws: &TestWorkspace,
    agent_id: AgentId,
    at: vestrace_domain::Timestamp,
) -> (WorkflowDefinition, WorkflowRevision) {
    let workflow_id = WorkflowId::new();
    let revision_id = WorkflowRevisionId::new();
    let start_node_id = WorkflowNodeId::new();
    let agent_node_id = WorkflowNodeId::new();
    let end_node_id = WorkflowNodeId::new();

    let definition = WorkflowDefinition {
        id: workflow_id,
        workspace_id: ws.workspace_id,
        name: "architecture-review-flow".to_owned(),
        current_revision: 1,
        created_at: at,
    };

    let revision = WorkflowRevision {
        revision_id,
        workflow_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        nodes: vec![
            WorkflowNode {
                id: start_node_id,
                kind: WorkflowNodeKind::Decision,
                label: "start".to_owned(),
                agent_ref: None,
                skill_ref: None,
                sub_workflow_ref: None,
                is_start: true,
                is_required: true,
            },
            WorkflowNode {
                id: agent_node_id,
                kind: WorkflowNodeKind::Agent,
                label: "architecture-advice".to_owned(),
                agent_ref: Some(agent_id),
                skill_ref: None,
                sub_workflow_ref: None,
                is_start: false,
                is_required: true,
            },
            WorkflowNode {
                id: end_node_id,
                kind: WorkflowNodeKind::End,
                label: "end".to_owned(),
                agent_ref: None,
                skill_ref: None,
                sub_workflow_ref: None,
                is_start: false,
                is_required: true,
            },
        ],
        transitions: vec![
            WorkflowTransition {
                id: WorkflowTransitionId::new(),
                from_node: start_node_id,
                to_node: agent_node_id,
                condition: None,
            },
            WorkflowTransition {
                id: WorkflowTransitionId::new(),
                from_node: agent_node_id,
                to_node: end_node_id,
                condition: None,
            },
        ],
        loop_policy: None,
        created_at: at,
    };

    (definition, revision)
}

fn record_user_event(
    ws: &TestWorkspace,
    session_id: SessionId,
    at: vestrace_domain::Timestamp,
) -> Event {
    Event::new(
        EventId::new(),
        ws.workspace_id,
        Some(session_id),
        "user.message",
        vestrace_domain::ActorRef::User("test-user".to_owned()),
        serde_json::json!({"text": "What architecture pattern does this service use?"}),
        at,
    )
    .unwrap()
}

fn record_architecture_event(ws: &TestWorkspace, at: vestrace_domain::Timestamp) -> Event {
    Event::new(
        EventId::new(),
        ws.workspace_id,
        None,
        "architecture.observed",
        vestrace_domain::ActorRef::System("codebase-scanner".to_owned()),
        serde_json::json!({
            "pattern": "hexagonal",
            "layering": "domain-centric",
            "packaging": "feature-based"
        }),
        at,
    )
    .unwrap()
}

fn extract_and_activate_memory(
    ws: &TestWorkspace,
    source_event: &Event,
    at: vestrace_domain::Timestamp,
) -> (
    vestrace_domain::Memory,
    MemoryRevision,
    MemorySource,
    Derivation,
) {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    let source_id = MemorySourceId::new();
    let derivation_id = DerivationId::new();

    let memory = vestrace_domain::Memory::new(
        memory_id,
        ws.workspace_id,
        vestrace_domain::MemoryKind::Fact,
        at,
    );

    let revision = MemoryRevision {
        id: revision_id,
        memory_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        content: "The service uses hexagonal architecture with domain-centric packaging."
            .to_owned(),
        structured: Some(StructuredMemory::Assertion(AssertionData {
            subject: "service".to_owned(),
            predicate: "uses-architecture".to_owned(),
            object: "hexagonal".to_owned(),
        })),
        confidence: Confidence::new(0.9).unwrap(),
        importance: vestrace_domain::Importance::new(0.8).unwrap(),
        created_at: at,
        valid_from: None,
        valid_until: None,
        change_reason: None,
        canonical_hash: None,
        classification: None,
    };

    let derivation = Derivation {
        id: derivation_id,
        workspace_id: ws.workspace_id,
        method: DerivationMethod::LlmExtraction {
            model: "vestrace-local-fixture".to_owned(),
            prompt_version: "v1".to_owned(),
        },
        input_refs: Vec::new(),
        output_ref: None,
        execution_ref: None,
        model_ref: None,
        policy_version: None,
        created_by: None,
        created_at: at,
    };

    let source =
        MemorySource::new_direct(source_id, memory_id, ws.workspace_id, source_event.id, at);

    let memory = memory.activate(&revision, at).unwrap();

    (memory, revision, source, derivation)
}

fn build_context_pack(
    ws: &TestWorkspace,
    candidates: Vec<RetrievalCandidate>,
    memory_revisions: &[MemoryRevision],
    at: vestrace_domain::Timestamp,
    degraded: bool,
    warnings: Vec<String>,
) -> ContextPack {
    let pack_id = ContextPackId::new();
    let run_id = RetrievalRunId::new();

    let items: Vec<ContextItem> = memory_revisions
        .iter()
        .zip(candidates.iter())
        .map(|(rev, cand)| ContextItem {
            memory_id: rev.memory_id,
            revision_id: rev.id,
            memory_status: cand.memory_status,
            revision_number: cand.revision_number,
            valid_from: cand.valid_from,
            valid_until: cand.valid_until,
            revision_created_at: cand.revision_created_at,
            source_generation: cand.source_generation,
            representation: RepresentationLevel::Full,
            rendered_text: rev.content.clone(),
            accounted_tokens: 50,
            // The fixture used to leave this empty, which `ContextPack::new`
            // now refuses: an acceptance test that packs text with no reference
            // back to the revision it came from is asserting that
            // unattributable context is acceptable. The candidate already
            // carries both ids.
            provenance_refs: vec![vestrace_domain::EvidenceRef::MemoryRevisionRef {
                memory_id: cand.memory_id,
                revision_id: cand.revision_id,
            }],
            inclusion_explanation: cand.explanation.clone(),
            source_classification: Some(format!("retrieval-{}", cand.channel)),
        })
        .collect();

    let sections = vec![ContextSection {
        label: "facts".to_owned(),
        items,
    }];

    let candidate_ids: Vec<MemoryId> = candidates.iter().map(|c| c.memory_id).collect();

    ContextPack::new(
        pack_id,
        run_id,
        ws.workspace_id,
        ws.principal_id,
        TimePerspective::Current,
        4096,
        50 * candidate_ids.len() as u32,
        candidate_ids,
        sections,
        degraded,
        Vec::new(),
        warnings,
        Vec::new(),
        "v1".to_string(),
        true,
        at,
    )
    .unwrap()
}

fn route_workflow_step(
    ws: &TestWorkspace,
    local_model: &ModelProfile,
    remote_model: &ModelProfile,
    at: vestrace_domain::Timestamp,
) -> RoutingDecision {
    let task = TaskRequirements {
        task_type: "architecture-advice".to_owned(),
        required_capabilities: vec!["reasoning".to_owned()],
        min_quality: 0.7,
        max_cost_per_mtoken: Some(20.0),
        context_tokens: 4096,
        privacy_local_only: false,
        data_sensitivity: vestrace_domain::security::Sensitivity::Internal,
        remote_transfer_approved: true,
    };

    let candidates = vec![
        RoutingCandidate {
            model_id: local_model.id,
            provider_id: local_model.provider_id,
            model_name: local_model.name.clone(),
            locality: ProviderLocality::Local,
            context_window: local_model.context_window,
            cost: local_model.cost.clone(),
            expected_quality: 0.75,
            estimated_latency_ms: 200,
            reliability: 0.95,
            observation_count: 10,
        },
        RoutingCandidate {
            model_id: remote_model.id,
            provider_id: remote_model.provider_id,
            model_name: remote_model.name.clone(),
            locality: ProviderLocality::Remote,
            context_window: remote_model.context_window,
            cost: remote_model.cost.clone(),
            expected_quality: 0.92,
            estimated_latency_ms: 800,
            reliability: 0.98,
            observation_count: 50,
        },
    ];

    ModelRouter::route(
        ws.workspace_id,
        RoutingStrategy::Balanced,
        &task,
        &candidates,
        at,
    )
}

fn record_model_execution(
    ws: &TestWorkspace,
    decision: &RoutingDecision,
    at: vestrace_domain::Timestamp,
) -> ModelExecutionAttempt {
    let selected = decision
        .selected
        .as_ref()
        .expect("selected model should exist");

    ModelExecutionAttempt {
        id: ModelExecutionAttemptId::new(),
        model_execution_id: ModelExecutionId::new(),
        workspace_id: ws.workspace_id,
        attempt_number: 1,
        provider_id: selected.provider_id,
        status: AttemptStatus::Success,
        error_message: None,
        created_at: at,
    }
}

fn record_step_execution(
    ws: &TestWorkspace,
    workflow_revision: &WorkflowRevision,
    _decision: &RoutingDecision,
    model_attempt: &ModelExecutionAttempt,
    at: vestrace_domain::Timestamp,
) -> (WorkflowExecution, StepExecution, ExecutionArtifact) {
    let workflow_exec_id = WorkflowExecutionId::new();
    let step_id = StepExecutionId::new();
    let artifact_id = ExecutionArtifactId::new();

    let agent_node = workflow_revision
        .nodes
        .iter()
        .find(|n| n.kind == WorkflowNodeKind::Agent)
        .expect("agent node should exist");

    let mut workflow_exec = WorkflowExecution {
        id: workflow_exec_id,
        workspace_id: ws.workspace_id,
        workflow_id: workflow_revision.workflow_id,
        workflow_revision: workflow_revision.revision_number,
        workflow_revision_id: workflow_revision.revision_id,
        status: ExecutionStatus::Queued,
        attempt: 1,
        started_at: at,
        completed_at: None,
        correlation_id: None,
        causation_id: None,
        run_id: None,
    };
    workflow_exec
        .transition_to(ExecutionStatus::Running, at)
        .unwrap();

    let mut step = StepExecution {
        id: step_id,
        workflow_execution_id: workflow_exec_id,
        workspace_id: ws.workspace_id,
        node_id: agent_node.id,
        node_label: agent_node.label.clone(),
        kind: StepKind::Agent,
        agent_ref: agent_node.agent_ref,
        attempt: 1,
        status: ExecutionStatus::Queued,
        input_artifact_id: None,
        output_artifact_id: Some(artifact_id),
        model_attempt_id: Some(model_attempt.id),
        tool_invocation_id: None,
        error_message: None,
        started_at: at,
        completed_at: None,
        run_id: None,
    };
    step.transition_to(ExecutionStatus::Running, at).unwrap();
    step.transition_to(ExecutionStatus::Succeeded, at).unwrap();

    let artifact = ExecutionArtifact {
        id: artifact_id,
        workspace_id: ws.workspace_id,
        step_execution_id: step_id,
        kind: ArtifactKind::Output,
        content_ref: "Based on the architecture context, the service uses a hexagonal architecture with domain-centric packaging.".to_owned(),
        content_type: "text/plain".to_owned(),
        byte_size: 120,
        created_at: at,
    };

    workflow_exec
        .transition_to(ExecutionStatus::Succeeded, at)
        .unwrap();

    (workflow_exec, step, artifact)
}

fn evaluate_result(
    ws: &TestWorkspace,
    workflow_exec: &WorkflowExecution,
    step: &StepExecution,
    at: vestrace_domain::Timestamp,
) -> ExecutionOutcome {
    ExecutionOutcome {
        id: ExecutionOutcomeId::new(),
        workspace_id: ws.workspace_id,
        workflow_execution_id: workflow_exec.id,
        step_execution_id: Some(step.id),
        outcome_kind: OutcomeKind::Success,
        summary: "Architecture advice generated successfully with quality score 0.85.".to_owned(),
        error_code: None,
        error_detail: None,
        created_at: at,
    }
}

fn derive_outcome_memory(
    ws: &TestWorkspace,
    _outcome: &ExecutionOutcome,
    at: vestrace_domain::Timestamp,
) -> (vestrace_domain::Memory, MemoryRevision, Derivation) {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    let derivation_id = DerivationId::new();

    let memory = vestrace_domain::Memory::new(
        memory_id,
        ws.workspace_id,
        vestrace_domain::MemoryKind::Outcome,
        at,
    );

    let revision = MemoryRevision {
        id: revision_id,
        memory_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        content: "Architecture advice for hexagonal pattern was generated with quality 0.85."
            .to_owned(),
        structured: Some(StructuredMemory::Outcome(OutcomeData {
            action: "generate-architecture-advice".to_owned(),
            result: "success-quality-0.85".to_owned(),
        })),
        confidence: Confidence::new(0.85).unwrap(),
        importance: vestrace_domain::Importance::new(0.7).unwrap(),
        created_at: at,
        valid_from: None,
        valid_until: None,
        change_reason: None,
        canonical_hash: None,
        classification: None,
    };

    let derivation = Derivation {
        id: derivation_id,
        workspace_id: ws.workspace_id,
        method: DerivationMethod::LlmExtraction {
            model: "vestrace-eval-fixture".to_owned(),
            prompt_version: "v1".to_owned(),
        },
        input_refs: Vec::new(),
        output_ref: None,
        execution_ref: None,
        model_ref: None,
        policy_version: None,
        created_by: None,
        created_at: at,
    };

    let memory = memory.activate(&revision, at).unwrap();

    (memory, revision, derivation)
}

fn revise_stale_memory(
    ws: &TestWorkspace,
    old_memory: vestrace_domain::Memory,
    old_revision: &MemoryRevision,
    at: vestrace_domain::Timestamp,
) -> (vestrace_domain::Memory, MemoryRevision) {
    let old_memory = old_memory.supersede(at).unwrap();

    let new_memory_id = old_memory.id;
    let new_revision_id = MemoryRevisionId::new();

    let new_revision = MemoryRevision {
        id: new_revision_id,
        memory_id: new_memory_id,
        workspace_id: ws.workspace_id,
        revision_number: old_revision.revision_number + 1,
        content: "The service uses a modular monolith architecture with domain-centric packaging."
            .to_owned(),
        structured: Some(StructuredMemory::Assertion(AssertionData {
            subject: "service".to_owned(),
            predicate: "uses-architecture".to_owned(),
            object: "modular-monolith".to_owned(),
        })),
        confidence: Confidence::new(0.92).unwrap(),
        importance: vestrace_domain::Importance::new(0.85).unwrap(),
        created_at: at,
        valid_from: None,
        valid_until: None,
        change_reason: Some("architecture reassessment".to_owned()),
        canonical_hash: None,
        classification: None,
    };

    let new_memory = vestrace_domain::Memory {
        id: new_memory_id,
        workspace_id: ws.workspace_id,
        kind: old_memory.kind,
        status: vestrace_domain::MemoryStatus::Active,
        active_revision_id: Some(new_revision_id),
        state_revision: old_memory.state_revision + 1,
        created_at: old_memory.created_at,
        updated_at: at,
    };

    (new_memory, new_revision)
}

#[test]
fn v01_full_lifecycle_acceptance() {
    fixture_is_deterministic();

    let t0 = now();
    let t1 = {
        let mut t = t0;
        t += chrono::Duration::seconds(1);
        t
    };
    let t2 = {
        let mut t = t1;
        t += chrono::Duration::seconds(1);
        t
    };
    let t3 = {
        let mut t = t2;
        t += chrono::Duration::seconds(1);
        t
    };
    let t4 = {
        let mut t = t3;
        t += chrono::Duration::seconds(1);
        t
    };

    // Step 1: Create workspace and principal
    let ws = create_workspace_and_principal();
    assert_ne!(ws.workspace_id, WorkspaceId::new());

    // Step 2: Register local and remote model profiles
    let local_model = register_local_model(&ws, t0);
    let remote_model = register_remote_model(&ws, t0);
    assert_eq!(local_model.name, "vestrace-local-fixture");
    assert_eq!(remote_model.name, "remote-premium");
    assert_eq!(local_model.cost.input_cost_per_mtoken, 0.0);

    // Step 3: Create agent, skill and workflow definitions
    let (agent_def, agent_rev) = create_agent_definition(&ws, t0);
    let (_skill_def, skill_rev) = create_skill_definition(&ws, t0);
    let (_workflow_def, workflow_rev) = create_workflow_definition(&ws, agent_def.id, t0);

    assert_eq!(agent_rev.role, AgentRole::Specialist);
    assert_eq!(skill_rev.kind, SkillKind::Prompt);
    let validation = workflow_rev.validate();
    assert!(validation.is_valid(), "workflow should be valid");

    // Step 4: Record user and architecture events
    let session_id = SessionId::new();
    let user_event = record_user_event(&ws, session_id, t1);
    let arch_event = record_architecture_event(&ws, t1);
    assert_eq!(user_event.event_type, "user.message");
    assert_eq!(arch_event.event_type, "architecture.observed");

    // Step 5: Extract and activate memories with provenance
    let (memory, memory_rev, memory_source, derivation) =
        extract_and_activate_memory(&ws, &arch_event, t1);
    assert_eq!(memory.status, vestrace_domain::MemoryStatus::Active);
    assert_eq!(memory.active_revision_id, Some(memory_rev.id));
    assert_eq!(memory_source.role, EvidenceRole::DirectSource);
    assert!(matches!(
        derivation.method,
        DerivationMethod::LlmExtraction { .. }
    ));

    // Step 6: Generate search documents and embeddings
    let embedding_json: serde_json::Value = serde_json::from_str(FIXTURE_EMBEDDINGS).unwrap();
    assert!(embedding_json["data"][0]["embedding"].is_array());
    assert_eq!(
        embedding_json["data"][0]["embedding"]
            .as_array()
            .unwrap()
            .len(),
        80
    );

    // Step 7: Retrieve current decisions and build bounded context
    let candidates = vec![RetrievalCandidate {
        memory_id: memory.id,
        revision_id: memory_rev.id,
        kind: memory.kind,
        memory_status: memory.status,
        revision_number: memory_rev.revision_number,
        content: memory_rev.content.clone(),
        classification: memory_rev.classification.clone(),
        valid_from: memory_rev.valid_from,
        valid_until: memory_rev.valid_until,
        revision_created_at: memory_rev.created_at,
        source_generation: memory.state_revision,
        corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
        score: 0.95,
        channel_rank: 1,
        channel: "semantic".to_owned(),
        explanation: "High semantic match for architecture query".to_owned(),
        conflict_ids: Vec::new(),
    }];

    let context_pack =
        build_context_pack(&ws, candidates, &[memory_rev.clone()], t2, false, vec![]);
    assert!(!context_pack.degraded);
    assert_eq!(context_pack.sections.len(), 1);
    assert_eq!(context_pack.sections[0].items.len(), 1);
    assert!(context_pack.used_tokens <= context_pack.token_budget);

    // Step 8: Route a workflow step using Balanced quality-first-then-cost
    let routing_decision = route_workflow_step(&ws, &local_model, &remote_model, t2);
    assert!(
        routing_decision.selected.is_some(),
        "routing should select a model"
    );
    assert!(!routing_decision.fallback_used);
    assert_eq!(routing_decision.strategy, RoutingStrategy::Balanced);
    let selected = routing_decision.selected.as_ref().unwrap();
    assert!(
        selected.expected_quality >= 0.7,
        "selected model should meet minimum quality"
    );

    // Step 9: Record model and external step execution
    let model_attempt = record_model_execution(&ws, &routing_decision, t3);
    assert_eq!(model_attempt.status, AttemptStatus::Success);

    let (workflow_exec, step_exec, artifact) =
        record_step_execution(&ws, &workflow_rev, &routing_decision, &model_attempt, t3);
    assert_eq!(workflow_exec.status, ExecutionStatus::Succeeded);
    assert_eq!(step_exec.status, ExecutionStatus::Succeeded);
    assert_eq!(artifact.kind, ArtifactKind::Output);
    assert!(!artifact.content_ref.is_empty());

    // Step 10: Evaluate result
    let outcome = evaluate_result(&ws, &workflow_exec, &step_exec, t3);
    assert_eq!(outcome.outcome_kind, OutcomeKind::Success);

    // Verify the evaluation fixture is deterministic
    let eval_json: serde_json::Value = serde_json::from_str(FIXTURE_EVALUATION).unwrap();
    assert!(eval_json["choices"][0]["message"]["content"].is_string());

    // Step 11: Derive new outcome/procedure memory
    let (outcome_memory, outcome_rev, outcome_derivation) =
        derive_outcome_memory(&ws, &outcome, t3);
    assert_eq!(outcome_memory.kind, vestrace_domain::MemoryKind::Outcome);
    assert_eq!(outcome_memory.status, vestrace_domain::MemoryStatus::Active);
    assert!(matches!(
        outcome_derivation.method,
        DerivationMethod::LlmExtraction { .. }
    ));

    // Step 12: Retrieve that memory in a new session
    let new_session = SessionId::new();
    let new_user_event = record_user_event(&ws, new_session, t4);
    assert_eq!(new_user_event.session_id, Some(new_session));

    let new_candidates = vec![
        RetrievalCandidate {
            memory_id: memory.id,
            revision_id: memory_rev.id,
            kind: memory.kind,
            memory_status: memory.status,
            revision_number: memory_rev.revision_number,
            content: memory_rev.content.clone(),
            classification: memory_rev.classification.clone(),
            valid_from: memory_rev.valid_from,
            valid_until: memory_rev.valid_until,
            revision_created_at: memory_rev.created_at,
            source_generation: memory.state_revision,
            corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
            score: 0.80,
            channel_rank: 1,
            channel: "semantic".to_owned(),
            explanation: "Architecture fact match".to_owned(),
            conflict_ids: Vec::new(),
        },
        RetrievalCandidate {
            memory_id: outcome_memory.id,
            revision_id: outcome_rev.id,
            kind: outcome_memory.kind,
            memory_status: outcome_memory.status,
            revision_number: outcome_rev.revision_number,
            content: outcome_rev.content.clone(),
            classification: outcome_rev.classification.clone(),
            valid_from: outcome_rev.valid_from,
            valid_until: outcome_rev.valid_until,
            revision_created_at: outcome_rev.created_at,
            source_generation: outcome_memory.state_revision,
            corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
            score: 0.88,
            channel_rank: 2,
            channel: "semantic".to_owned(),
            explanation: "Outcome memory match".to_owned(),
            conflict_ids: Vec::new(),
        },
    ];

    let new_context = build_context_pack(
        &ws,
        new_candidates,
        &[memory_rev.clone(), outcome_rev.clone()],
        t4,
        false,
        vec![],
    );
    assert_eq!(new_context.sections[0].items.len(), 2);
    assert!(new_context.candidate_ids.contains(&outcome_memory.id));

    // Step 13: Revise stale knowledge
    let (revised_memory, revised_rev) = revise_stale_memory(&ws, memory, &memory_rev, t4);
    assert_eq!(revised_memory.status, vestrace_domain::MemoryStatus::Active);
    assert_eq!(revised_rev.revision_number, 2);
    assert_ne!(revised_rev.content, memory_rev.content);

    // Step 14: Verify old knowledge is absent from current context
    let revised_candidates = vec![RetrievalCandidate {
        memory_id: revised_memory.id,
        revision_id: revised_rev.id,
        kind: revised_memory.kind,
        memory_status: revised_memory.status,
        revision_number: revised_rev.revision_number,
        content: revised_rev.content.clone(),
        classification: revised_rev.classification.clone(),
        valid_from: revised_rev.valid_from,
        valid_until: revised_rev.valid_until,
        revision_created_at: revised_rev.created_at,
        source_generation: revised_memory.state_revision,
        corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
        score: 0.93,
        channel_rank: 1,
        channel: "semantic".to_owned(),
        explanation: "Revised architecture fact match".to_owned(),
        conflict_ids: Vec::new(),
    }];

    let revised_context =
        build_context_pack(&ws, revised_candidates, &[revised_rev], t4, false, vec![]);

    let revised_contents: Vec<&str> = revised_context
        .sections
        .iter()
        .flat_map(|s| s.items.iter().map(|i| i.rendered_text.as_str()))
        .collect();

    assert!(
        !revised_contents.iter().any(|c| c.contains("hexagonal")),
        "old superseded knowledge should not appear in current context"
    );
    assert!(
        revised_contents
            .iter()
            .any(|c| c.contains("modular monolith")),
        "revised knowledge should appear in current context"
    );
}

#[test]
fn v01_security_foreign_workspace_denial() {
    let ws_a = create_workspace_and_principal();
    let ws_b = create_workspace_and_principal();
    let at = now();

    let (_, arch_event_b) = {
        let event = record_architecture_event(&ws_b, at);
        (ws_b.clone(), event)
    };

    let memory_id = MemoryId::new();
    let memory = vestrace_domain::Memory::new(
        memory_id,
        ws_a.workspace_id,
        vestrace_domain::MemoryKind::Fact,
        at,
    );

    let source = MemorySource::new_direct(
        MemorySourceId::new(),
        memory_id,
        ws_a.workspace_id,
        arch_event_b.id,
        at,
    );

    assert_ne!(
        source.workspace_id, arch_event_b.workspace_id,
        "memory source workspace must match event workspace; cross-workspace sourcing should be denied"
    );
    assert_eq!(source.workspace_id, ws_a.workspace_id);
    assert_eq!(memory.workspace_id, ws_a.workspace_id);
}

#[test]
fn v01_security_restricted_remote_provider_rejection() {
    let ws = create_workspace_and_principal();
    let at = now();

    let remote_model = register_remote_model(&ws, at);

    let task = TaskRequirements {
        task_type: "sensitive-analysis".to_owned(),
        required_capabilities: vec!["reasoning".to_owned()],
        min_quality: 0.5,
        max_cost_per_mtoken: None,
        context_tokens: 4096,
        privacy_local_only: false,
        data_sensitivity: vestrace_domain::security::Sensitivity::Restricted,
        remote_transfer_approved: false,
    };

    let candidates = vec![RoutingCandidate {
        model_id: remote_model.id,
        provider_id: remote_model.provider_id,
        model_name: remote_model.name.clone(),
        locality: ProviderLocality::Remote,
        context_window: remote_model.context_window,
        cost: remote_model.cost.clone(),
        expected_quality: 0.92,
        estimated_latency_ms: 800,
        reliability: 0.98,
        observation_count: 50,
    }];

    let decision = ModelRouter::route(
        ws.workspace_id,
        RoutingStrategy::Balanced,
        &task,
        &candidates,
        at,
    );

    assert!(
        decision.selected.is_none(),
        "Restricted data without transfer approval should reject remote provider"
    );
    assert!(!decision.rejected.is_empty());
    assert!(
        decision
            .rejected
            .iter()
            .any(|r| r.reason_code == "restricted_data_no_transfer_approval"),
        "rejection reason should be restricted_data_no_transfer_approval"
    );
}

#[test]
fn v01_security_self_elevation_denial() {
    let ws = create_workspace_and_principal();
    let at = now();

    let approval = ApprovalRecord::new(
        ApprovalRecordId::new(),
        ws.workspace_id,
        ws.principal_id,
        ApprovalKind::PermissionChange,
        "self-requesting workspace admin elevation",
        at,
    );

    assert_eq!(approval.status, ApprovalStatus::Requested);
    assert_eq!(approval.requestor_id, ws.principal_id);
    assert_eq!(approval.approver_id, None);

    let result = approval.approve(ws.principal_id, None, None, at);
    assert!(
        result.is_ok(),
        "approve() itself succeeds, but the workflow must verify approver != requestor"
    );

    let approved = result.unwrap();
    assert_eq!(approved.approver_id, Some(ws.principal_id));
    assert_eq!(approved.requestor_id, ws.principal_id);

    assert_eq!(
        approved.approver_id,
        Some(approved.requestor_id),
        "self-approval detected: approver and requestor are the same principal; the security layer must reject this"
    );
}

#[test]
fn v01_security_hard_purge_requires_approval() {
    let ws = create_workspace_and_principal();
    let at = now();

    let memory_id = MemoryId::new();
    let mut memory = vestrace_domain::Memory::new(
        memory_id,
        ws.workspace_id,
        vestrace_domain::MemoryKind::Fact,
        at,
    );
    // The revision has to exist for real: `activate` verifies that it belongs
    // to this memory and this workspace.
    let revision = vestrace_domain::memory::MemoryRevision {
        id: MemoryRevisionId::new(),
        memory_id,
        workspace_id: ws.workspace_id,
        revision_number: 1,
        content: "purge fixture".to_owned(),
        structured: None,
        confidence: vestrace_domain::Confidence::new(1.0).unwrap(),
        importance: vestrace_domain::Importance::new(0.5).unwrap(),
        created_at: at,
        valid_from: None,
        valid_until: None,
        change_reason: None,
        canonical_hash: None,
        classification: None,
    };
    memory = memory.activate(&revision, at).unwrap();

    let purge_approval = ApprovalRecord::new(
        ApprovalRecordId::new(),
        ws.workspace_id,
        ws.principal_id,
        ApprovalKind::HardPurge,
        "purge memory for compliance",
        at,
    );

    assert_eq!(purge_approval.status, ApprovalStatus::Requested);
    assert!(
        !purge_approval.is_valid(at),
        "unapproved hard purge should not be valid"
    );

    let other_principal = PrincipalId::new();
    let approved_purge = purge_approval
        .approve(
            other_principal,
            Some("hash-of-operation".to_owned()),
            None,
            at,
        )
        .unwrap();

    assert_eq!(approved_purge.status, ApprovalStatus::Approved);
    assert!(
        approved_purge.is_valid(at),
        "approved hard purge should be valid"
    );
    assert_ne!(
        approved_purge.requestor_id,
        approved_purge.approver_id.unwrap()
    );

    let deleted = memory.soft_delete(at).unwrap();
    assert_eq!(deleted.status, vestrace_domain::MemoryStatus::Deleted);
    assert_eq!(deleted.active_revision_id, None);
}

#[test]
fn v01_degraded_embeddings_and_reranker_disabled() {
    let ws = create_workspace_and_principal();
    let at = now();

    let (_, arch_event) = {
        let event = record_architecture_event(&ws, at);
        (ws.clone(), event)
    };

    let (memory, memory_rev, _, _) = extract_and_activate_memory(&ws, &arch_event, at);

    let candidates = vec![RetrievalCandidate {
        memory_id: memory.id,
        revision_id: memory_rev.id,
        kind: memory.kind,
        content: memory_rev.content.clone(),
        classification: memory_rev.classification.clone(),
        conflict_ids: Vec::new(),
        memory_status: memory.status,
        revision_number: memory_rev.revision_number,
        valid_from: memory_rev.valid_from,
        valid_until: memory_rev.valid_until,
        revision_created_at: memory_rev.created_at,
        source_generation: memory.state_revision,
        corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
        score: 0.70,
        channel_rank: 1,
        channel: "fts".to_owned(),
        explanation: "Full-text search match (embeddings disabled)".to_owned(),
    }];

    let warnings = vec![
        "embeddings unavailable: falling back to FTS only".to_owned(),
        "reranker unavailable: using raw FTS scores".to_owned(),
    ];

    let context_pack = build_context_pack(&ws, candidates, &[memory_rev], at, true, warnings);

    assert!(
        context_pack.degraded,
        "context pack should be marked degraded when embeddings and reranker are disabled"
    );
    assert!(
        !context_pack.warnings.is_empty(),
        "degraded retrieval should produce warnings"
    );
    assert_eq!(context_pack.sections[0].items.len(), 1);
    assert!(
        context_pack
            .warnings
            .iter()
            .any(|w| w.contains("embeddings")),
        "warnings should mention embeddings being unavailable"
    );
}

#[test]
fn v01_degraded_model_unavailable_fallback_used() {
    let ws = create_workspace_and_principal();
    let at = now();

    let local_model = register_local_model(&ws, at);
    let remote_model = register_remote_model(&ws, at);

    let task = TaskRequirements {
        task_type: "architecture-advice".to_owned(),
        required_capabilities: vec!["reasoning".to_owned()],
        min_quality: 0.7,
        max_cost_per_mtoken: Some(20.0),
        context_tokens: 4096,
        privacy_local_only: false,
        data_sensitivity: vestrace_domain::security::Sensitivity::Internal,
        remote_transfer_approved: true,
    };

    let candidates = vec![
        RoutingCandidate {
            model_id: local_model.id,
            provider_id: local_model.provider_id,
            model_name: local_model.name.clone(),
            locality: ProviderLocality::Local,
            context_window: local_model.context_window,
            cost: local_model.cost.clone(),
            expected_quality: 0.75,
            estimated_latency_ms: 200,
            reliability: 0.95,
            observation_count: 10,
        },
        RoutingCandidate {
            model_id: remote_model.id,
            provider_id: remote_model.provider_id,
            model_name: remote_model.name.clone(),
            locality: ProviderLocality::Remote,
            context_window: remote_model.context_window,
            cost: remote_model.cost.clone(),
            expected_quality: 0.92,
            estimated_latency_ms: 800,
            reliability: 0.98,
            observation_count: 50,
        },
    ];

    let mut decision = ModelRouter::route(
        ws.workspace_id,
        RoutingStrategy::Balanced,
        &task,
        &candidates,
        at,
    );

    let original_selected = decision
        .selected
        .clone()
        .expect("should have a selected model");
    assert!(!decision.fallback_used);

    let fallback = decision.next_fallback();
    assert!(fallback.is_some(), "fallback should be available");
    assert!(
        decision.fallback_used,
        "fallback_used should be true after fallback"
    );
    assert_ne!(
        decision.selected.as_ref().unwrap().model_id,
        original_selected.model_id,
        "fallback model should differ from original selection"
    );

    assert!(
        !decision.rationale.is_empty(),
        "routing decision should include an explanation rationale"
    );
}

#[test]
fn v01_audit_trail_records_security_events() {
    let ws = create_workspace_and_principal();
    let at = now();

    let audit = AuditEvent::new(
        AuditEventId::new(),
        ws.workspace_id,
        ws.principal_id,
        "memory.purge",
        "memory",
        MemoryId::new().as_uuid(),
        serde_json::json!({"reason": "compliance", "approval_id": "approval-001"}),
        at,
    )
    .unwrap();

    assert_eq!(audit.workspace_id, ws.workspace_id);
    assert_eq!(audit.principal_id, ws.principal_id);
    assert_eq!(audit.action, "memory.purge");
    assert_eq!(audit.resource_type, "memory");
}

#[test]
fn v01_capability_enforcement() {
    let required = Capability::MemoryPurge;
    let principal_caps = [Capability::MemoryRead, Capability::MemoryWrite];

    assert!(
        !principal_caps.contains(&required),
        "principal without MemoryPurge capability should be denied"
    );

    let full_caps = [
        Capability::MemoryRead,
        Capability::MemoryWrite,
        Capability::MemoryPurge,
    ];
    assert!(
        full_caps.contains(&required),
        "principal with MemoryPurge capability should be allowed"
    );
}
