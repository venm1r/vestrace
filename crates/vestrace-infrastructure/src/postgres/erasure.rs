use sqlx::Row;
use vestrace_application::{
    ApplicationError, FenceReceipt, MaterialErasurePreparation, MaterialErasureRepository,
    RequestContext,
};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntentId, ErasureReceipt, MaterialKeyId,
};

use super::PgStore;

/// PostgreSQL adapter for the database halves of two-phase material erasure.
/// Every public operation commits its short scoped transaction before returning
/// to the application, which leaves the host vault structurally outside all
/// database lock lifetimes.
#[derive(Clone, Debug)]
pub struct PgMaterialErasureRepository {
    store: PgStore,
}

impl PgMaterialErasureRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn preparation_from_row(row: sqlx::postgres::PgRow) -> MaterialErasurePreparation {
    MaterialErasurePreparation::new(
        row.get("preparation_id"),
        MaterialKeyId::from_uuid(row.get("material_key_id")),
        row.get::<Option<uuid::Uuid>, _>("finalized_erasure_receipt")
            .map(ErasureReceipt::from_uuid),
    )
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
impl MaterialErasureRepository for PgMaterialErasureRepository {
    async fn prepare_content(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<MaterialErasurePreparation, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
             FROM vestrace_prepare_content_material_erasure($1)",
        )
        .bind(material_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(preparation_from_row(row))
    }

    async fn prepare_credential(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<MaterialErasurePreparation, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
             FROM vestrace_prepare_credential_material_erasure($1)",
        )
        .bind(intent_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(preparation_from_row(row))
    }

    async fn record_fence(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: FenceReceipt,
    ) -> Result<(), ApplicationError> {
        execute_guarded(
            &self.store,
            context,
            sqlx::query("SELECT vestrace_record_material_erasure_fence($1, $2)")
                .bind(preparation_id)
                .bind(receipt.as_uuid()),
        )
        .await
    }

    async fn finalize_content(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: ErasureReceipt,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let receipt =
            sqlx::query_scalar("SELECT vestrace_finalize_content_material_erasure($1, $2)")
                .bind(preparation_id)
                .bind(receipt.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(ErasureReceipt::from_uuid(receipt))
    }

    async fn finalize_credential(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: ErasureReceipt,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let receipt =
            sqlx::query_scalar("SELECT vestrace_finalize_credential_material_erasure($1, $2)")
                .bind(preparation_id)
                .bind(receipt.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(ErasureReceipt::from_uuid(receipt))
    }
}
