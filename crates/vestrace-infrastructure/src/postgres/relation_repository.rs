use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, RelationRepository};
use vestrace_domain::KnowledgeRelation;

pub struct PgRelationRepository {
    pool: PgPool,
}

impl PgRelationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RelationRepository for PgRelationRepository {
    async fn save_relation(&mut self, relation: &KnowledgeRelation) -> Result<(), ApplicationError> {
        let type_str = match relation.relation_type {
            vestrace_domain::RelationType::Supports => "supports",
            vestrace_domain::RelationType::Contradicts => "contradicts",
            vestrace_domain::RelationType::Extends => "extends",
            vestrace_domain::RelationType::Refines => "refines",
            vestrace_domain::RelationType::DerivedFrom => "derived_from",
            vestrace_domain::RelationType::RelatesTo => "relates_to",
        };

        sqlx::query(
            r#"
            INSERT INTO knowledge_relations (id, workspace_id, source_memory_id, target_memory_id, relation_type, confidence, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(relation.id.as_uuid())
        .bind(relation.workspace_id.as_uuid())
        .bind(relation.source_memory_id.as_uuid())
        .bind(relation.target_memory_id.as_uuid())
        .bind(type_str)
        .bind(relation.confidence.value())
        .bind(relation.created_at.as_datetime())
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::StorageFailure(e.to_string()))?;

        Ok(())
    }
}
