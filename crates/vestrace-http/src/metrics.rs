use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use axum::{extract::State, response::IntoResponse};

use crate::router::AppState;

#[derive(Default)]
pub struct MetricsRegistry {
    http_requests_total: AtomicU64,
    http_errors_total: AtomicU64,
    mcp_tool_calls_total: AtomicU64,
    mcp_tool_errors_total: AtomicU64,
    jobs_processed_total: AtomicU64,
    jobs_failed_total: AtomicU64,
    jobs_dead_lettered_total: AtomicU64,
    outbox_pending: AtomicI64,
    outbox_oldest_age_seconds: AtomicI64,
    retrieval_requests_total: AtomicU64,
    retrieval_candidate_count_sum: AtomicU64,
    retrieval_degraded_total: AtomicU64,
    model_executions_total: AtomicU64,
    model_execution_failures_total: AtomicU64,
    model_tokens_input_total: AtomicU64,
    model_tokens_output_total: AtomicU64,
    model_estimated_cost_micros: AtomicU64,
    provider_errors_total: AtomicU64,
    provider_fallback_total: AtomicU64,
    memory_events_recorded_total: AtomicU64,
    memory_candidates_total: AtomicU64,
    memory_activations_total: AtomicU64,
    memory_revisions_total: AtomicU64,
    memory_conflicts_total: AtomicU64,
    memory_expirations_total: AtomicU64,
    retrieval_uses_total: AtomicU64,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_http_requests(&self) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_http_errors(&self) {
        self.http_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_mcp_tool_calls(&self) {
        self.mcp_tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_mcp_tool_errors(&self) {
        self.mcp_tool_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_processed(&self) {
        self.jobs_processed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_failed(&self) {
        self.jobs_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_dead_lettered(&self) {
        self.jobs_dead_lettered_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_outbox_pending(&self, count: i64) {
        self.outbox_pending.store(count, Ordering::Relaxed);
    }

    pub fn set_outbox_oldest_age(&self, seconds: i64) {
        self.outbox_oldest_age_seconds
            .store(seconds, Ordering::Relaxed);
    }

    pub fn inc_retrieval_requests(&self) {
        self.retrieval_requests_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_retrieval_candidates(&self, count: u64) {
        self.retrieval_candidate_count_sum
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn inc_retrieval_degraded(&self) {
        self.retrieval_degraded_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_executions(&self) {
        self.model_executions_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_execution_failures(&self) {
        self.model_execution_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_model_tokens_input(&self, count: u64) {
        self.model_tokens_input_total
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_model_tokens_output(&self, count: u64) {
        self.model_tokens_output_total
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_model_estimated_cost_micros(&self, micros: u64) {
        self.model_estimated_cost_micros
            .fetch_add(micros, Ordering::Relaxed);
    }

    pub fn inc_provider_errors(&self) {
        self.provider_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_provider_fallback(&self) {
        self.provider_fallback_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_events_recorded(&self) {
        self.memory_events_recorded_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_candidates(&self) {
        self.memory_candidates_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_activations(&self) {
        self.memory_activations_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_revisions(&self) {
        self.memory_revisions_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_conflicts(&self) {
        self.memory_conflicts_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_expirations(&self) {
        self.memory_expirations_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_retrieval_uses(&self) {
        self.retrieval_uses_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(4096);

        macro_rules! counter {
            ($name:expr, $help:expr, $val:expr) => {
                out.push_str(&format!(
                    "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
                    $name, $help, $name, $name, $val
                ));
            };
        }

        macro_rules! gauge {
            ($name:expr, $help:expr, $val:expr) => {
                out.push_str(&format!(
                    "# HELP {} {}\n# TYPE {} gauge\n{} {}\n",
                    $name, $help, $name, $name, $val
                ));
            };
        }

        counter!(
            "vestrace_http_requests_total",
            "Total HTTP requests received",
            self.http_requests_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_http_errors_total",
            "Total HTTP responses with 5xx status",
            self.http_errors_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_mcp_tool_calls_total",
            "Total MCP tool invocations",
            self.mcp_tool_calls_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_mcp_tool_errors_total",
            "Total MCP tool invocations that returned an error",
            self.mcp_tool_errors_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_jobs_processed_total",
            "Total jobs processed by workers",
            self.jobs_processed_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_jobs_failed_total",
            "Total jobs that exhausted retries",
            self.jobs_failed_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_jobs_dead_lettered_total",
            "Total jobs moved to dead-letter state",
            self.jobs_dead_lettered_total.load(Ordering::Relaxed)
        );
        gauge!(
            "vestrace_outbox_pending",
            "Outbox messages awaiting processing",
            self.outbox_pending.load(Ordering::Relaxed)
        );
        gauge!(
            "vestrace_outbox_oldest_age_seconds",
            "Age in seconds of the oldest unprocessed outbox message",
            self.outbox_oldest_age_seconds.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_retrieval_requests_total",
            "Total retrieval requests",
            self.retrieval_requests_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_retrieval_candidate_count_sum",
            "Sum of candidate counts across all retrieval requests",
            self.retrieval_candidate_count_sum.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_retrieval_degraded_total",
            "Retrieval requests that ran in degraded mode",
            self.retrieval_degraded_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_model_executions_total",
            "Total model executions",
            self.model_executions_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_model_execution_failures_total",
            "Total model execution failures",
            self.model_execution_failures_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_model_tokens_input_total",
            "Total input tokens consumed",
            self.model_tokens_input_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_model_tokens_output_total",
            "Total output tokens produced",
            self.model_tokens_output_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_model_estimated_cost_micros",
            "Estimated cost in micro-dollars",
            self.model_estimated_cost_micros.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_provider_errors_total",
            "Total provider errors",
            self.provider_errors_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_provider_fallback_total",
            "Total routing fallbacks to secondary provider",
            self.provider_fallback_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_events_recorded_total",
            "Total events recorded",
            self.memory_events_recorded_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_candidates_total",
            "Total memory candidates considered",
            self.memory_candidates_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_activations_total",
            "Total memory activations",
            self.memory_activations_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_revisions_total",
            "Total memory revisions",
            self.memory_revisions_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_conflicts_total",
            "Total memory conflicts detected",
            self.memory_conflicts_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_memory_expirations_total",
            "Total memory expirations",
            self.memory_expirations_total.load(Ordering::Relaxed)
        );
        counter!(
            "vestrace_retrieval_uses_total",
            "Total times a memory was used in retrieval context",
            self.retrieval_uses_total.load(Ordering::Relaxed)
        );

        out
    }
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let registry = state.metrics_registry();
    let body = registry.render_prometheus();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_contains_all_metrics() {
        let registry = MetricsRegistry::new();
        registry.inc_http_requests();
        registry.inc_http_errors();
        registry.inc_mcp_tool_calls();
        registry.set_outbox_pending(3);

        let output = registry.render_prometheus();

        assert!(output.contains("vestrace_http_requests_total 1"));
        assert!(output.contains("vestrace_http_errors_total 1"));
        assert!(output.contains("vestrace_mcp_tool_calls_total 1"));
        assert!(output.contains("vestrace_outbox_pending 3"));
        assert!(output.contains("# TYPE vestrace_outbox_pending gauge"));
        assert!(output.contains("# TYPE vestrace_http_requests_total counter"));
    }

    #[test]
    fn render_has_no_sensitive_labels() {
        let registry = MetricsRegistry::new();
        let output = registry.render_prometheus();

        assert!(!output.contains("query="));
        assert!(!output.contains("memory_id="));
        assert!(!output.contains("content="));
        assert!(!output.contains("user="));
    }
}
