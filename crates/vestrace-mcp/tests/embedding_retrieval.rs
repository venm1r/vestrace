//! What an agent is told when the governed vector channel declines.
//!
//! MCP is the surface an autonomous caller actually uses, and it is the one
//! surface where a missing fact cannot be worked around: an agent cannot open a
//! console and look. So this proves the tool output over the *public* server --
//! a real `search_memories` call through the authorization boundary and the
//! real `RetrievalService` -- rather than over the private formatter, because
//! the formatter being right is not the same claim as the tool being right.
//!
//! Two things are asserted, and the second matters more than the first:
//!
//! - the closed degradation reason and the attempt it belongs to reach the
//!   caller, so an agent holding the retry capability can name a predecessor;
//! - nothing about the query, its vector, or a digest of either reaches the
//!   caller, in the one place in the system where a leak would be handed
//!   straight to a model.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_application::{
    AgentRecord, ApplicationError, EvaluationRecord, MemoryUseCases, NormalizedRetrievalRequest,
    NullExecutionHistoryRepository, RequestContext, RetrievalService, SkillRecord, TextRetriever,
    WorkflowDefinitionRecord, WorkflowRevisionRecord,
    embedding::{
        DegradedRetrievalAttempt, EmbeddingRetrievalDegradation, EmbeddingRetrievalJobClient,
        EmbeddingRetrievalOutcome,
    },
    retrieval::RevisionHydrator,
};
use vestrace_domain::{
    CapabilityGrant, CapabilityGrantSpec, EmbeddingJobId, PolicyDecision, PrincipalId, WorkspaceId,
    embedding::RetrievalGenerationChangedReason,
    evaluate_capability_grants,
    id::{CapabilityGrantId, PolicyDecisionId, RetrievalRunId},
    retrieval::{ClassificationPolicy, HydratedRevision, RevisionRef},
};
use vestrace_mcp::McpServer;

/// Answers every attempt with one stated outcome.
struct FixedRetrievalClient(EmbeddingRetrievalOutcome);

#[async_trait]
impl EmbeddingRetrievalJobClient for FixedRetrievalClient {
    async fn retrieve(
        &self,
        _context: &RequestContext,
        _request_id: RetrievalRunId,
        _request: &NormalizedRetrievalRequest,
        _deadline: vestrace_domain::time::Timestamp,
    ) -> Result<EmbeddingRetrievalOutcome, ApplicationError> {
        Ok(self.0.clone())
    }
}

struct EmptyTextRetriever;

#[async_trait]
impl TextRetriever for EmptyTextRetriever {
    async fn search(
        &self,
        _: &RequestContext,
        _: &NormalizedRetrievalRequest,
    ) -> Result<Vec<vestrace_domain::RetrievalCandidate>, ApplicationError> {
        // Succeeds with nothing. A channel that returns no candidates is still a
        // channel that answered, which is what keeps the search from failing
        // closed and lets the vector channel's decline be the thing under test.
        Ok(Vec::new())
    }
}

struct SilentJournal;

