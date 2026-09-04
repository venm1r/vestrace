//! Transaction-bound PostgreSQL credential dispatch leases.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection};
use vestrace_application::{
    ApplicationError, ConnectionAuth, CredentialDispatchLease, CredentialDispatchLeaseRepository,
    CredentialDispatchLeaseRequest, CredentialMaterialPreparation,
    CredentialMaterialPreparationFaultInjector, CredentialMaterialPreparationFaultPoint,
    CredentialMaterialPreparationRequest, CredentialMaterialPreparationState,
    CredentialMaterialPreparer, InstallationMutationPermit, MaterialKeyVault,
    NoCredentialMaterialPreparationFaults, PermitMode, ProviderCredential, RequestContext,
    UnitOfWork,
};
use vestrace_domain::{
    ConnectionId, CredentialKeyCreationIntentId, CredentialRevisionId, CredentialSlotId,
    IntentNonce, MaterialKeyId, WorkspaceId,
};

use super::{PgScopedTransaction, PgStore};
use crate::crypto::{CredentialMaterialCodec, CredentialMaterialContext};

/// PostgreSQL lease authority coupled to the host-owned material-key vault.
pub struct PgCredentialDispatchLeaseRepository<V> {
    store: PgStore,
    vault: Arc<V>,
    codec: CredentialMaterialCodec,
}

/// PostgreSQL/vault composition for the fixed-identity credential preparation
/// lifecycle. The guarded row chain is the serialization authority; the
/// operator secret never enters PostgreSQL except as an authenticated frame.
pub struct PgCredentialMaterialPreparer<V> {
    permit: Arc<dyn InstallationMutationPermit>,
    vault: Arc<V>,
    codec: CredentialMaterialCodec,
    faults: Arc<dyn CredentialMaterialPreparationFaultInjector>,
}

impl<V> PgCredentialMaterialPreparer<V>
where
    V: MaterialKeyVault,
{
    pub fn new(permit: Arc<dyn InstallationMutationPermit>, vault: Arc<V>) -> Self {
        Self::with_faults(
            permit,
            vault,
            Arc::new(NoCredentialMaterialPreparationFaults),
        )
    }

    pub fn with_faults(
        permit: Arc<dyn InstallationMutationPermit>,
        vault: Arc<V>,
        faults: Arc<dyn CredentialMaterialPreparationFaultInjector>,
    ) -> Self {
        Self {
            permit,
            vault,
            codec: CredentialMaterialCodec::new(),
            faults,
        }
    }

    fn fault(
        &self,
        point: CredentialMaterialPreparationFaultPoint,
    ) -> Result<(), ApplicationError> {
        self.faults.check(point)
    }
}

impl<V> PgCredentialDispatchLeaseRepository<V>
where
    V: MaterialKeyVault,
{
    pub fn new(store: PgStore, vault: Arc<V>) -> Self {
        Self {
            store,
            vault,
            codec: CredentialMaterialCodec::new(),
        }
    }
}

#[derive(FromRow)]
struct LeaseRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    connection_id: uuid::Uuid,
    external_effect_id: uuid::Uuid,
    authorization_id: uuid::Uuid,
    credential_slot_id: uuid::Uuid,
    credential_revision_id: uuid::Uuid,
    credential_activation_guard_id: uuid::Uuid,
    destination_authority: String,
    auth_mode: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    terminal_state: Option<String>,
}

#[derive(FromRow)]
struct CredentialMaterialRow {
    associated_data_profile: String,
    intent_id: uuid::Uuid,
    nonce: uuid::Uuid,
    material_key_id: uuid::Uuid,
    ciphertext: Vec<u8>,
}

#[derive(FromRow)]
struct CredentialPreparationRow {
    intent_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    connection_id: uuid::Uuid,
    credential_slot_id: uuid::Uuid,
    credential_revision_id: uuid::Uuid,
    material_key_id: uuid::Uuid,
    nonce: uuid::Uuid,
    state: String,
    associated_data_profile: String,
    prepared_attachment_id: Option<uuid::Uuid>,
}

