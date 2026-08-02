use vestrace_domain::{
    id::*, observability::*, now,
};

#[test]
fn test_metric_rollup_creation() {
    let rollup_id = MetricRollupId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let rollup = MetricRollup {
        id: rollup_id,
        workspace_id: ws_id,
        metric_name: "harness.execution_latency_ms".into(),
        metric_value: 125.5,
        recorded_at: at,
    };

    assert_eq!(rollup.metric_name, "harness.execution_latency_ms");
    assert_eq!(rollup.metric_value, 125.5);
}
