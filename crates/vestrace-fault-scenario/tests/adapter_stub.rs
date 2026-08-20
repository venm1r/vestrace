use vestrace_application::ExternalEffectReadBackAdapter;
use vestrace_domain::external_effects::{
    EffectPrecondition, EvidenceStrength, ExternalEffectAdapter, ExternalEffectIntent,
    ExternalEffectReceipt,
};
use vestrace_domain::id::AgentRunId;
use vestrace_domain::{ExternalEffectId, PrincipalId, RiskCategory, WorkspaceId, now};
use vestrace_fault_scenario::AdapterStub;
use vestrace_infrastructure::{HttpExternalEffectReadBackAdapter, HttpWebhookEffectAdapter};

/// The count is the evidence for `retry_attempted`, so it has to be exact and
/// it has to survive the death of whatever was dispatching.
#[tokio::test]
async fn the_stub_counts_every_dispatch_it_receives() {
    let stub = AdapterStub::start().await.unwrap();
    assert_eq!(stub.dispatch_count(), 0);

    let client = reqwest::Client::new();
    client.post(stub.dispatch_url()).send().await.unwrap();
    client.post(stub.dispatch_url()).send().await.unwrap();

    assert_eq!(stub.dispatch_count(), 2, "a retry is a second dispatch");
    stub.shutdown().await;
}

/// Read-back is how reconciliation asks what happened. A stub that could not
/// answer would make every UNKNOWN unresolvable for a reason belonging to the
/// harness rather than to the system.
#[tokio::test]
async fn the_stub_answers_read_back_for_what_it_received() {
    let stub = AdapterStub::start().await.unwrap();
    let client = reqwest::Client::new();
    client.post(stub.dispatch_url()).send().await.unwrap();

    let body = client
        .get(stub.read_back_url())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("\"dispatches\":1"),
        "unexpected read-back body: {body}"
    );
    stub.shutdown().await;
}

/// The route reconciliation actually uses, driven by the adapter a deployment
/// actually ships.
///
/// `HttpExternalEffectReadBackAdapter` is what the worker constructs at
/// `crates/vestrace-cli/src/commands/worker.rs:369`, and its response reader is
/// `deny_unknown_fields` over four named fields. Asserting on the stub's raw
/// body would only pin what this crate believes that reader wants; running the
/// reader pins what it wants.
#[tokio::test]
async fn the_deployments_own_read_back_adapter_can_read_this_stub() {
    let stub = AdapterStub::start().await.unwrap();
    reqwest::Client::new()
        .post(stub.dispatch_url())
        .send()
        .await
        .unwrap();

    let effect_adapter = HttpWebhookEffectAdapter::new(
        "read-back-fixture-webhook",
        stub.dispatch_url(),
        stub.read_back_url(),
    )
    .expect("the fixture adapter is configurable");
    let intent = read_back_intent(&effect_adapter, &stub);
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        now(),
        vec![format!("effect://{}/timeout", intent.id())],
    )
    .expect("a synthetic unknown receipt is valid");

    let read_back = HttpExternalEffectReadBackAdapter::new(stub.read_back_url())
        .expect("the read-back adapter is configurable");
    let observed = read_back
        .observe(&intent, &receipt)
        .await
        .expect("the stub answers the vocabulary the read-back adapter parses");

    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].evidence_strength(),
        EvidenceStrength::ExternalResourceReadBack
    );
    assert_eq!(
        observed[0].effect_applied(),
        Some(true),
        "the stub counted the dispatch, so it says the effect reached it"
    );
    assert!(!observed[0].state_ref().trim().is_empty());
    assert!(!observed[0].evidence_refs().is_empty());

    stub.shutdown().await;
}

/// A stub that never received a dispatch says so, and says it in the same
/// vocabulary. This is the reading that makes a reconciliation `NotApplied`
/// rather than inconclusive, so it must not come back as an error or a blank.
#[tokio::test]
async fn a_stub_that_received_nothing_reads_back_as_not_applied() {
    let stub = AdapterStub::start().await.unwrap();
    let effect_adapter = HttpWebhookEffectAdapter::new(
        "read-back-fixture-webhook",
        stub.dispatch_url(),
        stub.read_back_url(),
    )
    .expect("the fixture adapter is configurable");
    let intent = read_back_intent(&effect_adapter, &stub);
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        now(),
        vec![format!("effect://{}/timeout", intent.id())],
    )
    .expect("a synthetic unknown receipt is valid");

    let read_back = HttpExternalEffectReadBackAdapter::new(stub.read_back_url())
        .expect("the read-back adapter is configurable");
    let observed = read_back
        .observe(&intent, &receipt)
        .await
        .expect("the stub answers even when it received nothing");

    assert_eq!(observed[0].effect_applied(), Some(false));

    stub.shutdown().await;
}

/// Adding the per-effect route must not have moved the count, which is the only
/// evidence `retry_attempted` has. Read-back is a `GET`; it observes and does
/// not act.
#[tokio::test]
async fn reading_back_never_moves_the_dispatch_count() {
    let stub = AdapterStub::start().await.unwrap();
    let client = reqwest::Client::new();
    client.post(stub.dispatch_url()).send().await.unwrap();
    assert_eq!(stub.dispatch_count(), 1);

    client.get(stub.read_back_url()).send().await.unwrap();
    client
        .get(format!(
            "{}/{}",
            stub.read_back_url(),
            ExternalEffectId::new()
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(
        stub.dispatch_count(),
        1,
        "observing what happened is not a second dispatch"
    );
    stub.shutdown().await;
}

fn read_back_intent(
    adapter: &HttpWebhookEffectAdapter,
    stub: &AdapterStub,
) -> ExternalEffectIntent {
    let descriptor = adapter.descriptor();
    ExternalEffectIntent::new(
        format!("run://{}", AgentRunId::new()),
        WorkspaceId::new(),
        PrincipalId::new(),
        descriptor.name(),
        "send",
        stub.dispatch_url(),
        "sha256:arguments",
        "deliver the read-back fixture",
        vec![EffectPrecondition::new("resource-version", "v1").expect("a valid precondition")],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        descriptor.reversibility(),
        descriptor.idempotency_profile(),
        descriptor.delivery_semantics(),
        descriptor.required_capability(),
        None::<String>,
        None::<String>,
        now(),
    )
    .expect("the read-back fixture intent is valid")
}
