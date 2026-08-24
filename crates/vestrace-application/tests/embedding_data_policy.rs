use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_application::retrieval::{
    EmbedMemoryHandler, EmbeddingBackfillService, EmbeddingProvider, EmbeddingSpace,
    EmbeddingStore, MemoryTextSource, PendingEmbedding, SharedEmbeddingProvider,
};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings, EmbeddingInput,
    EmbeddingPurpose, OutboxHandler, OutboxMessage, ProviderEgress, RequestContext,
};
use vestrace_domain::id::{EmbeddingSpaceId, MemoryId, OutboxId, RetrievalRunId};
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{DataDestination, DataPolicyId, PrincipalId, Sensitivity, WorkspaceId, now};

struct RecordingProvider {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl EmbeddingProvider for RecordingProvider {
    fn model(&self) -> &str {
        "embedding-model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.calls.lock().unwrap().push(inputs.to_vec());
        self.events.lock().unwrap().push("provider");
        Ok(inputs.iter().map(|_| vec![0.25, -0.5]).collect())
    }
}

#[derive(Default)]
struct Decisions {
    records: Mutex<Vec<EmbeddingDataPolicyDecisionRecord>>,
    events: Arc<Mutex<Vec<&'static str>>>,
    fail: bool,
}

#[async_trait]
impl EmbeddingDataPolicyDecisionRepository for Decisions {
    async fn record(
        &self,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        self.events.lock().unwrap().push("decision");
        if self.fail {
            return Err(ApplicationError::Storage(
                "embedding_data_policy_decisions insert refused".into(),
            ));
        }
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }
}

struct Harness {
    provider: Arc<vestrace_application::GovernedEmbeddingProvider>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    decisions: Arc<Decisions>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

fn harness(
    destination: DataDestination,
    mode: EmbeddingDataPolicyMode,
    labels: &[&str],
    allow_unclassified: bool,
    repository_fails: bool,
) -> Harness {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let raw: SharedEmbeddingProvider = Arc::new(RecordingProvider {
        calls: calls.clone(),
        events: events.clone(),
    });
    let decisions = Arc::new(Decisions {
        records: Mutex::new(Vec::new()),
        events: events.clone(),
        fail: repository_fails,
    });
    let endpoint = match destination {
        DataDestination::LocalModel => "http://localhost:12345/v1/embeddings",
        _ => "https://embeddings.example.test/v1/embeddings",
    };
    let settings = EmbeddingDataPolicySettings {
        classification_policy: ClassificationPolicy::new(
            labels.iter().copied(),
            allow_unclassified,
        )
        .unwrap(),
        policy: DataPolicy::new(
            DataPolicyId::new(),
            "embedding-policy-v1",
            Sensitivity::Confidential,
            BTreeSet::from([DataDestination::LocalModel]),
            None,
        )
        .unwrap(),
        classification: Sensitivity::Confidential,
        mode,
    };
    let provider = EmbeddingDataPolicyGate::new(settings, decisions.clone())
        .govern(raw, ProviderEgress::new(endpoint, destination, true, true));
    Harness {
        provider,
        calls,
        decisions,
        events,
    }
}

#[tokio::test]
async fn a_label_retrieval_would_withhold_is_refused_before_embedding() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        false,
    );

    let error = harness
        .provider
        .embed_delivery(
            OutboxId::new(),
            1,
            &EmbeddingInput::new("restricted memory", Some("restricted".into())),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Policy(_)));
    assert!(
        error.to_string().contains("classification label check"),
        "{error}"
    );
    assert!(error.to_string().contains("restricted"), "{error}");
    assert!(
        !error.to_string().contains("destination check refused"),
        "{error}"
    );
    assert!(harness.calls.lock().unwrap().is_empty());
    let records = harness.decisions.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].classification_allowed);
    assert!(records[0].destination_allowed);
    assert!(!records[0].allowed);
}

#[tokio::test]
async fn unclassified_content_follows_the_explicit_flag_in_both_directions() {
    let denied = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        false,
    );
    let allowed = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        true,
        false,
    );

    assert!(
        denied
            .provider
            .embed_delivery(OutboxId::new(), 1, &EmbeddingInput::new("unassessed", None))
            .await
            .is_err()
    );
    allowed
        .provider
        .embed_delivery(OutboxId::new(), 1, &EmbeddingInput::new("unassessed", None))
        .await
        .unwrap();

    let denied_records = denied.decisions.records.lock().unwrap();
    let allowed_records = allowed.decisions.records.lock().unwrap();
    assert_eq!(denied_records[0].unclassified_count, 1);
    assert_eq!(allowed_records[0].unclassified_count, 1);
    assert!(!denied_records[0].classification_allowed);
    assert!(allowed_records[0].classification_allowed);
    assert_ne!(denied_records[0].reason, allowed_records[0].reason);
}

