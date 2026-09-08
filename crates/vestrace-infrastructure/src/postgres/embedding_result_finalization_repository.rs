//! Short SQL publication transactions and the embedding-owned keyed commitment.
use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore};
use async_trait::async_trait;
use serde_json::Value;
use vestrace_application::{
    ApplicationError, EmbeddingOutputCommitment, EmbeddingOutputKeyBinding,
    EmbeddingResultBoundOutput, EmbeddingResultCommitter, EmbeddingResultFinalizationAuthority,
    EmbeddingResultFinalizationProgress, EmbeddingResultFinalizationRepository,
    EmbeddingResultPreparationId, EmbeddingResultPublication, InstallationMutationPermit,
    PermitMode, RequestContext,
};
use vestrace_domain::{
    ContentMaterialId, EmbeddingJobId, ExternalEffectId, IntentNonce, MaterialKeyBindingReceipt,
    MaterialKeyCreationIntentId, MaterialKeyId, WorkspaceId, ZeroizingDek,
};

pub struct PgEmbeddingResultFinalizationRepository {
    permit: PgInstallationMutationPermit,
}
impl PgEmbeddingResultFinalizationRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }
}
fn conflict() -> ApplicationError {
    ApplicationError::Conflict("EMBEDDING_RESULT_FINALIZATION_CONFLICT".into())
}
fn malformed() -> ApplicationError {
    ApplicationError::Policy("EMBEDDING_RESULT_FINALIZATION_MALFORMED".into())
}
fn transaction(
    uow: &mut dyn vestrace_application::UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    uow.as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL finalization transaction".into())
        })
}
fn storage(error: sqlx::Error) -> ApplicationError {
    match error.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("42501") => {
            ApplicationError::Policy("EMBEDDING_RESULT_FINALIZATION_AUTHORITY_REFUSED".into())
        }
        Some("22023") => malformed(),
        Some("23514" | "55000") => conflict(),
        _ => ApplicationError::Storage(
            "embedding result finalization database operation failed".into(),
        ),
    }
}
fn id(value: &Value, key: &str) -> Result<uuid::Uuid, ApplicationError> {
    let parsed = uuid::Uuid::parse_str(
        value
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(malformed)?,
    )
    .map_err(|_| malformed())?;
    if parsed.is_nil() {
        return Err(malformed());
    }
    Ok(parsed)
}
fn count(value: &Value, key: &str) -> Result<u64, ApplicationError> {
    let n = value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(malformed)?;
    i64::try_from(n).map_err(|_| malformed())?;
    Ok(n)
}
fn publication(
    value: &Value,
    authority: &EmbeddingResultFinalizationAuthority,
) -> Result<EmbeddingResultPublication, ApplicationError> {
    let p = EmbeddingResultPublication {
        publication_id: id(value, "publication_id")?,
        preparation_id: EmbeddingResultPreparationId::from_uuid(id(value, "preparation_id")?),
        job_id: EmbeddingJobId::from_uuid(id(value, "job_id")?),
        effect_id: ExternalEffectId::from_uuid(id(value, "effect_id")?),
        space_registration_id: id(value, "space_registration_id")?,
        rebuild_event_id: id(value, "rebuild_event_id")?,
        output_count: count(value, "output_count")?,
        terminal_job_version: count(value, "terminal_job_version")?,
        resulting_corpus_revision: count(value, "resulting_corpus_revision")?,
        live_member_count: count(value, "live_member_count")?,
        generation_epoch: count(value, "generation_epoch")?,
    };
    if p.preparation_id != authority.preparation_id
        || p.job_id != authority.job_id
        || p.effect_id != authority.effect_id
        || p.output_count == 0
    {
        return Err(conflict());
    }
    Ok(p)
}
fn binding(
    value: &Value,
    context: &RequestContext,
    authority: &EmbeddingResultFinalizationAuthority,
) -> Result<EmbeddingOutputKeyBinding, ApplicationError> {
    let b = EmbeddingOutputKeyBinding {
        workspace_id: WorkspaceId::from_uuid(id(value, "workspace_id")?),
        job_id: EmbeddingJobId::from_uuid(id(value, "job_id")?),
        intent_id: MaterialKeyCreationIntentId::from_uuid(id(value, "intent_id")?),
        material_id: ContentMaterialId::from_uuid(id(value, "material_id")?),
        key_id: MaterialKeyId::from_uuid(id(value, "material_key_id")?),
        nonce: IntentNonce::from_uuid(id(value, "intent_nonce")?),
        output_ordinal: count(value, "output_ordinal")?,
    };
    if b.workspace_id != context.workspace_id || b.job_id != authority.job_id {
        return Err(conflict());
    }
    Ok(b)
}
fn hex(value: &Value) -> Result<Vec<u8>, ApplicationError> {
    let text = value.as_str().ok_or_else(malformed)?;
    if text.is_empty() || text.len() % 2 != 0 {
        return Err(malformed());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            fn digit(b: u8) -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            }
            Ok(
                digit(pair[0]).ok_or_else(malformed)? * 16
                    + digit(pair[1]).ok_or_else(malformed)?,
            )
        })
        .collect()
}
fn progress(
    value: &Value,
    context: &RequestContext,
    authority: &EmbeddingResultFinalizationAuthority,
) -> Result<EmbeddingResultFinalizationProgress, ApplicationError> {
    let phase = value
        .get("phase")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?;
    if phase == "published" {
        return Ok(EmbeddingResultFinalizationProgress::Published(publication(
            value.get("publication").ok_or_else(malformed)?,
            authority,
        )?));
    }
    if value
        .get("credential_adoption_required")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(conflict());
    }
    let rows = value
        .get("outputs")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?;
    if rows.is_empty() {
        return Err(malformed());
    }
    let mut bindings = Vec::new();
    let mut outputs = Vec::new();
    let mut previous = None;
    let mut intents = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    let mut materials = std::collections::HashSet::new();
    let mut projections = std::collections::HashSet::new();
    for (ordinal, row) in rows.iter().enumerate() {
        let b = binding(row, context, authority)?;
        if previous.is_some_and(|last| last >= b.output_ordinal)
            || !intents.insert(b.intent_id)
            || !keys.insert(b.key_id)
            || !materials.insert(b.material_id)
        {
            return Err(conflict());
        }
        previous = Some(b.output_ordinal);
        match phase {
            "needs_binding" => {
                if row.get("binding_receipt").is_some_and(|v| !v.is_null()) {
                    return Err(conflict());
                }
                bindings.push(b);
            }
            "ready_to_publish" => {
                if u64::try_from(ordinal).ok() != Some(b.output_ordinal) {
                    return Err(conflict());
                }
                let projection_id = id(row, "projection_id")?;
                if !projections.insert(projection_id) {
                    return Err(conflict());
                }
                outputs.push(EmbeddingResultBoundOutput {
                    binding: b,
                    projection_id,
                    receipt: MaterialKeyBindingReceipt::from_uuid(id(row, "binding_receipt")?),
                    ciphertext: hex(row.get("ciphertext").ok_or_else(malformed)?)?,
                });
            }
            _ => return Err(malformed()),
        }
    }
    Ok(if phase == "needs_binding" {
        EmbeddingResultFinalizationProgress::NeedsBinding(bindings)
    } else {
        EmbeddingResultFinalizationProgress::ReadyToPublish(outputs)
    })
}

