use vestrace_domain::external_effects::EffectFaultPoint;
use vestrace_fault_scenario::child::{EFFECT_ID_MARKER, SETUP_FAILURE_MARKER};
use vestrace_fault_scenario::{ChildStage, aborts_at, completion_marker, confirm_reached_point};

fn url_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "vestrace-fault-scenario-vocabulary-{}-{}.txt",
        std::process::id(),
        uuid::Uuid::now_v7()
    ));
    std::fs::write(&path, "postgres://localhost/ephemeral").unwrap();
    path
}

fn invoke_with(point: &str, scenario: Option<&str>) -> std::process::Output {
    let url_file = url_file();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_vestrace-fault-scenario"));
    command
        .arg("--database-url-file")
        .arg(&url_file)
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", point);
    if let Some(scenario) = scenario {
        command.arg("--scenario").arg(scenario);
    }
    let output = command.output().unwrap();
    std::fs::remove_file(url_file).ok();
    output
}

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
    let points = EffectFaultPoint::required_points();
    assert_eq!(points.len(), 5, "the effect suite owns exactly five points");
    for point in points {
        let _ = aborts_at(point);
    }
}

#[test]
fn intent_points_are_refused_by_the_external_effect_stage_mapping() {
    for point in EffectFaultPoint::intent_points() {
        assert!(
            std::panic::catch_unwind(|| aborts_at(point)).is_err(),
            "{point:?} reached the external-effect stage mapping"
        );
    }
}

#[test]
fn mismatched_fault_vocabularies_are_refused_by_the_harness_with_exit_two() {
    let effect_for_intent = invoke_with(
        "after_dispatch_before_receipt",
        Some("material_intent_crash"),
    );
    assert_eq!(effect_for_intent.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&effect_for_intent.stderr).contains("unknown fault point"));

    let intent_for_effect = invoke_with("after_reserved", None);
    assert_eq!(intent_for_effect.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&intent_for_effect.stderr).contains("unknown fault point"));
}

/// A child's stderr as it looks when the child reached its point and aborted
/// there: the announced id, then the completion line `abort_after` writes.
fn reached(stage: ChildStage) -> String {
    format!(
        "{EFFECT_ID_MARKER}01900000-0000-7000-8000-0000000fa003\n\
         vestrace-fault-scenario: {}\n",
        completion_marker(stage)
    )
}

/// A child's stderr as it looks when `drive` returned `Err`: the announced id,
/// then the setup-failure line and no completion line at all.
fn stopped_short(reason: &str) -> String {
    format!(
        "{EFFECT_ID_MARKER}01900000-0000-7000-8000-0000000fa003\n\
         {SETUP_FAILURE_MARKER}{reason}\n"
    )
}

/// Every point's own completion line is accepted for that point.
#[test]
fn a_child_that_completed_its_stage_is_accepted() {
    for point in EffectFaultPoint::required_points() {
        confirm_reached_point(&reached(aborts_at(point)), point).unwrap_or_else(|error| {
            panic!("a completed {point:?} child must be accepted: {error}")
        });
    }
}

/// The one confusion this crate must never produce. A child that aborted before
/// reaching its point leaves the same dead process and the same announced effect
/// id as one that reached it, and the database it leaves behind reads as an
/// earlier lifecycle stage. Accepting it would report "it never got there" as a
/// finding about the point it never got to — at point 3, a failed dispatch
/// rendered as a lifecycle disagreement after recovery.
#[test]
fn a_child_that_stopped_short_of_its_point_is_refused() {
    let stderr = stopped_short("the effect could not be dispatched: connection refused");

    let error = confirm_reached_point(&stderr, EffectFaultPoint::AfterDispatchBeforeReceipt)
        .expect_err("a child that never reached its point must not yield an observation");

    assert!(
        error.contains("Dispatched"),
        "the refusal must name the stage that was never completed: {error}"
    );
    assert!(
        error.contains("connection refused"),
        "the refusal must carry why the child stopped, or it is unactionable: {error}"
    );
}

/// Point 5's precondition guard refuses *after* `insert_reconciliation` has
/// committed, so its rows are byte-identical to a successful point-5 run and no
/// reading of the database can tell the two apart. The guard added to refuse a
/// silent skip was therefore itself silent. This is the check that makes it
/// audible, and it is the only one: the refusal is visible upstream of the
/// database or it is not visible anywhere.
#[test]
fn the_point_five_precondition_refusal_is_not_an_observation() {
    let stderr = stopped_short(
        "the settled outcome is owed to nothing, so there is no run commit to die before",
    );

    let error = confirm_reached_point(&stderr, EffectFaultPoint::AfterOutcomeBeforeRunCommit)
        .expect_err("a point-5 precondition refusal must not be reported as a point-5 run");

    assert!(
        error.contains("owed to nothing"),
        "the refusal must carry the guard's own reason: {error}"
    );
}

/// A completion line is not a token of "some stage finished". A child driven to
/// the wrong stage — by a point-to-stage table that drifted, or by a parent that
/// passed a point the child did not run — is refused rather than filed under the
/// point that was asked for.
#[test]
fn another_points_completion_line_is_refused() {
    let stderr = reached(ChildStage::IntentPersisted);

    let error = confirm_reached_point(&stderr, EffectFaultPoint::AfterDispatchBeforeReceipt)
        .expect_err("a child that completed a different stage must be refused");

    assert!(
        error.contains("Dispatched"),
        "the refusal must name the stage that was asked for: {error}"
    );
}

/// A child that died before it could announce anything is the same refusal. It
/// is worth its own case because the marker check runs before the effect id is
/// parsed, and a run with no stderr at all must not slip through as "nothing to
/// object to".
#[test]
fn a_child_that_said_nothing_is_refused() {
    for point in EffectFaultPoint::required_points() {
        confirm_reached_point("", point).expect_err("a silent child must not yield an observation");
    }
}
