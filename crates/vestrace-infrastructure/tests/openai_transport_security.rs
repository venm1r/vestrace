#[cfg(debug_assertions)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(debug_assertions)]
use std::time::Duration;
#[cfg(debug_assertions)]
use tokio::sync::Mutex;

use vestrace_application::{
    ChatCompletionsOutput, ChatCompletionsRequest, ChatSseEvent, ConnectionAuth, ConnectionKind,
    EffectiveChatEvidence, EffectiveChatFinishReason, EffectiveChatMessage, EffectiveModelRequest,
    EffectiveModelResponse, EffectiveRequestLimits, EffectiveSampling, EmbeddingsRequest,
    ModelsListRequest, ProviderCredential, ProviderError, ProviderUsage,
};
use vestrace_domain::DataDestination;
use vestrace_infrastructure::OpenAiCompatibleClient;
#[cfg(debug_assertions)]
use vestrace_infrastructure::providers::openai_compatible::{
    DebugGovernedBufferKind, DebugGovernedBufferMetadataSink,
};

#[cfg(debug_assertions)]
struct GovernedBufferObservingAllocator;

#[cfg(debug_assertions)]
static DEBUG_BUFFER_SINK: DebugGovernedBufferMetadataSink = DebugGovernedBufferMetadataSink::new();
#[cfg(debug_assertions)]
static BUFFER_TEST_GUARD: Mutex<()> = Mutex::const_new(());
#[cfg(debug_assertions)]
static JSON_BODY_OBSERVED: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static JSON_BODY_ZERO: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static JSON_BODY_LEN: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static SSE_BODY_OBSERVED: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static SSE_BODY_ZERO: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static SSE_BODY_LEN: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static PARSER_OBSERVED: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static PARSER_ZERO: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static PARSER_LEN: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static CONTENT_OBSERVED: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static CONTENT_ZERO: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static CONTENT_LEN: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static AUTH_HEADER_OBSERVED: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static AUTH_HEADER_ZERO: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static AUTH_HEADER_LEN: AtomicUsize = AtomicUsize::new(0);

