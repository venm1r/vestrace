use axum::{Json, extract::State, http::HeaderMap, routing::post};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, ExternalEffectIntent,
    IdempotencyProfile,
};
use vestrace_domain::{Capability, RiskCategory};

use crate::AppState;

use super::{ApiError, context::request_context};

pub fn effect_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/effects", post(perform_effect))
}

#[derive(Debug, Deserialize)]
pub struct PerformEffectRequest {
    /// Which configured adapter performs this. The caller names an adapter, not
    /// a destination: where it goes is a deployment decision, and a surface that
    /// let a request choose would be an outbound proxy for whatever the process
    /// can reach.
    pub adapter: String,
    pub operation: String,
    pub target: String,
    /// What the caller says this will do, in its own words. Recorded on the
    /// intent and later compared against what actually happened.
    pub expected_effect: String,
    /// The conditions the caller believes hold. Checked immediately before
    /// dispatch, so an intent written against a world that has since moved is
    /// refused rather than acted on.
    pub preconditions: Vec<PreconditionRequest>,
    #[serde(default)]
    pub arguments: serde_json::Value,
    /// Which execution this belongs to: `run://<id>`, `execution://<id>`, or
    /// `workspace://` for an operator acting directly.
    ///
    /// This said "a run, a workflow step, an operator action — an effect with no
    /// execution behind it is untraceable", and was a free string checked only
    /// for being non-blank. `run-step-1` satisfied it and traced to nothing, so
    /// the claim was false for every effect this route had ever created: when a
    /// reconciliation finally settled what happened, there was no way to find
    /// what had asked for it. It is a reference now.
    pub execution_ref: String,
}

#[derive(Debug, Deserialize)]
pub struct PreconditionRequest {
    pub name: String,
    pub expected_value: String,
}

#[derive(Debug, Serialize)]
pub struct EffectReceiptResponse {
    pub effect_id: Uuid,
    pub receipt_id: Uuid,
    pub adapter: String,
    /// `acknowledged`, `failed` or `unknown`. Never "succeeded": an
    /// acknowledgement is the far side saying it received the request, not that
    /// the thing the request asked for happened.
    pub outcome: String,
    pub response_class: String,
    /// True when nobody knows whether the effect took place. The only honest
    /// next step is reconciliation, and retrying is refused.
    pub requires_reconciliation: bool,
    pub evidence_refs: Vec<String>,
}

/// Perform an external effect.
///
/// # Why this route did not exist
///
/// Eighteen EXT requirements execute against a domain model that no adapter
/// implemented and no surface exposed: **nothing in this build had ever
/// performed an external effect**. Every delta that touched the family said so
/// and moved on.
///
/// The effect is irreversible by declaration, requires `execution.write`, and is
/// requested at high risk. What comes back is a receipt, never a verdict.
pub async fn perform_effect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PerformEffectRequest>,
) -> Result<Json<EffectReceiptResponse>, ApiError> {
    let context = request_context(&headers)?;
    let effects = state.external_effects()?;
    let adapter = state.effect_adapter(&request.adapter)?;

    if request.preconditions.is_empty() {
        return Err(ApiError::bad_request(
            "an external effect requires at least one precondition: dispatch checks them \
             immediately before acting, and an effect that asserts nothing about the world \
             cannot be refused when the world moves",
        ));
    }

    let preconditions = request
        .preconditions
        .into_iter()
        .map(|precondition| EffectPrecondition::new(precondition.name, precondition.expected_value))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::from_domain)?;

    let arguments_digest = digest(&request.arguments);
    let precondition_digest = digest(&serde_json::json!(
        preconditions
            .iter()
            .map(|precondition| (precondition.name(), precondition.expected_value()))
            .collect::<Vec<_>>()
    ));

    let descriptor = adapter.descriptor();
    let intent = ExternalEffectIntent::new(
        request.execution_ref,
        context.workspace_id,
        context.principal_id,
        descriptor.name(),
        request.operation,
        request.target,
        arguments_digest,
        request.expected_effect,
        preconditions,
        precondition_digest,
        RiskCategory::High,
        descriptor.reversibility(),
        descriptor.idempotency_profile(),
        descriptor.delivery_semantics(),
        descriptor.required_capability(),
        None::<String>,
        None::<String>,
        vestrace_domain::now(),
    )
    .map_err(ApiError::from_domain)?;

    let receipt = effects
        .perform(&context, intent, adapter.as_ref(), vestrace_domain::now())
        .await
        .map_err(ApiError::from_application)?;

    let outcome = match receipt.outcome_status() {
        vestrace_domain::external_effects::EffectLifecycleStatus::Acknowledged => "acknowledged",
        vestrace_domain::external_effects::EffectLifecycleStatus::Failed => "failed",
        _ => "unknown",
    };

    Ok(Json(EffectReceiptResponse {
        effect_id: receipt.effect_id().as_uuid(),
        receipt_id: receipt.id().as_uuid(),
        adapter: receipt.adapter().to_owned(),
        outcome: outcome.to_owned(),
        response_class: receipt.response_class().to_owned(),
        requires_reconciliation: receipt.requires_reconciliation(),
        evidence_refs: receipt.evidence_refs().to_vec(),
    }))
}

/// A stable digest of a JSON value.
///
/// The intent stores digests rather than the arguments themselves: an effect
/// record that carried the request body verbatim would be one more place a
/// credential can be written and then read by everyone who can read a trace.
fn digest(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = value.to_string();
    format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()))
}

// Unused declarations kept off the type: `DeliverySemantics`,
// `EffectReversibility` and `IdempotencyProfile` come from the adapter's own
// descriptor, never from the request, so a caller cannot claim a guarantee the
// adapter does not provide.
const _: fn() = || {
    let _ = DeliverySemantics::AtLeastOnce;
    let _ = EffectReversibility::Irreversible;
    let _ = IdempotencyProfile::Unknown;
    let _ = Capability::ExecutionWrite;
};