#[async_trait]
impl vestrace_application::RetrievalJournal for SilentJournal {
    async fn record_run(
        &self,
        _: &RequestContext,
        _: &vestrace_application::retrieval::RetrievalRunRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn record_context_pack(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::ContextPackId,
        _: RetrievalRunId,
        _: WorkspaceId,
        _: u32,
        _: u32,
        _: &serde_json::Value,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

struct NoRevisions;

#[async_trait]
impl RevisionHydrator for NoRevisions {
    async fn hydrate(
        &self,
        _: &RequestContext,
        _: &[RevisionRef],
    ) -> Result<Vec<HydratedRevision>, ApplicationError> {
        Ok(Vec::new())
    }
}

/// A search names the corpus it ran against, and the service will not run one
/// it cannot name. Nothing here is under test; it is the world the declining
/// channel lives in.
struct FixedCorpus;

#[async_trait]
impl vestrace_application::retrieval::CorpusGenerationResolver for FixedCorpus {
    async fn resolve(
        &self,
        context: &RequestContext,
        space_name: &str,
        model: &str,
    ) -> Result<vestrace_application::retrieval::ResolvedCorpusGeneration, ApplicationError> {
        Ok(vestrace_application::retrieval::ResolvedCorpusGeneration {
            embedding_space_key: vestrace_domain::embedding::EmbeddingSpaceKey::new(
                context.workspace_id,
                space_name,
                model,
                3,
            )
            .expect("the suite states a valid space configuration"),
            corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
        })
    }
}

struct NoMemories;

#[async_trait]
impl MemoryUseCases for NoMemories {
    async fn record_event(
        &self,
        _: &RequestContext,
        _: vestrace_application::RecordEventCommand,
    ) -> Result<vestrace_domain::Event, ApplicationError> {
        unreachable!("this suite calls search_memories only")
    }
    async fn remember_memory(
        &self,
        _: &RequestContext,
        _: vestrace_application::RememberMemoryCommand,
    ) -> Result<vestrace_domain::Memory, ApplicationError> {
        unreachable!("this suite calls search_memories only")
    }
    async fn revise_memory(
        &self,
        _: &RequestContext,
        _: vestrace_application::ReviseMemoryCommand,
    ) -> Result<vestrace_domain::Memory, ApplicationError> {
        unreachable!("this suite calls search_memories only")
    }
    async fn link_knowledge(
        &self,
        _: &RequestContext,
        _: vestrace_application::LinkKnowledgeCommand,
    ) -> Result<vestrace_domain::KnowledgeRelation, ApplicationError> {
        unreachable!("this suite calls search_memories only")
    }
    async fn find_memory(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::MemoryId,
    ) -> Result<Option<vestrace_domain::Memory>, ApplicationError> {
        Ok(None)
    }
    async fn find_revision(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::MemoryRevisionId,
    ) -> Result<Option<vestrace_domain::MemoryRevision>, ApplicationError> {
        Ok(None)
    }
}

struct NoModels;

#[async_trait]
impl vestrace_application::ModelRepository for NoModels {
    async fn create(
        &self,
        _: &RequestContext,
        _: &vestrace_application::ModelRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &RequestContext,
    ) -> Result<Vec<vestrace_application::ModelRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::ModelId,
    ) -> Result<Option<vestrace_application::ModelRecord>, ApplicationError> {
        Ok(None)
    }
}

struct NoAgents;

#[async_trait]
impl vestrace_application::AgentRepository for NoAgents {
    async fn create(&self, _: &RequestContext, _: &AgentRecord) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(&self, _: &RequestContext) -> Result<Vec<AgentRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::AgentId,
    ) -> Result<Option<AgentRecord>, ApplicationError> {
        Ok(None)
    }
}

struct NoSkills;

#[async_trait]
impl vestrace_application::SkillRepository for NoSkills {
    async fn create(&self, _: &RequestContext, _: &SkillRecord) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(&self, _: &RequestContext) -> Result<Vec<SkillRecord>, ApplicationError> {
        Ok(Vec::new())
    }
}

struct NoWorkflows;

#[async_trait]
impl vestrace_application::WorkflowRepository for NoWorkflows {
    async fn create(
        &self,
        _: &RequestContext,
        _: &WorkflowDefinitionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &RequestContext,
    ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::WorkflowId,
    ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError> {
        Ok(None)
    }
    async fn save_revision(
        &self,
        _: &RequestContext,
        _: &WorkflowRevisionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn get_revision(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::WorkflowId,
        _: u32,
    ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError> {
        Ok(None)
    }
}

struct NoEvaluations;

#[async_trait]
impl vestrace_application::EvaluationRepository for NoEvaluations {
    async fn create(
        &self,
        _: &RequestContext,
        _: &EvaluationRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(&self, _: &RequestContext) -> Result<Vec<EvaluationRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &RequestContext,
        _: vestrace_domain::id::EvaluationId,
    ) -> Result<Option<EvaluationRecord>, ApplicationError> {
        Ok(None)
    }
}

/// Grants whatever is asked. The authorization boundary is not what this suite
/// is about; it is here because the tool will not run without one, and running
/// the real tool is the point.
struct AllowEverything;

#[async_trait]
impl vestrace_application::PolicyDecisionEngine for AllowEverything {
    async fn decide(
        &self,
        context: &RequestContext,
        request: vestrace_domain::AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        let at = vestrace_domain::now();
        let grant = CapabilityGrant::issue(
            CapabilityGrantSpec {
                id: CapabilityGrantId::new(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                issuer_id: context.principal_id,
                capability: request.capability.clone(),
                operation: request.operation.clone(),
                resource_scope: request.resource_scope.clone(),
                valid_from: at,
                valid_until: None,
                budget: None,
                risk_ceiling: vestrace_domain::RiskCategory::Critical,
                conditions: request.conditions.clone(),
            },
            at,
        )?;
        Ok(evaluate_capability_grants(
            PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            "embedding-retrieval-suite-v1",
            &request,
            &[grant],
            at,
        )?)
    }
}

fn server_answering(outcome: EmbeddingRetrievalOutcome) -> McpServer {
    let retrieval = RetrievalService::new(Arc::new(EmptyTextRetriever), Arc::new(SilentJournal))
        .with_hydration(
            Arc::new(NoRevisions),
            ClassificationPolicy::permissive(),
            "test-retrieval-policy",
        )
        .with_corpus_generation_resolver(Arc::new(FixedCorpus), "test-space", "test-model")
        .with_embedding_retrieval_client(
            Arc::new(FixedRetrievalClient(outcome)),
            std::time::Duration::from_secs(5),
        )
        .expect("five seconds is inside the wait bound");
    McpServer::new_with_policy(
        Arc::new(NoMemories),
        Arc::new(retrieval),
        Arc::new(NoModels),
        Arc::new(NoAgents),
        Arc::new(NoSkills),
        Arc::new(NoWorkflows),
        Arc::new(NoEvaluations),
        Arc::new(NullExecutionHistoryRepository::new()),
        Arc::new(AllowEverything),
    )
}

async fn search(server: &McpServer, query: &str) -> serde_json::Value {
    let workspace_id = WorkspaceId::new();
    server
        .handle_tool_call(
            workspace_id,
            PrincipalId::new(),
            "search_memories",
            &serde_json::json!({ "query": query }),
        )
        .await
        .expect("a declined vector channel is not a failed search")
        .content
}

/// A generation change reaches the agent as the exact reason and the exact
/// attempt.
///
/// Both halves are load-bearing. The reason without the identity would tell an
/// agent that a retry is permitted without telling it what to retry, and the
/// retry command takes a predecessor job; the identity without the reason would
/// not distinguish the one retryable case from the five that asking again
/// cannot fix.
#[tokio::test]
async fn a_generation_change_reaches_the_agent_as_a_reason_and_an_attempt() {
    let job_id = EmbeddingJobId::new();
    let server = server_answering(EmbeddingRetrievalOutcome::Degraded(
        DegradedRetrievalAttempt::of(
            job_id,
            EmbeddingRetrievalDegradation::GenerationChanged(
                RetrievalGenerationChangedReason::Revoked,
            ),
        ),
    ));

    let output = search(&server, "what did we decide").await;

    assert_eq!(output["degraded"], true);
    assert_eq!(
        output["vector_channel"]["reason"],
        "retrieval_generation_changed"
    );
    assert_eq!(output["vector_channel"]["retry_available"], true);
    assert_eq!(
        output["vector_channel"]["embedding_job_id"],
        serde_json::json!(job_id.as_uuid()),
        "a retryable decline must name the attempt a successor would follow"
    );
}

/// A decline that preceded any attempt says so, and does not offer a retry.
///
/// With no canonical space registered nothing was admitted, so there is no job
/// to name. An agent told `retry_available: true` here would call the retry
/// route with an identity it invented.
#[tokio::test]
async fn a_decline_before_any_attempt_offers_no_retry_and_names_no_job() {
    let server = server_answering(EmbeddingRetrievalOutcome::Degraded(
        DegradedRetrievalAttempt::unattempted(EmbeddingRetrievalDegradation::LegacyAdoptionPending),
    ));

    let output = search(&server, "what did we decide").await;

    assert_eq!(
        output["vector_channel"]["reason"],
        "legacy_adoption_pending"
    );
    assert_eq!(output["vector_channel"]["retry_available"], false);
    assert_eq!(
        output["vector_channel"]["embedding_job_id"],
        serde_json::Value::Null
    );
}

/// Every other member of the vocabulary reaches the agent as itself, and none
/// of them offers a retry -- including when an attempt exists to name.
#[tokio::test]
async fn the_unretryable_degradations_reach_the_agent_as_themselves() {
    for (degradation, expected) in [
        (
            EmbeddingRetrievalDegradation::MissingLocalIndex,
            "missing_local_index",
        ),
        (
            EmbeddingRetrievalDegradation::TransitionNotReady,
            "transition_not_ready",
        ),
        (
            EmbeddingRetrievalDegradation::GenerationNotReady,
            "generation_not_ready",
        ),
        (EmbeddingRetrievalDegradation::Pending, "retrieval_pending"),
    ] {
        let server = server_answering(EmbeddingRetrievalOutcome::Degraded(
            DegradedRetrievalAttempt::of(EmbeddingJobId::new(), degradation),
        ));

        let output = search(&server, "what did we decide").await;

        assert_eq!(
            output["vector_channel"]["reason"], expected,
            "{degradation:?} must reach the agent as its own reason"
        );
        assert_eq!(
            output["vector_channel"]["retry_available"], false,
            "{degradation:?} is not fixed by asking again"
        );
    }
}

/// An undegraded search says so by saying nothing.
///
/// Null rather than an object with false fields: an agent that has to inspect
/// `reason == null` inside a present object is one step from treating the
/// object's presence as the signal.
#[tokio::test]
async fn an_undegraded_search_carries_no_vector_channel_detail() {
    let server = server_answering(EmbeddingRetrievalOutcome::Completed(Vec::new()));

    let output = search(&server, "what did we decide").await;

    assert_eq!(output["degraded"], false);
    assert_eq!(output["vector_channel"], serde_json::Value::Null);
}

/// Nothing derived from the query reaches the agent.
///
/// Asserted against the whole serialized output rather than field by field,
/// because a field-by-field assertion only covers the fields that exist today.
/// The query text is searched for because it is what a query vector was
/// computed from, and the vocabulary words because a field named for any of
/// them would be one serialization away from carrying the thing it is named
/// for.
#[tokio::test]
async fn no_query_text_vector_or_digest_reaches_the_agent() {
    let server = server_answering(EmbeddingRetrievalOutcome::Degraded(
        DegradedRetrievalAttempt::of(
            EmbeddingJobId::new(),
            EmbeddingRetrievalDegradation::GenerationChanged(
                RetrievalGenerationChangedReason::CorpusChanged,
            ),
        ),
    ));

    let secret = "kestrel-fulcrum-9317";
    let output = search(&server, secret).await;
    let serialized = serde_json::to_string(&output).expect("tool output is serializable");

    assert!(
        !serialized.contains(secret),
        "the query text must not be echoed back: {serialized}"
    );
    for forbidden in [
        "query",
        "vector_bytes",
        "embedding_components",
        "digest",
        "ciphertext",
        "credential",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "{forbidden} must not appear anywhere in the tool output: {serialized}"
        );
    }
}
