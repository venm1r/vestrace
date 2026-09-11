use axum::{Router, http::Method, routing::MethodRouter};
use vestrace_domain::{AuthorizationRequest, Capability, RiskCategory};

use crate::AppState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteDescriptor {
    pub method: Method,
    pub path_pattern: &'static str,
    pub capability: Capability,
    pub risk: RiskCategory,
    pub exposure: RouteExposure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteExposure {
    Governed,
    PublicBounded,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RouteDecision {
    Governed(AuthorizationRequest),
    PublicBounded,
    NotInInventory,
}

macro_rules! governed {
    ($method:ident, $path:literal, $capability:ident, $risk:ident) => {
        RouteDescriptor {
            method: Method::$method,
            path_pattern: $path,
            capability: Capability::$capability,
            risk: RiskCategory::$risk,
            exposure: RouteExposure::Governed,
        }
    };
}

macro_rules! public_probe {
    ($path:literal) => {
        RouteDescriptor {
            method: Method::GET,
            path_pattern: $path,
            capability: Capability::AuditRead,
            risk: RiskCategory::Low,
            exposure: RouteExposure::PublicBounded,
        }
    };
}

static ROUTE_INVENTORY: &[RouteDescriptor] = &[
    public_probe!("/health/live"),
    public_probe!("/health/ready"),
    governed!(GET, "/metrics", AuditRead, Low),
    governed!(GET, "/ag-ui/endpoints", ExecutionRead, Low),
    governed!(POST, "/ag-ui/run", ExecutionWrite, High),
    governed!(GET, "/ag-ui/events/stream", ExecutionRead, Low),
    governed!(GET, "/v1/runs", ExecutionRead, Low),
    governed!(POST, "/v1/runs", ExecutionWrite, High),
    governed!(GET, "/v1/runs/{id}", ExecutionRead, Low),
    governed!(POST, "/v1/runs/{id}/steps", ExecutionWrite, High),
    governed!(POST, "/v1/runs/{id}/pause", ExecutionWrite, High),
    governed!(POST, "/v1/runs/{id}/resume", ExecutionWrite, High),
    governed!(POST, "/v1/runs/{id}/cancel", ExecutionWrite, High),
    governed!(POST, "/v1/runs/{id}/approve", ExecutionWrite, Critical),
    governed!(POST, "/v1/events", EventWrite, Medium),
    governed!(POST, "/v1/memories", MemoryWrite, Medium),
    governed!(GET, "/v1/memories/{id}", MemoryRead, Low),
    governed!(DELETE, "/v1/memories/{id}", MemoryPurge, Critical),
    governed!(POST, "/v1/memories/{id}/revisions", MemoryWrite, Medium),
    governed!(POST, "/v1/retrieval/search", ContextRetrieve, Medium),
    governed!(GET, "/v1/agents", AgentRead, Low),
    governed!(POST, "/v1/agents", AgentWrite, Medium),
    governed!(GET, "/v1/models", ModelRead, Low),
    governed!(POST, "/v1/models", ModelWrite, Medium),
    governed!(GET, "/v1/providers", ProviderRead, Low),
    governed!(POST, "/v1/providers", ProviderWrite, Medium),
    governed!(GET, "/v1/skills", SkillRead, Low),
    governed!(POST, "/v1/skills", SkillWrite, Medium),
    governed!(GET, "/v1/routing/decisions", ModelRead, Low),
    governed!(POST, "/v1/routing/decisions", ModelWrite, Medium),
    governed!(GET, "/v1/executions", ExecutionRead, Low),
    governed!(POST, "/v1/executions", ExecutionWrite, High),
    governed!(POST, "/v1/workflow-executions", ExecutionWrite, High),
    governed!(GET, "/v1/workflow-executions/{id}", ExecutionRead, Low),
    governed!(
        GET,
        "/v1/workflow-executions/{id}/steps",
        ExecutionRead,
        Low
    ),
    governed!(
        POST,
        "/v1/workflow-executions/{id}/steps",
        ExecutionWrite,
        High
    ),
    governed!(
        POST,
        "/v1/workflow-executions/{id}/complete",
        ExecutionWrite,
        High
    ),
    governed!(
        GET,
        "/v1/workflow-executions/{id}/outcomes",
        ExecutionRead,
        Low
    ),
    governed!(
        POST,
        "/v1/workflow-executions/{id}/outcomes",
        ExecutionWrite,
        High
    ),
    governed!(GET, "/v1/workflows", WorkflowRead, Low),
    governed!(POST, "/v1/workflows", WorkflowWrite, Medium),
    governed!(GET, "/v1/workflows/{id}", WorkflowRead, Low),
    governed!(GET, "/v1/evaluations", EvaluationRead, Low),
    governed!(POST, "/v1/evaluations", EvaluationWrite, Medium),
    governed!(GET, "/v1/evaluations/{id}", EvaluationRead, Low),
    governed!(GET, "/v1/evaluation-facts", EvaluationRead, Low),
    governed!(POST, "/v1/evaluation-facts", EvaluationWrite, Medium),
    governed!(GET, "/v1/evaluation-facts/{id}", EvaluationRead, Low),
    governed!(GET, "/v1/learning/projections", LearningRead, Low),
    governed!(POST, "/v1/learning/projections", LearningWrite, Medium),
    governed!(GET, "/v1/learning/projections/{id}", LearningRead, Low),
    governed!(GET, "/v1/learning/proposals", LearningRead, Low),
    governed!(POST, "/v1/learning/proposals", LearningWrite, Medium),
    governed!(GET, "/v1/learning/proposals/{id}", LearningRead, Low),
    governed!(
        POST,
        "/v1/learning/proposals/{id}/submit",
        LearningWrite,
        Medium
    ),
    governed!(GET, "/v1/capability-grants", WorkspaceAdmin, Low),
    governed!(POST, "/v1/capability-grants", WorkspaceAdmin, Medium),
    governed!(
        POST,
        "/v1/capability-grants/{id}/revoke",
        WorkspaceAdmin,
        Medium
    ),
    governed!(GET, "/v1/access-tokens", WorkspaceAdmin, Critical),
    governed!(POST, "/v1/access-tokens", WorkspaceAdmin, Critical),
    governed!(DELETE, "/v1/access-tokens/{id}", WorkspaceAdmin, Critical),
    governed!(
        GET,
        "/v1/access-tokens/{id}/value",
        WorkspaceAdmin,
        Critical
    ),
    governed!(GET, "/v1/secrets", WorkspaceAdmin, Low),
    governed!(POST, "/v1/secrets", WorkspaceAdmin, Critical),
    governed!(DELETE, "/v1/secrets/{id}", WorkspaceAdmin, Critical),
    governed!(GET, "/v1/secrets/{id}/value", WorkspaceAdmin, Critical),
    governed!(POST, "/v1/secrets/{id}/value", WorkspaceAdmin, Critical),
    governed!(GET, "/v1/settings", WorkspaceAdmin, Low),
    governed!(PUT, "/v1/settings", WorkspaceAdmin, Medium),
    governed!(GET, "/v1/audit", AuditRead, Low),
    governed!(
        POST,
        "/v1/embedding-jobs/{id}/acknowledge-unknown",
        EmbeddingRetryAfterUnknown,
        Critical
    ),
    governed!(
        POST,
        "/v1/embedding-transitions/{id}/acknowledge-carry",
        EmbeddingRetryCarriedTransitionBatchAfterUnknown,
        Critical
    ),
    governed!(
        POST,
        "/v1/embedding-jobs/{id}/retry-generation-changed",
        EmbeddingRetryRetrievalGenerationChanged,
        Critical
    ),
    governed!(GET, "/v1/artifacts", ExportRead, Low),
    governed!(GET, "/v1/triggers", WorkspaceAdmin, Low),
    governed!(GET, "/v1/connections", WorkspaceAdmin, Low),
    governed!(POST, "/v1/connections", WorkspaceAdmin, Critical),
    governed!(
        POST,
        "/v1/connections/{id}/revisions",
        WorkspaceAdmin,
        Critical
    ),
    governed!(
        POST,
        "/v1/connections/{id}/admission-policies",
        WorkspaceAdmin,
        Critical
    ),
    governed!(
        POST,
        "/v1/connections/{id}/qualifications",
        ProviderWrite,
        High
    ),
    governed!(
        POST,
        "/v1/connections/{id}/credentials",
        WorkspaceAdmin,
        Critical
    ),
    governed!(
        POST,
        "/v1/connections/{id}/credentials/{revision_id}/abandon",
        WorkspaceAdmin,
        Critical
    ),
    governed!(
        POST,
        "/v1/connections/{id}/credentials/{revision_id}/activate",
        WorkspaceAdmin,
        Critical
    ),
    governed!(
        POST,
        "/v1/connections/{id}/credentials/{revision_id}/revoke",
        WorkspaceAdmin,
        Critical
    ),
    governed!(POST, "/v1/models/{id}/revisions", ModelWrite, Medium),
    governed!(POST, "/v1/models/{id}/qualifications", ModelWrite, High),
    governed!(GET, "/v1/profile", WorkspaceAdmin, Low),
    governed!(POST, "/v1/effects", ExecutionWrite, Critical),
    governed!(GET, "/v1/system/health", WorkspaceAdmin, Low),
    governed!(
        POST,
        "/v1/system/health/findings/{id}/disposition",
        WorkspaceAdmin,
        Medium
    ),
    governed!(GET, "/v1/metrics/summary", AuditRead, Low),
];

pub fn route_inventory() -> &'static [RouteDescriptor] {
    validate_route_inventory(ROUTE_INVENTORY);
    ROUTE_INVENTORY
}

pub fn validate_route_inventory(descriptors: &[RouteDescriptor]) {
    for descriptor in descriptors {
        if descriptor.exposure == RouteExposure::PublicBounded
            && !matches!(descriptor.path_pattern, "/health/live" | "/health/ready")
        {
            panic!("only health probes may be public");
        }
    }

    for descriptor in descriptors {
        if descriptor.exposure == RouteExposure::Governed
            && descriptors.iter().any(|other| {
                other.exposure == RouteExposure::PublicBounded
                    && other.path_pattern == descriptor.path_pattern
            })
        {
            panic!("governed route cannot duplicate a public path pattern");
        }
    }
}

pub fn route_descriptor(method: &Method, path_pattern: &'static str) -> &'static RouteDescriptor {
    route_inventory()
        .iter()
        .find(|descriptor| descriptor.method == *method && descriptor.path_pattern == path_pattern)
        .unwrap_or_else(|| {
            panic!("route descriptor is absent from the inventory: {method} {path_pattern}")
        })
}

/// Registers `descriptor.path_pattern` only after confirming the descriptor is
/// in the inventory. This cannot inspect a `MethodRouter` to verify that it
/// serves `descriptor.method`; axum 0.8.9 exposes no sound public introspection
/// API for that relationship.
pub fn mount(
    router: Router<AppState>,
    descriptor: &RouteDescriptor,
    handler: MethodRouter<AppState>,
) -> Router<AppState> {
    let registered = route_inventory().iter().any(|entry| entry == descriptor);
    assert!(registered, "route descriptor is absent from the inventory");
    router.route(mounted_path(descriptor.path_pattern), handler)
}

pub fn inventory_lookup(method: &Method, path: &str) -> RouteDecision {
    let Some(descriptor) = route_inventory()
        .iter()
        .find(|descriptor| descriptor_matches(descriptor, method, path))
    else {
        return RouteDecision::NotInInventory;
    };

    match descriptor.exposure {
        RouteExposure::PublicBounded => RouteDecision::PublicBounded,
        RouteExposure::Governed => RouteDecision::Governed(AuthorizationRequest::new(
            descriptor.capability.clone(),
            format!("http.{}", method.as_str().to_ascii_lowercase()),
            path,
            descriptor.risk,
        )),
    }
}

fn mounted_path(path_pattern: &'static str) -> &'static str {
    path_pattern
        .strip_prefix("/v1")
        .or_else(|| path_pattern.strip_prefix("/ag-ui"))
        .unwrap_or(path_pattern)
}

fn descriptor_matches(descriptor: &RouteDescriptor, method: &Method, path: &str) -> bool {
    (descriptor.method == *method || (*method == Method::HEAD && descriptor.method == Method::GET))
        && path_matches(descriptor.path_pattern, path)
}

fn path_matches(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<_> = pattern.split('/').collect();
    let path_segments: Vec<_> = path.split('/').collect();
    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments)
            .all(|(pattern, actual)| {
                (pattern.starts_with('{') && pattern.ends_with('}')) || pattern == &actual
            })
}
