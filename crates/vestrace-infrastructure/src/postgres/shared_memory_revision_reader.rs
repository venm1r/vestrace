use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, SharedMemoryReadPermit, SharedMemoryRevisionReader,
    SharedMemoryRevisionRecord,
};
use vestrace_domain::id::{MemoryId, MemoryRevisionId, WorkspaceId};

use super::PgStore;

pub struct PgSharedMemoryRevisionReader {
    store: PgStore,
}

impl PgSharedMemoryRevisionReader {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl SharedMemoryRevisionReader for PgSharedMemoryRevisionReader {
    async fn read_exact(
        &self,
        permit: SharedMemoryReadPermit,
    ) -> Result<Option<SharedMemoryRevisionRecord>, ApplicationError> {
        let source_workspace_id = permit.source_workspace_id();
        let source_memory_id = permit.source_memory_id();
        let memory_revision_id = permit.memory_revision_id();
        let target_principal_id = permit.target_principal_id();

        let mut transaction = self.store.pool().begin().await.map_err(storage_error)?;

        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.workspace_id")
            .bind(source_workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;

        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.principal_id")
            .bind(target_principal_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;

        let active: bool =
            sqlx::query_scalar("SELECT row_security_active('memory_revisions'::regclass)")
                .fetch_one(&mut *transaction)
                .await
                .map_err(storage_error)?;

        if !active {
            return Err(ApplicationError::Storage(
                "row level security is inactive for shared memory reads".into(),
            ));
        }

        #[derive(sqlx::FromRow)]
        struct Row {
            workspace_id: uuid::Uuid,
            memory_id: uuid::Uuid,
            memory_revision_id: uuid::Uuid,
            content: String,
        }

        let row: Option<Row> = sqlx::query_as(
            "SELECT workspace_id, memory_id, id AS memory_revision_id, content
             FROM memory_revisions
             WHERE workspace_id = $1
               AND memory_id = $2
               AND id = $3",
        )
        .bind(source_workspace_id.as_uuid())
        .bind(source_memory_id.as_uuid())
        .bind(memory_revision_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(storage_error)?;

        let Some(row) = row else {
            transaction.commit().await.map_err(storage_error)?;
            return Ok(None);
        };

        let row_workspace_id = WorkspaceId::from_uuid(row.workspace_id);
        let row_memory_id = MemoryId::from_uuid(row.memory_id);
        let row_revision_id = MemoryRevisionId::from_uuid(row.memory_revision_id);

        if row_workspace_id != source_workspace_id
            || row_memory_id != source_memory_id
            || row_revision_id != memory_revision_id
        {
            return Err(ApplicationError::Storage(
                "shared memory reader selected a row outside the authorized namespace".into(),
            ));
        }

        transaction.commit().await.map_err(storage_error)?;

        Ok(Some(SharedMemoryRevisionRecord::new(
            row_workspace_id,
            row_memory_id,
            row_revision_id,
            row.content,
        )))
    }
}
