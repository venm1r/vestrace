use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use tokio::sync::Mutex;
use vestrace_application::{
    ApplicationError, AuthorizationBoundary, ConfiguredCapabilityPolicyEngine,
    ExternalEffectReadBackAdapter, ExternalEffectReadBackRegistry, ExternalEffectRecoveryCandidate,
    ExternalEffectRecoveryError, ExternalEffectRecoveryService, ExternalEffectRepository,
    ExternalEffectService, PerformExternalEffectService, RequestContext,
};
use vestrace_domain::external_effects::{
    AdapterDispatchResult, AdapterError, DeliverySemantics, DryRunMode, EffectLifecycleStatus,
    EffectPrecondition, EffectReversibility, EvidenceStrength, ExternalEffectAdapter,
    ExternalEffectAdapterDescriptor, ExternalEffectIntent, ExternalEffectReceipt,
    ExternalReconciliation, IdempotencyProfile, ObservedEffectState, ReconciliationOutcome,
};
use vestrace_domain::{
    AuthorizationRequest, Capability, ExternalEffectId, ExternalEffectReceiptId,
    ExternalReconciliationId, PrincipalId, RiskCategory, WorkerId, WorkspaceId,
};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

/// A distinct run for a fixture effect to belong to.
///
/// These were labels — `"current"`, `"other"` — which is what `execution_ref`
/// accepted before it had to be a reference anything could follow.
fn run_ref() -> String {
    format!("run://{}", vestrace_domain::id::AgentRunId::new())
}

fn intent(
    workspace_id: WorkspaceId,
    actor_id: PrincipalId,
    execution_ref: &str,
) -> ExternalEffectIntent {
    intent_for_adapter(workspace_id, actor_id, execution_ref, "webhook-v1")
}

fn intent_for_adapter(
    workspace_id: WorkspaceId,
    actor_id: PrincipalId,
    execution_ref: &str,
    adapter: &str,
) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        workspace_id,
        actor_id,
        adapter,
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
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap()
}

fn receipt_with_status(
    intent: &ExternalEffectIntent,
    status: EffectLifecycleStatus,
    recorded_at: vestrace_domain::Timestamp,
) -> ExternalEffectReceipt {
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        recorded_at,
        vec![format!("evidence:{status:?}")],
    )
    .unwrap();
    let mut payload = serde_json::to_value(receipt).unwrap();
    payload["outcome_status"] = serde_json::to_value(status).unwrap();
    payload["response_class"] = serde_json::json!(format!("{status:?}"));
    serde_json::from_value(payload).unwrap()
}

fn performable_intent(workspace_id: WorkspaceId, actor_id: PrincipalId) -> ExternalEffectIntent {
    let mut payload = serde_json::to_value(intent(workspace_id, actor_id, &run_ref())).unwrap();
    payload["policy_decision_ref"] = serde_json::Value::Null;
    serde_json::from_value(payload).unwrap()
}

#[derive(Default)]
struct MemoryEffectRepository {
    intents: Mutex<Vec<ExternalEffectIntent>>,
    receipts: Mutex<Vec<ExternalEffectReceipt>>,
    transitions: Mutex<Vec<MemoryLifecycleTransition>>,
    reconciled: Mutex<Vec<ExternalReconciliation>>,
    reconciled_keys: Mutex<Vec<(ExternalEffectId, Option<ExternalEffectReceiptId>)>>,
    delivered: Mutex<Vec<ExternalReconciliationId>>,
    dispatch_started_signal: std::sync::Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
}

#[derive(Clone, Copy)]
struct MemoryLifecycleTransition {
    workspace_id: WorkspaceId,
    effect_id: ExternalEffectId,
    status: EffectLifecycleStatus,
    recorded_at: vestrace_domain::Timestamp,
    dispatch_owner: Option<WorkerId>,
    dispatch_expires_at: Option<vestrace_domain::Timestamp>,
}

impl MemoryEffectRepository {
    fn signal_dispatch_started_with(&self, signal: Arc<std::sync::atomic::AtomicBool>) {
        *self.dispatch_started_signal.lock().unwrap() = Some(signal);
    }

    async fn seed(&self, candidate: ExternalEffectRecoveryCandidate) {
        let context = RequestContext::new(
            candidate.intent().workspace_id(),
            candidate.intent().actor_id(),
        );
        self.insert_intent(&context, candidate.intent())
            .await
            .unwrap();
        if let Some(receipt) = candidate.receipt() {
            self.insert_receipt(&context, receipt).await.unwrap();
        } else {
            self.record_dispatch_started(
                &context,
                candidate.intent().id(),
                WorkerId::new(),
                at(21),
                at(20),
            )
            .await
            .unwrap();
        }
    }

    async fn seed_deadline_less_dispatch(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        recorded_at: vestrace_domain::Timestamp,
    ) -> Result<(), ApplicationError> {
        if !self
            .intents
            .lock()
            .await
            .iter()
            .any(|intent| intent.id() == effect_id && intent.workspace_id() == context.workspace_id)
        {
            return Err(ApplicationError::Storage(
                "effect evidence not found".into(),
            ));
        }
        self.transitions
            .lock()
            .await
            .push(MemoryLifecycleTransition {
                workspace_id: context.workspace_id,
                effect_id,
                status: EffectLifecycleStatus::Dispatching,
                recorded_at,
                dispatch_owner: None,
                dispatch_expires_at: None,
            });
        Ok(())
    }

