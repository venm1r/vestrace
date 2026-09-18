//! The q1 loopback contract begins with the pinned profile, rather than a
//! local test-only profile that could diverge from the checked-in manifest.

use std::io::{Read, Write};

use vestrace_application::{
    ConnectionAuth, ConnectionKind, ModelsListRequest, Q1ProbeFailure, Q1ProbeRequest,
    Q1ProbeResponse,
};
use vestrace_domain::QualificationJobId;
use vestrace_infrastructure::{OpenAiCompatibleClient, openai_q1::OpenAiQ1Profile};

#[test]
fn pinned_q1_profile_drives_exact_network_and_static_probe_count() {
    let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();
    let static_count = profile
        .probes()
        .iter()
        .filter(|probe| !probe.is_network())
        .count();
    let network_count = profile
        .probes()
        .iter()
        .filter(|probe| probe.is_network())
        .count();

    assert_eq!(static_count, 2, "00 and 15 cannot create HTTP calls");
    assert_eq!(
        network_count, 10,
        "each network ordinal owns one effect/call"
    );
    let nonce = profile.nonce(QualificationJobId::new(), "20").unwrap();
    assert_eq!(nonce.len(), 24);
    assert!(
        nonce
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    );
}

#[tokio::test]
async fn q1_models_probe_uses_the_production_adapter_against_loopback() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let count = socket.read(&mut request).unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"data\":[{\"id\":\"q1-chat\"}]}",
            )
            .unwrap();
        String::from_utf8_lossy(&request[..count]).into_owned()
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();

    let response = client.models_list(ModelsListRequest::new()).await.unwrap();
    let request = server.join().unwrap();
    assert_eq!(response.data[0].id, "q1-chat");
    assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    assert!(!request.contains("Authorization:"));
}

#[tokio::test]
async fn q1_non_success_keeps_the_exact_numeric_status_without_body_or_error_text() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        // The request is read only so the client can finish sending it; its
        // content is never inspected, so a partial read is fine and the amount
        // is discarded deliberately rather than left unhandled.
        let _read = socket.read(&mut request).unwrap();
        let body = r#"{"error":{"message":"q1 raw secret must not escape"}}"#;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();

    let refusal = client
        .execute_q1_for_qualification(Q1ProbeRequest::ModelsList)
        .await
        .expect_err("the pinned q1 models probe must expose its exact safe HTTP status");
    server.join().unwrap();
    assert!(matches!(
        refusal,
        Q1ProbeFailure::HttpStatus { status: 404 }
    ));
    assert!(!format!("{refusal:?}").contains("q1 raw secret"));
}

#[tokio::test]
async fn q1_optional_status_table_keeps_every_numeric_refusal_closed() {
    for (status, reason) in [
        (400, "Bad Request"),
        (404, "Not Found"),
        (405, "Method Not Allowed"),
        (415, "Unsupported Media Type"),
        (422, "Unprocessable Content"),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _read = socket.read(&mut request).unwrap();
            let body = r#"{"error":{"message":"never retain this raw q1 body"}}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        });
        let client = OpenAiCompatibleClient::for_connection(
            ConnectionKind::LMStudioLocal,
            format!("http://{address}/v1"),
            ConnectionAuth::None,
        )
        .unwrap();
        let refusal = client
            .execute_q1_for_qualification(Q1ProbeRequest::ModelsList)
            .await
            .expect_err("the closed q1 error must retain only its numeric status");
        server.join().unwrap();
        assert!(
            matches!(refusal, Q1ProbeFailure::HttpStatus { status: observed } if observed == status)
        );
        assert!(!format!("{refusal:?}").contains("never retain this raw q1 body"));
    }
}

