use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    HealthScope, Incident, IncidentId, RecoveryPoint, RecoveryPointId, RevalidationRun,
    RevalidationRunId, TrustStateRecord,
};

use crate::ApplicationError;

/// Durable control-plane boundary for incident recovery and trust evidence.
///
/// These records are installation-level evidence. Workspace/resource scoping is
/// carried by the domain `HealthScope`; callers must not infer trust from a
/// process restart or from a record that cannot be read back losslessly.
#[async_trait]
pub trait RecoveryRepository: Send + Sync {
    async fn insert_incident(&self, incident: &Incident) -> Result<(), ApplicationError>;

    async fn find_incident(&self, id: IncidentId) -> Result<Option<Incident>, ApplicationError>;

    async fn insert_revalidation_run(&self, run: &RevalidationRun) -> Result<(), ApplicationError>;

    async fn find_revalidation_run(
        &self,
        id: RevalidationRunId,
    ) -> Result<Option<RevalidationRun>, ApplicationError>;

    async fn insert_trust_state(&self, state: &TrustStateRecord) -> Result<(), ApplicationError>;

    async fn find_latest_trust_state(
        &self,
        scope: &HealthScope,
    ) -> Result<Option<TrustStateRecord>, ApplicationError>;

    async fn insert_recovery_point(&self, point: &RecoveryPoint) -> Result<(), ApplicationError>;

    async fn find_recovery_point(
        &self,
        id: RecoveryPointId,
    ) -> Result<Option<RecoveryPoint>, ApplicationError>;
}

pub type SharedRecoveryRepository = Arc<dyn RecoveryRepository>;
