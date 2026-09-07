use std::sync::Arc;
use std::time::Duration;

use tokio::signal;
use vestrace_application::retrieval::EmbedMemoryHandler;
use vestrace_application::run::{
    AdvanceRunHandler, ExecuteStepHandler, ResumeRunHandler, RunWorkHandlerRegistry, RunWorker,
    RunWorkerConfig, SystemClock,
};
use vestrace_application::{
    ExternalEffectRepository, OutboxDispatcher, QualificationRuntime, RequestContext,
    WORKER_PRESENCE_HEARTBEAT_INTERVAL, WORKER_PRESENCE_LAPSE_AFTER,
};
use vestrace_domain::external_effects::ExternalEffectAdapter;
use vestrace_domain::id::{PrincipalId, WorkerId, WorkspaceId};
use vestrace_infrastructure::{
    AppConfig, PgEmbeddingStore, PgExternalEffectRepository, PgMemoryTextSource,
    PgOutboxRepository, PgRunLeasePort, PgStore, PgWorkQueuePort, PostgresRunStore,
};

/// How many messages one drain pass claims per workspace.
///
/// Small enough that a workspace with a large backlog does not starve the run
/// worker sharing this loop, and the loop returns immediately when it did work,
/// so a backlog still clears at full speed.
const OUTBOX_BATCH: u32 = 32;

#[derive(Clone, Copy, Debug, Default)]
struct PollOutcome {
    did_work: bool,
    failed: bool,
}

impl PollOutcome {
    fn merge(&mut self, other: Self) {
        self.did_work |= other.did_work;
        self.failed |= other.failed;
    }
}

