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
const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const IF_MATCH_HEADER: &str = "if-match";

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
    pub occurred_at: Option<vestrace_domain::Timestamp>,
    pub recorded_at: vestrace_domain::Timestamp,
}

pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEventRequest>,
) -> Result<(axum::http::StatusCode, Json<EventResponse>), ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = idempotency_key(&headers)?;

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
            occurred_at: event.occurred_at,
            recorded_at: event.recorded_at,
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
    #[serde(default)]
    pub classification: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoryResponse {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub classification: Option<String>,
    pub created_at: vestrace_domain::Timestamp,
    pub updated_at: vestrace_domain::Timestamp,
}

pub async fn create_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMemoryRequest>,
) -> Result<(axum::http::StatusCode, Json<MemoryResponse>), ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = idempotency_key(&headers)?;

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
        classification: request.classification,
        idempotency_key,
    };

    let memory = state
        .memory_use_cases()
        .remember_memory(&context, cmd)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(memory_response(&state, &context, memory).await?),
    ))
}

/// A new revision of an existing memory.
///
/// # Why this route exists
///
/// `MemoryService::revise_memory` has existed since the memory slice and
/// nothing exposed it, so the only way a memory could change was not to. That
/// also meant the transactional fix applied to it — memory, revision, source
/// and search document written in one commit — could be reasoned about but not
/// exercised: the identical defect on the creation path was found by calling
/// it, and this one was carried on the assumption that the same fix works.
#[derive(Debug, Deserialize)]
pub struct ReviseMemoryRequest {
    pub content: String,
    pub confidence: f32,
    pub importance: f32,
    pub source_event_id: Uuid,
    /// Free text recorded on the revision. Absent means "content update",
    /// which is what the service already writes.
    #[serde(default)]
    pub change_reason: Option<String>,
    #[serde(default)]
    classification: MemoryClassificationField,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum MemoryClassificationField {
    #[default]
    Omitted,
    Clear,
    Set(String),
}

impl<'de> Deserialize<'de> for MemoryClassificationField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match Option::<String>::deserialize(deserializer)? {
            Some(label) => Self::Set(label),
            None => Self::Clear,
        })
    }
}

pub async fn revise_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ReviseMemoryRequest>,
) -> Result<(axum::http::StatusCode, Json<MemoryResponse>), ApiError> {
    let context = request_context(&headers)?;
    let idempotency_key = idempotency_key(&headers)?;

    // The expected revision travels in `If-Match`, as the run version does.
    // Making it a body field would let a client omit it and get last-write-wins
    // silently, which for a memory means losing an edit with no error.
    let expected_revision = if_match_revision(&headers)?;

    let id = id
        .parse::<MemoryId>()
        .map_err(|_| ApiError::bad_request("memory id must be a UUID"))?;
    let confidence = vestrace_domain::Confidence::new(request.confidence)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let importance = vestrace_domain::Importance::new(request.importance)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let cmd = vestrace_application::ReviseMemoryCommand {
        memory_id: id,
        expected_revision,
        content: request.content,
        structured: None,
        confidence,
        importance,
        source_event_id: EventId::from_uuid(request.source_event_id),
        change_reason: request.change_reason,
        classification: match request.classification {
            MemoryClassificationField::Omitted => {
                vestrace_application::MemoryClassificationUpdate::Inherit
            }
            MemoryClassificationField::Clear => {
                vestrace_application::MemoryClassificationUpdate::Clear
            }
            MemoryClassificationField::Set(label) => {
                vestrace_application::MemoryClassificationUpdate::Set(label)
            }
        },
        idempotency_key,
    };

    let memory = state
        .memory_use_cases()
        .revise_memory(&context, cmd)
        .await
        .map_err(ApiError::from_application)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(memory_response(&state, &context, memory).await?),
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

    Ok(Json(memory_response(&state, &context, memory).await?))
}

