use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vestrace_domain::DataDestination;

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
    /// The provider refused the credential.
    ///
    /// Separate from [`Self::InvalidResponse`] because it is the one provider
    /// failure an operator can act on directly, and calling it an unusable
    /// response sends them to read adapter code instead of rotating a key.
    #[error("Credential rejected: {0}")]
    CredentialRejected(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError>;
}

/// Immutable network facts supplied by the adapter that owns the client.
///
/// The executor must not infer locality from deployment configuration: this
/// descriptor describes the endpoint and transport policy of the client that
/// will actually carry provider input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEgress {
    endpoint: String,
    destination: DataDestination,
    redirects_disabled: bool,
    proxy_disabled: bool,
}

impl ProviderEgress {
    pub fn new(
        endpoint: impl Into<String>,
        destination: DataDestination,
        redirects_disabled: bool,
        proxy_disabled: bool,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            destination,
            redirects_disabled,
            proxy_disabled,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn destination(&self) -> DataDestination {
        self.destination
    }

    pub fn redirects_disabled(&self) -> bool {
        self.redirects_disabled
    }

    pub fn proxy_disabled(&self) -> bool {
        self.proxy_disabled
    }
}

pub struct ResolvedTextGenerationProvider {
    pub provider: std::sync::Arc<dyn TextGenerationProvider>,
    pub egress: ProviderEgress,
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;
}

/// Builds a provider for a specific workspace.
///
/// A factory rather than a single shared provider, because the credential is a
/// per-workspace secret. One process-wide client would mean one workspace's key
/// being used to bill another's work.
///
/// Resolving per call also means a rotated key takes effect without a restart,
/// and the plaintext key is not held for the process's lifetime.
#[async_trait]
pub trait TextGenerationProviderFactory: Send + Sync {
    async fn provider_for(
        &self,
        context: &crate::RequestContext,
    ) -> Result<ResolvedTextGenerationProvider, crate::ApplicationError>;
}

pub type SharedTextGenerationProviderFactory = std::sync::Arc<dyn TextGenerationProviderFactory>;