    async fn dispatch_evidence(
        &self,
        effect_id: ExternalEffectId,
    ) -> Vec<(
        WorkerId,
        vestrace_domain::Timestamp,
        vestrace_domain::Timestamp,
    )> {
        self.transitions
            .lock()
            .await
            .iter()
            .filter(|transition| transition.effect_id == effect_id)
            .filter_map(|transition| {
                Some((
                    transition.dispatch_owner?,
                    transition.dispatch_expires_at?,
                    transition.recorded_at,
                ))
            })
            .collect()
    }

    async fn reconciled_count(&self) -> usize {
        self.reconciled.lock().await.len()
    }

    async fn mark_reconciled(&self, candidate: &ExternalEffectRecoveryCandidate) {
        self.reconciled_keys.lock().await.push((
            candidate.intent().id(),
            candidate.receipt().map(ExternalEffectReceipt::id),
        ));
    }
}

#[async_trait]
impl ExternalEffectRepository for MemoryEffectRepository {
    async fn insert_intent(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError> {
        if intent.workspace_id() != context.workspace_id {
            return Err(ApplicationError::Policy("workspace mismatch".into()));
        }
        let mut intents = self.intents.lock().await;
        if let Some(existing) = intents.iter().find(|item| item.id() == intent.id()) {
            return if existing == intent {
                Ok(())
            } else {
                Err(ApplicationError::Conflict(format!(
                    "external effect intent id {} already contains different evidence",
                    intent.id()
                )))
            };
        }
        intents.push(intent.clone());
        drop(intents);
        self.transitions
            .lock()
            .await
            .push(MemoryLifecycleTransition {
                workspace_id: context.workspace_id,
                effect_id: intent.id(),
                status: EffectLifecycleStatus::Prepared,
                recorded_at: intent.created_at(),
                dispatch_owner: None,
                dispatch_expires_at: None,
            });
        Ok(())
    }

    async fn find_intent(
        &self,
        context: &RequestContext,
        id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError> {
        Ok(self
            .intents
            .lock()
            .await
            .iter()
            .find(|item| item.id() == id && item.workspace_id() == context.workspace_id)
            .cloned())
    }

    async fn insert_receipt(
        &self,
        context: &RequestContext,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        if !self.intents.lock().await.iter().any(|item| {
            item.id() == receipt.effect_id() && item.workspace_id() == context.workspace_id
        }) {
            return Err(ApplicationError::Storage(
                "effect evidence not found".into(),
            ));
        }
        let mut receipts = self.receipts.lock().await;
        if let Some(existing) = receipts.iter().find(|item| item.id() == receipt.id()) {
            return if existing == receipt {
                Ok(())
            } else {
                Err(ApplicationError::Conflict(format!(
                    "external effect receipt id {} already contains different evidence",
                    receipt.id()
                )))
            };
        }
        receipts.push(receipt.clone());
        drop(receipts);
        self.transitions
            .lock()
            .await
            .push(MemoryLifecycleTransition {
                workspace_id: context.workspace_id,
                effect_id: receipt.effect_id(),
                status: receipt.outcome_status(),
                recorded_at: receipt.recorded_at(),
                dispatch_owner: None,
                dispatch_expires_at: None,
            });
        Ok(())
    }

    async fn find_receipt(
        &self,
        context: &RequestContext,
        id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        let intents = self.intents.lock().await;
        Ok(self
            .receipts
            .lock()
            .await
            .iter()
            .find(|receipt| {
                receipt.id() == id
                    && intents.iter().any(|intent| {
                        intent.id() == receipt.effect_id()
                            && intent.workspace_id() == context.workspace_id
                    })
            })
            .cloned())
    }

    async fn find_receipt_by_effect(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        if !self
            .intents
            .lock()
            .await
            .iter()
            .any(|intent| intent.id() == effect_id && intent.workspace_id() == context.workspace_id)
        {
            return Ok(None);
        }
        Ok(self
            .receipts
            .lock()
            .await
            .iter()
            .filter(|receipt| receipt.effect_id() == effect_id)
            .max_by_key(|receipt| receipt.recorded_at())
            .cloned())
    }

    async fn record_dispatch_started(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
        dispatch_owner: WorkerId,
        dispatch_expires_at: vestrace_domain::Timestamp,
        recorded_at: vestrace_domain::Timestamp,
    ) -> Result<(), ApplicationError> {
        if !self
            .intents
            .lock()
            .await
            .iter()
            .any(|intent| intent.id() == effect_id && intent.workspace_id() == context.workspace_id)
        {
            return Err(ApplicationError::Storage(
                "effect evidence not found".into(),
            ));
        }
        self.transitions
            .lock()
            .await
            .push(MemoryLifecycleTransition {
                workspace_id: context.workspace_id,
                effect_id,
                status: EffectLifecycleStatus::Dispatching,
                recorded_at,
                dispatch_owner: Some(dispatch_owner),
                dispatch_expires_at: Some(dispatch_expires_at),
            });
        if let Some(signal) = self.dispatch_started_signal.lock().unwrap().as_ref() {
            signal.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    async fn find_lifecycle_status(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<Option<EffectLifecycleStatus>, ApplicationError> {
        Ok(self
            .transitions
            .lock()
            .await
            .iter()
            .filter(|transition| {
                transition.workspace_id == context.workspace_id && transition.effect_id == effect_id
            })
            .max_by_key(|transition| transition.recorded_at)
            .map(|transition| transition.status))
    }

    async fn count_deadline_less_dispatching_transitions(
        &self,
        context: &RequestContext,
    ) -> Result<u64, ApplicationError> {
        Ok(self
            .transitions
            .lock()
            .await
            .iter()
            .filter(|transition| {
                transition.workspace_id == context.workspace_id
                    && transition.status == EffectLifecycleStatus::Dispatching
                    && transition.dispatch_expires_at.is_none()
            })
            .count() as u64)
    }

    async fn insert_reconciliation(
        &self,
        context: &RequestContext,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError> {
        if !self.intents.lock().await.iter().any(|intent| {
            intent.id() == reconciliation.effect_id()
                && intent.workspace_id() == context.workspace_id
        }) {
            return Err(ApplicationError::Storage(
                "effect evidence not found".into(),
            ));
        }
        if let Some(receipt_id) = reconciliation.receipt_id() {
            if !self.receipts.lock().await.iter().any(|receipt| {
                receipt.id() == receipt_id && receipt.effect_id() == reconciliation.effect_id()
            }) {
                return Err(ApplicationError::Storage(
                    "effect evidence not found".into(),
                ));
            }
        }
        if !self
            .reconciled
            .lock()
            .await
            .iter()
            .any(|item| item.id() == reconciliation.id())
        {
            self.reconciled.lock().await.push(reconciliation.clone());
            if reconciliation.outcome().is_settled() {
                self.transitions
                    .lock()
                    .await
                    .push(MemoryLifecycleTransition {
                        workspace_id: context.workspace_id,
                        effect_id: reconciliation.effect_id(),
                        status: EffectLifecycleStatus::Reconciling,
                        recorded_at: reconciliation.reconciled_at(),
                        dispatch_owner: None,
                        dispatch_expires_at: None,
                    });
            }
        }
        Ok(())
    }

    async fn find_reconciliation(
        &self,
        context: &RequestContext,
        id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError> {
        let intents = self.intents.lock().await;
        Ok(self
            .reconciled
            .lock()
            .await
            .iter()
            .find(|item| {
                item.id() == id
                    && intents.iter().any(|intent| {
                        intent.id() == item.effect_id()
                            && intent.workspace_id() == context.workspace_id
                    })
            })
            .cloned())
    }

    /// The delivery debt is not what this fixture is about.
    ///
    /// Returning nothing owed is honest here rather than lazy: this file tests
    /// the sweep, and a fake that invented debts would make the sweep's tests
    /// depend on delivery semantics they do not exercise. Delivery has its own
    /// test against real storage.
    async fn find_undelivered_outcomes(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<vestrace_application::UndeliveredOutcome>, ApplicationError> {
        let intents = self.intents.lock().await;
        let delivered = self.delivered.lock().await;
        Ok(self
            .reconciled
            .lock()
            .await
            .iter()
            .filter(|item| item.outcome().is_settled() && !delivered.contains(&item.id()))
            .filter_map(|item| {
                intents
                    .iter()
                    .find(|intent| {
                        intent.id() == item.effect_id()
                            && intent.workspace_id() == context.workspace_id
                    })
                    .map(|intent| vestrace_application::UndeliveredOutcome {
                        reconciliation: item.clone(),
                        intent: intent.clone(),
                    })
            })
            .take(limit as usize)
            .collect())
    }

    async fn mark_outcome_delivered(
        &self,
        context: &RequestContext,
        reconciliation_id: ExternalReconciliationId,
        at: vestrace_domain::Timestamp,
    ) -> Result<(), ApplicationError> {
        if self.delivered.lock().await.contains(&reconciliation_id) {
            return Ok(());
        }
        let reconciliation = self
            .find_reconciliation(context, reconciliation_id)
            .await?
            .ok_or_else(|| ApplicationError::Storage("effect evidence not found".into()))?;
        let status = match reconciliation.outcome() {
            ReconciliationOutcome::Confirmed => EffectLifecycleStatus::Confirmed,
            ReconciliationOutcome::NotApplied => EffectLifecycleStatus::Failed,
            _ => return Err(ApplicationError::Storage("outcome is not settled".into())),
        };
        self.delivered.lock().await.push(reconciliation_id);
        self.transitions
            .lock()
            .await
            .push(MemoryLifecycleTransition {
                workspace_id: context.workspace_id,
                effect_id: reconciliation.effect_id(),
                status,
                recorded_at: at,
                dispatch_owner: None,
                dispatch_expires_at: None,
            });
        Ok(())
    }

    /// Mirrors the stored query: an effect leaves the sweep when something
    /// **settled** it, not merely when somebody asked about it.
    async fn find_reconciliation_candidates(
        &self,
        context: &RequestContext,
        retry_unsettled_before: vestrace_domain::Timestamp,
        dispatch_expired_before: vestrace_domain::Timestamp,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let workspace_id = context.workspace_id;
        let reconciled = self.reconciled.lock().await;
        let reconciled_keys = self.reconciled_keys.lock().await;
        let intents = self.intents.lock().await.clone();
        let receipts = self.receipts.lock().await.clone();
        let transitions = self.transitions.lock().await.clone();
        let candidates = intents
            .into_iter()
            .filter(|intent| intent.workspace_id() == workspace_id)
            .flat_map(|intent| {
                let effect_receipts = receipts
                    .iter()
                    .filter(|receipt| receipt.effect_id() == intent.id())
                    .collect::<Vec<_>>();
                let mut candidates = effect_receipts
                    .iter()
                    .filter(|receipt| receipt.requires_reconciliation())
                    .map(|receipt| {
                        ExternalEffectRecoveryCandidate::new(
                            intent.clone(),
                            Some((*receipt).clone()),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                let latest_status = transitions
                    .iter()
                    .filter(|item| {
                        item.workspace_id == workspace_id && item.effect_id == intent.id()
                    })
                    .max_by_key(|item| item.recorded_at);
                if effect_receipts.is_empty()
                    && latest_status.is_some_and(|item| {
                        item.status == EffectLifecycleStatus::Dispatching
                            && item
                                .dispatch_expires_at
                                .is_some_and(|deadline| deadline < dispatch_expired_before)
                    })
                {
                    candidates.push(ExternalEffectRecoveryCandidate::new(intent, None).unwrap());
                }
                candidates
            })
            .filter(|candidate| {
                let latest = reconciled
                    .iter()
                    .filter(|item| {
                        item.effect_id() == candidate.intent().id()
                            && item.receipt_id()
                                == candidate.receipt().map(ExternalEffectReceipt::id)
                    })
                    .max_by_key(|item| item.reconciled_at());
                let still_open = match latest {
                    None => true,
                    Some(item) => {
                        !item.outcome().is_settled()
                            && item.reconciled_at() < retry_unsettled_before
                    }
                };
                still_open
                    && !reconciled_keys.contains(&(
                        candidate.intent().id(),
                        candidate.receipt().map(ExternalEffectReceipt::id),
                    ))
            })
            .collect::<Vec<_>>();
        Ok(candidates)
    }
}

struct MemoryReadBack {
    endpoint: &'static str,
    requests: Arc<Mutex<Vec<&'static str>>>,
    receipts: Arc<Mutex<Vec<Option<ExternalEffectReceiptId>>>>,
    observations: Vec<ObservedEffectState>,
}

#[async_trait]
impl ExternalEffectReadBackAdapter for MemoryReadBack {
    async fn observe(
        &self,
        _intent: &ExternalEffectIntent,
        receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        self.requests.lock().await.push(self.endpoint);
        self.receipts
            .lock()
            .await
            .push(receipt.map(ExternalEffectReceipt::id));
        Ok(self.observations.clone())
    }
}

fn read_back_registry(
    name: &str,
    adapter: Arc<dyn ExternalEffectReadBackAdapter>,
) -> ExternalEffectReadBackRegistry {
    ExternalEffectReadBackRegistry::new([(name.to_owned(), adapter)]).unwrap()
}

/// An adapter's answer is evidence only about effects that adapter owns.
///
/// This used to ask the first configured endpoint about every effect. A
/// confident answer from that endpoint could therefore settle an effect sent
/// somewhere else, making a routing mistake durable evidence.
#[tokio::test]
async fn recovery_asks_only_the_adapter_named_by_the_effect() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent_for_adapter(workspace_id, actor_id, &run_ref(), "second");
    repository
        .seed(
            ExternalEffectRecoveryCandidate::new(effect.clone(), unknown_receipt(&effect)).unwrap(),
        )
        .await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let first_read_back = Arc::new(MemoryReadBack {
        endpoint: "https://first.effects.test/read-back",
        requests: Arc::clone(&requests),
        receipts: Arc::new(Mutex::new(Vec::new())),
        observations: vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:first",
            vec!["evidence:first".into()],
        )],
    });
    let second_read_back = Arc::new(MemoryReadBack {
        endpoint: "https://second.effects.test/read-back",
        requests: Arc::clone(&requests),
        receipts: Arc::new(Mutex::new(Vec::new())),
        observations: vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:second",
            vec!["evidence:second".into()],
        )],
    });
    let registry = ExternalEffectReadBackRegistry::new([
        (
            "first".to_owned(),
            first_read_back as Arc<dyn ExternalEffectReadBackAdapter>,
        ),
        (
            "second".to_owned(),
            second_read_back as Arc<dyn ExternalEffectReadBackAdapter>,
        ),
    ])
    .unwrap();
    let service = ExternalEffectRecoveryService::new(repository, registry);
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30), at(30)).await.unwrap();

    let requests = requests.lock().await;
    assert!(!requests.contains(&"https://first.effects.test/read-back"));
    assert_eq!(
        &*requests,
        &["https://second.effects.test/read-back"],
        "the effect was not asked about at exactly its adapter's endpoint"
    );
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(
        report.reconciliations()[0].observed_state_ref(),
        "external:second"
    );
}

#[test]
fn read_back_registry_rejects_duplicate_adapter_names() {
    let adapter: Arc<dyn ExternalEffectReadBackAdapter> = Arc::new(MemoryReadBack {
        endpoint: "https://duplicate.effects.test/read-back",
        requests: Arc::new(Mutex::new(Vec::new())),
        receipts: Arc::new(Mutex::new(Vec::new())),
        observations: Vec::new(),
    });

    let error = match ExternalEffectReadBackRegistry::new([
        ("duplicate".to_owned(), Arc::clone(&adapter)),
        ("duplicate".to_owned(), adapter),
    ]) {
        Ok(_) => panic!("duplicate adapter names were accepted"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("duplicate"),
        "duplicate-name error did not name the collision: {error}"
    );
}

#[tokio::test]
async fn unregistered_adapter_is_unreachable_without_settling_the_effect() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent_for_adapter(workspace_id, actor_id, &run_ref(), "missing");
    let candidate =
        ExternalEffectRecoveryCandidate::new(effect.clone(), unknown_receipt(&effect)).unwrap();
    repository.seed(candidate.clone()).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let registry = read_back_registry(
        "registered",
        Arc::new(MemoryReadBack {
            endpoint: "https://registered.effects.test/read-back",
            requests: Arc::clone(&requests),
            receipts: Arc::new(Mutex::new(Vec::new())),
            observations: vec![ObservedEffectState::new(
                EvidenceStrength::ProviderIdempotencyLookup,
                Some(true),
                "external:registered",
                vec!["evidence:registered".into()],
            )],
        }),
    );
    let service = ExternalEffectRecoveryService::new(repository.clone(), registry);
    let context = RequestContext::new(workspace_id, actor_id);

    assert!(matches!(
        service.reconcile_candidate(&context, &candidate, at(29)).await,
        Err(ExternalEffectRecoveryError::MissingReadBackRoute { adapter })
            if adapter == "missing"
    ));

    let report = service.run(&context, at(30), at(30), at(30)).await.unwrap();

    assert!(report.reconciliations().is_empty());
    assert_eq!(repository.reconciled_count().await, 0);
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Unknown)
    );
    assert!(requests.lock().await.is_empty());
    assert_eq!(report.unreachable().len(), 1);
    assert_eq!(report.unreachable()[0].effect_id, effect.id());
    assert!(
        report.unreachable()[0].reason.contains("missing"),
        "no-route reason did not name the adapter: {}",
        report.unreachable()[0].reason
    );

    let next_report = service.run(&context, at(31), at(31), at(31)).await.unwrap();
    assert_eq!(next_report.unreachable().len(), 1);
    assert_eq!(next_report.unreachable()[0].effect_id, effect.id());
    assert!(requests.lock().await.is_empty());
}

#[tokio::test]
async fn single_registered_adapter_reconciles_only_its_own_effects() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let registered = intent_for_adapter(workspace_id, actor_id, &run_ref(), "registered");
    let unregistered = intent_for_adapter(workspace_id, actor_id, &run_ref(), "unregistered");
    for effect in [&registered, &unregistered] {
        repository
            .seed(
                ExternalEffectRecoveryCandidate::new(effect.clone(), unknown_receipt(effect))
                    .unwrap(),
            )
            .await;
    }
    let requests = Arc::new(Mutex::new(Vec::new()));
    let registry = read_back_registry(
        "registered",
        Arc::new(MemoryReadBack {
            endpoint: "https://registered.effects.test/read-back",
            requests: Arc::clone(&requests),
            receipts: Arc::new(Mutex::new(Vec::new())),
            observations: vec![ObservedEffectState::new(
                EvidenceStrength::ProviderIdempotencyLookup,
                Some(true),
                "external:registered",
                vec!["evidence:registered".into()],
            )],
        }),
    );
    let service = ExternalEffectRecoveryService::new(repository.clone(), registry);
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30), at(30)).await.unwrap();

    assert_eq!(
        &*requests.lock().await,
        &["https://registered.effects.test/read-back"]
    );
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(report.reconciliations()[0].effect_id(), registered.id());
    assert_eq!(report.unreachable().len(), 1);
    assert_eq!(report.unreachable()[0].effect_id, unregistered.id());
    assert!(report.unreachable()[0].reason.contains("unregistered"));
}

