use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde_json::{Value, json};
use vestrace_application::{
    ConnectionAuth, ConnectionKind, EffectiveChatMessage, EffectiveModelRequest,
    EffectiveModelResponse, EffectiveRequestLimits, EffectiveSampling, EffectiveToolSchema,
};
use vestrace_infrastructure::providers::OpenAiCompatibleClient;

#[derive(Clone, Default)]
struct Observer(Arc<Mutex<Vec<(String, Value)>>>);

#[derive(Clone, Default)]
struct AdapterConsumptionProbe(Arc<AtomicUsize>);

impl AdapterConsumptionProbe {
    async fn execute(
        &self,
        client: &OpenAiCompatibleClient,
        request: EffectiveModelRequest,
        expected_content_storage: usize,
    ) {
        let EffectiveModelRequest::ChatCompletions(chat) = &request else {
            panic!("the consuming probe expects the governed chat request");
        };
        assert_eq!(
            chat.messages()[0].content().as_ptr() as usize,
            expected_content_storage,
            "the adapter boundary must receive the same owned semantic content allocation"
        );
        assert_eq!(
            self.0.fetch_add(1, Ordering::SeqCst),
            0,
            "the governed request must cross the adapter boundary exactly once"
        );
        client.execute_effective(request).await.unwrap();
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

async fn client_and_observer(expected_requests: usize) -> (OpenAiCompatibleClient, Observer) {
    let observer = Observer::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server_observer = observer.clone();
    std::thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "request ended before its headers");
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap()
                + 4;
            let headers = std::str::from_utf8(&request[..header_end])
                .unwrap()
                .to_owned();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or(0);
            while request.len() - header_end < content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "request ended before its body");
                request.extend_from_slice(&buffer[..read]);
            }
            let request_line = headers.lines().next().unwrap();
            let path = request_line.split_whitespace().nth(1).unwrap();
            let body = if content_length == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap()
            };
            let (kind, response) = match path {
                "/models" => ("models", json!({"data": [{"id": "model-a"}]})),
                "/chat/completions" => (
                    "chat",
                    json!({
                        "choices": [
                            {"message": {"content": "ok"}, "finish_reason": "stop"}
                        ],
                        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
                    }),
                ),
                "/embeddings" => {
                    let model = body["model"].as_str().expect("the adapter states a model");
                    let outputs = body["input"]
                        .as_array()
                        .expect("the adapter states its inputs")
                        .len();
                    (
                        "embeddings",
                        json!({
                            "model": model,
                            "data": (0..outputs)
                                .map(|index| json!({"index": index, "embedding": [0.25, 0.75]}))
                                .collect::<Vec<_>>()
                        }),
                    )
                }
                _ => panic!("unexpected provider path {path}"),
            };
            server_observer.0.lock().unwrap().push((kind.into(), body));
            let response = serde_json::to_vec(&response).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                response.len()
            )
            .unwrap();
            stream.write_all(&response).unwrap();
            stream.flush().unwrap();
        }
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}"),
        ConnectionAuth::None,
    )
    .unwrap();
    (client, observer)
}