pub async fn run(config: &AppConfig, once: bool) -> anyhow::Result<bool> {
    // Without this the worker installs no subscriber, and every warning it
    // emits is discarded — which is how it ran silently until now.
    super::server::init_tracing(&config.observability)?;
    tracing::info!("starting vestrace worker");
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow::anyhow!("database is unavailable"))?;

    // Checked after the database, not before it. Both refuse startup, but a
    // worker pointed at a database that is not there should say so: reporting a
    // storage-root problem first sends an operator to look at the filesystem
    // for a fault that is in the connection string.
    let storage_roots = config.provider_execution.roots().map_err(|_| {
        anyhow::anyhow!("provider execution storage roots are unavailable or overlap")
    })?;

    match store.migrations_are_compatible().await {
        Ok(true) => {}
        Ok(false) => {
            return Err(anyhow::anyhow!(
                "database migration history is incompatible"
            ));
        }
        Err(_) => {
            return Err(anyhow::anyhow!(
                "database migration verification is unavailable"
            ));
        }
    }
    if config.qualification.enabled {
        crate::commands::conformance::run_automatic_qualification(
            &config.qualification,
            QualificationRuntime::Worker,
            &store,
        )
        .await?;
    }
    if config.recovery.enabled {
        crate::commands::recovery::run_startup_recovery(&config.workspaces, &store).await?;
    }

    let outbox = build_outbox_dispatcher(config, &store)?;
    let reconciliation = build_effect_reconciliation(config, &store)?;
    // Built unconditionally, unlike the sweep: a debt can be owed by a
    // reconciliation recorded before this worker started, or by one an adapter
    // that is no longer configured produced. Refusing to deliver those because
    // no adapter is configured *now* would strand them.
    let outcome_delivery = Arc::new(vestrace_application::EffectOutcomeDeliveryService::new(
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(vestrace_infrastructure::PgRunEventStore::new(store.clone())),
        Arc::new(vestrace_infrastructure::PgRunRecoveryStore::new(
            store.clone(),
        )),
    ));

    let run_store = Arc::new(PostgresRunStore::new(&store));
    let lease_port = Arc::new(PgRunLeasePort::new(&store));
    let queue_port = Arc::new(PgWorkQueuePort::new(&store));
    let clock = Arc::new(SystemClock::new());
    let worker_id = WorkerId::new();

    let run_worker_config = RunWorkerConfig {
        worker_id,
        poll_interval: Duration::from_millis(500),
        lease_ttl: WORKER_PRESENCE_LAPSE_AFTER
            .to_std()
            .expect("the worker presence lapse window is positive"),
        heartbeat_interval: WORKER_PRESENCE_HEARTBEAT_INTERVAL
            .to_std()
            .expect("the worker presence heartbeat interval is positive"),
        max_concurrency: 1,
    };

    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(Arc::new(AdvanceRunHandler::new(
        run_store.clone(),
        queue_port.clone(),
        clock.clone(),
    )));
    registry.register(Arc::new(ResumeRunHandler::new(
        run_store.clone(),
        clock.clone(),
    )));
    reject_config_only_model_execution(config)?;
    // The one governed route from a work item to a provider. Every authority
    // here already existed; what this composition adds is that the worker
    // holds them rather than failing every agent step for want of an executor.
    // Nothing is optional: a missing vault, policy or repository refuses
    // startup instead of registering a handler that would report success
    // having called nothing.
    let governed = vestrace_infrastructure::GovernedProviderRuntime::new(
        store.clone(),
        super::server::build_material_vault(config, &storage_roots)?,
        super::server::build_policy_engine(
            &config.policy,
            Arc::new(vestrace_infrastructure::PgCapabilityGrantRepository::new(
                store.clone(),
            )),
        )?,
        super::server::build_model_data_policy_settings(config)?,
        run_store.clone(),
    );
    registry.register(Arc::new(
        ExecuteStepHandler::new(run_store.clone(), clock.clone())
            .with_model_executor(governed.step_executor(worker_id)),
    ));

    let run_worker = Arc::new(RunWorker::new_with_policy(
        run_worker_config,
        run_store,
        lease_port,
        queue_port,
        clock,
        registry,
        // Built from configuration, exactly as the server does. This was
        // hardcoded to `DenyAllPolicyEngine`, so every work item the worker
        // leased was refused with `authorization_denied: DefaultDeny` and
        // dead-lettered — no run could ever advance, whatever `policy.engine`
        // was set to.
        // The worker consults the same grant store as the server, so a
        // grant issued over HTTP governs run execution too. A worker with a
        // different view of authority than the surface that issued it would be
        // two policies wearing one name.
        super::server::build_policy_engine(
            &config.policy,
            std::sync::Arc::new(vestrace_infrastructure::PgCapabilityGrantRepository::new(
                store.clone(),
            )),
        )?,
    )?);

    // Every run/lease/work-queue query is scoped by `WHERE workspace_id = $1`,
    // so the worker polls the workspaces it was configured to serve. A worker
    // with no configured workspace can never claim run work, so say so once at
    // startup instead of spinning silently against nothing.
    let run_contexts: Vec<RequestContext> = config
        .workspaces
        .iter()
        .map(|workspace| {
            RequestContext::new(
                WorkspaceId::from_uuid(*workspace),
                PrincipalId::from_uuid(*workspace),
            )
        })
        .collect();
    if run_contexts.is_empty() {
        // The outbox drain is scoped by workspace for the same reason, so a
        // worker with no configured workspace now does nothing at all rather
        // than only failing to claim run work.
        tracing::warn!(
            "no workspaces are configured; the worker will claim neither run work nor outbox messages"
        );
    }

    report_dispatches_without_a_deadline(&store, &run_contexts).await;

    let presence = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let presence_started_at = vestrace_domain::now();
    register_worker_presence(
        presence.as_ref(),
        &run_contexts,
        worker_id,
        presence_started_at,
    )
    .await?;
    let (presence_shutdown, presence_shutdown_rx) = tokio::sync::watch::channel(false);
    let presence_heartbeat = tokio::spawn(refresh_worker_presence_until_shutdown(
        presence.clone(),
        run_contexts.clone(),
        worker_id,
        presence_started_at,
        presence_shutdown_rx,
    ));

    tracing::info!(
        workspaces = run_contexts.len(),
        outbox_topics = outbox.topics().count(),
        "worker started, polling for run work items and outbox messages"
    );

    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    if !once {
        let shutdown_clone = shutdown_notify.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_clone.notify_waiters();
        });
    }

    // The two pollers run one after the other, not raced against each other.
    //
    // They were previously branches of a `tokio::select!`, which cancels the
    // losing branch. Whenever the job poller finished first — which it does
    // constantly, because an empty job table returns immediately — the run-work
    // future was dropped at whatever await point it had reached. An item leased
    // a moment earlier was then abandoned: still `leased` in the queue, with no
    // run lease, no error and nothing in the log. `lease_next` only considers
    // `status = 'ready'`, so the item was stranded permanently.
    //
    // Only shutdown is raced, and cancelling a poll at shutdown is exactly what
    // is wanted there.
    let once_outcome = once.then_some(async {
        let mut outcome = poll_run_work(&run_worker, &run_contexts).await;
        outcome.merge(drain_outbox(&outbox, &run_contexts).await);
        outcome.merge(reconcile_effects(reconciliation.as_ref(), &run_contexts).await);
        outcome.merge(deliver_outcomes(&outcome_delivery, &run_contexts).await);
        outcome
    });
    let once_outcome = match once_outcome {
        Some(outcome) => Some(outcome.await),
        None => None,
    };

    let run_result = if once {
        Ok::<(), anyhow::Error>(())
    } else {
        async {
            loop {
                let outcome = tokio::select! {
                    biased;

                    _ = shutdown_notify.notified() => {
                        tracing::info!("worker shutdown requested, draining current work");
                        break;
                    }

                    outcome = async {
                        let runs = poll_run_work(&run_worker, &run_contexts).await;
                        let messages = drain_outbox(&outbox, &run_contexts).await;
                        let reconciled = reconcile_effects(reconciliation.as_ref(), &run_contexts).await;
                        let delivered = deliver_outcomes(&outcome_delivery, &run_contexts).await;
                        Ok::<_, anyhow::Error>(
                            runs.did_work || messages.did_work || reconciled.did_work || delivered.did_work,
                        )
                    } => outcome?,
                };

                if !outcome {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            Ok::<(), anyhow::Error>(())
        }
        .await
    };

    let _ = presence_shutdown.send(true);
    let heartbeat_result = presence_heartbeat
        .await
        .map_err(|error| anyhow::anyhow!("worker presence heartbeat task failed: {error}"));
    let clear_result = clear_worker_presence(presence.as_ref(), &run_contexts, worker_id).await;

    run_result?;
    heartbeat_result?;
    clear_result?;

    let once_outcome = once_outcome.unwrap_or_default();
    if once_outcome.failed {
        return Err(anyhow::anyhow!("worker single pass failed"));
    }

    tracing::info!("worker stopped gracefully");
    Ok(once_outcome.did_work || !once)
}

/// Register every workspace before the worker starts polling.
///
/// If one registration fails, previously registered workspaces are explicitly
/// stopped before startup is refused. Leaving them active would make a process
/// that never started look healthy until the lapse window elapsed.
async fn register_worker_presence(
    repository: &dyn ExternalEffectRepository,
    contexts: &[RequestContext],
    worker_id: WorkerId,
    started_at: vestrace_domain::Timestamp,
) -> anyhow::Result<()> {
    for (index, context) in contexts.iter().enumerate() {
        if let Err(error) = repository
            .record_worker_presence(context, worker_id, started_at, started_at)
            .await
        {
            let stopped_at = vestrace_domain::now();
            for registered in &contexts[..index] {
                if let Err(clear_error) = repository
                    .clear_worker_presence(registered, worker_id, stopped_at)
                    .await
                {
                    tracing::warn!(
                        workspace = %registered.workspace_id,
                        error = %clear_error,
                        "worker presence could not be cleared after startup registration failed"
                    );
                }
            }
            return Err(anyhow::anyhow!(
                "worker presence registration failed for workspace {}: {error}",
                context.workspace_id
            ));
        }
    }
    Ok(())
}

/// Refresh presence independently of the poll loop.
///
/// The poll future must not be raced with a timer: losing a `select!` cancels it
/// at an arbitrary await point, which previously stranded leased work. A small
/// background task lets a long-running handler keep reporting without changing
/// the run-work cancellation boundary.
async fn refresh_worker_presence_until_shutdown(
    repository: Arc<dyn ExternalEffectRepository>,
    contexts: Vec<RequestContext>,
    worker_id: WorkerId,
    started_at: vestrace_domain::Timestamp,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let heartbeat_interval = WORKER_PRESENCE_HEARTBEAT_INTERVAL
        .to_std()
        .expect("the worker presence heartbeat interval is positive");
    let mut heartbeat = tokio::time::interval(heartbeat_interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            _ = heartbeat.tick() => {
                let reported_at = vestrace_domain::now();
                for context in &contexts {
                    if let Err(error) = repository
                        .record_worker_presence(context, worker_id, started_at, reported_at)
                        .await
                    {
                        tracing::warn!(
                            workspace = %context.workspace_id,
                            error = %error,
                            "worker presence heartbeat failed"
                        );
                    }
                }
            }
        }
    }
}

