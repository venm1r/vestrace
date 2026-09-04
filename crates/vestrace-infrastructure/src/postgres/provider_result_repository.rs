//! One-transaction retained provider-result preparation and publication.

use std::sync::Arc;

use async_trait::async_trait;
use ring::hmac;
use sqlx::{PgConnection, Row, postgres::PgRow};
use vestrace_application::run::{CommitProviderResultRun, RunStorePort};
use vestrace_application::{
    ApplicationError, EffectiveChatEvidence, EffectiveChatFinishReason, ExternalEffectRepository,
    FinalizeProviderResult, InstallationMutationPermit, MaterialKeyVault, NoProviderResultFaults,
    PROVIDER_RESULT_CONFLICT, PermitMode, PrepareProviderResult, PreparedProviderResult,
    ProviderDispatchAuthority, ProviderResultFaultInjector, ProviderResultFaultPoint,
    ProviderResultPublication, ProviderResultReceiptEvidence, ProviderResultRepository,
    ProviderUsage, RequestContext, SharedProviderDispatchRepository, UnitOfWork,
};
use vestrace_domain::{
    AgentRunId, ArtifactId, ArtifactRevisionId, ContentMaterialId, ExternalEffectId,
    ExternalEffectReceipt, ExternalEffectReceiptId, IntentNonce, MaterialKeyCreationIntentId,
    MaterialKeyId, ModelExecutionId, PreparedMaterialAttachmentId, RunStepId, WorkItemId,
    size_class_for,
};

use crate::crypto::ContentMaterialCodec;

use super::PgScopedTransaction;

const COMMITMENT_DOMAIN: &[u8] = b"vestrace-provider-result-erasure-bound-v1\0";

pub struct PgProviderResultRepository {
    permit: Arc<dyn InstallationMutationPermit>,
    vault: Arc<dyn MaterialKeyVault>,
    codec: Arc<ContentMaterialCodec>,
    effects: Arc<dyn ExternalEffectRepository>,
    runs: Arc<dyn RunStorePort>,
    faults: Arc<dyn ProviderResultFaultInjector>,
    dispatch: Option<SharedProviderDispatchRepository>,
}

impl PgProviderResultRepository {
    pub fn new(
        permit: Arc<dyn InstallationMutationPermit>,
        vault: Arc<dyn MaterialKeyVault>,
        codec: Arc<ContentMaterialCodec>,
        effects: Arc<dyn ExternalEffectRepository>,
        runs: Arc<dyn RunStorePort>,
    ) -> Self {
        Self::with_faults(
            permit,
            vault,
            codec,
            effects,
            runs,
            Arc::new(NoProviderResultFaults),
        )
    }

    pub fn with_faults(
        permit: Arc<dyn InstallationMutationPermit>,
        vault: Arc<dyn MaterialKeyVault>,
        codec: Arc<ContentMaterialCodec>,
        effects: Arc<dyn ExternalEffectRepository>,
        runs: Arc<dyn RunStorePort>,
        faults: Arc<dyn ProviderResultFaultInjector>,
    ) -> Self {
        Self {
            permit,
            vault,
            codec,
            effects,
            runs,
            faults,
            dispatch: None,
        }
    }

    /// Adds the single authoritative dispatch completion boundary used by Run
    /// success.  Keeping it opt-in preserves existing non-Run provider-result
    /// callers while causing a Run composition omission to fail closed.
    pub fn with_provider_dispatch(mut self, dispatch: SharedProviderDispatchRepository) -> Self {
        self.dispatch = Some(dispatch);
        self
    }

    fn fault(&self, point: ProviderResultFaultPoint) -> Result<(), ApplicationError> {
        self.faults.check(point)
    }