// SAFETY: allocation/deallocation are forwarded unchanged. The observer reads
// only the initialized range registered by the passive production metadata
// sink before forwarding its exact deallocation and performs atomic writes.
#[cfg(debug_assertions)]
unsafe impl GlobalAlloc for GovernedBufferObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        for (kind, observed, zero, length) in [
            (
                DebugGovernedBufferKind::JsonBody,
                &JSON_BODY_OBSERVED,
                &JSON_BODY_ZERO,
                &JSON_BODY_LEN,
            ),
            (
                DebugGovernedBufferKind::SseBody,
                &SSE_BODY_OBSERVED,
                &SSE_BODY_ZERO,
                &SSE_BODY_LEN,
            ),
            (
                DebugGovernedBufferKind::ParserScratch,
                &PARSER_OBSERVED,
                &PARSER_ZERO,
                &PARSER_LEN,
            ),
            (
                DebugGovernedBufferKind::Content,
                &CONTENT_OBSERVED,
                &CONTENT_ZERO,
                &CONTENT_LEN,
            ),
            (
                DebugGovernedBufferKind::AuthHeader,
                &AUTH_HEADER_OBSERVED,
                &AUTH_HEADER_ZERO,
                &AUTH_HEADER_LEN,
            ),
        ] {
            let metadata = DEBUG_BUFFER_SINK.metadata(kind);
            if metadata.pointer == pointer as usize && metadata.pointer != 0 {
                let initialized_len = metadata.initialized_len.min(layout.size());
                let mut all_zero = metadata.initialized_len <= layout.size();
                for offset in 0..initialized_len {
                    if unsafe { pointer.add(offset).read() } != 0 {
                        all_zero = false;
                        break;
                    }
                }
                length.store(metadata.initialized_len, Ordering::SeqCst);
                zero.store(all_zero, Ordering::SeqCst);
                observed.store(true, Ordering::SeqCst);
                DEBUG_BUFFER_SINK.retire_if_matches(kind, pointer as usize);
            }
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
#[cfg(debug_assertions)]
static GLOBAL_ALLOCATOR: GovernedBufferObservingAllocator = GovernedBufferObservingAllocator;

#[cfg(debug_assertions)]
fn reset_buffer_observer() {
    for value in [
        &JSON_BODY_OBSERVED,
        &JSON_BODY_ZERO,
        &SSE_BODY_OBSERVED,
        &SSE_BODY_ZERO,
        &PARSER_OBSERVED,
        &PARSER_ZERO,
        &CONTENT_OBSERVED,
        &CONTENT_ZERO,
        &AUTH_HEADER_OBSERVED,
        &AUTH_HEADER_ZERO,
    ] {
        value.store(false, Ordering::SeqCst);
    }
    for length in [
        &JSON_BODY_LEN,
        &SSE_BODY_LEN,
        &PARSER_LEN,
        &CONTENT_LEN,
        &AUTH_HEADER_LEN,
    ] {
        length.store(0, Ordering::SeqCst);
    }
    DEBUG_BUFFER_SINK.begin();
}

#[cfg(debug_assertions)]
fn assert_zeroed(observed: &AtomicBool, zero: &AtomicBool, length: &AtomicUsize) {
    assert!(observed.load(Ordering::SeqCst));
    assert!(zero.load(Ordering::SeqCst));
    assert!(length.load(Ordering::SeqCst) > 0);
}

#[cfg(debug_assertions)]
fn effective_chat_request(stream: bool) -> EffectiveModelRequest {
    EffectiveModelRequest::chat_completions(
        "chat-model",
        vec![EffectiveChatMessage::user("hello")],
        EffectiveSampling::new(0.0, 1.0).unwrap(),
        EffectiveRequestLimits::new(32, 1, 32).unwrap(),
        Vec::new(),
        stream,
    )
    .unwrap()
}

#[cfg(debug_assertions)]
fn serve_governed_response(
    content_type: &'static str,
    body: &'static [u8],
) -> std::net::SocketAddr {
    serve_governed_status("200 OK", content_type, body)
}

#[cfg(debug_assertions)]
fn serve_governed_status(
    status: &'static str,
    content_type: &'static str,
    body: &'static [u8],
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).unwrap();
        write!(
            socket,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
    });
    address
}

#[cfg(debug_assertions)]
fn observed_client(address: std::net::SocketAddr) -> OpenAiCompatibleClient {
    OpenAiCompatibleClient::for_connection_with_debug_buffer_metadata(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
        &DEBUG_BUFFER_SINK,
    )
    .unwrap()
}

#[cfg(debug_assertions)]
fn observed_governed_client(address: std::net::SocketAddr) -> OpenAiCompatibleClient {
    OpenAiCompatibleClient::for_governed_connection_with_debug_buffer_metadata(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        &DEBUG_BUFFER_SINK,
    )
    .unwrap()
}

#[cfg(debug_assertions)]
fn observed_client_with_timeouts(
    address: std::net::SocketAddr,
    request_total: Duration,
    read_idle: Duration,
    sse_idle: Duration,
    sse_total: Duration,
) -> OpenAiCompatibleClient {
    OpenAiCompatibleClient::for_connection_with_debug_buffer_metadata_and_timeouts(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
        (request_total, read_idle, sse_idle, sse_total),
        &DEBUG_BUFFER_SINK,
    )
    .unwrap()
}

#[cfg(debug_assertions)]
fn serve_governed_stall(
    content_type: &'static str,
    chunks: Vec<(&'static [u8], Duration)>,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).unwrap();
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .unwrap();
        for (chunk, pause_after) in chunks {
            if write!(socket, "{:X}\r\n", chunk.len()).is_err()
                || socket.write_all(chunk).is_err()
                || socket.write_all(b"\r\n").is_err()
                || socket.flush().is_err()
            {
                return;
            }
            std::thread::sleep(pause_after);
        }
        let _ = socket.write_all(b"0\r\n\r\n");
    });
    address
}

