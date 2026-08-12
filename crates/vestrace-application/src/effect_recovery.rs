use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::Timestamp;
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation, ObservedEffectState,
};

use crate::{
    ApplicationError, ExternalEffectReconciliationService, ExternalEffectRecoveryCandidate,
    RequestContext, SharedExternalEffectRepository,
};

#[async_trait]
pub trait ExternalEffectReadBackAdapter: Send + Sync {
    async fn observe(
        &self,
        intent: &ExternalEffectIntent,
        receipt: &ExternalEffectReceipt,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExternalEffectRecoveryReport {
    reconciliations: Vec<ExternalReconciliation>,
}

impl ExternalEffectRecoveryReport {
    pub fn reconciliations(&self) -> &[ExternalReconciliation] {
        &self.reconciliations
    }
}

pub struct ExternalEffectRecoveryService {
    repository: SharedExternalEffectRepository,
    read_back: Arc<dyn ExternalEffectReadBackAdapter>,
    reconciliation: ExternalEffectReconciliationService,
}

impl ExternalEffectRecoveryService {
    pub fn new(
        repository: SharedExternalEffectRepository,
        read_back: Arc<dyn ExternalEffectReadBackAdapter>,
    ) -> Self {
        Self {
            repository,
            read_back,
            reconciliation: ExternalEffectReconciliationService::new(),
        }
    }

    pub async fn discover(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        self.repository
            .find_reconciliation_candidates(context.workspace_id)
            .await
    }

    pub async fn reconcile_candidate(
        &self,
        context: &RequestContext,
        candidate: &ExternalEffectRecoveryCandidate,
        reconciled_at: Timestamp,
    ) -> Result<ExternalReconciliation, ApplicationError> {
        let observations = self
            .read_back
            .observe(candidate.intent(), candidate.receipt())
            .await?;
        let reconciliation = self.reconciliation.reconcile(
            context,
            candidate.intent(),
            candidate.receipt(),
            observations,
            reconciled_at,
        )?;
        self.repository
            .insert_reconciliation(&reconciliation)
            .await?;
        Ok(reconciliation)
    }

    pub async fn run(
        &self,
        context: &RequestContext,
        reconciled_at: Timestamp,
    ) -> Result<ExternalEffectRecoveryReport, ApplicationError> {
        let candidates = self.discover(context).await?;
        let mut reconciliations = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            reconciliations.push(
                self.reconcile_candidate(context, candidate, reconciled_at)
                    .await?,
            );
        }
        Ok(ExternalEffectRecoveryReport { reconciliations })
    }
}