/// This port runs only inside the service's exact bound-key callback.
#[derive(Default)]
pub struct EmbeddingOutputHmacCommitter;
impl EmbeddingOutputHmacCommitter {
    pub fn new() -> Self {
        Self
    }
}
impl EmbeddingResultCommitter for EmbeddingOutputHmacCommitter {
    fn commitment(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        output: &EmbeddingResultBoundOutput,
        dek: &ZeroizingDek,
    ) -> Result<[u8; 32], ApplicationError> {
        if output.binding.workspace_id != context.workspace_id
            || output.binding.job_id != authority.job_id
            || output.ciphertext.is_empty()
        {
            return Err(conflict());
        }
        let length = u64::try_from(output.ciphertext.len()).map_err(|_| malformed())?;
        let ids = [
            context.workspace_id.as_uuid(),
            authority.preparation_id.as_uuid(),
            authority.job_id.as_uuid(),
            authority.effect_id.as_uuid(),
            output.projection_id,
            output.binding.intent_id.as_uuid(),
            output.binding.material_id.as_uuid(),
            output.binding.key_id.as_uuid(),
            output.binding.nonce.as_uuid(),
        ];
        if ids.iter().any(uuid::Uuid::is_nil) {
            return Err(malformed());
        }
        Ok(dek.expose(|key| {
            let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key);
            let mut hmac = ring::hmac::Context::with_key(&key);
            hmac.update(b"vestrace.embedding-output.commitment.v1\0");
            for id in ids {
                hmac.update(id.as_bytes());
            }
            hmac.update(&output.binding.output_ordinal.to_be_bytes());
            hmac.update(&length.to_be_bytes());
            hmac.update(&output.ciphertext);
            let tag = hmac.sign();
            let mut bytes = [0; 32];
            bytes.copy_from_slice(tag.as_ref());
            bytes
        }))
    }
}