#[derive(Debug, Deserialize)]
pub struct PurgeMemoryRequest {
    /// Why the memory is being destroyed. Recorded in `purge_audits`, so a
    /// purge cannot be performed without saying what it was for.
    pub reason: String,
    /// The out-of-band approval this purge is carried out under — a ticket, a
    /// signed request, whatever the deployment's process produces. It is stored
    /// beside the counts of what was removed.
    pub approval_id: String,
}

#[derive(Debug, Serialize)]
pub struct PurgeMemoryResponse {
    pub memory_id: Uuid,
    /// What was actually removed, per table. A purge that removed nothing is a
    /// 404 rather than a success with zeros.
    pub removed: std::collections::BTreeMap<String, u64>,
}

/// Destroy a memory and everything that points at it.
///
/// # Why this route did not exist
///
/// `HardPurgeMemoryService` and `PgPurgeRepository` were both written and
/// neither was ever constructed: there was no way to delete a memory from this
/// system at all. The adapter, when it was finally read, ran unscoped against
/// tables with forced row-level security — so had it been wired, it would have
/// deleted nothing and reported success.
///
/// It is the one irreversible operation here, and it is deliberately not a
/// convenience:
///
/// - it takes `memory.purge`, which the local development configuration
///   pointedly does not grant;
/// - it asks the policy engine at **critical** risk, so a grant with a lower
///   ceiling refuses it;
/// - it requires a reason and an approval reference, both recorded;
/// - it answers 404 when nothing was removed, rather than a cheerful 200.
pub async fn purge_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<PurgeMemoryRequest>,
) -> Result<Json<PurgeMemoryResponse>, ApiError> {
    let context = request_context(&headers)?;
    let purge = state.purge_use_case()?;

    let outcome = purge
        .purge(
            &context,
            vestrace_application::HardPurgeMemoryCommand {
                memory_id: MemoryId::from_uuid(id),
                reason: request.reason,
                approval_id: request.approval_id,
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(PurgeMemoryResponse {
        memory_id: id,
        removed: outcome.removed,
    }))
}

async fn memory_response(
    state: &AppState,
    context: &vestrace_application::RequestContext,
    memory: vestrace_domain::Memory,
) -> Result<MemoryResponse, ApiError> {
    let Some(active_revision_id) = memory.active_revision_id else {
        return memory_to_response(memory, None);
    };
    let revision = state
        .memory_use_cases()
        .find_revision(context, active_revision_id)
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| {
            ApiError::from_application(vestrace_application::ApplicationError::Internal(format!(
                "memories.active_revision_id {active_revision_id} names no memory_revisions.id"
            )))
        })?;
    memory_to_response(memory, Some(&revision))
}

