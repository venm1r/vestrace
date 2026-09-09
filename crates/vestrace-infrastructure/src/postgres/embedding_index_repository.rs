//! Short guarded transactions for disposable index work; never receives local index bytes.
use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;
use vestrace_application::embedding::index::*;
use vestrace_application::{
    ApplicationError, InstallationMutationPermit, PermitMode, RequestContext,
};
use vestrace_domain::embedding::{
    CanonicalEmbeddingSpace, CanonicalGenerationSnapshot, EmbeddingSpaceKey,
};
use vestrace_domain::{
    CorpusChangeEventId, CorpusGenerationId, IndexBuildAttemptId, ModelQualificationRevisionId,
    ModelRevisionId,
};

pub struct PgEmbeddingIndexRepository {
    permit: PgInstallationMutationPermit,
}
impl PgEmbeddingIndexRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }
}
fn malformed() -> ApplicationError {
    ApplicationError::Unavailable("embedding-index-storage-unavailable".into())
}
fn storage(e: sqlx::Error) -> ApplicationError {
    if let Some(db) = e.as_database_error() {
        for message in [
            "embedding-generation-changed",
            "embedding-index-memory-limit",
            "embedding-index-material-unavailable",
        ] {
            if db.message() == message {
                return ApplicationError::Unavailable(message.into());
            }
        }
    }
    malformed()
}
fn tx(
    uow: &mut dyn vestrace_application::UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    uow.as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(malformed)
}
fn string(v: &Value, k: &str) -> Result<String, ApplicationError> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(malformed)
}
fn id(v: &Value, k: &str) -> Result<Uuid, ApplicationError> {
    let id = Uuid::parse_str(&string(v, k)?).map_err(|_| malformed())?;
    if id.is_nil() {
        Err(malformed())
    } else {
        Ok(id)
    }
}
fn number(v: &Value, k: &str) -> Result<u64, ApplicationError> {
    let n = v.get(k).and_then(Value::as_u64).ok_or_else(malformed)?;
    integer(n)?;
    Ok(n)
}
fn integer(n: u64) -> Result<i64, ApplicationError> {
    i64::try_from(n).map_err(|_| malformed())
}
fn snapshot(
    v: &Value,
    c: &RequestContext,
) -> Result<CanonicalGenerationSnapshot, ApplicationError> {
    if id(v, "workspace_id")? != c.workspace_id.as_uuid() {
        return Err(malformed());
    }
    let space = EmbeddingSpaceKey::canonical(
        c.workspace_id,
        string(v, "name")?,
        CanonicalEmbeddingSpace {
            model_revision_id: ModelRevisionId::from_uuid(id(v, "model_revision_id")?),
            model_qualification_revision_id: ModelQualificationRevisionId::from_uuid(id(
                v,
                "model_qualification_revision_id",
            )?),
            request_shape_revision_id: id(v, "request_shape_revision_id")?,
            adapter_profile_revision: string(v, "adapter_profile_revision")?,
            returned_model: string(v, "returned_model")?,
            encoding_format: string(v, "encoding_format")?,
            dimensions: u32::try_from(number(v, "dimensions")?).map_err(|_| malformed())?,
        },
    )
    .map_err(|_| malformed())?;
    CanonicalGenerationSnapshot::new(
        space,
        CorpusGenerationId::from_uuid(id(v, "generation_id")?),
        number(v, "generation_epoch")?,
        number(v, "guard_version")?,
        number(v, "corpus_revision")?,
        number(v, "built_through_projection_ordinal")?,
        number(v, "member_count")?,
    )
    .map_err(|_| malformed())
}
fn plan(
    v: &Value,
    c: &RequestContext,
    owner: &str,
) -> Result<EmbeddingIndexBuildPlan, ApplicationError> {
    let purpose = match string(v, "purpose")?.as_str() {
        "corpus_change" => IndexBuildPurpose::CorpusChange,
        "startup" => IndexBuildPurpose::Startup,
        "lazy_load" => IndexBuildPurpose::LazyLoad,
        _ => return Err(malformed()),
    };
    let event_id = if v.get("event_id").is_some_and(Value::is_null) {
        None
    } else {
        Some(CorpusChangeEventId::from_uuid(id(v, "event_id")?))
    };
    if string(v, "owner")? != owner
        || (purpose == IndexBuildPurpose::CorpusChange) != event_id.is_some()
    {
        return Err(malformed());
    }
    Ok(EmbeddingIndexBuildPlan {
        attempt_id: IndexBuildAttemptId::from_uuid(id(v, "attempt_id")?),
        space_registration_id: id(v, "space_registration_id")?,
        snapshot: snapshot(&v["snapshot"], c)?,
        purpose,
        event_id,
        owner: owner.into(),
    })
}