/// Stop every workspace presence and report all failures together.
async fn clear_worker_presence(
    repository: &dyn ExternalEffectRepository,
    contexts: &[RequestContext],
    worker_id: WorkerId,
) -> anyhow::Result<()> {
    let stopped_at = vestrace_domain::now();
    let mut failures = Vec::new();
    for context in contexts {
        if let Err(error) = repository
            .clear_worker_presence(context, worker_id, stopped_at)
            .await
        {
            failures.push(format!("{}: {error}", context.workspace_id));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "worker presence shutdown failed for {}",
            failures.join(", ")
        ))
    }
}

/// Poll each configured workspace once, returning whether any of them had work.
///
/// A failure in one workspace is logged and does not stop the others: one
/// workspace's storage problem must not stall every other workspace's runs.
async fn poll_run_work(worker: &Arc<RunWorker>, contexts: &[RequestContext]) -> PollOutcome {
    let mut outcome = PollOutcome::default();
    for context in contexts {
        match worker.run_once(context).await {
            Ok(true) => outcome.did_work = true,
            Ok(false) => {}
            Err(error) => {
                outcome.failed = true;
                tracing::warn!(
                    error = %error,
                    workspace = %context.workspace_id,
                    "run worker iteration failed"
                );
            }
        }
    }
    outcome
}

