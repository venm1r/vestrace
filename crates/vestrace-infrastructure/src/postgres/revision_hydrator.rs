//! Resolving revision references against Postgres.

use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;
use vestrace_application::retrieval::RevisionHydrator;
use vestrace_application::{ApplicationError, NormalizedRetrievalRequest, RequestContext};
use vestrace_domain::retrieval::{HydratedRevision, RevisionRef};
use vestrace_domain::{
    MemoryRevision, MemoryStatus,
    id::{MemoryId, MemoryRevisionId},
};

use super::PgStore;

pub struct PgRevisionHydrator {
    store: PgStore,
}

impl PgRevisionHydrator {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

/// An unrecognised status is refused rather than defaulted.
///
/// Defaulting to `Active` would present retired content as current, which is
/// the one thing the status is there to prevent.
fn memory_status_from_str(value: &str) -> Result<MemoryStatus, ApplicationError> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "active" => Ok(MemoryStatus::Active),
        "superseded" => Ok(MemoryStatus::Superseded),
        "rejected" => Ok(MemoryStatus::Rejected),
        "expired" => Ok(MemoryStatus::Expired),
        "deleted" => Ok(MemoryStatus::Deleted),
        _ => Err(ApplicationError::Storage(format!(
            "stored memory status '{value}' is not supported"
        ))),
    }
}

impl PgRevisionHydrator {
    async fn hydrate_references(
        &self,
        context: &RequestContext,
        references: &[RevisionRef],
        request: Option<&NormalizedRetrievalRequest>,
    ) -> Result<Vec<HydratedRevision>, ApplicationError> {
        if references.is_empty() && request.is_none() {
            return Ok(Vec::new());
        }

        let revision_ids: Vec<Uuid> = references
            .iter()
            .map(|reference| reference.revision_id.as_uuid())
            .collect();
        let memory_ids: Vec<Uuid> = references
            .iter()
            .map(|reference| reference.memory_id.as_uuid())
            .collect();

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        if let Some(request) = request {
            super::text_retriever::validate_canonical_retrieval_pin(&mut scoped, context, request)
                .await?;
        }
        let canonical_predicate = if request.is_some() {
            "AND m.status <> 'deleted' AND m.status=ANY($7) AND (cardinality($8::text[])=0 OR m.kind=ANY($8))
             AND EXISTS (SELECT 1 FROM vestrace_resolve_embedding_memory_references($1,$4,NULL) represented
                         WHERE represented.memory_id=m.id AND represented.revision_id=r.id)
             AND (CASE $5::text WHEN 'current' THEN r.id=m.active_revision_id
                  WHEN 'as_of' THEN r.id=(SELECT historical.id FROM memory_revisions historical
                    WHERE historical.workspace_id=m.workspace_id AND historical.memory_id=m.id
                      AND (historical.valid_from IS NULL OR historical.valid_from <= $6)
                      AND (historical.valid_until IS NULL OR $6 < historical.valid_until)
                    ORDER BY historical.revision_number DESC,historical.created_at DESC LIMIT 1)
                  ELSE TRUE END) AND ($6::timestamptz IS NULL OR TRUE)
             FOR SHARE OF m,r"
        } else {
            ""
        };

        // Selected by revision id, and additionally constrained to the memory
        // the reference named.
        //
        // The constraint that matters is the absence of anything resembling
        // `ORDER BY revision_number DESC LIMIT 1`: this must return the
        // revision that was asked for or no row at all. Falling back to the
        // memory's current revision is how a caller asking about the past gets
        // served the present, with the reference in hand as apparent proof that
        // it did not happen.
        //
        // The memory status is joined in because a reader has to be able to
        // tell retired content from current content, and the revision row does
        // not carry it.
        let sql = format!(
            r#"
            SELECT r.id,
                   r.memory_id,
                   r.revision_number,
                   r.content,
                   r.classification,
                   r.valid_from,
                   r.valid_until,
                   r.created_at,
                   m.status AS memory_status
            FROM memory_revisions r
            JOIN memories m
              ON m.id = r.memory_id
             AND m.workspace_id = r.workspace_id
            WHERE r.workspace_id = $1
              AND EXISTS (SELECT 1 FROM unnest($2::uuid[],$3::uuid[]) requested(revision_id,memory_id)
                          WHERE requested.revision_id=r.id AND requested.memory_id=r.memory_id)
              {canonical_predicate}
            "#
        );
        let mut query = sqlx::query(&sql)
            .bind(context.workspace_id.as_uuid())
            .bind(&revision_ids)
            .bind(&memory_ids);
        if let Some(request) = request {
            let (perspective, at) = match request.time_perspective {
                vestrace_domain::TimePerspective::Current => ("current", None),
                vestrace_domain::TimePerspective::AsOf(at) => ("as_of", Some(at)),
                vestrace_domain::TimePerspective::Timeline => ("timeline", None),
                vestrace_domain::TimePerspective::AllHistory => ("all_history", None),
            };
            query = query
                .bind(request.corpus_generation_id.as_uuid())
                .bind(perspective)
                .bind(at)
                .bind(super::embedding_retrieval_client::statuses(request))
                .bind(super::embedding_retrieval_client::kinds(request));
        }
        let rows = query
            .fetch_all(scoped.connection())
            .await
            .map_err(storage_error)?;

        let mut hydrated = Vec::with_capacity(rows.len());
        for row in &rows {
            let memory_id = MemoryId::from_uuid(row.get("memory_id"));
            let revision_id = MemoryRevisionId::from_uuid(row.get("id"));

            // Keep an exact pair check at the conversion boundary as well as
            // in the SQL request relation.
            if !references
                .iter()
                .any(|r| r.memory_id == memory_id && r.revision_id == revision_id)
            {
                continue;
            }

            let status: String = row.get("memory_status");
            let memory_status = memory_status_from_str(&status)?;
            let classification: Option<String> = row.get("classification");
            MemoryRevision::validate_classification_shape(classification.as_deref())
                .map_err(storage_error)?;

            let revision_number: i32 = row.get("revision_number");
            hydrated.push(HydratedRevision {
                memory_id,
                revision_id,
                revision_number: revision_number.max(0) as u32,
                memory_status,
                content: row.get("content"),
                classification,
                valid_from: row.get("valid_from"),
                valid_until: row.get("valid_until"),
                created_at: row.get("created_at"),
            });
        }

        scoped.commit().await.map_err(storage_error)?;
        Ok(hydrated)
    }
}

#[async_trait]
impl RevisionHydrator for PgRevisionHydrator {
    async fn hydrate(
        &self,
        context: &RequestContext,
        references: &[RevisionRef],
    ) -> Result<Vec<HydratedRevision>, ApplicationError> {
        self.hydrate_references(context, references, None).await
    }
    async fn hydrate_for_retrieval(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
        references: &[RevisionRef],
    ) -> Result<Vec<HydratedRevision>, ApplicationError> {
        if request.embedding_space_key.is_canonical() {
            self.hydrate_references(context, references, Some(request))
                .await
        } else {
            self.hydrate(context, references).await
        }
    }
}