#[tokio::test]
async fn every_unknown_receipt_remains_a_candidate_regardless_of_mixed_receipt_insertion_order() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = MemoryEffectRepository::default();
    let context = RequestContext::new(workspace_id, actor_id);
    let unknown_then_acknowledged = intent(workspace_id, actor_id, &run_ref());
    let acknowledged_then_unknown = intent(workspace_id, actor_id, &run_ref());

    for effect in [&unknown_then_acknowledged, &acknowledged_then_unknown] {
        repository.insert_intent(&context, effect).await.unwrap();
    }
    let first_unknown = receipt_with_status(
        &unknown_then_acknowledged,
        EffectLifecycleStatus::Unknown,
        at(20),
    );
    let first_acknowledged = receipt_with_status(
        &unknown_then_acknowledged,
        EffectLifecycleStatus::Acknowledged,
        at(30),
    );
    repository
        .insert_receipt(&context, &first_unknown)
        .await
        .unwrap();
    repository
        .insert_receipt(&context, &first_acknowledged)
        .await
        .unwrap();

    let second_acknowledged = receipt_with_status(
        &acknowledged_then_unknown,
        EffectLifecycleStatus::Acknowledged,
        at(30),
    );
    let second_unknown = receipt_with_status(
        &acknowledged_then_unknown,
        EffectLifecycleStatus::Unknown,
        at(20),
    );
    repository
        .insert_receipt(&context, &second_acknowledged)
        .await
        .unwrap();
    repository
        .insert_receipt(&context, &second_unknown)
        .await
        .unwrap();

    let mut candidate_receipts = repository
        .find_reconciliation_candidates(&context, at(100), at(100))
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.receipt().unwrap().id().to_string())
        .collect::<Vec<_>>();
    candidate_receipts.sort();
    let mut expected = vec![
        first_unknown.id().to_string(),
        second_unknown.id().to_string(),
    ];
    expected.sort();
    assert_eq!(candidate_receipts, expected);
}

