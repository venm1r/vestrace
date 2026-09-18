use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::id::WorkerId;
use vestrace_domain::run::{RunFailure, RunStatus};
use vestrace_domain::time::Timestamp;
use vestrace_domain::{
    AuthorizationRequest, Capability, ResourceKind, ResourceScope, RiskCategory,
};

use crate::{
    ApplicationError, AuthorizationBoundary, DenyAllPolicyEngine, RequestContext,
    SharedPolicyDecisionEngine,
};

use super::ports::{
    AcquireRunLease, LeaseWorkRequest, RunClockPort, RunLease, RunLeasePort, RunSnapshot,
    RunStorePort, WorkItem, WorkItemKindDiscriminant, WorkQueuePort,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunWorkOutcome {
    Completed,
    Retry {
        available_at: Timestamp,
        error: RunFailure,
    },
    DeadLetter {
        error: RunFailure,
    },
    NoopStaleVersion,
}

#[async_trait]
pub trait RunWorkHandler: Send + Sync {
    fn kind(&self) -> WorkItemKindDiscriminant;

    async fn handle(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError>;
}

pub struct RunWorkHandlerRegistry {
    handlers: HashMap<WorkItemKindDiscriminant, Arc<dyn RunWorkHandler>>,
}

impl RunWorkHandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(&mut self, handler: Arc<dyn RunWorkHandler>) {
        self.handlers.insert(handler.kind(), handler);
    }

    fn get(&self, kind: &WorkItemKindDiscriminant) -> Option<&Arc<dyn RunWorkHandler>> {
        self.handlers.get(kind)
    }
}

impl Default for RunWorkHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct RunWorkerConfig {
    pub worker_id: WorkerId,
    pub poll_interval: std::time::Duration,
    pub lease_ttl: std::time::Duration,
    pub heartbeat_interval: std::time::Duration,
    pub max_concurrency: usize,
}

impl RunWorkerConfig {
    pub fn validate(&self) -> Result<(), ApplicationError> {
        if self.max_concurrency != 1 {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "max_concurrency must be 1 in H1, got {}",
                self.max_concurrency
            )));
        }
        Ok(())
    }
}

pub struct RunWorker {
    config: RunWorkerConfig,
    store: Arc<dyn RunStorePort>,
    lease_port: Arc<dyn RunLeasePort>,
    queue_port: Arc<dyn WorkQueuePort>,
    clock: Arc<dyn RunClockPort>,
    registry: RunWorkHandlerRegistry,
    authorization_boundary: AuthorizationBoundary,
}

impl RunWorker {
    pub fn new(
        config: RunWorkerConfig,
        store: Arc<dyn RunStorePort>,
        lease_port: Arc<dyn RunLeasePort>,
        queue_port: Arc<dyn WorkQueuePort>,
        clock: Arc<dyn RunClockPort>,
        registry: RunWorkHandlerRegistry,
    ) -> Result<Self, ApplicationError> {
        Self::new_with_policy(
            config,
            store,
            lease_port,
            queue_port,
            clock,
            registry,
            Arc::new(DenyAllPolicyEngine),
        )
    }

    pub fn new_with_policy(
        config: RunWorkerConfig,
        store: Arc<dyn RunStorePort>,
        lease_port: Arc<dyn RunLeasePort>,
        queue_port: Arc<dyn WorkQueuePort>,
        clock: Arc<dyn RunClockPort>,
        registry: RunWorkHandlerRegistry,
        policy_engine: SharedPolicyDecisionEngine,
    ) -> Result<Self, ApplicationError> {
        config.validate()?;
        Ok(Self {
            config,
            store,
            lease_port,
            queue_port,
            clock,
            registry,
            authorization_boundary: AuthorizationBoundary::new(policy_engine),
        })
    }

    pub async fn run_once(&self, context: &RequestContext) -> Result<bool, ApplicationError> {
        let now = self.clock.now();
        let lease_until = now
            + chrono::Duration::from_std(self.config.lease_ttl)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        let item = self
            .queue_port
            .lease_next(
                context,
                LeaseWorkRequest {
                    worker_id: self.config.worker_id,
                    now,
                    lease_until,
                },
            )
            .await?;

        let Some(item) = item else {
            return Ok(false);
        };

        match self
            .authorization_boundary
            .require(context, work_item_authorization_request(&item))
            .await
        {
            Ok(_) => {}
            Err(ApplicationError::Policy(message)) => {
                self.queue_port
                    .dead_letter(
                        context,
                        &item,
                        RunFailure {
                            code: "authorization_denied".into(),
                            message,
                            retryable: false,
                        },
                        now,
                    )
                    .await?;
                return Ok(true);
            }
            Err(error) => return Err(error),
        }

        self.process_item(context, item).await?;
        Ok(true)
    }