impl CredentialPreparationRow {
    fn result(&self) -> Result<CredentialMaterialPreparation, ApplicationError> {
        let state = match self.state.as_str() {
            "credential_prepared" => CredentialMaterialPreparationState::CredentialPrepared,
            "bound" => CredentialMaterialPreparationState::Bound,
            "candidate" => CredentialMaterialPreparationState::Candidate,
            "active" => CredentialMaterialPreparationState::Active,
            "retired" => CredentialMaterialPreparationState::Retired,
            "revoked" => CredentialMaterialPreparationState::Revoked,
            "credential_abandon_prepared" => {
                CredentialMaterialPreparationState::CredentialAbandonPrepared
            }
            "erasure_prepared" => CredentialMaterialPreparationState::ErasurePrepared,
            "destroyed" => CredentialMaterialPreparationState::Destroyed,
            "abandoned" => CredentialMaterialPreparationState::Abandoned,
            _ => {
                return Err(ApplicationError::Policy(
                    "CREDENTIAL_MATERIAL_PREPARATION_REFUSED".into(),
                ));
            }
        };
        Ok(CredentialMaterialPreparation {
            intent_id: CredentialKeyCreationIntentId::from_uuid(self.intent_id),
            credential_revision_id: CredentialRevisionId::from_uuid(self.credential_revision_id),
            material_key_id: MaterialKeyId::from_uuid(self.material_key_id),
            prepared_attachment_id: self
                .prepared_attachment_id
                .map(vestrace_domain::CredentialPreparedAttachmentId::from_uuid),
            state,
        })
    }
}

impl From<LeaseRow> for CredentialDispatchLease {
    fn from(row: LeaseRow) -> Self {
        Self {
            id: row.id,
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            connection_id: ConnectionId::from_uuid(row.connection_id),
            external_effect_id: row.external_effect_id,
            authorization_id: row.authorization_id,
            credential_slot_id: CredentialSlotId::from_uuid(row.credential_slot_id),
            credential_revision_id: row.credential_revision_id,
            credential_activation_guard_id: row.credential_activation_guard_id,
            destination_authority: row.destination_authority,
            auth_mode: row.auth_mode,
            issued_at: row.issued_at,
            expires_at: row.expires_at,
            consumed_at: row.consumed_at,
            terminal_state: row.terminal_state,
        }
    }
}

impl LeaseRow {
    fn matches_immutable_authority(&self, lease: &CredentialDispatchLease) -> bool {
        self.id == lease.id
            && self.workspace_id == lease.workspace_id.as_uuid()
            && self.connection_id == lease.connection_id.as_uuid()
            && self.external_effect_id == lease.external_effect_id
            && self.authorization_id == lease.authorization_id
            && self.credential_slot_id == lease.credential_slot_id.as_uuid()
            && self.credential_revision_id == lease.credential_revision_id
            && self.credential_activation_guard_id == lease.credential_activation_guard_id
            && self.destination_authority == lease.destination_authority
            && self.auth_mode == lease.auth_mode
            && self.issued_at == lease.issued_at
            && self.expires_at == lease.expires_at
    }
}

enum PersistedAuthMode {
    Bearer,
    ApiKey,
    XApiKey,
}

impl PersistedAuthMode {
    fn parse(value: &str) -> Result<Self, ApplicationError> {
        match value {
            "bearer" => Ok(Self::Bearer),
            "api_key" => Ok(Self::ApiKey),
            "x_api_key" => Ok(Self::XApiKey),
            _ => Err(ApplicationError::Policy(
                "CREDENTIAL_DISPATCH_LEASE_REFUSED".to_owned(),
            )),
        }
    }

    fn into_auth(self, credential: ProviderCredential) -> ConnectionAuth {
        match self {
            Self::Bearer => ConnectionAuth::Bearer(credential),
            Self::ApiKey => ConnectionAuth::ApiKey(credential),
            Self::XApiKey => ConnectionAuth::XApiKey(credential),
        }
    }
}

