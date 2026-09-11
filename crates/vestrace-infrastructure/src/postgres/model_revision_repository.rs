//! Governed publication of stable Models, immutable revisions, and workspace defaults.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use vestrace_application::{
    ApplicationError, CreateModelRevision, GovernedModelProjection, GovernedMutation,
    GovernedMutationApply, GovernedMutationReceipt, GovernedMutationRepository,
    GovernedProviderProjection, ModelRevisionRepository, RequestContext, SetWorkspaceModelDefault,
    UnitOfWork,
};
use vestrace_domain::{
    ModelKind, ModelObservationSource, ModelRevisionId, embedding::EmbeddingReadinessReason,
    id::ModelId,
};

use super::{PgGovernedMutationRepository, PgScopedTransaction, PgStore};

/// PostgreSQL implementation of the one governed Model mutation path.
pub struct PgModelRevisionRepository {
    store: PgStore,
    governed_mutations: PgGovernedMutationRepository,
}

impl PgModelRevisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            governed_mutations: PgGovernedMutationRepository::new(store.clone()),
            store,
        }
    }
}

#[async_trait]
impl ModelRevisionRepository for PgModelRevisionRepository {
    async fn create_governed(
        &self,
        context: RequestContext,
        mut command: CreateModelRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        validate_revision_command(&context, &command)?;
        let audit = command.audit.clone();
        let idempotency = command.idempotency.take();
        let outbox = std::mem::take(&mut command.outbox);
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: ModelRevisionMutation { command },
            })
            .await
    }

    async fn set_workspace_default_governed(
        &self,
        context: RequestContext,
        mut command: SetWorkspaceModelDefault,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        validate_default_command(&context, &command)?;
        let audit = command.audit.clone();
        let idempotency = command.idempotency.take();
        let outbox = std::mem::take(&mut command.outbox);
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: WorkspaceDefaultMutation { command },
            })
            .await
    }

    async fn list_safe_models(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<GovernedModelProjection>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            r#"
            SELECT m.id,
                   model_head.current_revision_id AS model_revision_id,
                   model_revision.connection_revision_id,
                   connection_head.current_revision_id AS current_connection_revision_id,
                   connection_head.state AS connection_state,
                   connection_qualification_head.current_qualification_revision_id
                     AS current_connection_qualification_revision_id,
                   connection_qualification.valid_until AS connection_qualification_valid_until,
                   model_qualification.connection_qualification_revision_id
                     AS model_qualification_connection_qualification_revision_id,
                   model_qualification.valid_until AS model_qualification_valid_until,
                   model_revision.kind AS model_kind,
                   model_qualification_head.active_space_registration_id,
                   -- A legacy registration is keyed by the wire model name, not
                   -- by a revision: migration 0197 requires
                   -- `model_revision_id IS NULL` on a legacy row, so there is no
                   -- other link from a model to the corpus it has not yet
                   -- adopted.
                   EXISTS(
                     SELECT 1 FROM embedding_space_registrations AS legacy
                      WHERE legacy.workspace_id=m.workspace_id
                        AND legacy.registration_kind='legacy_upgrade'
                        AND legacy.model=model_revision.wire_model_id
                   ) AS legacy_registration_exists,
                   EXISTS(
                     SELECT 1 FROM embedding_index_generation_guards AS guard
                       JOIN embedding_corpus_generations AS generation
                         ON generation.workspace_id=guard.workspace_id
                        AND generation.id=guard.current_generation_id
                      WHERE guard.workspace_id=m.workspace_id
                        AND guard.space_registration_id
                            =model_qualification_head.active_space_registration_id
                        AND generation.state='ready'
                   ) AS has_ready_generation,
                   EXISTS(
                     SELECT 1 FROM embedding_transitions AS transition
                      WHERE transition.workspace_id=m.workspace_id
                        AND transition.target_space_registration_id
                            =model_qualification_head.active_space_registration_id
                        AND transition.state
                            IN ('planned','rebuilding','ready_to_activate')
                   ) AS transition_pending,
                   -- A source whose erasure is prepared but whose projections
                   -- are still drawable: the corpus would answer from material
                   -- that is on its way out.
                   EXISTS(
                     SELECT 1 FROM embedding_projection_entries AS projection
                       JOIN embedding_projection_source_dependencies AS dependency
                         ON dependency.workspace_id=projection.workspace_id
                        AND dependency.projection_id=projection.id
                       JOIN content_materials AS material
                         ON material.workspace_id=dependency.workspace_id
                        AND material.id=dependency.source_material_id
                      WHERE projection.workspace_id=m.workspace_id
                        AND projection.space_registration_id
                            =model_qualification_head.active_space_registration_id
                        AND projection.state='live'
                        AND material.state='erasure_prepared'
                   ) AS erasure_pending,
                   -- A failed build with nothing succeeding after it. Compared
                   -- on `terminal_at` rather than on existence, because a
                   -- generation that failed once and was rebuilt is ready, and
                   -- reporting it as failed would send an operator to fix
                   -- something already fixed.
                   EXISTS(
                     SELECT 1 FROM embedding_index_generation_guards AS guard
                       JOIN embedding_index_build_attempts AS failed
                         ON failed.workspace_id=guard.workspace_id
                        AND failed.generation_id=guard.current_generation_id
                        AND failed.state='failed'
                      WHERE guard.workspace_id=m.workspace_id
                        AND guard.space_registration_id
                            =model_qualification_head.active_space_registration_id
                        AND NOT EXISTS(
                          SELECT 1 FROM embedding_index_build_attempts AS later
                           WHERE later.workspace_id=failed.workspace_id
                             AND later.generation_id=failed.generation_id
                             AND later.state IN ('published','loaded')
                             AND later.terminal_at>failed.terminal_at
                        )
                   ) AS index_build_failed
              FROM models AS m
              LEFT JOIN model_revision_heads AS model_head
                ON model_head.workspace_id=m.workspace_id
               AND model_head.model_id=m.id
              LEFT JOIN model_revisions AS model_revision
                ON model_revision.workspace_id=m.workspace_id
               AND model_revision.id=model_head.current_revision_id
              LEFT JOIN connection_revisions AS connection_revision
                ON connection_revision.workspace_id=m.workspace_id
               AND connection_revision.id=model_revision.connection_revision_id
              LEFT JOIN connection_revision_heads AS connection_head
                ON connection_head.workspace_id=m.workspace_id
               AND connection_head.connection_id=connection_revision.connection_id
              LEFT JOIN connection_qualification_heads AS connection_qualification_head
                ON connection_qualification_head.workspace_id=m.workspace_id
               AND connection_qualification_head.connection_revision_id=model_revision.connection_revision_id
              LEFT JOIN connection_qualification_revisions AS connection_qualification
                ON connection_qualification.workspace_id=m.workspace_id
               AND connection_qualification.id=connection_qualification_head.current_qualification_revision_id
              LEFT JOIN model_qualification_heads AS model_qualification_head
                ON model_qualification_head.workspace_id=m.workspace_id
               AND model_qualification_head.model_revision_id=model_revision.id
              LEFT JOIN model_qualification_revisions AS model_qualification
                ON model_qualification.workspace_id=m.workspace_id
               AND model_qualification.id=model_qualification_head.current_qualification_revision_id
             WHERE m.workspace_id=$1
             ORDER BY m.created_at, m.id
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;

        let now = Utc::now();
        rows.into_iter()
            .map(|row| decode_safe_model_projection(row, now))
            .collect()
    }

    async fn list_safe_providers(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<GovernedProviderProjection>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT provider.id
              FROM providers AS provider
              JOIN models AS model
                ON model.workspace_id=provider.workspace_id
               AND model.provider_id=provider.id
              JOIN model_revision_heads AS model_head
                ON model_head.workspace_id=model.workspace_id
               AND model_head.model_id=model.id
              JOIN model_revisions AS model_revision
                ON model_revision.workspace_id=model.workspace_id
               AND model_revision.id=model_head.current_revision_id
              JOIN connection_revisions AS connection_revision
                ON connection_revision.workspace_id=model.workspace_id
               AND connection_revision.id=model_revision.connection_revision_id
              JOIN connection_revision_heads AS connection_head
                ON connection_head.workspace_id=model.workspace_id
               AND connection_head.connection_id=connection_revision.connection_id
               AND connection_head.current_revision_id=model_revision.connection_revision_id
               AND connection_head.state='enabled'
              JOIN connection_qualification_heads AS connection_qualification_head
                ON connection_qualification_head.workspace_id=model.workspace_id
               AND connection_qualification_head.connection_revision_id=model_revision.connection_revision_id
              JOIN connection_qualification_revisions AS connection_qualification
                ON connection_qualification.workspace_id=model.workspace_id
               AND connection_qualification.id=connection_qualification_head.current_qualification_revision_id
               AND connection_qualification.valid_until > NOW()
              JOIN model_qualification_heads AS model_qualification_head
                ON model_qualification_head.workspace_id=model.workspace_id
               AND model_qualification_head.model_revision_id=model_revision.id
              JOIN model_qualification_revisions AS model_qualification
                ON model_qualification.workspace_id=model.workspace_id
               AND model_qualification.id=model_qualification_head.current_qualification_revision_id
               AND model_qualification.connection_qualification_revision_id=connection_qualification.id
               AND model_qualification.valid_until > NOW()
             WHERE provider.workspace_id=$1
             ORDER BY provider.id
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(GovernedProviderProjection {
                    id: vestrace_domain::ProviderId::from_uuid(
                        row.try_get("id").map_err(storage_error)?,
                    ),
                    state: "qualified".to_owned(),
                    blockers: Vec::new(),
                })
            })
            .collect()
    }
}

