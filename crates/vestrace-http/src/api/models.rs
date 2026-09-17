use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_application::{
    CreateModelRevision, GovernedModelProjection, GovernedProviderProjection, IdempotencyRecord,
    ModelRecord, OutboxMessage, SetWorkspaceModelDefault,
};
use vestrace_domain::{
    AuditEvent, ConnectionId, ConnectionRevisionId, ModelKind, ModelObservation, ModelRevision,
    ModelRevisionId, id::AuditEventId, time::now,
};

use crate::AppState;

use super::{
    ApiError,
    connections::{GovernedMutationResponse, QualificationJobResponse, QualificationRequest},
    context::request_context,
    required_idempotency_key,
};

#[derive(Debug, Serialize)]
pub struct ProviderResponse {
    pub id: uuid::Uuid,
    pub state: String,
    pub blockers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateModelRequest {
    pub provider_id: uuid::Uuid,
    pub model_name: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub id: uuid::Uuid,
    pub revision_id: Option<uuid::Uuid>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
}

impl From<GovernedProviderProjection> for ProviderResponse {
    fn from(projection: GovernedProviderProjection) -> Self {
        Self {
            id: projection.id.as_uuid(),
            state: projection.state,
            blockers: projection.blockers,
        }
    }
}

impl From<GovernedModelProjection> for ModelResponse {
    fn from(projection: GovernedModelProjection) -> Self {
        Self {
            id: projection.id.as_uuid(),
            revision_id: projection.revision_id.map(|id| id.as_uuid()),
            state: projection.state,
            qualification_state: projection.qualification_state,
            blockers: projection.blockers,
        }
    }
}

/// The legacy create route remains unchanged until its separate governed
/// mutation contract is available.  It is intentionally not used by GET.
#[derive(Debug, Serialize)]
struct LegacyModelResponse {
    id: uuid::Uuid,
    provider_id: uuid::Uuid,
    model_name: String,
    context_window: u32,
    input_cost_per_mtoken: f32,
    output_cost_per_mtoken: f32,
}

impl From<ModelRecord> for LegacyModelResponse {
    fn from(model: ModelRecord) -> Self {
        Self {
            id: model.id.as_uuid(),
            provider_id: model.provider_id.as_uuid(),
            model_name: model.model_name,
            context_window: model.context_window,
            input_cost_per_mtoken: model.input_cost_per_mtoken,
            output_cost_per_mtoken: model.output_cost_per_mtoken,
        }
    }
}

pub async fn create_provider() -> Result<axum::http::StatusCode, ApiError> {
    // Provider registry rows do not carry an immutable, qualified connection
    // revision.  Leaving this compatibility route executable would therefore
    // create an object which the governed dispatcher must never use.
    Err(ApiError::refused(
        "legacy_provider_registry_retired",
        "Legacy provider registry retired",
    ))
}

pub async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProviderResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let providers = state
        .model_revision_repository()?
        .list_safe_providers(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(
        providers.into_iter().map(ProviderResponse::from).collect(),
    ))
}

pub async fn create_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateModelRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    let id = vestrace_domain::id::ModelId::new();
    let record = ModelRecord {
        id,
        provider_id: vestrace_domain::id::ProviderId::from_uuid(req.provider_id),
        workspace_id: context.workspace_id,
        model_name: req.model_name,
        context_window: req.context_window,
        input_cost_per_mtoken: req.input_cost_per_mtoken,
        output_cost_per_mtoken: req.output_cost_per_mtoken,
        created_at: now(),
    };
    state
        .model_repository()
        .create(&context, &record)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(LegacyModelResponse::from(record)),
    )
        .into_response())
}

/// Every immutable member of one governed Model revision.
///
/// The compatibility Model row and the immutable revision are stated together
/// because they are published together; a surface that created one and looked
/// the other up would be deciding which Connection revision this model runs
/// against. The observed context window and embedding dimension are absent on
/// purpose: they are qualification evidence, not something an operator asserts.
#[derive(Debug, Deserialize)]
pub struct ModelRevisionRequest {
    pub model_id: Uuid,
    pub provider_id: Uuid,
    pub model_name: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
    pub revision_id: Uuid,
    pub connection_id: Uuid,
    pub connection_revision_id: Uuid,
    pub wire_model_id: String,
    pub kind: ModelKind,
    pub execution_guard_id: Uuid,
    pub expected_head_version: u64,
}

