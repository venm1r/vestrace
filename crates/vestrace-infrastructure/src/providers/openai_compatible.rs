use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use vestrace_application::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, GenerationRequest, GenerationResponse,
    ProviderError, TextGenerationProvider, TextGenerationProviderEgress,
};
use vestrace_domain::DataDestination;

/// How long to wait for a provider before treating it as unavailable.
///
/// Generous, because a large completion legitimately takes a while, but finite:
/// without a deadline a wedged provider holds a worker slot forever, and the
/// run it is executing never reaches a terminal state.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct OpenAiCompatibleClient {
    client: Client,
    egress: TextGenerationProviderEgress,
    base_url: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatibleClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicit rather than absent, so that adding `#[derive(Debug)]` to a
        // struct holding this client cannot start printing the key.
        formatter
            .debug_struct("OpenAiCompatibleClient")
            .field("egress", &self.egress)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl OpenAiCompatibleClient {
    /// Construct a client, or fail if no HTTP client can be built.
    ///
    /// Previously this fell back to `Client::default()` when the builder
    /// failed, which silently produced a client with **no timeout** — the one
    /// property the builder was there to set.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        Self::with_timeout(base_url, api_key, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(
        base_url: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let parsed = reqwest::Url::parse(&base_url).map_err(|error| {
            ProviderError::Unavailable(format!(
                "model provider endpoint {base_url:?} is not a valid URL: {error}"
            ))
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ProviderError::Unavailable(format!(
                "model provider endpoint {base_url:?} must use http or https"
            )));
        }

        // Redirects and ambient system proxies are both disabled. A request
        // classified as loopback must not acquire a second egress path after
        // the decision has been persisted, and API credentials must never be
        // forwarded to a redirect target.
        let redirects_disabled = true;
        let proxy_disabled = true;
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        let destination = destination_for_endpoint(&parsed, redirects_disabled, proxy_disabled);

        Ok(Self {
            client,
            egress: TextGenerationProviderEgress::new(
                format!("{base_url}/chat/completions"),
                destination,
                redirects_disabled,
                proxy_disabled,
            ),
            base_url,
            api_key,
        })
    }

    pub fn egress(&self) -> &TextGenerationProviderEgress {
        &self.egress
    }

    fn map_status(status: reqwest::StatusCode) -> ProviderError {
        // Distinguished because they call for different responses: a rate limit
        // is worth retrying, a 4xx is not.
        match status.as_u16() {
            429 => ProviderError::RateLimited,
            408 | 504 => ProviderError::Timeout,
            // Both, because providers disagree on which one a bad key gets:
            // NVIDIA answers 403, OpenAI 401. Neither is a malformed reply, and
            // reporting one as such told an operator with an expired key to go
            // and read the adapter.
            401 | 403 => ProviderError::CredentialRejected(format!("provider returned {status}")),
            500..=599 => ProviderError::Unavailable(format!("provider returned {status}")),
            // The body is not included: it echoes back request content, which
            // may be the very data a caller is careful about.
            _ => ProviderError::InvalidResponse(format!("provider returned {status}")),
        }
    }
}

fn destination_for_endpoint(
    endpoint: &reqwest::Url,
    redirects_disabled: bool,
    proxy_disabled: bool,
) -> DataDestination {
    use std::net::ToSocketAddrs;

    destination_for_endpoint_with_resolver(
        endpoint,
        redirects_disabled,
        proxy_disabled,
        |host, port| {
            (host, port)
                .to_socket_addrs()
                .map(|addresses| addresses.map(|address| address.ip()).collect())
        },
    )
}

fn destination_for_endpoint_with_resolver<F>(
    endpoint: &reqwest::Url,
    redirects_disabled: bool,
    proxy_disabled: bool,
    resolve: F,
) -> DataDestination
where
    F: FnOnce(&str, u16) -> std::io::Result<Vec<std::net::IpAddr>>,
{
    let loopback = endpoint.host_str().is_some_and(|host| {
        let unbracketed = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        match unbracketed.parse::<std::net::IpAddr>() {
            Ok(address) => address.is_loopback(),
            Err(_)
                if unbracketed.eq_ignore_ascii_case("localhost")
                    || unbracketed.eq_ignore_ascii_case("localhost.") =>
            {
                let Some(port) = endpoint.port_or_known_default() else {
                    return false;
                };
                // The name alone is not a locality proof: a hosts-file entry
                // can point `localhost` elsewhere. Refuse LocalModel when
                // resolution fails, yields nothing, or exposes even one
                // non-loopback route. DNS rebinding after this construction
                // check remains the separately stated non-goal in PLAN.md.
                resolve(unbracketed, port).is_ok_and(|addresses| {
                    !addresses.is_empty() && addresses.iter().all(|a| a.is_loopback())
                })
            }
            Err(_) => false,
        }
    });
    if loopback && redirects_disabled && proxy_disabled {
        DataDestination::LocalModel
    } else {
        DataDestination::RemoteProvider
    }
}

