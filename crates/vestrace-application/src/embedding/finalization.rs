//! Resumable delivery publication. No provider authority is available here.
use crate::{
    ApplicationError, EmbeddingOutputKeyBinding, EmbeddingResultPreparationId, MaterialKeyVault,
    RequestContext,
};
use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::{EmbeddingJobId, ExternalEffectId, MaterialKeyBindingReceipt, ZeroizingDek};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultFinalizationAuthority {
    pub preparation_id: EmbeddingResultPreparationId,
    pub job_id: EmbeddingJobId,
    pub effect_id: ExternalEffectId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingResultPublication {
    pub publication_id: uuid::Uuid,
    pub preparation_id: EmbeddingResultPreparationId,
    pub job_id: EmbeddingJobId,
    pub effect_id: ExternalEffectId,
    pub space_registration_id: uuid::Uuid,
    pub rebuild_event_id: uuid::Uuid,
    pub output_count: u64,
    pub terminal_job_version: u64,
    pub resulting_corpus_revision: u64,
    pub live_member_count: u64,
    pub generation_epoch: u64,
}
#[derive(Clone)]
pub struct EmbeddingResultBoundOutput {
    pub binding: EmbeddingOutputKeyBinding,
    pub projection_id: uuid::Uuid,
    pub receipt: MaterialKeyBindingReceipt,
    pub ciphertext: Vec<u8>,
}
impl std::fmt::Debug for EmbeddingResultBoundOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingResultBoundOutput")
            .field("binding", &self.binding)
            .field("projection_id", &self.projection_id)
            .field("receipt", &self.receipt)
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}
#[derive(Clone)]
pub struct EmbeddingOutputCommitment {
    pub projection_id: uuid::Uuid,
    pub output_ordinal: u64,
    pub commitment: [u8; 32],
}
impl std::fmt::Debug for EmbeddingOutputCommitment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingOutputCommitment")
            .field("projection_id", &self.projection_id)
            .field("output_ordinal", &self.output_ordinal)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug)]
