//! PostgreSQL authority for delivery `EmbeddingJobResultPrepared` markers.
//!
//! Vault work is deliberately absent here. The application service loads the
//! plan through this repository, seals each vector outside this transaction,
//! then gives this repository ciphertext for one short guarded commit.

use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, EmbeddingOutputKeyBinding, EmbeddingResultDispatchAuthority,
    EmbeddingResultEligibility, EmbeddingResultEligibilityPlan, EmbeddingResultOutputPlan,
    EmbeddingResultPreparationId, EmbeddingResultPreparationIdentities,
    EmbeddingResultPreparationOutcome, EmbeddingResultRepository, InstallationMutationPermit,
    PermitMode, RequestContext, SealedEmbeddingResultOutput, SharedProviderDispatchRepository,
};
use vestrace_domain::{
    ContentMaterialId, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId, WorkspaceId,
};

use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore};

const RESULT_CONFLICT: &str = "EMBEDDING_RESULT_CONFLICT";

pub struct PgEmbeddingResultRepository {
    permit: PgInstallationMutationPermit,
    dispatch: SharedProviderDispatchRepository,
}

impl PgEmbeddingResultRepository {
    pub fn new(store: PgStore, dispatch: SharedProviderDispatchRepository) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store.clone()),
            dispatch,
        }
    }
}

impl vestrace_application::EmbeddingResultSealer for crate::crypto::ContentMaterialCodec {
    fn seal_embedding_vector(
        &self,
        binding: &vestrace_application::EmbeddingOutputKeyBinding,
        dek: &vestrace_domain::ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, vestrace_application::ApplicationError> {
        crate::crypto::ContentMaterialCodec::seal(
            self,
            binding.workspace_id,
            binding.material_id,
            binding.key_id,
            dek,
            plaintext,
        )
        .map_err(|error| vestrace_application::ApplicationError::Policy(error.to_string()))
    }
}

#[derive(FromRow)]
struct EligibilityRow {
    preparation_id: Option<uuid::Uuid>,
    preparation_output_count: Option<i64>,
    expected_job_version: i64,
    response_model: String,
    adapter: String,
    output_ordinal: i64,
    intent_id: uuid::Uuid,
    material_id: uuid::Uuid,
    material_key_id: uuid::Uuid,
    intent_nonce: uuid::Uuid,
    dimensions: i32,
    preparation_input_ordinal: Option<i64>,
    preparation_response_index: Option<i64>,
}

#[async_trait]
impl EmbeddingResultRepository for PgEmbeddingResultRepository {
    async fn load_eligibility(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultDispatchAuthority,
    ) -> Result<EmbeddingResultEligibility, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        self.dispatch
            .lock_embedding_result_completion_authority_in(
                context,
                permit.unit_of_work_mut(),
                &authority.dispatch,
                authority.job_id,
            )
            .await?;
        let rows = load_eligibility_rows(
            postgres_transaction(permit.unit_of_work_mut())?,
            context,
            authority,
        )
        .await?;
        permit.commit().await?;
        eligibility_from_rows(context, authority, rows)
    }

