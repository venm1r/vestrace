//! PostgreSQL authority for legacy embedding adoption plans and their cutover.
//!
//! Every state change here is one guarded function call. Nothing in this file
//! decides whether a member may advance, whether a plan is ready, or whether
//! the installation gate may commit: the database refuses, and this adapter
//! translates the refusal.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, CreateModelRequestEvidence, EffectiveRequestLimits,
    GovernedInputSealer, IdempotencyRecord, MaterialIntentCommands, MaterialKeyVault,
    ModelRequestEvidenceRepository, OutboxMessage, RequestContext, SharedEmbeddingJobRepository,
    TransactionManager,
    embedding::{
        EmbeddingLegacyAdoptionRepository, LegacyAdoptionBlocker, LegacyAdoptionBlockerRecord,
        LegacyAdoptionMember, LegacyAdoptionMemberState, LegacyAdoptionProgress,
        LegacyAdoptionRebuildFactory, LegacyAdoptionSourceMaterializer, MaterializedSource,
        StartLegacyAdoption,
    },
};
use vestrace_domain::{
    AuditEvent, Capability, ContentMaterialId, DeliverySemantics, EffectPrecondition,
    EffectReversibility, EmbeddingJobId, EmbeddingSpaceId, ExternalEffectIntent,
    IdempotencyProfile, IntentNonce, MaterialKeyBindingReceipt, MaterialKeyCreationIntent,
    MaterialKeyCreationIntentId, MaterialKeyId, ModelRequestEvidenceId,
    PreparedMaterialAttachmentId, RiskCategory,
    embedding::{EmbeddingJobKind, LegacyAdoptionState},
    id::{AuditEventId, LegacyAdoptionId},
};

use super::{
    PgModelRequestEvidenceRepository, PgStore, PgTransactionManager,
    material_intent::PgMaterialIntentRepository,
};

#[derive(Clone, Debug)]
pub struct PgEmbeddingLegacyAdoptionRepository {
    store: PgStore,
}

impl PgEmbeddingLegacyAdoptionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn adoption_state(value: &str) -> Result<LegacyAdoptionState, ApplicationError> {
    LegacyAdoptionState::ALL
        .into_iter()
        .find(|state| state.as_str() == value)
        .ok_or_else(|| {
            ApplicationError::Storage(format!("stored legacy adoption state {value} is unknown"))
        })
}

fn non_negative(value: i64, what: &str) -> Result<u64, ApplicationError> {
    u64::try_from(value)
        .map_err(|_| ApplicationError::Internal(format!("{what} must be non-negative")))
}

fn map_adoption_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("40001") | Some("23505") => {
            ApplicationError::Conflict("EMBEDDING_LEGACY_ADOPTION_CONFLICT".to_owned())
        }
        Some("23514") | Some("22023") => ApplicationError::Policy(
            error
                .as_database_error()
                .map(|database| database.message().to_owned())
                .unwrap_or_else(|| "legacy adoption refused".to_owned()),
        ),
        Some("42501") => ApplicationError::Unavailable(
            "governed legacy adoption authority is unavailable".to_owned(),
        ),
        _ => ApplicationError::Storage(error.to_string()),
    }
}