#[tokio::test]
async fn multiple_unknown_receipts_for_one_effect_are_distinct_candidates() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = MemoryEffectRepository::default();
    let context = RequestContext::new(workspace_id, actor_id);
    let effect = intent(workspace_id, actor_id, &run_ref());
    repository.insert_intent(&context, &effect).await.unwrap();
    let first = receipt_with_status(&effect, EffectLifecycleStatus::Unknown, at(20));
    let second = receipt_with_status(&effect, EffectLifecycleStatus::Unknown, at(30));
    repository.insert_receipt(&context, &first).await.unwrap();
    repository.insert_receipt(&context, &second).await.unwrap();

    let mut candidate_receipts = repository
        .find_reconciliation_candidates(&context, at(100), at(100))
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.receipt().unwrap().id().to_string())
        .collect::<Vec<_>>();
    candidate_receipts.sort();
    let mut expected = vec![first.id().to_string(), second.id().to_string()];
    expected.sort();
    assert_eq!(candidate_receipts, expected);
}

#[tokio::test]
async fn memory_repository_rejects_conflicting_duplicate_intent_and_receipt_evidence() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = MemoryEffectRepository::default();
    let context = RequestContext::new(workspace_id, actor_id);
    let effect = intent(workspace_id, actor_id, &run_ref());
    repository.insert_intent(&context, &effect).await.unwrap();
    let mut conflicting_intent_payload = serde_json::to_value(&effect).unwrap();
    conflicting_intent_payload["expected_effect"] = serde_json::json!("different effect");
    let conflicting_intent: ExternalEffectIntent =
        serde_json::from_value(conflicting_intent_payload).unwrap();
    assert!(matches!(
        repository
            .insert_intent(&context, &conflicting_intent)
            .await,
        Err(ApplicationError::Conflict(_))
    ));

    let receipt = unknown_receipt(&effect);
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let mut conflicting_receipt_payload = serde_json::to_value(&receipt).unwrap();
    conflicting_receipt_payload["response_class"] = serde_json::json!("different response");
    let conflicting_receipt: ExternalEffectReceipt =
        serde_json::from_value(conflicting_receipt_payload).unwrap();
    assert!(matches!(
        repository
            .insert_receipt(&context, &conflicting_receipt)
            .await,
        Err(ApplicationError::Conflict(_))
    ));
    assert_eq!(repository.transitions.lock().await.len(), 2);
}

