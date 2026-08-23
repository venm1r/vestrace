mod egress;
pub mod embeddings;
pub mod openai_compatible;
pub mod secret_backed;
pub mod webhook;

pub use embeddings::OpenAiCompatibleEmbeddingClient;
pub use openai_compatible::OpenAiCompatibleClient;
pub use secret_backed::{PROVIDER_API_KEY_PURPOSE, SecretBackedProviderFactory};
pub use webhook::HttpWebhookEffectAdapter;
