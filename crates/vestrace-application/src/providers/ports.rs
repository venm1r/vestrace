use async_trait::async_trait;
use vestrace_domain::id::WorkspaceId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationResponse {
    pub content: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Timeout occurred")]
    Timeout,
    #[error("Rate limited")]
    RateLimited,
    #[error("Service unavailable: {0}")]
    Unavailable(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse, ProviderError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;
}