#[async_trait]
impl EmbeddingIndexRepository for PgEmbeddingIndexRepository {
    async fn claim_next_build(
        &self,
        c: &RequestContext,
        owner: &str,
        limit: u32,
    ) -> Result<Option<EmbeddingIndexBuildPlan>, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        let v: Option<Value> =
            sqlx::query_scalar("SELECT vestrace_claim_embedding_index_build($1,$2,$3)")
                .bind(c.workspace_id.as_uuid())
                .bind(owner)
                .bind(i32::try_from(limit).map_err(|_| malformed())?)
                .fetch_one(tx(permit.unit_of_work_mut())?.connection())
                .await
                .map_err(storage)?;
        let result = v.as_ref().map(|v| plan(v, c, owner)).transpose()?;
        permit.commit().await?;
        Ok(result)
    }
    async fn claim_local_load(
        &self,
        c: &RequestContext,
        s: &CanonicalGenerationSnapshot,
        owner: &str,
    ) -> Result<EmbeddingIndexBuildPlan, ApplicationError> {
        s.validate().map_err(|_| malformed())?;
        if s.workspace_id != c.workspace_id {
            return Err(malformed());
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        let v: Value = sqlx::query_scalar(
            "SELECT vestrace_claim_embedding_index_load($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(c.workspace_id.as_uuid())
        .bind(s.generation_id.as_uuid())
        .bind(integer(s.generation_epoch)?)
        .bind(integer(s.guard_version)?)
        .bind(integer(s.corpus_revision)?)
        .bind(integer(s.built_through_projection_ordinal)?)
        .bind(integer(s.member_count)?)
        .bind(owner)
        .fetch_one(tx(permit.unit_of_work_mut())?.connection())
        .await
        .map_err(storage)?;
        let result = plan(&v, c, owner)?;
        if result.snapshot != *s || result.purpose != IndexBuildPurpose::LazyLoad {
            return Err(malformed());
        }
        permit.commit().await?;
        Ok(result)
    }
    async fn validate_current(
        &self,
        c: &RequestContext,
        s: &CanonicalGenerationSnapshot,
    ) -> Result<CurrentGenerationValidation, ApplicationError> {
        s.validate().map_err(|_| malformed())?;
        if s.workspace_id != c.workspace_id {
            return Err(malformed());
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        let v: Option<Value> = sqlx::query_scalar(
            "SELECT vestrace_validate_local_embedding_generation($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(c.workspace_id.as_uuid())
        .bind(s.generation_id.as_uuid())
        .bind(integer(s.generation_epoch)?)
        .bind(integer(s.guard_version)?)
        .bind(integer(s.corpus_revision)?)
        .bind(integer(s.built_through_projection_ordinal)?)
        .bind(integer(s.member_count)?)
        .fetch_one(tx(permit.unit_of_work_mut())?.connection())
        .await
        .map_err(storage)?;
        let result = match v {
            None => CurrentGenerationValidation::Changed,
            Some(v) => {
                if snapshot(&v["snapshot"], c)? != *s {
                    return Err(malformed());
                }
                CurrentGenerationValidation::Current {
                    space_registration_id: id(&v, "space_registration_id")?,
                }
            }
        };
        permit.commit().await?;
        Ok(result)
    }
    async fn publish_ready(
        &self,
        c: &RequestContext,
        p: &EmbeddingIndexBuildPlan,
    ) -> Result<GenerationPublicationOutcome, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        let v: Value =
            sqlx::query_scalar("SELECT vestrace_publish_embedding_index_build($1,$2,$3)")
                .bind(c.workspace_id.as_uuid())
                .bind(p.attempt_id.as_uuid())
                .bind(&p.owner)
                .fetch_one(tx(permit.unit_of_work_mut())?.connection())
                .await
                .map_err(storage)?;
        exact_plan(&v["plan"], c, p)?;
        let outcome = match string(&v, "outcome")?.as_str() {
            "published" => GenerationPublicationOutcome::Published,
            "discarded" => GenerationPublicationOutcome::Discarded,
            _ => return Err(malformed()),
        };
        // Installation is only possible after this durable commit returns successfully.
        permit.commit().await?;
        Ok(outcome)
    }
    async fn load_chunk(
        &self,
        c: &RequestContext,
        p: &EmbeddingIndexBuildPlan,
        after: Option<u64>,
        limit: u32,
    ) -> Result<EncryptedProjectionChunk, ApplicationError> {
        if !(1..=64).contains(&limit) {
            return Err(malformed());
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        let v: Value =
            sqlx::query_scalar("SELECT vestrace_load_embedding_index_chunk($1,$2,$3,$4,$5)")
                .bind(c.workspace_id.as_uuid())
                .bind(p.attempt_id.as_uuid())
                .bind(&p.owner)
                .bind(after.map(integer).transpose()?)
                .bind(i32::try_from(limit).map_err(|_| malformed())?)
                .fetch_one(tx(permit.unit_of_work_mut())?.connection())
                .await
                .map_err(storage)?;
        exact_plan(&v["plan"], c, p)?;
        let rows = v["projections"].as_array().ok_or_else(malformed)?;
        if rows.len() > limit as usize {
            return Err(malformed());
        }
        let mut projections = Vec::with_capacity(rows.len());
        let mut last = after.unwrap_or(0);
        for row in rows {
            let projection = decode_projection(row, c)?;
            if projection.projection_ordinal <= last
                || projection.projection_ordinal > p.snapshot.built_through_projection_ordinal
            {
                return Err(malformed());
            }
            last = projection.projection_ordinal;
            projections.push(projection);
        }
        let complete = v["complete"].as_bool().ok_or_else(malformed)?;
        if projections.is_empty() && !complete {
            return Err(malformed());
        }
        permit.commit().await?;
        Ok(EncryptedProjectionChunk {
            projections,
            complete,
        })
    }
    async fn finish_attempt(
        &self,
        c: &RequestContext,
        attempt: IndexBuildAttemptId,
        owner: &str,
        outcome: IndexBuildOutcome,
    ) -> Result<(), ApplicationError> {
        let (state, reason) = match outcome {
            IndexBuildOutcome::Published => ("published", None),
            IndexBuildOutcome::Loaded => ("loaded", None),
            IndexBuildOutcome::Discarded => ("discarded", None),
            IndexBuildOutcome::Failed(r) => ("failed", Some(r.as_str())),
        };
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        sqlx::query("SELECT vestrace_finish_embedding_index_attempt($1,$2,$3,$4,$5)")
            .bind(c.workspace_id.as_uuid())
            .bind(attempt.as_uuid())
            .bind(owner)
            .bind(state)
            .bind(reason)
            .execute(tx(permit.unit_of_work_mut())?.connection())
            .await
            .map_err(storage)?;
        permit.commit().await
    }
    async fn append_attempt_observation(
        &self,
        c: &RequestContext,
        attempt: IndexBuildAttemptId,
        owner: &str,
        reason: IndexFailureReason,
    ) -> Result<(), ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, c).await?;
        sqlx::query("SELECT vestrace_observe_embedding_index_attempt($1,$2,$3,$4)")
            .bind(c.workspace_id.as_uuid())
            .bind(attempt.as_uuid())
            .bind(owner)
            .bind(reason.as_str())
            .execute(tx(permit.unit_of_work_mut())?.connection())
            .await
            .map_err(storage)?;
        permit.commit().await
    }
}
fn exact_plan(
    v: &Value,
    c: &RequestContext,
    p: &EmbeddingIndexBuildPlan,
) -> Result<(), ApplicationError> {
    let q = plan(v, c, &p.owner)?;
    if q.attempt_id != p.attempt_id
        || q.space_registration_id != p.space_registration_id
        || q.snapshot != p.snapshot
        || q.purpose != p.purpose
        || q.event_id != p.event_id
    {
        return Err(malformed());
    }
    Ok(())
}
fn decode_projection(
    v: &Value,
    c: &RequestContext,
) -> Result<EncryptedIndexProjection, ApplicationError> {
    use vestrace_application::{EmbeddingOutputKeyBinding, EmbeddingResultPreparationId};
    use vestrace_domain::{
        ContentMaterialId, EmbeddingJobId, IntentNonce, MaterialKeyBindingReceipt,
        MaterialKeyCreationIntentId, MaterialKeyId,
    };
    let text = string(v, "ciphertext")?;
    if text.is_empty() || text.len() > 2 * 1048576 || text.len() % 2 != 0 {
        return Err(malformed());
    }
    let mut ciphertext = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        let h = (pair[0] as char).to_digit(16).ok_or_else(malformed)?;
        let l = (pair[1] as char).to_digit(16).ok_or_else(malformed)?;
        ciphertext.push((h * 16 + l) as u8);
    }
    Ok(EncryptedIndexProjection {
        projection_id: id(v, "projection_id")?,
        projection_ordinal: number(v, "projection_ordinal")?,
        binding: EmbeddingOutputKeyBinding {
            workspace_id: c.workspace_id,
            job_id: EmbeddingJobId::from_uuid(id(v, "job_id")?),
            intent_id: MaterialKeyCreationIntentId::from_uuid(id(v, "intent_id")?),
            material_id: ContentMaterialId::from_uuid(id(v, "material_id")?),
            key_id: MaterialKeyId::from_uuid(id(v, "material_key_id")?),
            nonce: IntentNonce::from_uuid(id(v, "intent_nonce")?),
            output_ordinal: number(v, "output_ordinal")?,
        },
        preparation_id: EmbeddingResultPreparationId::from_uuid(id(v, "preparation_id")?),
        receipt: MaterialKeyBindingReceipt::from_uuid(id(v, "binding_receipt")?),
        ciphertext,
    })
}
