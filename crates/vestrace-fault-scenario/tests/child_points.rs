use vestrace_domain::external_effects::EffectFaultPoint;
use vestrace_fault_scenario::{ChildStage, aborts_at};

/// The abort site is the whole experiment. If it drifts by one stage the
/// observation is about a different fault than the one reported, and nothing
/// downstream can tell.
#[test]
fn each_point_aborts_at_its_own_stage() {
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterIntentPersistence),
        ChildStage::IntentPersisted
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterAuthorizationBeforeDispatch),
        ChildStage::Authorized
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterDispatchBeforeReceipt),
        ChildStage::Dispatched
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation),
        ChildStage::ReceiptPersisted
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterOutcomeBeforeRunCommit),
        ChildStage::OutcomeSettled
    );
}

/// Every point must have a stage. A `_ =>` arm added later would silently send
/// a new point to whatever stage happened to be last.
#[test]
fn every_required_point_has_a_stage() {
    for point in EffectFaultPoint::required_points() {
        let _ = aborts_at(point);
    }
}
