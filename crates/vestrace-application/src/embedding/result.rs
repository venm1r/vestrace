//! Delivery embedding result preparation.
//!
//! This module owns the boundary between a validated provider embedding
//! response and the short guarded transaction that records its sealed output.
//! It deliberately stops at `ResultPrepared`: key binding, Live material,
//! projection publication and job completion belong to the next lifecycle
//! authority.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    EmbeddingJobId, ExternalEffectId, ExternalEffectReceiptId, MaterialKeyCreationIntentId,
    PreparedMaterialAttachmentId, ZeroizingDek,
};
use zeroize::Zeroizing;

use crate::{
    ApplicationError, EmbeddingOutputKeyBinding, GovernedEmbeddingVector,
    GovernedEmbeddingsResponse, MaterialKeyVault, ProviderDispatchAuthority, RequestContext,
};

pub const EMBEDDING_RESULT_CONFLICT: &str = "EMBEDDING_RESULT_CONFLICT";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingResultPreparationId(uuid::Uuid);

impl EmbeddingResultPreparationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for EmbeddingResultPreparationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingResultPreparedAttachment {
    pub output_ordinal: u64,
    pub intent_id: MaterialKeyCreationIntentId,
    pub attachment_id: PreparedMaterialAttachmentId,
}

/// Caller-allocated values are not replay authority. A committed marker wins
/// even when a racing caller allocated different ids and randomized ciphertext.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultPreparationIdentities {
    pub preparation_id: EmbeddingResultPreparationId,
    pub receipt_id: ExternalEffectReceiptId,
    pub attachments: Vec<EmbeddingResultPreparedAttachment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultDispatchAuthority {
    pub job_id: EmbeddingJobId,
    pub effect_id: ExternalEffectId,
    pub dispatch: ProviderDispatchAuthority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultOutputPlan {
    pub binding: EmbeddingOutputKeyBinding,
    pub expected_dimensions: u32,
}

/// Read before vault access. It contains only bounded identity and structural
/// facts; plaintext, a DEK and ciphertext cannot enter this plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultEligibilityPlan {
    pub job_id: EmbeddingJobId,
    pub effect_id: ExternalEffectId,
    pub expected_job_version: u64,
    pub adapter: String,
    pub response_model: String,
    pub outputs: Vec<EmbeddingResultOutputPlan>,
}

pub struct SealedEmbeddingResultOutput {
    pub binding: EmbeddingOutputKeyBinding,
    pub attachment_id: PreparedMaterialAttachmentId,
    pub dimensions: u32,
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for SealedEmbeddingResultOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SealedEmbeddingResultOutput")
            .field("binding", &self.binding)
            .field("attachment_id", &self.attachment_id)
            .field("dimensions", &self.dimensions)
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingResultPreparationOutcome {
    Prepared {
        preparation_id: EmbeddingResultPreparationId,
    },
    ConvergedExisting {
        preparation_id: EmbeddingResultPreparationId,
    },
}

#[async_trait]
pub trait EmbeddingResultRepository: Send + Sync {
    /// Locks the delivery job/effect authority and inspects an existing complete
    /// marker before any provisional output key can be used.
    async fn load_eligibility(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultDispatchAuthority,
    ) -> Result<EmbeddingResultEligibility, ApplicationError>;

