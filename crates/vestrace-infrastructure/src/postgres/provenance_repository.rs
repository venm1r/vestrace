use async_trait::async_trait;
use vestrace_application::{ApplicationError, ProvenanceRepository, RequestContext};
use vestrace_domain::MemorySource;

use super::PgStore;

pub struct PgProvenanceRepository {
    store: PgStore,
}

impl PgProvenanceRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl ProvenanceRepository for PgProvenanceRepository {
    async fn save_source(
        &self,
        context: &RequestContext,
        source: &MemorySource,
    ) -> Result<(), ApplicationError> {
        if source.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a memory source cannot be written into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let role_str = super::memory_encoding::evidence_role_str(source.role);

        let evidence_ref_json = source
            .evidence_ref
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO memory_sources (id, memory_id, workspace_id, event_id, role, derivation_id, created_at, evidence_ref)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(source.id.as_uuid())
        .bind(source.memory_id.as_uuid())
        .bind(source.workspace_id.as_uuid())
        .bind(source.event_id.as_uuid())
        .bind(role_str)
        .bind(source.derivation_id.map(|d| d.as_uuid()))
        .bind(source.created_at)
        .bind(evidence_ref_json)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }
}