impl PgEmbeddingLegacyAdoptionRepository {
    /// Reads the plan back from the rows the database holds, so a caller never
    /// reports counts it accumulated itself.
    async fn read_progress(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        connection: &mut sqlx::PgConnection,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let plan: (String, i64, Uuid) = sqlx::query_as(
            "SELECT state, version, target_space_registration_id \
               FROM embedding_legacy_adoptions WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_adoption_error)?
        .ok_or_else(|| ApplicationError::Policy("legacy adoption plan is absent".to_owned()))?;

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), \
                    count(*) FILTER (WHERE state='satisfied'), \
                    count(*) FILTER (WHERE state='blocked') \
               FROM embedding_legacy_adoption_members \
              WHERE workspace_id=$1 AND adoption_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_one(&mut *connection)
        .await
        .map_err(map_adoption_error)?;

        let blocker_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT member_ordinal, reason FROM embedding_legacy_adoption_blockers \
              WHERE workspace_id=$1 AND adoption_id=$2 ORDER BY member_ordinal, reason",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_all(&mut *connection)
        .await
        .map_err(map_adoption_error)?;
        let mut blockers = Vec::with_capacity(blocker_rows.len());
        for (ordinal, reason) in blocker_rows {
            blockers.push(LegacyAdoptionBlockerRecord {
                ordinal: non_negative(ordinal, "member ordinal")?,
                reason: LegacyAdoptionBlocker::parse(&reason).ok_or_else(|| {
                    ApplicationError::Storage(format!(
                        "stored legacy adoption blocker {reason} is unknown"
                    ))
                })?,
            });
        }

        let deleted: Option<i64> = sqlx::query_scalar(
            "SELECT deleted_legacy_row_count FROM embedding_legacy_cutover_receipts \
              WHERE workspace_id=$1 AND adoption_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_adoption_error)?;

        Ok(LegacyAdoptionProgress {
            plan_id,
            state: adoption_state(&plan.0)?,
            version: non_negative(plan.1, "plan version")?,
            target_space_registration_id: plan.2,
            total_members: non_negative(counts.0, "member count")?,
            satisfied_members: non_negative(counts.1, "satisfied count")?,
            blocked_members: non_negative(counts.2, "blocked count")?,
            blockers,
            deleted_legacy_rows: deleted
                .map(|value| non_negative(value, "deleted legacy row count"))
                .transpose()?,
        })
    }
}

#[async_trait]
impl EmbeddingLegacyAdoptionRepository for PgEmbeddingLegacyAdoptionRepository {
    async fn start_or_resume(
        &self,
        context: &RequestContext,
        command: StartLegacyAdoption,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let plan: Uuid =
            sqlx::query_scalar("SELECT vestrace_start_or_resume_legacy_adoption($1,$2,$3,$4,$5)")
                .bind(command.plan_id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .bind(command.legacy_space_registration_id)
                .bind(command.target_space_registration_id)
                .bind(command.idempotency_key.as_str())
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        let progress = self
            .read_progress(
                context,
                LegacyAdoptionId::from_uuid(plan),
                transaction.connection(),
            )
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(progress)
    }

    async fn progress(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let progress = self
            .read_progress(context, plan_id, transaction.connection())
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(progress)
    }

    async fn unfinished_members(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        limit: u32,
    ) -> Result<Vec<LegacyAdoptionMember>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let rows: Vec<(
            i64,
            Uuid,
            Uuid,
            Uuid,
            Option<Uuid>,
            Option<Uuid>,
            Option<Uuid>,
            String,
        )> = sqlx::query_as(
            "SELECT member_ordinal, legacy_embedding_id, memory_id, memory_revision_id, \
                        source_material_id, source_intent_id, rebuild_job_id, state \
                   FROM embedding_legacy_adoption_members \
                  WHERE workspace_id=$1 AND adoption_id=$2 AND state IN ('planned','sourced') \
                  ORDER BY member_ordinal LIMIT $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        rows.into_iter()
            .map(|row| {
                Ok(LegacyAdoptionMember {
                    ordinal: non_negative(row.0, "member ordinal")?,
                    legacy_embedding_id: row.1,
                    memory_id: row.2,
                    memory_revision_id: row.3,
                    source_material_id: row.4,
                    source_intent_id: row.5,
                    rebuild_job_id: row.6,
                    state: LegacyAdoptionMemberState::parse(&row.7).ok_or_else(|| {
                        ApplicationError::Storage(format!(
                            "stored legacy adoption member state {} is unknown",
                            row.7
                        ))
                    })?,
                })
            })
            .collect()
    }

    async fn bind_source(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        source: MaterializedSource,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_bind_legacy_adoption_source($1,$2,$3,$4,$5)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(source.material_id)
                    .bind(source.intent_id)
            },
        )
        .await
    }

    async fn bind_rebuild(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        job_id: EmbeddingJobId,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_bind_legacy_adoption_rebuild($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(job_id.as_uuid())
            },
        )
        .await
    }

    async fn satisfy_member(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        projection_entry_id: Uuid,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_satisfy_legacy_adoption_member($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(projection_entry_id)
            },
        )
        .await
    }

    async fn record_blocker(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        reason: LegacyAdoptionBlocker,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_record_legacy_adoption_blocker($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(reason.as_str())
            },
        )
        .await
    }

    async fn prove_ready(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        generation_id: Uuid,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let version: i64 =
            sqlx::query_scalar("SELECT vestrace_prove_legacy_adoption_ready($1,$2,$3)")
                .bind(context.workspace_id.as_uuid())
                .bind(plan_id.as_uuid())
                .bind(generation_id)
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(version, "plan version")
    }

    async fn commit_cutover(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        receipt_id: Uuid,
        expected_version: u64,
        audit_event_id: Uuid,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let deleted: i64 =
            sqlx::query_scalar("SELECT vestrace_commit_legacy_adoption_cutover($1,$2,$3,$4,$5)")
                .bind(receipt_id)
                .bind(context.workspace_id.as_uuid())
                .bind(plan_id.as_uuid())
                .bind(i64::try_from(expected_version).unwrap_or(i64::MAX))
                .bind(audit_event_id)
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(deleted, "deleted legacy row count")
    }

    async fn commit_plaintext_retirement(
        &self,
        context: &RequestContext,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let completed: i64 =
            sqlx::query_scalar("SELECT vestrace_commit_legacy_plaintext_retirement()")
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(completed, "completed adoption count")
    }
}