pub enum EmbeddingResultFinalizationProgress {
    Published(EmbeddingResultPublication),
    NeedsBinding(Vec<EmbeddingOutputKeyBinding>),
    ReadyToPublish(Vec<EmbeddingResultBoundOutput>),
}
#[async_trait]
pub trait EmbeddingResultFinalizationRepository: Send + Sync {
    /// Reads the durable ResultPrepared identity for recovery.  An executor may
    /// resume finalization, but it must not invent a preparation id from the
    /// job or effect identity.
    async fn load_preparation_id(
        &self,
        _context: &RequestContext,
        _job_id: EmbeddingJobId,
        _effect_id: ExternalEffectId,
    ) -> Result<EmbeddingResultPreparationId, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding result finalization recovery is not configured".into(),
        ))
    }

    async fn load_progress(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
    ) -> Result<EmbeddingResultFinalizationProgress, ApplicationError>;
    async fn record_binding(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        binding: &EmbeddingOutputKeyBinding,
        receipt: &MaterialKeyBindingReceipt,
    ) -> Result<MaterialKeyBindingReceipt, ApplicationError>;
    async fn publish(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        publication_id: uuid::Uuid,
        rebuild_event_id: uuid::Uuid,
        commitments: Vec<EmbeddingOutputCommitment>,
    ) -> Result<EmbeddingResultPublication, ApplicationError>;
}
pub trait EmbeddingResultCommitter: Send + Sync {
    fn commitment(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        output: &EmbeddingResultBoundOutput,
        dek: &ZeroizingDek,
    ) -> Result<[u8; 32], ApplicationError>;
}
pub struct EmbeddingResultFinalizationService<R, V, C> {
    repository: Arc<R>,
    vault: Arc<V>,
    committer: Arc<C>,
}
fn conflict() -> ApplicationError {
    ApplicationError::Conflict("EMBEDDING_RESULT_FINALIZATION_CONFLICT".into())
}
fn vault_error(error: crate::VaultError) -> ApplicationError {
    if error == crate::VaultError::Unavailable {
        ApplicationError::Unavailable("embedding output vault unavailable".into())
    } else {
        conflict()
    }
}
fn validate_publication(
    p: &EmbeddingResultPublication,
    a: &EmbeddingResultFinalizationAuthority,
) -> Result<(), ApplicationError> {
    if p.preparation_id != a.preparation_id
        || p.job_id != a.job_id
        || p.effect_id != a.effect_id
        || p.output_count == 0
        || [
            p.output_count,
            p.terminal_job_version,
            p.resulting_corpus_revision,
            p.live_member_count,
            p.generation_epoch,
        ]
        .iter()
        .any(|n| i64::try_from(*n).is_err())
    {
        return Err(conflict());
    }
    Ok(())
}
fn validate_bindings<'a>(
    context: &RequestContext,
    authority: &EmbeddingResultFinalizationAuthority,
    bindings: impl Iterator<Item = &'a EmbeddingOutputKeyBinding>,
    complete: bool,
) -> Result<(), ApplicationError> {
    let mut previous = None;
    let mut intents = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    let mut materials = std::collections::HashSet::new();
    for (position, binding) in bindings.enumerate() {
        if binding.workspace_id != context.workspace_id
            || binding.job_id != authority.job_id
            || i64::try_from(binding.output_ordinal).is_err()
            || previous.is_some_and(|ordinal| ordinal >= binding.output_ordinal)
            || (complete && u64::try_from(position).ok() != Some(binding.output_ordinal))
            || !intents.insert(binding.intent_id)
            || !keys.insert(binding.key_id)
            || !materials.insert(binding.material_id)
        {
            return Err(conflict());
        }
        previous = Some(binding.output_ordinal);
    }
    if previous.is_none() {
        return Err(conflict());
    }
    Ok(())
}
impl<R: EmbeddingResultFinalizationRepository, V: MaterialKeyVault, C: EmbeddingResultCommitter>
    EmbeddingResultFinalizationService<R, V, C>
{
    pub fn new(repository: Arc<R>, vault: Arc<V>, committer: Arc<C>) -> Self {
        Self {
            repository,
            vault,
            committer,
        }
    }
    pub async fn finalize(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
    ) -> Result<EmbeddingResultPublication, ApplicationError> {
        loop {
            let progress = self.repository.load_progress(context, authority).await?;
            match progress {
                EmbeddingResultFinalizationProgress::Published(publication) => {
                    validate_publication(&publication, authority)?;
                    return Ok(publication);
                }
                EmbeddingResultFinalizationProgress::NeedsBinding(mut bindings) => {
                    bindings.sort_by_key(|b| b.output_ordinal);
                    validate_bindings(context, authority, bindings.iter(), false)?;
                    for binding in bindings {
                        let receipt = self
                            .vault
                            .bind_embedding_output(&binding, authority.preparation_id)
                            .map_err(vault_error)?;
                        let recorded = self
                            .repository
                            .record_binding(context, authority, &binding, &receipt)
                            .await?;
                        if recorded != receipt {
                            return Err(conflict());
                        }
                    }
                }
                EmbeddingResultFinalizationProgress::ReadyToPublish(outputs) => {
                    validate_bindings(
                        context,
                        authority,
                        outputs.iter().map(|o| &o.binding),
                        true,
                    )?;
                    let mut projections = std::collections::HashSet::new();
                    if outputs
                        .iter()
                        .any(|o| !projections.insert(o.projection_id) || o.ciphertext.is_empty())
                    {
                        return Err(conflict());
                    }
                    let mut commitments = Vec::with_capacity(outputs.len());
                    for output in &outputs {
                        let mut result = None;
                        let mut calls = 0;
                        self.vault
                            .with_bound_embedding_output_key(
                                &output.binding,
                                authority.preparation_id,
                                output.receipt,
                                &mut |dek| {
                                    calls += 1;
                                    if calls == 1 {
                                        result = Some(
                                            self.committer
                                                .commitment(context, authority, output, dek),
                                        );
                                    }
                                },
                            )
                            .map_err(vault_error)?;
                        if calls != 1 {
                            return Err(conflict());
                        }
                        commitments.push(EmbeddingOutputCommitment {
                            projection_id: output.projection_id,
                            output_ordinal: output.binding.output_ordinal,
                            commitment: result.ok_or_else(conflict)??,
                        });
                    }
                    let publication = self
                        .repository
                        .publish(
                            context,
                            authority,
                            uuid::Uuid::now_v7(),
                            uuid::Uuid::now_v7(),
                            commitments,
                        )
                        .await?;
                    validate_publication(&publication, authority)?;
                    if u64::try_from(outputs.len()).ok() != Some(publication.output_count) {
                        return Err(conflict());
                    }
                    return Ok(publication);
                }
            }
        }
    }

    pub async fn finalize_recovered(
        &self,
        context: &RequestContext,
        job_id: EmbeddingJobId,
        effect_id: ExternalEffectId,
    ) -> Result<EmbeddingResultPublication, ApplicationError> {
        let preparation_id = self
            .repository
            .load_preparation_id(context, job_id, effect_id)
            .await?;
        self.finalize(
            context,
            &EmbeddingResultFinalizationAuthority {
                preparation_id,
                job_id,
                effect_id,
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use vestrace_domain::{
        ErasureReceipt, IntentNonce, MaterialKeyId, PrincipalId, VaultReceipt, WorkspaceId,
    };
    struct Repo {
        publication: EmbeddingResultPublication,
        outputs: Vec<EmbeddingResultBoundOutput>,
        recorded: Mutex<Vec<u64>>,
        published: AtomicUsize,
        already_published: bool,
    }
    #[async_trait]
    impl EmbeddingResultFinalizationRepository for Repo {
        async fn load_progress(
            &self,
            _: &RequestContext,
            a: &EmbeddingResultFinalizationAuthority,
        ) -> Result<EmbeddingResultFinalizationProgress, ApplicationError> {
            if self.already_published {
                return Ok(EmbeddingResultFinalizationProgress::Published(
                    self.publication.clone(),
                ));
            }
            validate_publication(&self.publication, a)?;
            let done = self.recorded.lock().unwrap();
            let missing: Vec<_> = self
                .outputs
                .iter()
                .filter(|o| !done.contains(&o.binding.output_ordinal))
                .map(|o| o.binding.clone())
                .collect();
            Ok(if missing.is_empty() {
                EmbeddingResultFinalizationProgress::ReadyToPublish(self.outputs.clone())
            } else {
                EmbeddingResultFinalizationProgress::NeedsBinding(missing)
            })
        }
        async fn record_binding(
            &self,
            _: &RequestContext,
            _: &EmbeddingResultFinalizationAuthority,
            b: &EmbeddingOutputKeyBinding,
            r: &MaterialKeyBindingReceipt,
        ) -> Result<MaterialKeyBindingReceipt, ApplicationError> {
            if !self
                .outputs
                .iter()
                .any(|o| o.binding == *b && o.receipt == *r)
            {
                return Err(conflict());
            }
            self.recorded.lock().unwrap().push(b.output_ordinal);
            Ok(*r)
        }
        async fn publish(
            &self,
            _: &RequestContext,
            _: &EmbeddingResultFinalizationAuthority,
            _: uuid::Uuid,
            _: uuid::Uuid,
            c: Vec<EmbeddingOutputCommitment>,
        ) -> Result<EmbeddingResultPublication, ApplicationError> {
            assert_eq!(c.len(), self.outputs.len());
            self.published.fetch_add(1, Ordering::SeqCst);
            Ok(self.publication.clone())
        }
    }
    struct Vault {
        outputs: Vec<EmbeddingResultBoundOutput>,
        preparation: EmbeddingResultPreparationId,
        binds: Mutex<Vec<u64>>,
        uses: AtomicUsize,
        fail_bind: AtomicUsize,
        never: bool,
        omit_callback: bool,
    }
    impl MaterialKeyVault for Vault {
        fn create_if_absent(
            &self,
            _: MaterialKeyId,
            _: IntentNonce,
        ) -> Result<VaultReceipt, crate::VaultError> {
            panic!("ordinary creation")
        }
        fn unwrap(
            &self,
            _: MaterialKeyId,
            _: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            panic!("ordinary unwrap")
        }
        fn prepare_erasure(
            &self,
            _: MaterialKeyId,
        ) -> Result<crate::FenceReceipt, crate::VaultError> {
            panic!("ordinary erasure")
        }
        fn erase(&self, _: MaterialKeyId) -> Result<ErasureReceipt, crate::VaultError> {
            panic!("ordinary erase")
        }
        fn bind_embedding_output(
            &self,
            b: &EmbeddingOutputKeyBinding,
            p: EmbeddingResultPreparationId,
        ) -> Result<MaterialKeyBindingReceipt, crate::VaultError> {
            self.binds.lock().unwrap().push(b.output_ordinal);
            if self.never || self.fail_bind.load(Ordering::SeqCst) == b.output_ordinal as usize {
                return Err(crate::VaultError::Unavailable);
            }
            if p != self.preparation {
                return Err(crate::VaultError::BindingMismatch);
            }
            self.outputs
                .iter()
                .find(|o| o.binding == *b)
                .map(|o| o.receipt)
                .ok_or(crate::VaultError::BindingMismatch)
        }
        fn with_bound_embedding_output_key(
            &self,
            b: &EmbeddingOutputKeyBinding,
            p: EmbeddingResultPreparationId,
            r: MaterialKeyBindingReceipt,
            callback: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), crate::VaultError> {
            self.uses.fetch_add(1, Ordering::SeqCst);
            if self.never {
                return Err(crate::VaultError::Unavailable);
            }
            if p != self.preparation
                || !self
                    .outputs
                    .iter()
                    .any(|o| o.binding == *b && o.receipt == r)
            {
                return Err(crate::VaultError::BindingMismatch);
            }
            if !self.omit_callback {
                callback(&ZeroizingDek::new([42; 32]));
            }
            Ok(())
        }
    }
    struct Committer(bool);
    impl EmbeddingResultCommitter for Committer {
        fn commitment(
            &self,
            _: &RequestContext,
            _: &EmbeddingResultFinalizationAuthority,
            _: &EmbeddingResultBoundOutput,
            _: &ZeroizingDek,
        ) -> Result<[u8; 32], ApplicationError> {
            if self.0 { Err(conflict()) } else { Ok([7; 32]) }
        }
    }
    fn fixture() -> (
        RequestContext,
        EmbeddingResultFinalizationAuthority,
        Repo,
        Vault,
    ) {
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
        let a = EmbeddingResultFinalizationAuthority {
            preparation_id: EmbeddingResultPreparationId::new(),
            job_id: EmbeddingJobId::new(),
            effect_id: ExternalEffectId::new(),
        };
        let outputs: Vec<_> = (0..2)
            .map(|output_ordinal| EmbeddingResultBoundOutput {
                binding: EmbeddingOutputKeyBinding {
                    workspace_id: context.workspace_id,
                    job_id: a.job_id,
                    intent_id: vestrace_domain::MaterialKeyCreationIntentId::new(),
                    material_id: vestrace_domain::ContentMaterialId::new(),
                    key_id: MaterialKeyId::new(),
                    nonce: IntentNonce::new(),
                    output_ordinal,
                },
                projection_id: uuid::Uuid::now_v7(),
                receipt: MaterialKeyBindingReceipt::new(),
                ciphertext: vec![81, 82, 83],
            })
            .collect();
        let publication = EmbeddingResultPublication {
            publication_id: uuid::Uuid::now_v7(),
            preparation_id: a.preparation_id,
            job_id: a.job_id,
            effect_id: a.effect_id,
            space_registration_id: uuid::Uuid::now_v7(),
            rebuild_event_id: uuid::Uuid::now_v7(),
            output_count: 2,
            terminal_job_version: 3,
            resulting_corpus_revision: 4,
            live_member_count: 2,
            generation_epoch: 1,
        };
        let vault = Vault {
            outputs: outputs.clone(),
            preparation: a.preparation_id,
            binds: Mutex::new(vec![]),
            uses: AtomicUsize::new(0),
            fail_bind: AtomicUsize::new(usize::MAX),
            never: false,
            omit_callback: false,
        };
        (
            context,
            a,
            Repo {
                publication,
                outputs,
                recorded: Mutex::new(vec![]),
                published: AtomicUsize::new(0),
                already_published: false,
            },
            vault,
        )
    }
    #[tokio::test]
    async fn published_replay_never_touches_vault() {
        let (c, a, mut r, mut v) = fixture();
        r.already_published = true;
        v.never = true;
        let r = Arc::new(r);
        let v = Arc::new(v);
        let expected = r.publication.clone();
        assert_eq!(
            EmbeddingResultFinalizationService::new(
                r.clone(),
                v.clone(),
                Arc::new(Committer(true))
            )
            .finalize(&c, &a)
            .await
            .unwrap(),
            expected
        );
        assert!(v.binds.lock().unwrap().is_empty());
        assert_eq!(v.uses.load(Ordering::SeqCst), 0);
        assert_eq!(r.published.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn malformed_published_identity_is_refused_before_vault() {
        for changed in 0..3 {
            let (context, authority, mut repository, mut vault) = fixture();
            repository.already_published = true;
            vault.never = true;
            match changed {
                0 => repository.publication.preparation_id = EmbeddingResultPreparationId::new(),
                1 => repository.publication.job_id = EmbeddingJobId::new(),
                _ => repository.publication.effect_id = ExternalEffectId::new(),
            }
            let repository = Arc::new(repository);
            let vault = Arc::new(vault);
            let result = EmbeddingResultFinalizationService::new(
                repository.clone(),
                vault.clone(),
                Arc::new(Committer(true)),
            )
            .finalize(&context, &authority)
            .await;
            assert!(matches!(result, Err(ApplicationError::Conflict(_))));
            assert!(vault.binds.lock().unwrap().is_empty());
            assert_eq!(vault.uses.load(Ordering::SeqCst), 0);
            assert_eq!(repository.published.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn partial_binding_resumes_missing_only() {
        let (c, a, r, v) = fixture();
        v.fail_bind.store(1, Ordering::SeqCst);
        let r = Arc::new(r);
        let v = Arc::new(v);
        let service = EmbeddingResultFinalizationService::new(
            r.clone(),
            v.clone(),
            Arc::new(Committer(false)),
        );
        assert!(service.finalize(&c, &a).await.is_err());
        assert_eq!(*r.recorded.lock().unwrap(), vec![0]);
        assert_eq!(r.published.load(Ordering::SeqCst), 0);
        v.fail_bind.store(usize::MAX, Ordering::SeqCst);
        service.finalize(&c, &a).await.unwrap();
        assert_eq!(*v.binds.lock().unwrap(), vec![0, 1, 1]);
        assert_eq!(r.published.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn changed_preparation_or_binding_is_refused() {
        let (c, mut a, r, v) = fixture();
        a.preparation_id = EmbeddingResultPreparationId::new();
        let v = Arc::new(v);
        assert!(
            EmbeddingResultFinalizationService::new(
                Arc::new(r),
                v.clone(),
                Arc::new(Committer(false))
            )
            .finalize(&c, &a)
            .await
            .is_err()
        );
        assert!(v.binds.lock().unwrap().is_empty());
        let (c, a, mut r, v) = fixture();
        r.outputs[0].binding.nonce = IntentNonce::new();
        let r = Arc::new(r);
        assert!(
            EmbeddingResultFinalizationService::new(
                r.clone(),
                Arc::new(v),
                Arc::new(Committer(false))
            )
            .finalize(&c, &a)
            .await
            .is_err()
        );
        assert!(r.recorded.lock().unwrap().is_empty());
        assert_eq!(r.published.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn commitment_failure_never_calls_publish() {
        let (c, a, r, v) = fixture();
        let r = Arc::new(r);
        let v = Arc::new(v);
        assert!(
            EmbeddingResultFinalizationService::new(
                r.clone(),
                v.clone(),
                Arc::new(Committer(true))
            )
            .finalize(&c, &a)
            .await
            .is_err()
        );
        assert_eq!(r.published.load(Ordering::SeqCst), 0);
        assert_eq!(v.uses.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn missing_bound_callback_refuses_publication() {
        let (c, a, r, mut v) = fixture();
        v.omit_callback = true;
        let r = Arc::new(r);
        assert!(
            EmbeddingResultFinalizationService::new(
                r.clone(),
                Arc::new(v),
                Arc::new(Committer(false))
            )
            .finalize(&c, &a)
            .await
            .is_err()
        );
        assert_eq!(r.published.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn debug_redacts_ciphertext_and_commitments() {
        let (_, _, r, _) = fixture();
        assert!(!format!("{:?}", r.outputs[0]).contains("81, 82, 83"));
        let c = EmbeddingOutputCommitment {
            projection_id: uuid::Uuid::now_v7(),
            output_ordinal: 0,
            commitment: [197; 32],
        };
        assert!(!format!("{c:?}").contains("197, 197"));
    }
}
