use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, NormalizedRetrievalRequest, RequestContext, TextRetriever,
};
use vestrace_domain::{
    CorpusGenerationId, MemoryKind, MemoryStatus, RetrievalCandidate, TimePerspective,
    id::MemoryId as DomainMemoryId,
};

use super::PgStore;

/// The text retrieval channel.
///
/// # Why this holds a store and not a pool
///
/// This adapter was already correct: it opened a transaction and set
/// `vestrace.workspace_id` on it by hand, which is what `begin_scoped` does. It
/// was also the only place doing it by hand, and a second way to establish the
/// scope is a second place for it to drift — the principal id was not set, for
/// instance, which no policy reads today and one might.
pub struct PgTextRetriever {
    store: PgStore,
}

impl PgTextRetriever {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

pub(super) fn memory_kind_from_str(value: &str) -> Result<MemoryKind, ApplicationError> {
    match value {
        "fact" => Ok(MemoryKind::Fact),
        "preference" => Ok(MemoryKind::Preference),
        "constraint" => Ok(MemoryKind::Constraint),
        "decision" => Ok(MemoryKind::Decision),
        "task" => Ok(MemoryKind::Task),
        "procedure" => Ok(MemoryKind::Procedure),
        "observation" => Ok(MemoryKind::Observation),
        "outcome" => Ok(MemoryKind::Outcome),
        "summary" => Ok(MemoryKind::Summary),
        _ => Err(ApplicationError::Storage(format!(
            "stored memory kind '{value}' is not supported"
        ))),
    }
}

pub(super) fn memory_status_from_str(value: &str) -> Result<MemoryStatus, ApplicationError> {
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

struct TemporalQueryPlan {
    revision_join: &'static str,
    rank_expression: &'static str,
    /// The predicate that decides whether a row is a candidate at all.
    ///
    /// It is always the same text the rank is computed over. Ranking one
    /// expression and filtering another lets a row be admitted on a match its
    /// score knows nothing about, which is how a channel starts returning
    /// results it cannot explain.
    match_expression: &'static str,
    order_by: &'static str,
    as_of: Option<chrono::DateTime<chrono::Utc>>,
}

fn temporal_query_plan(perspective: TimePerspective) -> TemporalQueryPlan {
    match perspective {
        TimePerspective::Current => TemporalQueryPlan {
            revision_join: r#"INNER JOIN memory_revisions mr
                    ON mr.id = m.active_revision_id
                   AND mr.memory_id = m.id
                   AND mr.workspace_id = m.workspace_id"#,
            rank_expression: "ts_rank(sd.fts_vector, plainto_tsquery('english', $2))",
            match_expression: "sd.fts_vector @@ plainto_tsquery('english', $2)",
            order_by: "rank DESC, mr.revision_number DESC",
            as_of: None,
        },
        TimePerspective::AsOf(at) => TemporalQueryPlan {
            revision_join: r#"INNER JOIN LATERAL (
                    SELECT historical_mr.id, historical_mr.revision_number,
                           historical_mr.content, historical_mr.valid_from,
                           historical_mr.valid_until, historical_mr.created_at
                    FROM memory_revisions historical_mr
                    WHERE historical_mr.memory_id = m.id
                      AND historical_mr.workspace_id = m.workspace_id
                      AND (historical_mr.valid_from IS NULL OR historical_mr.valid_from <= $7)
                      AND (historical_mr.valid_until IS NULL OR $7 < historical_mr.valid_until)
                    ORDER BY historical_mr.revision_number DESC, historical_mr.created_at DESC
                    LIMIT 1
                ) mr ON TRUE"#,
            rank_expression: "ts_rank(to_tsvector('english', mr.content), plainto_tsquery('english', $2))",
            match_expression: "to_tsvector('english', mr.content) @@ plainto_tsquery('english', $2)",
            order_by: "rank DESC, mr.revision_number DESC",
            as_of: Some(at),
        },
        TimePerspective::Timeline => TemporalQueryPlan {
            revision_join: r#"INNER JOIN memory_revisions mr
                    ON mr.memory_id = m.id
                   AND mr.workspace_id = m.workspace_id"#,
            rank_expression: "ts_rank(to_tsvector('english', mr.content), plainto_tsquery('english', $2))",
            match_expression: "to_tsvector('english', mr.content) @@ plainto_tsquery('english', $2)",
            order_by: "mr.created_at ASC, mr.revision_number ASC, rank DESC",
            as_of: None,
        },
        TimePerspective::AllHistory => TemporalQueryPlan {
            revision_join: r#"INNER JOIN memory_revisions mr
                    ON mr.memory_id = m.id
                   AND mr.workspace_id = m.workspace_id"#,
            rank_expression: "ts_rank(to_tsvector('english', mr.content), plainto_tsquery('english', $2))",
            match_expression: "to_tsvector('english', mr.content) @@ plainto_tsquery('english', $2)",
            order_by: "mr.created_at DESC, mr.revision_number DESC, rank DESC",
            as_of: None,
        },
    }
}

pub(super) async fn validate_canonical_retrieval_pin(
    scoped: &mut super::PgScopedTransaction,
    context: &RequestContext,
    request: &NormalizedRetrievalRequest,
) -> Result<(), ApplicationError> {
    let identity = request
        .embedding_space_key
        .canonical_identity()
        .ok_or_else(|| {
            ApplicationError::Policy("text retrieval requires a canonical space".to_owned())
        })?;
    if request.embedding_space_key.workspace_id() != context.workspace_id {
        return Err(ApplicationError::Policy(
            "text retrieval workspace differs from canonical space".to_owned(),
        ));
    }
    let exact: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM embedding_corpus_generations generation \
             JOIN embedding_space_registrations registration ON registration.workspace_id=generation.workspace_id \
              AND registration.id=generation.space_registration_id \
             WHERE generation.workspace_id=$1 AND generation.id=$2 AND registration.registration_kind='canonical' \
              AND registration.name=$3 AND registration.model_revision_id=$4 AND registration.model_qualification_revision_id=$5 \
              AND registration.request_shape_revision_id=$6 AND registration.adapter_profile_revision=$7 \
              AND registration.returned_model=$8 AND registration.encoding_format=$9 AND registration.dimensions=$10)")
            .bind(context.workspace_id.as_uuid()).bind(request.corpus_generation_id.as_uuid())
            .bind(request.embedding_space_key.name()).bind(identity.model_revision_id.as_uuid())
            .bind(identity.model_qualification_revision_id.as_uuid()).bind(identity.request_shape_revision_id)
            .bind(&identity.adapter_profile_revision).bind(&identity.returned_model).bind(&identity.encoding_format)
            .bind(i64::from(identity.dimensions)).fetch_one(scoped.connection()).await.map_err(storage_error)?;
    if !exact {
        return Err(ApplicationError::Policy(
            "text retrieval generation differs from exact canonical space".to_owned(),
        ));
    }
    sqlx::query(
        "SELECT * FROM vestrace_resolve_embedding_memory_references($1,$2,ARRAY[]::uuid[])",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(request.corpus_generation_id.as_uuid())
    .fetch_all(scoped.connection())
    .await
    .map_err(storage_error)?;

    Ok(())
}

#[async_trait]
impl TextRetriever for PgTextRetriever {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        validate_canonical_retrieval_pin(&mut scoped, context, request).await?;

