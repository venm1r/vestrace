//! PostgreSQL authority for immutable embedding-transition planning.

use async_trait::async_trait;
use vestrace_application::{
    AcknowledgeCarriedTransitionBatchAfterUnknown, ApplicationError, EmbeddingTransitionRepository,
    PlanEmbeddingTransitionVersion, RequestContext, TransitionAuthBinding,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgEmbeddingTransitionRepository {
    store: PgStore,
}

impl PgEmbeddingTransitionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EmbeddingTransitionRepository for PgEmbeddingTransitionRepository {
    async fn plan_version(
        &self,
        context: RequestContext,
        command: PlanEmbeddingTransitionVersion,
    ) -> Result<uuid::Uuid, ApplicationError> {
        if command
            .recipes
            .iter()
            .enumerate()
            .any(|(index, recipe)| recipe.ordinal.value() != index as u32)
        {
            return Err(ApplicationError::Policy(
                "EMBEDDING_TRANSITION_RECIPE_ORDINALS_MALFORMED".to_owned(),
            ));
        }
        let (source_branch, source_credential, source_no_auth) = match command.source_branch {
            TransitionAuthBinding::Credential {
                credential_revision_id,
                ..
            } => ("credential", Some(credential_revision_id), None),
            TransitionAuthBinding::NoAuth {
                binding_revision_id,
            } => ("no_auth", None, Some(binding_revision_id)),
        };
        let (
            target_branch,
            target_credential,
            target_slot,
            target_guard,
            target_version,
            target_no_auth,
        ) = match command.target_branch {
            TransitionAuthBinding::Credential {
                credential_revision_id,
                credential_slot_id,
                credential_activation_guard_id,
                expected_slot_version,
            } => (
                "credential",
                Some(credential_revision_id),
                credential_slot_id,
                credential_activation_guard_id,
                expected_slot_version.map(|value| value as i64),
                None,
            ),
            TransitionAuthBinding::NoAuth {
                binding_revision_id,
            } => ("no_auth", None, None, None, None, Some(binding_revision_id)),
        };
        let recipe_identities: Vec<_> = command
            .recipes
            .iter()
            .map(|recipe| recipe.identity)
            .collect();
        let recipe_inputs = serde_json::Value::Array(
            command
                .recipes
                .iter()
                .map(|recipe| {
                    serde_json::Value::Array(
                        recipe
                            .input_ordinals
                            .iter()
                            .map(|ordinal| serde_json::Value::from(ordinal.value()))
                            .collect(),
                    )
                })
                .collect(),
        );
        let mut transaction = self
            .store
            .begin_scoped(&context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let planned = sqlx::query_scalar(
            "SELECT vestrace_plan_embedding_transition_version(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)",
        )
        .bind(command.transition_id)
        .bind(command.plan_id)
        .bind(context.workspace_id.as_uuid())
        .bind(command.version.value() as i64)
        .bind(command.source_connection_id)
        .bind(command.source_connection_revision_id)
        .bind(source_branch)
        .bind(source_credential)
        .bind(source_no_auth)
        .bind(command.target_connection_id)
        .bind(command.target_connection_revision_id)
        .bind(command.target_connection_qualification_revision_id)
        .bind(command.target_model_revision_id)
        .bind(command.target_model_qualification_revision_id)
        .bind(target_branch)
        .bind(target_credential)
        .bind(target_slot)
        .bind(target_guard)
        .bind(target_version)
        .bind(target_no_auth)
        .bind(command.target_space_registration_id)
        .bind(command.batch_id.as_uuid())
        .bind(command.snapshot_id)
        .bind(recipe_identities)
        .bind(sqlx::types::Json(recipe_inputs))
        .fetch_one(transaction.connection())
        .await
        .map_err(map_planning_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(planned)
    }

    async fn acknowledge_carried_batch_after_unknown(
        &self,
        context: RequestContext,
        command: AcknowledgeCarriedTransitionBatchAfterUnknown,
    ) -> Result<uuid::Uuid, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(&context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let accepted = sqlx::query_scalar(
            "SELECT vestrace_acknowledge_carried_transition_batch_after_unknown(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(command.transition_id)
        .bind(command.carry_id)
        .bind(command.predecessor_embedding_job_id)
        .bind(command.predecessor_version.value() as i64)
        .bind(command.successor_embedding_job_id)
        .bind(command.space_registration_id)
        .bind(command.kind)
        .bind(command.model_binding_snapshot_id)
        .bind(command.external_effect_id)
        .bind(command.model_request_evidence_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_planning_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(accepted)
    }
}

fn map_planning_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("40001") => {
            ApplicationError::Conflict("EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT".to_owned())
        }
        Some("22023") | Some("23514") => {
            ApplicationError::Policy("EMBEDDING_TRANSITION_PLANNING_REFUSED".to_owned())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
