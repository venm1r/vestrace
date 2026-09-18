use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_application::{
    CandidateCredentialAbandonCommand, CreateConnectionRevision, CredentialActivationCommand,
    CredentialActivationError, CredentialRevocationCommand, CredentialRotationCommand,
    GovernedConnectionProjection, GovernedMutationReceipt, IdempotencyRecord, OutboxMessage,
    PublishConnectionAdmissionPolicy, QualificationJobRecord, QualificationJobRequest,
};
use vestrace_domain::{
    AuditEvent, ConnectionAdmissionPolicyId, ConnectionAuthMode, ConnectionId, ConnectionKind,
    ConnectionRevisionId, ConnectionTransportPolicy, ConnectorId, CredentialActivationGuardId,
    CredentialRevisionId, CredentialSlotId, ModelRevisionId, NoAuthBindingRevisionId,
    QualificationJobId, QualificationJobState, QualificationTargetBinding,
    connection::{Connection, ConnectionStatus},
    id::AuditEventId,
    models::ConnectionAdmissionLimits,
    time::now,
};

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{ApiError, context::request_context, required_idempotency_key};

/// How long a recorded Request-Id keeps answering with its original response.
///
/// A governed mutation is retried by operators and by scripts, and the window
/// has to outlive the retry rather than the request: an expired key turns a
/// replay into a second immutable revision.
const IDEMPOTENCY_WINDOW_HOURS: i64 = 24;

/// An opaque governed Connection projection.  It never carries legacy display,
/// connector, or routing fields.
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub id: uuid::Uuid,
    pub revision_id: Option<uuid::Uuid>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
    pub no_auth_binding_revision_id: Option<uuid::Uuid>,
    pub execution_guard_id: Option<uuid::Uuid>,
}

impl From<GovernedConnectionProjection> for ConnectionResponse {
    fn from(projection: GovernedConnectionProjection) -> Self {
        Self {
            id: projection.id.as_uuid(),
            revision_id: projection.revision_id.map(|id| id.as_uuid()),
            state: projection.state,
            qualification_state: projection.qualification_state,
            blockers: projection.blockers,
            no_auth_binding_revision_id: projection.no_auth_binding_revision_id,
            execution_guard_id: projection.execution_guard_id,
        }
    }
}

/// Every immutable member of one governed Connection revision, named by the
/// caller.
///
/// Nothing here is defaulted, inferred or looked up. A surface that filled in
/// the head version would let two callers race into one revision believing
/// each had the current one; a surface that invented a revision or guard id
/// would decide what the operator meant to publish. The only fields taken from
/// elsewhere are the workspace and principal, which are the authenticated
/// identity rather than a choice.
#[derive(Debug, Deserialize)]
pub struct ConnectionRevisionRequest {
    pub connection_id: Uuid,
    pub connector_id: Uuid,
    pub name: String,
    pub revision_id: Uuid,
    pub execution_guard_id: Uuid,
    pub kind: ConnectionKind,
    pub logical_base_url: String,
    pub runtime_base_url: String,
    pub adapter_profile_revision: String,
    pub transport_policy: ConnectionTransportPolicy,
    pub auth_mode: ConnectionAuthMode,
    /// Required by every credential auth mode and refused by `none`. The
    /// repository enforces that pairing; sending the wrong one is a request
    /// error rather than something to correct here.
    pub credential_slot_id: Option<Uuid>,
    pub expected_head_version: u64,
}

/// What a governed mutation leaves behind. Identities only: the caller already
/// knows what it asked for, and the response exists to name the evidence.
#[derive(Debug, Serialize)]
pub struct GovernedMutationResponse {
    pub audit_event_id: Uuid,
    pub idempotency_key: Option<String>,
    pub outbox_message_ids: Vec<Uuid>,
}

impl From<GovernedMutationReceipt> for GovernedMutationResponse {
    fn from(receipt: GovernedMutationReceipt) -> Self {
        Self {
            audit_event_id: receipt.audit_event_id.as_uuid(),
            idempotency_key: receipt.idempotency_key,
            outbox_message_ids: receipt
                .outbox_message_ids
                .into_iter()
                .map(|id| id.as_uuid())
                .collect(),
        }
    }
}