#[tokio::test]
async fn memory_repository_rejects_cross_workspace_reconciliation_before_append() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = MemoryEffectRepository::default();
    let owner = RequestContext::new(workspace_id, actor_id);
    let stranger = RequestContext::new(WorkspaceId::new(), actor_id);
    let effect = intent(workspace_id, actor_id, &run_ref());
    repository.insert_intent(&owner, &effect).await.unwrap();
    let reconciliation = vestrace_domain::external_effects::reconcile_effect(
        &effect,
        None,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    assert!(
        repository
            .insert_reconciliation(&stranger, &reconciliation)
            .await
            .is_err()
    );
    assert_eq!(repository.reconciled_count().await, 0);
    assert_eq!(repository.transitions.lock().await.len(), 1);
}

#[tokio::test]
async fn discovery_is_workspace_scoped_and_excludes_already_reconciled_records() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let current = intent(workspace_id, actor_id, &run_ref());
    let resolved = intent(workspace_id, actor_id, &run_ref());
    let other = intent(WorkspaceId::new(), actor_id, &run_ref());

    repository
        .seed(
            ExternalEffectRecoveryCandidate::new(current.clone(), unknown_receipt(&current))
                .unwrap(),
        )
        .await;
    let resolved_candidate =
        ExternalEffectRecoveryCandidate::new(resolved.clone(), unknown_receipt(&resolved)).unwrap();
    repository.seed(resolved_candidate.clone()).await;
    repository.mark_reconciled(&resolved_candidate).await;
    repository
        .seed(ExternalEffectRecoveryCandidate::new(other.clone(), unknown_receipt(&other)).unwrap())
        .await;

    let read_back = Arc::new(MemoryReadBack {
        endpoint: "https://webhook.effects.test/read-back",
        requests: Arc::new(Mutex::new(Vec::new())),
        receipts: Arc::new(Mutex::new(Vec::new())),
        observations: vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:resolved",
            vec!["evidence:provider".into()],
        )],
    });
    let service = ExternalEffectRecoveryService::new(
        repository.clone(),
        read_back_registry("webhook-v1", read_back),
    );
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30), at(30)).await.unwrap();
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(
        report.reconciliations()[0].outcome(),
        ReconciliationOutcome::Confirmed
    );
    assert_eq!(repository.reconciled_count().await, 1);

    let second_report = service.run(&context, at(31), at(31), at(31)).await.unwrap();
    assert!(second_report.reconciliations().is_empty());
}

