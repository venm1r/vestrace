use std::sync::Arc;

use serde::Deserialize;
use vestrace_application::{
    ApplicationError, AuthorizationBoundary, DenyAllPolicyEngine, RequestContext,
};
use vestrace_domain::{AuthorizationRequest, Capability, PrincipalId, RiskCategory, WorkspaceId};

const G2_FIXTURE: &str = include_str!("fixtures/qualification/g2-universal-authorization.json");

#[derive(Debug, Deserialize)]
struct G2FixtureManifest {
    schema_version: String,
    profile: String,
    release_gate: String,
    status: String,
    cases: Vec<G2FixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct G2FixtureCase {
    id: String,
    status: String,
    surfaces: Vec<String>,
    test: String,
}

#[test]
fn g2_fixture_manifest_is_traceable_without_claiming_qualification() {
    let manifest: G2FixtureManifest = serde_json::from_str(G2_FIXTURE).unwrap();

    assert_eq!(
        manifest.schema_version,
        "vestrace.g2.authorization-fixture.v1"
    );
    assert_eq!(manifest.profile, "GOVERNANCE");
    assert_eq!(manifest.release_gate, "v0.4 Govern");
    assert_eq!(manifest.status, "evidence_fixture_only");
    assert!(!manifest.known_limitations.is_empty());
    for surface in ["application", "http", "mcp", "worker", "internal"] {
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.surfaces.iter().any(|candidate| candidate == surface)),
            "{surface} has no G2 fixture mapping"
        );
    }
    for case in &manifest.cases {
        assert!(!case.id.is_empty());
        assert_eq!(case.status, "covered");
        assert!(
            case.test.starts_with("g2_")
                || case.test.starts_with("default_")
                || case.test.starts_with("denied_")
        );
    }
}

#[tokio::test]
async fn g2_shared_boundary_rejects_a_denied_decision_before_execution() {
    let boundary = AuthorizationBoundary::new(Arc::new(DenyAllPolicyEngine));
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let request = AuthorizationRequest::new(
        Capability::ExecutionWrite,
        "g2.test.execute",
        "workspace://",
        RiskCategory::Low,
    );

    let error = boundary.require(&context, request).await.unwrap_err();

    assert!(matches!(error, ApplicationError::Policy(message) if message.contains("DefaultDeny")));
}