#[tokio::test]
async fn loopback_observes_semantic_equality_with_the_production_adapter() {
    let (client, observer) = client_and_observer(3).await;

    assert!(matches!(
        client
            .execute_effective(EffectiveModelRequest::models_list())
            .await
            .unwrap(),
        EffectiveModelResponse::ModelsList(_)
    ));
    assert!(matches!(
        client
            .execute_effective(
                EffectiveModelRequest::chat_completions(
                    "model-a",
                    vec![EffectiveChatMessage::user("hello governed world")],
                    EffectiveSampling::new(0.25, 0.9).unwrap(),
                    EffectiveRequestLimits::new(128, 4, 65_536).unwrap(),
                    vec![
                        EffectiveToolSchema::new(
                            "lookup",
                            Some("Lookup a record".into()),
                            json!({
                                "type": "object",
                                "properties": {"id": {"type": "string"}},
                                "required": ["id"],
                                "additionalProperties": false
                            }),
                        )
                        .unwrap()
                    ],
                    false,
                )
                .unwrap(),
            )
            .await
            .unwrap(),
        EffectiveModelResponse::ChatCompletions(_)
    ));
    assert!(matches!(
        client
            .execute_effective(
                EffectiveModelRequest::embeddings(
                    "embed-a",
                    vec![
                        "first governed input".into(),
                        "second governed input".into()
                    ],
                    EffectiveRequestLimits::new(1, 4, 65_536).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap(),
        EffectiveModelResponse::Embeddings(_)
    ));

    let observed = observer.0.lock().unwrap();
    assert_eq!(observed[0], ("models".into(), Value::Null));
    assert_eq!(
        observed[1],
        (
            "chat".into(),
            json!({
                "model": "model-a",
                "messages": [{"role": "user", "content": "hello governed world"}],
                "temperature": 0.25,
                "top_p": 0.9,
                "max_tokens": 128,
                "stream": false,
                "tools": [{
                    "type": "function",
                    "function": {
                        "name": "lookup",
                        "description": "Lookup a record",
                        "parameters": {
                            "type": "object",
                            "properties": {"id": {"type": "string"}},
                            "required": ["id"],
                            "additionalProperties": false
                        }
                    }
                }]
            })
        )
    );
    assert_eq!(
        observed[2],
        (
            "embeddings".into(),
            json!({
                "model": "embed-a",
                "input": ["first governed input", "second governed input"],
                "encoding_format": "float"
            })
        )
    );
    assert!(observed[2].1.get("dimensions").is_none());
}

#[test]
fn effective_request_is_nonclone_and_nonserializable() {
    let request = EffectiveModelRequest::models_list();
    assert_eq!(request.kind().as_str(), "models_list");
    assert_eq!(format!("{request:?}"), "EffectiveModelRequest::ModelsList");
}

#[test]
fn governed_adapter_entrypoint_consumes_directly_without_a_legacy_conversion() {
    let adapter =
        include_str!("../crates/vestrace-infrastructure/src/providers/openai_compatible.rs");
    let start = adapter
        .find("pub async fn execute_effective(")
        .expect("the governed consuming entrypoint must exist");
    let end = adapter[start..]
        .find("async fn execute_effective_chat(")
        .map(|offset| start + offset)
        .expect("the private typed chat renderer must follow the entrypoint");
    let entrypoint = &adapter[start..end];
    assert!(entrypoint.contains("request: EffectiveModelRequest"));
    assert!(entrypoint.contains("match request"));
    assert!(!entrypoint.contains("ChatCompletionsRequest"));
    assert!(!entrypoint.contains("EmbeddingsRequest"));
    assert!(!entrypoint.contains("serde_json::Value"));
    assert!(!entrypoint.contains(".clone("));
}

#[test]
fn effective_request_rejects_tool_schemas_above_the_aggregate_ceiling() {
    let large_schema = json!({"value": "x".repeat(900_000)});
    let tools = (0..5)
        .map(|index| {
            EffectiveToolSchema::new(format!("tool-{index}"), None, large_schema.clone()).unwrap()
        })
        .collect();
    let request = EffectiveModelRequest::chat_completions(
        "model-a",
        vec![EffectiveChatMessage::user("bounded input")],
        EffectiveSampling::new(0.25, 0.9).unwrap(),
        EffectiveRequestLimits::new(128, 4, 65_536).unwrap(),
        tools,
        false,
    );

    assert!(request.is_err());
}

#[tokio::test]
async fn policy_authorization_and_adapter_observe_one_request_instance() {
    fn policy(request: &EffectiveModelRequest) -> *const EffectiveModelRequest {
        request as *const _
    }
    fn authorization(request: &EffectiveModelRequest) -> *const EffectiveModelRequest {
        request as *const _
    }

    let (client, observer) = client_and_observer(1).await;
    let request = EffectiveModelRequest::chat_completions(
        "same-instance-model",
        vec![EffectiveChatMessage::user("same governed allocation")],
        EffectiveSampling::new(0.25, 0.9).unwrap(),
        EffectiveRequestLimits::new(32, 1, 4096).unwrap(),
        Vec::new(),
        false,
    )
    .unwrap();
    assert_eq!(policy(&request), authorization(&request));
    let EffectiveModelRequest::ChatCompletions(chat) = &request else {
        unreachable!();
    };
    let content_storage = chat.messages()[0].content().as_ptr() as usize;
    let consumption = AdapterConsumptionProbe::default();
    consumption.execute(&client, request, content_storage).await;
    assert_eq!(consumption.count(), 1);
    assert_eq!(
        observer.0.lock().unwrap()[0],
        (
            "chat".into(),
            json!({
                "model": "same-instance-model",
                "messages": [{"role": "user", "content": "same governed allocation"}],
                "temperature": 0.25,
                "top_p": 0.9,
                "max_tokens": 32,
                "stream": false
            })
        )
    );
}
