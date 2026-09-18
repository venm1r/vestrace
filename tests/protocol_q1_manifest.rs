use ring::digest;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn root_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn q1() -> Value {
    serde_json::from_slice(
        &fs::read(root_path(
            "schemas/openai-compatible/openai-chat-completions-v1-q1.json",
        ))
        .expect("q1 manifest must exist"),
    )
    .expect("q1 manifest must be JSON")
}

fn probe<'a>(manifest: &'a Value, ordinal: &str) -> &'a Value {
    manifest["model_probes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["ordinal"] == ordinal)
        .unwrap_or_else(|| panic!("missing probe {ordinal}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    digest::digest(&digest::SHA256, bytes).as_ref().iter().fold(
        String::with_capacity(64),
        |mut hex, byte| {
            std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"))
                .expect("writing to String cannot fail");
            hex
        },
    )
}

fn oracle_without_classification(value: &Value) -> Value {
    let mut oracle = value.clone();
    oracle.as_object_mut().unwrap().remove("classification");
    oracle
}

#[test]
fn q1_manifest_independently_locks_every_probe_template_oracle_and_status_rule() {
    let manifest = q1();
    let path = |pointer: &str| {
        manifest
            .pointer(pointer)
            .unwrap_or_else(|| panic!("missing {pointer}"))
    };
    assert_eq!(path("/profile_id"), "openai-chat-completions-v1/q1");
    assert_eq!(path("/schema_version"), 1);
    assert_eq!(
        path("/bounds"),
        &json!({
            "embedding_dimensions":{"max":65536,"min":1},"event_count":4096,
            "idle_timeout_seconds":10,"nonstream_response_bytes":1048576,
            "sse_response_bytes":2097152,"stream_total_timeout_seconds":30,"tool_argument_bytes":8192
        })
    );
    assert_eq!(
        path("/nonce_derivation"),
        &json!({
            "encoding":"24_lowercase_hex_characters",
            "input":"QualificationJobId || profile_digest || probe_ordinal","sha256_bits":96
        })
    );
    assert_eq!(path("/no_automatic_retry_after_dispatch"), true);
    assert_eq!(
        manifest["connection_probes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["ordinal"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["00", "10"]
    );
    assert_eq!(
        manifest["model_probes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["ordinal"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["15", "20", "30", "35", "40", "50", "60", "70", "80", "90"]
    );
    for oracle in manifest["connection_probes"]
        .as_array()
        .unwrap()
        .iter()
        .chain(manifest["model_probes"].as_array().unwrap())
        .map(|probe| &probe["oracle"])
    {
        assert_eq!(oracle["classification"], "structural_bounded");
        assert_eq!(oracle["safe_evidence"]["provider_body"], "forbidden");
    }

    let static_probe = &manifest["connection_probes"][0];
    assert_eq!(static_probe["kind"], "connection_static");
    assert_eq!(static_probe["network"], false);
    assert_eq!(static_probe["required_for"], json!(["connection"]));
    assert_eq!(
        oracle_without_classification(&static_probe["oracle"]),
        json!({
            "auth":{"closed_enum":["None","Bearer","ApiKey","XApiKey"],"credential_required_for":["Bearer","ApiKey","XApiKey"],"no_auth_header_for":"None","no_credential_reference_for":"None"},
            "base_url":{"forbid_fragment":true,"forbid_query":true,"forbid_userinfo":true,"normalized":true},
            "safe_evidence":{"provider_body":"forbidden"},
            "transport_policy":"connection_revision_exact","uses_manifest_bounds":true
        })
    );
    let models = &manifest["connection_probes"][1];
    assert_eq!(models["kind"], "models_list");
    assert_eq!(models["method"], "GET");
    assert_eq!(models["network"], true);
    assert_eq!(models["path"], "/models");
    assert_eq!(models["required_for"], json!(["connection"]));
    assert_eq!(models["valid_for_seconds"], 900);
    assert_eq!(
        oracle_without_classification(&models["oracle"]),
        json!({
            "adapter":"production",
            "auth_binding":{"credential_branch":"exact_revision_guard_lineage","no_auth_branch":"no_auth_header_and_no_credential_reference","target_binding":"QualificationTargetBinding"},
            "discovery_digest":{"model_ids":"sorted_before_digest"},
            "egress_evidence":{"peer":"record_actual","webpki":"record_actual"},
            "http":{"content_type":"application/json","status":200},
            "models":{"id_utf8_bytes":{"max":256,"min":1},"max_entries":4096,"nonempty":true,"unique":true},
            "response_bytes_max":1048576,"safe_evidence":{"provider_body":"forbidden"}
        })
    );
    let fixture = &manifest["fixtures"]["image_marker"];
    assert_eq!(
        fixture["data_url"],
        "data:image/png;base64,${q1_marker_png_base64}"
    );
    assert_eq!(fixture["media_type"], "image/png");
    assert_eq!(fixture["path"], "tests/fixtures/openai-q1/marker.png");
    assert_eq!(fixture["visible_text"], "VESTRACE_Q1_IMAGE");
    assert_eq!(
        fixture["sha256"],
        sha256_hex(&fs::read(root_path("tests/fixtures/openai-q1/marker.png")).unwrap())
    );

    let binding = probe(&manifest, "15");
    assert_eq!(binding["kind"], "discovery_binding");
    assert_eq!(binding["network"], false);
    assert_eq!(binding["optional"], false);
    assert_eq!(binding["prerequisites"], json!([]));
    assert_eq!(binding["required_for"], json!(["model_binding"]));
    assert_eq!(binding["valid_for_seconds"], 86400);
    assert_eq!(
        oracle_without_classification(&binding["oracle"]),
        json!({"connection_discovery":{"compatible":true,"unexpired":true},"safe_evidence":{"provider_body":"forbidden"},"wire_model_id":"exact_discovered_model_id"})
    );

    let text = probe(&manifest, "20");
    assert_eq!(text["kind"], "chat_text");
    assert_eq!(text["method"], "POST");
    assert_eq!(text["network"], true);
    assert_eq!(text["optional"], false);
    assert_eq!(text["path"], "/chat/completions");
    assert_eq!(text["prerequisites"], json!([]));
    assert_eq!(text["required_for"], json!(["chat"]));
    assert_eq!(
        text["request_template"],
        json!({"max_tokens":32,"messages":[{"content":"Return exactly VESTRACE_Q1_TEXT_<nonce> and nothing else.","role":"user"}],"model":"${qualified_wire_model_id}","stream":false,"temperature":0})
    );
    assert_eq!(
        oracle_without_classification(&text["oracle"]),
        json!({"assistant_choices":{"count":1,"role":"assistant"},"content":{"equals":"VESTRACE_Q1_TEXT_<nonce>","normalize":"trim"},"finish_reason":"stop","http":{"content_type":"application/json","status":200},"returned_model":{"when_present":"exact_qualified_wire_model_id"},"safe_evidence":{"provider_body":"forbidden"}})
    );

    let stream = probe(&manifest, "30");
    assert_eq!(stream["kind"], "sse_stream");
    assert_eq!(stream["method"], "POST");
    assert_eq!(stream["network"], true);
    assert_eq!(stream["optional"], true);
    assert_eq!(stream["path"], "/chat/completions");
    assert_eq!(stream["prerequisites"], json!([]));
    assert_eq!(stream["required_for"], json!(["streaming"]));
    assert_eq!(
        stream["request_template"],
        json!({"max_tokens":32,"messages":[{"content":"Return exactly VESTRACE_Q1_TEXT_<nonce> and nothing else.","role":"user"}],"model":"${qualified_wire_model_id}","stream":true,"temperature":0})
    );
    assert!(stream["request_template"].get("stream_options").is_none());
    assert_eq!(
        oracle_without_classification(&stream["oracle"]),
        json!({"deltas":{"ordered":true,"reconstructs_exactly":"VESTRACE_Q1_TEXT_<nonce>"},"finish_reason":{"count":1,"value":"stop"},"http":{"content_type":"text/event-stream"},"post_terminal_data":"forbidden","response_identity":{"model":"consistent_and_exact_qualified_wire_model_id_when_present","response":"one_consistent_identity"},"safe_evidence":{"provider_body":"forbidden","usage_when_absent":"unknown","usage_when_present":"record"},"sse":{"done_after_terminal":true,"event_count_max":4096,"idle_timeout_seconds":10,"response_bytes_max":2097152,"total_timeout_seconds":30}})
    );

    let usage = probe(&manifest, "35");
    assert_eq!(usage["kind"], "stream_usage");
    assert_eq!(usage["method"], "POST");
    assert_eq!(usage["network"], true);
    assert_eq!(usage["optional"], true);
    assert_eq!(usage["path"], "/chat/completions");
    assert_eq!(
        usage["prerequisites"],
        json!([{"ordinal":"30","result":"Pass"}])
    );
    assert_eq!(usage["prerequisite_failure_result"], "SkippedPrerequisite");
    assert_eq!(usage["required_for"], json!(["streamed_usage"]));
    assert_eq!(
        usage["request_template"],
        json!({"max_tokens":32,"messages":[{"content":"Return exactly VESTRACE_Q1_TEXT_<nonce> and nothing else.","role":"user"}],"model":"${qualified_wire_model_id}","stream":true,"stream_options":{"include_usage":true},"temperature":0})
    );
    assert_eq!(
        oracle_without_classification(&usage["oracle"]),
        json!({"includes_ordinal_30_oracle":true,"safe_evidence":{"provider_body":"forbidden"},"usage":{"before_done":true,"completion_tokens":"nonnegative_integer","count":1,"prompt_tokens":"nonnegative_integer","total_equals":"prompt_tokens + completion_tokens","total_tokens":"nonnegative_integer","unambiguous":true}})
    );

    let tool = probe(&manifest, "40");
    assert_eq!(tool["kind"], "tool_call");
    assert_eq!(tool["method"], "POST");
    assert_eq!(tool["network"], true);
    assert_eq!(tool["optional"], true);
    assert_eq!(tool["path"], "/chat/completions");
    assert_eq!(tool["prerequisites"], json!([]));
    assert_eq!(tool["required_for"], json!(["tool_calling"]));
    assert_eq!(
        tool["request_template"],
        json!({"messages":[{"content":"Call vestrace_probe with value <nonce>.","role":"user"}],"model":"${qualified_wire_model_id}","stream":false,"tool_choice":{"function":{"name":"vestrace_probe"},"type":"function"},"tools":[{"function":{"name":"vestrace_probe","parameters":{"additionalProperties":false,"properties":{"value":{"const":"<nonce>"}},"required":["value"],"type":"object"}},"type":"function"}]})
    );
    assert_eq!(
        oracle_without_classification(&tool["oracle"]),
        json!({"finish_reason":"tool_calls","safe_evidence":{"provider_body":"forbidden"},"tool_calls":{"arguments":{"additional_properties":false,"equals":{"value":"<nonce>"},"schema_valid":true},"call_id":{"count":1,"nonblank":true},"function_name":"vestrace_probe"}})
    );

    let result = probe(&manifest, "50");
    assert_eq!(result["kind"], "tool_result");
    assert_eq!(result["method"], "POST");
    assert_eq!(result["network"], true);
    assert_eq!(result["optional"], true);
    assert_eq!(result["path"], "/chat/completions");
    assert_eq!(
        result["prerequisites"],
        json!([{"ordinal":"40","result":"Pass"}])
    );
    assert_eq!(result["prerequisite_failure_result"], "SkippedPrerequisite");
    assert_eq!(result["required_for"], json!(["tool_result"]));
    assert_eq!(
        result["request_template"],
        json!({"messages":[{"content":"${ordinal_40.assistant_tool_call}","role":"assistant"},{"content":"VESTRACE_Q1_TOOL_RESULT_<nonce>","role":"tool","tool_call_id":"${ordinal_40.tool_call_id}"},{"content":"Return exactly VESTRACE_Q1_TOOL_DONE_<nonce> and nothing else.","role":"user"}],"model":"${qualified_wire_model_id}","stream":false})
    );
    assert_eq!(
        oracle_without_classification(&result["oracle"]),
        json!({"assistant_choices":{"count":1,"role":"assistant"},"content":{"equals":"VESTRACE_Q1_TOOL_DONE_<nonce>","normalize":"trim"},"finish_reason":"stop","http":{"content_type":"application/json","status":200},"safe_evidence":{"provider_body":"forbidden"}})
    );

    let parallel = probe(&manifest, "60");
    assert_eq!(parallel["kind"], "parallel_tools");
    assert_eq!(parallel["method"], "POST");
    assert_eq!(parallel["network"], true);
    assert_eq!(parallel["optional"], true);
    assert_eq!(parallel["path"], "/chat/completions");
    assert_eq!(
        parallel["prerequisites"],
        json!([{"ordinal":"40","result":"Pass"}])
    );
    assert_eq!(
        parallel["prerequisite_failure_result"],
        "SkippedPrerequisite"
    );
    assert_eq!(parallel["required_for"], json!(["parallel_tool_calling"]));
    assert_eq!(
        parallel["request_template"],
        json!({"messages":[{"content":"Call both required tools with their exact nonce values.","role":"user"}],"model":"${qualified_wire_model_id}","parallel_tool_calls":true,"stream":false,"tool_choice":"required","tools":[{"function":{"name":"vestrace_probe_a","parameters":{"additionalProperties":false,"properties":{"value":{"const":"<nonce>-a"}},"required":["value"],"type":"object"}},"type":"function"},{"function":{"name":"vestrace_probe_b","parameters":{"additionalProperties":false,"properties":{"value":{"const":"<nonce>-b"}},"required":["value"],"type":"object"}},"type":"function"}]})
    );
    assert_eq!(
        oracle_without_classification(&parallel["oracle"]),
        json!({"finish_reason":"tool_calls","safe_evidence":{"provider_body":"forbidden"},"tool_calls":{"count":2,"distinct_call_ids":true,"exact_function_set":["vestrace_probe_a","vestrace_probe_b"],"one_turn":true,"schema_valid_exact_arguments":true}})
    );

    let structured = probe(&manifest, "70");
    assert_eq!(structured["kind"], "structured_output");
    assert_eq!(structured["method"], "POST");
    assert_eq!(structured["network"], true);
    assert_eq!(structured["optional"], true);
    assert_eq!(structured["path"], "/chat/completions");
    assert_eq!(structured["prerequisites"], json!([]));
    assert_eq!(structured["required_for"], json!(["structured_output"]));
    let strict = json!({"additionalProperties":false,"properties":{"nonce":{"const":"<nonce>"},"ok":{"const":true}},"required":["nonce","ok"],"type":"object"});
    assert_eq!(
        structured["request_template"]["messages"],
        json!([{"content":"Return the required strict JSON object for nonce <nonce>.","role":"user"}])
    );
    assert_eq!(
        structured["request_template"]["model"],
        "${qualified_wire_model_id}"
    );
    assert_eq!(structured["request_template"]["stream"], false);
    assert_eq!(
        structured["request_template"]["response_format"]["type"],
        "json_schema"
    );
    assert_eq!(
        structured["request_template"]["response_format"]["json_schema"]["name"],
        "vestrace_q1_structured_output"
    );
    assert_eq!(
        structured["request_template"]["response_format"]["json_schema"]["schema"],
        strict
    );
    assert_eq!(
        structured["request_template"]["response_format"]["json_schema"]["strict"],
        true
    );
    assert_eq!(
        oracle_without_classification(&structured["oracle"]),
        json!({"safe_evidence":{"provider_body":"forbidden"},"terminal_json":{"additional_properties":false,"properties":{"nonce":{"const":"<nonce>"},"ok":{"const":true}},"required":["nonce","ok"],"validates_exactly":true}})
    );

    let image = probe(&manifest, "80");
    assert_eq!(image["kind"], "image_input");
    assert_eq!(image["method"], "POST");
    assert_eq!(image["network"], true);
    assert_eq!(image["optional"], true);
    assert_eq!(image["path"], "/chat/completions");
    assert_eq!(image["prerequisites"], json!([]));
    assert_eq!(image["required_for"], json!(["image_input"]));
    assert_eq!(
        image["request_template"],
        json!({"messages":[{"content":[{"text":"Return exactly VESTRACE_Q1_IMAGE and nothing else.","type":"text"},{"image_url":{"url":"data:image/png;base64,${q1_marker_png_base64}"},"type":"image_url"}],"role":"user"}],"model":"${qualified_wire_model_id}","stream":false})
    );
    assert_eq!(
        oracle_without_classification(&image["oracle"]),
        json!({"adapter_url_fetch":"forbidden","assistant_choices":{"count":1,"role":"assistant"},"content":{"equals":"VESTRACE_Q1_IMAGE","normalize":"trim"},"fixture_sha256":"b0343846f3c9f250e25edb45b73993b747bc85143f040c207cd76b6936af4de6","finish_reason":"stop","safe_evidence":{"provider_body":"forbidden"}})
    );

    let embeddings = probe(&manifest, "90");
    assert_eq!(embeddings["kind"], "embeddings");
    assert_eq!(embeddings["method"], "POST");
    assert_eq!(embeddings["network"], true);
    assert_eq!(embeddings["optional"], false);
    assert_eq!(embeddings["path"], "/embeddings");
    assert_eq!(embeddings["prerequisites"], json!([]));
    assert_eq!(embeddings["required_for"], json!(["embeddings"]));
    assert_eq!(
        embeddings["request_template"],
        json!({"encoding_format":"float","input":["VESTRACE_Q1_EMBED_<nonce>_A","VESTRACE_Q1_EMBED_<nonce>_B"],"model":"${qualified_embedding_wire_model_id}"})
    );
    assert_eq!(
        oracle_without_classification(&embeddings["oracle"]),
        json!({"embeddings":{"count":2,"dimensions":{"all_same_positive_dimension":true,"max":65536,"vectors":"finite_nonempty"},"indices_exactly":[0,1]},"http":{"content_type":"application/json","status":"2xx"},"returned_model":{"when_present":"exact_qualified_wire_model_id"},"safe_evidence":{"provider_body":"forbidden","usage_when_absent":"unknown","usage_when_present":"record"}})
    );

    let rules = &manifest["qualification_rules"];
    assert_eq!(rules["connection_valid_for_seconds"], 900);
    assert_eq!(rules["model_valid_for_seconds"], 86400);
    assert_eq!(
        rules["network_probe"],
        json!({"adapter_invocations":1,"automatic_retry":false,"external_effects":1})
    );
    assert_eq!(
        rules["result_classes"],
        json!([
            "Pass",
            "UnsupportedDefinite",
            "FailedDefinite",
            "InconclusiveUnknown",
            "SkippedPrerequisite"
        ])
    );
    assert_eq!(
        rules["cancellation"],
        json!({"allowed":"between_probes_only","while_dispatching":"forbidden"})
    );
    assert_eq!(
        rules["status_rules"],
        json!({"after_dispatch":{"crash":"InconclusiveUnknown","eof":"InconclusiveUnknown","partial_response":"InconclusiveUnknown","precedence_over_oracle_failure":true,"timeout":"InconclusiveUnknown"},"http":{"401_403":"FailedDefinite","429_5xx":{"does_not_invalidate_earlier_compatible_qualification":true,"result":"FailedDefinite","retry":"forbidden"},"optional_400_404_405_415_422":"UnsupportedDefinite","required_400_404_405_415_422":"FailedDefinite"},"optional_safely_bounded_completed_2xx_exact_oracle_failure":"UnsupportedDefinite","pre_dispatch_transport_or_validation":"FailedDefinite","required_safely_bounded_completed_2xx_exact_oracle_failure":"FailedDefinite","response_bound_or_structural_evidence_violation":"FailedDefinite"})
    );
    assert_eq!(
        rules["completion"],
        json!({"inconclusive_unknown":{"job_result":"InconclusiveUnknown","publishes_qualification":false,"stops_requested_suite":true},"optional_unsupported":{"capability":"not_qualified_by_q1","only_after":"every_requested_probe_definite"},"required_definite_failure":"FailedDefinite","succeeded":{"optional_results":["Pass","UnsupportedDefinite","SkippedPrerequisite"],"required_results":"all_Pass"}})
    );
}