/// Read a token count that the accounting path depends on.
///
/// Absent usage is an error rather than zero. A silent zero would understate
/// consumption in a system that enforces budgets, so the budget would be
/// spent without ever appearing spent.
fn required_token_count(body: &serde_json::Value, field: &str) -> Result<u32, ProviderError> {
    body["usage"][field]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            ProviderError::InvalidResponse(format!("provider response has no usage.{field}"))
        })
}

#[async_trait]
impl TextGenerationProvider for OpenAiCompatibleClient {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        let url = self.egress.endpoint();
        let payload = json!({
            "model": request.model,
            "messages": [{"role": "user", "content": request.prompt}],
            "max_tokens": request.max_tokens,
        });

        let mut http_request = self.client.post(url).json(&payload);
        if let Some(key) = &self.api_key {
            http_request = http_request.bearer_auth(key);
        }

        let response = http_request.send().await.map_err(|error| {
            if error.is_timeout() {
                ProviderError::Timeout
            } else {
                // `error` renders the URL but never the bearer token, which
                // reqwest keeps out of its Display output.
                ProviderError::Unavailable(error.to_string())
            }
        })?;

        if !response.status().is_success() {
            return Err(Self::map_status(response.status()));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;

        // An absent content field is a failed call, not an empty answer.
        // Defaulting to "" here would report success for a response the
        // adapter did not understand, and the run would record that the model
        // said nothing.
        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "provider response has no choices[0].message.content".into(),
                )
            })?
            .to_string();

        Ok(GenerationResponse {
            content,
            model: request.model,
            prompt_tokens: required_token_count(&body, "prompt_tokens")?,
            completion_tokens: required_token_count(&body, "completion_tokens")?,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatibleClient {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/embeddings", self.base_url);
        let expected = request.input.len();
        let payload = json!({
            "model": request.model,
            "input": request.input,
        });

        let mut http_request = self.client.post(&url).json(&payload);
        if let Some(key) = &self.api_key {
            http_request = http_request.bearer_auth(key);
        }

        let response = http_request.send().await.map_err(|error| {
            if error.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Unavailable(error.to_string())
            }
        })?;

        if !response.status().is_success() {
            return Err(Self::map_status(response.status()));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;

        let data = body["data"].as_array().ok_or_else(|| {
            ProviderError::InvalidResponse("provider response has no data array".into())
        })?;

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let vector = item["embedding"].as_array().ok_or_else(|| {
                ProviderError::InvalidResponse("embedding entry has no vector".into())
            })?;
            let mut values = Vec::with_capacity(vector.len());
            for value in vector {
                // Silently dropping unparseable components would shorten the
                // vector, and a shortened vector is not a worse embedding — it
                // is a different point in a different space.
                values.push(value.as_f64().ok_or_else(|| {
                    ProviderError::InvalidResponse("embedding component is not a number".into())
                })? as f32);
            }
            embeddings.push(values);
        }

        // One embedding per input, or the caller cannot tell which input each
        // vector belongs to.
        if embeddings.len() != expected {
            return Err(ProviderError::InvalidResponse(format!(
                "provider returned {} embeddings for {expected} inputs",
                embeddings.len()
            )));
        }

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use vestrace_domain::DataDestination;

    #[test]
    fn the_api_key_is_never_rendered() {
        let client =
            OpenAiCompatibleClient::new("https://api.example.com/v1", Some("sk-secret".into()))
                .unwrap();
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("sk-secret"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn a_trailing_slash_does_not_produce_a_doubled_path() {
        let client = OpenAiCompatibleClient::new("https://api.example.com/v1/", None).unwrap();
        assert_eq!(
            client.egress.endpoint(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn missing_usage_is_an_error_rather_than_zero() {
        // Zero here would understate consumption in a system that enforces
        // budgets, so the budget would be spent without appearing spent.
        let body = serde_json::json!({ "choices": [] });
        assert!(matches!(
            required_token_count(&body, "prompt_tokens"),
            Err(ProviderError::InvalidResponse(_))
        ));
    }

    #[test]
    fn present_usage_is_read() {
        let body = serde_json::json!({ "usage": { "prompt_tokens": 12 } });
        assert_eq!(required_token_count(&body, "prompt_tokens").unwrap(), 12);
    }

    #[test]
    fn rate_limiting_is_distinguished_from_other_failures() {
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            ProviderError::RateLimited
        ));
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            ProviderError::Unavailable(_)
        ));
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::BAD_REQUEST),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[test]
    fn a_refused_credential_is_distinguished_from_a_malformed_reply() {
        // Providers disagree on the status: NVIDIA answers 403 to a bad key,
        // OpenAI 401. Both mean the same thing to the operator.
        for status in [
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::FORBIDDEN,
        ] {
            assert!(
                matches!(
                    OpenAiCompatibleClient::map_status(status),
                    ProviderError::CredentialRejected(_)
                ),
                "{status} should be reported as a rejected credential"
            );
        }
    }

    #[test]
    fn a_rejected_credential_does_not_render_the_key() {
        // The status line is safe to repeat; a provider's 403 body is not
        // guaranteed to be, and neither is anything derived from the key.
        let rendered =
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::FORBIDDEN).to_string();
        assert_eq!(
            rendered,
            "Credential rejected: provider returned 403 Forbidden"
        );
    }

    #[test]
    fn a_provider_error_does_not_carry_the_response_body() {
        // The body echoes request content back, which may be the data a caller
        // is careful about.
        let rendered =
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::BAD_REQUEST).to_string();
        assert_eq!(
            rendered,
            "Invalid response: provider returned 400 Bad Request"
        );
    }

    #[test]
    fn loopback_is_local_only_with_redirects_and_proxies_disabled() {
        let client = OpenAiCompatibleClient::new("http://[::1]:12345/v1", None).unwrap();
        let egress = client.egress();

        assert_eq!(egress.endpoint(), "http://[::1]:12345/v1/chat/completions");
        assert_eq!(egress.destination(), DataDestination::LocalModel);
        assert!(egress.redirects_disabled());
        assert!(egress.proxy_disabled());
    }

    #[test]
    fn private_network_hosts_are_remote_providers() {
        for endpoint in [
            "http://10.0.0.4:12345/v1",
            "http://172.16.0.4:12345/v1",
            "http://192.168.0.4:12345/v1",
        ] {
            let client = OpenAiCompatibleClient::new(endpoint, None).unwrap();
            assert_eq!(
                client.egress().destination(),
                DataDestination::RemoteProvider,
                "{endpoint} was mistaken for a local model"
            );
        }
    }

    #[test]
    fn localhost_requires_every_resolved_address_to_be_loopback() {
        let endpoint = reqwest::Url::parse("http://localhost:12345/v1").unwrap();
        let mixed_addresses = vec!["127.0.0.1".parse().unwrap(), "203.0.113.7".parse().unwrap()];

        let destination =
            destination_for_endpoint_with_resolver(&endpoint, true, true, |host, port| {
                assert_eq!(host, "localhost");
                assert_eq!(port, 12345);
                Ok(mixed_addresses)
            });

        assert_eq!(destination, DataDestination::RemoteProvider);
    }

    #[test]
    fn loopback_without_either_transport_guard_is_remote() {
        let endpoint = reqwest::Url::parse("http://localhost:12345/v1").unwrap();
        assert_eq!(
            destination_for_endpoint(&endpoint, false, true),
            DataDestination::RemoteProvider
        );
        assert_eq!(
            destination_for_endpoint(&endpoint, true, false),
            DataDestination::RemoteProvider
        );
    }

    #[tokio::test]
    async fn a_completion_redirect_to_a_remote_host_is_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let received = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 8192];
            let count = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]).into_owned();
            socket
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            request
        });

        let client = OpenAiCompatibleClient::with_timeout(
            format!("http://{address}/v1"),
            Some("credential-that-must-not-be-forwarded".into()),
            Duration::from_secs(2),
        )
        .unwrap();
        let result = client
            .generate(GenerationRequest {
                model: "test-model".into(),
                prompt: "prompt-that-must-not-leave-loopback".into(),
                max_tokens: Some(1),
            })
            .await;

        assert!(
            matches!(result, Err(ProviderError::InvalidResponse(ref message)) if message.contains("302 Found")),
            "redirect was not returned to the caller: {result:?}"
        );
        let request = received.join().unwrap();
        assert!(request.contains("prompt-that-must-not-leave-loopback"));
    }
}