/// The outbox drain, and the handlers that give it something to deliver.
///
/// # Why this did not exist
///
/// `OutboxRepository::claim_pending` and `mark_processed` were implemented and
/// called by nothing. Every memory written since the system began recorded a
/// message that was never delivered, and `outbox.backlog_within_budget` fired on
/// every deployment with a remediation — "process the outbox queue" — naming no
/// component able to do it.
///
/// A dispatcher with no handlers is still built, deliberately: it reports
/// unhandled topics rather than pretending the backlog is being cleared, which
/// is the distinction between an idle drain and a missing one.
fn build_outbox_dispatcher(
    config: &AppConfig,
    store: &PgStore,
) -> anyhow::Result<Arc<OutboxDispatcher>> {
    let repository = Arc::new(PgOutboxRepository::new(store.clone()));
    let mut dispatcher = OutboxDispatcher::new(repository);

    match super::server::build_embedding_provider(config, store)? {
        Some(provider) => {
            let embeddings = Arc::new(PgEmbeddingStore::new(store.clone()));
            let memories = Arc::new(PgMemoryTextSource::new(store.clone()));
            // Two topics, one behaviour: a revision changes the text a memory
            // means, so an index that only followed creation would answer with
            // the superseded meaning and look perfectly healthy doing it.
            for topic in ["memory.created", "memory.revised"] {
                dispatcher = dispatcher.with_handler(Arc::new(EmbedMemoryHandler::new(
                    provider.clone(),
                    embeddings.clone(),
                    memories.clone(),
                    config.embedding.space_name.clone(),
                    topic,
                )));
            }
            tracing::info!(
                model = %config.embedding.model_name,
                space = %config.embedding.space_name,
                "memories will be embedded as they are written"
            );
        }
        None => {
            // Said out loud, because the consequence is a vector channel that
            // returns nothing while every retrieval still succeeds — the exact
            // silence `memory.active_memory_is_embedded` exists to break.
            tracing::warn!(
                "no embedding provider is configured; memories will not be embedded on write"
            );
        }
    }

    Ok(Arc::new(dispatcher))
}