pub fn connection_routes() -> axum::Router<AppState> {
    let router = mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/connections"),
        get(list_connections),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/connections"),
        post(create_connection),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/connections/{id}/revisions"),
        post(revise_connection),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/connections/{id}/admission-policies"),
        post(publish_admission_policy),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/connections/{id}/qualifications"),
        post(request_connection_qualification),
    );
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/connections/{id}/credentials"),
        post(activate_credential),
    );
    let router = mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/activate",
        ),
        post(rotate_credential),
    );
    let router = mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/revoke",
        ),
        post(revoke_credential),
    );
    mount(
        router,
        route_descriptor(
            &Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/abandon",
        ),
        post(abandon_credential),
    )
}

async fn list_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConnectionResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.connection_revision_repository()?;
    let connections = repository
        .list_safe_connections(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(connections.into_iter().map(Into::into).collect()))
}

async fn create_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConnectionRevisionRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    let command = command(&context, &headers, request, "connection.revision.created")?;
    let receipt = state
        .connection_revision_repository()?
        .create_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

async fn revise_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<ConnectionRevisionRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    // The path and the body must agree. Preferring one over the other would
    // publish a revision under a Connection the caller did not name twice.
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    let command = command(&context, &headers, request, "connection.revision.published")?;
    let receipt = state
        .connection_revision_repository()?
        .revise_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

/// The four admission limits, stated as plain integers and validated once.
///
/// `ConnectionAdmissionLimits` derives `Deserialize` over private fields, which
/// would construct it straight from the body and bypass the bounds its `new`
/// enforces. Parsing into this shape and then calling `new` is what keeps a
/// body that asks for zero in-flight dispatches, or a fifteen-minute queue
/// wait, a request error rather than a stored policy.
#[derive(Debug, Deserialize)]
pub struct AdmissionPolicyRequest {
    pub connection_id: Uuid,
    pub policy_revision_id: Uuid,
    pub max_in_flight: u8,
    pub requests_per_60_seconds: u32,
    pub queue_wait_timeout_seconds: u16,
    pub provider_throttle_cap_seconds: u16,
    /// Zero means "this Connection has no policy yet". Every later publication
    /// must name the version it is replacing, so two operators cannot both
    /// believe they published the policy the next dispatch will use.
    pub expected_head_version: u64,
}