    async fn prepare_inner(
        &self,
        request: PrepareProviderResult,
        authority: Option<&ProviderDispatchAuthority>,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        let PrepareProviderResult {
            context,
            effect_id,
            run_id,
            step_id,
            identities,
            result,
        } = request;
        let (content, evidence, usage) = result.into_parts();
        let mut permit = self.permit.acquire(PermitMode::Shared, &context).await?;
        if let Some(authority) = authority {
            if authority.effect_id != effect_id {
                return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
            }
            let dispatch = self.dispatch.as_ref().ok_or_else(|| {
                ApplicationError::Policy(
                    "Run provider-result preparation has no dispatch completion authority".into(),
                )
            })?;
            // The dispatch authority locks the permanent Connection guard
            // before result preparation takes Run/step parent locks.  The
            // release below deliberately revalidates it after the receipt is
            // witnessed, immediately before changing any lease state.
            dispatch
                .lock_provider_result_completion_authority_in(
                    &context,
                    permit.unit_of_work_mut(),
                    authority,
                )
                .await?;
        }
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let step_status = lock_result_parents(
            transaction.connection(),
            &context,
            effect_id,
            run_id,
            step_id,
        )
        .await?;
        if step_status != "running" {
            return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
        }

        let existing: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM provider_result_preparations WHERE workspace_id=$1 AND external_effect_id=$2)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(effect_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_provider_result_error)?;
        if existing {
            let prepared =
                load_prepared_result(transaction.connection(), &context, effect_id).await?;
            if prepared.run_id != run_id
                || prepared.step_id != step_id
                || prepared.identities != identities
                || prepared.evidence != evidence
                || prepared.usage != usage
            {
                return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
            }
            if authority.is_some() {
                transition_run_step_execution_attempt_in(
                    permit.unit_of_work_mut(),
                    &context,
                    run_id,
                    step_id,
                    "result_prepared",
                )
                .await?;
            }
            permit.commit().await?;
            return Ok(prepared);
        }