#[tokio::test]
async fn q1_all_ten_network_ordinals_use_closed_typed_adapter_requests_once() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut captured = Vec::new();
        for _ in 0..10 {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = Vec::with_capacity(16 * 1024);
            let mut chunk = [0_u8; 4096];
            loop {
                let read = socket.read(&mut chunk).unwrap();
                assert_ne!(read, 0, "loopback client closed before a complete request");
                bytes.extend_from_slice(&chunk[..read]);
                let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&bytes[..headers_end]).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                if bytes.len() >= headers_end + 4 + content_length {
                    break;
                }
            }
            let response = q1_response_for_request(&bytes);
            socket.write_all(response.as_bytes()).unwrap();
            captured.push(bytes);
        }
        captured
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();
    let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();
    let job = QualificationJobId::new();
    let ordinals = ["10", "20", "30", "35", "40", "50", "60", "70", "80", "90"];
    for ordinal in ordinals {
        let request = profile
            .typed_request(
                job,
                ordinal,
                "q1-chat",
                "q1-embedding",
                (ordinal == "50").then(|| {
                    vestrace_application::Q1AssistantToolCallReplay::new("call_q1").unwrap()
                }),
            )
            .unwrap();
        let response = client
            .execute_q1(request)
            .await
            .unwrap_or_else(|error| panic!("q1 ordinal {ordinal} oracle failed: {error:?}"));
        assert!(matches!(
            response,
            Q1ProbeResponse::ModelsList(_)
                | Q1ProbeResponse::Chat(_)
                | Q1ProbeResponse::Embeddings(_)
        ));
    }
    let captured = server.join().unwrap();
    assert_eq!(captured.len(), 10, "q1 must not retry a dispatched request");
    let request_text = captured
        .iter()
        .map(|request| String::from_utf8_lossy(request).into_owned())
        .collect::<Vec<_>>();
    assert!(request_text[0].starts_with("GET /v1/models HTTP/1.1\r\n"));
    for request in &request_text {
        assert!(!request.contains("Authorization:"));
    }
    let body = |index: usize| -> serde_json::Value {
        let (_, body) = request_text[index].split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    };
    let nonce = |ordinal| profile.nonce(job, ordinal).unwrap();
    assert_eq!(body(1)["model"], "q1-chat");
    assert_eq!(
        body(1)["messages"][0]["content"],
        format!(
            "Return exactly VESTRACE_Q1_TEXT_{} and nothing else.",
            nonce("20")
        )
    );
    assert_eq!(body(2)["stream"], true);
    assert!(body(2).get("stream_options").is_none());
    assert_eq!(body(3)["stream_options"]["include_usage"], true);
    assert_eq!(body(4)["tool_choice"]["function"]["name"], "vestrace_probe");
    assert_eq!(body(5)["messages"][0]["role"], "assistant");
    assert_eq!(body(5)["messages"][0]["tool_calls"][0]["id"], "call_q1");
    assert_eq!(body(6)["tool_choice"], "required");
    assert_eq!(body(6)["parallel_tool_calls"], true);
    assert_eq!(body(7)["response_format"]["type"], "json_schema");
    assert!(body(8)["messages"][0]["content"].is_array());
    assert_eq!(body(9)["model"], "q1-embedding");
    assert_eq!(body(9)["encoding_format"], "float");
    assert_eq!(body(9)["input"].as_array().unwrap().len(), 2);
}

