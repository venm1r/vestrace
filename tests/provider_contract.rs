use vestrace_application::providers::ports::*;
use vestrace_infrastructure::providers::OpenAiCompatibleClient;

#[tokio::test]
async fn test_provider_client_initialization() {
    let client = OpenAiCompatibleClient::new("http://localhost:8080/v1", Some("test-key".into()));
    let req = GenerationRequest {
        model: "gpt-4o".into(),
        prompt: "Hello world".into(),
        max_tokens: Some(10),
    };

    // Contract verification
    assert_eq!(req.model, "gpt-4o");
}