pub async fn create_model_revision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<ModelRevisionRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.model_id != model_id {
        return Err(ApiError::bad_request(
            "the path model id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let model = ModelRecord {
        id: vestrace_domain::id::ModelId::from_uuid(request.model_id),
        provider_id: vestrace_domain::id::ProviderId::from_uuid(request.provider_id),
        workspace_id: context.workspace_id,
        model_name: request.model_name,
        context_window: request.context_window,
        input_cost_per_mtoken: request.input_cost_per_mtoken,
        output_cost_per_mtoken: request.output_cost_per_mtoken,
        created_at: at,
    };
    // `from_persisted` is the only constructor that accepts a stated identity.
    // The alternative allocates one, which would mean the caller could not name
    // the revision it is publishing and could not replay this request.
    let revision = ModelRevision::from_persisted(
        ModelRevisionId::from_uuid(request.revision_id),
        context.workspace_id,
        ConnectionRevisionId::from_uuid(request.connection_revision_id),
        request.wire_model_id,
        request.kind,
        ModelObservation::unknown(),
        ModelObservation::unknown(),
    )
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let evidence = serde_json::json!({
        "model_id": request.model_id,
        "model_revision_id": request.revision_id,
    });
    let command = CreateModelRevision {
        model,
        revision,
        connection_id: ConnectionId::from_uuid(request.connection_id),
        execution_guard_id: request.execution_guard_id,
        expected_head_version: request.expected_head_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.clone(),
            workspace_id: context.workspace_id,
            request_hash: request.revision_id.to_string(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(24),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "model.revision.created",
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "model.revision.created",
            "model",
            request.model_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    };
    let receipt = state
        .model_revision_repository()?
        .create_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

/// Names the workspace default this call is publishing. `purpose` travels in
/// the body even though this build only ever sends `"chat"`, because the
/// backend command already carries it generally.
#[derive(Debug, Deserialize)]
pub struct SetWorkspaceModelDefaultRequest {
    pub default_id: Uuid,
    pub model_id: Uuid,
    pub purpose: String,
    pub required_capabilities: Vec<String>,
    pub expected_version: u64,
}

pub async fn set_workspace_model_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<SetWorkspaceModelDefaultRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.model_id != model_id {
        return Err(ApiError::bad_request(
            "the path model id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let evidence = serde_json::json!({
        "default_id": request.default_id,
        "purpose": request.purpose,
        "model_id": request.model_id,
    });
    let command = SetWorkspaceModelDefault {
        default_id: request.default_id,
        workspace_id: context.workspace_id,
        purpose: request.purpose,
        model_id: vestrace_domain::id::ModelId::from_uuid(request.model_id),
        required_capabilities: request.required_capabilities,
        expected_version: request.expected_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.clone(),
            workspace_id: context.workspace_id,
            request_hash: request.default_id.to_string(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(24),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "model.workspace_default.set",
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "model.workspace_default.set",
            "model",
            request.model_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    };
    let receipt = state
        .model_revision_repository()?
        .set_workspace_default_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceModelDefaultQuery {
    pub purpose: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceModelDefaultResponse {
    pub model_id: Option<Uuid>,
    pub purpose: String,
    pub version: u64,
}

pub async fn get_workspace_model_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<WorkspaceModelDefaultQuery>,
) -> Result<Json<WorkspaceModelDefaultResponse>, ApiError> {
    let context = request_context(&headers)?;
    let purpose = query
        .purpose
        .unwrap_or_else(|| vestrace_application::LEGACY_RUN_MODEL_DEFAULT_PURPOSE.to_owned());
    let projection = state
        .model_revision_repository()?
        .get_workspace_default(&context, &purpose)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(WorkspaceModelDefaultResponse {
        model_id: projection.as_ref().map(|p| p.model_id.as_uuid()),
        version: projection.map(|p| p.version).unwrap_or(0),
        purpose,
    }))
}

/// Requests one qualification job for the Model named by the path.
///
/// The job itself is stated over the exact Connection revision and both model
/// revisions, because that is the tuple a q1 run qualifies. The path model id
/// is carried into the evidence rather than into the command: HTTP cannot
/// prove a revision belongs to that model without a lookup, and a lookup here
/// would be this surface deciding what the caller meant.
pub async fn request_model_qualification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<QualificationRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    required_idempotency_key(&headers)?;
    let job_id = request.job_id;
    let record = state
        .qualification_job_repository()?
        .request(&context, request.into_command())
        .await
        .map_err(ApiError::from_application)?;
    tracing::info!(
        model_id = %model_id,
        qualification_job_id = %job_id,
        "model qualification requested"
    );
    Ok((
        StatusCode::CREATED,
        Json(QualificationJobResponse::from(record)),
    )
        .into_response())
}

pub async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ModelResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let models = state
        .model_revision_repository()?
        .list_safe_models(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(models.into_iter().map(ModelResponse::from).collect()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use vestrace_application::{
        ApplicationError, ModelRevisionRepository, RequestContext, SharedModelRevisionRepository,
    };
    use vestrace_domain::embedding::EmbeddingReadinessReason;

    use super::*;
    use crate::{
        api::runs::tests::{TestAllowPolicy, test_state},
        build_router,
    };

    /// Answers the governed projection with one stated model.
    struct StubProjection(Vec<GovernedModelProjection>);

    #[async_trait::async_trait]
    impl ModelRevisionRepository for StubProjection {
        async fn create_governed(
            &self,
            _context: RequestContext,
            _command: CreateModelRevision,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            unreachable!("this suite reads the projection only")
        }

        async fn set_workspace_default_governed(
            &self,
            _context: RequestContext,
            _command: vestrace_application::SetWorkspaceModelDefault,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            unreachable!("this suite reads the projection only")
        }

        async fn list_safe_models(
            &self,
            _context: &RequestContext,
        ) -> Result<Vec<GovernedModelProjection>, ApplicationError> {
            Ok(self.0.clone())
        }
    }

    fn model_with(blockers: Vec<String>) -> GovernedModelProjection {
        GovernedModelProjection {
            id: vestrace_domain::id::ModelId::new(),
            revision_id: Some(ModelRevisionId::new()),
            state: "blocked".to_owned(),
            qualification_state: "qualified".to_owned(),
            blockers,
        }
    }

    fn listing(repository: SharedModelRevisionRepository) -> Request<Body> {
        let _ = repository;
        Request::builder()
            .method("GET")
            .uri("/v1/models")
            .header("x-workspace-id", Uuid::now_v7().to_string())
            .header("x-principal-id", Uuid::now_v7().to_string())
            .body(Body::empty())
            .unwrap()
    }

    /// The embedding readiness reasons reach a caller by their exact names.
    ///
    /// Exact because an operator reading this list is meant to act on it, and
    /// because the same names are what `doctor` prints and what migration 0197
    /// raises. A surface that paraphrased one would give three descriptions of
    /// one condition.
    #[tokio::test]
    async fn the_model_listing_carries_embedding_readiness_by_its_exact_names() {
        let repository: SharedModelRevisionRepository =
            Arc::new(StubProjection(vec![model_with(vec![
                "qualification_required".to_owned(),
                EmbeddingReadinessReason::LegacyAdoptionRequired
                    .as_str()
                    .to_owned(),
                EmbeddingReadinessReason::GenerationNotReady
                    .as_str()
                    .to_owned(),
            ])]));
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_model_revision_repository(repository.clone()),
        );

        let response = app.oneshot(listing(repository)).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let blockers: Vec<&str> = json[0]["blockers"]
            .as_array()
            .expect("a model states its blockers")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();

        assert!(
            blockers.contains(&"embedding-legacy-adoption-required"),
            "{blockers:?}"
        );
        assert!(
            blockers.contains(&"embedding-generation-not-ready"),
            "{blockers:?}"
        );
        // The qualification blockers are still there. A model can be both
        // unqualified and unable to serve retrieval, and an operator fixing one
        // must not be told the other has gone.
        assert!(blockers.contains(&"qualification_required"), "{blockers:?}");

        // Every embedding-shaped blocker parses back into the closed
        // vocabulary. One that did not would be a name an operator could read
        // and no client could handle.
        for blocker in &blockers {
            if blocker.starts_with("embedding-") {
                assert!(
                    blocker.parse::<EmbeddingReadinessReason>().is_ok(),
                    "{blocker} is not in the vocabulary"
                );
            }
        }
    }

    /// And the one reason storage cannot know never reaches this surface.
    ///
    /// `/v1/models` is answered from a database projection. If it ever carried
    /// `embedding-index-not-loaded`, an operator would rebuild an index that
    /// may already be loaded in a worker this transaction cannot see.
    #[tokio::test]
    async fn the_model_listing_never_claims_an_index_is_not_loaded() {
        assert!(
            !EmbeddingReadinessReason::IndexNotLoaded.is_observable_from_storage(),
            "the rule this surface depends on"
        );
        let repository: SharedModelRevisionRepository =
            Arc::new(StubProjection(vec![model_with(vec![
                EmbeddingReadinessReason::GenerationNotReady
                    .as_str()
                    .to_owned(),
            ])]));
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_model_revision_repository(repository.clone()),
        );

        let response = app.oneshot(listing(repository)).await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let serialized = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            !serialized.contains(EmbeddingReadinessReason::IndexNotLoaded.as_str()),
            "a database-backed listing must not assert process-local index \
             presence: {serialized}"
        );
    }

    #[derive(Default)]
    struct SpyDefaults {
        set: std::sync::Mutex<Option<vestrace_application::SetWorkspaceModelDefault>>,
        stored: std::sync::Mutex<Option<vestrace_application::WorkspaceModelDefaultProjection>>,
    }

    #[async_trait::async_trait]
    impl ModelRevisionRepository for SpyDefaults {
        async fn create_governed(
            &self,
            _context: RequestContext,
            _command: CreateModelRevision,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            unreachable!("this suite exercises the default pointer only")
        }

        async fn set_workspace_default_governed(
            &self,
            _context: RequestContext,
            command: vestrace_application::SetWorkspaceModelDefault,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            *self.stored.lock().unwrap() =
                Some(vestrace_application::WorkspaceModelDefaultProjection {
                    model_id: command.model_id,
                    version: command.expected_version + 1,
                });
            *self.set.lock().unwrap() = Some(command);
            Ok(vestrace_application::GovernedMutationReceipt {
                audit_event_id: vestrace_domain::id::AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: vec![],
            })
        }

        async fn get_workspace_default(
            &self,
            _context: &RequestContext,
            _purpose: &str,
        ) -> Result<Option<vestrace_application::WorkspaceModelDefaultProjection>, ApplicationError>
        {
            Ok(*self.stored.lock().unwrap())
        }

        async fn list_safe_models(
            &self,
            _context: &RequestContext,
        ) -> Result<Vec<GovernedModelProjection>, ApplicationError> {
            Ok(Vec::new())
        }
    }

    fn default_request(uri: &str, idempotency_key: Option<&str>, body: &str) -> Request<Body> {
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
    async fn setting_the_workspace_default_reaches_the_application_and_a_later_read_sees_it() {
        use std::sync::Arc;
        let spy = Arc::new(SpyDefaults::default());
        let model_id = Uuid::now_v7();
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(spy.clone()),
        );

        let body = serde_json::json!({
            "default_id": Uuid::now_v7(),
            "model_id": model_id,
            "purpose": "chat",
            "required_capabilities": ["chat.completions"],
            "expected_version": 0,
        })
        .to_string();

        let response = app
            .clone()
            .oneshot(default_request(
                &format!("/v1/models/{model_id}/default"),
                Some(&Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let recorded = spy
            .set
            .lock()
            .unwrap()
            .take()
            .expect("the set reached the application");
        assert_eq!(recorded.model_id.as_uuid(), model_id);
        assert_eq!(recorded.purpose, "chat");

        let read = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/models/default?purpose=chat")
                    .header("x-workspace-id", Uuid::now_v7().to_string())
                    .header("x-principal-id", Uuid::now_v7().to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);
        let read_body = to_bytes(read.into_body(), 4096).await.unwrap();
        let read_body: serde_json::Value = serde_json::from_slice(&read_body).unwrap();
        assert_eq!(read_body["model_id"], model_id.to_string());
    }

    #[tokio::test]
    async fn a_default_read_with_nothing_configured_answers_a_null_model_id_not_an_error() {
        use std::sync::Arc;
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(Arc::new(SpyDefaults::default())),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/models/default?purpose=chat")
                    .header("x-workspace-id", Uuid::now_v7().to_string())
                    .header("x-principal-id", Uuid::now_v7().to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["model_id"].is_null());
    }

    #[tokio::test]
    async fn a_default_write_whose_path_and_body_disagree_is_refused() {
        use std::sync::Arc;
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(Arc::new(SpyDefaults::default())),
        );
        let body = serde_json::json!({
            "default_id": Uuid::now_v7(),
            "model_id": Uuid::now_v7(),
            "purpose": "chat",
            "required_capabilities": ["chat.completions"],
            "expected_version": 0,
        })
        .to_string();

        let response = app
            .oneshot(default_request(
                &format!("/v1/models/{}/default", Uuid::now_v7()),
                Some(&Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