fn q1_response_for_request(request: &[u8]) -> String {
    let text = String::from_utf8_lossy(request);
    if text.starts_with("GET /v1/models HTTP/1.1\r\n") {
        return http_json(r#"{"data":[{"id":"q1-chat"},{"id":"q1-embedding"}]}"#);
    }
    let (_, body) = text.split_once("\r\n\r\n").unwrap();
    let request: serde_json::Value = serde_json::from_str(body).unwrap();
    let model = request["model"].as_str().unwrap();
    if text.starts_with("POST /v1/embeddings HTTP/1.1\r\n") {
        return http_json(&format!(
            r#"{{"model":"{model}","data":[{{"index":0,"embedding":[0.0,1.0]}},{{"index":1,"embedding":[1.0,0.0]}}],"usage":{{"prompt_tokens":2,"total_tokens":2}}}}"#
        ));
    }
    let nonce = |suffix: &str| -> String {
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|message| message["content"].as_str())
            .find_map(|content| {
                content
                    .split("VESTRACE_Q1_TEXT_")
                    .nth(1)
                    .and_then(|value| value.split_whitespace().next())
            })
            .unwrap_or(suffix)
            .trim_end_matches(['.', ','])
            .to_owned()
    };
    let expected_nonce = request
        .get("response_format")
        .and_then(|format| format.get("json_schema"))
        .and_then(|schema| schema.get("schema"))
        .and_then(|schema| schema.get("properties"))
        .and_then(|properties| properties.get("nonce"))
        .and_then(|nonce| nonce.get("const"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            request["tools"]
                .as_array()
                .and_then(|tools| tools.first())
                .and_then(|tool| {
                    tool["function"]["parameters"]["properties"]["value"]["const"].as_str()
                })
                .map(|value| value.strip_suffix("-a").unwrap_or(value).to_owned())
        })
        .or_else(|| {
            request["messages"]
                .as_array()
                .and_then(|messages| messages.first())
                .and_then(|message| message["tool_calls"][0]["function"]["arguments"].as_str())
                .and_then(|arguments| serde_json::from_str::<serde_json::Value>(arguments).ok())
                .and_then(|arguments| arguments["value"].as_str().map(ToOwned::to_owned))
        })
        .unwrap_or_else(|| nonce("missing"));
    if request["stream"].as_bool() == Some(true) {
        let expected = format!("VESTRACE_Q1_TEXT_{expected_nonce}");
        let usage = if request
            .get("stream_options")
            .is_some_and(|options| options["include_usage"] == true)
        {
            ",\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}"
        } else {
            ""
        };
        let stream = format!(
            "data: {{\"model\":\"{model}\",\"choices\":[{{\"delta\":{{\"content\":\"{expected}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"model\":\"{model}\",\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]{usage}}}\n\ndata: [DONE]\n\n"
        );
        return http_sse(&stream);
    }
    let tools = request["tools"].as_array().unwrap_or(&Vec::new()).len();
    if tools == 1 {
        return http_json(&format!(
            r#"{{"model":"{model}","choices":[{{"message":{{"role":"assistant","tool_calls":[{{"id":"call_q1","type":"function","function":{{"name":"vestrace_probe","arguments":"{{\"value\":\"{expected_nonce}\"}}"}}}}]}},"finish_reason":"tool_calls"}}]}}"#
        ));
    }
    if tools == 2 {
        return http_json(&format!(
            r#"{{"model":"{model}","choices":[{{"message":{{"role":"assistant","tool_calls":[{{"id":"call_q1_a","type":"function","function":{{"name":"vestrace_probe_a","arguments":"{{\"value\":\"{expected_nonce}-a\"}}"}}}},{{"id":"call_q1_b","type":"function","function":{{"name":"vestrace_probe_b","arguments":"{{\"value\":\"{expected_nonce}-b\"}}"}}}}]}},"finish_reason":"tool_calls"}}]}}"#
        ));
    }
    let content = if request.get("response_format").is_some() {
        format!(r#"{{"nonce":"{expected_nonce}","ok":true}}"#)
    } else if request["messages"].as_array().is_some_and(|messages| {
        messages
            .first()
            .is_some_and(|message| message["content"].is_array())
    }) {
        "VESTRACE_Q1_IMAGE".to_owned()
    } else if request["messages"].as_array().is_some_and(|messages| {
        messages
            .first()
            .is_some_and(|message| message["role"] == "assistant")
    }) {
        format!("VESTRACE_Q1_TOOL_DONE_{expected_nonce}")
    } else {
        format!("VESTRACE_Q1_TEXT_{expected_nonce}")
    };
    http_json(&format!(
        r#"{{"model":"{model}","choices":[{{"message":{{"role":"assistant","content":{}}},"finish_reason":"stop"}}]}}"#,
        serde_json::to_string(&content).unwrap()
    ))
}

fn http_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn http_sse(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
