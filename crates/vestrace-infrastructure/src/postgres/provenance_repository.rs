use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, ProvenanceRepository};
use vestrace_domain::MemorySource;

pub struct PgProvenanceRepository {
    pool: PgPool,
}

impl PgProvenanceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProvenanceRepository for PgProvenanceRepository {
    async fn save_source(&self, source: &MemorySource) -> Result<(), ApplicationError> {
        let role_str = match source.role {
            vestrace_domain::EvidenceRole::DirectSource => "direct_source",
            vestrace_domain::EvidenceRole::SupportingContext => "supporting_context",
            vestrace_domain::EvidenceRole::ContradictingEvidence => "contradicting_evidence",
        };

        sqlx::query(
            r#"
            INSERT INTO memory_sources (id, memory_id, workspace_id, event_id, role, derivation_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(source.id.as_uuid())
        .bind(source.memory_id.as_uuid())
        .bind(source.workspace_id.as_uuid())
        .bind(source.event_id.as_uuid())
        .bind(role_str)
        .bind(source.derivation_id.map(|d| d.as_uuid()))
        .bind(source.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }
}
