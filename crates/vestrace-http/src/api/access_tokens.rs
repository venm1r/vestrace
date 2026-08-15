//! Credential administration.
//!
//! # The plaintext credential is returned exactly once
//!
//! `POST /v1/access-tokens` is the only response in the system that carries a
//! token, and only the response to the request that created it. Nothing reads
//! one back: the database holds a SHA-256 hash, and a hash cannot be presented
//! as a credential. An operator who has lost a token revokes it and mints
//! another — which is also the behaviour that makes revocation meaningful,
//! since a readable credential store turns every authorization mistake into a
//! disclosure of every credential.
//!
//! # Revocation does not delete
//!
//! A revoked credential keeps its row. Deleting it would destroy the record
//! that it ever existed — when it was issued, under what label, and when it was
//! last used — which is exactly what an investigation into its misuse needs.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::{Deserialize, Serialize};
use vestrace_domain::identity::AccessToken;
use vestrace_domain::{AccessTokenId, PrincipalId, time::now};

use crate::AppState;

use super::{ApiError, context::request_context};

const MAX_LABEL_LENGTH: usize = 128;

/// The longest life a credential may be given, in days.
///
/// A bound exists because "no expiry" should be a deliberate choice rather than
/// the accidental result of a large number in a request body.
const MAX_LIFETIME_DAYS: i64 = 365;

#[derive(Debug, Serialize)]
pub struct AccessTokenResponse {
    pub id: uuid::Uuid,
    pub principal_id: uuid::Uuid,
    pub label: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    /// Whether this credential would authenticate a request right now.
    pub live: bool,
}

impl From<AccessToken> for AccessTokenResponse {
    fn from(token: AccessToken) -> Self {
        let live = token.is_live_at(now());
        Self {
            id: token.id.as_uuid(),
            principal_id: token.principal_id.as_uuid(),
            label: token.label,
            created_at: token.created_at.to_rfc3339(),
            expires_at: token.expires_at.map(|at| at.to_rfc3339()),
            revoked_at: token.revoked_at.map(|at| at.to_rfc3339()),
            last_used_at: token.last_used_at.map(|at| at.to_rfc3339()),
            live,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AccessTokenListResponse {
    pub access_tokens: Vec<AccessTokenResponse>,
    /// States the contract rather than leaving a client to discover it.
    pub values_are_returned_once_at_creation: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccessTokenRequest {
    /// Who the credential authenticates as. Defaults to the caller.
    #[serde(default)]
    pub principal_id: Option<uuid::Uuid>,
    pub label: String,
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Serialize)]
pub struct CreateAccessTokenResponse {
    pub id: uuid::Uuid,
    pub principal_id: uuid::Uuid,
    pub label: String,
    pub expires_at: Option<String>,
    /// The credential. Shown here and nowhere else, ever.
    pub token: String,
    pub store_it_now: &'static str,
}

impl std::fmt::Debug for CreateAccessTokenResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Axum's rejection paths and any tracing of a response body would
        // otherwise print the credential.
        formatter
            .debug_struct("CreateAccessTokenResponse")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

pub fn access_token_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/access-tokens",
            get(list_access_tokens).post(create_access_token),
        )
        .route(
            "/access-tokens/{id}",
            axum::routing::delete(revoke_access_token),
        )
        // Present so a caller expecting a read endpoint gets an explicit
        // refusal rather than a confusing 405.
        .route("/access-tokens/{id}/value", get(value_is_never_returned))
}

async fn list_access_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccessTokenListResponse>, ApiError> {
    let context = request_context(&headers)?;
    let store = state.access_token_store()?;
    let tokens = store
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(AccessTokenListResponse {
        access_tokens: tokens.into_iter().map(AccessTokenResponse::from).collect(),
        values_are_returned_once_at_creation: true,
    }))
}

