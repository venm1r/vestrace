use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use vestrace_application::ApplicationError;
use vestrace_application::retrieval::EmbeddingProvider;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Embeddings from any OpenAI-compatible `/v1/embeddings` endpoint.
///
/// The same shape serves LM Studio, Ollama, NVIDIA NIM and OpenAI itself, which
/// is the point: the model that embeds a workspace's memories is a deployment
/// choice, and the space records which one it was so a later change cannot go
/// unnoticed.
pub struct OpenAiCompatibleEmbeddingClient {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatibleEmbeddingClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleEmbeddingClient")
            .field("base_url", &self.base_url)
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
        // No fallback to `Client::default()`: that produces a client with no
        // timeout, which is the one property this builder exists to set. The
        // predecessor of this file had exactly that bug.
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|error| {
                ApplicationError::InvalidConfiguration(format!(
                    "embedding HTTP client could not be built: {error}"
                ))
            })?;

        let base_url = base_url.into();
        let model = model.into();
        if base_url.trim().is_empty() || model.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "embedding base URL and model must not be empty".into(),
            ));
        }

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
        })
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
            .post(format!("{}/embeddings", self.base_url))
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