impl PgEmbeddingLegacyAdoptionRepository {
    /// The shared shape of every void-returning guarded call: one scoped
    /// transaction, the workspace bound first, and the database's refusal
    /// carried through unchanged.
    async fn call<F>(
        &self,
        context: &RequestContext,
        sql: &str,
        bind: F,
    ) -> Result<(), ApplicationError>
    where
        F: for<'q> FnOnce(
            sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        )
            -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        bind(sqlx::query(sql).bind(context.workspace_id.as_uuid()))
            .execute(transaction.connection())
            .await
            .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(())
    }
}

/// Turns one memory revision's current content into a Live governed content
/// material owned by that revision.
///
/// This is the join the canonical corpus was missing. A canonical generation
/// member points at a projection, whose source is a content material; memory
/// content lived only in `memory_revisions.content`, so nothing connected a
/// member back to a memory. The `content_material_ordinary_references` row this
/// produces, with `owner_kind` `memory_revision`, is that connection.
///
/// The lifecycle is the ordinary one -- reserve, provisional key, seal,
/// prepare, bind, finalize -- and deliberately not a shortcut of it: a material
/// adoption produced by a different path would not be erasable, hydratable or
/// revocable by the machinery every other material already answers to.
pub struct PgLegacyAdoptionSourceMaterializer<V, C> {
    store: PgStore,
    materials: MaterialIntentCommands<PgMaterialIntentRepository>,
    vault: Arc<V>,
    sealer: Arc<C>,
}

impl<V, C> PgLegacyAdoptionSourceMaterializer<V, C> {
    pub fn new(store: PgStore, vault: Arc<V>, sealer: Arc<C>) -> Self {
        Self {
            materials: MaterialIntentCommands::new(PgMaterialIntentRepository::new(store.clone())),
            store,
            vault,
            sealer,
        }
    }
}

#[async_trait]
impl<V, C> LegacyAdoptionSourceMaterializer for PgLegacyAdoptionSourceMaterializer<V, C>
where
    V: MaterialKeyVault + Send + Sync + 'static,
    C: GovernedInputSealer + Send + Sync + 'static,
{
    async fn materialize(
        &self,
        context: &RequestContext,
        member: &LegacyAdoptionMember,
    ) -> Result<Result<MaterializedSource, LegacyAdoptionBlocker>, ApplicationError> {
        // Read the content and the memory's state together, so a memory that
        // was deleted between planning and this call is a blocker rather than
        // a material nobody should have made.
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT revision.content, memory.status \
               FROM memory_revisions AS revision \
               JOIN memories AS memory \
                 ON memory.workspace_id = revision.workspace_id \
                AND memory.id = revision.memory_id \
              WHERE revision.workspace_id = $1 AND revision.id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(member.memory_revision_id)
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        let Some((content, status)) = row else {
            return Ok(Err(LegacyAdoptionBlocker::SourceRevisionAbsent));
        };
        if status == "deleted" {
            return Ok(Err(LegacyAdoptionBlocker::SourceContentErased));
        }
        if content.is_empty() {
            // Nothing to embed. Visible as a blocker rather than silently
            // producing a vector of an empty string.
            return Ok(Err(LegacyAdoptionBlocker::SourceContentErased));
        }

        let intent_id = MaterialKeyCreationIntentId::new();
        let material_id = ContentMaterialId::new();
        let key_id = MaterialKeyId::new();
        let nonce = IntentNonce::new();
        let attachment_id = PreparedMaterialAttachmentId::new();
        let intent = MaterialKeyCreationIntent::reserve(
            intent_id,
            context.workspace_id,
            material_id,
            key_id,
            nonce,
            "memory_revision",
            member.memory_revision_id,
            0,
        );
        self.materials.reserve(context, &intent).await?;

        let receipt = self
            .vault
            .create_if_absent(key_id, nonce)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        self.materials
            .record_provisional_created(context, intent_id)
            .await?;
        self.materials
            .record_provisional_receipt(context, intent_id, receipt)
            .await?;

        // The plaintext is borrowed for exactly this callback; the DEK never
        // leaves it and `sealed` receives ciphertext only.
        let mut sealed: Option<Result<Vec<u8>, ApplicationError>> = None;
        self.vault
            .unwrap(key_id, &mut |dek| {
                sealed = Some(self.sealer.seal(
                    context.workspace_id,
                    material_id,
                    key_id,
                    dek,
                    content.as_bytes(),
                ));
            })
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let ciphertext = sealed.ok_or_else(|| {
            ApplicationError::Internal(
                "the material vault returned without sealing the adoption source".to_owned(),
            )
        })??;
        drop(content);

        self.materials
            .prepare_content(
                context,
                intent_id,
                attachment_id,
                &ciphertext,
                vestrace_domain::size_class_for(ciphertext.len()),
            )
            .await?;
        self.materials
            .bind(context, intent_id, MaterialKeyBindingReceipt::new())
            .await?;
        self.materials.finalize_bound(context, intent_id).await?;

        Ok(Ok(MaterializedSource {
            material_id: material_id.as_uuid(),
            intent_id: intent_id.as_uuid(),
        }))
    }
}

