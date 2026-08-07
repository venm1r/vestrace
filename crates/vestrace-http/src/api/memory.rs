use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::{
    ActorRef,
    id::{EventId, MemoryId, SessionId},
};

use crate::AppState;

use super::{ApiError, context::request_context};

const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct EventResponse {
    pub id: Uuid,
    pub event_type: String,
    pub created_at: vestrace_domain::Timestamp,
}

pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEventRequest>,
) -> Result<(axum::http::StatusCode, Json<EventResponse>), ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = required_string_header(&headers, REQUEST_ID_HEADER)?;

    let cmd = vestrace_application::RecordEventCommand {
        id: EventId::new(),
        session_id: request.session_id.map(SessionId::from_uuid),
        event_type: request.event_type,
        actor: ActorRef::User(context.principal_id.to_string()),
        subject: None,
        payload: request.payload,
        idempotency_key,
    };

    let event = state
        .memory_use_cases()
        .record_event(&context, cmd)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(EventResponse {
            id: event.id.as_uuid(),
            event_type: event.event_type,
            created_at: event.created_at,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    pub kind: String,
    pub content: String,
    pub confidence: f32,
    pub importance: f32,
    pub source_event_id: Uuid,
    pub evidence_role: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryResponse {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub created_at: vestrace_domain::Timestamp,
    pub updated_at: vestrace_domain::Timestamp,
}

pub async fn create_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMemoryRequest>,
) -> Result<(axum::http::StatusCode, Json<MemoryResponse>), ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = required_string_header(&headers, REQUEST_ID_HEADER)?;

    let kind = parse_memory_kind(&request.kind)?;
    let evidence_role = parse_evidence_role(&request.evidence_role)?;
    let confidence = vestrace_domain::Confidence::new(request.confidence)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let importance = vestrace_domain::Importance::new(request.importance)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let cmd = vestrace_application::RememberMemoryCommand {
        memory_id: MemoryId::new(),
        kind,
        content: request.content,
        structured: None,
        confidence,
        importance,
        source_event_id: EventId::from_uuid(request.source_event_id),
        evidence_role,
        policy: vestrace_domain::MemoryWritePolicy::Manual,
        idempotency_key,
    };

    let memory = state
        .memory_use_cases()
        .remember_memory(&context, cmd)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(memory_to_response(memory)),
    ))
}

pub async fn get_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<MemoryResponse>, ApiError> {
    let context = request_context(&headers)?;
    let id = id
        .parse::<MemoryId>()
        .map_err(|_| ApiError::bad_request("memory id must be a UUID"))?;

    let memory = state
        .memory_use_cases()
        .find_memory(&context, id)
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| ApiError::not_found("memory"))?;

    Ok(Json(memory_to_response(memory)))
}

fn memory_to_response(memory: vestrace_domain::Memory) -> MemoryResponse {
    let kind = match memory.kind {
        vestrace_domain::MemoryKind::Fact => "fact",
        vestrace_domain::MemoryKind::Preference => "preference",
        vestrace_domain::MemoryKind::Constraint => "constraint",
        vestrace_domain::MemoryKind::Decision => "decision",
        vestrace_domain::MemoryKind::Task => "task",
        vestrace_domain::MemoryKind::Procedure => "procedure",
        vestrace_domain::MemoryKind::Observation => "observation",
        vestrace_domain::MemoryKind::Outcome => "outcome",
        vestrace_domain::MemoryKind::Summary => "summary",
    };
    let status = match memory.status {
        vestrace_domain::MemoryStatus::Candidate => "candidate",
        vestrace_domain::MemoryStatus::Active => "active",
        vestrace_domain::MemoryStatus::Superseded => "superseded",
        vestrace_domain::MemoryStatus::Rejected => "rejected",
        vestrace_domain::MemoryStatus::Expired => "expired",
        vestrace_domain::MemoryStatus::Deleted => "deleted",
    };
    MemoryResponse {
        id: memory.id.as_uuid(),
        kind: kind.to_owned(),
        status: status.to_owned(),
        created_at: memory.created_at,
        updated_at: memory.updated_at,
    }
}

fn parse_memory_kind(value: &str) -> Result<vestrace_domain::MemoryKind, ApiError> {
    match value {
        "fact" => Ok(vestrace_domain::MemoryKind::Fact),
        "preference" => Ok(vestrace_domain::MemoryKind::Preference),
        "constraint" => Ok(vestrace_domain::MemoryKind::Constraint),
        "decision" => Ok(vestrace_domain::MemoryKind::Decision),
        "task" => Ok(vestrace_domain::MemoryKind::Task),
        "procedure" => Ok(vestrace_domain::MemoryKind::Procedure),
        "observation" => Ok(vestrace_domain::MemoryKind::Observation),
        "outcome" => Ok(vestrace_domain::MemoryKind::Outcome),
        "summary" => Ok(vestrace_domain::MemoryKind::Summary),
        _ => Err(ApiError::bad_request(format!(
            "unknown memory kind: {value}"
        ))),
    }
}

fn parse_evidence_role(value: &str) -> Result<vestrace_domain::EvidenceRole, ApiError> {
    match value {
        "direct_source" => Ok(vestrace_domain::EvidenceRole::DirectSource),
        "supporting_context" => Ok(vestrace_domain::EvidenceRole::SupportingContext),
        "contradicting_evidence" => Ok(vestrace_domain::EvidenceRole::ContradictingEvidence),
        _ => Err(ApiError::bad_request(format!(
            "unknown evidence role: {value}"
        ))),
    }
}

fn required_string_header(headers: &HeaderMap, name: &'static str) -> Result<String, ApiError> {
    let value = headers
        .get(name)
        .ok_or_else(|| ApiError::bad_request(format!("missing {name} header")))?;
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request(format!("{name} header must be valid UTF-8")))?;
    Ok(value.to_owned())
}
