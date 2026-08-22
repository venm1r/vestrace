use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
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

fn receipt_with_provider_evidence(
    intent: &ExternalEffectIntent,
    external_resource_id: Option<&str>,
    external_version: Option<&str>,
    response_digest: Option<&str>,
) -> ExternalEffectReceipt {
    let mut payload = serde_json::to_value(unknown_receipt(intent)).unwrap();
    payload["external_resource_id"] = serde_json::json!(external_resource_id);
    payload["external_version"] = serde_json::json!(external_version);
    payload["response_digest"] = serde_json::json!(response_digest);
    serde_json::from_value(payload).unwrap()
}

fn request_parts(target: &str) -> (&str, BTreeMap<&str, &str>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let parameters = query
        .split('&')
        .filter(|parameter| !parameter.is_empty())
        .map(|parameter| parameter.split_once('=').unwrap())
        .collect();
    (path, parameters)
}

async fn observing_stub_once(
    status_line: &'static str,
    body: &'static str,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_sender, request_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 512];
        loop {
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8(request).unwrap();
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap()
            .to_owned();
        let _ = request_sender.send(target);

        let response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
    });
    (format!("http://{address}"), request_receiver)
}

/// Serve exactly one request with a fixed status line and body, then stop.
async fn stub_once(status_line: &'static str, body: &'static str) -> String {
    observing_stub_once(status_line, body).await.0
}

#[tokio::test]
async fn read_back_sends_an_exact_external_resource_id_without_changing_the_path() {
    let (base, request) = observing_stub_once(
        "200 OK",
        r#"{"effect_applied":true,"state_ref":"order/9001","evidence_refs":["provider:order-9001"],"evidence_strength":"external_resource_read_back"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = receipt_with_provider_evidence(&intent, Some("provider-order-9001"), None, None);

    adapter.observe(&intent, Some(&receipt)).await.unwrap();

    let target = request.await.unwrap();
    let (path, parameters) = request_parts(&target);
    assert_eq!(path, format!("/{}", intent.id()));
    assert_eq!(
        parameters,
        BTreeMap::from([("exact_external_resource_id", "provider-order-9001")])
    );
}

#[tokio::test]
async fn read_back_sends_an_etag_version_revision_independently() {
    let (base, request) = observing_stub_once(
        "200 OK",
        r#"{"effect_applied":null,"state_ref":"order/9001","evidence_refs":["etag:v17"],"evidence_strength":"etag_version"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = receipt_with_provider_evidence(&intent, None, Some("v17"), None);

    adapter.observe(&intent, Some(&receipt)).await.unwrap();

    let target = request.await.unwrap();
    let (path, parameters) = request_parts(&target);
    assert_eq!(path, format!("/{}", intent.id()));
    assert_eq!(
        parameters,
        BTreeMap::from([("etag_version_revision", "v17")])
    );
}

#[tokio::test]
async fn read_back_sends_a_content_hash_independently() {
    let (base, request) = observing_stub_once(
        "200 OK",
        r#"{"effect_applied":null,"state_ref":"order/9001","evidence_refs":["digest:deadbeef"],"evidence_strength":"content_hash"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = receipt_with_provider_evidence(&intent, None, None, Some("sha256-deadbeef"));

    adapter.observe(&intent, Some(&receipt)).await.unwrap();

    let target = request.await.unwrap();
    let (path, parameters) = request_parts(&target);
    assert_eq!(path, format!("/{}", intent.id()));
    assert_eq!(
        parameters,
        BTreeMap::from([("content_hash", "sha256-deadbeef")])
    );
}

#[tokio::test]
async fn read_back_without_provider_evidence_issues_the_original_request() {
    let intent = intent();
    let empty_receipt = unknown_receipt(&intent);

    for receipt in [Some(&empty_receipt), None] {
        let (base, request) = observing_stub_once(
            "200 OK",
            r#"{"effect_applied":null,"state_ref":"order/9001","evidence_refs":["marker:effect"],"evidence_strength":"marker_search"}"#,
        )
        .await;
        let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();

        adapter.observe(&intent, receipt).await.unwrap();

        let target = request.await.unwrap();
        assert_eq!(target, format!("/{}", intent.id()));
    }
}

#[tokio::test]
async fn provider_answer_remains_the_evidence_strength_with_a_strong_request_hint() {
    let (base, request) = observing_stub_once(
        "200 OK",
        r#"{"effect_applied":true,"state_ref":"order/9001","evidence_refs":["marker:effect"],"evidence_strength":"marker_search"}"#,
    )
    .await;
    let adapter = HttpExternalEffectReadBackAdapter::new(base).unwrap();
    let intent = intent();
    let receipt = receipt_with_provider_evidence(
        &intent,
        Some("provider-order-9001"),
        Some("v17"),
        Some("sha256-deadbeef"),
    );

    let observations = adapter.observe(&intent, Some(&receipt)).await.unwrap();

    let target = request.await.unwrap();
    let (path, parameters) = request_parts(&target);
    assert_eq!(path, format!("/{}", intent.id()));
    assert_eq!(
        parameters,
        BTreeMap::from([
            ("content_hash", "sha256-deadbeef"),
            ("etag_version_revision", "v17"),
            ("exact_external_resource_id", "provider-order-9001"),
        ])
    );
    assert_eq!(
        observations[0].evidence_strength(),
        EvidenceStrength::MarkerSearch
    );
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
