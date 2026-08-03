use vestrace_application::GenerationRequest;
use vestrace_infrastructure::providers::OpenAiCompatibleClient;

#[tokio::test]
async fn test_provider_client_initialization() {
    let _client = OpenAiCompatibleClient::new("http://localhost:8080/v1", Some("test-key".into()));
    let request = GenerationRequest {
        model: "gpt-4o".into(),
        prompt: "Hello world".into(),
        max_tokens: Some(10),
    };

    assert_eq!(request.model, "gpt-4o");
}
