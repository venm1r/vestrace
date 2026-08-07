use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{
    ApplicationError, EvaluationRecord, EvaluationRepository, RequestContext,
    WorkflowDefinitionRecord, WorkflowRepository, WorkflowRevisionRecord,
};
use vestrace_domain::id::{EvaluationId, WorkflowId, WorkflowRevisionId, WorkspaceId};

pub struct PgWorkflowRepository {
    pool: PgPool,
}

impl PgWorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl WorkflowRepository for PgWorkflowRepository {
    async fn create(
        &self,
        _context: &RequestContext,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO workflow_definitions (id, workspace_id, name, current_revision, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(&record.name)
        .bind(record.current_revision as i32)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, name, current_revision, created_at
            FROM workflow_definitions
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| WorkflowDefinitionRecord {
                id: WorkflowId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                name: r.get("name"),
                current_revision: r.get::<i32, _>("current_revision") as u32,
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn find_by_id(
        &self,
        _context: &RequestContext,
        id: WorkflowId,
    ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, name, current_revision, created_at
            FROM workflow_definitions
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|r| WorkflowDefinitionRecord {
            id: WorkflowId::from_uuid(r.get("id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            name: r.get("name"),
            current_revision: r.get::<i32, _>("current_revision") as u32,
            created_at: r.get("created_at"),
        }))
    }

    async fn save_revision(
        &self,
        _context: &RequestContext,
        record: &WorkflowRevisionRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO workflow_revisions (revision_id, workflow_id, workspace_id, revision_number, definition, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(record.revision_id.as_uuid())
        .bind(record.workflow_id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.revision_number as i32)
        .bind(&record.definition)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn get_revision(
        &self,
        _context: &RequestContext,
        workflow_id: WorkflowId,
        revision_number: u32,
    ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT revision_id, workflow_id, workspace_id, revision_number, definition, created_at
            FROM workflow_revisions
            WHERE workflow_id = $1 AND revision_number = $2
            "#,
        )
        .bind(workflow_id.as_uuid())
        .bind(revision_number as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|r| WorkflowRevisionRecord {
            revision_id: WorkflowRevisionId::from_uuid(r.get("revision_id")),
            workflow_id: WorkflowId::from_uuid(r.get("workflow_id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            revision_number: r.get::<i32, _>("revision_number") as u32,
            definition: r.get("definition"),
            created_at: r.get("created_at"),
        }))
    }
}

pub struct PgEvaluationRepository {
    pool: PgPool,
}

impl PgEvaluationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EvaluationRepository for PgEvaluationRepository {
    async fn create(
        &self,
        _context: &RequestContext,
        record: &EvaluationRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO evaluations (id, workspace_id, model_id, name, status, score, summary, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.model_id.map(|m| m.as_uuid()))
        .bind(&record.name)
        .bind(&record.status)
        .bind(record.score)
        .bind(&record.summary)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<EvaluationRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, model_id, name, status, score, summary, created_at
            FROM evaluations
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| EvaluationRecord {
                id: EvaluationId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                model_id: r
                    .get::<Option<uuid::Uuid>, _>("model_id")
                    .map(vestrace_domain::id::ModelId::from_uuid),
                name: r.get("name"),
                status: r.get("status"),
                score: r.get::<Option<f64>, _>("score"),
                summary: r.get("summary"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn find_by_id(
        &self,
        _context: &RequestContext,
        id: EvaluationId,
    ) -> Result<Option<EvaluationRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, model_id, name, status, score, summary, created_at
            FROM evaluations
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|r| EvaluationRecord {
            id: EvaluationId::from_uuid(r.get("id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            model_id: r
                .get::<Option<uuid::Uuid>, _>("model_id")
                .map(vestrace_domain::id::ModelId::from_uuid),
            name: r.get("name"),
            status: r.get("status"),
            score: r.get::<Option<f64>, _>("score"),
            summary: r.get("summary"),
            created_at: r.get("created_at"),
        }))
    }
}