    async fn commit_prepared(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultDispatchAuthority,
        identities: EmbeddingResultPreparationIdentities,
        plan: &EmbeddingResultEligibilityPlan,
        outputs: Vec<SealedEmbeddingResultOutput>,
    ) -> Result<EmbeddingResultPreparationOutcome, ApplicationError> {
        validate_commit_inputs(context, authority, &identities, plan, &outputs)?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        self.dispatch
            .lock_embedding_result_completion_authority_in(
                context,
                permit.unit_of_work_mut(),
                &authority.dispatch,
                authority.job_id,
            )
            .await?;
        let existing = eligibility_from_rows(
            context,
            authority,
            load_eligibility_rows(
                postgres_transaction(permit.unit_of_work_mut())?,
                context,
                authority,
            )
            .await?,
        )?;
        if let EmbeddingResultEligibility::Existing {
            preparation_id,
            plan: existing_plan,
        } = existing
        {
            if existing_plan != *plan {
                return Err(ApplicationError::Conflict(RESULT_CONFLICT.into()));
            }
            permit.commit().await?;
            return Ok(EmbeddingResultPreparationOutcome::ConvergedExisting { preparation_id });
        }
        let attachment_ids = outputs
            .iter()
            .map(|output| output.attachment_id.as_uuid())
            .collect::<Vec<_>>();
        let dimensions = outputs
            .iter()
            .map(|output| {
                i32::try_from(output.dimensions)
                    .map_err(|_| ApplicationError::Conflict(RESULT_CONFLICT.into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ciphertexts = outputs
            .into_iter()
            .map(|output| output.ciphertext)
            .collect::<Vec<_>>();
        let preparation_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_commit_embedding_result_preparation($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(authority.job_id.as_uuid())
        .bind(authority.effect_id.as_uuid())
        .bind(identities.preparation_id.as_uuid())
        .bind(identities.receipt_id.as_uuid())
        .bind(
            i64::try_from(plan.expected_job_version)
                .map_err(|_| ApplicationError::Conflict(RESULT_CONFLICT.into()))?,
        )
        .bind(&plan.response_model)
        .bind(attachment_ids)
        .bind(ciphertexts)
        .bind(dimensions)
        .fetch_one(postgres_transaction(permit.unit_of_work_mut())?.connection())
        .await
        .map_err(storage)?;
        if preparation_id != identities.preparation_id.as_uuid() {
            match eligibility_from_rows(
                context,
                authority,
                load_eligibility_rows(
                    postgres_transaction(permit.unit_of_work_mut())?,
                    context,
                    authority,
                )
                .await?,
            )? {
                EmbeddingResultEligibility::Existing {
                    preparation_id: existing_id,
                    plan: existing_plan,
                } if existing_id.as_uuid() == preparation_id && existing_plan == *plan => {}
                _ => return Err(ApplicationError::Conflict(RESULT_CONFLICT.into())),
            }
        }
        permit.commit().await?;
        if preparation_id == identities.preparation_id.as_uuid() {
            Ok(EmbeddingResultPreparationOutcome::Prepared {
                preparation_id: identities.preparation_id,
            })
        } else {
            Ok(EmbeddingResultPreparationOutcome::ConvergedExisting {
                preparation_id: EmbeddingResultPreparationId::from_uuid(preparation_id),
            })
        }
    }
}

fn validate_commit_inputs(
    context: &RequestContext,
    authority: &EmbeddingResultDispatchAuthority,
    identities: &EmbeddingResultPreparationIdentities,
    plan: &EmbeddingResultEligibilityPlan,
    outputs: &[SealedEmbeddingResultOutput],
) -> Result<(), ApplicationError> {
    if authority.job_id != plan.job_id
        || authority.effect_id != plan.effect_id
        || authority.dispatch.effect_id != authority.effect_id
        || outputs.len() != plan.outputs.len()
        || outputs.len() != identities.attachments.len()
        || outputs.is_empty()
    {
        return Err(ApplicationError::Conflict(RESULT_CONFLICT.into()));
    }
    for (ordinal, ((planned, identity), sealed)) in plan
        .outputs
        .iter()
        .zip(&identities.attachments)
        .zip(outputs)
        .enumerate()
    {
        if planned.binding.workspace_id != context.workspace_id
            || planned.binding != sealed.binding
            || planned.binding.output_ordinal != ordinal as u64
            || identity.output_ordinal != ordinal as u64
            || identity.intent_id != planned.binding.intent_id
            || identity.attachment_id != sealed.attachment_id
            || sealed.dimensions != planned.expected_dimensions
            || sealed.ciphertext.is_empty()
        {
            return Err(ApplicationError::Conflict(RESULT_CONFLICT.into()));
        }
    }
    Ok(())
}

async fn load_eligibility_rows(
    transaction: &mut PgScopedTransaction,
    context: &RequestContext,
    authority: &EmbeddingResultDispatchAuthority,
) -> Result<Vec<EligibilityRow>, ApplicationError> {
    sqlx::query_as::<_, EligibilityRow>(
        "SELECT * FROM vestrace_load_embedding_result_eligibility($1,$2,$3)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(authority.job_id.as_uuid())
    .bind(authority.effect_id.as_uuid())
    .fetch_all(transaction.connection())
    .await
    .map_err(storage)
}

/// The SQL loader is SECURITY DEFINER and validates marker completeness without
/// granting runtime raw marker-table lock authority. The guarded commit owns
/// the mutable canonical lock sequence, so runtime must never issue raw
/// `FOR UPDATE` or `FOR KEY SHARE` marker reads here.
fn eligibility_from_rows(
    context: &RequestContext,
    authority: &EmbeddingResultDispatchAuthority,
    rows: Vec<EligibilityRow>,
) -> Result<EmbeddingResultEligibility, ApplicationError> {
    let first = rows.first().ok_or_else(|| {
        ApplicationError::Conflict("embedding result eligibility has no accepted outputs".into())
    })?;
    let expected_job_version = u64::try_from(first.expected_job_version)
        .map_err(|_| ApplicationError::Storage("embedding result version is invalid".into()))?;
    let dimensions = u32::try_from(first.dimensions)
        .map_err(|_| ApplicationError::Storage("embedding result dimensions are invalid".into()))?;
    let first_expected_job_version = first.expected_job_version;
    let first_response_model = first.response_model.clone();
    let first_adapter = first.adapter.clone();
    let first_dimensions = first.dimensions;
    let existing_preparation_id = first.preparation_id;
    let existing_output_count = first.preparation_output_count;
    let outputs = rows
        .iter()
        .enumerate()
        .map(|(ordinal, row)| {
            let output_ordinal = u64::try_from(row.output_ordinal).map_err(|_| {
                ApplicationError::Storage("embedding output ordinal is invalid".into())
            })?;
            if row.preparation_id != existing_preparation_id
                || row.preparation_output_count != existing_output_count
                || output_ordinal != ordinal as u64
                || row.expected_job_version != first_expected_job_version
                || row.response_model != first_response_model
                || row.adapter != first_adapter
                || row.dimensions != first_dimensions
                || (existing_preparation_id.is_some()
                    && (row.preparation_input_ordinal != Some(row.output_ordinal)
                        || row.preparation_response_index != Some(row.output_ordinal)))
                || (existing_preparation_id.is_none()
                    && (row.preparation_input_ordinal.is_some()
                        || row.preparation_response_index.is_some()))
            {
                return Err(ApplicationError::Storage(
                    "embedding result eligibility output set was inconsistent".into(),
                ));
            }
            Ok(EmbeddingResultOutputPlan {
                binding: EmbeddingOutputKeyBinding {
                    workspace_id: WorkspaceId::from_uuid(context.workspace_id.as_uuid()),
                    job_id: authority.job_id,
                    intent_id: MaterialKeyCreationIntentId::from_uuid(row.intent_id),
                    material_id: ContentMaterialId::from_uuid(row.material_id),
                    key_id: MaterialKeyId::from_uuid(row.material_key_id),
                    nonce: IntentNonce::from_uuid(row.intent_nonce),
                    output_ordinal,
                },
                expected_dimensions: dimensions,
            })
        })
        .collect::<Result<Vec<_>, ApplicationError>>()?;
    let plan = EmbeddingResultEligibilityPlan {
        job_id: authority.job_id,
        effect_id: authority.effect_id,
        expected_job_version,
        adapter: first_adapter,
        response_model: first_response_model,
        outputs,
    };
    match (existing_preparation_id, existing_output_count) {
        (Some(id), Some(output_count))
            if usize::try_from(output_count).ok() == Some(plan.outputs.len()) =>
        {
            Ok(EmbeddingResultEligibility::Existing {
                preparation_id: EmbeddingResultPreparationId::from_uuid(id),
                plan,
            })
        }
        (None, None) => Ok(EmbeddingResultEligibility::Eligible(plan)),
        _ => Err(ApplicationError::Storage(
            "embedding result marker semantic output set was inconsistent".into(),
        )),
    }
}

fn postgres_transaction(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL embedding-result transaction".into())
        })
}

fn storage(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("23505") | Some("40001") => ApplicationError::Conflict(RESULT_CONFLICT.into()),
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Policy(RESULT_CONFLICT.into())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
