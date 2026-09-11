use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use vestrace_application::{
    AcceptEmbeddingJob, AcknowledgeCarriedTransitionBatchAfterUnknown, ApplicationError,
    CarryRecipeMapping, IdempotencyRecord, OutboxMessage,
};
use vestrace_domain::{
    AuditEvent, Capability, EmbeddingJobId, EmbeddingSpaceId, ExternalEffectIntent,
    ModelRequestEvidenceId, RiskCategory,
    embedding::EmbeddingJobKind,
    external_effects::{
        DeliverySemantics, EffectPrecondition, EffectReversibility, IdempotencyProfile,
    },
    id::AuditEventId,
    time::now,
};

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{
    ApiError, connections::GovernedMutationResponse, context::request_context,
    required_idempotency_key,
};

const IDEMPOTENCY_WINDOW_HOURS: i64 = 24;

pub fn embedding_job_routes() -> axum::Router<AppState> {
    let router = mount(
        axum::Router::new(),
        route_descriptor(&Method::POST, "/v1/embedding-jobs/{id}/acknowledge-unknown"),
        post(acknowledge_unknown),
    );
    let router = mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/embedding-transitions/{id}/acknowledge-carry",
        ),
        post(acknowledge_carry),
    );
    let router = mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/embedding-jobs/{id}/retry-generation-changed",
        ),
        post(retry_generation_changed),
    );
    mount(
        router,
        route_descriptor(&Method::GET, "/v1/embedding-jobs/{id}/retrieval"),
        get(retrieval_attempt),
    )
}

/// What one retrieval attempt did, read back after the fact.
///
/// The absences are the contract. There is no field here for ciphertext, for
/// vector components, for a stable vector digest, for a credential, or for
/// whether a process has a local index loaded -- the last because the authority
/// this is read from is a database transaction, and a database transaction
/// cannot see another process's memory. A read model that answered that
/// question would be reporting a guess as a fact, and an operator would act on
/// it.
#[derive(Debug, serde::Serialize)]
pub struct RetrievalAttemptResponse {
    pub embedding_job_id: Uuid,
    pub retrieval_request_id: Uuid,
    pub job_state: String,
    /// The handle an authorized retry must agree with. Published so a caller
    /// deciding to spend a further provider call states what it read rather
    /// than guessing, which is what makes the adapter's check meaningful.
    pub job_version: u64,
    pub space_registration_id: Uuid,
    pub pinned_generation_id: Uuid,
    pub pinned_generation_epoch: u64,
    pub pinned_generation_member_count: u64,
    /// How many memories the terminal result named. Absent while no terminal
    /// result exists, which is different from a result that named none.
    pub reference_count: Option<u32>,
    /// The closed degradation vocabulary, or absent.
    pub degradation_reason: Option<String>,
    pub generation_changed_reason: Option<String>,
    pub predecessor_embedding_job_id: Option<Uuid>,
    pub successor_embedding_job_id: Option<Uuid>,
    /// Whether `POST .../retry-generation-changed` would have something to do.
    /// False once a successor exists, because a predecessor has exactly one.
    pub retry_available: bool,
}

/// Reads one attempt.
///
/// A job this workspace does not have, or has but never admitted as a retrieval
/// attempt, is 404 rather than an empty view: those are the same answer to a
/// reader, and neither is "this attempt found nothing".
async fn retrieval_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(embedding_job_id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    let view = state
        .embedding_retrieval_repository()?
        .attempt_view(&context, EmbeddingJobId::from_uuid(embedding_job_id))
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| ApiError::not_found("embedding retrieval attempt"))?;
    Ok((
        StatusCode::OK,
        Json(RetrievalAttemptResponse {
            embedding_job_id: view.job_id.as_uuid(),
            retrieval_request_id: view.request_id.as_uuid(),
            job_state: view.job_state,
            job_version: view.job_version,
            space_registration_id: view.space_registration_id,
            pinned_generation_id: view.generation_id,
            pinned_generation_epoch: view.generation_epoch,
            pinned_generation_member_count: view.generation_member_count,
            reference_count: view.reference_count,
            degradation_reason: view.degradation_reason.map(str::to_owned),
            generation_changed_reason: view
                .generation_changed_reason
                .map(|reason| reason.as_str().to_owned()),
            predecessor_embedding_job_id: view.predecessor_job_id.map(|id| id.as_uuid()),
            successor_embedding_job_id: view.successor_job_id.map(|id| id.as_uuid()),
            retry_available: view.retry_available,
        }),
    )
        .into_response())
}