/// Drain each configured workspace once, returning whether any of them had work.
///
/// One workspace's failure is logged and does not stop the others, and an
/// undelivered message stays pending: the backlog is the signal, and a drain
/// that discarded what it could not deliver would erase it.
async fn drain_outbox(
    dispatcher: &Arc<OutboxDispatcher>,
    contexts: &[RequestContext],
) -> PollOutcome {
    let mut outcome = PollOutcome::default();
    for context in contexts {
        match dispatcher.drain_once(context, OUTBOX_BATCH).await {
            Ok(report) => {
                if report.delivered > 0 || report.unhandled > 0 || report.failed > 0 {
                    tracing::debug!(
                        workspace = %context.workspace_id,
                        delivered = report.delivered,
                        unhandled = report.unhandled,
                        failed = report.failed,
                        dead_lettered = report.dead_lettered,
                        "outbox drained"
                    );
                }
                outcome.did_work |= report.did_work();
                // A failed handler leaves durable retry/dead-letter evidence,
                // but the bounded process must still tell its caller that the
                // pass failed. Otherwise `--once` reports success after a
                // mixed cycle merely because another poll completed work.
                outcome.failed |= report.failed > 0;
            }
            Err(error) => {
                outcome.failed = true;
                tracing::warn!(
                    error = %error,
                    workspace = %context.workspace_id,
                    "outbox drain failed"
                );
            }
        }
    }
    outcome
}

/// The sweep that answers "did that actually happen".
///
/// # Why this is the worker's job
///
/// An effect whose outcome is unknown cannot be retried and cannot be assumed
/// away; the only route out is asking the far side. Until now nothing asked:
/// `ExternalEffectRecoveryService` and `HttpExternalEffectReadBackAdapter` were
/// both written and neither was constructed, so an unknown outcome stayed
/// unknown for the life of the deployment.
///
/// It runs beside the outbox drain rather than at startup because an unknown
/// outcome does not wait for a restart to appear — the timeout that produces
/// one happens while the system is running.
/// Say once, at startup, how many dispatches can never be declared lost.
///
/// Migration 0154 exempts the `dispatching` transitions written before a
/// dispatch stated an owner and a deadline: they carry no deadline, so no cutoff
/// can ever select them and no sweep will ever ask about them. That exemption is
/// the truthful compatibility state — inventing a deadline for a call that may
/// already have touched the world would be worse — but an exemption nobody can
/// count is a leak wearing correctness as a costume, which is why the port
/// carries the count at all.
///
/// Reported here rather than per sweep because nothing creates a deadline-less
/// transition any more, so the number only ever falls, and repeating it every
/// tick would bury it. A failure to count is logged and does not stop the
/// worker: this is disclosure, not a precondition for doing work.
async fn report_dispatches_without_a_deadline(store: &PgStore, contexts: &[RequestContext]) {
    let effects = PgExternalEffectRepository::new(store.clone());
    for context in contexts {
        match effects
            .count_deadline_less_dispatching_transitions(context)
            .await
        {
            Ok(0) => {}
            Ok(count) => tracing::warn!(
                workspace = %context.workspace_id,
                dispatches = count,
                "dispatches predate deadline ownership and can never be declared lost; \
                 they need an operator decision, not a timeout"
            ),
            Err(error) => tracing::warn!(
                workspace = %context.workspace_id,
                %error,
                "the count of dispatches without a deadline could not be read"
            ),
        }
    }
}

