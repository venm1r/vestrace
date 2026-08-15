use async_trait::async_trait;
use vestrace_application::{ApplicationError, RelationRepository, RequestContext};
use vestrace_domain::KnowledgeRelation;

use super::PgStore;

pub struct PgRelationRepository {
    store: PgStore,
}

impl PgRelationRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl RelationRepository for PgRelationRepository {
    async fn save_relation(
        &self,
        context: &RequestContext,
        relation: &KnowledgeRelation,
    ) -> Result<(), ApplicationError> {
        if relation.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a knowledge relation cannot be written into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

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
        .bind(relation.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }
}