#[cfg(debug_assertions)]
fn serve_governed_chunked_limit() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        for chunk in [vec![b'x'; 700_000], vec![b'y'; 400_000]] {
            write!(socket, "{:X}\r\n", chunk.len()).unwrap();
            socket.write_all(&chunk).unwrap();
            socket.write_all(b"\r\n").unwrap();
        }
        let _ = socket.write_all(b"0\r\n\r\n");
    });
    address
}

fn serve_three_requests(listener: TcpListener) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        let mut received = Vec::new();
        for body in [
            r#"{"data":[{"id":"model-a"}]}"#,
            r#"{"choices":[{"message":{"content":"chat"}}]}"#,
            r#"{"data":[{"index":0,"embedding":[0.25,0.5]}]}"#,
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 8192];
            let count = socket.read(&mut request).unwrap();
            received.push(String::from_utf8_lossy(&request[..count]).into_owned());
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        received
    })
}

fn serve_q1_chat_requests(listener: TcpListener) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        let mut received = Vec::new();
        for (content_type, body) in [
            (
                "application/json",
                r#"{"id":"nonstream","model":"q1-model","choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"vestrace_probe","arguments":"{\"value\":\"nonce\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            ),
            (
                "text/event-stream",
                "data: {\"id\":\"stream-1\",\"model\":\"q1-model\",\"choices\":[{\"delta\":{\"content\":\"VESTRACE_\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"stream-1\",\"model\":\"q1-model\",\"choices\":[{\"delta\":{\"content\":\"Q1\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"stream-1\",\"model\":\"q1-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2,\"total_tokens\":9}}\n\ndata: [DONE]\n\n",
            ),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut chunk = [0_u8; 1024];
                let count = socket.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..count]);
                if let Some(position) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while request.len() < header_end + content_length {
                let mut chunk = [0_u8; 1024];
                let count = socket.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..count]);
            }
            received.push(String::from_utf8(request).unwrap());
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        received
    })
}

#[tokio::test]
async fn typed_methods_join_a_fixed_prefix_and_emit_no_unauthorized_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let received = serve_three_requests(listener);
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/fixed/v1/"),
        ConnectionAuth::None,
    )
    .unwrap();

    assert_eq!(client.egress().destination(), DataDestination::LocalModel);
    let models = client.models_list(ModelsListRequest::new()).await.unwrap();
    let chat = client
        .chat_completions(ChatCompletionsRequest::single_user_message(
            "chat-model",
            "hello",
        ))
        .await
        .unwrap();
    let embeddings = client
        .embeddings(EmbeddingsRequest::new("embed-model", vec!["one".into()]))
        .await
        .unwrap();

    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "model-a");
    assert_eq!(chat.content, "chat");
    assert_eq!(embeddings.data.len(), 1);
    assert_eq!(embeddings.data[0].index, 0);
    assert_eq!(embeddings.data[0].embedding, vec![0.25, 0.5]);

    let requests = received.join().unwrap();
    assert!(requests[0].starts_with("GET /fixed/v1/models HTTP/1.1"));
    assert!(requests[1].starts_with("POST /fixed/v1/chat/completions HTTP/1.1"));
    assert!(requests[2].starts_with("POST /fixed/v1/embeddings HTTP/1.1"));
    assert!(requests[2].contains("\"encoding_format\":\"float\""));
    assert!(!requests[2].contains("\"dimensions\""));
    let chat_body: serde_json::Value =
        serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(chat_body["model"], "chat-model");
    assert_eq!(chat_body["messages"][0]["role"], "user");
    assert_eq!(chat_body["messages"][0]["content"], "hello");
    let embedding_body: serde_json::Value =
        serde_json::from_str(requests[2].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(embedding_body["model"], "embed-model");
    assert_eq!(embedding_body["input"], serde_json::json!(["one"]));
    for request in requests {
        let lower = request.to_ascii_lowercase();
        assert!(!lower.contains("authorization:"));
        assert!(!lower.contains("api-key:"));
        assert!(!lower.contains("x-api-key:"));
    }
}