fn build_effect_reconciliation(
    config: &AppConfig,
    store: &PgStore,
) -> anyhow::Result<Option<Arc<vestrace_application::ExternalEffectRecoveryService>>> {
    let configured = config.effects.configured();
    if configured.is_empty() {
        tracing::info!(
            "no external effect adapter is configured; nothing can produce an unknown outcome \
             and nothing needs reconciling"
        );
        return Ok(None);
    }

    let read_backs = build_effect_read_back_registry(&configured)?;
    let adapter_names = read_backs.names().collect::<Vec<_>>().join(",");

    tracing::info!(
        adapter_count = read_backs.len(),
        adapters = %adapter_names,
        "unknown external effect outcomes will be reconciled against the far side"
    );
    Ok(Some(Arc::new(
        vestrace_application::ExternalEffectRecoveryService::new(
            Arc::new(PgExternalEffectRepository::new(store.clone())),
            read_backs,
        ),
    )))
}

/// Build every named route before recovery is allowed to start.
///
/// Taking only the first configured adapter made configuration order decide
/// which external system supplied evidence for every effect. Building the
/// complete registry preserves the adapter identity persisted on each intent,
/// while registry construction refuses ambiguous duplicate names.
fn build_effect_read_back_registry(
    configured: &[&vestrace_infrastructure::EffectAdapterConfig],
) -> anyhow::Result<vestrace_application::ExternalEffectReadBackRegistry> {
    let mut adapters = Vec::with_capacity(configured.len());
    for adapter in configured {
        // Recovery must use the same descriptor the configured dispatch adapter
        // publishes; copying a capability bit here would create a second truth
        // that could drift from the adapter we actually send through.
        let dispatch = vestrace_infrastructure::HttpWebhookEffectAdapter::new(
            &adapter.name,
            &adapter.dispatch_url,
            &adapter.read_back_url,
        )
        .map_err(|error| {
            anyhow::anyhow!(
                "effect adapter `{}` is not configurable: {error}",
                adapter.name
            )
        })?;
        let read_back =
            vestrace_infrastructure::HttpExternalEffectReadBackAdapter::new(&adapter.read_back_url)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "effect read-back for adapter `{}` is not configurable: {error}",
                        adapter.name
                    )
                })?;
        adapters.push((
            adapter.name.clone(),
            dispatch.descriptor().clone(),
            Arc::new(read_back) as Arc<dyn vestrace_application::ExternalEffectReadBackAdapter>,
        ));
    }
    vestrace_application::ExternalEffectReadBackRegistry::new(adapters)
        .map_err(|error| anyhow::anyhow!(error))
}

