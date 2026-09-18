//! Deliberate live acceptance against the configured LM Studio embedding model.
//!
//! This is ignored in the ordinary suite because LM Studio is an external
//! prerequisite. Run it explicitly while the named model is served at
//! `http://localhost:12345/v1`.

use vestrace_application::retrieval::EmbeddingProvider;
use vestrace_domain::DataDestination;
use vestrace_infrastructure::OpenAiCompatibleEmbeddingClient;

#[tokio::test]
#[ignore = "needs LM Studio with text-embedding-nomic-embed-text-v1.5 loaded"]
async fn hardened_embedding_client_reaches_the_named_lm_studio_model() {
    let client = OpenAiCompatibleEmbeddingClient::new(
        "http://localhost:12345/v1",
        "text-embedding-nomic-embed-text-v1.5",
        None,
    )
    .unwrap();

    let embeddings = client
        .embed(&["A live embedding must cross the hardened client.".into()])
        .await
        .unwrap();

    assert_eq!(client.egress().destination(), DataDestination::LocalModel);
    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].len(), 768);
    assert!(embeddings[0].iter().any(|component| *component != 0.0));
}
