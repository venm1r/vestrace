//! PostgreSQL authority for erasure propagation through embedding corpora.
//!
//! Nothing here decides what erasing a source does. Migration 0203 holds the
//! whole ordering under its own locks and this adapter calls it once, reads
//! back what it recorded, and translates its refusals. The one judgement this
//! file does make is whether the propagating path applies at all -- a routing
//! question, taken without a lock, and fail-closed when it is answered wrongly.

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, MaterialErasurePreparation, RequestContext,
    embedding::{
        CommittedEmbeddingInvalidation, EmbeddingErasurePropagation, EmbeddingErasureRepository,
    },
};
use vestrace_domain::{ContentMaterialId, ErasureReceipt, MaterialKeyId};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgEmbeddingErasureRepository {
    store: PgStore,
}

impl PgEmbeddingErasureRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        // The propagation raises 23514 for every refusal it owns: a source that
        // is not exactly Live, an affected space with no corpus state or guard,
        // a preparation that could not be produced. None of them is a storage
        // fault and none of them is retryable by repeating the same call.
        Some("23514") | Some("22023") => ApplicationError::Conflict(error.to_string()),
        Some("42501") => ApplicationError::Policy(error.to_string()),
        _ => ApplicationError::Storage(error.to_string()),
    }
}

fn non_negative(value: i64, what: &str) -> Result<u64, ApplicationError> {
    u64::try_from(value)
        .map_err(|_| ApplicationError::Storage(format!("stored {what} is negative: {value}")))
}

#[async_trait]
impl EmbeddingErasureRepository for PgEmbeddingErasureRepository {
    async fn propagate_source_erasure(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<Option<EmbeddingErasurePropagation>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        // Does the propagating path apply? Two ways it can: this source was
        // already propagated and we are replaying, or a projection in this
        // workspace was computed from it.
        //
        // Read without a row lock, deliberately. The runtime role has SELECT on
        // `content_materials` and nothing more, so it cannot take one -- and it
        // should not: the ordering this module exists to enforce is held by
        // locks inside the propagating function, which takes them itself. This
        // query only routes.
        //
        // That leaves a race: a projection computed from this source between
        // the routing question and the ordinary path's transaction. The
        // fallback is fail-closed for exactly it. Creating a delivery source
        // records a nonterminal erasure blocker and
        // `vestrace_prepare_content_material_erasure` refuses while one exists,
        // so routing wrongly refuses the erasure rather than performing it.
        let applies: Option<bool> = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM embedding_erasure_propagations \
                            WHERE workspace_id=$1 AND source_material_id=$2) \
                 OR EXISTS(SELECT 1 FROM embedding_projection_source_dependencies \
                            WHERE workspace_id=$1 AND source_material_id=$2) \
               FROM content_materials \
              WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(material_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage)?;

        // No such material in this workspace is not this module's refusal to
        // make. The ordinary authority owns identity of the erasure target and
        // says so in its own words.
        let Some(true) = applies else {
            return Ok(None);
        };

        let propagation_id = Uuid::now_v7();
        let preparation_id: Uuid =
            sqlx::query_scalar("SELECT vestrace_propagate_embedding_source_erasure($1,$2,$3)")
                .bind(propagation_id)
                .bind(context.workspace_id.as_uuid())
                .bind(material_id.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage)?;

        // The propagation returns only the preparation's identity. The vault
        // sequence that follows needs its key and any receipt a completed
        // replay already carries, so read it back from the row the same
        // function wrote -- inside this transaction, before it commits, so what
        // the caller carries forward cannot describe a propagation that rolled
        // back.
        let recorded: (Uuid, i64, i64, i64) = sqlx::query_as(
            "SELECT id, dependent_projection_count, revoked_generation_count, \
                    staled_transition_count \
               FROM embedding_erasure_propagations \
              WHERE workspace_id=$1 AND source_material_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(material_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage)?;

        // The ciphertext of every projection this propagation retired. Read
        // from the propagation's own record rather than from the projections:
        // once retired, this row is the only remaining way back to the material
        // a projection named.
        let vectors: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT vector_material_id FROM embedding_erasure_revoked_members \
              WHERE workspace_id=$1 AND propagation_id=$2 \
              ORDER BY projection_entry_id",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(recorded.0)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage)?;

        let preparation: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
            "SELECT id, material_key_id, erasure_receipt \
               FROM material_erasure_preparations \
              WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(preparation_id)
        .fetch_one(transaction.connection())
        .await
        .map_err(storage)?;

        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        Ok(Some(EmbeddingErasurePropagation::new(
            recorded.0,
            MaterialErasurePreparation::new(
                preparation.0,
                MaterialKeyId::from_uuid(preparation.1),
                preparation.2.map(ErasureReceipt::from_uuid),
            ),
            vectors
                .into_iter()
                .map(|(material,)| ContentMaterialId::from_uuid(material))
                .collect(),
            non_negative(recorded.1, "dependent projection count")?,
            non_negative(recorded.2, "revoked generation count")?,
            non_negative(recorded.3, "staled transition count")?,
        )))
    }

    async fn committed_invalidations(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<CommittedEmbeddingInvalidation>, ApplicationError> {
        if limit == 0 || limit > 1024 {
            return Err(ApplicationError::Conflict(
                "embedding invalidation read limit must be between 1 and 1024".into(),
            ));
        }
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        // Only erasure-caused events. A publication advances the same epoch,
        // but the publishing path installs and removes its own index and
        // sweeping those here would be this module acting outside its cause.
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT space_registration_id, max(after_generation_epoch) \
               FROM embedding_index_rebuild_events \
              WHERE workspace_id=$1 AND cause='material_erasure' \
              GROUP BY space_registration_id \
              ORDER BY max(after_generation_epoch) DESC \
              LIMIT $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        rows.into_iter()
            .map(|(space_registration_id, epoch)| {
                Ok(CommittedEmbeddingInvalidation {
                    space_registration_id,
                    committed_epoch: non_negative(epoch, "invalidation epoch")?,
                })
            })
            .collect()
    }
}