/// Reconcile each configured workspace once, returning whether anything moved.
///
/// A candidate that could not be asked is logged and left; it stays a candidate
/// because nothing about it has been settled.
async fn reconcile_effects(
    service: Option<&Arc<vestrace_application::ExternalEffectRecoveryService>>,
    contexts: &[RequestContext],
) -> PollOutcome {
    let Some(service) = service else {
        return PollOutcome::default();
    };
    let mut outcome = PollOutcome::default();
    for context in contexts {
        match service.sweep(context, vestrace_domain::now()).await {
            Ok(report) => {
                for (reconciliation, run_id) in report.settled_with_runs() {
                    // The run is named because the outcome is only useful to
                    // whoever asked for the effect. `execution_ref` was a free
                    // string until this slice, so this could not be reported at
                    // all — an operator learning that an effect did not happen
                    // had no way to find what had asked for it.
                    tracing::info!(
                        workspace = %context.workspace_id,
                        outcome = ?reconciliation.outcome(),
                        strength = ?reconciliation.evidence_strength(),
                        effect = %reconciliation.effect_id(),
                        run = run_id.map(|id| id.to_string()).unwrap_or_else(|| "none".into()),
                        "an unknown external effect outcome was settled"
                    );
                    outcome.did_work = true;
                }
                // Every outcome used to be logged with the line above, so
                // "the provider could not tell us" and "a human has to decide"
                // both read as *settled* at `info` — indistinguishable from
                // "confirmed, all fine". They are neither settled nor fine, and
                // they do not count as work: counting them would keep the
                // worker from ever sleeping while nothing was being resolved.
                for reconciliation in report.unsettled() {
                    tracing::warn!(
                        workspace = %context.workspace_id,
                        outcome = ?reconciliation.outcome(),
                        strength = ?reconciliation.evidence_strength(),
                        effect = %reconciliation.effect_id(),
                        retry_after_seconds =
                            vestrace_application::RECONCILIATION_RETRY_AFTER.num_seconds(),
                        "an external effect was asked about and its outcome is still unknown"
                    );
                }
                for pending in report.unreachable() {
                    tracing::warn!(
                        workspace = %context.workspace_id,
                        effect = %pending.effect_id,
                        reason = %pending.reason,
                        "an unknown external effect could not be asked about and stays unknown"
                    );
                }
                if report.saturated() {
                    tracing::warn!(
                        workspace = %context.workspace_id,
                        batch_size = vestrace_application::RECONCILIATION_BATCH,
                        "effect reconciliation used its full candidate budget; more work may remain"
                    );
                }
            }
            Err(error) => {
                outcome.failed = true;
                tracing::warn!(
                    error = %error,
                    workspace = %context.workspace_id,
                    "effect reconciliation sweep failed"
                );
            }
        }
    }
    outcome
}

/// Tell each configured workspace's runs what became of their effects.
///
/// A settled outcome is owed to the run that asked for the effect until this
/// records that it was delivered. A workspace whose delivery fails is logged and
/// skipped: its debts stay owed and the next pass retries them, and one
/// workspace's storage problem must not stop the others.
async fn deliver_outcomes(
    service: &Arc<vestrace_application::EffectOutcomeDeliveryService>,
    contexts: &[RequestContext],
) -> PollOutcome {
    let mut outcome = PollOutcome::default();
    for context in contexts {
        match vestrace_application::deliver_effect_outcomes(service, context).await {
            Ok(report) => {
                if report.delivered > 0 || report.unattributable > 0 || report.deferred > 0 {
                    tracing::info!(
                        workspace = %context.workspace_id,
                        delivered = report.delivered,
                        unattributable = report.unattributable,
                        deferred = report.deferred,
                        "settled external effect outcomes were recorded in their runs"
                    );
                }
                // A deferred debt is not work: counting it would keep the worker
                // spinning while nothing was being resolved.
                outcome.did_work |= report.did_work();
            }
            Err(error) => {
                outcome.failed = true;
                tracing::warn!(
                    error = %error,
                    workspace = %context.workspace_id,
                    "settled external effect outcomes could not be delivered to their runs"
                );
            }
        }
    }
    outcome
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Process configuration may seed a local candidate connection, but it cannot
/// authorize a Run adapter call.  Refuse the retired execution switch instead
/// of accepting an unpinned URL, model name, or secret path.
fn reject_config_only_model_execution(config: &AppConfig) -> anyhow::Result<()> {
    if config.model.enabled {
        return Err(anyhow::anyhow!(
            "config-only model execution is retired; governed provider dispatch authority is required"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_effect_read_back_registry;
    use vestrace_infrastructure::{EffectAdapterConfig, EffectsConfig};

    #[test]
    fn worker_builds_read_back_routes_for_every_configured_adapter() {
        let effects = EffectsConfig {
            webhook: Some(EffectAdapterConfig {
                name: "environment".into(),
                dispatch_url: "https://environment.effects.test/dispatch".into(),
                read_back_url: "https://environment.effects.test/read-back".into(),
            }),
            adapters: vec![EffectAdapterConfig {
                name: "file".into(),
                dispatch_url: "https://file.effects.test/dispatch".into(),
                read_back_url: "https://file.effects.test/read-back".into(),
            }],
        };
        let configured = effects.configured();

        let registry = build_effect_read_back_registry(&configured).unwrap();

        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            ["environment", "file"]
        );
    }
}