#[tokio::test]
async fn a_local_allowance_is_recorded_before_one_provider_call() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        false,
    );
    let message_id = OutboxId::new();

    let result = harness
        .provider
        .embed_delivery(
            message_id,
            3,
            &EmbeddingInput::new("admitted", Some("internal".into())),
        )
        .await
        .unwrap();

    assert_eq!(result, vec![vec![0.25, -0.5]]);
    assert_eq!(
        *harness.events.lock().unwrap(),
        vec!["decision", "provider"]
    );
    let records = harness.decisions.records.lock().unwrap();
    let record = &records[0];
    assert_eq!(record.purpose, EmbeddingPurpose::Delivery);
    assert_eq!(record.causal_reference_id, message_id.as_uuid());
    assert_eq!(record.delivery_attempt, Some(3));
    assert_eq!(record.batch_ordinal, None);
    assert_eq!(record.input_count, 1);
    assert!(record.allowed);
}

#[tokio::test]
async fn a_remote_destination_is_denied_without_sending_content() {
    let harness = harness(
        DataDestination::RemoteProvider,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        false,
    );

    let error = harness
        .provider
        .embed_delivery(
            OutboxId::new(),
            1,
            &EmbeddingInput::new("admitted label", Some("internal".into())),
        )
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("destination check refused"),
        "{error}"
    );
    assert!(error.to_string().contains("RemoteProvider"), "{error}");
    assert!(harness.calls.lock().unwrap().is_empty());
    let records = harness.decisions.records.lock().unwrap();
    assert!(records[0].classification_allowed);
    assert!(!records[0].destination_allowed);
}

#[tokio::test]
async fn observe_mode_records_a_denial_and_still_calls_the_provider() {
    let harness = harness(
        DataDestination::RemoteProvider,
        EmbeddingDataPolicyMode::Observe,
        &["internal"],
        false,
        false,
    );

    harness
        .provider
        .embed_delivery(
            OutboxId::new(),
            1,
            &EmbeddingInput::new("observed", Some("internal".into())),
        )
        .await
        .unwrap();

    assert_eq!(harness.calls.lock().unwrap().len(), 1);
    assert_eq!(
        *harness.events.lock().unwrap(),
        vec!["decision", "provider"]
    );
    let records = harness.decisions.records.lock().unwrap();
    assert!(!records[0].allowed);
    assert_eq!(records[0].mode, EmbeddingDataPolicyMode::Observe);
}

#[tokio::test]
async fn a_repository_failure_prevents_the_provider_call() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        true,
    );

    let error = harness
        .provider
        .embed_delivery(
            OutboxId::new(),
            1,
            &EmbeddingInput::new("admitted", Some("internal".into())),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Storage(_)));
    assert!(
        error
            .to_string()
            .contains("embedding_data_policy_decisions")
    );
    assert!(harness.calls.lock().unwrap().is_empty());
    assert_eq!(*harness.events.lock().unwrap(), vec!["decision"]);
}

#[tokio::test]
async fn purpose_specific_methods_write_only_their_own_purpose_and_cause_shape() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        true,
        false,
    );
    let request_id = RetrievalRunId::new();
    let invocation_id = Uuid::now_v7();
    let message_id = OutboxId::new();

    harness
        .provider
        .embed_retrieval_query(request_id, "query")
        .await
        .unwrap();
    harness
        .provider
        .probe_dimensions(invocation_id, 7)
        .await
        .unwrap();
    harness
        .provider
        .embed_backfill(
            invocation_id,
            7,
            &[
                EmbeddingInput::new("one", Some("internal".into())),
                EmbeddingInput::new("two", None),
            ],
        )
        .await
        .unwrap();
    harness
        .provider
        .embed_delivery(
            message_id,
            1,
            &EmbeddingInput::new("delivery one", Some("internal".into())),
        )
        .await
        .unwrap();
    harness
        .provider
        .embed_delivery(
            message_id,
            2,
            &EmbeddingInput::new("delivery two", Some("internal".into())),
        )
        .await
        .unwrap();

    let records = harness.decisions.records.lock().unwrap();
    assert_eq!(records.len(), 5);
    assert_eq!(records[0].purpose, EmbeddingPurpose::RetrievalQuery);
    assert_eq!(records[0].causal_reference_id, request_id.as_uuid());
    assert_eq!(records[1].purpose, EmbeddingPurpose::DimensionProbe);
    assert_eq!(records[1].batch_ordinal, Some(7));
    assert_eq!(records[2].purpose, EmbeddingPurpose::Backfill);
    assert_eq!(records[2].batch_ordinal, Some(7));
    assert_eq!(records[2].input_count, 2);
    assert_eq!(records[3].purpose, EmbeddingPurpose::Delivery);
    assert_eq!(records[3].delivery_attempt, Some(1));
    assert_eq!(records[4].purpose, EmbeddingPurpose::Delivery);
    assert_eq!(records[4].delivery_attempt, Some(2));
    assert_ne!(records[3].delivery_attempt, records[4].delivery_attempt);
}

