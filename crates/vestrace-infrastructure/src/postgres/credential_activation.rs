//! One governed PostgreSQL path for credential activation, rotation and revocation.

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, AuditRepository, CandidateCredentialAbandonCommand,
    CredentialActivationCommand, CredentialActivationError, CredentialActivationRepository,
    CredentialRevocationCommand, CredentialRotationCommand, GovernedMutation,
    GovernedMutationApply, GovernedMutationReceipt, GovernedMutationRepository,
    IdempotencyRepository, InstallationMutationPermit, MaterialErasurePreparation,
    OutboxRepository, PermitMode, RequestContext, UnitOfWork,
};
use vestrace_domain::{DomainError, ErasureReceipt, MaterialKeyId};

use super::{
    PgAuditRepository, PgGovernedMutationRepository, PgIdempotencyRepository,
    PgInstallationMutationPermit, PgOutboxRepository, PgScopedTransaction, PgStore,
};

/// PostgreSQL implementation of the sole credential lifecycle publication boundary.
pub struct PgCredentialActivationRepository {
    store: PgStore,
    governed_mutations: PgGovernedMutationRepository,
}

impl PgCredentialActivationRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            governed_mutations: PgGovernedMutationRepository::new(store.clone()),
            store,
        }
    }

    async fn prepare_candidate_abandon(
        &self,
        context: RequestContext,
        command: CandidateCredentialAbandonCommand,
    ) -> Result<MaterialErasurePreparation, CredentialActivationError> {
        validate_candidate_abandon_command(&context, &command)?;

        let permit_authority = PgInstallationMutationPermit::new(self.store.clone());
        let mut permit = permit_authority
            .acquire(PermitMode::Shared, &context)
            .await
            .map_err(CredentialActivationError::from)?;
        let existing_hash = lock_and_find_candidate_idempotency(
            permit.unit_of_work_mut(),
            &context,
            &command.idempotency,
        )
        .await
        .map_err(CredentialActivationError::from)?;

        if let Some(existing_hash) = existing_hash {
            if existing_hash != command.idempotency.request_hash {
                return Err(CredentialActivationError::Application(
                    ApplicationError::Conflict("IDEMPOTENCY_KEY_REUSE_CONFLICT".to_owned()),
                ));
            }
            let preparation =
                prepare_candidate_abandon_in(permit.unit_of_work_mut(), &context, &command)
                    .await
                    .map_err(CredentialActivationError::from)?;
            permit
                .commit()
                .await
                .map_err(CredentialActivationError::from)?;
            return Ok(preparation);
        }

        let preparation =
            prepare_candidate_abandon_in(permit.unit_of_work_mut(), &context, &command)
                .await
                .map_err(CredentialActivationError::from)?;
        PgAuditRepository::new(self.store.clone())
            .record_in(&context, permit.unit_of_work_mut(), &command.audit)
            .await
            .map_err(CredentialActivationError::from)?;
        PgIdempotencyRepository::new(self.store.clone())
            .save_in(&context, permit.unit_of_work_mut(), &command.idempotency)
            .await
            .map_err(CredentialActivationError::from)?;
        let outbox = PgOutboxRepository::new(self.store.clone());
        for message in &command.outbox {
            outbox
                .save_in(&context, permit.unit_of_work_mut(), message)
                .await
                .map_err(CredentialActivationError::from)?;
        }
        record_governed_mutation_mark(permit.unit_of_work_mut(), &context, &command.audit)
            .await
            .map_err(CredentialActivationError::from)?;
        permit
            .commit()
            .await
            .map_err(CredentialActivationError::from)?;
        Ok(preparation)
    }

    async fn commit(
        &self,
        context: RequestContext,
        command: CredentialMutationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError> {
        validate_command(&context, &command)?;
        let (audit, idempotency, outbox) = command.evidence();
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: CredentialActivationMutation { command },
            })
            .await
            .map_err(CredentialActivationError::from)
    }
}