    /// Performs the one short guarded marker transaction. A unique race must
    /// read the marker and converge only on the complete semantic tuple.
    async fn commit_prepared(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultDispatchAuthority,
        identities: EmbeddingResultPreparationIdentities,
        plan: &EmbeddingResultEligibilityPlan,
        outputs: Vec<SealedEmbeddingResultOutput>,
    ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbeddingResultEligibility {
    Existing {
        preparation_id: EmbeddingResultPreparationId,
        /// The persisted marker's complete semantic response tuple.  It is
        /// deliberately checked before any vault callback so a replay cannot
        /// converge merely because it names the same delivery job.
        plan: EmbeddingResultEligibilityPlan,
    },
    Eligible(EmbeddingResultEligibilityPlan),
}

pub trait EmbeddingResultSealer: Send + Sync {
    fn seal_embedding_vector(
        &self,
        binding: &EmbeddingOutputKeyBinding,
        dek: &ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ApplicationError>;
}

/// Seals vectors outside database transactions, then asks the repository for
/// one atomic result-prepared commit.
pub struct EmbeddingResultPreparationService<R, V, S> {
    repository: Arc<R>,
    vault: Arc<V>,
    sealer: Arc<S>,
}

impl<R, V, S> EmbeddingResultPreparationService<R, V, S>
where
    R: EmbeddingResultRepository,
    V: MaterialKeyVault,
    S: EmbeddingResultSealer,
{
    pub fn new(repository: Arc<R>, vault: Arc<V>, sealer: Arc<S>) -> Self {
        Self {
            repository,
            vault,
            sealer,
        }
    }

    pub async fn prepare(
        &self,
        context: RequestContext,
        authority: EmbeddingResultDispatchAuthority,
        identities: EmbeddingResultPreparationIdentities,
        response: GovernedEmbeddingsResponse,
    ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError> {
        match self
            .repository
            .load_eligibility(&context, &authority)
            .await?
        {
            EmbeddingResultEligibility::Existing {
                preparation_id,
                plan,
            } => {
                validate_plan(&authority, &identities, &plan, &response)?;
                Ok(EmbeddingResultPreparationOutcome::ConvergedExisting { preparation_id })
            }
            EmbeddingResultEligibility::Eligible(plan) => {
                validate_plan(&authority, &identities, &plan, &response)?;
                let outputs =
                    seal_outputs(&*self.vault, &*self.sealer, &plan, &identities, response)?;
                self.repository
                    .commit_prepared(&context, &authority, identities, &plan, outputs)
                    .await
            }
        }
    }

    /// Prepares a fresh provider response using the immutable output-intent
    /// identities loaded from the accepted result plan.
    pub async fn prepare_with_generated_identities(
        &self,
        context: RequestContext,
        authority: EmbeddingResultDispatchAuthority,
        response: GovernedEmbeddingsResponse,
    ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError> {
        let eligibility = self
            .repository
            .load_eligibility(&context, &authority)
            .await?;
        let (preparation_id, plan) = match eligibility {
            EmbeddingResultEligibility::Existing {
                preparation_id,
                plan,
            } => {
                let identities = generated_identities(&plan);
                validate_plan(&authority, &identities, &plan, &response)?;
                return Ok(EmbeddingResultPreparationOutcome::ConvergedExisting { preparation_id });
            }
            EmbeddingResultEligibility::Eligible(plan) => {
                (EmbeddingResultPreparationId::new(), plan)
            }
        };
        let identities = EmbeddingResultPreparationIdentities {
            preparation_id,
            receipt_id: ExternalEffectReceiptId::new(),
            attachments: plan
                .outputs
                .iter()
                .enumerate()
                .map(|(ordinal, output)| EmbeddingResultPreparedAttachment {
                    output_ordinal: ordinal as u64,
                    intent_id: output.binding.intent_id,
                    attachment_id: PreparedMaterialAttachmentId::new(),
                })
                .collect(),
        };
        validate_plan(&authority, &identities, &plan, &response)?;
        let outputs = seal_outputs(&*self.vault, &*self.sealer, &plan, &identities, response)?;
        self.repository
            .commit_prepared(&context, &authority, identities, &plan, outputs)
            .await
    }
}

fn generated_identities(
    plan: &EmbeddingResultEligibilityPlan,
) -> EmbeddingResultPreparationIdentities {
    EmbeddingResultPreparationIdentities {
        preparation_id: EmbeddingResultPreparationId::new(),
        receipt_id: ExternalEffectReceiptId::new(),
        attachments: plan
            .outputs
            .iter()
            .enumerate()
            .map(|(ordinal, output)| EmbeddingResultPreparedAttachment {
                output_ordinal: ordinal as u64,
                intent_id: output.binding.intent_id,
                attachment_id: PreparedMaterialAttachmentId::new(),
            })
            .collect(),
    }
}

fn validate_plan(
    authority: &EmbeddingResultDispatchAuthority,
    identities: &EmbeddingResultPreparationIdentities,
    plan: &EmbeddingResultEligibilityPlan,
    response: &GovernedEmbeddingsResponse,
) -> Result<(), ApplicationError> {
    if authority.job_id != plan.job_id
        || authority.effect_id != plan.effect_id
        || authority.dispatch.effect_id != authority.effect_id
        || response.model() != plan.response_model
        || plan.outputs.len() != response.vectors().len()
        || plan.outputs.len() != identities.attachments.len()
        || plan.outputs.is_empty()
    {
        return Err(ApplicationError::Conflict(EMBEDDING_RESULT_CONFLICT.into()));
    }
    for (ordinal, ((output, attachment), vector)) in plan
        .outputs
        .iter()
        .zip(&identities.attachments)
        .zip(response.vectors())
        .enumerate()
    {
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| ApplicationError::Conflict(EMBEDDING_RESULT_CONFLICT.into()))?;
        if output.binding.output_ordinal != ordinal
            || attachment.output_ordinal != ordinal
            || attachment.intent_id != output.binding.intent_id
            || vector.index() != usize::try_from(ordinal).unwrap_or(usize::MAX)
            || vector.components().len() != output.expected_dimensions as usize
        {
            return Err(ApplicationError::Conflict(EMBEDDING_RESULT_CONFLICT.into()));
        }
    }
    Ok(())
}

fn seal_outputs<V, S>(
    vault: &V,
    sealer: &S,
    plan: &EmbeddingResultEligibilityPlan,
    identities: &EmbeddingResultPreparationIdentities,
    response: GovernedEmbeddingsResponse,
) -> Result<Vec<SealedEmbeddingResultOutput>, ApplicationError>
where
    V: MaterialKeyVault,
    S: EmbeddingResultSealer,
{
    plan.outputs
        .iter()
        .zip(&identities.attachments)
        .zip(response.into_vectors())
        .map(|((output, attachment), vector)| {
            let plaintext = canonical_vector_bytes(&vector)?;
            let mut sealed = None;
            let mut seal_error = None;
            vault
                .with_embedding_output_key(&output.binding, &mut |dek| match sealer
                    .seal_embedding_vector(&output.binding, dek, plaintext.as_slice())
                {
                    Ok(ciphertext) => sealed = Some(ciphertext),
                    Err(error) => seal_error = Some(error),
                })
                .map_err(|error| ApplicationError::Policy(error.to_string()))?;
            if let Some(error) = seal_error {
                return Err(error);
            }
            let ciphertext = sealed.ok_or_else(|| {
                ApplicationError::Internal(
                    "embedding output vault callback did not seal a vector".into(),
                )
            })?;
            Ok(SealedEmbeddingResultOutput {
                binding: output.binding.clone(),
                attachment_id: attachment.attachment_id,
                dimensions: output.expected_dimensions,
                ciphertext,
            })
        })
        .collect()
}

/// Canonical IEEE-754 binary32 big-endian bytes. The only plaintext scratch is
/// zeroized when it leaves the sealing callback.
fn canonical_vector_bytes(
    vector: &GovernedEmbeddingVector,
) -> Result<Zeroizing<Vec<u8>>, ApplicationError> {
    let capacity = vector
        .components()
        .len()
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| {
            ApplicationError::Policy("embedding vector exceeds bounded encoding".into())
        })?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(capacity));
    for component in vector.components() {
        if !component.is_finite() {
            return Err(ApplicationError::Policy(
                "embedding vector is non-finite".into(),
            ));
        }
        bytes.extend_from_slice(&component.to_bits().to_be_bytes());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use chrono::Utc;
    use vestrace_domain::{
        ConnectionId, ConnectionRevisionId, ExternalEffectId, ExternalEffectLifecycleTransitionId,
        PolicyDecisionId, PrincipalId, WorkspaceId,
    };

    use super::*;

    #[test]
    fn canonical_encoding_is_big_endian_binary32() {
        let vector =
            GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0_f32, -0.0_f32])).unwrap();
        assert_eq!(
            canonical_vector_bytes(&vector).unwrap().as_slice(),
            &[0x3f, 0x80, 0, 0, 0x80, 0, 0, 0]
        );
    }