#[async_trait]
impl<V> CredentialDispatchLeaseRepository for PgCredentialDispatchLeaseRepository<V>
where
    V: MaterialKeyVault + 'static,
{
    async fn issue(
        &self,
        context: &RequestContext,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let lease = self.issue_in(context, &mut transaction, request).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(lease)
    }

    async fn issue_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        issue_on(transaction.connection(), context, request).await
    }

    async fn consume_for_dispatch(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        lease: &CredentialDispatchLease,
    ) -> Result<ConnectionAuth, ApplicationError> {
        if lease.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "CREDENTIAL_DISPATCH_LEASE_WORKSPACE_REFUSED".to_owned(),
            ));
        }
        let transaction = postgres_transaction(unit_of_work)?;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_consume_credential_dispatch_lease($1,$2,$3,$4)",
        )
        .bind(lease.id)
        .bind(context.workspace_id.as_uuid())
        .bind(lease.connection_id.as_uuid())
        .bind(lease.external_effect_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_credential_dispatch_lease_error)?;

        let persisted = fetch_lease(transaction.connection(), lease.id)
            .await
            .map_err(map_credential_dispatch_lease_error)?;
        if !persisted.matches_immutable_authority(lease) {
            return Err(ApplicationError::Policy(
                "CREDENTIAL_DISPATCH_LEASE_REFUSED".to_owned(),
            ));
        }
        let auth_mode = PersistedAuthMode::parse(&persisted.auth_mode)?;

        let material: CredentialMaterialRow = sqlx::query_as(
            "SELECT revision.associated_data_profile, intent.id AS intent_id, intent.nonce, \
                    revision.material_key_id, prepared.ciphertext \
               FROM credential_key_creation_intents AS intent \
               JOIN credential_revisions AS revision \
                 ON revision.workspace_id=intent.workspace_id \
                AND revision.id=intent.credential_revision_id \
               JOIN credential_prepared_materials AS prepared \
                 ON prepared.workspace_id=intent.workspace_id \
                AND prepared.intent_id=intent.id \
                AND prepared.credential_revision_id=revision.id \
              WHERE intent.workspace_id=$1 AND intent.connection_id=$2 \
                AND intent.credential_slot_id=$3 AND intent.credential_revision_id=$4 \
                AND (intent.state='active' OR (intent.state='candidate' AND EXISTS ( \
                    SELECT 1 \
                      FROM provider_dispatch_causes AS cause \
                      JOIN qualification_target_bindings AS binding \
                        ON binding.workspace_id=cause.workspace_id \
                       AND binding.id=cause.qualification_target_binding_id \
                     WHERE cause.workspace_id=$1 \
                       AND cause.external_effect_id=$5 \
                       AND cause.cause_kind='qualification_probe' \
                       AND binding.connection_id=$2 \
                       AND binding.branch='credential' \
                       AND binding.credential_slot_id=$3 \
                       AND binding.credential_revision_id=$4 \
                       AND binding.credential_activation_guard_id=$6 \
                )))",
        )
        .bind(persisted.workspace_id)
        .bind(persisted.connection_id)
        .bind(persisted.credential_slot_id)
        .bind(persisted.credential_revision_id)
        .bind(persisted.external_effect_id)
        .bind(persisted.credential_activation_guard_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_credential_dispatch_lease_error)?;

        let material_context = CredentialMaterialContext {
            profile: material.associated_data_profile.as_str(),
            workspace_id: WorkspaceId::from_uuid(persisted.workspace_id),
            connection_id: ConnectionId::from_uuid(persisted.connection_id),
            credential_slot_id: CredentialSlotId::from_uuid(persisted.credential_slot_id),
            credential_revision_id: CredentialRevisionId::from_uuid(
                persisted.credential_revision_id,
            ),
            material_key_id: MaterialKeyId::from_uuid(material.material_key_id),
            intent_id: CredentialKeyCreationIntentId::from_uuid(material.intent_id),
            intent_nonce: IntentNonce::from_uuid(material.nonce),
        };
        self.codec
            .validate_frame(&material.ciphertext)
            .map_err(credential_codec_error)?;

        // The guarded consume function has updated the lease only after every
        // live/expiry/current-revision predicate passed. Do not move this
        // vault call above it: even a refused dispatch must never unwrap a DEK.
        let mut opened = None;
        self.vault
            .unwrap(material_context.material_key_id, &mut |dek| {
                opened = Some(
                    self.codec
                        .open(&material_context, dek, &material.ciphertext),
                );
            })
            .map_err(|error| ApplicationError::Unavailable(error.to_string()))?;
        let plaintext = opened
            .ok_or_else(|| ApplicationError::Unavailable("vault returned no credential".into()))?
            .map_err(credential_codec_error)?;
        Ok(auth_mode.into_auth(provider_credential_from_zeroizing(plaintext)))
    }
}

fn provider_credential_from_zeroizing(
    plaintext: zeroize::Zeroizing<Vec<u8>>,
) -> ProviderCredential {
    ProviderCredential::new(ZeroizingUtf8Transfer(plaintext))
}

struct ZeroizingUtf8Transfer(zeroize::Zeroizing<Vec<u8>>);

impl From<ZeroizingUtf8Transfer> for String {
    fn from(mut transfer: ZeroizingUtf8Transfer) -> Self {
        let bytes = std::mem::take(&mut *transfer.0);
        match String::from_utf8(bytes) {
            Ok(value) => value,
            Err(error) => {
                use zeroize::Zeroize;
                let mut bytes = error.into_bytes();
                bytes.zeroize();
                unreachable!("credential codec returned bytes it had not validated as UTF-8")
            }
        }
    }
}

