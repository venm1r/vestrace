use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::retrieval::MemoryTextSource;
use vestrace_application::{ApplicationError, OutboxMessage, OutboxRepository, RequestContext};
use vestrace_domain::id::MemoryId;
use vestrace_domain::{WorkspaceId, id::OutboxId};

use super::PgStore;

/// The outbox.
///
/// # What changed
///
/// `claim_pending` carried `FOR UPDATE SKIP LOCKED` while running through a bare
/// pool, so the row lock was released by the statement's own implicit commit and
/// the clause provided no concurrency control at all. It also selected across
/// every workspace, which is why it could not be scoped: a caller had no
/// workspace to scope it to.
///
/// Both are fixed: the query is workspace-scoped inside a scoped transaction
/// like every other adapter, and the misleading clause is gone in favour of the
/// at-least-once contract the port now states.
pub struct PgOutboxRepository {
    store: PgStore,
}

impl PgOutboxRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn parse_outbox_row(row: sqlx::postgres::PgRow) -> Result<OutboxMessage, ApplicationError> {
    let attempts: i32 = row.try_get("attempts").map_err(storage_error)?;
    Ok(OutboxMessage {
        id: OutboxId::from_uuid(row.try_get("id").map_err(storage_error)?),
        workspace_id: WorkspaceId::from_uuid(row.try_get("workspace_id").map_err(storage_error)?),
        topic: row.try_get("topic").map_err(storage_error)?,
        payload: row.try_get("payload").map_err(storage_error)?,
        created_at: row.try_get("created_at").map_err(storage_error)?,
        attempts: u32::try_from(attempts).map_err(|_| {
            // The column is CHECKed non-negative, so this is unreachable
            // through the schema; it is reported rather than clamped, because
            // an attempt count that silently became 0 would restart a
            // dead-lettered message's budget.
            ApplicationError::Storage(format!("outbox attempt count {attempts} is negative"))
        })?,
    })
}

#[async_trait]
impl OutboxRepository for PgOutboxRepository {
    async fn save(
        &self,
        context: &RequestContext,
        message: &OutboxMessage,
    ) -> Result<(), ApplicationError> {
        // Scoped, because the table's policy is now FORCEd: on a bare pool
        // `vestrace.workspace_id` is unset and this insert would be refused.
        // That refusal is the policy working — the statement previously wrote a
        // workspace's row on a connection that had never said which workspace
        // it was acting for.
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // A message is born unattempted and immediately due; the remaining
        // columns take their defaults so a producer cannot set a delivery
        // history it has no business knowing about.
        sqlx::query(
            "INSERT INTO outbox (id, workspace_id, topic, payload, processed_at, created_at)
             VALUES ($1, $2, $3, $4, NULL, $5)",
        )
        .bind(message.id.as_uuid())
        .bind(message.workspace_id.as_uuid())
        .bind(&message.topic)
        .bind(&message.payload)
        .bind(message.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn claim_pending(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<OutboxMessage>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            "SELECT id, workspace_id, topic, payload, created_at, attempts
             FROM outbox
             WHERE workspace_id = $1
               AND processed_at IS NULL
               AND dead_lettered_at IS NULL
               AND next_attempt_at <= NOW()
             ORDER BY created_at ASC, id ASC
             LIMIT $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        rows.into_iter().map(parse_outbox_row).collect()
    }

    async fn mark_processed(
        &self,
        context: &RequestContext,
        id: OutboxId,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            "UPDATE outbox SET processed_at = NOW()
             WHERE id = $1 AND workspace_id = $2 AND processed_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn record_failure(
        &self,
        context: &RequestContext,
        id: OutboxId,
        error: &str,
        retry_after: std::time::Duration,
        dead_letter: bool,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The backoff is computed from the row's own attempt count rather than
        // passed as an absolute time, so two drains disagreeing about the clock
        // cannot make a message due earlier than its own history says.
        let retry_after = chrono::Duration::from_std(retry_after)
            .map_err(|_| ApplicationError::Internal("retry delay does not fit".into()))?;

        sqlx::query(
            "UPDATE outbox
             SET attempts = attempts + 1,
                 last_attempt_at = NOW(),
                 last_error = $3,
                 next_attempt_at = NOW() + $4,
                 dead_lettered_at = CASE WHEN $5 THEN NOW() ELSE dead_lettered_at END
             WHERE id = $1 AND workspace_id = $2 AND processed_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        // Bounded, because a provider that returns a large body in its error
        // would otherwise write it into every row it fails.
        .bind(error.chars().take(2000).collect::<String>())
        .bind(retry_after)
        .bind(dead_letter)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }
}

/// Reads the text a memory should be embedded from.
///
/// The active revision's content, which is the same text the search document
/// holds — one memory, one meaning, whichever channel is asked.
pub struct PgMemoryTextSource {
    store: PgStore,
}

impl PgMemoryTextSource {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl MemoryTextSource for PgMemoryTextSource {
    async fn active_text(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
    ) -> Result<Option<String>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            "SELECT r.content
             FROM memories m
             JOIN memory_revisions r ON r.id = m.active_revision_id
             WHERE m.id = $1 AND m.workspace_id = $2 AND m.status = 'active'",
        )
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.map(|row| row.try_get("content").map_err(storage_error))
            .transpose()
    }
}
