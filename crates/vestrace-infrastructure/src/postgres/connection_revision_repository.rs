//! Governed creation and revision publication for stable Connections.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use vestrace_application::{
    ApplicationError, ConnectionRevisionRepository, CreateConnectionRevision,
    GovernedConnectionProjection, GovernedMutation, GovernedMutationApply, GovernedMutationReceipt,
    GovernedMutationRepository, PublishConnectionAdmissionPolicy, RequestContext, UnitOfWork,
};
use vestrace_domain::{
    ConnectionAuthMode, ConnectionKind, ConnectionRevisionId, ConnectionTransportPolicy,
    DomainError, connection::ConnectionStatus, id::ConnectionId,
};

use super::{PgGovernedMutationRepository, PgScopedTransaction, PgStore};

/// PostgreSQL implementation of the one governed Connection mutation path.
pub struct PgConnectionRevisionRepository {
    store: PgStore,
    governed_mutations: PgGovernedMutationRepository,
}

impl PgConnectionRevisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            governed_mutations: PgGovernedMutationRepository::new(store.clone()),
            store,
        }
    }

    async fn commit(
        &self,
        context: RequestContext,
        mut command: CreateConnectionRevision,
        creates_connection: bool,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        validate_command(&context, &command)?;
        let audit = command.audit.clone();
        let idempotency = command.idempotency.take();
        let outbox = std::mem::take(&mut command.outbox);
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: ConnectionRevisionMutation {
                    command,
                    creates_connection,
                },
            })
            .await
    }
}

#[async_trait]
impl ConnectionRevisionRepository for PgConnectionRevisionRepository {
    async fn create_governed(
        &self,
        context: RequestContext,
        command: CreateConnectionRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        self.commit(context, command, true).await
    }

    async fn revise_governed(
        &self,
        context: RequestContext,
        command: CreateConnectionRevision,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        self.commit(context, command, false).await
    }

    async fn publish_admission_policy_governed(
        &self,
        context: RequestContext,
        mut command: PublishConnectionAdmissionPolicy,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        if command.audit.workspace_id != context.workspace_id
            || command.audit.principal_id != context.principal_id
            || command
                .idempotency
                .as_ref()
                .is_some_and(|record| record.workspace_id != context.workspace_id)
            || command
                .outbox
                .iter()
                .any(|message| message.workspace_id != context.workspace_id)
        {
            return Err(ApplicationError::Policy(
                "connection mutation identity must match its request context".to_owned(),
            ));
        }
        let audit = command.audit.clone();
        let idempotency = command.idempotency.take();
        let outbox = std::mem::take(&mut command.outbox);
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: ConnectionAdmissionPolicyMutation { command },
            })
            .await
    }

    async fn list_safe_connections(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<GovernedConnectionProjection>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            r#"
            SELECT connection.id,
                   head.current_revision_id AS connection_revision_id,
                   head.state AS connection_state,
                   qualification.valid_until AS qualification_valid_until
              FROM connections AS connection
              LEFT JOIN connection_revision_heads AS head
                ON head.workspace_id=connection.workspace_id
               AND head.connection_id=connection.id
              LEFT JOIN connection_qualification_heads AS qualification_head
                ON qualification_head.workspace_id=connection.workspace_id
               AND qualification_head.connection_revision_id=head.current_revision_id
              LEFT JOIN connection_qualification_revisions AS qualification
                ON qualification.workspace_id=connection.workspace_id
               AND qualification.id=qualification_head.current_qualification_revision_id
             WHERE connection.workspace_id=$1
             ORDER BY connection.created_at, connection.id
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;

        let now = Utc::now();
        rows.into_iter()
            .map(|row| decode_safe_connection_projection(row, now))
            .collect()
    }
}