    struct ExistingRepository {
        preparation_id: EmbeddingResultPreparationId,
        plan: EmbeddingResultEligibilityPlan,
        commits: AtomicUsize,
    }

    #[async_trait]
    impl EmbeddingResultRepository for ExistingRepository {
        async fn load_eligibility(
            &self,
            _context: &RequestContext,
            _authority: &EmbeddingResultDispatchAuthority,
        ) -> Result<EmbeddingResultEligibility, ApplicationError> {
            Ok(EmbeddingResultEligibility::Existing {
                preparation_id: self.preparation_id,
                plan: self.plan.clone(),
            })
        }

        async fn commit_prepared(
            &self,
            _context: &RequestContext,
            _authority: &EmbeddingResultDispatchAuthority,
            _identities: EmbeddingResultPreparationIdentities,
            _plan: &EmbeddingResultEligibilityPlan,
            _outputs: Vec<SealedEmbeddingResultOutput>,
        ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            panic!("a committed marker must converge before sealing or commit")
        }
    }

    struct NeverVault;

    impl MaterialKeyVault for NeverVault {
        fn create_if_absent(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
            _nonce: vestrace_domain::IntentNonce,
        ) -> Result<vestrace_domain::VaultReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn unwrap(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
            _use_dek: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            panic!("existing marker must not call the ordinary vault path")
        }