/// One authorized successor to an attempt whose pinned generation moved.
///
/// Every field is stated by the caller and checked here rather than inferred,
/// for the same reason the acknowledgement above states its predecessor twice:
/// this spends a further provider call, and a surface that inferred any part of
/// which call to make could spend it on something the caller did not ask for.
#[derive(Debug, Deserialize)]
pub struct RetryGenerationChangedRequest {
    pub embedding_job_id: Uuid,
    pub expected_predecessor_version: u64,
    pub successor_embedding_job_id: Uuid,
    pub successor_request_id: Uuid,
    /// The caller's acknowledgement that this asks the provider again, and is
    /// charged again. Required and required to be true: a default would make
    /// the acknowledgement a formality, and a formality acknowledges nothing.
    pub acknowledge_additional_provider_call: bool,
}

async fn retry_generation_changed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(embedding_job_id): Path<Uuid>,
    Json(request): Json<RetryGenerationChangedRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.embedding_job_id != embedding_job_id {
        return Err(ApiError::bad_request(
            "the path embedding job id and the request body disagree",
        ));
    }
    if !request.acknowledge_additional_provider_call {
        return Err(ApiError::bad_request(
            "a retrieval retry asks the provider again and is charged again; set              acknowledge_additional_provider_call to confirm it",
        ));
    }
    if request.successor_embedding_job_id == embedding_job_id {
        return Err(ApiError::bad_request(
            "a retrieval retry successor must be a new job, not its own predecessor",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let successor = state
        .embedding_retrieval_repository()?
        .authorize_retry(
            &context,
            vestrace_application::embedding::RetryRetrievalGenerationChanged {
                predecessor_job_id: EmbeddingJobId::from_uuid(embedding_job_id),
                expected_predecessor_version: request.expected_predecessor_version,
                successor_job_id: EmbeddingJobId::from_uuid(request.successor_embedding_job_id),
                successor_request_id: vestrace_domain::id::RetrievalRunId::from_uuid(
                    request.successor_request_id,
                ),
                idempotency_key,
            },
        )
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(RetryGenerationChangedResponse {
            predecessor_embedding_job_id: embedding_job_id,
            successor_embedding_job_id: successor.as_uuid(),
        }),
    )
        .into_response())
}

/// What the caller may be told. Identities only: no vector, no digest, no
/// reason text beyond what the durable record already holds.
#[derive(Debug, serde::Serialize)]
pub struct RetryGenerationChangedResponse {
    pub predecessor_embedding_job_id: Uuid,
    pub successor_embedding_job_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CarryMappingRequest {
    pub old_recipe_ordinal: u32,
    pub old_input_ordinal: u32,
    pub new_recipe_ordinal: u32,
    pub new_input_ordinal: u32,
}

#[derive(Debug, Deserialize)]
pub struct AcknowledgeCarryRequest {
    pub transition_id: Uuid,
    pub carry_id: Uuid,
    pub predecessor_embedding_job_id: Uuid,
    pub predecessor_version: u64,
    pub predecessor_transition_batch_id: Uuid,
    pub successor_transition_batch_id: Uuid,
    pub successor_embedding_job_id: Uuid,
    pub space_registration_id: Uuid,
    pub kind: String,
    pub model_binding_snapshot_id: Uuid,
    pub external_effect_id: Uuid,
    pub model_request_evidence_id: Uuid,
    pub mappings: Vec<CarryMappingRequest>,
}

async fn acknowledge_carry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(transition_id): Path<Uuid>,
    Json(request): Json<AcknowledgeCarryRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.transition_id != transition_id {
        return Err(ApiError::bad_request(
            "the path transition id and the request body disagree",
        ));
    }
    required_idempotency_key(&headers)?;
    let command = AcknowledgeCarriedTransitionBatchAfterUnknown {
        transition_id: request.transition_id,
        carry_id: request.carry_id,
        predecessor_embedding_job_id: request.predecessor_embedding_job_id,
        predecessor_version: vestrace_domain::embedding::TransitionVersion::new(
            request.predecessor_version,
        ),
        predecessor_transition_batch_id: request.predecessor_transition_batch_id,
        successor_transition_batch_id: request.successor_transition_batch_id,
        successor_embedding_job_id: request.successor_embedding_job_id,
        space_registration_id: request.space_registration_id,
        kind: request.kind,
        model_binding_snapshot_id: request.model_binding_snapshot_id,
        external_effect_id: request.external_effect_id,
        model_request_evidence_id: request.model_request_evidence_id,
        mappings: request
            .mappings
            .into_iter()
            .map(|mapping| CarryRecipeMapping {
                old_recipe: vestrace_domain::embedding::TransitionRecipeOrdinal::new(
                    mapping.old_recipe_ordinal,
                ),
                old_input: vestrace_domain::embedding::TransitionInputOrdinal::new(
                    mapping.old_input_ordinal,
                ),
                new_recipe: vestrace_domain::embedding::TransitionRecipeOrdinal::new(
                    mapping.new_recipe_ordinal,
                ),
                new_input: vestrace_domain::embedding::TransitionInputOrdinal::new(
                    mapping.new_input_ordinal,
                ),
            })
            .collect(),
    };
    let successor = state
        .embedding_transition_repository()?
        .acknowledge_carried_batch_after_unknown(context, command)
        .await
        .map_err(|error| {
            ApiError::from_application(match error {
                ApplicationError::Policy(message) => ApplicationError::Conflict(message),
                error => error,
            })
        })?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "successor_embedding_job_id": successor })),
    )
        .into_response())
}

