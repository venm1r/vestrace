use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use vestrace_application::retrieval::EmbeddingProvider;
use vestrace_infrastructure::OpenAiCompatibleEmbeddingClient;

#[tokio::test]
async fn ambient_proxy_variables_cannot_intercept_a_loopback_embedding() {
    let provider = TcpListener::bind("127.0.0.1:0").unwrap();
    let provider_address = provider.local_addr().unwrap();
    let provider_request = std::thread::spawn(move || {
        let (mut socket, _) = provider.accept().unwrap();
        let mut request = [0_u8; 8192];
        let count = socket.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..count]).into_owned();
        let body = r#"{"data":[{"embedding":[0.25,-0.5],"index":0}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });

    // Keep the port bound without accepting. Honouring an ambient proxy makes
    // this request time out here instead of reaching the configured provider.
    let black_hole_proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://{}", black_hole_proxy.local_addr().unwrap());
    // This integration test has its own process, so its temporary process
    // environment cannot race provider unit tests. Reqwest snapshots proxy
    // environment while the client is built.
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
            OpenAiCompatibleEmbeddingClient::with_timeout(
                format!("http://{provider_address}/v1"),
                "proxy-test",
                None,
                Duration::from_secs(2),
            )
            .unwrap()
        },
    );

    let response = client
        .embed(&["must reach loopback directly".into()])
        .await
        .unwrap();

    assert_eq!(response, vec![vec![0.25, -0.5]]);
    let request = provider_request.join().unwrap();
    assert!(request.contains("must reach loopback directly"));
}
