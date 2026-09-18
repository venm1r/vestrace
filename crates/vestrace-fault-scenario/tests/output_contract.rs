//! The process contract: one JSON object on stdout, and nothing else.
//!
//! The reader on the other side is `ProcessFaultObservation` in
//! `crates/vestrace-application/src/fault_runtime.rs`. It is `deny_unknown_fields`
//! and its `point` and `status` parsers accept a closed vocabulary, so an extra
//! field, a renamed field or a renamed variant is a hard failure — and one that
//! only shows up after a database, a container and five child processes have
//! been paid for. This file pays for it in milliseconds instead.
//!
//! Nothing here asserts *values*. What the observation says is whatever the
//! world said; this pins only the shape it is said in.

use serde::Deserialize;
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, FaultObservation,
};
use vestrace_fault_scenario::report;

/// A copy of the reader's shape, field for field.
///
/// It is a copy rather than the reader itself because the reader is private to
/// `vestrace-application` and reachable only by spawning a process — which is
/// the very cost this test exists to avoid paying. The copy is kept honest by
/// `every_emitted_name_is_one_the_reader_accepts`, which reads the reader's own
/// source.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessFaultObservation {
    point: String,
    status: String,
    retry_attempted: bool,
    reconciliation_started: bool,
    receipt_persisted: bool,
}

const ALL_STATUSES: [EffectLifecycleStatus; 8] = [
    EffectLifecycleStatus::Prepared,
    EffectLifecycleStatus::Authorized,
    EffectLifecycleStatus::Dispatching,
    EffectLifecycleStatus::Acknowledged,
    EffectLifecycleStatus::Failed,
    EffectLifecycleStatus::Unknown,
    EffectLifecycleStatus::Confirmed,
    EffectLifecycleStatus::Reconciling,
];

fn observation(point: EffectFaultPoint, status: EffectLifecycleStatus) -> FaultObservation {
    // Deliberately not `FaultObservation::expected`: the flags below are
    // arbitrary, because this test is about the envelope and not the contents.
    FaultObservation {
        point,
        status,
        retry_attempted: true,
        reconciliation_started: false,
        receipt_persisted: true,
    }
}

/// Every observation this program can print must survive the reader.
#[test]
fn the_report_is_accepted_by_the_readers_shape() {
    assert_eq!(
        EffectFaultPoint::required_points().len(),
        5,
        "the external-effect output contract has exactly five points"
    );
    for point in EffectFaultPoint::required_points() {
        for status in ALL_STATUSES {
            let rendered = report::render(&observation(point, status));
            let parsed: ProcessFaultObservation = serde_json::from_str(&rendered)
                .unwrap_or_else(|error| panic!("the reader rejected {rendered}: {error}"));

            assert_eq!(parsed.point, report::point_name(point));
            assert_eq!(parsed.status, report::status_name(status));
            assert!(parsed.retry_attempted);
            assert!(!parsed.reconciliation_started);
            assert!(parsed.receipt_persisted);
        }
    }
}

#[test]
fn the_external_effect_report_refuses_intent_points() {
    for point in EffectFaultPoint::intent_points() {
        assert!(
            std::panic::catch_unwind(|| report::point_name(point)).is_err(),
            "{point:?} reached the external-effect report vocabulary"
        );
    }
}

/// One object, one line, nothing else — `serde_json::from_slice` is handed the
/// process's whole stdout, so a second line or a trailing banner is a parse
/// failure in an environment that takes minutes to reach.
#[test]
fn the_report_is_a_single_json_object_and_nothing_else() {
    let rendered = report::render(&observation(
        EffectFaultPoint::AfterDispatchBeforeReceipt,
        EffectLifecycleStatus::Unknown,
    ));

    assert!(
        !rendered.contains('\n'),
        "the report spans lines: {rendered}"
    );
    assert!(rendered.starts_with('{') && rendered.ends_with('}'));
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");
    let object = value.as_object().expect("a JSON object");
    assert_eq!(object.len(), 5, "the reader denies unknown fields");
}

/// The vocabulary is closed on the reader's side, so a renamed point or status
/// is not a compile error anywhere — it is a rejected observation. This reads
/// the reader's own source and requires every name this program can emit to
/// appear there.
#[test]
fn every_emitted_name_is_one_the_reader_accepts() {
    let reader = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vestrace-application/src/fault_runtime.rs"),
    )
    .expect("the reader's source is in this workspace");

    for field in [
        "point",
        "status",
        "retry_attempted",
        "reconciliation_started",
        "receipt_persisted",
    ] {
        assert!(
            reader.contains(&format!("    {field}:")),
            "the reader declares no field named {field}"
        );
    }
    for point in EffectFaultPoint::required_points() {
        let name = report::point_name(point);
        assert!(
            reader.contains(&format!("\"{name}\"")),
            "the reader does not accept the point name {name}"
        );
    }
    for status in ALL_STATUSES {
        let name = report::status_name(status);
        assert!(
            reader.contains(&format!("\"{name}\"")),
            "the reader does not accept the status name {name}"
        );
    }
}