async fn load(
    tx: &mut PgScopedTransaction,
    context: &RequestContext,
    authority: &EmbeddingResultFinalizationAuthority,
) -> Result<Value, ApplicationError> {
    sqlx::query_scalar("SELECT vestrace_load_embedding_result_finalization($1,$2,$3,$4)")
        .bind(context.workspace_id.as_uuid())
        .bind(authority.preparation_id.as_uuid())
        .bind(authority.job_id.as_uuid())
        .bind(authority.effect_id.as_uuid())
        .fetch_one(tx.connection())
        .await
        .map_err(storage)
}
#[async_trait]
impl EmbeddingResultFinalizationRepository for PgEmbeddingResultFinalizationRepository {
    async fn load_progress(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
    ) -> Result<EmbeddingResultFinalizationProgress, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let mut value = load(transaction(permit.unit_of_work_mut())?, context, authority).await?;
        if value.get("phase").and_then(Value::as_str) != Some("published")
            && value
                .get("credential_adoption_required")
                .and_then(Value::as_bool)
                == Some(true)
        {
            let _: uuid::Uuid = sqlx::query_scalar(
                "SELECT vestrace_adopt_embedding_result_credential_blocker($1,$2,$3,$4)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(authority.preparation_id.as_uuid())
            .bind(authority.job_id.as_uuid())
            .bind(authority.effect_id.as_uuid())
            .fetch_one(transaction(permit.unit_of_work_mut())?.connection())
            .await
            .map_err(storage)?;
            value = load(transaction(permit.unit_of_work_mut())?, context, authority).await?;
        }
        let result = progress(&value, context, authority)?;
        permit.commit().await?;
        Ok(result)
    }
    async fn record_binding(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        binding: &EmbeddingOutputKeyBinding,
        receipt: &MaterialKeyBindingReceipt,
    ) -> Result<MaterialKeyBindingReceipt, ApplicationError> {
        if binding.workspace_id != context.workspace_id
            || binding.job_id != authority.job_id
            || receipt.as_uuid().is_nil()
        {
            return Err(conflict());
        }
        let ordinal = i64::try_from(binding.output_ordinal).map_err(|_| malformed())?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        // The caller's complete binding must match persisted output identity;
        // an ordinal alone is not authority for a changed key or intent.
        let value = load(transaction(permit.unit_of_work_mut())?, context, authority).await?;
        let rows = value
            .get("all_output_bindings")
            .and_then(Value::as_array)
            .ok_or_else(malformed)?;
        let mut found = false;
        for (position, row) in rows.iter().enumerate() {
            let stored_binding = self::binding(row, context, authority)?;
            if u64::try_from(position).ok() != Some(stored_binding.output_ordinal) {
                return Err(conflict());
            }
            if stored_binding == *binding {
                found = true;
            }
        }
        if !found {
            return Err(conflict());
        }
        let stored: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_record_embedding_result_key_binding($1,$2,$3,$4,$5,$6)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(authority.preparation_id.as_uuid())
        .bind(authority.job_id.as_uuid())
        .bind(authority.effect_id.as_uuid())
        .bind(ordinal)
        .bind(receipt.as_uuid())
        .fetch_one(transaction(permit.unit_of_work_mut())?.connection())
        .await
        .map_err(storage)?;
        if stored != receipt.as_uuid() {
            return Err(conflict());
        }
        permit.commit().await?;
        Ok(MaterialKeyBindingReceipt::from_uuid(stored))
    }
    async fn publish(
        &self,
        context: &RequestContext,
        authority: &EmbeddingResultFinalizationAuthority,
        publication_id: uuid::Uuid,
        rebuild_event_id: uuid::Uuid,
        commitments: Vec<EmbeddingOutputCommitment>,
    ) -> Result<EmbeddingResultPublication, ApplicationError> {
        if publication_id.is_nil() || rebuild_event_id.is_nil() || commitments.is_empty() {
            return Err(malformed());
        }
        let mut projections = Vec::with_capacity(commitments.len());
        let mut ordinals = Vec::with_capacity(commitments.len());
        let mut bytes = Vec::with_capacity(commitments.len());
        let mut seen = std::collections::HashSet::new();
        for (position, c) in commitments.into_iter().enumerate() {
            if c.projection_id.is_nil()
                || !seen.insert(c.projection_id)
                || u64::try_from(position).ok() != Some(c.output_ordinal)
            {
                return Err(malformed());
            }
            projections.push(c.projection_id);
            ordinals.push(i64::try_from(c.output_ordinal).map_err(|_| malformed())?);
            bytes.push(c.commitment.to_vec());
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let value: Value = sqlx::query_scalar(
            "SELECT vestrace_publish_embedding_job_result($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(authority.preparation_id.as_uuid())
        .bind(authority.job_id.as_uuid())
        .bind(authority.effect_id.as_uuid())
        .bind(publication_id)
        .bind(rebuild_event_id)
        .bind(projections)
        .bind(ordinals)
        .bind(bytes)
        .fetch_one(transaction(permit.unit_of_work_mut())?.connection())
        .await
        .map_err(storage)?;
        let result = publication(&value, authority)?;
        permit.commit().await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        RequestContext,
        EmbeddingResultFinalizationAuthority,
        EmbeddingResultBoundOutput,
    ) {
        let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
        let authority = EmbeddingResultFinalizationAuthority {
            preparation_id: EmbeddingResultPreparationId::new(),
            job_id: EmbeddingJobId::new(),
            effect_id: ExternalEffectId::new(),
        };
        let output = EmbeddingResultBoundOutput {
            binding: EmbeddingOutputKeyBinding {
                workspace_id: context.workspace_id,
                job_id: authority.job_id,
                intent_id: MaterialKeyCreationIntentId::new(),
                material_id: ContentMaterialId::new(),
                key_id: MaterialKeyId::new(),
                nonce: IntentNonce::new(),
                output_ordinal: 0,
            },
            projection_id: uuid::Uuid::now_v7(),
            receipt: MaterialKeyBindingReceipt::new(),
            ciphertext: vec![1, 2, 3, 4],
        };
        (context, authority, output)
    }
    fn output_json(o: &EmbeddingResultBoundOutput) -> Value {
        serde_json::json!({"workspace_id":o.binding.workspace_id.as_uuid(),"job_id":o.binding.job_id.as_uuid(),"intent_id":o.binding.intent_id.as_uuid(),"material_id":o.binding.material_id.as_uuid(),"material_key_id":o.binding.key_id.as_uuid(),"intent_nonce":o.binding.nonce.as_uuid(),"output_ordinal":o.binding.output_ordinal,"projection_id":o.projection_id,"binding_receipt":o.receipt.as_uuid(),"ciphertext":"01020304"})
    }
    #[test]
    fn commitment_matches_exact_binary_protocol_and_every_tuple_field() {
        let (c, a, o) = fixture();
        let dek = ZeroizingDek::new([45; 32]);
        let committer = EmbeddingOutputHmacCommitter;
        let baseline = committer.commitment(&c, &a, &o, &dek).unwrap();
        let mut expected = b"vestrace.embedding-output.commitment.v1\0".to_vec();
        for id in [
            c.workspace_id.as_uuid(),
            a.preparation_id.as_uuid(),
            a.job_id.as_uuid(),
            a.effect_id.as_uuid(),
            o.projection_id,
            o.binding.intent_id.as_uuid(),
            o.binding.material_id.as_uuid(),
            o.binding.key_id.as_uuid(),
            o.binding.nonce.as_uuid(),
        ] {
            expected.extend_from_slice(id.as_bytes());
        }
        expected.extend_from_slice(&0u64.to_be_bytes());
        expected.extend_from_slice(&4u64.to_be_bytes());
        expected.extend_from_slice(&[1, 2, 3, 4]);
        let expected = ring::hmac::sign(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &[45; 32]),
            &expected,
        );
        assert_eq!(&baseline[..], expected.as_ref());
        for field in 0..12 {
            let mut c = c.clone();
            let mut a = a.clone();
            let mut o = o.clone();
            match field {
                0 => {
                    c.workspace_id = WorkspaceId::new();
                    o.binding.workspace_id = c.workspace_id;
                }
                1 => a.preparation_id = EmbeddingResultPreparationId::new(),
                2 => {
                    a.job_id = EmbeddingJobId::new();
                    o.binding.job_id = a.job_id;
                }
                3 => a.effect_id = ExternalEffectId::new(),
                4 => o.projection_id = uuid::Uuid::now_v7(),
                5 => o.binding.intent_id = MaterialKeyCreationIntentId::new(),
                6 => o.binding.material_id = ContentMaterialId::new(),
                7 => o.binding.key_id = MaterialKeyId::new(),
                8 => o.binding.nonce = IntentNonce::new(),
                9 => o.binding.output_ordinal = 1,
                10 => o.ciphertext[0] ^= 1,
                _ => o.ciphertext.push(5),
            }
            assert_ne!(
                committer.commitment(&c, &a, &o, &dek).unwrap(),
                baseline,
                "tuple field {field}"
            );
        }
        assert_ne!(
            committer
                .commitment(&c, &a, &o, &ZeroizingDek::new([46; 32]))
                .unwrap(),
            baseline
        );
    }
    #[test]
    fn decoding_refuses_partial_mismatched_and_overflowed_outputs() {
        let (c, a, o) = fixture();
        let row = output_json(&o);
        let valid = serde_json::json!({"phase":"ready_to_publish","credential_adoption_required":false,"outputs":[row]});
        assert!(matches!(
            progress(&valid, &c, &a).unwrap(),
            EmbeddingResultFinalizationProgress::ReadyToPublish(_)
        ));
        for field in [
            "workspace_id",
            "job_id",
            "intent_id",
            "material_id",
            "material_key_id",
            "intent_nonce",
            "output_ordinal",
            "projection_id",
            "binding_receipt",
            "ciphertext",
        ] {
            let mut broken = valid.clone();
            broken["outputs"][0].as_object_mut().unwrap().remove(field);
            assert!(progress(&broken, &c, &a).is_err(), "missing {field}");
        }
        for bad in [
            serde_json::json!(-1),
            serde_json::json!(u64::MAX),
            serde_json::json!("0"),
        ] {
            let mut broken = valid.clone();
            broken["outputs"][0]["output_ordinal"] = bad;
            assert!(progress(&broken, &c, &a).is_err());
        }
        for bad in ["", "0", "zz", "eg"] {
            let mut broken = valid.clone();
            broken["outputs"][0]["ciphertext"] = serde_json::json!(bad);
            assert!(progress(&broken, &c, &a).is_err());
        }
        let mut wrong = valid.clone();
        wrong["outputs"][0]["workspace_id"] = serde_json::json!(uuid::Uuid::now_v7());
        assert!(progress(&wrong, &c, &a).is_err());
        let mut duplicate = valid.clone();
        duplicate["outputs"]
            .as_array_mut()
            .unwrap()
            .push(output_json(&o));
        assert!(progress(&duplicate, &c, &a).is_err());
        let mut legacy = valid.clone();
        legacy["credential_adoption_required"] = serde_json::json!(true);
        assert!(progress(&legacy, &c, &a).is_err());
        let mut missing = valid.clone();
        missing["phase"] = serde_json::json!("needs_binding");
        assert!(progress(&missing, &c, &a).is_err());
        missing["outputs"][0]["binding_receipt"] = Value::Null;
        assert!(matches!(
            progress(&missing, &c, &a).unwrap(),
            EmbeddingResultFinalizationProgress::NeedsBinding(_)
        ));
    }
    #[test]
    fn published_decode_uses_stored_identity_before_adoption_fields() {
        let (c, a, _) = fixture();
        let p = serde_json::json!({"publication_id":uuid::Uuid::now_v7(),"preparation_id":a.preparation_id.as_uuid(),"job_id":a.job_id.as_uuid(),"effect_id":a.effect_id.as_uuid(),"space_registration_id":uuid::Uuid::now_v7(),"rebuild_event_id":uuid::Uuid::now_v7(),"output_count":2,"terminal_job_version":3,"resulting_corpus_revision":4,"live_member_count":2,"generation_epoch":1});
        let value = serde_json::json!({"phase":"published","publication":p,"credential_adoption_required":true});
        assert!(matches!(
            progress(&value, &c, &a).unwrap(),
            EmbeddingResultFinalizationProgress::Published(_)
        ));
        for field in ["preparation_id", "job_id", "effect_id"] {
            let mut wrong = value.clone();
            wrong["publication"][field] = serde_json::json!(uuid::Uuid::now_v7());
            assert!(progress(&wrong, &c, &a).is_err());
        }
        let mut overflow = value.clone();
        overflow["publication"]["output_count"] = serde_json::json!(u64::MAX);
        assert!(progress(&overflow, &c, &a).is_err());
    }
}
