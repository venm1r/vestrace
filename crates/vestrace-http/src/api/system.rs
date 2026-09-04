use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method},
    routing::get,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_application::MonitoredFinding;
use vestrace_domain::health::HealthSeverity;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{ApiError, context::request_context};

/// One violated invariant.
///
/// `code` is the invariant's id and `severity` and `remediation` come from its
/// definition in the registry — not from the adapter that measured it, which is
/// where all three used to be decided. `invariant_version` and `fingerprint` are
/// new and are the point of the change: a reader can tell a check whose
/// definition moved from one that did not, and can tell the same violation seen
/// twice from two different ones.
#[derive(Debug, Serialize)]
pub struct FindingResponse {
    pub code: String,
    pub invariant_version: String,
    pub severity: String,
    pub message: String,
    pub remediation: String,
    pub fingerprint: String,
    /// How many times this finding has been observed, across every run.
    ///
    /// It was always absent before findings were stored, because every run
    /// built a fresh one; a count of eleven is the difference between "the
    /// outbox is backed up" and "the outbox has been backed up for eleven runs".
    pub occurrence_count: u64,
    pub first_seen_at: vestrace_domain::Timestamp,
    pub last_seen_at: vestrace_domain::Timestamp,
    /// False when the finding is known but this run did not reproduce it. A
    /// problem that stopped being observed is still open until something
    /// verifies it away.
    pub observed_on_this_run: bool,
    /// `open`, `reopened`, `resolved`, `suppressed` or `accepted_risk`, with a
    /// lapsed silence already accounted for.
    pub status: String,
}

impl From<MonitoredFinding> for FindingResponse {
    fn from(inspected: MonitoredFinding) -> Self {
        Self {
            code: inspected.finding.invariant_id().to_owned(),
            invariant_version: inspected.finding.invariant_version().to_owned(),
            occurrence_count: inspected.finding.occurrence_count(),
            first_seen_at: inspected.finding.first_seen_at(),
            last_seen_at: inspected.finding.last_seen_at(),
            observed_on_this_run: inspected.observed_on_this_run,
            status: match inspected.status {
                vestrace_domain::health::FindingLifecycleStatus::Open => "open",
                vestrace_domain::health::FindingLifecycleStatus::Reopened => "reopened",
                vestrace_domain::health::FindingLifecycleStatus::Resolved => "resolved",
                vestrace_domain::health::FindingLifecycleStatus::Suppressed => "suppressed",
                vestrace_domain::health::FindingLifecycleStatus::AcceptedRisk => "accepted_risk",
            }
            .to_owned(),
            severity: match inspected.severity() {
                HealthSeverity::Critical => "critical",
                HealthSeverity::Error => "error",
                HealthSeverity::Warning => "warning",
                HealthSeverity::Info => "info",
            }
            .to_owned(),
            message: inspected.detail,
            remediation: inspected.definition.remediation().to_owned(),
            fingerprint: inspected.finding.fingerprint().to_owned(),
        }
    }
}

/// What the runtime reports about the environment it is actually running in.
///
/// Every value here is observed at request time rather than stored, so it
/// cannot drift from reality the way a copied configuration value would.
#[derive(Debug, Serialize)]
pub struct SystemHealthResponse {
    /// `false` when any check reported an error.
    pub healthy: bool,
    pub findings: Vec<FindingResponse>,
    pub database_role: String,
    pub database_role_is_superuser: bool,
    /// When true the connecting role can read across workspaces regardless of
    /// row-level security, which a deployment should not accept.
    pub database_role_bypasses_rls: bool,
    pub migration_history_compatible: bool,
}

pub fn system_routes() -> axum::Router<AppState> {
    let router = mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/system/health"),
        get(system_health),
    );
    mount(
        router,
        route_descriptor(&Method::POST, "/v1/system/health/findings/{id}/disposition"),
        axum::routing::post(disposition_finding),
    )
}

async fn system_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SystemHealthResponse>, ApiError> {
    let context = request_context(&headers)?;
    let monitor = state.health_monitor()?;
    let evidence = state
        .runtime_evidence()
        .await
        .map_err(ApiError::from_application)?;

    let findings = monitor
        .inspect(&context, vestrace_domain::time::now())
        .await
        .map_err(ApiError::from_application)?;
    let healthy = !findings.iter().any(MonitoredFinding::is_error);

    Ok(Json(SystemHealthResponse {
        healthy,
        findings: findings.into_iter().map(Into::into).collect(),
        database_role: evidence.runtime_role,
        database_role_is_superuser: evidence.is_superuser,
        database_role_bypasses_rls: evidence.bypasses_rls,
        migration_history_compatible: evidence.migration_history_compatible,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DispositionRequest {
    /// `suppressed` — the finding is understood and should stop shouting; or
    /// `accepted_risk` — the violation is real and is being lived with.
    pub kind: String,
    pub reason: String,
    /// When the decision lapses. Optional, and leaving it out is the choice
    /// this mechanism is least happy about: see the note on the handler.
    #[serde(default)]
    pub expires_at: Option<vestrace_domain::Timestamp>,
    /// The record of the decision outside this system — a ticket, a change
    /// request, a meeting.
    pub audit_ref: String,
}

/// Answer a finding.
///
/// # Why this route did not exist
///
/// `FindingDisposition` has been in the domain since findings were persisted,
/// with `Suppressed` and `AcceptedRisk` variants, an expiry, an actor, a policy
/// version and an audit reference — and nothing could produce one. So an
/// error-severity finding pinned `healthy` to false permanently: fixing the
/// cause left the finding `open` and `observed_on_this_run: false`, which is
/// correct (a finding is a durable record) and left the operator no way to say
/// so.
///
/// The decision now has a way in, and it carries who made it. A silence with no
/// author is the thing this mechanism exists to prevent.
pub async fn disposition_finding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<DispositionRequest>,
) -> Result<Json<FindingResponse>, ApiError> {
    let context = request_context(&headers)?;
    let monitor = state.health_monitor()?;

    if request.reason.trim().is_empty() || request.audit_ref.trim().is_empty() {
        return Err(ApiError::bad_request(
            "a disposition requires a reason and an audit reference",
        ));
    }

    // The disposition records the policy under which it was decided, so it is
    // taken from a decision that was really made rather than from configuration
    // read at render time. Asking here also means the resource-level authority
    // is checked against the finding itself, not only against the route.
    let decision = state
        .authorization_boundary()
        .require(
            &context,
            vestrace_domain::AuthorizationRequest::new(
                vestrace_domain::Capability::WorkspaceAdmin,
                "health.disposition",
                format!("finding://{id}"),
                vestrace_domain::RiskCategory::High,
            ),
        )
        .await
        .map_err(ApiError::from_application)?;
    let policy_version = decision.policy_version.clone();
    let disposition = match request.kind.as_str() {
        "suppressed" => vestrace_domain::health::FindingDisposition::suppressed(
            context.principal_id,
            request.reason,
            request.expires_at,
            policy_version,
            request.audit_ref,
        ),
        "accepted_risk" => vestrace_domain::health::FindingDisposition::accepted_risk(
            context.principal_id,
            request.reason,
            request.expires_at,
            policy_version,
            request.audit_ref,
        ),
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown disposition kind: {other}; expected suppressed or accepted_risk"
            )));
        }
    };

    let finding = monitor
        .disposition(
            &context,
            vestrace_domain::HealthFindingId::from_uuid(id),
            disposition,
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(FindingResponse::from(finding)))
}
