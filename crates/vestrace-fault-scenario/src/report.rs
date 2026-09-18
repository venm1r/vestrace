//! The one line this program writes to stdout.
//!
//! Rendering lives here rather than in `main.rs` so the process contract can be
//! tested without starting a process. `tests/output_contract.rs` builds the
//! exact bytes `main` prints and parses them with the shape
//! `ProcessFaultObservation` in `crates/vestrace-application/src/fault_runtime.rs`
//! has, which is `deny_unknown_fields` over a closed name vocabulary. A field
//! renamed here is a failing unit test rather than a rejected observation after
//! a database, five child processes and several minutes.
//!
//! Nothing in this module decides anything. It is given a `FaultObservation`
//! that was read back out of the world and turns it into JSON; the names are the
//! reader's names, and there is no `_ =>` arm anywhere so a variant added later
//! cannot be quietly rendered as something else.

use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, FaultObservation,
};

/// The observation as the invoking runtime reads it: one JSON object, five
/// fields, no newline.
pub fn render(observation: &FaultObservation) -> String {
    serde_json::json!({
        "point": point_name(observation.point),
        "status": status_name(observation.status),
        "retry_attempted": observation.retry_attempted,
        "reconciliation_started": observation.reconciliation_started,
        "receipt_persisted": observation.receipt_persisted,
    })
    .to_string()
}

/// The wire name of a fault point, as `parse_fault_point` spells it.
pub fn point_name(point: EffectFaultPoint) -> &'static str {
    match point {
        EffectFaultPoint::AfterIntentPersistence => "after_intent_persistence",
        EffectFaultPoint::AfterAuthorizationBeforeDispatch => "after_authorization_before_dispatch",
        EffectFaultPoint::AfterDispatchBeforeReceipt => "after_dispatch_before_receipt",
        EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation => {
            "after_receipt_before_outcome_confirmation"
        }
        EffectFaultPoint::AfterOutcomeBeforeRunCommit => "after_outcome_before_run_commit",
        EffectFaultPoint::AfterReserved
        | EffectFaultPoint::AfterVaultCreateBeforeReceipt
        | EffectFaultPoint::AfterReceiptBeforePrepared
        | EffectFaultPoint::AfterPreparedBeforeBound
        | EffectFaultPoint::AfterBoundBeforePromotion
        | EffectFaultPoint::AfterAbortBeforeWitnessedErase
        | EffectFaultPoint::AfterEraseReceiptBeforeTerminalAppend => {
            panic!("intent fault point cannot name an external-effect report")
        }
    }
}

/// The wire name of a lifecycle status, as `parse_effect_status` spells it.
pub fn status_name(status: EffectLifecycleStatus) -> &'static str {
    match status {
        EffectLifecycleStatus::Prepared => "prepared",
        EffectLifecycleStatus::Authorized => "authorized",
        EffectLifecycleStatus::Dispatching => "dispatching",
        EffectLifecycleStatus::Acknowledged => "acknowledged",
        EffectLifecycleStatus::Failed => "failed",
        EffectLifecycleStatus::Unknown => "unknown",
        EffectLifecycleStatus::Confirmed => "confirmed",
        EffectLifecycleStatus::Reconciling => "reconciling",
    }
}