#[async_trait]
impl CredentialActivationRepository for PgCredentialActivationRepository {
    async fn prepare_candidate_abandon(
        &self,
        context: RequestContext,
        command: CandidateCredentialAbandonCommand,
    ) -> Result<MaterialErasurePreparation, CredentialActivationError> {
        self.prepare_candidate_abandon(context, command).await
    }

    async fn activate_first(
        &self,
        context: RequestContext,
        command: CredentialActivationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError> {
        self.commit(context, CredentialMutationCommand::ActivateFirst(command))
            .await
    }

    async fn rotate(
        &self,
        context: RequestContext,
        command: CredentialRotationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError> {
        self.commit(context, CredentialMutationCommand::Rotate(command))
            .await
    }

    async fn revoke(
        &self,
        context: RequestContext,
        command: CredentialRevocationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError> {
        self.commit(context, CredentialMutationCommand::Revoke(command))
            .await
    }
}

enum CredentialMutationCommand {
    ActivateFirst(CredentialActivationCommand),
    Rotate(CredentialRotationCommand),
    Revoke(CredentialRevocationCommand),
}

impl CredentialMutationCommand {
    fn evidence(
        &self,
    ) -> (
        vestrace_domain::AuditEvent,
        Option<vestrace_application::IdempotencyRecord>,
        Vec<vestrace_application::OutboxMessage>,
    ) {
        match self {
            Self::ActivateFirst(command) => (
                command.audit.clone(),
                command.idempotency.clone(),
                command.outbox.clone(),
            ),
            Self::Rotate(command) => (
                command.audit.clone(),
                command.idempotency.clone(),
                command.outbox.clone(),
            ),
            Self::Revoke(command) => (
                command.audit.clone(),
                command.idempotency.clone(),
                command.outbox.clone(),
            ),
        }
    }

    fn workspace_matches(&self, context: &RequestContext) -> bool {
        match self {
            Self::ActivateFirst(command) => {
                command.audit.workspace_id == context.workspace_id
                    && command.audit.principal_id == context.principal_id
                    && command
                        .idempotency
                        .as_ref()
                        .is_none_or(|record| record.workspace_id == context.workspace_id)
                    && command
                        .outbox
                        .iter()
                        .all(|message| message.workspace_id == context.workspace_id)
            }
            Self::Rotate(command) => {
                command.audit.workspace_id == context.workspace_id
                    && command.audit.principal_id == context.principal_id
                    && command
                        .idempotency
                        .as_ref()
                        .is_none_or(|record| record.workspace_id == context.workspace_id)
                    && command
                        .outbox
                        .iter()
                        .all(|message| message.workspace_id == context.workspace_id)
            }
            Self::Revoke(command) => {
                command.audit.workspace_id == context.workspace_id
                    && command.audit.principal_id == context.principal_id
                    && command
                        .idempotency
                        .as_ref()
                        .is_none_or(|record| record.workspace_id == context.workspace_id)
                    && command
                        .outbox
                        .iter()
                        .all(|message| message.workspace_id == context.workspace_id)
            }
        }
    }
}

struct CredentialActivationMutation {
    command: CredentialMutationCommand,
}

#[async_trait]
impl GovernedMutationApply for CredentialActivationMutation {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        let transaction = unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
            })?;
        match &self.command {
            CredentialMutationCommand::ActivateFirst(command) => {
                activate_first(transaction, context, command).await
            }
            CredentialMutationCommand::Rotate(command) => {
                rotate(transaction, context, command).await
            }
            CredentialMutationCommand::Revoke(command) => {
                revoke(transaction, context, command).await
            }
        }
    }
}