fn decode_safe_connection_projection(
    row: sqlx::postgres::PgRow,
    now: DateTime<Utc>,
) -> Result<GovernedConnectionProjection, ApplicationError> {
    let id = ConnectionId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let revision_id: Option<uuid::Uuid> = row
        .try_get("connection_revision_id")
        .map_err(storage_error)?;
    let connection_state: Option<String> =
        row.try_get("connection_state").map_err(storage_error)?;
    let qualification_valid_until: Option<DateTime<Utc>> = row
        .try_get("qualification_valid_until")
        .map_err(storage_error)?;

    let mut blockers = Vec::new();
    if revision_id.is_none() {
        blockers.push("missing_governed_connection_revision".to_owned());
    } else if connection_state.as_deref() != Some("enabled") {
        blockers.push("connection_not_enabled".to_owned());
    }
    let qualification_is_expired = qualification_valid_until.is_some_and(|until| until <= now);
    let qualification_is_current = qualification_valid_until.is_some_and(|until| until > now);
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

    Ok(GovernedConnectionProjection {
        id,
        revision_id: revision_id.map(ConnectionRevisionId::from_uuid),
        state: state.to_owned(),
        qualification_state: qualification_state.to_owned(),
        blockers,
    })
}

/// The governed write behind `POST /v1/connections/{id}/admission-policies`.
///
/// It carries no read of the current head. The guarded function compares the
/// version the caller stated against the one it locks, so a stale publisher
/// loses with 40001 instead of overwriting a policy it never saw.
struct ConnectionAdmissionPolicyMutation {
    command: PublishConnectionAdmissionPolicy,
}

#[async_trait]
impl GovernedMutationApply for ConnectionAdmissionPolicyMutation {
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
        let published: Result<i64, sqlx::Error> = sqlx::query_scalar(
            "SELECT vestrace_publish_connection_admission_policy($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.command.policy_revision_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(self.command.connection_id.as_uuid())
        .bind(self.command.expected_head_version as i64)
        .bind(i16::from(self.command.limits.max_in_flight()))
        .bind(self.command.limits.requests_per_60_seconds() as i32)
        .bind(i32::from(self.command.limits.queue_wait_timeout_seconds()))
        .bind(i32::from(
            self.command.limits.provider_throttle_cap_seconds(),
        ))
        .fetch_one(transaction.connection())
        .await;
        match published {
            Ok(_) => Ok(()),
            Err(error) if sqlstate(&error).as_deref() == Some("40001") => {
                Err(ApplicationError::Conflict(
                    "CONNECTION_ADMISSION_POLICY_VERSION_CONFLICT".to_owned(),
                ))
            }
            // 23514 is what the function raises when the Connection has no
            // execution guard, which means it has no governed revision either.
            // A policy published against a Connection that cannot execute would
            // be a policy nothing consults.
            Err(error) if sqlstate(&error).as_deref() == Some("23514") => {
                Err(ApplicationError::Policy(
                    "connection admission policy requires an existing governed connection"
                        .to_owned(),
                ))
            }
            Err(error) => Err(storage_error(error)),
        }
    }
}

struct ConnectionRevisionMutation {
    command: CreateConnectionRevision,
    creates_connection: bool,
}

#[async_trait]
impl GovernedMutationApply for ConnectionRevisionMutation {
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
        if self.creates_connection {
            insert_stable_connection(transaction, &self.command).await?;
        }