#[derive(Default)]
struct Store {
    pending: Mutex<Vec<PendingEmbedding>>,
}

#[async_trait]
impl EmbeddingStore for Store {
    async fn ensure_space(
        &self,
        _: &RequestContext,
        name: &str,
        model: &str,
        dimensions: u32,
    ) -> Result<EmbeddingSpace, ApplicationError> {
        Ok(EmbeddingSpace {
            id: EmbeddingSpaceId::new(),
            name: name.into(),
            model: model.into(),
            dimensions,
        })
    }

    async fn memories_without_embedding(
        &self,
        _: &RequestContext,
        _: &EmbeddingSpace,
        _: u32,
    ) -> Result<Vec<PendingEmbedding>, ApplicationError> {
        Ok(std::mem::take(&mut *self.pending.lock().unwrap()))
    }

    async fn upsert(
        &self,
        _: &RequestContext,
        _: &EmbeddingSpace,
        _: MemoryId,
        _: &[f32],
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn missing_count(
        &self,
        _: &RequestContext,
        _: &EmbeddingSpace,
    ) -> Result<i64, ApplicationError> {
        Ok(0)
    }
}

struct Source(EmbeddingInput);

#[async_trait]
impl MemoryTextSource for Source {
    async fn active_text(
        &self,
        _: &RequestContext,
        _: MemoryId,
    ) -> Result<Option<EmbeddingInput>, ApplicationError> {
        Ok(Some(self.0.clone()))
    }
}

#[tokio::test]
async fn backfill_and_probe_share_the_rebuild_invocation_and_batch_cause() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        true,
        false,
    );
    let store = Arc::new(Store {
        pending: Mutex::new(vec![PendingEmbedding {
            memory_id: MemoryId::new(),
            content: "classified memory".into(),
            classification: Some("internal".into()),
        }]),
    });
    let backfill = EmbeddingBackfillService::new(harness.provider, store, "space");
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

    let report = backfill.run(&context, 32).await.unwrap();

    assert_eq!(report.embedded, 1);
    let records = harness.decisions.records.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].purpose, EmbeddingPurpose::DimensionProbe);
    assert_eq!(records[1].purpose, EmbeddingPurpose::Backfill);
    assert_eq!(
        records[0].causal_reference_id,
        records[1].causal_reference_id
    );
    assert_eq!(records[0].batch_ordinal, Some(1));
    assert_eq!(records[1].batch_ordinal, Some(1));
}

#[tokio::test]
async fn delivery_turns_prior_failures_into_distinguishable_attempt_numbers() {
    let harness = harness(
        DataDestination::LocalModel,
        EmbeddingDataPolicyMode::Enforce,
        &["internal"],
        false,
        false,
    );
    let store = Arc::new(Store::default());
    let source = Arc::new(Source(EmbeddingInput::new(
        "classified memory",
        Some("internal".into()),
    )));
    let handler =
        EmbedMemoryHandler::new(harness.provider, store, source, "space", "memory.created");
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let memory_id = MemoryId::new();
    let mut message = OutboxMessage::new(
        context.workspace_id,
        "memory.created",
        serde_json::json!({"memory_id": memory_id.as_uuid()}),
        now(),
    );

    handler.handle(&context, &message).await.unwrap();
    message.attempts = 1;
    handler.handle(&context, &message).await.unwrap();

    let records = harness.decisions.records.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].causal_reference_id, message.id.as_uuid());
    assert_eq!(records[1].causal_reference_id, message.id.as_uuid());
    assert_eq!(records[0].delivery_attempt, Some(1));
    assert_eq!(records[1].delivery_attempt, Some(2));
}