fn validate_command(
    context: &RequestContext,
    command: &CredentialMutationCommand,
) -> Result<(), CredentialActivationError> {
    if !command.workspace_matches(context) {
        return Err(CredentialActivationError::Application(
            ApplicationError::Policy(
                "credential activation evidence must stay in its request workspace".to_owned(),
            ),
        ));
    }
    let expected_slot_version = match command {
        CredentialMutationCommand::ActivateFirst(command) => command.expected_slot_version,
        CredentialMutationCommand::Rotate(command) => command.expected_slot_version,
        CredentialMutationCommand::Revoke(command) => command.expected_slot_version,
    };
    if i64::try_from(expected_slot_version).is_err() {
        return Err(CredentialActivationError::Application(
            ApplicationError::Domain(DomainError::InvalidArgument(
                "credential slot version exceeds PostgreSQL BIGINT".to_owned(),
            )),
        ));
    }
    Ok(())
}

fn validate_candidate_abandon_command(
    context: &RequestContext,
    command: &CandidateCredentialAbandonCommand,
) -> Result<(), CredentialActivationError> {
    if command.audit.workspace_id != context.workspace_id
        || command.audit.principal_id != context.principal_id
        || command.audit.resource_id != command.credential_intent_id
        || command.idempotency.workspace_id != context.workspace_id
        || command
            .outbox
            .iter()
            .any(|message| message.workspace_id != context.workspace_id)
    {
        return Err(CredentialActivationError::Application(
            ApplicationError::Policy(
                "candidate abandon evidence must bind its exact request workspace and intent"
                    .to_owned(),
            ),
        ));
    }
    if i64::try_from(command.expected_association_version).is_err() {
        return Err(CredentialActivationError::Application(
            ApplicationError::Domain(DomainError::InvalidArgument(
                "credential association version exceeds PostgreSQL BIGINT".to_owned(),
            )),
        ));
    }
    Ok(())
}

async fn lock_and_find_candidate_idempotency(
    unit_of_work: &mut dyn UnitOfWork,
    context: &RequestContext,
    record: &vestrace_application::IdempotencyRecord,
) -> Result<Option<String>, ApplicationError> {
    let transaction = unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "candidate-abandon-idempotency:{}:{}",
            context.workspace_id, record.idempotency_key
        ))
        .execute(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
    sqlx::query_scalar(
        "SELECT request_hash FROM idempotency_keys \
         WHERE workspace_id = $1 AND idempotency_key = $2 FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(&record.idempotency_key)
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error| ApplicationError::Storage(error.to_string()))
}

async fn prepare_candidate_abandon_in(
    unit_of_work: &mut dyn UnitOfWork,
    context: &RequestContext,
    command: &CandidateCredentialAbandonCommand,
) -> Result<MaterialErasurePreparation, ApplicationError> {
    let transaction = unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))?;
    let (preparation_id, material_key_id, finalized_erasure_receipt) =
        sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, Option<uuid::Uuid>)>(
            "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
        )
        .bind(command.credential_intent_id)
        .bind(command.expected_association_version as i64)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_credential_activation_error)?;
    let _ = context;
    Ok(MaterialErasurePreparation::new(
        preparation_id,
        MaterialKeyId::from_uuid(material_key_id),
        finalized_erasure_receipt.map(ErasureReceipt::from_uuid),
    ))
}

async fn record_governed_mutation_mark(
    unit_of_work: &mut dyn UnitOfWork,
    context: &RequestContext,
    audit: &vestrace_domain::AuditEvent,
) -> Result<(), ApplicationError> {
    let transaction = unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))?;
    sqlx::query("SELECT vestrace_record_governed_mutation_audit_mark_and_advance($1, $2, $3, $4)")
        .bind(uuid::Uuid::now_v7())
        .bind(context.workspace_id.as_uuid())
        .bind(audit.id.as_uuid())
        .bind(vestrace_domain::time::now())
        .execute(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
    Ok(())
}