        let actual_guard_id: uuid::Uuid =
            sqlx::query_scalar("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
                .bind(self.command.execution_guard_id)
                .bind(context.workspace_id.as_uuid())
                .bind(self.command.connection.id.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage_error)?;
        if actual_guard_id != self.command.execution_guard_id {
            return Err(ApplicationError::Conflict(
                "CONNECTION_EXECUTION_GUARD_MISMATCH".to_owned(),
            ));
        }
        if let Some(credential_slot_id) = self.command.credential_slot_id {
            let has_activation_guard: bool = sqlx::query_scalar(
                "SELECT EXISTS (\
                 SELECT 1 FROM credential_activation_guards \
                 WHERE workspace_id = $1 AND connection_id = $2 \
                   AND credential_slot_id = $3 AND execution_guard_id = $4)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(self.command.connection.id.as_uuid())
            .bind(credential_slot_id.as_uuid())
            .bind(actual_guard_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
            if !has_activation_guard {
                return Err(ApplicationError::Policy(
                    "credential-auth connection revision requires its exact activation guard"
                        .to_owned(),
                ));
            }
        }

        let revision = sqlx::query_scalar(
            "SELECT vestrace_create_connection_revision_and_advance_head(\
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(self.command.revision_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(self.command.connection.id.as_uuid())
        .bind(self.command.execution_guard_id)
        .bind(connection_kind(self.command.kind))
        .bind(&self.command.logical_base_url)
        .bind(&self.command.runtime_base_url)
        .bind(&self.command.adapter_profile_revision)
        .bind(transport_policy(&self.command.transport_policy))
        .bind(auth_mode(self.command.auth_mode))
        .bind(self.command.credential_slot_id.map(|id| id.as_uuid()))
        .bind(self.command.expected_head_version as i64)
        .fetch_one(transaction.connection())
        .await;
        map_revision_result(revision)?;

        if self.command.auth_mode == ConnectionAuthMode::None {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT vestrace_create_no_auth_binding_revision($1, $2, $3, $4)",
            )
            .bind(uuid::Uuid::now_v7())
            .bind(context.workspace_id.as_uuid())
            .bind(self.command.connection.id.as_uuid())
            .bind(self.command.revision_id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
        }
        Ok(())
    }
}

async fn insert_stable_connection(
    transaction: &mut PgScopedTransaction,
    command: &CreateConnectionRevision,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO connections \
         (id, connector_id, workspace_id, principal_id, name, status, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(command.connection.id.as_uuid())
    .bind(command.connection.connector_id.as_uuid())
    .bind(command.connection.workspace_id.as_uuid())
    .bind(command.connection.principal_id.as_uuid())
    .bind(&command.connection.name)
    .bind(connection_status(command.connection.status))
    .bind(command.connection.created_at)
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;
    Ok(())
}

fn validate_command(
    context: &RequestContext,
    command: &CreateConnectionRevision,
) -> Result<(), ApplicationError> {
    if command.connection.workspace_id != context.workspace_id
        || command.connection.principal_id != context.principal_id
        || command.audit.workspace_id != context.workspace_id
        || command.audit.principal_id != context.principal_id
    {
        return Err(ApplicationError::Policy(
            "connection mutation identity must match its request context".to_owned(),
        ));
    }
    if command
        .idempotency
        .as_ref()
        .is_some_and(|record| record.workspace_id != context.workspace_id)
        || command
            .outbox
            .iter()
            .any(|message| message.workspace_id != context.workspace_id)
    {
        return Err(ApplicationError::Policy(
            "connection mutation evidence must stay in its request workspace".to_owned(),
        ));
    }
    match (command.auth_mode, command.credential_slot_id) {
        (ConnectionAuthMode::None, None)
        | (
            ConnectionAuthMode::Bearer | ConnectionAuthMode::ApiKey | ConnectionAuthMode::XApiKey,
            Some(_),
        ) => Ok(()),
        (ConnectionAuthMode::None, Some(_)) => {
            Err(ApplicationError::Domain(DomainError::InvalidArgument(
                "no-auth connection revision cannot name a credential slot".to_owned(),
            )))
        }
        (_, None) => Err(ApplicationError::Domain(DomainError::InvalidArgument(
            "credential auth requires an exact credential slot".to_owned(),
        ))),
    }
}

fn map_revision_result(result: Result<uuid::Uuid, sqlx::Error>) -> Result<(), ApplicationError> {
    match result {
        Ok(_) => Ok(()),
        Err(error) if sqlstate(&error).as_deref() == Some("40001") => Err(
            ApplicationError::Conflict("CONNECTION_VERSION_CONFLICT".to_owned()),
        ),
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

fn connection_kind(value: ConnectionKind) -> &'static str {
    match value {
        ConnectionKind::LMStudioLocal => "lm_studio_local",
        ConnectionKind::OpenAiChatCompletionsV1 => "open_ai_chat_completions_v1",
    }
}

fn transport_policy(value: &ConnectionTransportPolicy) -> &'static str {
    match value {
        ConnectionTransportPolicy::LoopbackOnly => "loopback_only",
        ConnectionTransportPolicy::RemoteHttps => "remote_https",
    }
}

fn auth_mode(value: ConnectionAuthMode) -> &'static str {
    match value {
        ConnectionAuthMode::None => "none",
        ConnectionAuthMode::Bearer => "bearer",
        ConnectionAuthMode::ApiKey => "api_key",
        ConnectionAuthMode::XApiKey => "x_api_key",
    }
}

fn connection_status(value: ConnectionStatus) -> &'static str {
    match value {
        ConnectionStatus::Active => "active",
        ConnectionStatus::Revoked => "revoked",
        ConnectionStatus::Expired => "expired",
    }
}