#[async_trait]
impl<V> CredentialMaterialPreparer for PgCredentialMaterialPreparer<V>
where
    V: MaterialKeyVault + 'static,
{
    async fn prepare(
        &self,
        context: &RequestContext,
        request: CredentialMaterialPreparationRequest,
    ) -> Result<CredentialMaterialPreparation, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let row = lock_credential_preparation(transaction.connection(), context, &request).await?;

        if matches!(
            row.state.as_str(),
            "credential_prepared"
                | "bound"
                | "candidate"
                | "active"
                | "retired"
                | "revoked"
                | "credential_abandon_prepared"
                | "erasure_prepared"
                | "destroyed"
                | "abandoned"
        ) {
            let result = row.result()?;
            permit.commit().await?;
            return Ok(result);
        }
        if !matches!(
            row.state.as_str(),
            "reserved" | "provisional_created" | "provisional_receipted"
        ) {
            return Err(ApplicationError::Policy(
                "CREDENTIAL_MATERIAL_PREPARATION_REFUSED".into(),
            ));
        }

        self.codec
            .validate_plaintext(request.credential.expose())
            .map_err(credential_codec_error)?;
        let receipt = if matches!(row.state.as_str(), "reserved" | "provisional_created") {
            let receipt = self
                .vault
                .create_if_absent(request.material_key_id, request.intent_nonce)
                .map_err(|error| ApplicationError::Unavailable(error.to_string()))?;
            self.fault(CredentialMaterialPreparationFaultPoint::AfterVaultCreate)?;
            Some(receipt)
        } else {
            None
        };
        if row.state == "reserved" {
            sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
                .bind(request.intent_id.as_uuid())
                .execute(transaction.connection())
                .await
                .map_err(map_credential_dispatch_lease_error)?;
            self.fault(CredentialMaterialPreparationFaultPoint::AfterProvisionalCreated)?;
        }
        if let Some(receipt) = receipt {
            sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1,$2)")
                .bind(request.intent_id.as_uuid())
                .bind(receipt.as_uuid())
                .execute(transaction.connection())
                .await
                .map_err(map_credential_dispatch_lease_error)?;
            self.fault(CredentialMaterialPreparationFaultPoint::AfterProvisionalReceipt)?;
        }

        let material_context = CredentialMaterialContext {
            profile: row.associated_data_profile.as_str(),
            workspace_id: context.workspace_id,
            connection_id: request.connection_id,
            credential_slot_id: request.credential_slot_id,
            credential_revision_id: request.credential_revision_id,
            material_key_id: request.material_key_id,
            intent_id: request.intent_id,
            intent_nonce: request.intent_nonce,
        };
        let mut sealed = None;
        self.vault
            .unwrap(request.material_key_id, &mut |dek| {
                sealed = Some(
                    self.codec
                        .seal(&material_context, dek, request.credential.expose()),
                );
            })
            .map_err(|error| ApplicationError::Unavailable(error.to_string()))?;
        let frame = sealed
            .ok_or_else(|| {
                ApplicationError::Unavailable("vault returned no credential material key".into())
            })?
            .map_err(credential_codec_error)?;
        self.fault(CredentialMaterialPreparationFaultPoint::AfterSeal)?;
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1,$2,$3)")
            .bind(request.intent_id.as_uuid())
            .bind(request.prepared_attachment_id.as_uuid())
            .bind(frame)
            .execute(transaction.connection())
            .await
            .map_err(map_credential_dispatch_lease_error)?;
        self.fault(CredentialMaterialPreparationFaultPoint::AfterPrepared)?;
        permit.commit().await?;
        Ok(CredentialMaterialPreparation {
            intent_id: request.intent_id,
            credential_revision_id: request.credential_revision_id,
            material_key_id: request.material_key_id,
            prepared_attachment_id: Some(request.prepared_attachment_id),
            state: CredentialMaterialPreparationState::CredentialPrepared,
        })
    }
}