fn memory_to_response(
    memory: vestrace_domain::Memory,
    revision: Option<&vestrace_domain::MemoryRevision>,
) -> Result<MemoryResponse, ApiError> {
    match (memory.active_revision_id, revision) {
        (None, None) => {}
        (Some(active_revision_id), Some(revision))
            if active_revision_id == revision.id
                && memory.id == revision.memory_id
                && memory.workspace_id == revision.workspace_id => {}
        (active_revision_id, revision) => {
            return Err(ApiError::from_application(
                vestrace_application::ApplicationError::Internal(format!(
                    "memories.active_revision_id {active_revision_id:?} for memory {} does not \
                     match memory_revisions.id {:?}",
                    memory.id,
                    revision.map(|revision| revision.id)
                )),
            ));
        }
    }
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
    Ok(MemoryResponse {
        id: memory.id.as_uuid(),
        kind: kind.to_owned(),
        status: status.to_owned(),
        classification: revision.and_then(|revision| revision.classification.clone()),
        created_at: memory.created_at,
        updated_at: memory.updated_at,
    })
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

/// The revision a caller believes is active, from `If-Match`.
///
/// Revision numbers are `u32` on the command, so a value that does not fit is
/// rejected rather than truncated into a number that might match.
fn if_match_revision(headers: &HeaderMap) -> Result<u32, ApiError> {
    let value = headers
        .get(IF_MATCH_HEADER)
        .ok_or_else(|| ApiError::bad_request("missing If-Match header"))?;
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match header must be valid UTF-8"))?;
    value
        .parse::<u32>()
        .map_err(|_| ApiError::bad_request("If-Match header must be a revision number"))
}

/// The key this write is deduplicated on.
///
/// # Why this is not simply `x-request-id`
///
/// It was, and the two cannot be the same header. `x-request-id` is normalised
/// to a UUIDv7 by `add_request_context` — deliberately, so every request has a
/// well-formed tracing identity — which meant a caller supplying its own
/// idempotency key had it **silently replaced with a new value on every
/// attempt**. The retry carried a different key, the write happened again, and
/// nothing reported anything: the request succeeded, twice.
///
/// `idempotency-key` is the caller's, kept as sent. `x-request-id` remains the
/// fallback so existing clients that send a UUIDv7 keep the behaviour they
/// have, and it is still normalised, which is safe precisely because it is a
/// UUID by then.
fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    if let Some(value) = headers.get(IDEMPOTENCY_KEY_HEADER) {
        let value = value
            .to_str()
            .map_err(|_| {
                ApiError::bad_request(format!(
                    "{IDEMPOTENCY_KEY_HEADER} header must be valid UTF-8"
                ))
            })?
            .trim();
        if value.is_empty() || value.len() > 200 {
            return Err(ApiError::bad_request(format!(
                "{IDEMPOTENCY_KEY_HEADER} header must be between 1 and 200 characters"
            )));
        }
        return Ok(value.to_owned());
    }
    required_string_header(headers, REQUEST_ID_HEADER)
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

#[cfg(test)]
mod classification_http_tests {
    use super::{MemoryClassificationField, ReviseMemoryRequest, memory_to_response};
    use vestrace_domain::{
        Confidence, Importance, Memory, MemoryKind, MemoryRevision,
        id::{MemoryId, MemoryRevisionId, WorkspaceId},
        now,
    };

    #[test]
    fn revision_json_distinguishes_omitted_null_and_a_label() {
        let body = |classification: Option<&str>| {
            let mut value = serde_json::json!({
                "content": "correction",
                "confidence": 0.9,
                "importance": 0.7,
                "source_event_id": "10000000-0000-0000-0000-000000000001"
            });
            if let Some(classification) = classification {
                value["classification"] = serde_json::from_str(classification).unwrap();
            }
            serde_json::from_value::<ReviseMemoryRequest>(value).unwrap()
        };

        assert_eq!(
            body(None).classification,
            MemoryClassificationField::Omitted
        );
        assert_eq!(
            body(Some("null")).classification,
            MemoryClassificationField::Clear
        );
        assert_eq!(
            body(Some("\"internal\"")).classification,
            MemoryClassificationField::Set("internal".to_owned())
        );
    }

    #[test]
    fn a_memory_response_exposes_the_exact_revision_classification() {
        let at = now();
        let memory_id = MemoryId::new();
        let workspace_id = WorkspaceId::new();
        let revision = MemoryRevision {
            id: MemoryRevisionId::new(),
            memory_id,
            workspace_id,
            revision_number: 1,
            content: "labelled".to_owned(),
            structured: None,
            confidence: Confidence::new(0.9).unwrap(),
            importance: Importance::new(0.7).unwrap(),
            created_at: at,
            valid_from: None,
            valid_until: None,
            change_reason: None,
            canonical_hash: None,
            classification: Some("internal".to_owned()),
        };
        let memory = Memory::new(memory_id, workspace_id, MemoryKind::Fact, at)
            .activate(&revision, at)
            .unwrap();

        let response = memory_to_response(memory, Some(&revision)).unwrap();

        assert_eq!(response.classification.as_deref(), Some("internal"));
    }

    #[test]
    fn a_memory_without_an_active_revision_returns_a_null_classification() {
        let at = now();
        let memory = Memory::new(MemoryId::new(), WorkspaceId::new(), MemoryKind::Fact, at);

        let response = memory_to_response(memory, None).unwrap();

        assert_eq!(response.classification, None);
    }
}