        self.fault(ProviderResultFaultPoint::BeforeReserve)?;
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'provider_result',$6,0)")
            .bind(identities.material_intent_id.as_uuid())
            .bind(context.workspace_id.as_uuid())
            .bind(identities.content_material_id.as_uuid())
            .bind(identities.material_key_id.as_uuid())
            .bind(identities.intent_nonce.as_uuid())
            .bind(effect_id.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(map_provider_result_error)?;
        self.fault(ProviderResultFaultPoint::AfterReserve)?;

        let vault_receipt = self
            .vault
            .create_if_absent(identities.material_key_id, identities.intent_nonce)
            .map_err(vault_error)?;
        self.fault(ProviderResultFaultPoint::AfterVaultCreate)?;
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(identities.material_intent_id.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(map_provider_result_error)?;
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
            .bind(identities.material_intent_id.as_uuid())
            .bind(vault_receipt.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(map_provider_result_error)?;
        self.fault(ProviderResultFaultPoint::AfterVaultReceipt)?;

        let mut sealed = None;
        let mut seal_error = None;
        self.vault
            .unwrap(
                identities.material_key_id,
                &mut |dek| match self.codec.seal(
                    context.workspace_id,
                    identities.content_material_id,
                    identities.material_key_id,
                    dek,
                    content.as_bytes(),
                ) {
                    Ok(frame) => sealed = Some(frame),
                    Err(error) => seal_error = Some(error),
                },
            )
            .map_err(vault_error)?;
        if let Some(error) = seal_error {
            return Err(ApplicationError::Policy(error.to_string()));
        }
        let ciphertext = sealed.ok_or_else(|| {
            ApplicationError::Internal("material vault did not return a sealed result".into())
        })?;
        let size_class = size_class_for(ciphertext.len());
        if size_class.minimum_bytes() != ciphertext.len() {
            return Err(ApplicationError::Policy(
                "provider result frame is not an exact size class".into(),
            ));
        }
        self.fault(ProviderResultFaultPoint::AfterEncryption)?;
        let preparation_id: uuid::Uuid =
            sqlx::query_scalar("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(effect_id.as_uuid())
                .bind(identities.material_intent_id.as_uuid())
                .bind(identities.prepared_attachment_id.as_uuid())
                .bind(identities.artifact_id.as_uuid())
                .bind(identities.artifact_revision_id.as_uuid())
                .bind(identities.model_execution_id.as_uuid())
                .bind(&ciphertext)
                .bind(
                    i64::try_from(size_class.minimum_bytes()).expect("material frames fit BIGINT"),
                )
                .fetch_one(transaction.connection())
                .await
                .map_err(map_provider_result_error)?;
        self.fault(ProviderResultFaultPoint::AfterResultPrepared)?;

        let recorded_at = database_now(transaction.connection()).await?;
        let receipt_id = identities.receipt_id;
        let receipt = ExternalEffectReceipt::provider_result_acknowledged(
            receipt_id,
            effect_id,
            "provider",
            preparation_id,
            recorded_at,
        );
        let receipt_evidence =
            ProviderResultReceiptEvidence::new(identities.advance_work_item_id, evidence, &usage)?;
        self.effects
            .insert_provider_result_receipt_in(
                &context,
                permit.unit_of_work_mut(),
                &receipt,
                receipt_evidence,
            )
            .await?;
        self.fault(ProviderResultFaultPoint::AfterReceipt)?;
        let witnessed: uuid::Uuid =
            sqlx::query_scalar("SELECT vestrace_witness_provider_result_receipt($1,$2)")
                .bind(preparation_id)
                .bind(receipt_id.as_uuid())
                .fetch_one(postgres_transaction(permit.unit_of_work_mut())?.connection())
                .await
                .map_err(map_provider_result_error)?;
        if witnessed != receipt_id.as_uuid() {
            return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
        }
        if let Some(authority) = authority {
            let dispatch = self.dispatch.as_ref().ok_or_else(|| {
                ApplicationError::Policy(
                    "Run provider-result preparation has no dispatch completion authority".into(),
                )
            })?;
            dispatch
                .release_after_provider_result_in(
                    &context,
                    permit.unit_of_work_mut(),
                    authority,
                    &receipt,
                )
                .await?;
            transition_run_step_execution_attempt_in(
                permit.unit_of_work_mut(),
                &context,
                run_id,
                step_id,
                "result_prepared",
            )
            .await?;
        }
        self.fault(ProviderResultFaultPoint::AfterWitness)?;
        permit.commit().await?;
        Ok(PreparedProviderResult {
            preparation_id,
            effect_id,
            run_id,
            step_id,
            identities,
            size_class,
            evidence,
            usage,
        })
    }
}

#[async_trait]
impl ProviderResultRepository for PgProviderResultRepository {
    async fn prepare(
        &self,
        request: PrepareProviderResult,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        self.prepare_inner(request, None).await
    }

    async fn prepare_after_dispatch(
        &self,
        request: PrepareProviderResult,
        authority: &ProviderDispatchAuthority,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        self.prepare_inner(request, Some(authority)).await
    }

    async fn recover_result_prepared(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let identity: Option<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
            "SELECT run_id,step_id FROM provider_result_preparations WHERE workspace_id=$1 AND external_effect_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(effect_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(map_provider_result_error)?;
        let (run_id, step_id) =
            identity.ok_or_else(|| ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()))?;
        lock_result_parents(
            transaction.connection(),
            context,
            effect_id,
            AgentRunId::from_uuid(run_id),
            RunStepId::from_uuid(step_id),
        )
        .await?;
        let prepared = load_prepared_result(transaction.connection(), context, effect_id).await?;
        permit.commit().await?;
        Ok(prepared)
    }

    async fn finalize(
        &self,
        context: &RequestContext,
        request: FinalizeProviderResult,
    ) -> Result<ProviderResultPublication, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let publication = self
            .finalize_in(context, permit.unit_of_work_mut(), request)
            .await?;
        permit.commit().await?;
        Ok(publication)
    }

    async fn finalize_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        request: FinalizeProviderResult,
    ) -> Result<ProviderResultPublication, ApplicationError> {
        let prepared = request.prepared;
        if let Some(dispatch) = self.dispatch.as_ref() {
            // The dispatch recovery seam takes the permanent Connection guard
            // and the immutable attempt before this finalizer locks any Run,
            // step, or preparation parent.  It runs in this same UoW so a
            // publication cannot compose around a different adapter effect.
            dispatch
                .lock_provider_result_publication_authority_in(
                    context,
                    unit_of_work,
                    prepared.effect_id,
                    prepared.run_id,
                    prepared.step_id,
                )
                .await?;
        }
        let transaction = postgres_transaction(unit_of_work)?;
        let row =
            lock_and_validate_preparation(transaction.connection(), context, &prepared).await?;
        let was_published = row.state == "published";
        if row.bound_receipt.is_some()
            && row.bound_receipt != Some(request.binding_receipt.as_uuid())
        {
            return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
        }
        if !was_published {
            if row.bound_receipt.is_none() {
                sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
                    .bind(prepared.identities.material_intent_id.as_uuid())
                    .bind(request.binding_receipt.as_uuid())
                    .execute(transaction.connection())
                    .await
                    .map_err(map_provider_result_error)?;
            }
            self.fault(ProviderResultFaultPoint::AfterBind)?;
            insert_runtime_envelopes(transaction.connection(), context, &prepared).await?;
            self.fault(ProviderResultFaultPoint::AfterRuntimeEnvelopes)?;
        }

        let ciphertext: Vec<u8> = sqlx::query_scalar(
            "SELECT ciphertext FROM content_material_bytes WHERE workspace_id=$1 AND intent_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(prepared.identities.material_intent_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_provider_result_error)?;
        let mut commitment = None;
        self.vault
            .unwrap(prepared.identities.material_key_id, &mut |dek| {
                commitment = Some(erasure_bound_commitment(
                    dek,
                    context,
                    &prepared,
                    &ciphertext,
                ));
            })
            .map_err(vault_error)?;
        let commitment = commitment.ok_or_else(|| {
            ApplicationError::Internal("material vault did not compute a commitment".into())
        })?;
        sqlx::query("SELECT vestrace_finalize_provider_result($1,$2)")
            .bind(prepared.preparation_id)
            .bind(commitment.as_slice())
            .execute(transaction.connection())
            .await
            .map_err(map_provider_result_error)?;
        self.fault(ProviderResultFaultPoint::AfterLivePromotion)?;
        if !was_published {
            self.fault(ProviderResultFaultPoint::BeforeRunSuccess)?;
            self.runs
                .commit_provider_result_in(
                    context,
                    unit_of_work,
                    CommitProviderResultRun {
                        run_id: prepared.run_id,
                        step_id: prepared.step_id,
                        expected_run_version: u64::try_from(row.expected_run_version).map_err(
                            |_| ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()),
                        )?,
                        artifact_id: prepared.identities.artifact_id,
                        artifact_revision_id: prepared.identities.artifact_revision_id,
                        model_execution_id: prepared.identities.model_execution_id,
                        work_item_id: prepared.identities.advance_work_item_id,
                        occurred_at: row.now,
                    },
                )
                .await?;
        }
        if self.dispatch.is_some() {
            transition_run_step_execution_attempt_in(
                unit_of_work,
                context,
                prepared.run_id,
                prepared.step_id,
                "published",
            )
            .await?;
        }
        load_publication(
            postgres_transaction(unit_of_work)?.connection(),
            context,
            &prepared,
        )
        .await
    }
}

