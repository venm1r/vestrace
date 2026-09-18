use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
    routing::post,
};
use tower::ServiceExt;
use vestrace_application::{
    ArtifactListing, ConnectionRevisionRepository, GovernedArtifactMaterial,
    GovernedConnectionProjection, GovernedModelProjection, GovernedProviderProjection,
    ModelRevisionRepository,
};
use vestrace_domain::{
    Capability, ConnectionId, ConnectionRevisionId, ModelId, ModelRevisionId, ProviderId,
    RiskCategory,
};
use vestrace_http::{
    AppState,
    route_inventory::{RouteExposure, route_inventory},
};

#[test]
fn app_state_exposes_only_explicit_governed_projection_ports() {
    fn projection_ports(state: &AppState) {
        let _: Result<&dyn ConnectionRevisionRepository, _> =
            state.connection_revision_repository();
        let _: Result<&dyn ModelRevisionRepository, _> = state.model_revision_repository();
    }

    let _ = projection_ports;
}

#[test]
fn governed_projection_responses_serialize_only_opaque_safe_fields() {
    let connection_id = ConnectionId::new();
    let connection_revision_id = ConnectionRevisionId::new();
    let connection = serde_json::to_value(
        vestrace_http::api::connections::ConnectionResponse::from(GovernedConnectionProjection {
            id: connection_id,
            revision_id: Some(connection_revision_id),
            state: "qualified".to_owned(),
            qualification_state: "qualified".to_owned(),
            blockers: Vec::new(),
            no_auth_binding_revision_id: None,
        }),
    )
    .unwrap();
    assert_eq!(
        connection,
        serde_json::json!({
            "id": connection_id.as_uuid(),
            "revision_id": connection_revision_id.as_uuid(),
            "state": "qualified",
            "qualification_state": "qualified",
            "blockers": [],
            "no_auth_binding_revision_id": null,
        })
    );

    let model_id = ModelId::new();
    let model_revision_id = ModelRevisionId::new();
    let model = serde_json::to_value(vestrace_http::api::models::ModelResponse::from(
        GovernedModelProjection {
            id: model_id,
            revision_id: Some(model_revision_id),
            state: "blocked".to_owned(),
            qualification_state: "missing".to_owned(),
            blockers: vec!["qualification_required".to_owned()],
        },
    ))
    .unwrap();
    assert_eq!(
        model,
        serde_json::json!({
            "id": model_id.as_uuid(),
            "revision_id": model_revision_id.as_uuid(),
            "state": "blocked",
            "qualification_state": "missing",
            "blockers": ["qualification_required"],
        })
    );

    let provider_id = ProviderId::new();
    let provider = serde_json::to_value(vestrace_http::api::models::ProviderResponse::from(
        GovernedProviderProjection {
            id: provider_id,
            state: "qualified".to_owned(),
            blockers: Vec::new(),
        },
    ))
    .unwrap();
    assert_eq!(
        provider,
        serde_json::json!({
            "id": provider_id.as_uuid(),
            "state": "qualified",
            "blockers": [],
        })
    );
}

/// The provider-control surface is one governed contract.  Adding a handler
/// without this descriptor would bypass the authorization middleware, while a
/// descriptor without a handler is caught by the route-inventory exhaustive
/// test.  Keep the exact capability and risk at the boundary.
#[test]
fn provider_execution_routes_have_exact_governed_descriptors() {
    let expected = [
        (
            Method::POST,
            "/v1/connections",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/revisions",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/qualifications",
            Capability::ProviderWrite,
            RiskCategory::High,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/credentials",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/abandon",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/activate",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/connections/{id}/credentials/{revision_id}/revoke",
            Capability::WorkspaceAdmin,
            RiskCategory::Critical,
        ),
        (
            Method::POST,
            "/v1/models/{id}/revisions",
            Capability::ModelWrite,
            RiskCategory::Medium,
        ),
        (
            Method::POST,
            "/v1/models/{id}/qualifications",
            Capability::ModelWrite,
            RiskCategory::High,
        ),
    ];

    for (method, path_pattern, capability, risk) in expected {
        assert!(
            route_inventory().iter().any(|descriptor| {
                descriptor.method == method
                    && descriptor.path_pattern == path_pattern
                    && descriptor.capability == capability
                    && descriptor.risk == risk
                    && descriptor.exposure == RouteExposure::Governed
            }),
            "missing or weakened descriptor for {method} {path_pattern}",
        );
    }
}

#[test]
fn the_legacy_provider_create_route_remains_governed_until_its_typed_refusal() {
    assert!(route_inventory().iter().any(|descriptor| {
        descriptor.method == Method::POST
            && descriptor.path_pattern == "/v1/providers"
            && descriptor.capability == Capability::ProviderWrite
            && descriptor.risk == RiskCategory::Medium
            && descriptor.exposure == RouteExposure::Governed
    }));
}

/// The registry must be retired at the actual handler boundary.  In
/// particular, a valid-looking legacy body must not reach the repository and
/// create a row merely because the route remains mounted for compatibility.
#[tokio::test]
async fn legacy_provider_create_is_a_typed_retirement_refusal() {
    let app = Router::new().route(
        "/v1/providers",
        post(vestrace_http::api::models::create_provider),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/providers")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{\"name\":\"legacy\",\"locality\":\"local\"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "legacy_provider_registry_retired");
    assert_eq!(body["message"], "Legacy provider registry retired");
}

#[test]
fn provider_material_artifact_projection_never_populates_legacy_digest_or_exact_size() {
    let listing = ArtifactListing {
        artifact: vestrace_domain::artifact::Artifact {
            id: vestrace_domain::ArtifactId::new(),
            workspace_id: vestrace_domain::WorkspaceId::new(),
            name: "provider-result-opaque".to_owned(),
            status: vestrace_domain::artifact::ArtifactStatus::Active,
            created_at: vestrace_domain::now(),
        },
        latest_revision: None,
        governed_material: Some(GovernedArtifactMaterial {
            artifact_revision_id: vestrace_domain::ArtifactRevisionId::new(),
            content_material_id: vestrace_domain::ContentMaterialId::new(),
            erasure_bound_commitment: [0x42; 32],
            size_class: vestrace_domain::SizeClass::EightKiB,
            media_class: vestrace_application::ProviderArtifactMediaClass::Text,
        }),
    };

    let response = vestrace_http::api::artifacts::ArtifactResponse::from(listing);
    let value = serde_json::to_value(response).unwrap();
    assert!(value["revision_id"].is_string());
    assert!(value["content_material_id"].is_string());
    assert_eq!(
        value["erasure_bound_commitment"].as_str().unwrap().len(),
        64
    );
    assert_eq!(value["size_class"], "eight_ki_b");
    assert_eq!(value["media_class"], "text");
    assert!(value["content_sha256"].is_null());
    assert!(value["size_bytes"].is_null());
    assert!(value["media_type"].is_null());
}
