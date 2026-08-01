use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use vestrace_application::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, GenerationRequest, GenerationResponse,
    ProviderError, TextGenerationProvider,
};

pub struct OpenAiCompatibleClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url: base_url.into(),
            api_key,
        }
    }
}

#[async_trait]
impl TextGenerationProvider for OpenAiCompatibleClient {
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let payload = json!({
            "model": request.model,
            "messages": [{"role": "user", "content": request.prompt}],
            "max_tokens": request.max_tokens,
        });

        let mut req = self.client.post(&url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP Status: {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(GenerationResponse {
            content,
            model: request.model,
            prompt_tokens: body["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: body["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatibleClient {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/embeddings", self.base_url);
        let payload = json!({
            "model": request.model,
            "input": request.input,
        });

        let mut req = self.client.post(&url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP Status: {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let mut embeddings = Vec::new();
        if let Some(data) = body["data"].as_array() {
            for item in data {
                if let Some(vec) = item["embedding"].as_array() {
                    let vec_f32: Vec<f32> = vec.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                    embeddings.push(vec_f32);
                }
            }
        }

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model,
        })
    }
}