        fn prepare_erasure(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
        ) -> Result<crate::FenceReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn erase(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
        ) -> Result<vestrace_domain::ErasureReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn with_embedding_output_key(
            &self,
            _binding: &EmbeddingOutputKeyBinding,
            _use_dek: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            panic!("existing marker must not lend an output key")
        }
    }

    struct NeverSealer;

    impl EmbeddingResultSealer for NeverSealer {
        fn seal_embedding_vector(
            &self,
            _binding: &EmbeddingOutputKeyBinding,
            _dek: &ZeroizingDek,
            _plaintext: &[u8],
        ) -> Result<Vec<u8>, ApplicationError> {
            panic!("existing marker must not reseal provider output")
        }
    }

    struct EligibleRepository {
        plan: EmbeddingResultEligibilityPlan,
        commits: AtomicUsize,
    }

    #[async_trait]
    impl EmbeddingResultRepository for EligibleRepository {
        async fn load_eligibility(
            &self,
            _context: &RequestContext,
            _authority: &EmbeddingResultDispatchAuthority,
        ) -> Result<EmbeddingResultEligibility, ApplicationError> {
            Ok(EmbeddingResultEligibility::Eligible(self.plan.clone()))
        }

        async fn commit_prepared(
            &self,
            _context: &RequestContext,
            _authority: &EmbeddingResultDispatchAuthority,
            identities: EmbeddingResultPreparationIdentities,
            _plan: &EmbeddingResultEligibilityPlan,
            _outputs: Vec<SealedEmbeddingResultOutput>,
        ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Ok(EmbeddingResultPreparationOutcome::Prepared {
                preparation_id: identities.preparation_id,
            })
        }
    }

    struct CountingVault(AtomicUsize);

    impl MaterialKeyVault for CountingVault {
        fn create_if_absent(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
            _nonce: vestrace_domain::IntentNonce,
        ) -> Result<vestrace_domain::VaultReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn unwrap(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
            _use_dek: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            Err(crate::VaultError::Provisional)
        }

        fn prepare_erasure(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
        ) -> Result<crate::FenceReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn erase(
            &self,
            _key_id: vestrace_domain::MaterialKeyId,
        ) -> Result<vestrace_domain::ErasureReceipt, crate::VaultError> {
            Err(crate::VaultError::Unavailable)
        }

        fn with_embedding_output_key(
            &self,
            _binding: &EmbeddingOutputKeyBinding,
            use_dek: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let dek = ZeroizingDek::new([0xA5; 32]);
            use_dek(&dek);
            Ok(())
        }
    }

    struct CountingSealer(AtomicUsize);

