//! PostgreSQL resolver for the atomic legacy Run model-binding snapshot.

use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, LEGACY_RUN_MODEL_DEFAULT_PURPOSE, ModelBindingResolver, RequestContext,
    UnitOfWork,
};
use vestrace_domain::{
    ConnectionQualificationRevisionId, ConnectionRevisionId, CredentialActivationGuardId,
    CredentialRevisionId, CredentialSlotId, ModelBindingSnapshot, ModelBindingSnapshotId,
    ModelQualificationRevisionId, ModelRevisionId, NoAuthBindingRevisionId,
    QualificationTargetBinding,
    id::{AgentRunId, WorkspaceId},
};

use super::PgScopedTransaction;

#[derive(Clone, Debug, Default)]
pub struct PgModelBindingRepository;

#[derive(FromRow)]
struct SnapshotRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    connection_revision_id: uuid::Uuid,
    connection_qualification_revision_id: uuid::Uuid,
    model_revision_id: uuid::Uuid,
    model_qualification_revision_id: uuid::Uuid,
    branch: String,
    credential_revision_id: Option<uuid::Uuid>,
    credential_slot_id: Option<uuid::Uuid>,
    credential_activation_guard_id: Option<uuid::Uuid>,
    expected_slot_version: Option<i64>,
    no_auth_binding_revision_id: Option<uuid::Uuid>,
}

#[async_trait]
impl ModelBindingResolver for PgModelBindingRepository {
    async fn resolve_for_run_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        run_id: AgentRunId,
    ) -> Result<ModelBindingSnapshot, ApplicationError> {
        let transaction = unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
            })?;
        let snapshot_id: uuid::Uuid =
            sqlx::query_scalar("SELECT vestrace_create_run_model_binding_snapshot($1, $2, $3, $4)")
                .bind(uuid::Uuid::now_v7())
                .bind(context.workspace_id.as_uuid())
                .bind(run_id.as_uuid())
                .bind(LEGACY_RUN_MODEL_DEFAULT_PURPOSE)
                .fetch_one(transaction.connection())
                .await
                .map_err(map_resolution_error)?;

        let row = sqlx::query_as::<_, SnapshotRow>(
            "SELECT id, workspace_id, connection_revision_id,
                    connection_qualification_revision_id, model_revision_id,
                    model_qualification_revision_id, branch, credential_revision_id,
                    credential_slot_id, credential_activation_guard_id,
                    expected_slot_version, no_auth_binding_revision_id
               FROM model_binding_snapshots
              WHERE workspace_id = $1 AND id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(snapshot_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        snapshot_from_row(row)
    }
}

fn snapshot_from_row(row: SnapshotRow) -> Result<ModelBindingSnapshot, ApplicationError> {
    let qualification_target = match row.branch.as_str() {
        "credential" => QualificationTargetBinding::Credential {
            revision_id: CredentialRevisionId::from_uuid(row.credential_revision_id.ok_or_else(
                || {
                    ApplicationError::Storage(
                        "credential binding snapshot is incomplete".to_owned(),
                    )
                },
            )?),
            slot_id: CredentialSlotId::from_uuid(row.credential_slot_id.ok_or_else(|| {
                ApplicationError::Storage("credential binding snapshot is incomplete".to_owned())
            })?),
            activation_guard_id: CredentialActivationGuardId::from_uuid(
                row.credential_activation_guard_id.ok_or_else(|| {
                    ApplicationError::Storage(
                        "credential binding snapshot is incomplete".to_owned(),
                    )
                })?,
            ),
            expected_slot_version: u64::try_from(row.expected_slot_version.ok_or_else(|| {
                ApplicationError::Storage("credential binding snapshot is incomplete".to_owned())
            })?)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        },
        "no_auth" => QualificationTargetBinding::NoAuth {
            binding_revision_id: NoAuthBindingRevisionId::from_uuid(
                row.no_auth_binding_revision_id.ok_or_else(|| {
                    ApplicationError::Storage("no-auth binding snapshot is incomplete".to_owned())
                })?,
            ),
        },
        _ => {
            return Err(ApplicationError::Storage(
                "binding snapshot has an unknown branch".to_owned(),
            ));
        }
    };
    ModelBindingSnapshot::from_persisted(
        ModelBindingSnapshotId::from_uuid(row.id),
        WorkspaceId::from_uuid(row.workspace_id),
        ConnectionRevisionId::from_uuid(row.connection_revision_id),
        ConnectionQualificationRevisionId::from_uuid(row.connection_qualification_revision_id),
        ModelRevisionId::from_uuid(row.model_revision_id),
        ModelQualificationRevisionId::from_uuid(row.model_qualification_revision_id),
        qualification_target,
    )
    .map_err(ApplicationError::from)
}

fn map_resolution_error(error: sqlx::Error) -> ApplicationError {
    let Some(database_error) = error.as_database_error() else {
        return storage_error(error);
    };
    match database_error.code().as_deref() {
        Some("22023") => ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(
            "MODEL_BINDING_MALFORMED_ARGUMENT".to_owned(),
        )),
        Some("40001") => ApplicationError::Conflict("MODEL_BINDING_VERSION_CONFLICT".to_owned()),
        Some("23514") => match database_error.constraint() {
            Some("legacy_run_chat_default_absent") => {
                ApplicationError::Policy("MODEL_BINDING_LEGACY_CHAT_DEFAULT_ABSENT".to_owned())
            }
            Some("legacy_run_chat_default_not_current_chat") => ApplicationError::Policy(
                "MODEL_BINDING_LEGACY_CHAT_DEFAULT_NOT_CURRENT_CHAT".to_owned(),
            ),
            Some("model_binding_qualification_expired") => {
                ApplicationError::Policy("MODEL_BINDING_QUALIFICATION_EXPIRED".to_owned())
            }
            Some("model_binding_qualification_incompatible") => {
                ApplicationError::Policy("MODEL_BINDING_QUALIFICATION_INCOMPATIBLE".to_owned())
            }
            _ => ApplicationError::Policy("MODEL_BINDING_PREDICATE_UNSATISFIED".to_owned()),
        },
        Some("42501") => ApplicationError::Policy("MODEL_BINDING_RAW_MUTATION_REFUSED".to_owned()),
        _ => storage_error(error),
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
