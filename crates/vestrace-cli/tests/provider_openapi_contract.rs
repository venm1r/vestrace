use std::process::Command;

fn schema() -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["schema", "http"])
        .output()
        .expect("schema command should start");
    assert!(
        output.status.success(),
        "schema command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("schema command should emit OpenAPI JSON")
}

#[test]
fn provider_execution_openapi_has_the_exact_governed_mutation_set() {
    let document = schema();
    let paths = &document["paths"];
    for path in [
        "/v1/connections",
        "/v1/connections/{id}/revisions",
        "/v1/connections/{id}/admission-policies",
        "/v1/connections/{id}/qualifications",
        "/v1/connections/{id}/credentials",
        "/v1/connections/{id}/credentials/{revision_id}/abandon",
        "/v1/connections/{id}/credentials/{revision_id}/activate",
        "/v1/connections/{id}/credentials/{revision_id}/revoke",
        "/v1/models/{id}/revisions",
        "/v1/models/{id}/qualifications",
    ] {
        assert_eq!(
            paths[path]["post"]["parameters"][0]["name"], "x-workspace-id",
            "{path}"
        );
        assert_eq!(
            paths[path]["post"]["parameters"][1]["name"], "x-principal-id",
            "{path}"
        );
    }
    assert_eq!(
        paths["/v1/providers"]["post"]["responses"]["403"]["description"],
        "Legacy provider registry retired"
    );
}

#[test]
fn embedding_unknown_acknowledgement_is_a_governed_mutation_contract() {
    let document = schema();
    let operation = &document["paths"]["/v1/embedding-jobs/{id}/acknowledge-unknown"]["post"];
    let request = &document["components"]["schemas"]["AcknowledgeEmbeddingJobUnknownRequest"];

    assert_eq!(
        operation["parameters"][0]["name"], "x-workspace-id",
        "the shared governed headers retain their positional compatibility"
    );
    assert_eq!(operation["parameters"][1]["name"], "x-principal-id");
    assert_eq!(
        operation["parameters"][3]["name"], "Idempotency-Key",
        "new governed-mutation header is appended after the existing contract"
    );
    assert_eq!(operation["parameters"][3]["required"], true);
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/AcknowledgeEmbeddingJobUnknownRequest"
    );
    assert!(
        request["required"]
            .as_array()
            .is_some_and(|required| required
                .iter()
                .any(|field| field == "expected_predecessor_version")),
        "the predecessor version is an explicit caller statement"
    );
    assert!(
        request["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|field| field == "kind")),
        "the successor kind is an explicit caller statement"
    );
    assert!(operation["responses"]["409"].is_object());
}

#[test]
fn governed_list_projection_openapi_has_only_safe_projection_shapes() {
    let document = schema();
    let paths = &document["paths"];
    let components = &document["components"]["schemas"];
    for (path, projection) in [
        ("/v1/connections", "GovernedConnection"),
        ("/v1/models", "GovernedModel"),
        ("/v1/providers", "GovernedProvider"),
    ] {
        let get = &paths[path]["get"];
        assert_eq!(
            get["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|parameter| parameter["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["x-workspace-id", "x-principal-id"],
            "{path} must carry its workspace and principal scope"
        );
        assert_eq!(
            get["responses"]["200"]["content"]["application/json"]["schema"]["type"], "array",
            "{path} must document a successful projection list"
        );
        assert_eq!(
            get["responses"]["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            format!("#/components/schemas/{projection}"),
            "{path} must not expose its legacy registry schema"
        );
    }

    for (projection, fields) in [
        (
            "GovernedConnection",
            [
                "blockers",
                "id",
                "qualification_state",
                "revision_id",
                "state",
            ]
            .as_slice(),
        ),
        (
            "GovernedModel",
            [
                "blockers",
                "id",
                "qualification_state",
                "revision_id",
                "state",
            ]
            .as_slice(),
        ),
        ("GovernedProvider", ["blockers", "id", "state"].as_slice()),
    ] {
        let properties = components[projection]["properties"].as_object().unwrap();
        let mut actual = properties.keys().map(String::as_str).collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(
            actual, fields,
            "{projection} must expose only opaque safe fields"
        );
        assert_eq!(components[projection]["additionalProperties"], false);
    }
}

/// The published document must state the admission bounds the domain enforces.
///
/// `ConnectionAdmissionLimits` derives `Deserialize` over private fields, so a
/// surface that deserialized the domain type directly would accept a policy its
/// own constructor refuses. The route parses plain integers and calls `new`;
/// this asserts the document says what `new` will accept, so an operator does
/// not discover the bounds by being refused.
#[test]
fn the_admission_policy_request_documents_the_bounds_the_domain_enforces() {
    let document = schema();
    let schema = &document["components"]["schemas"]["PublishConnectionAdmissionPolicyRequest"];
    for (field, minimum, maximum) in [
        ("max_in_flight", 1, 64),
        ("requests_per_60_seconds", 1, 60_000),
        ("queue_wait_timeout_seconds", 1, 300),
        ("provider_throttle_cap_seconds", 1, 900),
    ] {
        assert_eq!(schema["properties"][field]["minimum"], minimum, "{field}");
        assert_eq!(schema["properties"][field]["maximum"], maximum, "{field}");
    }
    // Zero is lawful here and nowhere else: it is how a caller says the
    // Connection has no policy yet.
    assert_eq!(schema["properties"]["expected_head_version"]["minimum"], 0);
    let required = schema["required"]
        .as_array()
        .expect("the request must state its required fields")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        required.contains(&"expected_head_version"),
        "an optional head version would let a publisher omit what it is replacing"
    );
    assert_eq!(
        document["paths"]["/v1/connections/{id}/admission-policies"]["post"]["responses"]["409"]["description"],
        "The stated head version is not the current one"
    );
}