async fn create_access_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAccessTokenRequest>,
) -> Result<(StatusCode, Json<CreateAccessTokenResponse>), ApiError> {
    let context = request_context(&headers)?;

    let label = request.label.trim().to_string();
    if label.is_empty() {
        return Err(ApiError::bad_request("label must not be blank"));
    }
    if label.len() > MAX_LABEL_LENGTH {
        return Err(ApiError::bad_request(format!(
            "label must be at most {MAX_LABEL_LENGTH} characters"
        )));
    }

    let at = now();
    let expires_at = match request.expires_in_days {
        None => None,
        Some(days) if days < 1 || days > MAX_LIFETIME_DAYS => {
            return Err(ApiError::bad_request(format!(
                "expires_in_days must be between 1 and {MAX_LIFETIME_DAYS}"
            )));
        }
        Some(days) => Some(at + chrono::Duration::days(days)),
    };

    // Defaults to the caller rather than requiring one to be named: minting a
    // credential for oneself is the common case, and a body that must always
    // carry a principal id invites copying somebody else's.
    let principal_id = request
        .principal_id
        .map(PrincipalId::from_uuid)
        .unwrap_or(context.principal_id);

    let entropy_source = state.token_entropy_source()?;
    let mut entropy = [0u8; 32];
    entropy_source
        .fill(&mut entropy)
        .map_err(ApiError::from_application)?;

    let issued = AccessToken::issue(
        AccessTokenId::new(),
        context.workspace_id,
        principal_id,
        label.clone(),
        &entropy,
        expires_at,
        at,
    )
    .map_err(|error| ApiError::from_application(error.into()))?;

    let record = issued.record().clone();
    let store = state.access_token_store()?;
    store
        .put(&context, &record)
        .await
        .map_err(ApiError::from_application)?;

    // Recorded before the caller is told it succeeded. The event names the
    // credential and never its value.
    record_audit(
        &state,
        &context,
        "access_token.created",
        record.id.as_uuid(),
        serde_json::json!({
            "label": label,
            "principal_id": principal_id.as_uuid(),
            "expires_at": record.expires_at.map(|at| at.to_rfc3339()),
        }),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateAccessTokenResponse {
            id: record.id.as_uuid(),
            principal_id: principal_id.as_uuid(),
            label,
            expires_at: record.expires_at.map(|at| at.to_rfc3339()),
            token: issued.into_token(),
            store_it_now: "this value is not stored and cannot be retrieved again",
        }),
    ))
}

async fn revoke_access_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let context = request_context(&headers)?;
    let store = state.access_token_store()?;
    store
        .revoke(&context, AccessTokenId::from_uuid(id), now())
        .await
        .map_err(ApiError::from_application)?;

    record_audit(
        &state,
        &context,
        "access_token.revoked",
        id,
        serde_json::json!({}),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Answers the one question this surface will not answer.
async fn value_is_never_returned() -> ApiError {
    ApiError::refused(
        "access_token_value_not_readable",
        "an access token is shown once when it is created; revoke it and mint another",
    )
}

async fn record_audit(
    state: &AppState,
    context: &vestrace_application::RequestContext,
    action: &str,
    subject: uuid::Uuid,
    detail: serde_json::Value,
) -> Result<(), ApiError> {
    let audit = state.audit_repository()?;
    let event = vestrace_domain::AuditEvent::new(
        vestrace_domain::id::AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        action,
        "access_token",
        subject,
        detail,
        now(),
    )
    .map_err(|error| ApiError::from_application(error.into()))?;
    audit
        .record(context, &event)
        .await
        .map_err(ApiError::from_application)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::WorkspaceId;

    #[test]
    fn the_creation_response_never_renders_its_token() {
        let response = CreateAccessTokenResponse {
            id: uuid::Uuid::nil(),
            principal_id: uuid::Uuid::nil(),
            label: "console".into(),
            expires_at: None,
            token: "vst_super_secret".into(),
            store_it_now: "",
        };
        let rendered = format!("{response:?}");
        assert!(!rendered.contains("super_secret"));
        assert!(rendered.contains("[REDACTED]"));
        // The label is not a secret and stays visible, because it is what makes
        // the log line useful.
        assert!(rendered.contains("console"));
    }

    #[test]
    fn a_listing_entry_carries_no_secret_material() {
        let token = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "console",
            &[1u8; 32],
            None,
            now(),
        )
        .unwrap();
        let hash = token.record().token_hash.clone();
        let response = AccessTokenResponse::from(token.record().clone());
        let rendered = serde_json::to_string(&response).unwrap();

        // Not even the hash: it is not usable as a credential, but publishing
        // it hands an attacker an offline target for free.
        assert!(!rendered.contains(&hash));
        assert!(rendered.contains("console"));
        assert!(response.live);
    }

    #[test]
    fn a_revoked_credential_reports_itself_as_not_live() {
        let token = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "retired",
            &[2u8; 32],
            None,
            now(),
        )
        .unwrap();
        let revoked = token.record().clone().revoke(now()).unwrap();
        let response = AccessTokenResponse::from(revoked);
        assert!(!response.live);
        assert!(response.revoked_at.is_some());
    }
}
