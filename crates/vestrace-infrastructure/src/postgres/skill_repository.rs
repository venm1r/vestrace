use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, RequestContext, SkillRecord, SkillRepository};

pub struct PgSkillRepository {
    store: PgStore,
}

impl PgSkillRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

use super::PgStore;

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl SkillRepository for PgSkillRepository {
    async fn create(
        &self,
        context: &RequestContext,
        skill: &SkillRecord,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            r#"
            INSERT INTO skills (id, workspace_id, name, instructions, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(skill.id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(&skill.name)
        .bind(&skill.instructions)
        .bind(skill.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)
    }

    async fn list(&self, context: &RequestContext) -> Result<Vec<SkillRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, name, instructions, created_at
            FROM skills
            WHERE workspace_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let mut skills = Vec::new();
        for row in rows {
            skills.push(SkillRecord {
                id: vestrace_domain::id::SkillId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                name: row.try_get("name").map_err(storage_error)?,
                instructions: row.try_get("instructions").map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }
        scoped.commit().await.map_err(storage_error)?;
        Ok(skills)
    }
}