fn decode_safe_model_projection(
    row: sqlx::postgres::PgRow,
    now: DateTime<Utc>,
) -> Result<GovernedModelProjection, ApplicationError> {
    let id = ModelId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let revision_id: Option<uuid::Uuid> =
        row.try_get("model_revision_id").map_err(storage_error)?;
    let connection_revision_id: Option<uuid::Uuid> = row
        .try_get("connection_revision_id")
        .map_err(storage_error)?;
    let current_connection_revision_id: Option<uuid::Uuid> = row
        .try_get("current_connection_revision_id")
        .map_err(storage_error)?;
    let connection_state: Option<String> =
        row.try_get("connection_state").map_err(storage_error)?;
    let current_connection_qualification_revision_id: Option<uuid::Uuid> = row
        .try_get("current_connection_qualification_revision_id")
        .map_err(storage_error)?;
    let connection_qualification_valid_until: Option<DateTime<Utc>> = row
        .try_get("connection_qualification_valid_until")
        .map_err(storage_error)?;
    let model_qualification_connection_qualification_revision_id: Option<uuid::Uuid> = row
        .try_get("model_qualification_connection_qualification_revision_id")
        .map_err(storage_error)?;
    let model_qualification_valid_until: Option<DateTime<Utc>> = row
        .try_get("model_qualification_valid_until")
        .map_err(storage_error)?;

    let mut blockers = Vec::new();
    if revision_id.is_none() {
        blockers.push("missing_governed_model_revision".to_owned());
    } else if current_connection_revision_id != connection_revision_id {
        blockers.push("connection_revision_not_current".to_owned());
    } else if connection_state.as_deref() != Some("enabled") {
        blockers.push("connection_not_enabled".to_owned());
    }
    let qualification_is_expired = connection_qualification_valid_until
        .is_some_and(|until| until <= now)
        || model_qualification_valid_until.is_some_and(|until| until <= now);
    let qualification_is_current = current_connection_qualification_revision_id.is_some()
        && current_connection_qualification_revision_id
            == model_qualification_connection_qualification_revision_id
        && connection_qualification_valid_until.is_some_and(|until| until > now)
        && model_qualification_valid_until.is_some_and(|until| until > now);
    if revision_id.is_some() && !qualification_is_current {
        blockers.push(
            if qualification_is_expired {
                "qualification_expired"
            } else {
                "qualification_required"
            }
            .to_owned(),
        );
    }
    // Embedding readiness, in the vocabulary Doctor and this projection share.
    //
    // Only for an embedding model: a chat model has no corpus, and reporting
    // one of these against it would send an operator looking for a space that
    // was never meant to exist. The reasons are appended rather than replacing
    // the qualification blockers, because a model can be both unqualified and
    // unable to serve retrieval, and an operator fixing one would otherwise be
    // told the other had gone.
    //
    // `embedding-index-not-loaded` is absent and must stay absent: this is a
    // database read, and `is_observable_from_storage` says what a database read
    // may claim. Whether a process holds an index is knowable only to that
    // process.
    let model_kind: Option<String> = row.try_get("model_kind").map_err(storage_error)?;
    if model_kind.as_deref() == Some("embedding") {
        let active_space: Option<uuid::Uuid> = row
            .try_get("active_space_registration_id")
            .map_err(storage_error)?;
        let mut embedding_blocker = |reason: EmbeddingReadinessReason| {
            debug_assert!(
                reason.is_observable_from_storage(),
                "a storage projection may not assert {reason}"
            );
            blockers.push(reason.as_str().to_owned());
        };
        if active_space.is_none() {
            // No canonical space is active. That is only a blocker when there
            // is a legacy corpus waiting to be adopted; a model that has simply
            // never been used for embedding is not broken.
            if row
                .try_get("legacy_registration_exists")
                .map_err(storage_error)?
            {
                embedding_blocker(EmbeddingReadinessReason::LegacyAdoptionRequired);
            }
        } else {
            if row.try_get("transition_pending").map_err(storage_error)? {
                embedding_blocker(EmbeddingReadinessReason::QualificationTransitionNotReady);
            }
            if !row
                .try_get::<bool, _>("has_ready_generation")
                .map_err(storage_error)?
            {
                embedding_blocker(EmbeddingReadinessReason::GenerationNotReady);
            }
            if row.try_get("erasure_pending").map_err(storage_error)? {
                embedding_blocker(EmbeddingReadinessReason::ErasurePending);
            }
            if row.try_get("index_build_failed").map_err(storage_error)? {
                embedding_blocker(EmbeddingReadinessReason::IndexBuildFailed);
            }
        }
    }

    let state = if revision_id.is_none() {
        "legacy"
    } else if blockers.is_empty() {
        "qualified"
    } else {
        "blocked"
    };
    let qualification_state = if qualification_is_current {
        "qualified"
    } else if qualification_is_expired {
        "expired"
    } else {
        "missing"
    };
    Ok(GovernedModelProjection {
        id,
        revision_id: revision_id.map(ModelRevisionId::from_uuid),
        state: state.to_owned(),
        qualification_state: qualification_state.to_owned(),
        blockers,
    })
}