#[tokio::test]
async fn one_chat_method_preserves_exact_q1_json_and_returns_typed_json_or_sse() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let received = serve_q1_chat_requests(listener);
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/fixed/v1"),
        ConnectionAuth::None,
    )
    .unwrap();
    let nonstream_payload = serde_json::json!({
        "model": "q1-model",
        "messages": [
            {"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"vestrace_probe","arguments":"{\"value\":\"nonce\"}"}}]},
            {"role":"tool","tool_call_id":"call-1","content":"VESTRACE_Q1_TOOL_RESULT_nonce"},
            {"role":"user","content":[{"type":"text","text":"Return marker"},{"type":"image_url","image_url":{"url":"data:image/png;base64,marker"}}]}
        ],
        "tools": [{"type":"function","function":{"name":"vestrace_probe","parameters":{"type":"object","additionalProperties":false}}}],
        "tool_choice": "required",
        "parallel_tool_calls": true,
        "response_format": {"type":"json_schema","json_schema":{"name":"q1","strict":true,"schema":{"type":"object"}}},
        "stream": false
    });
    let nonstream = client
        .chat_completions(
            ChatCompletionsRequest::from_json_object(nonstream_payload.clone()).unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(nonstream.output, ChatCompletionsOutput::Json(_)));

    let stream_payload = serde_json::json!({
        "model":"q1-model",
        "messages":[{"role":"user","content":"Return exactly VESTRACE_Q1"}],
        "stream":true,
        "stream_options":{"include_usage":true},
        "temperature":0,
        "max_tokens":32
    });
    let stream = client
        .chat_completions(ChatCompletionsRequest::from_json_object(stream_payload.clone()).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        stream.output,
        ChatCompletionsOutput::Sse(ref events)
            if matches!(events.last(), Some(ChatSseEvent::Done))
    ));
    assert_eq!(
        stream.usage,
        ProviderUsage::Known {
            prompt_tokens: 7,
            completion_tokens: 2,
        }
    );

    let wire = received.join().unwrap();
    let first: serde_json::Value =
        serde_json::from_str(wire[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    let second: serde_json::Value =
        serde_json::from_str(wire[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(first, nonstream_payload);
    assert_eq!(second, stream_payload);
}

#[tokio::test]
async fn governed_chat_returns_only_bounded_zeroizing_content_and_closed_evidence() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).unwrap();
        let body = r#"{"choices":[{"message":{"content":"retained sentinel"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":3}}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();
    let request = EffectiveModelRequest::chat_completions(
        "chat-model",
        vec![EffectiveChatMessage::user("hello")],
        EffectiveSampling::new(0.0, 1.0).unwrap(),
        EffectiveRequestLimits::new(32, 1, 32).unwrap(),
        Vec::new(),
        false,
    )
    .unwrap();

    let result = client.execute_effective(request).await.unwrap();
    let EffectiveModelResponse::ChatCompletions(result) = result else {
        panic!("governed chat returned a non-chat response");
    };
    assert_eq!(result.content(), "retained sentinel");
    assert_eq!(
        result.evidence(),
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop)
    );
    assert_eq!(
        result.usage(),
        &ProviderUsage::Known {
            prompt_tokens: 7,
            completion_tokens: 3,
        }
    );
    let content = result.into_content();
    assert_eq!(content.as_str(), "retained sentinel");
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
#[cfg(debug_assertions)]
async fn governed_json_and_sse_owned_allocations_zeroize_on_success_and_malformed_exit() {
    let _guard = BUFFER_TEST_GUARD.lock().await;

    reset_buffer_observer();
    let address = serve_governed_response(
        "application/json",
        br#"{"choices":[{"message":{"content":"json allocation sentinel"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":3}}"#,
    );
    let response = observed_client(address)
        .execute_effective(effective_chat_request(false))
        .await
        .unwrap();
    let EffectiveModelResponse::ChatCompletions(result) = response else {
        panic!("governed JSON returned a non-chat response");
    };
    assert_zeroed(&JSON_BODY_OBSERVED, &JSON_BODY_ZERO, &JSON_BODY_LEN);
    let content_metadata = DEBUG_BUFFER_SINK.metadata(DebugGovernedBufferKind::Content);
    assert_eq!(content_metadata.pointer, result.content().as_ptr() as usize);
    assert_eq!(content_metadata.initialized_len, result.content().len());
    assert!(!CONTENT_OBSERVED.load(Ordering::SeqCst));
    drop(result);
    assert_zeroed(&CONTENT_OBSERVED, &CONTENT_ZERO, &CONTENT_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_response(
        "application/json",
        br#"{"choices":[{"message":{"content":"malformed json sentinel"},"finish_reason":17}]}"#,
    );
    let error = match observed_client(address)
        .execute_effective(effective_chat_request(false))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("malformed governed JSON unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::InvalidResponse(_)));
    assert_zeroed(&JSON_BODY_OBSERVED, &JSON_BODY_ZERO, &JSON_BODY_LEN);
    assert_zeroed(&CONTENT_OBSERVED, &CONTENT_ZERO, &CONTENT_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_response(
        "text/event-stream",
        b"data: {\"choices\":[{\"delta\":{\"content\":\"sse allocation sentinel\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
    );
    let response = observed_client(address)
        .execute_effective(effective_chat_request(true))
        .await
        .unwrap();
    let EffectiveModelResponse::ChatCompletions(result) = response else {
        panic!("governed SSE returned a non-chat response");
    };
    assert_zeroed(&SSE_BODY_OBSERVED, &SSE_BODY_ZERO, &SSE_BODY_LEN);
    assert_zeroed(&PARSER_OBSERVED, &PARSER_ZERO, &PARSER_LEN);
    assert_eq!(
        DEBUG_BUFFER_SINK
            .metadata(DebugGovernedBufferKind::Content)
            .pointer,
        result.content().as_ptr() as usize
    );
    drop(result);
    assert_zeroed(&CONTENT_OBSERVED, &CONTENT_ZERO, &CONTENT_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_response(
        "text/event-stream",
        b"data: {\"choices\":[{\"delta\":{\"content\":\"late malformed sentinel\"},\"finish_reason\":null}]}\n\ndata: {not-json}\n\n",
    );
    let error = match observed_client(address)
        .execute_effective(effective_chat_request(true))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("malformed governed SSE unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::InvalidResponse(_)));
    assert_zeroed(&SSE_BODY_OBSERVED, &SSE_BODY_ZERO, &SSE_BODY_LEN);
    assert_zeroed(&PARSER_OBSERVED, &PARSER_ZERO, &PARSER_LEN);
    assert_zeroed(&CONTENT_OBSERVED, &CONTENT_ZERO, &CONTENT_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let error = match observed_client(serve_governed_chunked_limit())
        .execute_effective(effective_chat_request(false))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("over-limit governed JSON unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::InvalidResponse(_)));
    assert_zeroed(&JSON_BODY_OBSERVED, &JSON_BODY_ZERO, &JSON_BODY_LEN);
    assert!(!CONTENT_OBSERVED.load(Ordering::SeqCst));
    DEBUG_BUFFER_SINK.end();
}

#[tokio::test(flavor = "current_thread")]
#[cfg(debug_assertions)]
async fn governed_auth_header_allocation_zeroizes_after_success_refusal_and_transport_error() {
    let _guard = BUFFER_TEST_GUARD.lock().await;

    reset_buffer_observer();
    observed_governed_client(serve_governed_response(
        "application/json",
        br#"{"data":[]}"#,
    ))
    .execute_effective_once(
        ConnectionAuth::Bearer(ProviderCredential::new("success-auth-sentinel")),
        EffectiveModelRequest::models_list(),
    )
    .await
    .unwrap();
    assert_zeroed(&AUTH_HEADER_OBSERVED, &AUTH_HEADER_ZERO, &AUTH_HEADER_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let refusal = observed_governed_client(serve_governed_status(
        "401 Unauthorized",
        "application/json",
        br#"{"error":{"message":"refused"}}"#,
    ))
    .execute_effective_once(
        ConnectionAuth::ApiKey(ProviderCredential::new("refusal-auth-sentinel")),
        EffectiveModelRequest::models_list(),
    )
    .await;
    assert!(matches!(refusal, Err(ProviderError::CredentialRejected(_))));
    assert_zeroed(&AUTH_HEADER_OBSERVED, &AUTH_HEADER_ZERO, &AUTH_HEADER_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap();
    drop(reserved);
    let transport = observed_governed_client(address)
        .execute_effective_once(
            ConnectionAuth::XApiKey(ProviderCredential::new("transport-auth-sentinel")),
            EffectiveModelRequest::models_list(),
        )
        .await;
    assert!(matches!(transport, Err(ProviderError::Unavailable(_))));
    assert_zeroed(&AUTH_HEADER_OBSERVED, &AUTH_HEADER_ZERO, &AUTH_HEADER_LEN);
    DEBUG_BUFFER_SINK.end();
}

#[tokio::test(flavor = "current_thread")]
#[cfg(debug_assertions)]
async fn governed_json_and_sse_timeout_allocations_zeroize_on_idle_and_total_exit() {
    let _guard = BUFFER_TEST_GUARD.lock().await;

    reset_buffer_observer();
    let address = serve_governed_stall(
        "application/json",
        vec![(
            br#"{"choices":[{"message":{"content":"json idle sentinel"}"#,
            Duration::from_millis(160),
        )],
    );
    let error = match observed_client_with_timeouts(
        address,
        Duration::from_millis(500),
        Duration::from_millis(40),
        Duration::from_millis(400),
        Duration::from_millis(450),
    )
    .execute_effective(effective_chat_request(false))
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("governed JSON idle timeout unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::Timeout));
    assert_zeroed(&JSON_BODY_OBSERVED, &JSON_BODY_ZERO, &JSON_BODY_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_stall(
        "application/json",
        vec![
            (br#"{"choices":["#, Duration::from_millis(35)),
            (
                br#"{"message":{"content":"json total"}}"#,
                Duration::from_millis(35),
            ),
            (
                br#"],"usage":{"prompt_tokens":1"#,
                Duration::from_millis(35),
            ),
            (br#", "completion_tokens":1}}"#, Duration::from_millis(35)),
        ],
    );
    let error = match observed_client_with_timeouts(
        address,
        Duration::from_millis(85),
        Duration::from_millis(250),
        Duration::from_millis(250),
        Duration::from_millis(300),
    )
    .execute_effective(effective_chat_request(false))
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("governed JSON total timeout unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::Timeout));
    assert_zeroed(&JSON_BODY_OBSERVED, &JSON_BODY_ZERO, &JSON_BODY_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_stall(
        "text/event-stream",
        vec![(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"sse idle sentinel\"},\"finish_reason\":null}]}\n\n",
            Duration::from_millis(160),
        )],
    );
    let error = match observed_client_with_timeouts(
        address,
        Duration::from_millis(500),
        Duration::from_millis(400),
        Duration::from_millis(40),
        Duration::from_millis(450),
    )
    .execute_effective(effective_chat_request(true))
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("governed SSE idle timeout unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::Timeout));
    assert_zeroed(&SSE_BODY_OBSERVED, &SSE_BODY_ZERO, &SSE_BODY_LEN);
    DEBUG_BUFFER_SINK.end();

    reset_buffer_observer();
    let address = serve_governed_stall(
        "text/event-stream",
        vec![
            (
                b"data: {\"choices\":[{\"delta\":{\"content\":\"sse\"},\"finish_reason\":null}]}\n\n",
                Duration::from_millis(35),
            ),
            (
                b"data: {\"choices\":[{\"delta\":{\"content\":\" total\"},\"finish_reason\":null}]}\n\n",
                Duration::from_millis(35),
            ),
            (
                b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                Duration::from_millis(35),
            ),
        ],
    );
    let error = match observed_client_with_timeouts(
        address,
        Duration::from_millis(500),
        Duration::from_millis(250),
        Duration::from_millis(100),
        Duration::from_millis(80),
    )
    .execute_effective(effective_chat_request(true))
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("governed SSE total timeout unexpectedly succeeded"),
    };
    assert!(matches!(error, ProviderError::Timeout));
    assert_zeroed(&SSE_BODY_OBSERVED, &SSE_BODY_ZERO, &SSE_BODY_LEN);
    DEBUG_BUFFER_SINK.end();
}

#[test]
fn governed_buffer_observer_is_debug_only_and_passive() {
    let source = include_str!("../src/providers/openai_compatible.rs");
    assert!(source.contains("#[cfg(debug_assertions)]\n#[doc(hidden)]"));
    assert!(source.contains("for_connection_with_debug_buffer_metadata"));
    assert!(source.contains("DebugGovernedBufferMetadataSink"));
    assert!(!source.contains("Fn(DebugGovernedBufferMetadata"));
    assert!(!source.contains("dyn DebugGovernedBuffer"));
    assert!(source.contains("#![forbid(unsafe_code)]") || !source.contains("unsafe "));
}

#[test]
fn release_build_cannot_import_governed_buffer_observer_api() {
    let temporary = tempfile::tempdir().unwrap();
    let source_directory = temporary.path().join("src");
    std::fs::create_dir(&source_directory).unwrap();
    let infrastructure = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let application = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vestrace-application")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    std::fs::write(
        temporary.path().join("Cargo.toml"),
        format!(
            "[package]\nname='release-observer-negative-probe'\nversion='0.0.0'\nedition='2024'\n\n[dependencies]\nvestrace-infrastructure={{path='{infrastructure}'}}\nvestrace-application={{path='{application}'}}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        source_directory.join("lib.rs"),
        r#"
use vestrace_application::{ConnectionAuth, ConnectionKind};
use vestrace_infrastructure::OpenAiCompatibleClient;
use vestrace_infrastructure::providers::openai_compatible::{
    DebugGovernedBufferKind, DebugGovernedBufferMetadataSink,
};

pub fn forbidden_release_probe() {
    let sink = Box::leak(Box::new(DebugGovernedBufferMetadataSink::new()));
    let _kind = DebugGovernedBufferKind::JsonBody;
    let _client = OpenAiCompatibleClient::for_connection_with_debug_buffer_metadata(
        ConnectionKind::LMStudioLocal,
        "http://127.0.0.1:12345/v1",
        ConnectionAuth::None,
        sink,
    );
}
"#,
    )
    .unwrap();
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .arg("check")
            .arg("--release")
            .arg("--offline")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(temporary.path().join("Cargo.toml"))
            .arg("--target-dir")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .unwrap();
    assert!(
        !output.status.success(),
        "release build unexpectedly exposed the governed buffer observer API"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DebugGovernedBufferKind")
            && stderr.contains("DebugGovernedBufferMetadataSink")
            && stderr.contains("for_connection_with_debug_buffer_metadata"),
        "release negative probe failed for an unrelated reason: {stderr}"
    );
}

#[test]
fn remote_profiles_are_refused_before_any_connection_for_http_or_forbidden_addresses() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    for endpoint in [
        format!("http://{address}/v1"),
        format!("https://{address}/v1"),
        "https://10.0.0.1/v1".into(),
        "https://169.254.169.254/v1".into(),
        "https://0.0.0.0/v1".into(),
        "https://224.0.0.1/v1".into(),
        "https://[::]/v1".into(),
        "https://user:password@api.example.test/v1".into(),
        "https://api.example.test/v1?query=value".into(),
        "https://api.example.test/v1#fragment".into(),
    ] {
        let error = OpenAiCompatibleClient::for_connection(
            ConnectionKind::OpenAiChatCompletionsV1,
            endpoint,
            ConnectionAuth::None,
        )
        .unwrap_err();
        assert!(
            matches!(error, ProviderError::Unavailable(_)),
            "unexpected error: {error:?}"
        );
        assert!(!format!("{error}").contains("password"));
    }

    listener.set_nonblocking(true).unwrap();
    assert!(
        listener.accept().is_err(),
        "remote profile opened a connection before refusal"
    );
}

#[test]
fn lm_studio_http_is_the_only_http_exception_and_non_loopback_hosts_are_not_local() {
    let local = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        "http://127.0.0.1:12345/v1",
        ConnectionAuth::None,
    )
    .unwrap();
    assert_eq!(local.egress().destination(), DataDestination::LocalModel);

    let host_bridge = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        "http://192.168.65.2:12345/v1",
        ConnectionAuth::None,
    )
    .unwrap();
    assert_eq!(
        host_bridge.egress().destination(),
        DataDestination::RemoteProvider
    );
}

#[test]
fn legacy_https_kind_inference_is_case_insensitive_and_keeps_remote_peer_policy() {
    let error = OpenAiCompatibleClient::new("HTTPS://127.0.0.1:12345/v1", None).unwrap_err();

    assert!(matches!(error, ProviderError::Unavailable(_)));
}

#[tokio::test]
async fn redirect_and_raw_body_are_not_followed_or_retained() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let received = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 8192];
        let count = socket.read(&mut request).unwrap();
        socket
            .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        String::from_utf8_lossy(&request[..count]).into_owned()
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::Bearer(ProviderCredential::new("credential-never-on-error")),
    )
    .unwrap();

    let error = client
        .chat_completions(ChatCompletionsRequest::single_user_message(
            "model",
            "safe input",
        ))
        .await
        .unwrap_err();
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("credential-never-on-error"));
    assert!(!rendered.contains("safe input"));
    assert!(!rendered.contains("collect"));
    assert!(received.join().unwrap().contains("safe input"));
}

#[tokio::test]
async fn provider_errors_retain_only_bounded_safe_fields() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 8192];
        let _ = socket.read(&mut request).unwrap();
        let long_code = "c".repeat(140);
        let body = format!(
            r#"{{"error":{{"message":"raw-body-secret","code":"{long_code}","type":"invalid_request"}},"fingerprint":"must-not-persist"}}"#
        );
        write!(
            socket,
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nX-Request-Id: request-allowed-123\r\nX-Unsafe-Trace: must-not-persist\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();

    let error = client
        .chat_completions(ChatCompletionsRequest::single_user_message(
            "model",
            "request-body-secret",
        ))
        .await
        .unwrap_err();
    server.join().unwrap();
    let rendered = format!("{error:?} {error}");

    assert!(rendered.contains("status=400"));
    assert!(rendered.contains("type=invalid_request"));
    assert!(rendered.contains("correlation_id=request-allowed-123"));
    assert!(rendered.contains(&format!("code={}", "c".repeat(96))));
    let unbounded_code = "c".repeat(140);
    for forbidden in [
        "raw-body-secret",
        "request-body-secret",
        "must-not-persist",
        unbounded_code.as_str(),
    ] {
        assert!(!rendered.contains(forbidden));
    }
}

#[tokio::test]
async fn oversized_responses_are_refused_without_retaining_exact_size() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 8192];
        let _ = socket.read(&mut request).unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/v1"),
        ConnectionAuth::None,
    )
    .unwrap();

    let error = client
        .models_list(ModelsListRequest::new())
        .await
        .unwrap_err();
    server.join().unwrap();
    let rendered = format!("{error:?} {error}");

    assert!(rendered.contains("configured byte limit"));
    assert!(!rendered.contains("1048577"));
}