struct LockedPreparation {
    state: String,
    expected_run_version: i64,
    now: chrono::DateTime<chrono::Utc>,
    bound_receipt: Option<uuid::Uuid>,
}

async fn lock_result_parents(
    connection: &mut PgConnection,
    context: &RequestContext,
    effect_id: vestrace_domain::ExternalEffectId,
    run_id: vestrace_domain::AgentRunId,
    step_id: vestrace_domain::RunStepId,
) -> Result<String, ApplicationError> {
    let run: Option<(i64, String)> = sqlx::query_as(
        "SELECT run_version,status FROM agent_runs WHERE workspace_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    if !matches!(run, Some((_, ref status)) if status == "running") {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    let step: Option<String> = sqlx::query_scalar(
        "SELECT status FROM run_steps WHERE workspace_id=$1 AND run_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(step_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    let Some(step_status) = step else {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    };
    if step_status != "running" && step_status != "succeeded" {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    let cause_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM provider_dispatch_causes WHERE workspace_id=$1 AND external_effect_id=$2 AND cause_kind='run_step' AND run_id=$3 AND step_id=$4)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(effect_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(step_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    if !cause_exists {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    sqlx::query(
        "SELECT id FROM external_effect_intents WHERE workspace_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(effect_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    Ok(step_status)
}

async fn lock_and_validate_preparation(
    connection: &mut PgConnection,
    context: &RequestContext,
    prepared: &PreparedProviderResult,
) -> Result<LockedPreparation, ApplicationError> {
    let step_status = lock_result_parents(
        connection,
        context,
        prepared.effect_id,
        prepared.run_id,
        prepared.step_id,
    )
    .await?;
    let row = sqlx::query(
        "SELECT state,expected_run_version,material_intent_id,prepared_attachment_id,artifact_id,artifact_revision_id,model_execution_id,size_class,external_effect_receipt_id,advance_work_item_id,finish_reason,usage_known,prompt_tokens,completion_tokens,NOW() AS now FROM provider_result_preparations WHERE id=$1 AND workspace_id=$2 AND external_effect_id=$3",
    )
    .bind(prepared.preparation_id)
    .bind(context.workspace_id.as_uuid())
    .bind(prepared.effect_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_provider_result_error)?
    .ok_or_else(|| ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()))?;
    if row.get::<uuid::Uuid, _>("material_intent_id")
        != prepared.identities.material_intent_id.as_uuid()
        || row.get::<uuid::Uuid, _>("prepared_attachment_id")
            != prepared.identities.prepared_attachment_id.as_uuid()
        || row.get::<uuid::Uuid, _>("artifact_id") != prepared.identities.artifact_id.as_uuid()
        || row.get::<uuid::Uuid, _>("artifact_revision_id")
            != prepared.identities.artifact_revision_id.as_uuid()
        || row.get::<uuid::Uuid, _>("model_execution_id")
            != prepared.identities.model_execution_id.as_uuid()
        || row.get::<i64, _>("size_class")
            != i64::try_from(prepared.size_class.minimum_bytes()).expect("size class fits BIGINT")
        || row.get::<Option<uuid::Uuid>, _>("external_effect_receipt_id")
            != Some(prepared.identities.receipt_id.as_uuid())
        || row.get::<Option<uuid::Uuid>, _>("advance_work_item_id")
            != Some(prepared.identities.advance_work_item_id.as_uuid())
        || row.get::<Option<String>, _>("finish_reason").as_deref()
            != Some(evidence_name(prepared.evidence))
        || persisted_usage(&row)? != prepared.usage
    {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    let state: String = row.get("state");
    if (state == "published" && step_status != "succeeded")
        || (state != "published" && step_status != "running")
    {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    // Parent Run/step/effect locks serialize provider-result recovery. The
    // guarded finalizer below re-locks the preparation and intent in the exact
    // canonical order; runtime deliberately has no mutation privilege on
    // either guarded table, so these authority reads remain ordinary SELECTs.
    let bound_receipt: Option<uuid::Uuid> = sqlx::query_scalar("SELECT bound_receipt FROM material_key_creation_intents WHERE workspace_id=$1 AND id=$2 AND material_id=$3 AND material_key_id=$4 AND nonce=$5 AND owner_kind='provider_result' AND owner_id=$6 AND output_ordinal=0")
        .bind(context.workspace_id.as_uuid())
        .bind(prepared.identities.material_intent_id.as_uuid())
        .bind(prepared.identities.content_material_id.as_uuid())
        .bind(prepared.identities.material_key_id.as_uuid())
        .bind(prepared.identities.intent_nonce.as_uuid())
        .bind(prepared.effect_id.as_uuid())
        .fetch_one(&mut *connection)
        .await
        .map_err(map_provider_result_error)?;
    Ok(LockedPreparation {
        state,
        expected_run_version: row.get("expected_run_version"),
        now: row.get("now"),
        bound_receipt,
    })
}

async fn load_prepared_result(
    connection: &mut PgConnection,
    context: &RequestContext,
    effect_id: ExternalEffectId,
) -> Result<PreparedProviderResult, ApplicationError> {
    let row = sqlx::query(
        "SELECT preparation.id,preparation.run_id,preparation.step_id,\
                preparation.material_intent_id,preparation.prepared_attachment_id,\
                preparation.artifact_id,preparation.artifact_revision_id,\
                preparation.model_execution_id,preparation.size_class,\
                preparation.external_effect_receipt_id,preparation.advance_work_item_id,\
                preparation.finish_reason,preparation.usage_known,\
                preparation.prompt_tokens,preparation.completion_tokens,\
                intent.material_id,intent.material_key_id,intent.nonce,bytes.ciphertext \
           FROM provider_result_preparations AS preparation \
           JOIN material_key_creation_intents AS intent \
             ON intent.workspace_id=preparation.workspace_id \
            AND intent.id=preparation.material_intent_id \
           JOIN content_material_bytes AS bytes \
             ON bytes.intent_id=intent.id AND bytes.material_id=intent.material_id \
           JOIN external_effect_receipts AS receipt \
             ON receipt.workspace_id=preparation.workspace_id \
            AND receipt.id=preparation.external_effect_receipt_id \
            AND receipt.effect_id=preparation.external_effect_id \
          WHERE preparation.workspace_id=$1 \
            AND preparation.external_effect_id=$2 \
            AND preparation.state IN ('result_prepared','published') \
            AND preparation.receipt_witnessed_at IS NOT NULL \
            AND intent.owner_kind='provider_result' \
            AND intent.owner_id=preparation.external_effect_id \
            AND intent.output_ordinal=0 \
            AND receipt.outcome_status='acknowledged' \
            AND jsonb_typeof(receipt.payload->'evidence_refs')='array' \
            AND jsonb_array_length(receipt.payload->'evidence_refs')=1 \
            AND receipt.payload->'evidence_refs'->>0 \
                = 'provider_result_preparation:' || lower(preparation.id::TEXT) \
            AND EXISTS (\
                SELECT 1 FROM external_effect_lifecycle_transitions AS transition \
                 WHERE transition.workspace_id=receipt.workspace_id \
                   AND transition.effect_id=receipt.effect_id \
                   AND transition.status='acknowledged' \
                   AND transition.cause='receipt_recorded' \
                   AND transition.cause_ref=receipt.id::TEXT\
            )",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(effect_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_provider_result_error)?
    .ok_or_else(|| ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()))?;
    let ciphertext: Vec<u8> = row.get("ciphertext");
    let size_class_value = row.get::<i64, _>("size_class");
    let size_class = usize::try_from(size_class_value)
        .ok()
        .map(size_class_for)
        .filter(|class| class.minimum_bytes() == ciphertext.len())
        .filter(|class| {
            i64::try_from(class.minimum_bytes()).ok() == Some(size_class_value)
                && ciphertext.len() >= 4096
                && ciphertext.len() <= 1_048_576
                && ciphertext.len().is_power_of_two()
                && ciphertext.starts_with(b"VMRF\x01")
        })
        .ok_or_else(|| ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()))?;
    let finish_reason: String = row.get("finish_reason");
    Ok(PreparedProviderResult {
        preparation_id: row.get("id"),
        effect_id,
        run_id: AgentRunId::from_uuid(row.get("run_id")),
        step_id: RunStepId::from_uuid(row.get("step_id")),
        identities: vestrace_application::ProviderResultIdentities {
            material_intent_id: MaterialKeyCreationIntentId::from_uuid(
                row.get("material_intent_id"),
            ),
            content_material_id: ContentMaterialId::from_uuid(row.get("material_id")),
            material_key_id: MaterialKeyId::from_uuid(row.get("material_key_id")),
            intent_nonce: IntentNonce::from_uuid(row.get("nonce")),
            prepared_attachment_id: PreparedMaterialAttachmentId::from_uuid(
                row.get("prepared_attachment_id"),
            ),
            receipt_id: ExternalEffectReceiptId::from_uuid(row.get("external_effect_receipt_id")),
            artifact_id: ArtifactId::from_uuid(row.get("artifact_id")),
            artifact_revision_id: ArtifactRevisionId::from_uuid(row.get("artifact_revision_id")),
            model_execution_id: ModelExecutionId::from_uuid(row.get("model_execution_id")),
            advance_work_item_id: WorkItemId::from_uuid(row.get("advance_work_item_id")),
        },
        size_class,
        evidence: evidence_from_name(&finish_reason)?,
        usage: persisted_usage(&row)?,
    })
}

fn evidence_name(evidence: EffectiveChatEvidence) -> &'static str {
    match evidence {
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop) => "stop",
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Length) => "length",
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::ToolCalls) => "tool_calls",
        EffectiveChatEvidence::Completed(EffectiveChatFinishReason::ContentFilter) => {
            "content_filter"
        }
    }
}

fn evidence_from_name(name: &str) -> Result<EffectiveChatEvidence, ApplicationError> {
    let reason = match name {
        "stop" => EffectiveChatFinishReason::Stop,
        "length" => EffectiveChatFinishReason::Length,
        "tool_calls" => EffectiveChatFinishReason::ToolCalls,
        "content_filter" => EffectiveChatFinishReason::ContentFilter,
        _ => return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into())),
    };
    Ok(EffectiveChatEvidence::Completed(reason))
}

