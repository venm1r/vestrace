use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use vestrace_application::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, GenerationRequest, GenerationResponse,
    ProviderError, TextGenerationProvider,
};

/// How long to wait for a provider before treating it as unavailable.
///
/// Generous, because a large completion legitimately takes a while, but finite:
/// without a deadline a wedged provider holds a worker slot forever, and the
/// run it is executing never reaches a terminal state.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct OpenAiCompatibleClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatibleClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicit rather than absent, so that adding `#[derive(Debug)]` to a
        // struct holding this client cannot start printing the key.
        formatter
            .debug_struct("OpenAiCompatibleClient")
            .field("base_url", &self.base_url)
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
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;

        Ok(Self {
            client,
            // Trailing slashes would otherwise produce `//chat/completions`,
            // which some gateways route differently.
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
        })
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
        let url = format!("{}/chat/completions", self.base_url);
        let payload = json!({
            "model": request.model,
            "messages": [{"role": "user", "content": request.prompt}],
            "max_tokens": request.max_tokens,
        });

        let mut http_request = self.client.post(&url).json(&payload);
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
    use super::*;

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
        assert_eq!(client.base_url, "https://api.example.com/v1");
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
}
