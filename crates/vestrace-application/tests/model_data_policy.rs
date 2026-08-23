use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::run::{
    ProviderStepModelExecutor, StepModelExecutor, StepModelRequest, StepModelSettings,
};
use vestrace_application::{
    ApplicationError, ArtifactContent, ArtifactListing, ArtifactRepository, GenerationRequest,
    GenerationResponse, ModelDataPolicyDecisionRecord, ModelDataPolicyDecisionRepository,
    ModelDataPolicyMode, ModelDataPolicySettings, ModelExecutionRecord, ModelExecutionRepository,
    ModelRecord, ModelRepository, ProviderError, RequestContext, ResolvedTextGenerationProvider,
    StoredArtifact, TextGenerationProvider, TextGenerationProviderEgress,
    TextGenerationProviderFactory,
};
use vestrace_domain::id::{
    AgentRunId, ArtifactId, ArtifactRevisionId, ModelId, ProviderId, RunStepId,
};
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{DataDestination, DataPolicyId, PrincipalId, Sensitivity, WorkspaceId, now};

struct RecordingProvider {
    calls: Arc<Mutex<u32>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl TextGenerationProvider for RecordingProvider {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        *self.calls.lock().unwrap() += 1;
        self.events.lock().unwrap().push("provider");
        Ok(GenerationResponse {
            content: "completion".into(),
            model: request.model,
            prompt_tokens: 2,
            completion_tokens: 3,
        })
    }
}

struct Factory {
    provider: Arc<dyn TextGenerationProvider>,
    egress: TextGenerationProviderEgress,
}

#[async_trait]
impl TextGenerationProviderFactory for Factory {
    async fn provider_for(
        &self,
        _: &RequestContext,
    ) -> Result<ResolvedTextGenerationProvider, ApplicationError> {
        Ok(ResolvedTextGenerationProvider {
            provider: self.provider.clone(),
            egress: self.egress.clone(),
        })
    }
}

struct Models(ModelRecord);

#[async_trait]
impl ModelRepository for Models {
    async fn create(&self, _: &RequestContext, _: &ModelRecord) -> Result<(), ApplicationError> {
        unreachable!()
    }

    async fn list(&self, _: &RequestContext) -> Result<Vec<ModelRecord>, ApplicationError> {
        Ok(vec![self.0.clone()])
    }

    async fn find_by_id(
        &self,
        _: &RequestContext,
        _: ModelId,
    ) -> Result<Option<ModelRecord>, ApplicationError> {
        unreachable!()
    }
}

struct Artifacts;

#[async_trait]
impl ArtifactRepository for Artifacts {
    async fn list(
        &self,
        _: &RequestContext,
        _: u32,
    ) -> Result<Vec<ArtifactListing>, ApplicationError> {
        unreachable!()
    }

    async fn store(
        &self,
        _: &RequestContext,
        _: &str,
        content: &ArtifactContent,
    ) -> Result<StoredArtifact, ApplicationError> {
        Ok(StoredArtifact {
            artifact_id: ArtifactId::new(),
            revision_id: ArtifactRevisionId::new(),
            content_hash: content.content_hash(),
            byte_size: content.bytes.len() as u64,
        })
    }

    async fn fetch_content(
        &self,
        _: &RequestContext,
        _: &str,
    ) -> Result<Option<ArtifactContent>, ApplicationError> {
        unreachable!()
    }
}

#[derive(Default)]
struct Executions;

