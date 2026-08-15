//! Issuing, listing and revoking capability grants.
//!
//! # Why this surface has to exist
//!
//! `CapabilityGrant` was verified by eleven conformance cases and issued by
//! nothing. Without a way to create one, the only workable authorization engine
//! was the configured static list — so the capability model was code the system
//! carried rather than code it used.
//!
//! Issuing a grant is `workspace.admin`, because handing out authority is the
//! most consequential thing an administrator does.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use vestrace_domain::security::{
    BudgetConstraint, CapabilityGrant, CapabilityGrantSpec, GrantCondition,
};
use vestrace_domain::{Capability, CapabilityGrantId, PrincipalId, RiskCategory, Timestamp};

use crate::AppState;

use super::{ApiError, context::request_context};

pub fn capability_grant_routes() -> Router<AppState> {
    Router::new()
        .route("/capability-grants", post(issue_grant).get(list_grants))
        .route("/capability-grants/{id}/revoke", post(revoke_grant))
}

#[derive(Debug, Deserialize)]
pub struct IssueGrantRequest {
    /// Who the grant is for. A grant issued to nobody in particular is the
    /// configured static list again, so this is required.
    pub subject_id: Uuid,
    /// The capability name, e.g. `memory.write`.
    pub capability: String,
    /// The operation the grant permits, which a delegation may only narrow.
    pub operation: String,
    pub resource_scope: String,
    #[serde(default)]
    pub risk_ceiling: Option<String>,
    #[serde(default)]
    pub budget_max_units: Option<u64>,
    #[serde(default)]
    pub valid_from: Option<Timestamp>,
    #[serde(default)]
    pub valid_until: Option<Timestamp>,
    #[serde(default)]
    pub conditions: Vec<ConditionRequest>,
}

#[derive(Debug, Deserialize)]
pub struct ConditionRequest {
    pub name: String,
    pub expected_value: String,
}

#[derive(Debug, Serialize)]
pub struct GrantResponse {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub issuer_id: Uuid,
    pub capability: String,
    pub operation: String,
    pub resource_scope: String,
    pub status: String,
    pub risk_ceiling: String,
    pub budget_max_units: Option<u64>,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub created_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

impl From<CapabilityGrant> for GrantResponse {
    fn from(grant: CapabilityGrant) -> Self {
        Self {
            id: grant.id.as_uuid(),
            subject_id: grant.subject_id.as_uuid(),
            issuer_id: grant.issuer_id.as_uuid(),
            capability: grant.capability.to_string(),
            operation: grant.operation,
            resource_scope: grant.resource_scope,
            status: format!("{:?}", grant.status).to_lowercase(),
            risk_ceiling: format!("{:?}", grant.risk_ceiling).to_lowercase(),
            budget_max_units: grant.budget.map(|budget| budget.max_units),
            valid_from: grant.valid_from,
            valid_until: grant.valid_until,
            created_at: grant.created_at,
            revoked_at: grant.revoked_at,
        }
    }
}

fn parse_risk(value: Option<&str>) -> Result<RiskCategory, ApiError> {
    match value.unwrap_or("low") {
        "low" => Ok(RiskCategory::Low),
        "medium" => Ok(RiskCategory::Medium),
        "high" => Ok(RiskCategory::High),
        "critical" => Ok(RiskCategory::Critical),
        other => Err(ApiError::bad_request(format!(
            "unknown risk ceiling: {other}"
        ))),
    }
}

async fn issue_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IssueGrantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.capability_grants()?;

    let capability = Capability::from_str(&request.capability)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let risk_ceiling = parse_risk(request.risk_ceiling.as_deref())?;
    let now = vestrace_domain::time::now();

    let conditions = request
        .conditions
        .into_iter()
        .map(|condition| GrantCondition::new(condition.name, condition.expected_value))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;

    let spec = CapabilityGrantSpec {
        id: CapabilityGrantId::new(),
        workspace_id: context.workspace_id,
        subject_id: PrincipalId::from_uuid(request.subject_id),
        // The issuer is the authenticated principal. Taking it from the body
        // would let a caller attribute a grant to somebody else, which is the
        // one field in an authority record that must not be assertable.
        issuer_id: context.principal_id,
        capability,
        operation: request.operation,
        resource_scope: request.resource_scope,
        valid_from: request.valid_from.unwrap_or(now),
        valid_until: request.valid_until,
        budget: request.budget_max_units.map(BudgetConstraint::new),
        risk_ceiling,
        conditions,
    };

    let grant = CapabilityGrant::issue(spec, now).map_err(ApiError::from_domain)?;
    repository
        .insert(&context, &grant)
        .await
        .map_err(ApiError::from_application)?;

    Ok((StatusCode::CREATED, Json(GrantResponse::from(grant))))
}

async fn list_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let grants = state
        .capability_grants()?
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(
        grants
            .into_iter()
            .map(GrantResponse::from)
            .collect::<Vec<_>>(),
    ))
}

async fn revoke_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let context = request_context(&headers)?;
    let repository = state.capability_grants()?;
    let id = CapabilityGrantId::from_uuid(id);

    let mut grant = repository
        .find(&context, id)
        .await
        .map_err(ApiError::from_application)?
        .ok_or_else(|| ApiError::not_found("capability grant"))?;

    // The domain decides whether this grant can be revoked and stamps when;
    // the repository writes what it decided. Revoking an already-revoked grant
    // is a conflict rather than a no-op, so a second call cannot move the
    // revocation time.
    grant
        .revoke(vestrace_domain::time::now())
        .map_err(ApiError::from_domain)?;
    repository
        .save_revocation(&context, &grant)
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(GrantResponse::from(grant)))
}
