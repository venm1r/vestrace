//! Durable q1 qualification lifecycle ports.
//!
//! The application layer owns the closed protocol terms. PostgreSQL owns the
//! guarded state transitions; a worker may not substitute a retry loop or a
//! second, q1-only dispatch state machine.

use async_trait::async_trait;
use vestrace_domain::{
    ConnectionId, ConnectionQualificationRevisionId, ConnectionRevisionId,
    ModelQualificationRevisionId, ModelRevisionId, QualificationJobId, QualificationJobState,
    QualificationProbeResult, QualificationTargetBinding, WorkspaceId,
};

use crate::{ApplicationError, RequestContext};

pub const OPENAI_Q1_PROFILE_REVISION: &str = "openai-chat-completions-v1/q1";

#[derive(Clone, Debug)]
pub struct QualificationJobRequest {
    pub job_id: QualificationJobId,
    pub target_binding_id: uuid::Uuid,
    pub connection_id: ConnectionId,
    pub connection_revision_id: ConnectionRevisionId,
    pub target: QualificationTargetBinding,
    pub chat_model_revision_id: ModelRevisionId,
    pub embedding_model_revision_id: ModelRevisionId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualificationJobRecord {
    pub id: QualificationJobId,
    pub target_binding_id: uuid::Uuid,
    pub state: QualificationJobState,
}

#[derive(Clone, Debug)]
pub struct QualificationProbeCompletion {
    pub probe_result_id: uuid::Uuid,
    pub job_id: QualificationJobId,
    pub ordinal: String,
    pub result: QualificationProbeResult,
    /// Static q1 ordinals contain neither an effect nor an MRE. Every network
    /// ordinal names exactly one of each, before post-network completion.
    pub external_effect_id: Option<uuid::Uuid>,
    pub model_request_evidence_id: Option<uuid::Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualificationFinalization {
    pub job_id: QualificationJobId,
    pub connection_qualification_revision_id: ConnectionQualificationRevisionId,
    pub chat_model_qualification_revision_id: ModelQualificationRevisionId,
    pub embedding_model_qualification_revision_id: ModelQualificationRevisionId,
}

#[async_trait]
pub trait QualificationJobRepository: Send + Sync {
    /// Acquires the shared installation permit before inspecting target guards
    /// and atomically persists the immutable connection/auth/model tuple.
    async fn request(
        &self,
        context: &RequestContext,
        request: QualificationJobRequest,
    ) -> Result<QualificationJobRecord, ApplicationError>;

    async fn record_probe_result(
        &self,
        context: &RequestContext,
        completion: QualificationProbeCompletion,
    ) -> Result<QualificationJobState, ApplicationError>;

    async fn cancel_between_probes(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
    ) -> Result<(), ApplicationError>;

    /// The original durable dispatch cause, not a new provider request, is the
    /// authority for terminal `InconclusiveUnknown` after a lost response.
    async fn recover_lost_dispatch(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<(), ApplicationError>;

    async fn finalize_success(
        &self,
        context: &RequestContext,
        finalization: QualificationFinalization,
    ) -> Result<(), ApplicationError>;
}

/// The only application seam allowed to cross the q1 network boundary.  Its
/// production implementation must reconstruct the Complete q1 MRE, enter the
/// shared provider-dispatch authority, call the hardened adapter once, and
/// commit the shared post-network completion before it returns this record.
#[async_trait]
pub trait QualificationProbeRunner: Send + Sync {
    async fn run_network_probe(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<QualificationProbeCompletion, ApplicationError>;
}

/// Small protocol-facing service.  The repository is purposefully the only
/// persistence authority, so construction of a worker cannot bypass the
/// permit/guard ordering with direct SQL.
pub struct QualificationJobService<R> {
    repository: R,
}

impl<R> QualificationJobService<R>
where
    R: QualificationJobRepository,
{
    pub const fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn request(
        &self,
        context: &RequestContext,
        request: QualificationJobRequest,
    ) -> Result<QualificationJobRecord, ApplicationError> {
        self.repository.request(context, request).await
    }

    pub async fn record_probe_result(
        &self,
        context: &RequestContext,
        completion: QualificationProbeCompletion,
    ) -> Result<QualificationJobState, ApplicationError> {
        self.repository
            .record_probe_result(context, completion)
            .await
    }

    /// Runs exactly one q1 protocol ordinal.  Static probes are durable local
    /// checks; every other ordinal is delegated exactly once to the shared
    /// network runner, whose completion is then committed through the guarded
    /// qualification result transition.  This method intentionally contains no
    /// retry path: recovery adopts the original dispatch cause instead.
    pub async fn run_next_probe<P>(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
        runner: &P,
    ) -> Result<QualificationJobState, ApplicationError>
    where
        P: QualificationProbeRunner,
    {
        let completion = match ordinal {
            "00" | "15" => QualificationProbeCompletion {
                probe_result_id: uuid::Uuid::now_v7(),
                job_id,
                ordinal: ordinal.to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: None,
                model_request_evidence_id: None,
            },
            "10" | "20" | "30" | "35" | "40" | "50" | "60" | "70" | "80" | "90" => {
                let completion = runner.run_network_probe(context, job_id, ordinal).await?;
                if completion.job_id != job_id
                    || completion.ordinal != ordinal
                    || completion.external_effect_id.is_none()
                    || completion.model_request_evidence_id.is_none()
                {
                    return Err(ApplicationError::Policy(
                        "q1 network runner did not return its exact durable probe tuple".into(),
                    ));
                }
                completion
            }
            _ => {
                return Err(ApplicationError::Policy(
                    "q1 runner was asked for an unknown probe ordinal".into(),
                ));
            }
        };
        self.record_probe_result(context, completion).await
    }

    pub async fn cancel_between_probes(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
    ) -> Result<(), ApplicationError> {
        self.repository.cancel_between_probes(context, job_id).await
    }

    pub async fn recover_lost_dispatch(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<(), ApplicationError> {
        self.repository
            .recover_lost_dispatch(context, job_id, ordinal)
            .await
    }

    pub async fn finalize_success(
        &self,
        context: &RequestContext,
        finalization: QualificationFinalization,
    ) -> Result<(), ApplicationError> {
        self.repository
            .finalize_success(context, finalization)
            .await
    }
}

pub const fn q1_profile_for_workspace(_workspace_id: WorkspaceId) -> &'static str {
    OPENAI_Q1_PROFILE_REVISION
}

fn qualification_state(value: &str) -> Result<QualificationJobState, ApplicationError> {
    match value {
        "requested" => Ok(QualificationJobState::Requested),
        "running" => Ok(QualificationJobState::Running),
        "succeeded" => Ok(QualificationJobState::Succeeded),
        "failed_definite" => Ok(QualificationJobState::FailedDefinite),
        "inconclusive_unknown" => Ok(QualificationJobState::InconclusiveUnknown),
        "cancelled" => Ok(QualificationJobState::Cancelled),
        _ => Err(ApplicationError::Storage(
            "qualification lifecycle returned an unknown state".to_owned(),
        )),
    }
}

/// Re-exported only for PostgreSQL's string-to-domain boundary.  Product
/// callers keep using the closed domain enum above.
pub fn qualification_state_from_storage(
    value: &str,
) -> Result<QualificationJobState, ApplicationError> {
    qualification_state(value)
}

#[cfg(test)]
mod lifecycle_tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use vestrace_domain::{PrincipalId, QualificationJobId, QualificationProbeResult, WorkspaceId};