async fn activate_first(
    transaction: &mut PgScopedTransaction,
    context: &RequestContext,
    command: &CredentialActivationCommand,
) -> Result<(), ApplicationError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_activate_first_credential($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection_id.as_uuid())
    .bind(command.execution_guard_id)
    .bind(command.activation_guard_id)
    .bind(command.credential_slot_id.as_uuid())
    .bind(command.credential_revision_id)
    .bind(command.credential_intent_id)
    .bind(command.connection_qualification_revision_id)
    .bind(command.audit.id.as_uuid())
    .bind(command.expected_slot_version as i64)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_credential_activation_error)?;
    Ok(())
}

async fn rotate(
    transaction: &mut PgScopedTransaction,
    context: &RequestContext,
    command: &CredentialRotationCommand,
) -> Result<(), ApplicationError> {
    lock_and_refuse_embedding_dependencies(transaction, context, command).await?;
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_rotate_credential($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection_id.as_uuid())
    .bind(command.execution_guard_id)
    .bind(command.activation_guard_id)
    .bind(command.credential_slot_id.as_uuid())
    .bind(command.previous_credential_revision_id)
    .bind(command.activated_credential_revision_id)
    .bind(command.activated_credential_intent_id)
    .bind(command.connection_qualification_revision_id)
    .bind(command.audit.id.as_uuid())
    .bind(command.expected_slot_version as i64)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_credential_activation_error)?;
    Ok(())
}

async fn revoke(
    transaction: &mut PgScopedTransaction,
    context: &RequestContext,
    command: &CredentialRevocationCommand,
) -> Result<(), ApplicationError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_revoke_credential($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection_id.as_uuid())
    .bind(command.execution_guard_id)
    .bind(command.activation_guard_id)
    .bind(command.credential_slot_id.as_uuid())
    .bind(command.credential_revision_id)
    .bind(command.credential_intent_id)
    .bind(command.audit.id.as_uuid())
    .bind(command.expected_slot_version as i64)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_credential_activation_error)?;
    Ok(())
}

async fn lock_and_refuse_embedding_dependencies(
    transaction: &mut PgScopedTransaction,
    context: &RequestContext,
    command: &CredentialRotationCommand,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "SELECT vestrace_acquire_credential_lock_chain($1, $2, $3, \
         ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[])",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection_id.as_uuid())
    .bind(command.credential_slot_id.as_uuid())
    .execute(transaction.connection())
    .await
    .map_err(map_credential_activation_error)?;

    let has_embedding_dependency: bool = sqlx::query_scalar(
        "SELECT EXISTS (\
         SELECT 1 \
           FROM connection_revision_heads AS connection_head \
           JOIN model_revision_heads AS model_head \
             ON model_head.workspace_id = connection_head.workspace_id \
           JOIN model_revisions AS model_revision \
             ON model_revision.workspace_id = model_head.workspace_id \
            AND model_revision.model_id = model_head.model_id \
            AND model_revision.id = model_head.current_revision_id \
          WHERE connection_head.workspace_id = $1 \
            AND connection_head.connection_id = $2 \
            AND model_revision.connection_revision_id = connection_head.current_revision_id \
            AND model_revision.kind = 'embedding'\
         )",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection_id.as_uuid())
    .fetch_one(transaction.connection())
    .await
    .map_err(map_credential_activation_error)?;
    if has_embedding_dependency {
        return Err(ApplicationError::Conflict(
            "EMBEDDING_TRANSITION_REQUIRED".to_owned(),
        ));
    }
    Ok(())
}

fn map_credential_activation_error(error: sqlx::Error) -> ApplicationError {
    match sqlstate(&error).as_deref() {
        Some("22023") => ApplicationError::Domain(DomainError::InvalidArgument(
            "credential activation arguments are malformed".to_owned(),
        )),
        Some("40001") => ApplicationError::Conflict("CREDENTIAL_SLOT_VERSION_CONFLICT".to_owned()),
        Some("23514") => ApplicationError::Policy("CREDENTIAL_STATE_REFUSED".to_owned()),
        Some("42501") => ApplicationError::Policy("CREDENTIAL_RAW_MUTATION_REFUSED".to_owned()),
        _ => ApplicationError::Storage(error.to_string()),
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}