        let TemporalQueryPlan {
            revision_join,
            rank_expression,
            match_expression,
            order_by,
            as_of,
        } = temporal_query_plan(request.time_perspective);

        // `{match_expression}` is the line that makes this a search.
        //
        // Without it the query text reached only `ts_rank` in the SELECT list,
        // so every active memory in the workspace was a candidate for every
        // query, scored zero when it did not match. Three memories and a
        // `LIMIT` hid it: a search for a word present in none of them returned
        // all three, with exactly the scores a search for a word present in one
        // of them returned. `search_documents.fts_vector` — the column that
        // exists for this and is indexed for it — was never read.
        let query = format!(
            r#"
            SELECT sd.memory_id, m.kind AS memory_kind, m.status AS memory_status,
                   m.state_revision AS source_generation, mr.id AS revision_id,
                   mr.revision_number, mr.content, mr.valid_from, mr.valid_until,
                   mr.created_at AS revision_created_at,
                   $6::uuid AS corpus_generation_id,
                   {rank_expression} AS rank
            FROM search_documents sd
            INNER JOIN memories m ON m.id = sd.memory_id
            {revision_join}
            WHERE sd.workspace_id = $1
              AND m.workspace_id = $1
              AND m.status = ANY($3)
              AND (cardinality($4::text[]) = 0 OR m.kind = ANY($4))
              AND m.status <> 'deleted'
              AND EXISTS (SELECT 1 FROM vestrace_resolve_embedding_memory_references($1,$6,NULL) represented
                          WHERE represented.memory_id = m.id AND represented.revision_id = mr.id)
              AND {match_expression}
            ORDER BY {order_by}
            LIMIT $5
            "#,
        );