    use super::*;

    #[derive(Clone, Default)]
    struct RecordingRepository {
        completions: Arc<Mutex<Vec<QualificationProbeCompletion>>>,
    }

    #[async_trait]
    impl QualificationJobRepository for RecordingRepository {
        async fn request(
            &self,
            _context: &RequestContext,
            _request: QualificationJobRequest,
        ) -> Result<QualificationJobRecord, ApplicationError> {
            Err(ApplicationError::Policy("unused test request".into()))
        }

        async fn record_probe_result(
            &self,
            _context: &RequestContext,
            completion: QualificationProbeCompletion,
        ) -> Result<QualificationJobState, ApplicationError> {
            self.completions.lock().unwrap().push(completion);
            Ok(QualificationJobState::Running)
        }

        async fn cancel_between_probes(
            &self,
            _context: &RequestContext,
            _job_id: QualificationJobId,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn recover_lost_dispatch(
            &self,
            _context: &RequestContext,
            _job_id: QualificationJobId,
            _ordinal: &str,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn finalize_success(
            &self,
            _context: &RequestContext,
            _finalization: QualificationFinalization,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingRunner {
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl QualificationProbeRunner for RecordingRunner {
        async fn run_network_probe(
            &self,
            _context: &RequestContext,
            job_id: QualificationJobId,
            ordinal: &str,
        ) -> Result<QualificationProbeCompletion, ApplicationError> {
            self.calls.lock().unwrap().push(ordinal.to_owned());
            Ok(QualificationProbeCompletion {
                probe_result_id: uuid::Uuid::now_v7(),
                job_id,
                ordinal: ordinal.to_owned(),
                result: QualificationProbeResult::Pass,
                external_effect_id: Some(uuid::Uuid::now_v7()),
                model_request_evidence_id: Some(uuid::Uuid::now_v7()),
            })
        }
    }

    #[tokio::test]
    async fn runner_keeps_static_ordinals_local_and_uses_one_network_execution() {
        let repository = RecordingRepository::default();
        let completion_log = repository.completions.clone();
        let service = QualificationJobService::new(repository);
        let runner = RecordingRunner::default();
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
        let job_id = QualificationJobId::new();

        service
            .run_next_probe(&context, job_id, "00", &runner)
            .await
            .unwrap();
        service
            .run_next_probe(&context, job_id, "10", &runner)
            .await
            .unwrap();

        assert_eq!(runner.calls.lock().unwrap().as_slice(), ["10"]);
        let completions = completion_log.lock().unwrap();
        assert_eq!(completions.len(), 2);
        assert!(completions[0].external_effect_id.is_none());
        assert!(completions[0].model_request_evidence_id.is_none());
        assert!(completions[1].external_effect_id.is_some());
        assert!(completions[1].model_request_evidence_id.is_some());
    }
}