    impl EmbeddingResultSealer for CountingSealer {
        fn seal_embedding_vector(
            &self,
            _binding: &EmbeddingOutputKeyBinding,
            _dek: &ZeroizingDek,
            plaintext: &[u8],
        ) -> Result<Vec<u8>, ApplicationError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(plaintext.to_vec())
        }
    }

    fn planned_authority_and_output(
        dimensions: u32,
    ) -> (
        EmbeddingResultDispatchAuthority,
        EmbeddingResultOutputPlan,
        EmbeddingResultPreparedAttachment,
    ) {
        let effect_id = ExternalEffectId::new();
        let job_id = EmbeddingJobId::new();
        let binding = EmbeddingOutputKeyBinding {
            workspace_id: WorkspaceId::new(),
            job_id,
            intent_id: vestrace_domain::MaterialKeyCreationIntentId::new(),
            material_id: vestrace_domain::ContentMaterialId::new(),
            key_id: vestrace_domain::MaterialKeyId::new(),
            nonce: vestrace_domain::IntentNonce::new(),
            output_ordinal: 0,
        };
        let attachment = EmbeddingResultPreparedAttachment {
            output_ordinal: 0,
            intent_id: binding.intent_id,
            attachment_id: vestrace_domain::PreparedMaterialAttachmentId::new(),
        };
        let authority = EmbeddingResultDispatchAuthority {
            job_id,
            effect_id,
            dispatch: ProviderDispatchAuthority {
                effect_id,
                authorization_id: PolicyDecisionId::new(),
                connection_id: ConnectionId::new(),
                connection_revision_id: ConnectionRevisionId::new(),
                concurrency_lease_id: uuid::Uuid::now_v7(),
                credential_lease_id: None,
                dispatch_transition_id: ExternalEffectLifecycleTransitionId::new(),
                dispatch_expires_at: Utc::now(),
            },
        };
        (
            authority,
            EmbeddingResultOutputPlan {
                binding,
                expected_dimensions: dimensions,
            },
            attachment,
        )
    }