/// A read-back that observed nothing settles nothing, and says so.
///
/// # What changed here, and why
///
/// This asserted that the whole sweep returned `Err`. The property it exists to
/// protect is that **no reconciliation is persisted from zero observations** —
/// "we asked and learned nothing" must not be written down as a conclusion —
/// and that still holds. What changed is the blast radius: one candidate whose
/// endpoint says nothing used to abort the sweep, leaving every other unknown
/// effect in the workspace unreconciled, including ones whose endpoints were
/// answering. That is the same shape as a drain that stops at its first failed
/// message.
///
/// The candidate is now reported by identity and reason, and stays a candidate.
#[tokio::test]
async fn read_back_without_observations_settles_nothing_and_reports_it() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent(workspace_id, actor_id, &run_ref());
    repository
        .seed(
            ExternalEffectRecoveryCandidate::new(effect.clone(), unknown_receipt(&effect)).unwrap(),
        )
        .await;

    let read_back = Arc::new(MemoryReadBack {
        endpoint: "https://webhook.effects.test/read-back",
        requests: Arc::new(Mutex::new(Vec::new())),
        receipts: Arc::new(Mutex::new(Vec::new())),
        observations: Vec::new(),
    });
    let service = ExternalEffectRecoveryService::new(
        repository.clone(),
        read_back_registry("webhook-v1", read_back),
    );
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30), at(30)).await.unwrap();

    // Nothing was concluded, and nothing was written.
    assert!(report.reconciliations().is_empty());
    assert_eq!(repository.reconciled_count().await, 0);

    // And the failure is not swallowed: it names the effect and why.
    assert_eq!(report.unreachable().len(), 1);
    assert_eq!(report.unreachable()[0].effect_id, effect.id());
    assert!(
        report.unreachable()[0]
            .reason
            .contains("at least one observation"),
        "{}",
        report.unreachable()[0].reason
    );
}