struct ModelRevisionMutation {
    command: CreateModelRevision,
}

#[async_trait]
impl GovernedMutationApply for ModelRevisionMutation {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        let transaction = transaction(unit_of_work)?;
        insert_or_verify_stable_model(transaction, &self.command).await?;
        let revision = &self.command.revision;
        let result = sqlx::query_scalar(
            "SELECT vestrace_create_model_revision_and_advance_head(\
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(revision.id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(self.command.model.id.as_uuid())
        .bind(self.command.connection_id.as_uuid())
        .bind(self.command.execution_guard_id)
        .bind(revision.connection_revision_id().as_uuid())
        .bind(revision.wire_model_id())
        .bind(model_kind(revision.kind()))
        .bind(
            revision
                .observed_context_window()
                .value()
                .copied()
                .map(|value| value as i32),
        )
        .bind(
            revision
                .observed_context_window()
                .qualification_revision_id()
                .map(|id| id.as_uuid()),
        )
        .bind(observation_source(
            revision.observed_context_window().source(),
        ))
        .bind(
            revision
                .observed_embedding_dimension()
                .value()
                .copied()
                .map(|value| value as i32),
        )
        .bind(
            revision
                .observed_embedding_dimension()
                .qualification_revision_id()
                .map(|id| id.as_uuid()),
        )
        .bind(observation_source(
            revision.observed_embedding_dimension().source(),
        ))
        .bind(self.command.expected_head_version as i64)
        .fetch_one(transaction.connection())
        .await;
        map_conflict(result, "MODEL_VERSION_CONFLICT")
    }
}

