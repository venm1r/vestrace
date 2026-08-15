use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, RequestContext, RoutingDecisionRecord, RoutingDecisionRepository,
};

pub struct PgRoutingDecisionRepository {
    store: PgStore,
}

impl PgRoutingDecisionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

use super::PgStore;

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl RoutingDecisionRepository for PgRoutingDecisionRepository {
    async fn record(
        &self,
        context: &RequestContext,
        decision: &RoutingDecisionRecord,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            r#"
            INSERT INTO routing_decisions (id, workspace_id, selected_model_id, intent, rationale, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(decision.id.as_uuid())
        .bind(decision.workspace_id.as_uuid())
        .bind(decision.selected_model_id.map(|m| m.as_uuid()))
        .bind(&decision.intent)
        .bind(&decision.rationale)
        .bind(decision.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<RoutingDecisionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, selected_model_id, intent, rationale, created_at
            FROM routing_decisions
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let mut decisions = Vec::new();
        for row in rows {
            let selected_model_id: Option<uuid::Uuid> =
                row.try_get("selected_model_id").map_err(storage_error)?;
            decisions.push(RoutingDecisionRecord {
                id: vestrace_domain::id::RoutingDecisionId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                selected_model_id: selected_model_id.map(vestrace_domain::id::ModelId::from_uuid),
                intent: row.try_get("intent").map_err(storage_error)?,
                rationale: row.try_get("rationale").map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }
        scoped.commit().await.map_err(storage_error)?;
        Ok(decisions)
    }
}