    #[tokio::test]
    async fn model_or_width_mismatch_is_refused_before_vault_or_commit() {
        let (authority, output, attachment) = planned_authority_and_output(2);
        let repository = Arc::new(EligibleRepository {
            plan: EmbeddingResultEligibilityPlan {
                job_id: authority.job_id,
                effect_id: authority.effect_id,
                expected_job_version: 1,
                adapter: "openai-compatible".into(),
                response_model: "locked-model".into(),
                outputs: vec![output],
            },
            commits: AtomicUsize::new(0),
        });
        let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
        let sealer = Arc::new(CountingSealer(AtomicUsize::new(0)));
        let service = EmbeddingResultPreparationService::new(
            repository.clone(),
            vault.clone(),
            sealer.clone(),
        );
        let response = GovernedEmbeddingsResponse::new(
            "wrong-model",
            "wrong-model".into(),
            vec![GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0])).unwrap()],
            1,
        )
        .unwrap();
        let error = service
            .prepare(
                RequestContext::new(
                    output_binding_workspace(&repository.plan),
                    PrincipalId::new(),
                ),
                authority,
                EmbeddingResultPreparationIdentities {
                    preparation_id: EmbeddingResultPreparationId::new(),
                    receipt_id: vestrace_domain::ExternalEffectReceiptId::new(),
                    attachments: vec![attachment],
                },
                response,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, ApplicationError::Conflict(_)));
        assert_eq!(vault.0.load(Ordering::SeqCst), 0);
        assert_eq!(sealer.0.load(Ordering::SeqCst), 0);
        assert_eq!(repository.commits.load(Ordering::SeqCst), 0);
    }

    fn output_binding_workspace(plan: &EmbeddingResultEligibilityPlan) -> WorkspaceId {
        plan.outputs[0].binding.workspace_id
    }

    #[tokio::test]
    async fn marker_first_replay_converges_without_vault_or_ciphertext_comparison() {
        let preparation_id = EmbeddingResultPreparationId::new();
        let (authority, output, attachment) = planned_authority_and_output(2);
        let plan = EmbeddingResultEligibilityPlan {
            job_id: authority.job_id,
            effect_id: authority.effect_id,
            expected_job_version: 1,
            adapter: "openai-compatible".into(),
            response_model: "replayed-model".into(),
            outputs: vec![output],
        };
        let repository = Arc::new(ExistingRepository {
            preparation_id,
            plan: plan.clone(),
            commits: AtomicUsize::new(0),
        });
        let service = EmbeddingResultPreparationService::new(
            repository.clone(),
            Arc::new(NeverVault),
            Arc::new(NeverSealer),
        );
        let response = GovernedEmbeddingsResponse::new(
            "replayed-model",
            "replayed-model".into(),
            vec![GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0, 2.0])).unwrap()],
            1,
        )
        .unwrap();
        let outcome = service
            .prepare(
                RequestContext::new(output_binding_workspace(&plan), PrincipalId::new()),
                authority,
                EmbeddingResultPreparationIdentities {
                    preparation_id: EmbeddingResultPreparationId::new(),
                    receipt_id: vestrace_domain::ExternalEffectReceiptId::new(),
                    attachments: vec![attachment],
                },
                response,
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            EmbeddingResultPreparationOutcome::ConvergedExisting { preparation_id }
        );
        assert_eq!(repository.commits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn marker_first_semantic_mismatches_refuse_before_vault_or_sealer() {
        let (authority, output, attachment) = planned_authority_and_output(2);
        let matching_plan = EmbeddingResultEligibilityPlan {
            job_id: authority.job_id,
            effect_id: authority.effect_id,
            expected_job_version: 1,
            adapter: "openai-compatible".into(),
            response_model: "locked-model".into(),
            outputs: vec![output],
        };

        for mismatch in ["model", "count", "index", "dimensions"] {
            let mut plan = matching_plan.clone();
            let response = match mismatch {
                "model" => GovernedEmbeddingsResponse::new(
                    "other-model",
                    "other-model".into(),
                    vec![GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0, 2.0])).unwrap()],
                    1,
                )
                .unwrap(),
                "count" => GovernedEmbeddingsResponse::new(
                    "locked-model",
                    "locked-model".into(),
                    vec![
                        GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0, 2.0])).unwrap(),
                        GovernedEmbeddingVector::new(1, Zeroizing::new(vec![3.0, 4.0])).unwrap(),
                    ],
                    2,
                )
                .unwrap(),
                "index" => {
                    plan.outputs[0].binding.output_ordinal = 1;
                    GovernedEmbeddingsResponse::new(
                        "locked-model",
                        "locked-model".into(),
                        vec![
                            GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0, 2.0]))
                                .unwrap(),
                        ],
                        1,
                    )
                    .unwrap()
                }
                "dimensions" => {
                    plan.outputs[0].expected_dimensions = 3;
                    GovernedEmbeddingsResponse::new(
                        "locked-model",
                        "locked-model".into(),
                        vec![
                            GovernedEmbeddingVector::new(0, Zeroizing::new(vec![1.0, 2.0]))
                                .unwrap(),
                        ],
                        1,
                    )
                    .unwrap()
                }
                _ => unreachable!(),
            };
            let repository = Arc::new(ExistingRepository {
                preparation_id: EmbeddingResultPreparationId::new(),
                plan,
                commits: AtomicUsize::new(0),
            });
            let service = EmbeddingResultPreparationService::new(
                repository.clone(),
                Arc::new(NeverVault),
                Arc::new(NeverSealer),
            );
            let result = service
                .prepare(
                    RequestContext::new(
                        output_binding_workspace(&matching_plan),
                        PrincipalId::new(),
                    ),
                    authority.clone(),
                    EmbeddingResultPreparationIdentities {
                        preparation_id: EmbeddingResultPreparationId::new(),
                        receipt_id: vestrace_domain::ExternalEffectReceiptId::new(),
                        attachments: vec![attachment],
                    },
                    response,
                )
                .await;
            assert!(
                matches!(result, Err(ApplicationError::Conflict(_))),
                "{mismatch}"
            );
            assert_eq!(repository.commits.load(Ordering::SeqCst), 0, "{mismatch}");
        }
    }
}
