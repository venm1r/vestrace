use vestrace_fault_scenario::AdapterStub;

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