#[async_trait]
impl ModelExecutionRepository for Executions {
    async fn record(
        &self,
        _: &RequestContext,
        _: &ModelExecutionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn list(
        &self,
        _: &RequestContext,
    ) -> Result<Vec<ModelExecutionRecord>, ApplicationError> {
        Ok(vec![])
    }
}

#[derive(Default)]
struct Decisions {
    records: Mutex<Vec<ModelDataPolicyDecisionRecord>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl ModelDataPolicyDecisionRepository for Decisions {
    async fn record(&self, record: &ModelDataPolicyDecisionRecord) -> Result<(), ApplicationError> {
        self.records.lock().unwrap().push(record.clone());
        self.events.lock().unwrap().push("decision");
        Ok(())
    }
}

fn policy(mode: ModelDataPolicyMode) -> ModelDataPolicySettings {
    ModelDataPolicySettings {
        policy: DataPolicy::new(
            DataPolicyId::new(),
            "data-policy-v7",
            Sensitivity::Confidential,
            BTreeSet::from([DataDestination::LocalModel]),
            None,
        )
        .unwrap(),
        classification: Sensitivity::Confidential,
        mode,
    }
}

struct ExecutorHarness {
    executor: ProviderStepModelExecutor,
    calls: Arc<Mutex<u32>>,
    decisions: Arc<Decisions>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

fn executor(destination: DataDestination, mode: ModelDataPolicyMode) -> ExecutorHarness {
    let calls = Arc::new(Mutex::new(0));
    let events = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(RecordingProvider {
        calls: calls.clone(),
        events: events.clone(),
    });
    let factory = Arc::new(Factory {
        provider,
        egress: TextGenerationProviderEgress::new(
            if destination == DataDestination::LocalModel {
                "http://localhost:12345/v1"
            } else {
                "https://models.example.test/v1"
            },
            destination,
            true,
            true,
        ),
    });
    let workspace_id = WorkspaceId::new();
    let models = Arc::new(Models(ModelRecord {
        id: ModelId::new(),
        provider_id: ProviderId::new(),
        workspace_id,
        model_name: "prism-ml/bonsai-27b".into(),
        context_window: 4096,
        input_cost_per_mtoken: 0.0,
        output_cost_per_mtoken: 0.0,
        created_at: now(),
    }));
    let decisions = Arc::new(Decisions {
        records: Mutex::new(Vec::new()),
        events: events.clone(),
    });
    let executor = ProviderStepModelExecutor::new(
        factory,
        models,
        Arc::new(Artifacts),
        Arc::new(Executions),
        decisions.clone(),
        StepModelSettings {
            model_name: "prism-ml/bonsai-27b".into(),
            max_tokens: Some(4),
        },
        policy(mode),
    );
    ExecutorHarness {
        executor,
        calls,
        decisions,
        events,
    }
}

fn request() -> (RequestContext, StepModelRequest) {
    (
        RequestContext::new(WorkspaceId::new(), PrincipalId::new()),
        StepModelRequest {
            run_id: AgentRunId::new(),
            step_id: RunStepId::new(),
            objective: "confidential objective".into(),
        },
    )
}

#[tokio::test]
async fn enforce_records_a_remote_denial_and_never_calls_the_provider() {
    let ExecutorHarness {
        executor,
        calls,
        decisions,
        events,
    } = executor(
        DataDestination::RemoteProvider,
        ModelDataPolicyMode::Enforce,
    );
    let (context, request) = request();

    let error = executor
        .execute(&context, request.clone())
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Policy(_)));
    assert!(error.to_string().contains("RemoteProvider"), "{error}");
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(*events.lock().unwrap(), vec!["decision"]);
    let records = decisions.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.run_id, request.run_id);
    assert_eq!(record.step_id, request.step_id);
    assert_eq!(record.destination, DataDestination::RemoteProvider);
    assert_eq!(record.classification, Sensitivity::Confidential);
    assert!(!record.allowed);
    assert_eq!(record.policy_version, "data-policy-v7");
    assert_eq!(record.mode, ModelDataPolicyMode::Enforce);
    assert!(
        record.reason.contains("RemoteProvider"),
        "{}",
        record.reason
    );
}

#[tokio::test]
async fn observe_records_the_denial_before_calling_the_provider() {
    let ExecutorHarness {
        executor,
        calls,
        decisions,
        events,
    } = executor(
        DataDestination::RemoteProvider,
        ModelDataPolicyMode::Observe,
    );
    let (context, request) = request();

    executor.execute(&context, request).await.unwrap();

    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(*events.lock().unwrap(), vec!["decision", "provider"]);
    let records = decisions.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].allowed);
    assert_eq!(records[0].mode, ModelDataPolicyMode::Observe);
}

#[tokio::test]
async fn an_allowance_is_recorded_before_the_local_provider_is_called() {
    let ExecutorHarness {
        executor,
        calls,
        decisions,
        events,
    } = executor(DataDestination::LocalModel, ModelDataPolicyMode::Enforce);
    let (context, request) = request();

    executor.execute(&context, request).await.unwrap();

    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(*events.lock().unwrap(), vec!["decision", "provider"]);
    let records = decisions.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].allowed);
    assert_eq!(records[0].policy_version, "data-policy-v7");
    assert_eq!(records[0].destination, DataDestination::LocalModel);
}