        let mut query = sqlx::query(&query)
            .bind(context.workspace_id.as_uuid())
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
            .bind(request.corpus_generation_id.as_uuid());

        if let Some(at) = as_of {
            query = query.bind(at);
        }

        let rows = query
            .fetch_all(scoped.connection())
            .await
            .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        let candidates = rows
            .into_iter()
            .enumerate()
            .map(|(idx, row)| {
                let memory_id: uuid::Uuid = row.try_get("memory_id").map_err(storage_error)?;
                let kind: String = row.try_get("memory_kind").map_err(storage_error)?;
                let status: String = row.try_get("memory_status").map_err(storage_error)?;
                let source_generation: i32 =
                    row.try_get("source_generation").map_err(storage_error)?;
                let rank: f32 = row.try_get("rank").map_err(storage_error)?;
                let revision_id: uuid::Uuid = row.try_get("revision_id").map_err(storage_error)?;
                let revision_number: i32 = row.try_get("revision_number").map_err(storage_error)?;
                let content: String = row.try_get("content").map_err(storage_error)?;
                let valid_from = row.try_get("valid_from").map_err(storage_error)?;
                let valid_until = row.try_get("valid_until").map_err(storage_error)?;
                let revision_created_at =
                    row.try_get("revision_created_at").map_err(storage_error)?;

                Ok::<_, ApplicationError>(RetrievalCandidate {
                    memory_id: DomainMemoryId::from_uuid(memory_id),
                    revision_id: vestrace_domain::id::MemoryRevisionId::from_uuid(revision_id),
                    kind: memory_kind_from_str(&kind)?,
                    memory_status: memory_status_from_str(&status)?,
                    revision_number: u32::try_from(revision_number)
                        .map_err(|e| ApplicationError::Storage(e.to_string()))?,
                    content,
                    classification: None,
                    valid_from,
                    valid_until,
                    revision_created_at,
                    source_generation: u32::try_from(source_generation)
                        .map_err(|e| ApplicationError::Storage(e.to_string()))?,
                    corpus_generation_id: CorpusGenerationId::from_uuid(
                        row.try_get("corpus_generation_id").map_err(storage_error)?,
                    ),
                    score: rank,
                    channel_rank: (idx + 1) as u32,
                    channel: "text".to_owned(),
                    explanation: "FTS match".to_owned(),
                    conflict_ids: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::temporal_query_plan;
    use chrono::{TimeZone, Utc};
    use vestrace_domain::TimePerspective;

    #[test]
    fn current_plan_joins_only_the_active_revision() {
        let plan = temporal_query_plan(TimePerspective::Current);

        assert!(plan.revision_join.contains("m.active_revision_id"));
        assert!(plan.rank_expression.contains("sd.fts_vector"));
        assert!(plan.as_of.is_none());
    }

    #[test]
    fn as_of_plan_uses_a_validity_window_and_historical_content() {
        let at = Utc.with_ymd_and_hms(2026, 8, 11, 12, 0, 0).unwrap();
        let plan = temporal_query_plan(TimePerspective::AsOf(at));

        assert!(plan.revision_join.contains("valid_from"));
        assert!(plan.revision_join.contains("valid_until"));
        assert!(plan.rank_expression.contains("mr.content"));
        assert_eq!(plan.as_of, Some(at));
    }

    #[test]
    fn timeline_and_history_have_explicit_revision_ordering() {
        let timeline = temporal_query_plan(TimePerspective::Timeline);
        let history = temporal_query_plan(TimePerspective::AllHistory);

        assert!(timeline.order_by.starts_with("mr.created_at ASC"));
        assert!(history.order_by.starts_with("mr.created_at DESC"));
    }
}