    async fn process_item(
        &self,
        context: &RequestContext,
        item: WorkItem,
    ) -> Result<(), ApplicationError> {
        let now = self.clock.now();

        let snapshot = match self.store.load(context, item.run_id).await? {
            Some(s) => s,
            None => {
                self.queue_port
                    .dead_letter(
                        context,
                        &item,
                        RunFailure {
                            code: "run_not_found".into(),
                            message: "run does not exist".into(),
                            retryable: false,
                        },
                        now,
                    )
                    .await?;
                return Ok(());
            }
        };

        if snapshot.run.version != item.expected_run_version {
            self.queue_port
                .complete(context, &item, now)
                .await
                .or_else(|e| match e {
                    ApplicationError::Conflict(_) => Ok(()),
                    other => Err(other),
                })?;
            return Ok(());
        }

        if snapshot.run.status.is_terminal() || snapshot.run.status == RunStatus::Paused {
            self.queue_port
                .cancel_for_run(context, item.run_id, now)
                .await?;
            return Ok(());
        }

        let lease_until = now
            + chrono::Duration::from_std(self.config.lease_ttl)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;
        let lease = match self
            .lease_port
            .acquire(
                context,
                AcquireRunLease {
                    run_id: item.run_id,
                    worker_id: self.config.worker_id,
                    now,
                    lease_until,
                },
            )
            .await
        {
            Ok(lease) => lease,
            Err(ApplicationError::Conflict(_)) => {
                let retry_at = now + retry_delay(item.attempt);
                self.queue_port
                    .retry(
                        context,
                        &item,
                        retry_at,
                        RunFailure {
                            code: "lease_busy".into(),
                            message: "run lease held by another worker".into(),
                            retryable: true,
                        },
                    )
                    .await?;
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let handler = match self.registry.get(&item.kind.discriminant()) {
            Some(h) => h.clone(),
            None => {
                self.queue_port
                    .dead_letter(
                        context,
                        &item,
                        RunFailure {
                            code: "no_handler".into(),
                            message: "no handler registered for this work item kind".into(),
                            retryable: false,
                        },
                        now,
                    )
                    .await?;
                self.lease_port.release(context, &lease).await.ok();
                return Ok(());
            }
        };

        let outcome = handler.handle(context, &snapshot, &item, &lease).await;

        match outcome {
            Ok(RunWorkOutcome::Completed) => {
                self.queue_port.complete(context, &item, now).await?;
            }
            Ok(RunWorkOutcome::Retry {
                available_at,
                error,
            }) => {
                self.queue_port
                    .retry(context, &item, available_at, error)
                    .await?;
            }
            Ok(RunWorkOutcome::DeadLetter { error }) => {
                self.queue_port
                    .dead_letter(context, &item, error, now)
                    .await?;
            }
            Ok(RunWorkOutcome::NoopStaleVersion) => {
                self.queue_port.complete(context, &item, now).await?;
            }
            Err(ApplicationError::Conflict(_)) => {
                self.queue_port
                    .retry(
                        context,
                        &item,
                        now + retry_delay(item.attempt),
                        RunFailure {
                            code: "stale_lease".into(),
                            message: "lease was lost during handler execution".into(),
                            retryable: true,
                        },
                    )
                    .await?;
            }
            Err(e) => {
                let retryable = is_retryable(&e);
                let failure = RunFailure {
                    code: "handler_error".into(),
                    message: e.to_string(),
                    retryable,
                };
                if retryable {
                    self.queue_port
                        .retry(context, &item, now + retry_delay(item.attempt), failure)
                        .await?;
                } else {
                    self.queue_port
                        .dead_letter(context, &item, failure, now)
                        .await?;
                }
            }
        }

        self.lease_port.release(context, &lease).await.ok();
        Ok(())
    }

    pub async fn run_loop(
        self: Arc<Self>,
        context: RequestContext,
        shutdown: tokio::sync::Notify,
    ) -> Result<(), ApplicationError> {
        loop {
            tokio::select! {
                processed = self.run_once(&context) => {
                    match processed {
                        Ok(true) => {}
                        Ok(false) => {
                            tokio::time::sleep(self.config.poll_interval).await;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "worker iteration failed");
                            tokio::time::sleep(self.config.poll_interval).await;
                        }
                    }
                }
                _ = shutdown.notified() => {
                    tracing::info!("run worker shutting down");
                    break;
                }
            }
        }
        Ok(())
    }
}

/// What the worker asks permission for, in the vocabulary grants are written
/// in.
///
/// This asked about `run:{id}` until the scope vocabulary was made explicit.
/// The boundary rule that decides whether a grant contains a request looks for
/// `/`, so `run:` did not contain `run:{id}` and **no grant covering more than
/// one named run could be written** — an operator who wanted "may work any run
/// in this workspace" had to enumerate them, or grant something broader
/// elsewhere. `run://{id}` is inside `run://`, which is the grant they meant.
fn work_item_authorization_request(item: &WorkItem) -> AuthorizationRequest {
    AuthorizationRequest::new(
        Capability::ExecutionWrite,
        format!("worker.run.{:?}", item.kind.discriminant()).to_ascii_lowercase(),
        ResourceScope::one(ResourceKind::Run, item.run_id).to_string(),
        RiskCategory::Low,
    )
}

fn retry_delay(attempt: u32) -> chrono::Duration {
    match attempt {
        0 | 1 => chrono::Duration::seconds(1),
        2 => chrono::Duration::seconds(5),
        3 => chrono::Duration::seconds(30),
        _ => chrono::Duration::seconds(60),
    }
}

fn is_retryable(e: &ApplicationError) -> bool {
    match e {
        ApplicationError::Storage(_) => true,
        ApplicationError::Conflict(_) => false,
        _ => false,
    }
}