struct WorkspaceDefaultMutation {
    command: SetWorkspaceModelDefault,
}

#[async_trait]
impl GovernedMutationApply for WorkspaceDefaultMutation {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        let transaction = transaction(unit_of_work)?;
        let result = sqlx::query_scalar(
            "SELECT vestrace_set_workspace_model_default($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.command.default_id)
        .bind(context.workspace_id.as_uuid())
        .bind(&self.command.purpose)
        .bind(self.command.model_id.as_uuid())
        .bind(&self.command.required_capabilities)
        .bind(self.command.expected_version as i64)
        .fetch_one(transaction.connection())
        .await;
        map_conflict(result, "WORKSPACE_MODEL_DEFAULT_VERSION_CONFLICT")
    }
}

async fn insert_or_verify_stable_model(
    transaction: &mut PgScopedTransaction,
    command: &CreateModelRevision,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO models \
         (id, provider_id, workspace_id, model_name, context_window, \
          input_cost_per_mtoken, output_cost_per_mtoken, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(command.model.id.as_uuid())
    .bind(command.model.provider_id.as_uuid())
    .bind(command.model.workspace_id.as_uuid())
    .bind(&command.model.model_name)
    .bind(command.model.context_window as i32)
    .bind(command.model.input_cost_per_mtoken)
    .bind(command.model.output_cost_per_mtoken)
    .bind(command.model.created_at)
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;

    let matches: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM models WHERE id = $1 AND workspace_id = $2 \
         AND provider_id = $3 AND model_name = $4 AND context_window = $5 \
         AND input_cost_per_mtoken = $6 AND output_cost_per_mtoken = $7)",
    )
    .bind(command.model.id.as_uuid())
    .bind(command.model.workspace_id.as_uuid())
    .bind(command.model.provider_id.as_uuid())
    .bind(&command.model.model_name)
    .bind(command.model.context_window as i32)
    .bind(command.model.input_cost_per_mtoken)
    .bind(command.model.output_cost_per_mtoken)
    .fetch_one(transaction.connection())
    .await
    .map_err(storage_error)?;
    if !matches {
        return Err(ApplicationError::Policy(
            "a stable model identity cannot be rewritten by a revision".to_owned(),
        ));
    }
    Ok(())
}

