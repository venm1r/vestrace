use sqlx::Row;
use vestrace_application::{
    ApplicationError, CredentialIntentRepository, CredentialIntentSnapshot, RequestContext,
};
use vestrace_domain::{
    CredentialKeyCreationIntent, CredentialKeyCreationIntentId, CredentialKeyCreationIntentState,
    CredentialPreparedAttachmentId, CredentialRevisionId, ErasureReceipt, IntentNonce,
    MaterialKeyBindingReceipt, MaterialKeyId, VaultReceipt,
};

use super::PgStore;

/// PostgreSQL-backed credential-key lifecycle. Each operation enters through
/// the single guarded SQL authority, which owns lock ordering and publication.
#[derive(Clone, Debug)]
pub struct PgCredentialIntentRepository {
    store: PgStore,
}

impl PgCredentialIntentRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
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

async fn execute_guarded(
    store: &PgStore,
    context: &RequestContext,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<(), ApplicationError> {
    let mut transaction = store.begin_scoped(context).await.map_err(storage_error)?;
    query
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;
    transaction.commit().await.map_err(storage_error)
}

#[async_trait::async_trait]
impl CredentialIntentRepository for PgCredentialIntentRepository {
    async fn reserve(
        &self,
        context: &RequestContext,
        intent: &CredentialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let result = sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(\
             $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
        )
        .bind(intent.id().as_uuid())
        .bind(intent.workspace_id().as_uuid())
        .bind(intent.connection_id().as_uuid())
        .bind(intent.credential_slot_id().as_uuid())
        .bind(intent.occupancy_id())
        .bind(intent.credential_revision_id().as_uuid())
        .bind(intent.material_key_id().as_uuid())
        .bind(intent.nonce().as_uuid())
        .execute(transaction.connection())
        .await;
        match result {
            Ok(_) => transaction.commit().await.map_err(storage_error),
            Err(error) if sqlstate(&error).as_deref() == Some("23505") => Err(
                ApplicationError::Conflict("CREDENTIAL_GUARD_OCCUPIED".to_owned()),
            ),
            Err(error) => Err(storage_error(error)),
        }
    }

    async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn prepare(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        attachment_id: CredentialPreparedAttachmentId,
        ciphertext: &[u8],
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
                .bind(intent_id.as_uuid())
                .bind(attachment_id.as_uuid())
                .bind(ciphertext),
        )
        .await
    }

    async fn bind(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn finalize_candidate(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn prepare_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        expected_association_version: u64,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_prepare_credential_pre_live_abandon($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(expected_association_version as i64),
        )
        .await
    }

    async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn is_candidate(
        &self,
        context: &RequestContext,
        revision_id: CredentialRevisionId,
    ) -> Result<bool, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let candidate =
            sqlx::query("SELECT vestrace_credential_revision_is_candidate($1) AS is_candidate")
                .bind(revision_id.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage_error)?
                .get::<bool, _>("is_candidate");
        transaction.commit().await.map_err(storage_error)?;
        Ok(candidate)
    }

    async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<Option<CredentialIntentSnapshot>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT intent.state, intent.material_key_id, intent.nonce,                     occupancy.association_version,                     EXISTS (                         SELECT 1 FROM credential_key_creation_intent_erasure_receipts                          WHERE intent_id = intent.id                     ) AS has_erasure_receipt                FROM credential_key_creation_intents AS intent                JOIN credential_guard_occupancies AS occupancy                  ON occupancy.id = intent.occupancy_id               WHERE intent.id = $1",
        )
        .bind(intent_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        let Some(row) = row else {
            return Ok(None);
        };
        let state = match row.get::<String, _>("state").as_str() {
            "reserved" => CredentialKeyCreationIntentState::Reserved,
            "provisional_created" => CredentialKeyCreationIntentState::ProvisionalCreated,
            "provisional_receipted" => CredentialKeyCreationIntentState::ProvisionalReceipted,
            "credential_prepared" => CredentialKeyCreationIntentState::CredentialPrepared,
            "credential_abandon_prepared" => {
                CredentialKeyCreationIntentState::CredentialAbandonPrepared
            }
            "bound" => CredentialKeyCreationIntentState::Bound,
            "candidate" => CredentialKeyCreationIntentState::Candidate,
            "erasure_prepared" => CredentialKeyCreationIntentState::ErasurePrepared,
            "destroyed" => CredentialKeyCreationIntentState::Destroyed,
            "abandoned" => CredentialKeyCreationIntentState::Abandoned,
            other => {
                return Err(ApplicationError::Storage(format!(
                    "credential key creation intent state '{other}' is not a known state"
                )));
            }
        };
        let association_version: i64 = row.get("association_version");
        Ok(Some(CredentialIntentSnapshot {
            state,
            material_key_id: MaterialKeyId::from_uuid(row.get("material_key_id")),
            nonce: IntentNonce::from_uuid(row.get("nonce")),
            association_version: association_version as u64,
            has_erasure_receipt: row.get("has_erasure_receipt"),
        }))
    }
}
