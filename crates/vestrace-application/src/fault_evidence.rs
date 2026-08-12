use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::DomainError;
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, FaultObservation,
};

use crate::{ApplicationError, ExternalEffectFaultSuiteReport};

pub type Timestamp = DateTime<Utc>;

use async_trait::async_trait;

#[async_trait]
pub trait FaultSuiteEvidenceRepository: Send + Sync {
    async fn insert(
        &self,
        evidence: &ExternalEffectFaultSuiteEvidence,
    ) -> Result<(), ApplicationError>;

    async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ExternalEffectFaultSuiteEvidence>, ApplicationError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalEffectFaultObservationEvidence {
    pub point: EffectFaultPoint,
    pub status: EffectLifecycleStatus,
    pub retry_attempted: bool,
    pub reconciliation_started: bool,
    pub receipt_persisted: bool,
}

impl From<&FaultObservation> for ExternalEffectFaultObservationEvidence {
    fn from(observation: &FaultObservation) -> Self {
        Self {
            point: observation.point,
            status: observation.status,
            retry_attempted: observation.retry_attempted,
            reconciliation_started: observation.reconciliation_started,
            receipt_persisted: observation.receipt_persisted,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalEffectFaultSuiteEvidence {
    id: Uuid,
    target_digest: String,
    observations: Vec<ExternalEffectFaultObservationEvidence>,
    passed: bool,
    failures: Vec<String>,
    created_at: Timestamp,
}

impl ExternalEffectFaultSuiteEvidence {
    pub fn from_report(
        target_digest: impl Into<String>,
        report: &ExternalEffectFaultSuiteReport,
        created_at: Timestamp,
    ) -> Result<Self, ApplicationError> {
        let target_digest = target_digest.into();
        if target_digest.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "fault-suite evidence target digest must not be blank".into(),
            )
            .into());
        }
        Ok(Self {
            id: Uuid::now_v7(),
            target_digest,
            observations: report
                .observations()
                .iter()
                .map(ExternalEffectFaultObservationEvidence::from)
                .collect(),
            passed: report.decision().is_passed(),
            failures: report.decision().failures().to_vec(),
            created_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn target_digest(&self) -> &str {
        &self.target_digest
    }

    pub fn observations(&self) -> &[ExternalEffectFaultObservationEvidence] {
        &self.observations
    }

    pub fn is_passed(&self) -> bool {
        self.passed
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}