fn validate_revision_command(
    context: &RequestContext,
    command: &CreateModelRevision,
) -> Result<(), ApplicationError> {
    if command.model.workspace_id != context.workspace_id
        || command.revision.workspace_id() != context.workspace_id
        || command.audit.workspace_id != context.workspace_id
        || command.audit.principal_id != context.principal_id
    {
        return Err(ApplicationError::Policy(
            "model revision identity must match its request context".to_owned(),
        ));
    }
    validate_evidence_workspace(context, command.idempotency.as_ref(), &command.outbox)
}

fn validate_default_command(
    context: &RequestContext,
    command: &SetWorkspaceModelDefault,
) -> Result<(), ApplicationError> {
    if command.workspace_id != context.workspace_id
        || command.audit.workspace_id != context.workspace_id
        || command.audit.principal_id != context.principal_id
    {
        return Err(ApplicationError::Policy(
            "workspace default identity must match its request context".to_owned(),
        ));
    }
    validate_evidence_workspace(context, command.idempotency.as_ref(), &command.outbox)
}

fn validate_evidence_workspace(
    context: &RequestContext,
    idempotency: Option<&vestrace_application::IdempotencyRecord>,
    outbox: &[vestrace_application::OutboxMessage],
) -> Result<(), ApplicationError> {
    if idempotency.is_some_and(|record| record.workspace_id != context.workspace_id)
        || outbox
            .iter()
            .any(|message| message.workspace_id != context.workspace_id)
    {
        return Err(ApplicationError::Policy(
            "model mutation evidence must stay in its request workspace".to_owned(),
        ));
    }
    Ok(())
}

fn transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))
}

fn map_conflict(
    result: Result<uuid::Uuid, sqlx::Error>,
    conflict_code: &str,
) -> Result<(), ApplicationError> {
    match result {
        Ok(_) => Ok(()),
        Err(error) if sqlstate(&error).as_deref() == Some("40001") => {
            Err(ApplicationError::Conflict(conflict_code.to_owned()))
        }
        Err(error) => Err(storage_error(error)),
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn model_kind(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Chat => "chat",
        ModelKind::Embedding => "embedding",
    }
}

fn observation_source(source: Option<ModelObservationSource>) -> Option<&'static str> {
    source.map(|source| match source {
        ModelObservationSource::Provider => "provider",
        ModelObservationSource::Operator => "operator",
    })
}