/// An externally visible acknowledgement never chooses the successor effect
/// id. The boundary constructs the intent, and `ExternalEffectIntent::new`
/// allocates that identity so a caller cannot collide with another effect.
#[derive(Debug, Deserialize)]
pub struct EmbeddingEffectIntentRequest {
    pub execution_ref: String,
    pub adapter: String,
    pub operation: String,
    pub target: String,
    pub normalized_arguments_digest: String,
    pub expected_effect: String,
    pub preconditions: Vec<EffectPreconditionRequest>,
    pub precondition_digest: String,
    pub risk: RiskCategory,
    pub reversibility: EffectReversibility,
    pub idempotency_profile: IdempotencyProfile,
    pub delivery_semantics: DeliverySemantics,
    pub required_capability: Capability,
    pub budget_reservation_ref: Option<String>,
    pub policy_decision_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EffectPreconditionRequest {
    pub name: String,
    pub expected_value: String,
}

/// Everything the operator states to replace one acknowledged ambiguous job.
/// The predecessor is intentionally named in both the path and body so a
/// proxy or client cannot silently substitute either identity.
#[derive(Debug, Deserialize)]
pub struct AcknowledgeUnknownRequest {
    pub embedding_job_id: Uuid,
    pub expected_predecessor_version: u64,
    pub successor_embedding_job_id: Uuid,
    pub successor_model_request_evidence_id: Uuid,
    pub space_registration_id: Uuid,
    pub model_binding_snapshot_id: Uuid,
    pub kind: String,
    pub effect_intent: EmbeddingEffectIntentRequest,
}

async fn acknowledge_unknown(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(embedding_job_id): Path<Uuid>,
    Json(request): Json<AcknowledgeUnknownRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.embedding_job_id != embedding_job_id {
        return Err(ApiError::bad_request(
            "the path embedding job id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let command = command(&context, idempotency_key, request)?;
    let receipt = state
        .embedding_job_repository()?
        .accept_governed(context, command)
        .await
        .map_err(acceptance_error)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

fn command(
    context: &vestrace_application::RequestContext,
    idempotency_key: String,
    request: AcknowledgeUnknownRequest,
) -> Result<AcceptEmbeddingJob, ApiError> {
    let AcknowledgeUnknownRequest {
        embedding_job_id,
        expected_predecessor_version,
        successor_embedding_job_id,
        successor_model_request_evidence_id,
        space_registration_id,
        model_binding_snapshot_id,
        kind: stated_kind,
        effect_intent,
    } = request;
    let kind = stated_kind
        .parse::<EmbeddingJobKind>()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let request_hash = request_hash(
        embedding_job_id,
        expected_predecessor_version,
        successor_embedding_job_id,
        successor_model_request_evidence_id,
        space_registration_id,
        model_binding_snapshot_id,
        kind,
    );
    let preconditions = effect_intent
        .preconditions
        .into_iter()
        .map(|precondition| EffectPrecondition::new(precondition.name, precondition.expected_value))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::from_domain)?;
    let at = now();
    let intent = ExternalEffectIntent::new(
        effect_intent.execution_ref,
        context.workspace_id,
        context.principal_id,
        effect_intent.adapter,
        effect_intent.operation,
        effect_intent.target,
        effect_intent.normalized_arguments_digest,
        effect_intent.expected_effect,
        preconditions,
        effect_intent.precondition_digest,
        effect_intent.risk,
        effect_intent.reversibility,
        effect_intent.idempotency_profile,
        effect_intent.delivery_semantics,
        effect_intent.required_capability,
        effect_intent.budget_reservation_ref,
        effect_intent.policy_decision_ref,
        at,
    )
    .map_err(ApiError::from_domain)?;
    let evidence = serde_json::json!({
        "predecessor_embedding_job_id": embedding_job_id,
        "expected_predecessor_version": expected_predecessor_version,
        "successor_embedding_job_id": successor_embedding_job_id,
        "successor_model_request_evidence_id": successor_model_request_evidence_id,
        "space_registration_id": space_registration_id,
        "model_binding_snapshot_id": model_binding_snapshot_id,
        "kind": kind.as_str(),
    });
    Ok(AcceptEmbeddingJob {
        job_id: EmbeddingJobId::from_uuid(successor_embedding_job_id),
        space_registration_id: EmbeddingSpaceId::from_uuid(space_registration_id),
        kind,
        model_binding_snapshot_id,
        intent,
        model_request_evidence_id: ModelRequestEvidenceId::from_uuid(
            successor_model_request_evidence_id,
        ),
        retries_unknown_embedding_job_id: Some(EmbeddingJobId::from_uuid(embedding_job_id)),
        expected_predecessor_version: Some(expected_predecessor_version),
        idempotency: Some(IdempotencyRecord {
            idempotency_key,
            workspace_id: context.workspace_id,
            request_hash,
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(IDEMPOTENCY_WINDOW_HOURS),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "embedding.job.unknown_acknowledged",
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "embedding.job.unknown_acknowledged",
            "embedding_job",
            successor_embedding_job_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    })
}

/// The idempotency boundary compares only what the caller named. The effect id
/// is allocated by `ExternalEffectIntent::new`, so including it would make two
/// byte-identical requests falsely look different.
fn request_hash(
    predecessor_job_id: Uuid,
    expected_predecessor_version: u64,
    successor_job_id: Uuid,
    successor_mre_id: Uuid,
    space_registration_id: Uuid,
    binding_snapshot_id: Uuid,
    kind: EmbeddingJobKind,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(predecessor_job_id.as_bytes());
    hasher.update(expected_predecessor_version.to_be_bytes());
    hasher.update(successor_job_id.as_bytes());
    hasher.update(successor_mre_id.as_bytes());
    hasher.update(space_registration_id.as_bytes());
    hasher.update(binding_snapshot_id.as_bytes());
    hasher.update(kind.as_str().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn acceptance_error(error: ApplicationError) -> ApiError {
    match error {
        ApplicationError::Policy(message) if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED" => {
            ApiError::from_application(ApplicationError::Conflict(message))
        }
        error => ApiError::from_application(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use vestrace_application::{EmbeddingJobRepository, GovernedMutationReceipt, RequestContext};
    use vestrace_domain::id::{AuditEventId, OutboxId};

    use super::*;
    use crate::{
        api::runs::tests::{TestAllowPolicy, test_state},
        build_router,
    };

    #[derive(Default)]
    struct SpyEmbeddingJobs {
        command: Mutex<Option<AcceptEmbeddingJob>>,
    }

    #[async_trait::async_trait]
    impl EmbeddingJobRepository for SpyEmbeddingJobs {
        async fn accept_governed(
            &self,
            _context: RequestContext,
            command: AcceptEmbeddingJob,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            *self.command.lock().unwrap() = Some(command);
            Ok(GovernedMutationReceipt {
                audit_event_id: AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: vec![OutboxId::new()],
            })
        }
    }

    struct RefusingEmbeddingJobs;

    #[async_trait::async_trait]
    impl EmbeddingJobRepository for RefusingEmbeddingJobs {
        async fn accept_governed(
            &self,
            _context: RequestContext,
            _command: AcceptEmbeddingJob,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            Err(ApplicationError::Policy(
                "EMBEDDING_JOB_ACCEPTANCE_REFUSED".to_owned(),
            ))
        }
    }

    struct Posted {
        predecessor_id: Uuid,
        successor_id: Uuid,
        mre_id: Uuid,
        space_id: Uuid,
        snapshot_id: Uuid,
        body: String,
    }

    fn posted() -> Posted {
        let predecessor_id = Uuid::now_v7();
        let successor_id = Uuid::now_v7();
        let mre_id = Uuid::now_v7();
        let space_id = Uuid::now_v7();
        let snapshot_id = Uuid::now_v7();
        let body = serde_json::json!({
            "embedding_job_id": predecessor_id,
            "expected_predecessor_version": 2,
            "successor_embedding_job_id": successor_id,
            "successor_model_request_evidence_id": mre_id,
            "space_registration_id": space_id,
            "model_binding_snapshot_id": snapshot_id,
            "kind": "delivery",
            "effect_intent": {
                "execution_ref": "workspace://",
                "adapter": "openai-compatible",
                "operation": "embeddings",
                "target": "http://127.0.0.1:1234/v1/embeddings",
                "normalized_arguments_digest": "sha256:arguments",
                "expected_effect": "produce a governed embedding",
                "preconditions": [{"name": "model-snapshot", "expected_value": snapshot_id}],
                "precondition_digest": "sha256:preconditions",
                "risk": "critical",
                "reversibility": "unknown",
                "idempotency_profile": "provider_key",
                "delivery_semantics": "at_least_once",
                "required_capability": "embedding_retry_after_unknown",
                "budget_reservation_ref": null,
                "policy_decision_ref": null,
            },
        })
        .to_string();
        Posted {
            predecessor_id,
            successor_id,
            mre_id,
            space_id,
            snapshot_id,
            body,
        }
    }

    /// Records what the retry authority was asked to do, and answers with the
    /// successor the caller named.
    #[derive(Default)]
    struct SpyRetrievalRetry {
        command: Mutex<Option<vestrace_application::embedding::RetryRetrievalGenerationChanged>>,
    }

    #[async_trait::async_trait]
    impl vestrace_application::embedding::EmbeddingRetrievalRepository for SpyRetrievalRetry {
        async fn authorize_retry(
            &self,
            _context: &RequestContext,
            command: vestrace_application::embedding::RetryRetrievalGenerationChanged,
        ) -> Result<vestrace_domain::EmbeddingJobId, ApplicationError> {
            let successor = command.successor_job_id;
            *self.command.lock().unwrap() = Some(command);
            Ok(successor)
        }
    }

    /// The authority refusing a caller that acted on a stale reading.
    struct ConflictingRetrievalRetry;

    #[async_trait::async_trait]
    impl vestrace_application::embedding::EmbeddingRetrievalRepository for ConflictingRetrievalRetry {
        async fn authorize_retry(
            &self,
            _context: &RequestContext,
            _command: vestrace_application::embedding::RetryRetrievalGenerationChanged,
        ) -> Result<vestrace_domain::EmbeddingJobId, ApplicationError> {
            Err(ApplicationError::Conflict(
                "the retrieval retry predecessor is at version 5, not the expected 2".to_owned(),
            ))
        }
    }

    struct RetryPost {
        predecessor_id: Uuid,
        successor_id: Uuid,
        request_id: Uuid,
    }

    impl RetryPost {
        fn new() -> Self {
            Self {
                predecessor_id: Uuid::now_v7(),
                successor_id: Uuid::now_v7(),
                request_id: Uuid::now_v7(),
            }
        }

        fn body(&self, acknowledged: bool) -> String {
            serde_json::json!({
                "embedding_job_id": self.predecessor_id,
                "expected_predecessor_version": 2,
                "successor_embedding_job_id": self.successor_id,
                "successor_request_id": self.request_id,
                "acknowledge_additional_provider_call": acknowledged,
            })
            .to_string()
        }

        fn uri(&self) -> String {
            format!(
                "/v1/embedding-jobs/{}/retry-generation-changed",
                self.predecessor_id
            )
        }
    }

    #[tokio::test]
    async fn a_confirmed_retry_reaches_the_authority_with_every_stated_identity() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();
        let key = Uuid::now_v7().to_string();

        let response = app
            .oneshot(request(&post.uri(), Some(key.clone()), &post.body(true)))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let command = spy.command.lock().unwrap().take().expect("retry reached");
        assert_eq!(command.predecessor_job_id.as_uuid(), post.predecessor_id);
        assert_eq!(command.expected_predecessor_version, 2);
        assert_eq!(command.successor_job_id.as_uuid(), post.successor_id);
        assert_eq!(command.successor_request_id.as_uuid(), post.request_id);
        assert_eq!(command.idempotency_key, key);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json["successor_embedding_job_id"].as_str(),
            Some(post.successor_id.to_string().as_str())
        );
        // Identities only. A retrieval surface that answered with anything
        // derived from a vector would be the one place the whole package is
        // careful about leaking from.
        assert_eq!(
            json.as_object().map(|fields| fields.len()),
            Some(2),
            "the response names the two jobs and nothing else: {json}"
        );
    }

    /// An unconfirmed retry is refused, and the authority is never asked.
    ///
    /// The confirmation is the caller saying it accepts a second provider call
    /// and its charge. A default would make that a formality, so the field is
    /// required and required to be true.
    #[tokio::test]
    async fn an_unconfirmed_retry_never_reaches_the_authority() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();

        let response = app
            .oneshot(request(
                &post.uri(),
                Some(Uuid::now_v7().to_string()),
                &post.body(false),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            spy.command.lock().unwrap().is_none(),
            "a refused confirmation must not reach the authority"
        );
    }

    /// A body missing the acknowledgement entirely is refused too. Absent and
    /// false must mean the same thing, or the field could be omitted to skip
    /// the decision it exists to record.
    #[tokio::test]
    async fn a_retry_without_the_acknowledgement_field_is_refused() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();
        let body = serde_json::json!({
            "embedding_job_id": post.predecessor_id,
            "expected_predecessor_version": 2,
            "successor_embedding_job_id": post.successor_id,
            "successor_request_id": post.request_id,
        })
        .to_string();

        let response = app
            .oneshot(request(
                &post.uri(),
                Some(Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();

        assert!(
            response.status().is_client_error(),
            "an absent acknowledgement is not an acknowledgement"
        );
        assert!(spy.command.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn a_retry_whose_path_and_body_disagree_is_refused() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();

        let response = app
            .oneshot(request(
                &format!(
                    "/v1/embedding-jobs/{}/retry-generation-changed",
                    Uuid::now_v7()
                ),
                Some(Uuid::now_v7().to_string()),
                &post.body(true),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.command.lock().unwrap().is_none());
    }

    /// A successor that is its own predecessor is not a successor.
    #[tokio::test]
    async fn a_retry_that_names_itself_as_its_successor_is_refused() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();
        let body = serde_json::json!({
            "embedding_job_id": post.predecessor_id,
            "expected_predecessor_version": 2,
            "successor_embedding_job_id": post.predecessor_id,
            "successor_request_id": post.request_id,
            "acknowledge_additional_provider_call": true,
        })
        .to_string();

        let response = app
            .oneshot(request(
                &post.uri(),
                Some(Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.command.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn a_retry_without_an_idempotency_key_is_refused() {
        let spy = Arc::new(SpyRetrievalRetry::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(spy.clone()),
        );
        let post = RetryPost::new();

        let response = app
            .oneshot(request(&post.uri(), None, &post.body(true)))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.command.lock().unwrap().is_none());
    }

    /// A stale expected version is a conflict the caller must resolve by
    /// reading again, not a failure it should retry blindly.
    #[tokio::test]
    async fn a_stale_expected_version_is_reported_as_a_conflict() {
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(Arc::new(ConflictingRetrievalRetry)),
        );
        let post = RetryPost::new();

        let response = app
            .oneshot(request(
                &post.uri(),
                Some(Uuid::now_v7().to_string()),
                &post.body(true),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    /// Without a configured authority the route refuses rather than reporting a
    /// successor nothing recorded.
    #[tokio::test]
    async fn a_retry_without_a_configured_authority_fails_closed() {
        let app = build_router(test_state().with_policy(Arc::new(TestAllowPolicy)));
        let post = RetryPost::new();

        let response = app
            .oneshot(request(
                &post.uri(),
                Some(Uuid::now_v7().to_string()),
                &post.body(true),
            ))
            .await
            .unwrap();

        assert!(
            response.status().is_server_error(),
            "an unconfigured retry authority must not look like a client mistake"
        );
    }

    /// Answers the read with one stated view, and records what it was asked.
    struct StubAttemptView {
        view: Mutex<Option<vestrace_application::embedding::RetrievalAttemptView>>,
        asked: Mutex<Option<EmbeddingJobId>>,
    }

    impl StubAttemptView {
        fn holding(view: Option<vestrace_application::embedding::RetrievalAttemptView>) -> Self {
            Self {
                view: Mutex::new(view),
                asked: Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl vestrace_application::embedding::EmbeddingRetrievalRepository for StubAttemptView {
        async fn attempt_view(
            &self,
            _context: &RequestContext,
            job_id: EmbeddingJobId,
        ) -> Result<Option<vestrace_application::embedding::RetrievalAttemptView>, ApplicationError>
        {
            *self.asked.lock().unwrap() = Some(job_id);
            Ok(self.view.lock().unwrap().clone())
        }
    }

    /// A degraded attempt that has earned a retry and has not spent it.
    fn changed_attempt(job_id: Uuid) -> vestrace_application::embedding::RetrievalAttemptView {
        vestrace_application::embedding::RetrievalAttemptView {
            job_id: EmbeddingJobId::from_uuid(job_id),
            request_id: vestrace_domain::id::RetrievalRunId::new(),
            job_state: "succeeded".to_owned(),
            job_version: 3,
            space_registration_id: Uuid::now_v7(),
            generation_id: Uuid::now_v7(),
            generation_epoch: 7,
            generation_member_count: 42,
            reference_count: None,
            degradation_reason: Some("retrieval_generation_changed"),
            generation_changed_reason: Some(
                vestrace_domain::embedding::RetrievalGenerationChangedReason::Revoked,
            ),
            predecessor_job_id: None,
            successor_job_id: None,
            retry_available: true,
        }
    }

    fn read(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("x-workspace-id", Uuid::now_v7().to_string())
            .header("x-principal-id", Uuid::now_v7().to_string())
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn an_attempt_reads_back_as_identities_states_counts_and_closed_reasons() {
        let job_id = Uuid::now_v7();
        let expected = changed_attempt(job_id);
        let stub = Arc::new(StubAttemptView::holding(Some(expected.clone())));
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(stub.clone()),
        );

        let response = app
            .oneshot(read(&format!("/v1/embedding-jobs/{job_id}/retrieval")))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            stub.asked.lock().unwrap().map(|id| id.as_uuid()),
            Some(job_id),
            "the read must ask about the job the path names"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["embedding_job_id"], job_id.to_string());
        assert_eq!(json["job_state"], "succeeded");
        assert_eq!(json["job_version"], 3);
        assert_eq!(json["pinned_generation_epoch"], 7);
        assert_eq!(json["pinned_generation_member_count"], 42);
        assert_eq!(json["degradation_reason"], "retrieval_generation_changed");
        assert_eq!(json["generation_changed_reason"], "revoked");
        assert_eq!(json["retry_available"], true);
        assert_eq!(json["successor_embedding_job_id"], serde_json::Value::Null);
        // No terminal result yet is absent, not zero. A result that named no
        // memories is a different fact from no result at all, and an operator
        // deciding whether to authorize a retry needs to tell them apart.
        assert_eq!(json["reference_count"], serde_json::Value::Null);
    }

    /// The view carries nothing derived from a vector.
    ///
    /// Asserted over the serialized body rather than field by field, because a
    /// field-by-field assertion only covers the fields that exist today, and
    /// this is the surface where a new one would arrive.
    #[tokio::test]
    async fn the_attempt_view_carries_no_ciphertext_vector_digest_or_credential() {
        let job_id = Uuid::now_v7();
        let stub = Arc::new(StubAttemptView::holding(Some(changed_attempt(job_id))));
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(stub),
        );

        let response = app
            .oneshot(read(&format!("/v1/embedding-jobs/{job_id}/retrieval")))
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let serialized = String::from_utf8(body.to_vec()).unwrap();

        for forbidden in [
            "ciphertext",
            "vector",
            "digest",
            "credential",
            "query",
            "embedding_components",
            "index_loaded",
            "local_index",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "{forbidden} must not appear in the attempt view: {serialized}"
            );
        }
    }

    /// A job this workspace never admitted as a retrieval attempt is absent,
    /// not an empty attempt.
    ///
    /// A `delivery` or `rebuild` job has no fence. Answering for one with a
    /// zeroed view would say it found nothing, which is a claim about a search
    /// that never happened.
    #[tokio::test]
    async fn a_job_that_is_not_a_retrieval_attempt_is_not_found() {
        let stub = Arc::new(StubAttemptView::holding(None));
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_retrieval_repository(stub),
        );

        let response = app
            .oneshot(read(&format!(
                "/v1/embedding-jobs/{}/retrieval",
                Uuid::now_v7()
            )))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// Without a configured authority the read refuses rather than reporting an
    /// attempt nothing recorded.
    #[tokio::test]
    async fn an_attempt_read_without_a_configured_authority_fails_closed() {
        let app = build_router(test_state().with_policy(Arc::new(TestAllowPolicy)));

        let response = app
            .oneshot(read(&format!(
                "/v1/embedding-jobs/{}/retrieval",
                Uuid::now_v7()
            )))
            .await
            .unwrap();

        assert!(
            response.status().is_server_error(),
            "an unconfigured read authority must not look like an absent attempt"
        );
    }

    fn request(uri: &str, idempotency_key: Option<String>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-workspace-id", Uuid::now_v7().to_string())
            .header("x-principal-id", Uuid::now_v7().to_string());
        if let Some(key) = idempotency_key {
            builder = builder.header("idempotency-key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    #[tokio::test]
    async fn an_acknowledgement_reaches_the_application_with_the_stated_successor() {
        let spy = Arc::new(SpyEmbeddingJobs::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_job_repository(spy.clone()),
        );
        let post = posted();
        let key = Uuid::now_v7().to_string();

        let response = app
            .oneshot(request(
                &format!(
                    "/v1/embedding-jobs/{}/acknowledge-unknown",
                    post.predecessor_id
                ),
                Some(key.clone()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let command = spy
            .command
            .lock()
            .unwrap()
            .take()
            .expect("acknowledgement reached");
        assert_eq!(
            command
                .retries_unknown_embedding_job_id
                .map(|id| id.as_uuid()),
            Some(post.predecessor_id)
        );
        assert_eq!(command.expected_predecessor_version, Some(2));
        assert_eq!(command.job_id.as_uuid(), post.successor_id);
        assert_eq!(command.model_request_evidence_id.as_uuid(), post.mre_id);
        assert_eq!(command.space_registration_id.as_uuid(), post.space_id);
        assert_eq!(command.model_binding_snapshot_id, post.snapshot_id);
        assert_eq!(command.kind, EmbeddingJobKind::Delivery);
        assert_eq!(
            command
                .idempotency
                .as_ref()
                .map(|record| record.idempotency_key.as_str()),
            Some(key.as_str())
        );
    }

    #[tokio::test]
    async fn an_acknowledgement_whose_path_and_body_disagree_is_refused() {
        let spy = Arc::new(SpyEmbeddingJobs::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_job_repository(spy.clone()),
        );
        let post = posted();

        let response = app
            .oneshot(request(
                &format!("/v1/embedding-jobs/{}/acknowledge-unknown", Uuid::now_v7()),
                Some(Uuid::now_v7().to_string()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.command.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn an_acknowledgement_without_an_idempotency_key_is_refused() {
        let spy = Arc::new(SpyEmbeddingJobs::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_job_repository(spy.clone()),
        );
        let post = posted();

        let response = app
            .oneshot(request(
                &format!(
                    "/v1/embedding-jobs/{}/acknowledge-unknown",
                    post.predecessor_id
                ),
                None,
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.command.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn an_unconfigured_embedding_authority_is_unavailable() {
        let app = build_router(test_state().with_policy(Arc::new(TestAllowPolicy)));
        let post = posted();

        let response = app
            .oneshot(request(
                &format!(
                    "/v1/embedding-jobs/{}/acknowledge-unknown",
                    post.predecessor_id
                ),
                Some(Uuid::now_v7().to_string()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "unavailable");
    }

    #[tokio::test]
    async fn an_acceptance_policy_refusal_is_a_conflict_not_an_authorization_denial() {
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_embedding_job_repository(Arc::new(RefusingEmbeddingJobs)),
        );
        let post = posted();

        let response = app
            .oneshot(request(
                &format!(
                    "/v1/embedding-jobs/{}/acknowledge-unknown",
                    post.predecessor_id
                ),
                Some(Uuid::now_v7().to_string()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["message"], "EMBEDDING_JOB_ACCEPTANCE_REFUSED");
    }
}
