use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use vestrace_application::{GenerationRequest, TextGenerationProvider};
use vestrace_infrastructure::OpenAiCompatibleClient;

#[tokio::test]
async fn ambient_proxy_variables_cannot_intercept_a_loopback_completion() {
    let provider = TcpListener::bind("127.0.0.1:0").unwrap();
    let provider_address = provider.local_addr().unwrap();
    let provider_request = std::thread::spawn(move || {
        let (mut socket, _) = provider.accept().unwrap();
        let mut request = [0_u8; 8192];
        let count = socket.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..count]).into_owned();
        let body = r#"{"choices":[{"message":{"content":"direct"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });

    // Keep the port bound without accepting. A client that honors the ambient
    // proxy connects here and then times out; a client that refused ambient
    // proxy configuration reaches the provider listener above immediately.
    let black_hole_proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://{}", black_hole_proxy.local_addr().unwrap());
    // This integration test has its own process, so the temporary process
    // environment cannot race provider unit tests. Reqwest snapshots proxy
    // environment while the client is built; it does not need to remain set
    // while the asynchronous request runs.
    let client = temp_env::with_vars(
        [
            ("HTTP_PROXY", Some(proxy_url.as_str())),
            ("HTTPS_PROXY", Some(proxy_url.as_str())),
            ("ALL_PROXY", Some(proxy_url.as_str())),
            ("http_proxy", Some(proxy_url.as_str())),
            ("https_proxy", Some(proxy_url.as_str())),
            ("all_proxy", Some(proxy_url.as_str())),
            ("NO_PROXY", None),
            ("no_proxy", None),
        ],
        || {
            OpenAiCompatibleClient::with_timeout(
                format!("http://{provider_address}/v1"),
                None,
                Duration::from_secs(2),
            )
            .unwrap()
        },
    );

    let response = client
        .generate(GenerationRequest {
            model: "proxy-test".into(),
            prompt: "must reach loopback directly".into(),
            max_tokens: Some(1),
        })
        .await
        .unwrap();

    assert_eq!(response.content, "direct");
    let request = provider_request.join().unwrap();
    assert!(request.contains("must reach loopback directly"));
}