fn usage_parts(
    usage: &ProviderUsage,
) -> Result<(bool, Option<i32>, Option<i32>), ApplicationError> {
    match usage {
        ProviderUsage::Known {
            prompt_tokens,
            completion_tokens,
        } => Ok((
            true,
            Some(i32::try_from(*prompt_tokens).map_err(|_| {
                ApplicationError::Policy("provider usage exceeds storage bounds".into())
            })?),
            Some(i32::try_from(*completion_tokens).map_err(|_| {
                ApplicationError::Policy("provider usage exceeds storage bounds".into())
            })?),
        )),
        ProviderUsage::Unknown => Ok((false, None, None)),
    }
}

fn persisted_usage(row: &PgRow) -> Result<ProviderUsage, ApplicationError> {
    match row.get::<Option<bool>, _>("usage_known") {
        Some(true) => {
            let prompt = row
                .get::<Option<i32>, _>("prompt_tokens")
                .and_then(|value| u32::try_from(value).ok());
            let completion = row
                .get::<Option<i32>, _>("completion_tokens")
                .and_then(|value| u32::try_from(value).ok());
            match (prompt, completion) {
                (Some(prompt_tokens), Some(completion_tokens)) => Ok(ProviderUsage::Known {
                    prompt_tokens,
                    completion_tokens,
                }),
                _ => Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into())),
            }
        }
        Some(false)
            if row.get::<Option<i32>, _>("prompt_tokens").is_none()
                && row.get::<Option<i32>, _>("completion_tokens").is_none() =>
        {
            Ok(ProviderUsage::Unknown)
        }
        _ => Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into())),
    }
}

