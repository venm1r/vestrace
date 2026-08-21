use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use vestrace_application::{ApplicationError, ExternalEffectReadBackAdapter};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, IdempotencyProfile,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};
use vestrace_infrastructure::HttpExternalEffectReadBackAdapter;

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn intent() -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "run://01900000-0000-7000-8000-000000000001",
        WorkspaceId::new(),
        PrincipalId::new(),
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        Some("budget:reservation-1"),
        Some("policy:decision-1"),
        at(10),
    )
    .unwrap()
}

fn unknown_receipt(intent: &ExternalEffectIntent) -> ExternalEffectReceipt {
    ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        "webhook-v1",
        at(20),
        vec!["evidence:dispatch-timeout".to_owned()],
    )
    .unwrap()
}

/// Serve exactly one request with a fixed status line and body, then stop.
async fn stub_once(status_line: &'static str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0_u8; 2048];
        let _ = socket.read(&mut buffer).await;
        let response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
    });
    format!("http://{address}")
}

#[tokio::test]
async fn read_back_maps_a_confirmed_observation() {
    let base = stub_once(
        "200 OK",
        r#"{"effect_applied":true,"state_ref":"order/9001","evidence_refs":["etag:abc"],"evidence_strength":"external_resource_read_back"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();

    let observations = adapter.observe(&intent, None).await.unwrap();

    assert_eq!(observations.len(), 1);
    let observed = &observations[0];
    assert_eq!(observed.effect_applied(), Some(true));
    assert_eq!(observed.state_ref(), "order/9001");
    assert_eq!(
        observed.evidence_strength(),
        EvidenceStrength::ExternalResourceReadBack
    );
}

/// An unreachable or erroring provider is unavailable evidence, never evidence
/// that the effect did not happen.
#[tokio::test]
async fn read_back_failure_is_unavailable_not_an_empty_observation() {
    let base = stub_once("503 Service Unavailable", r#"{"error":"down"}"#).await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    let error = adapter.observe(&intent, Some(&receipt)).await.unwrap_err();

    assert!(
        matches!(error, ApplicationError::Unavailable(ref message) if message.contains("503")),
        "unexpected error: {error:?}"
    );
}

/// Evidence with no reference cannot be traced back to anything, so it is
/// rejected rather than recorded as weak evidence.
#[tokio::test]
async fn read_back_rejects_observations_without_evidence_references() {
    let base = stub_once(
        "200 OK",
        r#"{"effect_applied":true,"state_ref":"order/9001","evidence_refs":[],"evidence_strength":"external_resource_read_back"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    let error = adapter.observe(&intent, Some(&receipt)).await.unwrap_err();

    assert!(
        matches!(error, ApplicationError::Domain(_)),
        "unexpected error: {error:?}"
    );
}

/// An indeterminate provider answer must stay indeterminate.
#[tokio::test]
async fn read_back_preserves_an_unknown_effect_application() {
    let base = stub_once(
        "200 OK",
        r#"{"effect_applied":null,"state_ref":"order/9001","evidence_refs":["trace:1"],"evidence_strength":"operation_status"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    let observations = adapter.observe(&intent, Some(&receipt)).await.unwrap();

    assert_eq!(observations[0].effect_applied(), None);
    assert_eq!(
        observations[0].evidence_strength(),
        EvidenceStrength::OperationStatus
    );
}

#[tokio::test]
async fn read_back_requires_a_non_blank_endpoint() {
    let error = HttpExternalEffectReadBackAdapter::new("   ").unwrap_err();

    assert!(matches!(error, ApplicationError::InvalidConfiguration(_)));
}

#[tokio::test]
async fn read_back_adapter_is_usable_as_a_shared_port() {
    let adapter: Arc<dyn ExternalEffectReadBackAdapter> =
        Arc::new(HttpExternalEffectReadBackAdapter::new("http://127.0.0.1:1").unwrap());
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    assert!(adapter.observe(&intent, Some(&receipt)).await.is_err());
}