/// Creates the governed `rebuild` job that recomputes one adopted vector.
///
/// It mints no binding snapshot. On the embedding side a snapshot is issued
/// only by `vestrace_plan_embedding_transition_version`, which records it in
/// `model_binding_snapshot_scopes` against the transition plan that established
/// the canonical space; this looks that snapshot up rather than inventing a
/// second binding for the same space, which would let two jobs claim the same
/// corpus under different qualification.
///
/// Order matters and is not a preference. `vestrace_create_model_request_evidence`
/// proves an `embedding_job` cause by finding the job that owns this exact
/// effect and pinned this exact snapshot, so the job is accepted first and its
/// evidence written second.
pub struct PgLegacyAdoptionRebuildFactory {
    store: PgStore,
    jobs: SharedEmbeddingJobRepository,
    evidence: Arc<PgModelRequestEvidenceRepository>,
    transactions: PgTransactionManager,
    limits: EffectiveRequestLimits,
}

/// The binding a canonical space was established under.
struct CanonicalBinding {
    snapshot_id: Uuid,
    connection_revision_id: Uuid,
    connection_qualification_revision_id: Uuid,
    model_revision_id: Uuid,
    model_qualification_revision_id: Uuid,
    runtime_base_url: String,
    adapter_profile_revision: String,
}

impl PgLegacyAdoptionRebuildFactory {
    pub fn new(
        store: PgStore,
        jobs: SharedEmbeddingJobRepository,
        evidence: Arc<PgModelRequestEvidenceRepository>,
        limits: EffectiveRequestLimits,
    ) -> Self {
        Self {
            transactions: PgTransactionManager::new(store.clone()),
            store,
            jobs,
            evidence,
            limits,
        }
    }

    async fn canonical_binding(
        &self,
        context: &RequestContext,
        registration: Uuid,
    ) -> Result<CanonicalBinding, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        // The newest transition plan for this space names the binding it was
        // most recently established under; an older plan's snapshot would pin a
        // qualification the space has since moved off.
        let row: Option<(Uuid, Uuid, Uuid, Uuid, Uuid, String, String)> = sqlx::query_as(
            "SELECT snapshot.id, snapshot.connection_revision_id, \
                    snapshot.connection_qualification_revision_id, \
                    snapshot.model_revision_id, snapshot.model_qualification_revision_id, \
                    revision.runtime_base_url, revision.adapter_profile_revision \
               FROM model_binding_snapshot_scopes AS scope \
               JOIN embedding_transition_plans AS plan \
                 ON plan.workspace_id = scope.workspace_id \
                AND plan.id = scope.transition_plan_id \
               JOIN model_binding_snapshots AS snapshot \
                 ON snapshot.workspace_id = scope.workspace_id \
                AND snapshot.id = scope.snapshot_id \
               JOIN connection_revisions AS revision \
                 ON revision.workspace_id = snapshot.workspace_id \
                AND revision.id = snapshot.connection_revision_id \
              WHERE scope.workspace_id = $1 AND scope.scope = 'transition' \
                AND plan.target_space_registration_id = $2 \
              ORDER BY plan.version DESC LIMIT 1",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(registration)
        .fetch_optional(transaction.connection())
        .await
        .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        let Some(row) = row else {
            return Err(ApplicationError::Unavailable(
                "the canonical target space has no transition-issued binding snapshot".to_owned(),
            ));
        };
        Ok(CanonicalBinding {
            snapshot_id: row.0,
            connection_revision_id: row.1,
            connection_qualification_revision_id: row.2,
            model_revision_id: row.3,
            model_qualification_revision_id: row.4,
            runtime_base_url: row.5,
            adapter_profile_revision: row.6,
        })
    }
}