async fn insert_runtime_envelopes(
    connection: &mut PgConnection,
    context: &RequestContext,
    prepared: &PreparedProviderResult,
) -> Result<(), ApplicationError> {
    let expected_artifact_name = format!("provider-result-{}", prepared.identities.artifact_id);
    sqlx::query(
        "INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,$3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(prepared.identities.artifact_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(&expected_artifact_name)
    .execute(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    let artifact: Option<(uuid::Uuid, String)> =
        sqlx::query_as("SELECT workspace_id,name FROM artifacts WHERE id=$1")
            .bind(prepared.identities.artifact_id.as_uuid())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_provider_result_error)?;
    if artifact != Some((context.workspace_id.as_uuid(), expected_artifact_name)) {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    sqlx::query("INSERT INTO artifact_revisions(id,artifact_id,workspace_id,revision_number,media_type,content_hash,byte_size,storage_kind) VALUES($1,$2,$3,1,'text/plain',NULL,NULL,'governed_material') ON CONFLICT (id) DO NOTHING")
        .bind(prepared.identities.artifact_revision_id.as_uuid())
        .bind(prepared.identities.artifact_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&mut *connection)
        .await
        .map_err(map_provider_result_error)?;
    let model_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT revision.model_id FROM provider_dispatch_causes AS cause JOIN model_binding_snapshots AS snapshot ON snapshot.workspace_id=cause.workspace_id AND snapshot.id=cause.model_binding_snapshot_id JOIN model_revisions AS revision ON revision.workspace_id=snapshot.workspace_id AND revision.id=snapshot.model_revision_id WHERE cause.workspace_id=$1 AND cause.external_effect_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(prepared.effect_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    type ArtifactRevisionEnvelope = (
        uuid::Uuid,
        uuid::Uuid,
        i32,
        String,
        Option<String>,
        Option<i64>,
        String,
    );
    let revision: Option<ArtifactRevisionEnvelope> = sqlx::query_as("SELECT workspace_id,artifact_id,revision_number,media_type,content_hash,byte_size,storage_kind FROM artifact_revisions WHERE id=$1")
            .bind(prepared.identities.artifact_revision_id.as_uuid())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_provider_result_error)?;
    if revision
        != Some((
            context.workspace_id.as_uuid(),
            prepared.identities.artifact_id.as_uuid(),
            1,
            "text/plain".into(),
            None,
            None,
            "governed_material".into(),
        ))
    {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    let (usage_known, prompt_tokens, completion_tokens) = usage_parts(&prepared.usage)?;
    let stored_prompt_tokens = prompt_tokens.unwrap_or(0);
    let stored_completion_tokens = completion_tokens.unwrap_or(0);
    sqlx::query("INSERT INTO model_executions(id,workspace_id,model_id,prompt_tokens,completion_tokens,latency_ms,status,usage_known) VALUES($1,$2,$3,$4,$5,0,'succeeded',$6) ON CONFLICT (id) DO NOTHING")
        .bind(prepared.identities.model_execution_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(model_id)
        .bind(stored_prompt_tokens)
        .bind(stored_completion_tokens)
        .bind(usage_known)
        .execute(&mut *connection)
        .await
        .map_err(map_provider_result_error)?;
    let execution: Option<(uuid::Uuid, uuid::Uuid, i32, i32, i32, String, bool)> =
        sqlx::query_as("SELECT workspace_id,model_id,prompt_tokens,completion_tokens,latency_ms,status,usage_known FROM model_executions WHERE id=$1")
            .bind(prepared.identities.model_execution_id.as_uuid())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_provider_result_error)?;
    if execution
        != Some((
            context.workspace_id.as_uuid(),
            model_id,
            stored_prompt_tokens,
            stored_completion_tokens,
            0,
            "succeeded".into(),
            usage_known,
        ))
    {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    Ok(())
}

fn erasure_bound_commitment(
    dek: &vestrace_domain::ZeroizingDek,
    context: &RequestContext,
    prepared: &PreparedProviderResult,
    ciphertext: &[u8],
) -> [u8; 32] {
    let mut input = Vec::with_capacity(COMMITMENT_DOMAIN.len() + 16 * 3 + 8 + ciphertext.len());
    input.extend_from_slice(COMMITMENT_DOMAIN);
    input.extend_from_slice(context.workspace_id.as_uuid().as_bytes());
    input.extend_from_slice(prepared.identities.content_material_id.as_uuid().as_bytes());
    input.extend_from_slice(prepared.identities.material_key_id.as_uuid().as_bytes());
    input.extend_from_slice(&(ciphertext.len() as u64).to_be_bytes());
    input.extend_from_slice(ciphertext);
    dek.expose(|bytes| {
        let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, bytes), &input);
        let mut commitment = [0_u8; 32];
        commitment.copy_from_slice(tag.as_ref());
        commitment
    })
}

async fn load_publication(
    connection: &mut PgConnection,
    context: &RequestContext,
    prepared: &PreparedProviderResult,
) -> Result<ProviderResultPublication, ApplicationError> {
    let row = sqlx::query(
        "SELECT publication.id,
                publication.workspace_id AS publication_workspace_id,
                publication.external_effect_id AS publication_effect_id,
                publication.run_id AS publication_run_id,
                publication.step_id AS publication_step_id,
                publication.artifact_id AS publication_artifact_id,
                publication.artifact_revision_id AS publication_artifact_revision_id,
                publication.model_execution_id AS publication_model_execution_id,
                publication.material_intent_id AS publication_material_intent_id,
                publication.prepared_attachment_id AS publication_attachment_id,
                publication.size_class AS publication_size_class,
                content.workspace_id AS content_workspace_id,
                content.artifact_id AS content_artifact_id,
                content.artifact_revision_id AS content_artifact_revision_id,
                content.content_material_id,
                content.external_effect_id AS content_effect_id,
                content.run_id AS content_run_id,
                content.step_id AS content_step_id,
                content.material_intent_id AS content_material_intent_id,
                content.prepared_attachment_id AS content_attachment_id,
                content.model_execution_id AS content_model_execution_id,
                content.size_class AS content_size_class,
                content.media_class
           FROM provider_result_publications AS publication
           JOIN artifact_revision_contents AS content
             ON content.workspace_id=publication.workspace_id
            AND content.provider_result_preparation_id=publication.provider_result_preparation_id
          WHERE publication.workspace_id=$1
            AND publication.provider_result_preparation_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(prepared.preparation_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(map_provider_result_error)?;
    let expected_size =
        i64::try_from(prepared.size_class.minimum_bytes()).expect("material frames fit BIGINT");
    if row.get::<uuid::Uuid, _>("publication_workspace_id") != context.workspace_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_effect_id") != prepared.effect_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_run_id") != prepared.run_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_step_id") != prepared.step_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_artifact_id")
            != prepared.identities.artifact_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_artifact_revision_id")
            != prepared.identities.artifact_revision_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_model_execution_id")
            != prepared.identities.model_execution_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_material_intent_id")
            != prepared.identities.material_intent_id.as_uuid()
        || row.get::<uuid::Uuid, _>("publication_attachment_id")
            != prepared.identities.prepared_attachment_id.as_uuid()
        || row.get::<i64, _>("publication_size_class") != expected_size
        || row.get::<uuid::Uuid, _>("content_workspace_id") != context.workspace_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_artifact_id")
            != prepared.identities.artifact_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_artifact_revision_id")
            != prepared.identities.artifact_revision_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_material_id")
            != prepared.identities.content_material_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_effect_id") != prepared.effect_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_run_id") != prepared.run_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_step_id") != prepared.step_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_material_intent_id")
            != prepared.identities.material_intent_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_attachment_id")
            != prepared.identities.prepared_attachment_id.as_uuid()
        || row.get::<uuid::Uuid, _>("content_model_execution_id")
            != prepared.identities.model_execution_id.as_uuid()
        || row.get::<i64, _>("content_size_class") != expected_size
        || row.get::<String, _>("media_class") != "text"
    {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    Ok(ProviderResultPublication {
        publication_id: row.get("id"),
        preparation_id: prepared.preparation_id,
        artifact_id: prepared.identities.artifact_id,
        artifact_revision_id: prepared.identities.artifact_revision_id,
        content_material_id: prepared.identities.content_material_id,
        model_execution_id: prepared.identities.model_execution_id,
        size_class: prepared.size_class,
    })
}

async fn database_now(
    connection: &mut PgConnection,
) -> Result<chrono::DateTime<chrono::Utc>, ApplicationError> {
    sqlx::query_scalar("SELECT NOW()")
        .fetch_one(connection)
        .await
        .map_err(map_provider_result_error)
}

async fn transition_run_step_execution_attempt_in(
    unit_of_work: &mut dyn UnitOfWork,
    context: &RequestContext,
    run_id: AgentRunId,
    step_id: RunStepId,
    target_phase: &'static str,
) -> Result<(), ApplicationError> {
    let transitioned: uuid::Uuid =
        sqlx::query_scalar("SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,$4)")
            .bind(context.workspace_id.as_uuid())
            .bind(run_id.as_uuid())
            .bind(step_id.as_uuid())
            .bind(target_phase)
            .fetch_one(postgres_transaction(unit_of_work)?.connection())
            .await
            .map_err(map_provider_result_error)?;
    if transitioned.is_nil() {
        return Err(ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into()));
    }
    Ok(())
}

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL provider-result transaction".into())
        })
}

fn map_provider_result_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("40001") | Some("42501") => {
            ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.into())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}

fn vault_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
