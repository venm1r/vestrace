use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{
    ApplicationError, NormalizedRetrievalRequest, RequestContext, TextRetriever,
};
use vestrace_domain::{RetrievalCandidate, id::MemoryId as DomainMemoryId};

pub struct PgTextRetriever {
    pool: PgPool,
}

impl PgTextRetriever {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl TextRetriever for PgTextRetriever {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
            .bind(context.workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT sd.memory_id, sd.content,
                   ts_rank(sd.fts_vector, plainto_tsquery('english', $2)) AS rank
            FROM search_documents sd
            INNER JOIN memories m ON m.id = sd.memory_id
            WHERE m.status = ANY($3)
              AND ($4::text[] IS NULL OR m.kind = ANY($4))
            ORDER BY rank DESC
            LIMIT $5
            "#,
        )
        .bind(&request.query)
        .bind(
            request
                .allowed_statuses
                .iter()
                .map(|s| match s {
                    vestrace_domain::MemoryStatus::Active => "active",
                    vestrace_domain::MemoryStatus::Candidate => "candidate",
                    vestrace_domain::MemoryStatus::Superseded => "superseded",
                    vestrace_domain::MemoryStatus::Rejected => "rejected",
                    vestrace_domain::MemoryStatus::Expired => "expired",
                    vestrace_domain::MemoryStatus::Deleted => "deleted",
                })
                .collect::<Vec<&str>>(),
        )
        .bind(
            request
                .allowed_kinds
                .iter()
                .map(|k| match k {
                    vestrace_domain::MemoryKind::Fact => "fact",
                    vestrace_domain::MemoryKind::Preference => "preference",
                    vestrace_domain::MemoryKind::Constraint => "constraint",
                    vestrace_domain::MemoryKind::Decision => "decision",
                    vestrace_domain::MemoryKind::Task => "task",
                    vestrace_domain::MemoryKind::Procedure => "procedure",
                    vestrace_domain::MemoryKind::Observation => "observation",
                    vestrace_domain::MemoryKind::Outcome => "outcome",
                    vestrace_domain::MemoryKind::Summary => "summary",
                })
                .collect::<Vec<&str>>(),
        )
        .bind(i64::from(request.channel_limit))
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)?;

        let candidates = rows
            .into_iter()
            .enumerate()
            .map(|(idx, row)| {
                let memory_id: uuid::Uuid = row.try_get("memory_id").map_err(storage_error)?;
                let rank: f32 = row.try_get("rank").map_err(storage_error)?;
                Ok::<_, ApplicationError>(RetrievalCandidate {
                    memory_id: DomainMemoryId::from_uuid(memory_id),
                    revision_id: None,
                    score: rank,
                    channel_rank: (idx + 1) as u32,
                    channel: "text".to_owned(),
                    explanation: "FTS match".to_owned(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(candidates)
    }
}
