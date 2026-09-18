use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{AgentRecord, AgentRepository, ApplicationError, RequestContext};

pub struct PgAgentRepository {
    store: PgStore,
}

impl PgAgentRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

use super::PgStore;

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl AgentRepository for PgAgentRepository {
    async fn create(
        &self,
        context: &RequestContext,
        agent: &AgentRecord,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            r#"
            INSERT INTO agents (id, workspace_id, name, description, system_prompt, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(agent.id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(&agent.name)
        .bind(&agent.description)
        .bind(&agent.system_prompt)
        .bind(agent.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)
    }

    async fn list(&self, context: &RequestContext) -> Result<Vec<AgentRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, name, description, system_prompt, created_at
            FROM agents
            WHERE workspace_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let mut agents = Vec::new();
        for row in rows {
            agents.push(AgentRecord {
                id: vestrace_domain::id::AgentId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                name: row.try_get("name").map_err(storage_error)?,
                description: row.try_get("description").map_err(storage_error)?,
                system_prompt: row.try_get("system_prompt").map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }
        scoped.commit().await.map_err(storage_error)?;
        Ok(agents)
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: vestrace_domain::id::AgentId,
    ) -> Result<Option<AgentRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, name, description, system_prompt, created_at
            FROM agents
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        // Committed before decoding: the read is done, and holding the
        // transaction open across the mapping keeps a pool connection
        // checked out for no reason.
        scoped.commit().await.map_err(storage_error)?;

        match row {
            Some(row) => Ok(Some(AgentRecord {
                id: vestrace_domain::id::AgentId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                name: row.try_get("name").map_err(storage_error)?,
                description: row.try_get("description").map_err(storage_error)?,
                system_prompt: row.try_get("system_prompt").map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            })),
            None => Ok(None),
        }
    }
}
