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
    ModelRecord, OutboxMessage,
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