async fn lock_credential_preparation(
    connection: &mut PgConnection,
    context: &RequestContext,
    request: &CredentialMaterialPreparationRequest,
) -> Result<CredentialPreparationRow, ApplicationError> {
    sqlx::query(
        "SELECT vestrace_acquire_credential_lock_chain(\
         $1,$2,$3,ARRAY['connection_execution_guard','credential_activation_guard',\
         'credential_slot','revision_material']::TEXT[])",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(request.connection_id.as_uuid())
    .bind(request.credential_slot_id.as_uuid())
    .execute(&mut *connection)
    .await
    .map_err(map_credential_dispatch_lease_error)?;

    let row: CredentialPreparationRow = sqlx::query_as(
        "SELECT intent.id AS intent_id, intent.workspace_id, intent.connection_id, \
                intent.credential_slot_id, intent.credential_revision_id, \
                intent.material_key_id, intent.nonce, \
                CASE WHEN intent.state='retired' AND slot.tombstone_version IS NOT NULL \
                     THEN 'revoked' ELSE intent.state END AS state, \
                revision.associated_data_profile, attachment.id AS prepared_attachment_id \
           FROM credential_key_creation_intents AS intent \
           JOIN credential_revisions AS revision \
             ON revision.workspace_id=intent.workspace_id \
            AND revision.id=intent.credential_revision_id \
           JOIN credential_slots AS slot \
             ON slot.workspace_id=intent.workspace_id \
            AND slot.connection_id=intent.connection_id \
            AND slot.id=intent.credential_slot_id \
           LEFT JOIN credential_prepared_attachments AS attachment \
             ON attachment.workspace_id=intent.workspace_id \
             AND attachment.intent_id=intent.id \
          WHERE intent.id=$1",
    )
    .bind(request.intent_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(map_credential_dispatch_lease_error)?;

    if row.workspace_id != context.workspace_id.as_uuid()
        || row.connection_id != request.connection_id.as_uuid()
        || row.credential_slot_id != request.credential_slot_id.as_uuid()
        || row.credential_revision_id != request.credential_revision_id.as_uuid()
        || row.material_key_id != request.material_key_id.as_uuid()
        || row.nonce != request.intent_nonce.as_uuid()
        || row.associated_data_profile != "credential_v2"
        || row
            .prepared_attachment_id
            .is_some_and(|id| id != request.prepared_attachment_id.as_uuid())
    {
        return Err(ApplicationError::Policy(
            "CREDENTIAL_MATERIAL_PREPARATION_REFUSED".into(),
        ));
    }
    Ok(row)
}

fn credential_codec_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Policy(format!("CREDENTIAL_MATERIAL_REFUSED: {error}"))
}

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL dispatch transaction".to_owned())
        })
}

async fn issue_on(
    connection: &mut PgConnection,
    context: &RequestContext,
    request: CredentialDispatchLeaseRequest,
) -> Result<CredentialDispatchLease, ApplicationError> {
    let lease_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT vestrace_issue_credential_dispatch_lease(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(request.lease_id)
    .bind(context.workspace_id.as_uuid())
    .bind(request.connection_id.as_uuid())
    .bind(request.external_effect_id)
    .bind(request.authorization_id)
    .bind(request.credential_slot_id.as_uuid())
    .bind(request.credential_revision_id)
    .bind(request.credential_activation_guard_id)
    .bind(request.destination_authority)
    .bind(request.auth_mode)
    .bind(request.expires_at)
    .fetch_one(&mut *connection)
    .await
    .map_err(map_credential_dispatch_lease_error)?;
    fetch_lease(connection, lease_id)
        .await
        .map(CredentialDispatchLease::from)
        .map_err(map_credential_dispatch_lease_error)
}

async fn fetch_lease(
    connection: &mut PgConnection,
    lease_id: uuid::Uuid,
) -> Result<LeaseRow, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, workspace_id, connection_id, external_effect_id, authorization_id, \
                credential_slot_id, credential_revision_id, credential_activation_guard_id, \
                destination_authority, auth_mode, issued_at, expires_at, consumed_at, terminal_state \
           FROM credential_dispatch_leases WHERE id = $1",
    )
    .bind(lease_id)
    .fetch_one(connection)
    .await
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn map_credential_dispatch_lease_error(error: sqlx::Error) -> ApplicationError {
    match sqlstate(&error).as_deref() {
        Some("22023") => ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(
            "credential dispatch lease arguments are malformed".to_owned(),
        )),
        Some("40001") | Some("23505") => {
            ApplicationError::Conflict("CREDENTIAL_DISPATCH_LEASE_CONFLICT".to_owned())
        }
        Some("23514") => ApplicationError::Policy("CREDENTIAL_DISPATCH_LEASE_REFUSED".to_owned()),
        Some("42501") => {
            ApplicationError::Policy("CREDENTIAL_DISPATCH_LEASE_RAW_MUTATION_REFUSED".to_owned())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ForeignUnitOfWork;

    #[async_trait]
    impl UnitOfWork for ForeignUnitOfWork {
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[test]
    fn a_foreign_unit_of_work_is_a_typed_error() {
        let mut unit_of_work = ForeignUnitOfWork;
        match postgres_transaction(&mut unit_of_work) {
            Err(ApplicationError::Internal(message)) => {
                assert_eq!(message, "expected PostgreSQL dispatch transaction")
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("foreign unit of work was accepted"),
        }
    }
}
