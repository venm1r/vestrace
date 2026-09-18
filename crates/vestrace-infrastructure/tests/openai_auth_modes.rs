use std::io::{Read, Write};
use std::net::TcpListener;

use vestrace_application::{
    ChatCompletionsRequest, ConnectionAuth, ConnectionKind, EffectiveModelRequest,
    ProviderCredential, ProviderUsage,
};
use vestrace_infrastructure::OpenAiCompatibleClient;

fn receive_request(listener: TcpListener) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0_u8; 1024];
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0, "client closed before sending headers");
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
            assert!(count > 0, "client closed before sending body");
            request.extend_from_slice(&chunk[..count]);
        }
        let body = r#"{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    })
}

fn receive_requests(
    listener: TcpListener,
    statuses: Vec<&'static str>,
) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        statuses
            .into_iter()
            .map(|status| {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut chunk = [0_u8; 1024];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(count > 0, "client closed before sending headers");
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
                    assert!(count > 0, "client closed before sending body");
                    request.extend_from_slice(&chunk[..count]);
                }
                let body = if status == "200 OK" {
                    r#"{"data":[]}"#
                } else {
                    r#"{"error":{"message":"refused"}}"#
                };
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            })
            .collect()
    })
}

fn request() -> ChatCompletionsRequest {
    ChatCompletionsRequest::single_user_message("test-model", "only the canonical auth header")
}

#[tokio::test]
async fn each_closed_auth_mode_emits_only_its_canonical_header() {
    let cases = [
        (ConnectionAuth::None, None),
        (
            ConnectionAuth::Bearer(ProviderCredential::new("bearer-secret")),
            Some(("authorization", "Bearer bearer-secret")),
        ),
        (
            ConnectionAuth::ApiKey(ProviderCredential::new("api-key-secret")),
            Some(("api-key", "api-key-secret")),
        ),
        (
            ConnectionAuth::XApiKey(ProviderCredential::new("x-api-key-secret")),
            Some(("x-api-key", "x-api-key-secret")),
        ),
    ];

    for (auth, expected) in cases {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let received = receive_request(listener);
        let client = OpenAiCompatibleClient::for_connection(
            ConnectionKind::LMStudioLocal,
            format!("http://{address}/fixed/v1"),
            auth,
        )
        .unwrap();

        let response = client.chat_completions(request()).await.unwrap();
        assert_eq!(
            response.usage,
            ProviderUsage::Known {
                prompt_tokens: 3,
                completion_tokens: 2,
            }
        );

        let wire = received.join().unwrap();
        let header_value = |wanted: &str| {
            wire.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case(wanted)
                    .then(|| value.trim().to_string())
            })
        };
        for name in ["authorization", "api-key", "x-api-key"] {
            let actual = header_value(name);
            let expected_value = expected
                .filter(|(expected_name, _)| expected_name.eq_ignore_ascii_case(name))
                .map(|(_, value)| value);
            assert!(
                actual.as_deref() == expected_value,
                "closed auth mapping emitted a missing, duplicate, or non-canonical header"
            );
        }
    }
}

#[tokio::test]
async fn governed_auth_is_consumed_per_successful_call_and_never_retained_by_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let received = receive_requests(listener, vec!["200 OK", "200 OK"]);
    let client = OpenAiCompatibleClient::for_governed_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/fixed/v1"),
    )
    .unwrap();

    client
        .execute_effective_once(
            ConnectionAuth::Bearer(ProviderCredential::new("single-call-secret")),
            EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();
    client
        .execute_effective(EffectiveModelRequest::models_list())
        .await
        .unwrap();

    let received = received.join().unwrap();
    assert!(received[0].lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization")
                && value.trim() == "Bearer single-call-secret"
        })
    }));
    assert!(
        !received[1]
            .lines()
            .any(|line| line.to_ascii_lowercase().starts_with("authorization:")),
        "a successful governed request retained its credential on the reusable transport"
    );
}

#[tokio::test]
async fn governed_auth_is_consumed_when_the_provider_refuses_the_call() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let received = receive_requests(listener, vec!["401 Unauthorized", "200 OK"]);
    let client = OpenAiCompatibleClient::for_governed_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/fixed/v1"),
    )
    .unwrap();

    let refusal = match client
        .execute_effective_once(
            ConnectionAuth::ApiKey(ProviderCredential::new("refused-once-secret")),
            EffectiveModelRequest::models_list(),
        )
        .await
    {
        Ok(_) => panic!("a 401 must be classified as a credential refusal"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        vestrace_application::ProviderError::CredentialRejected(_)
    ));
    client
        .execute_effective(EffectiveModelRequest::models_list())
        .await
        .unwrap();

    let received = received.join().unwrap();
    assert!(received[0].lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("api-key") && value.trim() == "refused-once-secret"
        })
    }));
    assert!(
        !received[1]
            .lines()
            .any(|line| line.to_ascii_lowercase().starts_with("api-key:")),
        "a refused governed request retained its credential on the reusable transport"
    );
}

#[tokio::test]
async fn governed_auth_is_consumed_when_transport_fails_before_a_response() {
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap();
    drop(reserved);
    let client = OpenAiCompatibleClient::for_governed_connection(
        ConnectionKind::LMStudioLocal,
        format!("http://{address}/fixed/v1"),
    )
    .unwrap();

    let failure = match client
        .execute_effective_once(
            ConnectionAuth::XApiKey(ProviderCredential::new("transport-once-secret")),
            EffectiveModelRequest::models_list(),
        )
        .await
    {
        Ok(_) => panic!("an unbound loopback port must fail transport"),
        Err(error) => error,
    };
    assert!(matches!(
        failure,
        vestrace_application::ProviderError::Unavailable(_)
    ));

    let listener = TcpListener::bind(address).unwrap();
    let received = receive_requests(listener, vec!["200 OK"]);
    client
        .execute_effective(EffectiveModelRequest::models_list())
        .await
        .unwrap();

    let received = received.join().unwrap();
    assert!(
        !received[0]
            .lines()
            .any(|line| line.to_ascii_lowercase().starts_with("x-api-key:")),
        "a transport failure retained its governed credential on the reusable transport"
    );
}

#[test]
fn credentials_cannot_enter_debug_or_provider_errors() {
    let secret = "never-render-this-credential";
    let client = OpenAiCompatibleClient::for_connection(
        ConnectionKind::LMStudioLocal,
        "http://127.0.0.1:12345/v1",
        ConnectionAuth::Bearer(ProviderCredential::new(secret)),
    )
    .unwrap();

    assert!(!format!("{client:?}").contains(secret));
    assert!(!format!("{:?}", client.safe_error_for_status(400)).contains(secret));
}

#[test]
fn empty_credentials_are_refused_for_every_credential_auth_mode() {
    for auth in [
        ConnectionAuth::Bearer(ProviderCredential::new("")),
        ConnectionAuth::ApiKey(ProviderCredential::new("")),
        ConnectionAuth::XApiKey(ProviderCredential::new("")),
    ] {
        let error = OpenAiCompatibleClient::for_connection(
            ConnectionKind::LMStudioLocal,
            "http://127.0.0.1:12345/v1",
            auth,
        )
        .unwrap_err();
        assert!(error.to_string().contains("must not be empty"));
    }
}