#[tokio::test]
async fn a_lost_dispatch_without_a_receipt_reconciles_end_to_end() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent(workspace_id, actor_id, &run_ref());
    let context = RequestContext::new(workspace_id, actor_id);
    repository.insert_intent(&context, &effect).await.unwrap();
    repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();

    let observed_receipts = Arc::new(Mutex::new(Vec::new()));
    let read_back = Arc::new(MemoryReadBack {
        endpoint: "https://webhook.effects.test/read-back",
        requests: Arc::new(Mutex::new(Vec::new())),
        receipts: Arc::clone(&observed_receipts),
        observations: vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:resolved",
            vec!["evidence:provider".into()],
        )],
    });
    let service = ExternalEffectRecoveryService::new(
        repository.clone(),
        read_back_registry("webhook-v1", read_back),
    );
    let report = service.run(&context, at(30), at(30), at(22)).await.unwrap();

    assert_eq!(&*observed_receipts.lock().await, &[None]);
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(report.reconciliations()[0].receipt_id(), None);
    assert_eq!(
        report.settled_with_runs().next().unwrap().1,
        effect.execution_run_id()
    );
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Reconciling)
    );
}

#[tokio::test]
async fn memory_recovery_uses_each_dispatch_deadline_and_counts_legacy_exemptions_per_workspace() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let context = RequestContext::new(workspace_id, actor_id);
    let other_context = RequestContext::new(WorkspaceId::new(), actor_id);
    let repository = MemoryEffectRepository::default();
    let expired = intent(workspace_id, actor_id, &run_ref());
    let live = intent(workspace_id, actor_id, &run_ref());
    let legacy = intent(workspace_id, actor_id, &run_ref());
    for effect in [&expired, &live, &legacy] {
        repository.insert_intent(&context, effect).await.unwrap();
    }

    let same_recorded_at = at(20);
    repository
        .record_dispatch_started(
            &context,
            expired.id(),
            WorkerId::new(),
            at(19),
            same_recorded_at,
        )
        .await
        .unwrap();
    repository
        .record_dispatch_started(
            &context,
            live.id(),
            WorkerId::new(),
            at(21),
            same_recorded_at,
        )
        .await
        .unwrap();
    repository
        .seed_deadline_less_dispatch(&context, legacy.id(), same_recorded_at)
        .await
        .unwrap();

    let candidates = repository
        .find_reconciliation_candidates(&context, at(10_000), at(20))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].intent(), &expired);

    for cutoff in [at(0), at(20), at(10_000)] {
        assert!(
            repository
                .find_reconciliation_candidates(&context, at(10_000), cutoff)
                .await
                .unwrap()
                .iter()
                .all(|candidate| candidate.intent().id() != legacy.id()),
            "a dispatch nobody gave a deadline was swept at {cutoff}"
        );
    }
    assert_eq!(
        repository
            .count_deadline_less_dispatching_transitions(&context)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repository
            .count_deadline_less_dispatching_transitions(&other_context)
            .await
            .unwrap(),
        0
    );
}

