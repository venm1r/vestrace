use sqlx::Row;
use vestrace_application::{
    ApplicationError, MaterialIntentRepository, MaterialIntentSnapshot, RequestContext, UnitOfWork,
};
use vestrace_domain::{
    ContentMaterialId, ErasureReceipt, IntentNonce, MaterialKeyBindingReceipt,
    MaterialKeyCreationIntent, MaterialKeyCreationIntentId, MaterialKeyCreationIntentState,
    MaterialKeyId, PreparedMaterialAttachmentId, SizeClass, VaultReceipt,
};

use super::{PgScopedTransaction, PgStore};

/// PostgreSQL-backed material-key creation lifecycle. Each public operation
/// receives a scoped transaction and calls only a guarded database transition.
#[derive(Clone, Debug)]
pub struct PgMaterialIntentRepository {
    store: PgStore,
}

impl PgMaterialIntentRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
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

fn postgres_transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL material transaction".into())
        })
}

#[async_trait::async_trait]
impl MaterialIntentRepository for PgMaterialIntentRepository {
    async fn reserve_in(
        &self,
        _context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        intent: &MaterialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(intent.id().as_uuid())
        .bind(intent.workspace_id().as_uuid())
        .bind(intent.material_id().as_uuid())
        .bind(intent.material_key_id().as_uuid())
        .bind(intent.nonce().as_uuid())
        .bind(intent.owner_kind())
        .bind(intent.owner_id().as_uuid())
        .bind(intent.output_ordinal() as i64)
        .execute(postgres_transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn reserve(
        &self,
        context: &RequestContext,
        intent: &MaterialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query(
                "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(intent.id().as_uuid())
            .bind(intent.workspace_id().as_uuid())
            .bind(intent.material_id().as_uuid())
            .bind(intent.material_key_id().as_uuid())
            .bind(intent.nonce().as_uuid())
            .bind(intent.owner_kind())
            .bind(intent.owner_id().as_uuid())
            .bind(intent.output_ordinal() as i64),
        )
        .await
    }

    async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn prepare_content(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
                .bind(intent_id.as_uuid())
                .bind(attachment_id.as_uuid())
                .bind(ciphertext)
                .bind(size_class.minimum_bytes() as i64),
        )
        .await
    }

    async fn prepare_content_in(
        &self,
        _context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
            .bind(intent_id.as_uuid())
            .bind(attachment_id.as_uuid())
            .bind(ciphertext)
            .bind(size_class.minimum_bytes() as i64)
            .execute(postgres_transaction(unit_of_work)?.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn prepare_result(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_prepare_result_material($1, $2, $3, $4)")
                .bind(intent_id.as_uuid())
                .bind(attachment_id.as_uuid())
                .bind(ciphertext)
                .bind(size_class.minimum_bytes() as i64),
        )
        .await
    }

    async fn bind(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn finalize_bound(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn prepare_content_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_prepare_content_abandon($1)").bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn prepare_pre_prepared_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_prepare_pre_prepared_material_abandon($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1, $2)")
                .bind(intent_id.as_uuid())
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
                .bind(intent_id.as_uuid()),
        )
        .await
    }

    async fn is_live(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<bool, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let live = sqlx::query("SELECT vestrace_content_material_is_live($1) AS is_live")
            .bind(material_id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?
            .get::<bool, _>("is_live");
        transaction.commit().await.map_err(storage_error)?;
        Ok(live)
    }

    async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<Option<MaterialIntentSnapshot>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT intent.state, intent.material_key_id, intent.nonce, \
                    EXISTS ( \
                        SELECT 1 FROM material_key_creation_intent_erasure_receipts \
                         WHERE intent_id = intent.id \
                    ) AS has_erasure_receipt \
               FROM material_key_creation_intents AS intent \
              WHERE intent.id = $1",
        )
        .bind(intent_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        let Some(row) = row else {
            return Ok(None);
        };
        // A state this build does not know is an error, never a default. The
        // repository already refuses to turn a decode failure into an absence.
        let state = match row.get::<String, _>("state").as_str() {
            "reserved" => MaterialKeyCreationIntentState::Reserved,
            "provisional_created" => MaterialKeyCreationIntentState::ProvisionalCreated,
            "provisional_receipted" => MaterialKeyCreationIntentState::ProvisionalReceipted,
            "content_prepared" => MaterialKeyCreationIntentState::ContentPrepared,
            "result_prepared" => MaterialKeyCreationIntentState::ResultPrepared,
            "content_abandon_prepared" => MaterialKeyCreationIntentState::ContentAbandonPrepared,
            "pre_prepared_abandon_prepared" => {
                MaterialKeyCreationIntentState::PrePreparedAbandonPrepared
            }
            "bound" => MaterialKeyCreationIntentState::Bound,
            "live" => MaterialKeyCreationIntentState::Live,
            "erasure_prepared" => MaterialKeyCreationIntentState::ErasurePrepared,
            "tombstoned" => MaterialKeyCreationIntentState::Tombstoned,
            "abandoned" => MaterialKeyCreationIntentState::Abandoned,
            other => {
                return Err(ApplicationError::Storage(format!(
                    "material key creation intent state '{other}' is not a known state"
                )));
            }
        };
        Ok(Some(MaterialIntentSnapshot {
            state,
            material_key_id: MaterialKeyId::from_uuid(row.get("material_key_id")),
            nonce: IntentNonce::from_uuid(row.get("nonce")),
            has_erasure_receipt: row.get("has_erasure_receipt"),
        }))
    }
}