#[async_trait]
impl LegacyAdoptionRebuildFactory for PgLegacyAdoptionRebuildFactory {
    async fn create_rebuild(
        &self,
        context: &RequestContext,
        target_space_registration_id: Uuid,
        member: &LegacyAdoptionMember,
        source: MaterializedSource,
    ) -> Result<EmbeddingJobId, ApplicationError> {
        let binding = self
            .canonical_binding(context, target_space_registration_id)
            .await?;
        let job_id = EmbeddingJobId::new();
        let evidence_id = ModelRequestEvidenceId::new();
        let at = vestrace_domain::now();

        // The effect is scoped to the workspace and names the runtime endpoint
        // the canonical binding already resolved. Nothing here chooses a target
        // a caller supplied.
        let intent = ExternalEffectIntent::new(
            "workspace://",
            context.workspace_id,
            context.principal_id,
            binding.adapter_profile_revision.clone(),
            "embeddings",
            binding.runtime_base_url.clone(),
            format!("sha256:adoption-rebuild:{}", member.legacy_embedding_id),
            "recompute one adopted embedding through the governed provider path",
            vec![
                EffectPrecondition::new("model-snapshot", binding.snapshot_id.to_string())
                    .map_err(ApplicationError::Domain)?,
            ],
            format!("sha256:adoption-preconditions:{}", binding.snapshot_id),
            RiskCategory::Medium,
            EffectReversibility::Unknown,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExportRead,
            None::<String>,
            None::<String>,
            at,
        )
        .map_err(ApplicationError::Domain)?;

        let idempotency_key = format!("legacy-adoption-rebuild:{}", member.legacy_embedding_id);
        let acceptance = AcceptEmbeddingJob {
            job_id,
            space_registration_id: EmbeddingSpaceId::from_uuid(target_space_registration_id),
            kind: EmbeddingJobKind::Rebuild,
            model_binding_snapshot_id: binding.snapshot_id,
            intent,
            model_request_evidence_id: evidence_id,
            retries_unknown_embedding_job_id: None,
            expected_predecessor_version: None,
            idempotency: Some(IdempotencyRecord {
                idempotency_key: idempotency_key.clone(),
                workspace_id: context.workspace_id,
                request_hash: format!("legacy-adoption-rebuild:{}", member.memory_revision_id),
                response_payload: None,
                status: "completed".to_owned(),
                created_at: at,
                expires_at: at + chrono::Duration::hours(24),
            }),
            outbox: vec![OutboxMessage::new(
                context.workspace_id,
                "embedding.job.rebuild_accepted",
                serde_json::json!({
                    "job_id": job_id.as_uuid(),
                    "legacy_embedding_id": member.legacy_embedding_id,
                }),
                at,
            )],
            audit: AuditEvent::new(
                AuditEventId::new(),
                context.workspace_id,
                context.principal_id,
                "embedding.job.rebuild_accepted",
                "embedding_job",
                job_id.as_uuid(),
                serde_json::json!({
                    "legacy_embedding_id": member.legacy_embedding_id,
                    "memory_revision_id": member.memory_revision_id,
                }),
                at,
            )
            .map_err(ApplicationError::Domain)?,
        };
        self.jobs
            .accept_governed(context.clone(), acceptance)
            .await?;

        let creation = CreateModelRequestEvidence::for_embedding_job(
            evidence_id.as_uuid(),
            context.workspace_id,
            // The intent allocated the effect identity; read it back from the
            // acceptance rather than minting a second one.
            job_external_effect(context, &self.store, job_id).await?,
            binding.snapshot_id,
            binding.connection_revision_id,
            binding.connection_qualification_revision_id,
            binding.model_revision_id,
            binding.model_qualification_revision_id,
            job_id.as_uuid(),
            &[source.material_id],
            self.limits,
        )?;
        let mut unit = self.transactions.begin(context).await?;
        self.evidence.create_in(unit.as_mut(), &creation).await?;
        unit.commit().await?;
        Ok(job_id)
    }
}

/// The effect the accepted job actually owns.
async fn job_external_effect(
    context: &RequestContext,
    store: &PgStore,
    job_id: EmbeddingJobId,
) -> Result<Uuid, ApplicationError> {
    let mut transaction = store
        .begin_scoped(context)
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
    let effect: Uuid = sqlx::query_scalar(
        "SELECT external_effect_id FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(job_id.as_uuid())
    .fetch_one(transaction.connection())
    .await
    .map_err(map_adoption_error)?;
    transaction
        .commit()
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
    Ok(effect)
}
