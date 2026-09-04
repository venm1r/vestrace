use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::post,
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
    mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/embedding-transitions/{id}/acknowledge-carry",
        ),
        post(acknowledge_carry),
    )
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
