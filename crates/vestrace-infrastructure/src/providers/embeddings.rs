use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use vestrace_application::retrieval::EmbeddingProvider;
use vestrace_application::{ApplicationError, ProviderEgress};

use super::egress::destination_for_endpoint;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Embeddings from any OpenAI-compatible `/v1/embeddings` endpoint.
///
/// The same shape serves LM Studio, Ollama, NVIDIA NIM and OpenAI itself, which
/// is the point: the model that embeds a workspace's memories is a deployment
/// choice, and the space records which one it was so a later change cannot go
/// unnoticed.
pub struct OpenAiCompatibleEmbeddingClient {
    client: Client,
    egress: ProviderEgress,
    model: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatibleEmbeddingClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleEmbeddingClient")
            .field("egress", &self.egress)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl OpenAiCompatibleEmbeddingClient {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Result<Self, ApplicationError> {
        Self::with_timeout(base_url, model, api_key, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ApplicationError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let model = model.into();
        if base_url.trim().is_empty() || model.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "embedding base URL and model must not be empty".into(),
            ));
        }
        let parsed = reqwest::Url::parse(&base_url).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "embedding provider endpoint {base_url:?} is not a valid URL: {error}"
            ))
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "embedding provider endpoint {base_url:?} must use http or https"
            )));
        }

        // No fallback to `Client::default()`: that produces a client with no
        // timeout, which is the one property this builder exists to set. The
        // predecessor of this file had exactly that bug.
        //
        // Redirects and ambient system proxies are refused because the
        // configured endpoint must be the whole embedding egress path. Memory
        // contents and credentials must not acquire a second destination from
        // an HTTP response or process environment.
        let redirects_disabled = true;
        let proxy_disabled = true;
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| {
                ApplicationError::InvalidConfiguration(format!(
                    "embedding HTTP client could not be built: {error}"
                ))
            })?;

        let destination = destination_for_endpoint(&parsed, redirects_disabled, proxy_disabled);

        Ok(Self {
            client,
            egress: ProviderEgress::new(
                format!("{base_url}/embeddings"),
                destination,
                redirects_disabled,
                proxy_disabled,
            ),
            model,
            api_key,
        })
    }

    pub fn egress(&self) -> &ProviderEgress {
        &self.egress
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    embedding: Vec<f32>,
    #[serde(default)]
    index: usize,
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatibleEmbeddingClient {
    fn model(&self) -> &str {
        &self.model
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .client
            .post(self.egress.endpoint())
            .header("content-type", "application/json")
            .bearer_auth(self.api_key.clone().unwrap_or_default())
            .json(&EmbeddingRequest {
                model: &self.model,
                input: inputs,
            })
            .send()
            .await
            .map_err(|error| {
                // A provider that cannot be reached is unavailable, not
                // misconfigured: the distinction decides whether an operator
                // checks the network or the settings.
                ApplicationError::Unavailable(format!(
                    "the embedding provider could not be reached: {error}"
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            // Each status means a different thing to whoever is paged, which is
            // why they are not collapsed into one error the way the completion
            // client's used to be.
            return Err(match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    ApplicationError::InvalidConfiguration(format!(
                        "the embedding provider rejected the credential for model {}; replace \
                         it rather than retrying",
                        self.model
                    ))
                }
                StatusCode::TOO_MANY_REQUESTS => ApplicationError::Unavailable(
                    "the embedding provider is rate limiting; retry later".into(),
                ),
                StatusCode::NOT_FOUND => ApplicationError::InvalidConfiguration(format!(
                    "the embedding provider does not serve model {}",
                    self.model
                )),
                other => ApplicationError::Unavailable(format!(
                    "the embedding provider answered {other}: {}",
                    body.chars().take(200).collect::<String>()
                )),
            });
        }

        let mut payload: EmbeddingResponse = response.json().await.map_err(|error| {
            ApplicationError::Unavailable(format!(
                "the embedding provider returned an unreadable body: {error}"
            ))
        })?;

        if payload.data.len() != inputs.len() {
            return Err(ApplicationError::Unavailable(format!(
                "the embedding provider returned {} vectors for {} inputs",
                payload.data.len(),
                inputs.len()
            )));
        }

        // The API permits results in any order and carries an index for exactly
        // that reason. Trusting arrival order would attach each vector to the
        // wrong memory, and every one of them would look plausible.
        payload.data.sort_by_key(|datum| datum.index);
        Ok(payload
            .data
            .into_iter()
            .map(|datum| datum.embedding)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use vestrace_domain::DataDestination;

    fn read_http_request(socket: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0_u8; 1024];
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0, "embedding client closed before sending headers");
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
            assert!(count > 0, "embedding client closed before sending its body");
            request.extend_from_slice(&chunk[..count]);
        }
        String::from_utf8(request).unwrap()
    }

    #[test]
    fn embedding_egress_uses_the_shared_destination_derivation() {
        let loopback =
            OpenAiCompatibleEmbeddingClient::new("http://[::1]:12345/v1", "embedding", None)
                .unwrap();
        assert_eq!(
            loopback.egress().endpoint(),
            "http://[::1]:12345/v1/embeddings"
        );
        assert_eq!(loopback.egress().destination(), DataDestination::LocalModel);
        assert!(loopback.egress().redirects_disabled());
        assert!(loopback.egress().proxy_disabled());

        for endpoint in [
            "http://10.0.0.4:12345/v1",
            "http://172.16.0.4:12345/v1",
            "http://192.168.0.4:12345/v1",
            "http://host.docker.internal:12345/v1",
        ] {
            let client = OpenAiCompatibleEmbeddingClient::new(endpoint, "embedding", None).unwrap();
            assert_eq!(
                client.egress().destination(),
                DataDestination::RemoteProvider,
                "{endpoint} was mistaken for a local model"
            );
        }
    }

    #[tokio::test]
    async fn an_embedding_redirect_to_a_remote_host_is_returned_to_the_caller() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let received = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let request = read_http_request(&mut socket);
            socket
                .write_all(
                    b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://192.0.2.1/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            request
        });

        let client = OpenAiCompatibleEmbeddingClient::with_timeout(
            format!("http://{address}/v1"),
            "embedding",
            Some("credential-that-must-not-be-forwarded".into()),
            Duration::from_secs(2),
        )
        .unwrap();
        let result = client
            .embed(&["memory-that-must-not-leave-loopback".into()])
            .await;

        assert!(
            matches!(result, Err(ApplicationError::Unavailable(ref message)) if message.contains("307 Temporary Redirect")),
            "redirect was not returned to the caller: {result:?}"
        );
        let request = received.join().unwrap();
        assert!(request.contains("memory-that-must-not-leave-loopback"));
    }
}
