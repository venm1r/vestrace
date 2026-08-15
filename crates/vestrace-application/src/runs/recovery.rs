use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, sync::Arc};
use vestrace_domain::{
    DomainError, classify_recovery,
    id::AgentRunId,
    now,
    run::{AgentRun, RunState, RunVersion, apply, replay},
    time::Timestamp,
    trust::{RecoveryClassification, RecoveryTarget},
};

use crate::{ApplicationError, RequestContext};

use super::{SharedRunRecoveryStore, project_run};

pub const RUN_CHECKPOINT_FORMAT_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunCheckpoint {
    pub workspace_id: vestrace_domain::WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: RunVersion,
    pub format_version: u16,
    pub state_hash: String,
    pub state: RunState,
    pub created_at: Timestamp,
}

pub fn hash_run_state(state: &RunState) -> Result<String, ApplicationError> {
    let bytes = serde_json::to_vec(state)
        .map_err(|_| ApplicationError::Internal("run state serialization failed".to_owned()))?;
    let digest = Sha256::digest(bytes);
    let mut hash = String::with_capacity(64);
    for byte in digest {
        write!(&mut hash, "{byte:02x}")
            .map_err(|_| ApplicationError::Internal("run state hashing failed".to_owned()))?;
    }
    Ok(hash)
}

#[async_trait]
pub trait RunRecoveryOperations: Send + Sync {
    async fn create_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<RunCheckpoint, ApplicationError>;