async fn publish_admission_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<AdmissionPolicyRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let limits = ConnectionAdmissionLimits::new(
        request.max_in_flight,
        request.requests_per_60_seconds,
        request.queue_wait_timeout_seconds,
        request.provider_throttle_cap_seconds,
    )
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let at = now();
    let action = "connection.admission_policy.published";
    let evidence = serde_json::json!({
        "connection_id": request.connection_id,
        "connection_admission_policy_id": request.policy_revision_id,
    });
    let command = PublishConnectionAdmissionPolicy {
        connection_id: ConnectionId::from_uuid(request.connection_id),
        policy_revision_id: ConnectionAdmissionPolicyId::from_uuid(request.policy_revision_id),
        limits,
        expected_head_version: request.expected_head_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key,
            workspace_id: context.workspace_id,
            request_hash: request.policy_revision_id.to_string(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(IDEMPOTENCY_WINDOW_HOURS),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            action,
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            action,
            "connection",
            request.connection_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    };
    let receipt = state
        .connection_revision_repository()?
        .publish_admission_policy_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

/// Builds the exact application command from what the caller stated.
///
/// The Request-Id becomes the idempotency key, so the same key with the same
/// canonical request replays its original response and the same key with a
/// different one conflicts. The request hash is the revision identity rather
/// than a digest of the body: the revision is what the caller is publishing,
/// and hashing the bytes would make an insignificant reordering look like a
/// different request.
fn command(
    context: &vestrace_application::RequestContext,
    headers: &HeaderMap,
    request: ConnectionRevisionRequest,
    action: &'static str,
) -> Result<CreateConnectionRevision, ApiError> {
    let idempotency_key = required_idempotency_key(headers)?;
    let at = now();
    let connection = Connection {
        id: ConnectionId::from_uuid(request.connection_id),
        connector_id: ConnectorId::from_uuid(request.connector_id),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        name: request.name,
        // The compatibility row's only meaningful value. Governed executability
        // comes from the revision and its qualification, never from here.
        status: ConnectionStatus::Active,
        created_at: at,
    };
    let evidence = serde_json::json!({
        "connection_id": request.connection_id,
        "connection_revision_id": request.revision_id,
    });
    Ok(CreateConnectionRevision {
        connection,
        revision_id: ConnectionRevisionId::from_uuid(request.revision_id),
        execution_guard_id: request.execution_guard_id,
        kind: request.kind,
        logical_base_url: request.logical_base_url,
        runtime_base_url: request.runtime_base_url,
        adapter_profile_revision: request.adapter_profile_revision,
        transport_policy: request.transport_policy,
        auth_mode: request.auth_mode,
        credential_slot_id: request.credential_slot_id.map(CredentialSlotId::from_uuid),
        expected_head_version: request.expected_head_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.clone(),
            workspace_id: context.workspace_id,
            request_hash: request.revision_id.to_string(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(IDEMPOTENCY_WINDOW_HOURS),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            action,
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            action,
            "connection",
            request.connection_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    })
}

/// The pinned auth branch a qualification job is requested against.
///
/// Declared here rather than deserialized into the domain type: that type is
/// deliberately not `Deserialize`, so a caller cannot post a binding straight
/// into the model. This closed shape is parsed and then mapped, which is the
/// only way the branch reaches the domain.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "branch")]
pub enum QualificationTargetRequest {
    Credential {
        revision_id: Uuid,
        slot_id: Uuid,
        activation_guard_id: Uuid,
        expected_slot_version: u64,
    },
    NoAuth {
        binding_revision_id: Uuid,
    },
}

impl From<QualificationTargetRequest> for QualificationTargetBinding {
    fn from(request: QualificationTargetRequest) -> Self {
        match request {
            QualificationTargetRequest::Credential {
                revision_id,
                slot_id,
                activation_guard_id,
                expected_slot_version,
            } => Self::Credential {
                revision_id: CredentialRevisionId::from_uuid(revision_id),
                slot_id: CredentialSlotId::from_uuid(slot_id),
                activation_guard_id: CredentialActivationGuardId::from_uuid(activation_guard_id),
                expected_slot_version,
            },
            QualificationTargetRequest::NoAuth {
                binding_revision_id,
            } => Self::NoAuth {
                binding_revision_id: NoAuthBindingRevisionId::from_uuid(binding_revision_id),
            },
        }
    }
}

/// One qualification job over an exact Connection revision and the two model
/// revisions it will qualify.
#[derive(Debug, Deserialize)]
pub struct QualificationRequest {
    pub job_id: Uuid,
    pub target_binding_id: Uuid,
    pub connection_id: Uuid,
    pub connection_revision_id: Uuid,
    pub target: QualificationTargetRequest,
    pub chat_model_revision_id: Uuid,
    pub embedding_model_revision_id: Uuid,
}

impl QualificationRequest {
    pub(crate) fn into_command(self) -> QualificationJobRequest {
        QualificationJobRequest {
            job_id: QualificationJobId::from_uuid(self.job_id),
            target_binding_id: self.target_binding_id,
            connection_id: ConnectionId::from_uuid(self.connection_id),
            connection_revision_id: ConnectionRevisionId::from_uuid(self.connection_revision_id),
            target: self.target.into(),
            chat_model_revision_id: ModelRevisionId::from_uuid(self.chat_model_revision_id),
            embedding_model_revision_id: ModelRevisionId::from_uuid(
                self.embedding_model_revision_id,
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct QualificationJobResponse {
    pub id: Uuid,
    pub target_binding_id: Uuid,
    pub state: QualificationJobState,
}

impl From<QualificationJobRecord> for QualificationJobResponse {
    fn from(record: QualificationJobRecord) -> Self {
        Self {
            id: record.id.as_uuid(),
            target_binding_id: record.target_binding_id,
            state: record.state,
        }
    }
}

async fn request_connection_qualification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<QualificationRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    // The Request-Id is mandatory here as on every governed mutation, even
    // though the job id is what makes the request itself idempotent: a caller
    // that cannot name its request cannot be told what its retry did.
    required_idempotency_key(&headers)?;
    let record = state
        .qualification_job_repository()?
        .request(&context, request.into_command())
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(QualificationJobResponse::from(record)),
    )
        .into_response())
}

/// The exact first publication of a Candidate credential into an empty slot.
#[derive(Debug, Deserialize)]
pub struct CredentialActivationRequest {
    pub connection_id: Uuid,
    pub credential_slot_id: Uuid,
    pub execution_guard_id: Uuid,
    pub activation_guard_id: Uuid,
    pub credential_revision_id: Uuid,
    pub credential_intent_id: Uuid,
    pub connection_qualification_revision_id: Uuid,
    pub expected_slot_version: u64,
}

/// One ordinary replacement of the current revision by a named Candidate. The
/// previous revision is stated, not looked up: rotating away from whatever
/// happens to be current is how two operators overwrite each other.
#[derive(Debug, Deserialize)]
pub struct CredentialRotationRequest {
    pub connection_id: Uuid,
    pub credential_slot_id: Uuid,
    pub execution_guard_id: Uuid,
    pub activation_guard_id: Uuid,
    pub previous_credential_revision_id: Uuid,
    pub activated_credential_intent_id: Uuid,
    pub connection_qualification_revision_id: Uuid,
    pub expected_slot_version: u64,
}

#[derive(Debug, Deserialize)]
pub struct CredentialRevocationRequest {
    pub connection_id: Uuid,
    pub credential_slot_id: Uuid,
    pub execution_guard_id: Uuid,
    pub activation_guard_id: Uuid,
    pub credential_intent_id: Uuid,
    pub expected_slot_version: u64,
}

#[derive(Debug, Deserialize)]
pub struct CredentialAbandonRequest {
    pub credential_intent_id: Uuid,
    pub expected_association_version: u64,
}

/// A prepared erasure, named by identity only. The material key is opaque and
/// the receipt says whether the fence has already been finalized.
#[derive(Debug, Serialize)]
pub struct ErasurePreparationResponse {
    pub preparation_id: Uuid,
    pub material_key_id: Uuid,
    pub finalized: bool,
}

fn evidence(
    context: &vestrace_application::RequestContext,
    action: &'static str,
    resource_id: Uuid,
    payload: serde_json::Value,
    at: vestrace_domain::time::Timestamp,
) -> Result<(AuditEvent, Vec<OutboxMessage>), ApiError> {
    let audit = AuditEvent::new(
        AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        action,
        "credential",
        resource_id,
        payload.clone(),
        at,
    )
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok((
        audit,
        vec![OutboxMessage::new(
            context.workspace_id,
            action,
            payload,
            at,
        )],
    ))
}

fn idempotency(
    context: &vestrace_application::RequestContext,
    key: String,
    request_hash: String,
    at: vestrace_domain::time::Timestamp,
) -> IdempotencyRecord {
    IdempotencyRecord {
        idempotency_key: key,
        workspace_id: context.workspace_id,
        request_hash,
        response_payload: None,
        status: "completed".to_owned(),
        created_at: at,
        expires_at: at + chrono::Duration::hours(IDEMPOTENCY_WINDOW_HOURS),
    }
}

/// Map the credential boundary's typed failures onto the surface.
///
/// `EmbeddingTransitionRequired` is not an authorization failure and not a bad
/// request: it names evidence a later package owns, so it keeps its own code
/// rather than collapsing into a generic refusal.
fn credential_error(error: CredentialActivationError) -> ApiError {
    match error {
        CredentialActivationError::EmbeddingTransitionRequired => ApiError::refused(
            "embedding_transition_required",
            "a live embedding dependency requires an embedding transition",
        ),
        CredentialActivationError::Application(error) => ApiError::from_application(error),
    }
}

async fn activate_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<CredentialActivationRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let payload = serde_json::json!({
        "connection_id": request.connection_id,
        "credential_revision_id": request.credential_revision_id,
    });
    let (audit, outbox) = evidence(
        &context,
        "credential.activated",
        request.credential_revision_id,
        payload,
        at,
    )?;
    let command = CredentialActivationCommand {
        connection_id: ConnectionId::from_uuid(request.connection_id),
        credential_slot_id: CredentialSlotId::from_uuid(request.credential_slot_id),
        execution_guard_id: request.execution_guard_id,
        activation_guard_id: request.activation_guard_id,
        credential_revision_id: request.credential_revision_id,
        credential_intent_id: request.credential_intent_id,
        connection_qualification_revision_id: request.connection_qualification_revision_id,
        expected_slot_version: request.expected_slot_version,
        idempotency: Some(idempotency(
            &context,
            idempotency_key,
            request.credential_revision_id.to_string(),
            at,
        )),
        outbox,
        audit,
    };
    let receipt = state
        .credential_activation_repository()?
        .activate_first(context, command)
        .await
        .map_err(credential_error)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

async fn rotate_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((connection_id, revision_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<CredentialRotationRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let payload = serde_json::json!({
        "connection_id": request.connection_id,
        "activated_credential_revision_id": revision_id,
        "previous_credential_revision_id": request.previous_credential_revision_id,
    });
    let (audit, outbox) = evidence(&context, "credential.rotated", revision_id, payload, at)?;
    let command = CredentialRotationCommand {
        connection_id: ConnectionId::from_uuid(request.connection_id),
        credential_slot_id: CredentialSlotId::from_uuid(request.credential_slot_id),
        execution_guard_id: request.execution_guard_id,
        activation_guard_id: request.activation_guard_id,
        previous_credential_revision_id: request.previous_credential_revision_id,
        // The path names the revision being activated; the body may not
        // contradict it, so it is not asked for twice.
        activated_credential_revision_id: revision_id,
        activated_credential_intent_id: request.activated_credential_intent_id,
        connection_qualification_revision_id: request.connection_qualification_revision_id,
        expected_slot_version: request.expected_slot_version,
        idempotency: Some(idempotency(
            &context,
            idempotency_key,
            revision_id.to_string(),
            at,
        )),
        outbox,
        audit,
    };
    let receipt = state
        .credential_activation_repository()?
        .rotate(context, command)
        .await
        .map_err(credential_error)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

async fn revoke_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((connection_id, revision_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<CredentialRevocationRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.connection_id != connection_id {
        return Err(ApiError::bad_request(
            "the path connection id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let payload = serde_json::json!({
        "connection_id": request.connection_id,
        "credential_revision_id": revision_id,
    });
    let (audit, outbox) = evidence(&context, "credential.revoked", revision_id, payload, at)?;
    let command = CredentialRevocationCommand {
        connection_id: ConnectionId::from_uuid(request.connection_id),
        credential_slot_id: CredentialSlotId::from_uuid(request.credential_slot_id),
        execution_guard_id: request.execution_guard_id,
        activation_guard_id: request.activation_guard_id,
        credential_revision_id: revision_id,
        credential_intent_id: request.credential_intent_id,
        expected_slot_version: request.expected_slot_version,
        idempotency: Some(idempotency(
            &context,
            idempotency_key,
            revision_id.to_string(),
            at,
        )),
        outbox,
        audit,
    };
    let receipt = state
        .credential_activation_repository()?
        .revoke(context, command)
        .await
        .map_err(credential_error)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

/// Abandons a Candidate association and prepares its erasure.
///
/// This is phase one only. The host-vault fence that finishes the erasure is
/// owned by `CandidateCredentialAbandonService`, which needs an erasure
/// authority and a vault this surface does not hold; until those are composed,
/// the response says what was prepared rather than claiming the material is
/// gone.
async fn abandon_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_connection_id, revision_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<CredentialAbandonRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let payload = serde_json::json!({
        "credential_revision_id": revision_id,
        "credential_intent_id": request.credential_intent_id,
    });
    let (audit, outbox) = evidence(&context, "credential.abandoned", revision_id, payload, at)?;
    let command = CandidateCredentialAbandonCommand {
        credential_intent_id: request.credential_intent_id,
        expected_association_version: request.expected_association_version,
        idempotency: idempotency(&context, idempotency_key, revision_id.to_string(), at),
        outbox,
        audit,
    };
    let preparation = state
        .credential_activation_repository()?
        .prepare_candidate_abandon(context, command)
        .await
        .map_err(credential_error)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ErasurePreparationResponse {
            preparation_id: preparation.id(),
            material_key_id: preparation.material_key_id().as_uuid(),
            finalized: preparation.finalized_receipt().is_some(),
        }),
    )
        .into_response())
}

/// These routes exist so an operator can publish a governed Connection without
/// the surface deciding anything on their behalf. What is under test is
/// therefore not that a mutation happens, but that exactly what was stated
/// reaches the application and nothing else does.
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use vestrace_application::{
        ApplicationError, ConnectionRevisionRepository, GovernedMutationReceipt, RequestContext,
    };
    use vestrace_domain::id::{AuditEventId, OutboxId};

    use super::*;
    use crate::api::runs::tests::{TestAllowPolicy, test_state};
    use crate::build_router;

    /// Records the exact command the transport handed the application.
    #[derive(Default)]
    struct SpyRevisions {
        created: Mutex<Option<CreateConnectionRevision>>,
        revised: Mutex<Option<CreateConnectionRevision>>,
        published_policy: Mutex<Option<PublishConnectionAdmissionPolicy>>,
    }

    #[async_trait::async_trait]
    impl ConnectionRevisionRepository for SpyRevisions {
        async fn create_governed(
            &self,
            _context: RequestContext,
            command: CreateConnectionRevision,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            *self.created.lock().unwrap() = Some(command);
            Ok(GovernedMutationReceipt {
                audit_event_id: AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: vec![OutboxId::new()],
            })
        }

        async fn revise_governed(
            &self,
            _context: RequestContext,
            command: CreateConnectionRevision,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            *self.revised.lock().unwrap() = Some(command);
            Ok(GovernedMutationReceipt {
                audit_event_id: AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: Vec::new(),
            })
        }

        async fn publish_admission_policy_governed(
            &self,
            _context: RequestContext,
            command: PublishConnectionAdmissionPolicy,
        ) -> Result<GovernedMutationReceipt, ApplicationError> {
            *self.published_policy.lock().unwrap() = Some(command);
            Ok(GovernedMutationReceipt {
                audit_event_id: AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: Vec::new(),
            })
        }
    }

    struct Posted {
        connection_id: Uuid,
        revision_id: Uuid,
        guard_id: Uuid,
        slot_id: Uuid,
        body: String,
    }

    fn posted() -> Posted {
        let connection_id = Uuid::now_v7();
        let revision_id = Uuid::now_v7();
        let guard_id = Uuid::now_v7();
        let slot_id = Uuid::now_v7();
        let body = serde_json::json!({
            "connection_id": connection_id,
            "connector_id": Uuid::now_v7(),
            "name": "governed-connection",
            "revision_id": revision_id,
            "execution_guard_id": guard_id,
            "kind": "open_ai_chat_completions_v1",
            "logical_base_url": "https://provider.test/v1",
            "runtime_base_url": "https://provider.test/v1",
            "adapter_profile_revision": "openai-chat-completions/v1",
            "transport_policy": {"kind": "remote_https"},
            "auth_mode": "bearer",
            "credential_slot_id": slot_id,
            "expected_head_version": 7,
        })
        .to_string();
        Posted {
            connection_id,
            revision_id,
            guard_id,
            slot_id,
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
    async fn a_created_revision_reaches_the_application_exactly_as_it_was_stated() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let post = posted();
        let key = Uuid::now_v7().to_string();

        let response = app
            .oneshot(request("/v1/connections", Some(key.clone()), &post.body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let command = spy.created.lock().unwrap().take().expect("create reached");
        assert_eq!(command.connection.id.as_uuid(), post.connection_id);
        assert_eq!(command.revision_id.as_uuid(), post.revision_id);
        assert_eq!(command.execution_guard_id, post.guard_id);
        assert_eq!(
            command.credential_slot_id.map(|slot| slot.as_uuid()),
            Some(post.slot_id)
        );
        // Not defaulted, not read from a head: the caller's stated version is
        // the one the guarded head will be compared against.
        assert_eq!(command.expected_head_version, 7);
        assert_eq!(command.kind, ConnectionKind::OpenAiChatCompletionsV1);
        assert_eq!(command.auth_mode, ConnectionAuthMode::Bearer);
        assert_eq!(
            command.transport_policy,
            ConnectionTransportPolicy::RemoteHttps
        );
        // The caller's key, kept as sent, so a redelivery is a replay rather
        // than a second immutable revision.
        assert_eq!(
            command
                .idempotency
                .as_ref()
                .map(|record| record.idempotency_key.as_str()),
            Some(key.as_str())
        );
        assert!(spy.revised.lock().unwrap().is_none());
    }

    /// A mutation nobody can name cannot be replayed, and a key the server
    /// invents deduplicates nothing.
    #[tokio::test]
    async fn a_governed_mutation_without_an_idempotency_key_is_refused() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let post = posted();

        let response = app
            .oneshot(request("/v1/connections", None, &post.body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.created.lock().unwrap().is_none());
    }

    /// Preferring the path or the body would publish a revision under a
    /// Connection the caller named only once.
    #[tokio::test]
    async fn a_revision_whose_path_and_body_disagree_is_refused() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let post = posted();

        let response = app
            .oneshot(request(
                &format!("/v1/connections/{}/revisions", Uuid::now_v7()),
                Some(Uuid::now_v7().to_string()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.revised.lock().unwrap().is_none());
    }

    /// An omitted immutable field is a refusal rather than a default. Filling in
    /// a head version would let two callers race into one revision each
    /// believing it held the current one.
    #[tokio::test]
    async fn an_omitted_immutable_field_is_refused_rather_than_defaulted() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let mut body: serde_json::Value = serde_json::from_str(&posted().body).unwrap();
        body.as_object_mut()
            .unwrap()
            .remove("expected_head_version");

        let response = app
            .oneshot(request(
                "/v1/connections",
                Some(Uuid::now_v7().to_string()),
                &body.to_string(),
            ))
            .await
            .unwrap();

        assert!(
            response.status().is_client_error(),
            "an incomplete governed command must not be accepted"
        );
        assert!(spy.created.lock().unwrap().is_none());
    }

    /// A missing authority is not an empty success. Without the repository the
    /// route must refuse rather than report a mutation that never happened.
    #[tokio::test]
    async fn an_unconfigured_authority_refuses_instead_of_reporting_success() {
        let app = build_router(test_state().with_policy(Arc::new(TestAllowPolicy)));
        let post = posted();

        let response = app
            .oneshot(request(
                "/v1/connections",
                Some(Uuid::now_v7().to_string()),
                &post.body,
            ))
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|message| message.contains("not configured")),
            "{body}"
        );
    }

    fn admission_body(connection_id: Uuid, max_in_flight: i64) -> String {
        serde_json::json!({
            "connection_id": connection_id,
            "policy_revision_id": Uuid::now_v7(),
            "max_in_flight": max_in_flight,
            "requests_per_60_seconds": 60,
            "queue_wait_timeout_seconds": 30,
            "provider_throttle_cap_seconds": 900,
            "expected_head_version": 0,
        })
        .to_string()
    }

    /// The admission policy reaches the application as the caller stated it.
    ///
    /// Version zero is the interesting value: it is how a caller says the
    /// Connection has no policy yet, and it is the only version that may create
    /// the head. A surface that dropped it, or filled it in, would let the
    /// second publisher overwrite a policy it never read.
    #[tokio::test]
    async fn a_published_admission_policy_reaches_the_application_as_stated() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let connection_id = Uuid::now_v7();

        let response = app
            .oneshot(request(
                &format!("/v1/connections/{connection_id}/admission-policies"),
                Some(Uuid::now_v7().to_string()),
                &admission_body(connection_id, 4),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let command = spy
            .published_policy
            .lock()
            .unwrap()
            .take()
            .expect("the publication reached the application");
        assert_eq!(command.connection_id.as_uuid(), connection_id);
        assert_eq!(command.expected_head_version, 0);
        assert_eq!(command.limits.max_in_flight(), 4);
        assert_eq!(command.limits.provider_throttle_cap_seconds(), 900);
    }

    /// A body outside the domain's bounds is refused at the edge.
    ///
    /// `ConnectionAdmissionLimits` derives `Deserialize` over private fields, so
    /// deserializing the domain type straight from the body would construct a
    /// policy its own `new` refuses -- zero in-flight dispatches, in this case,
    /// which admits nothing and would silently stall every dispatch on the
    /// Connection. The route parses integers and calls `new`, so the refusal
    /// happens before anything is published.
    #[tokio::test]
    async fn an_admission_policy_outside_the_domain_bounds_is_refused() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );
        let connection_id = Uuid::now_v7();

        let response = app
            .oneshot(request(
                &format!("/v1/connections/{connection_id}/admission-policies"),
                Some(Uuid::now_v7().to_string()),
                &admission_body(connection_id, 0),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.published_policy.lock().unwrap().is_none());
    }

    /// The path and the body must name the same Connection.
    #[tokio::test]
    async fn an_admission_policy_whose_path_and_body_disagree_is_refused() {
        let spy = Arc::new(SpyRevisions::default());
        let app = build_router(
            test_state()
                .with_policy(Arc::new(TestAllowPolicy))
                .with_connection_revision_repository(spy.clone()),
        );

        let response = app
            .oneshot(request(
                &format!("/v1/connections/{}/admission-policies", Uuid::now_v7()),
                Some(Uuid::now_v7().to_string()),
                &admission_body(Uuid::now_v7(), 4),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(spy.published_policy.lock().unwrap().is_none());
    }
}
