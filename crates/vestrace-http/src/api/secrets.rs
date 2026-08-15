//! Workspace secret administration.
//!
//! # There is deliberately no endpoint that returns a secret's value
//!
//! Secrets go in and are used internally; they never come back out over HTTP.
//! A read endpoint would turn every authorization mistake, every log of a
//! response body and every browser cache into a credential disclosure, and it
//! would serve no purpose an operator actually has — an operator who has lost a
//! key replaces it, they do not read it back out of the system that stored it.
//!
//! What a caller can do: list what exists, write a value, and delete one.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use vestrace_application::SecretMaterial;

use crate::AppState;

use super::{ApiError, context::request_context};

/// The longest secret this surface accepts.
///
/// A bound exists so a caller cannot use the store as arbitrary storage; 8 KiB
/// comfortably holds an API key or a PEM-encoded private key.
const MAX_SECRET_BYTES: usize = 8 * 1024;
const MAX_NAME_LENGTH: usize = 128;

#[derive(Debug, Serialize)]
pub struct SecretDescriptorResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub purpose: String,
    /// Which key generation the material is currently wrapped under, so an
    /// operator can see what a rotation has not yet covered.
    pub key_version: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SecretListResponse {
    pub secrets: Vec<SecretDescriptorResponse>,
    /// States the contract rather than leaving a client to discover it: this
    /// listing never carries values, and no endpoint returns them.
    pub values_are_never_returned: bool,
}

#[derive(Deserialize)]
pub struct PutSecretRequest {
    pub name: String,
    pub purpose: String,
    pub value: String,
}

impl std::fmt::Debug for PutSecretRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Axum's rejection paths and any tracing of extracted bodies would
        // otherwise print the value.
        formatter
            .debug_struct("PutSecretRequest")
            .field("name", &self.name)
            .field("purpose", &self.purpose)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct PutSecretResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub purpose: String,
    /// The opaque reference. It names where the secret lives and contains no
    /// part of its value.
    pub reference: String,
}

pub fn secret_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/secrets", get(list_secrets).post(put_secret))
        .route("/secrets/{id}", axum::routing::delete(delete_secret))
        // Present so a caller that expects a read endpoint gets an explicit
        // refusal rather than a confusing 405.
        .route("/secrets/{id}/value", post(value_is_never_returned))
        .route("/secrets/{id}/value", get(value_is_never_returned))
}

async fn list_secrets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SecretListResponse>, ApiError> {
    let context = request_context(&headers)?;
    let store = state.secret_store()?;
    let secrets = store
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(SecretListResponse {
        secrets: secrets
            .into_iter()
            .map(|descriptor| SecretDescriptorResponse {
                id: descriptor.id.as_uuid(),
                name: descriptor.name,
                purpose: descriptor.purpose,
                key_version: descriptor.key_version,
                created_at: descriptor.created_at.to_rfc3339(),
                updated_at: descriptor.updated_at.to_rfc3339(),
            })
            .collect(),
        values_are_never_returned: true,
    }))
}

async fn put_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PutSecretRequest>,
) -> Result<(StatusCode, Json<PutSecretResponse>), ApiError> {
    let context = request_context(&headers)?;
    let name = validated_label("name", &request.name)?;
    let purpose = validated_label("purpose", &request.purpose)?;

    if request.value.is_empty() {
        return Err(ApiError::bad_request("value must not be empty"));
    }
    if request.value.len() > MAX_SECRET_BYTES {
        return Err(ApiError::bad_request(format!(
            "value must be at most {MAX_SECRET_BYTES} bytes"
        )));
    }

    let store = state.secret_store()?;
    let reference = store
        .put(
            &context,
            &name,
            &purpose,
            SecretMaterial::new(request.value.into_bytes()),
        )
        .await
        .map_err(ApiError::from_application)?;

    // Recorded before the caller is told it succeeded. The event names the
    // secret and never its value — an audit trail that logs credentials is a
    // credential store with worse access control.
    record_audit(
        &state,
        &context,
        "workspace_secret.stored",
        reference.id().as_uuid(),
        serde_json::json!({ "name": name, "purpose": purpose }),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(PutSecretResponse {
            id: reference.id().as_uuid(),
            name,
            purpose,
            reference: format!("secret://{}", reference.id().as_uuid()),
        }),
    ))
}

async fn delete_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let context = request_context(&headers)?;
    let store = state.secret_store()?;
    store
        .delete(&context, vestrace_domain::SecretRefId::from_uuid(id))
        .await
        .map_err(ApiError::from_application)?;

    record_audit(
        &state,
        &context,
        "workspace_secret.deleted",
        id,
        serde_json::json!({}),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Answers the one question this surface will not answer.
async fn value_is_never_returned() -> ApiError {
    ApiError::refused(
        "secret_value_not_readable",
        "secret values are never returned; replace the secret instead of reading it",
    )
}

fn validated_label(field: &str, value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(format!("{field} must not be blank")));
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(ApiError::bad_request(format!(
            "{field} must be at most {MAX_NAME_LENGTH} characters"
        )));
    }
    Ok(trimmed.to_string())
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
        "workspace_secret",
        subject,
        detail,
        vestrace_domain::time::now(),
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

    #[test]
    fn the_request_body_never_renders_its_value() {
        let request = PutSecretRequest {
            name: "openai".into(),
            purpose: "provider-api-key".into(),
            value: "sk-super-secret".into(),
        };
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("sk-super-secret"));
        assert!(rendered.contains("[REDACTED]"));
        // The name is not a secret and stays visible, because it is what makes
        // the log line useful.
        assert!(rendered.contains("openai"));
    }

    #[test]
    fn blank_and_oversized_labels_are_refused() {
        assert!(validated_label("name", "   ").is_err());
        assert!(validated_label("name", &"a".repeat(MAX_NAME_LENGTH + 1)).is_err());
        assert_eq!(validated_label("name", "  openai  ").unwrap(), "openai");
    }
}
