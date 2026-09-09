use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{ErasureReceipt, VaultReceipt};

use crate::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingOutputKeyBinding, MaterialKeyVault,
    RequestContext, TerminateEmbeddingJobPreDispatch,
};

/// A single immutable output reservation read from the embedding-owned SQL
/// authority.  `receipt` is absent only while the host-vault call has not yet
/// been durably witnessed back into PostgreSQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingOutputKeyPlan {
    pub binding: EmbeddingOutputKeyBinding,
    pub receipt: Option<VaultReceipt>,
    pub retirement_requested: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeliveryOutputIdentity {
    pub output_ordinal: u64,
    pub intent_id: vestrace_domain::MaterialKeyCreationIntentId,
    pub material_id: vestrace_domain::ContentMaterialId,
    pub key_id: vestrace_domain::MaterialKeyId,
    pub nonce: vestrace_domain::IntentNonce,
}
pub struct AcceptDeliveryOutputs {
    pub receipt_id: uuid::Uuid,
    pub idempotency_key: String,
    /// The complete existing delivery/rebuild embedding acceptance authority. Keeping this
    /// value intact prevents the output path from inventing a second job or
    /// external-effect lifecycle.
    pub acceptance: AcceptEmbeddingJob,
    pub outputs: Vec<DeliveryOutputIdentity>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryOutputAcceptanceReceipt {
    pub receipt_id: uuid::Uuid,
    pub job_id: vestrace_domain::EmbeddingJobId,
    pub outputs: Vec<DeliveryOutputIdentity>,
}

/// A durable output-retirement request carries the exact future 14B terminal
/// command. The SQL authority binds every enrolled output to this tuple before
/// any host-vault fence can be published.
#[derive(Clone, Debug)]
pub struct RequestEmbeddingOutputRetirement {
    pub termination: TerminateEmbeddingJobPreDispatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbeddingOutputKeyProgress {
    WaitingForResultKeys,
    Prepared { receipt: VaultReceipt },
    Retired { receipt: ErasureReceipt },
}

/// This port is intentionally embedding-owned.  The generic material intent
/// resumer is not permitted to abandon an active `embedding_job_output`.
#[async_trait]
pub trait EmbeddingOutputKeyRepository: Send + Sync {
    async fn accept_delivery_outputs(
        &self,
        _: &RequestContext,
        _: AcceptDeliveryOutputs,
    ) -> Result<DeliveryOutputAcceptanceReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding delivery/rebuild output acceptance is not configured".into(),
        ))
    }
    async fn claim_next(
        &self,
        context: &RequestContext,
    ) -> Result<Option<EmbeddingOutputKeyPlan>, ApplicationError>;

    async fn request_retirement(
        &self,
        _context: &RequestContext,
        _command: RequestEmbeddingOutputRetirement,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding output retirement is not configured".into(),
        ))
    }

    async fn record_receipt(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: VaultReceipt,
    ) -> Result<EmbeddingOutputKeyProgress, ApplicationError>;

    async fn record_retirement(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError>;
}

pub type SharedEmbeddingOutputKeyRepository = Arc<dyn EmbeddingOutputKeyRepository>;

/// Reconciles exactly one durable reservation.  Each repository method owns a
/// short mutation permit; the vault call is deliberately between them.
pub struct EmbeddingOutputKeyService<V> {
    repository: SharedEmbeddingOutputKeyRepository,
    vault: Arc<V>,
}

impl<V: MaterialKeyVault + 'static> EmbeddingOutputKeyService<V> {
    pub fn new(repository: SharedEmbeddingOutputKeyRepository, vault: Arc<V>) -> Self {
        Self { repository, vault }
    }

    pub async fn reconcile_one(
        &self,
        context: &RequestContext,
    ) -> Result<Option<EmbeddingOutputKeyProgress>, ApplicationError> {
        let Some(plan) = self.repository.claim_next(context).await? else {
            return Ok(None);
        };
        if plan.retirement_requested {
            let receipt = self
                .vault
                .retire_embedding_output(&plan.binding)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            self.repository
                .record_retirement(context, &plan.binding, receipt)
                .await?;
            return Ok(Some(EmbeddingOutputKeyProgress::Retired { receipt }));
        }
        if let Some(receipt) = plan.receipt {
            return Ok(Some(EmbeddingOutputKeyProgress::Prepared { receipt }));
        }
        let receipt = self
            .vault
            .create_embedding_output_if_absent(&plan.binding)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        self.repository
            .record_receipt(context, &plan.binding, receipt)
            .await
            .map(Some)
    }

    pub async fn request_retirement(
        &self,
        context: &RequestContext,
        command: RequestEmbeddingOutputRetirement,
    ) -> Result<(), ApplicationError> {
        self.repository.request_retirement(context, command).await
    }
}