    async fn validate_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        sequence: RunVersion,
    ) -> Result<RunCheckpoint, ApplicationError>;

    async fn restore(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<RunState, ApplicationError>;

    async fn rebuild_projection(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<AgentRun, ApplicationError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupRecoveryCandidate {
    pub run_id: AgentRunId,
    pub target: RecoveryTarget,
}

impl StartupRecoveryCandidate {
    pub fn new(run_id: AgentRunId, target: RecoveryTarget) -> Self {
        Self { run_id, target }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupRecoveryOutcome {
    Restored,
    RetryReady,
    ReconciliationRequired,
    Aborted,
    HumanReviewRequired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupRecoveryRecord {
    pub run_id: AgentRunId,
    pub target: RecoveryTarget,
    pub classification: RecoveryClassification,
    pub outcome: StartupRecoveryOutcome,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartupRecoveryReport {
    records: Vec<StartupRecoveryRecord>,
}

impl StartupRecoveryReport {
    pub fn records(&self) -> &[StartupRecoveryRecord] {
        &self.records
    }
}

/// Durable discovery of runs that were interrupted before the current process
/// started. Implementations are workspace-scoped: they see only the workspace
/// carried by the supplied context.
#[async_trait]
pub trait StartupRecoveryCandidateSource: Send + Sync {
    async fn find_startup_recovery_candidates(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<StartupRecoveryCandidate>, ApplicationError>;
}

pub struct StartupRecoveryService {
    operations: Arc<dyn RunRecoveryOperations>,
    candidates: Option<Arc<dyn StartupRecoveryCandidateSource>>,
}

impl StartupRecoveryService {
    pub fn new(operations: Arc<dyn RunRecoveryOperations>) -> Self {
        Self {
            operations,
            candidates: None,
        }
    }

    pub fn with_candidate_source(
        operations: Arc<dyn RunRecoveryOperations>,
        candidates: Arc<dyn StartupRecoveryCandidateSource>,
    ) -> Self {
        Self {
            operations,
            candidates: Some(candidates),
        }
    }

    /// Discover interrupted runs durably and recover them. An unconfigured
    /// source or a failing query is an error, never an empty recovery run.
    pub async fn run_discovered(
        &self,
        context: &RequestContext,
    ) -> Result<StartupRecoveryReport, ApplicationError> {
        let source = self.candidates.as_ref().ok_or_else(|| {
            ApplicationError::InvalidConfiguration(
                "startup recovery candidate source is not configured".to_owned(),
            )
        })?;
        let candidates = source.find_startup_recovery_candidates(context).await?;
        self.run(context, candidates).await
    }

    pub async fn run(
        &self,
        context: &RequestContext,
        candidates: Vec<StartupRecoveryCandidate>,
    ) -> Result<StartupRecoveryReport, ApplicationError> {
        let mut seen = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            if !seen.iter().any(|run_id| *run_id == candidate.run_id) {
                seen.push(candidate.run_id);
            } else {
                return Err(DomainError::InvalidArgument(format!(
                    "duplicate startup recovery candidate: {}",
                    candidate.run_id
                ))
                .into());
            }
        }

        let mut records = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let classification = classify_recovery(candidate.target);
            let outcome = match classification {
                RecoveryClassification::SafeToResume => {
                    self.operations
                        .rebuild_projection(context, candidate.run_id)
                        .await?;
                    StartupRecoveryOutcome::Restored
                }
                RecoveryClassification::SafeToRetry => {
                    self.operations
                        .rebuild_projection(context, candidate.run_id)
                        .await?;
                    StartupRecoveryOutcome::RetryReady
                }
                RecoveryClassification::MustReconcile => {
                    StartupRecoveryOutcome::ReconciliationRequired
                }
                RecoveryClassification::MustAbort => StartupRecoveryOutcome::Aborted,
                RecoveryClassification::HumanRequired => {
                    StartupRecoveryOutcome::HumanReviewRequired
                }
            };

            records.push(StartupRecoveryRecord {
                run_id: candidate.run_id,
                target: candidate.target,
                classification,
                outcome,
            });
        }

        Ok(StartupRecoveryReport { records })
    }
}

pub struct RunRecoveryService {
    store: SharedRunRecoveryStore,
}

impl RunRecoveryService {
    pub fn new(store: SharedRunRecoveryStore) -> Self {
        Self { store }
    }

    async fn stream_head(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<RunVersion, ApplicationError> {
        self.store
            .load_stream_head(context, run_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("run stream does not exist".to_owned()).into())
    }

    fn validate_checkpoint_structure(
        context: &RequestContext,
        run_id: AgentRunId,
        checkpoint: &RunCheckpoint,
    ) -> Result<(), ApplicationError> {
        if checkpoint.format_version != RUN_CHECKPOINT_FORMAT_VERSION {
            return Err(storage_corruption("unsupported run checkpoint format"));
        }
        if checkpoint.workspace_id != context.workspace_id
            || checkpoint.state.workspace_id != context.workspace_id
        {
            return Err(storage_corruption("run checkpoint workspace mismatch"));
        }
        if checkpoint.run_id != run_id || checkpoint.state.id != run_id {
            return Err(storage_corruption("run checkpoint identity mismatch"));
        }
        if checkpoint.state.version != checkpoint.sequence {
            return Err(storage_corruption("run checkpoint version mismatch"));
        }
        if hash_run_state(&checkpoint.state)? != checkpoint.state_hash {
            return Err(storage_corruption("run checkpoint hash mismatch"));
        }
        Ok(())
    }

    async fn validate_loaded_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        checkpoint: RunCheckpoint,
    ) -> Result<RunCheckpoint, ApplicationError> {
        Self::validate_checkpoint_structure(context, run_id, &checkpoint)?;
        let prefix = self
            .store
            .load_events_through(context, run_id, checkpoint.sequence)
            .await?;
        let replayed = replay(prefix)
            .map_err(replay_error)?
            .ok_or_else(|| storage_corruption("run checkpoint has no event prefix"))?;
        if replayed != checkpoint.state {
            return Err(storage_corruption(
                "run checkpoint differs from authoritative event prefix",
            ));
        }
        Ok(checkpoint)
    }

    async fn restore_from_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        head: RunVersion,
        checkpoint: RunCheckpoint,
    ) -> Result<RunState, ApplicationError> {
        if checkpoint.sequence.value() > head.value() {
            return Err(storage_corruption(
                "run checkpoint is ahead of the stream head",
            ));
        }
        let checkpoint = self
            .validate_loaded_checkpoint(context, run_id, checkpoint)
            .await?;
        let tail = self
            .store
            .load_events_after(context, run_id, checkpoint.sequence)
            .await?;
        let mut state = checkpoint.state;
        for event in tail
            .into_iter()
            .filter(|event| event.sequence.value() <= head.value())
        {
            state = apply(Some(state), &event).map_err(reduce_error)?;
        }
        verify_restored_head(&state, head)?;
        Ok(state)
    }
}

#[async_trait]
impl RunRecoveryOperations for RunRecoveryService {
    async fn create_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<RunCheckpoint, ApplicationError> {
        let head = self.stream_head(context, run_id).await?;
        let events = self
            .store
            .load_events_through(context, run_id, head)
            .await?;
        let state = replay(events)
            .map_err(replay_error)?
            .ok_or_else(|| storage_corruption("run stream has no canonical events"))?;
        verify_restored_head(&state, head)?;
        let checkpoint = RunCheckpoint {
            workspace_id: context.workspace_id,
            run_id,
            sequence: state.version,
            format_version: RUN_CHECKPOINT_FORMAT_VERSION,
            state_hash: hash_run_state(&state)?,
            state,
            created_at: canonical_checkpoint_timestamp(now())?,
        };
        self.store.save_checkpoint(context, &checkpoint).await?;
        Ok(checkpoint)
    }

    async fn validate_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        sequence: RunVersion,
    ) -> Result<RunCheckpoint, ApplicationError> {
        let checkpoint = self
            .store
            .load_checkpoint(context, run_id, sequence)
            .await?
            .ok_or_else(|| DomainError::NotFound("run checkpoint does not exist".to_owned()))?;
        self.validate_loaded_checkpoint(context, run_id, checkpoint)
            .await
    }

    async fn restore(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<RunState, ApplicationError> {
        let head = self.stream_head(context, run_id).await?;
        if let Some(checkpoint) = self.store.load_latest_checkpoint(context, run_id).await? {
            if checkpoint.sequence.value() <= head.value() {
                return self
                    .restore_from_checkpoint(context, run_id, head, checkpoint)
                    .await;
            }
        }

        let events = self
            .store
            .load_events_through(context, run_id, head)
            .await?;
        let state = replay(events)
            .map_err(replay_error)?
            .ok_or_else(|| storage_corruption("run stream has no canonical events"))?;
        verify_restored_head(&state, head)?;
        Ok(state)
    }

    async fn rebuild_projection(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<AgentRun, ApplicationError> {
        let state = self.restore(context, run_id).await?;
        let head = self.stream_head(context, run_id).await?;
        verify_restored_head(&state, head)?;
        let projection = project_run(&state);
        self.store
            .replace_projection(context, head, &projection)
            .await?;
        Ok(projection)
    }
}

fn canonical_checkpoint_timestamp(timestamp: Timestamp) -> Result<Timestamp, ApplicationError> {
    Timestamp::from_timestamp_micros(timestamp.timestamp_micros()).ok_or_else(|| {
        ApplicationError::Internal("run checkpoint timestamp normalization failed".to_owned())
    })
}

fn verify_restored_head(state: &RunState, head: RunVersion) -> Result<(), ApplicationError> {
    if state.version != head {
        return Err(storage_corruption(
            "restored run version does not match stream head",
        ));
    }
    Ok(())
}

fn replay_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(format!("run recovery replay failed: {error}"))
}

fn reduce_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(format!("run recovery reduction failed: {error}"))
}

fn storage_corruption(message: &str) -> ApplicationError {
    ApplicationError::Storage(message.to_owned())
}