struct OrderedAdapter {
    descriptor: ExternalEffectAdapterDescriptor,
    dispatch_started: Arc<std::sync::atomic::AtomicBool>,
}

impl ExternalEffectAdapter for OrderedAdapter {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
        &self.descriptor
    }

    fn dispatch(
        &self,
        _intent: &ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, AdapterError> {
        assert!(
            self.dispatch_started
                .load(std::sync::atomic::Ordering::SeqCst),
            "the adapter was called before Dispatching was committed"
        );
        Ok(AdapterDispatchResult::acknowledged(
            "accepted",
            None,
            Some("sha256:response".into()),
            vec!["evidence:adapter".into()],
        ))
    }
}

#[tokio::test]
async fn granular_service_validates_then_commits_dispatching_before_calling_the_adapter() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let effect = performable_intent(workspace_id, actor_id);
    let context = RequestContext::new(workspace_id, actor_id);
    let repository = Arc::new(MemoryEffectRepository::default());
    repository.insert_intent(&context, &effect).await.unwrap();
    let dispatch_owner = WorkerId::new();
    let service = ExternalEffectService::new(
        repository.clone(),
        AuthorizationBoundary::new(Arc::new(
            ConfiguredCapabilityPolicyEngine::new(
                "policy-v1",
                [Capability::ExportRead],
                RiskCategory::High,
            )
            .unwrap(),
        )),
        dispatch_owner,
    );
    let authorized = service
        .authorize(
            &context,
            &effect,
            AuthorizationRequest::new(
                effect.required_capability(),
                effect.operation().to_owned(),
                effect.target().to_owned(),
                effect.risk(),
            ),
        )
        .await
        .unwrap();
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    repository.signal_dispatch_started_with(Arc::clone(&started));
    let adapter = OrderedAdapter {
        descriptor: ExternalEffectAdapterDescriptor::new(
            "webhook-v1",
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            EffectReversibility::Compensatable,
            DryRunMode::Unsupported,
            true,
            true,
            Capability::ExportRead,
        )
        .unwrap(),
        dispatch_started: Arc::clone(&started),
    };

    assert!(
        service
            .dispatch(&context, &authorized, &adapter, "sha256:stale", at(20))
            .await
            .is_err()
    );
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Prepared),
        "failed validation committed a dispatch start"
    );

    service
        .dispatch(
            &context,
            &authorized,
            &adapter,
            effect.precondition_digest(),
            at(21),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Dispatching)
    );
    let evidence = repository.dispatch_evidence(effect.id()).await;
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].0, dispatch_owner);
    assert_eq!(evidence[0].2, at(21));
    assert!(
        evidence[0].1 > evidence[0].2,
        "the default dispatch deadline was not strictly after the recorded transition"
    );
}

#[tokio::test]
async fn perform_commits_dispatching_after_validation_and_before_the_adapter_call() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let context = RequestContext::new(workspace_id, actor_id);
    let repository = Arc::new(MemoryEffectRepository::default());
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    repository.signal_dispatch_started_with(Arc::clone(&started));
    let adapter = OrderedAdapter {
        descriptor: ExternalEffectAdapterDescriptor::new(
            "webhook-v1",
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            EffectReversibility::Compensatable,
            DryRunMode::Unsupported,
            true,
            true,
            Capability::ExportRead,
        )
        .unwrap(),
        dispatch_started: Arc::clone(&started),
    };
    let authorization = AuthorizationBoundary::new(Arc::new(
        ConfiguredCapabilityPolicyEngine::new(
            "policy-v1",
            [Capability::ExportRead],
            RiskCategory::High,
        )
        .unwrap(),
    ));
    let service =
        PerformExternalEffectService::new(repository.clone(), authorization, WorkerId::new());
    let effect = performable_intent(workspace_id, actor_id);
    let effect_id = effect.id();

    service
        .perform(&context, effect, &adapter, at(20))
        .await
        .unwrap();
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect_id)
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Acknowledged)
    );
}
