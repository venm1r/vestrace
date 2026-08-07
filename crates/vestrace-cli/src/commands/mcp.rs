use std::sync::Arc;

use vestrace_application::{MemoryService, RetrievalService};
use vestrace_infrastructure::{
    AppConfig, PgAgentRepository, PgEvaluationRepository, PgEventRepository,
    PgExecutionHistoryRepository, PgIdempotencyRepository, PgMemoryRepository, PgModelRepository,
    PgOutboxRepository, PgProvenanceRepository, PgRelationRepository, PgRetrievalJournal,
    PgSkillRepository, PgStore, PgTextRetriever, PgWorkflowRepository,
};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow::anyhow!("database is unavailable"))?;
    store
        .migrate()
        .await
        .map_err(|_| anyhow::anyhow!("database migrations are unavailable"))?;

    let pool = store.pool().clone();

    let memory_service = MemoryService::new(
        PgEventRepository::new(pool.clone()),
        PgMemoryRepository::new(pool.clone()),
        PgProvenanceRepository::new(pool.clone()),
        PgRelationRepository::new(pool.clone()),
        PgOutboxRepository::new(pool.clone()),
        PgIdempotencyRepository::new(pool.clone()),
    );
    let memory_use_cases: vestrace_application::SharedMemoryUseCases = Arc::new(memory_service);

    let text_retriever: vestrace_application::SharedTextRetriever =
        Arc::new(PgTextRetriever::new(pool.clone()));
    let retrieval_journal: vestrace_application::SharedRetrievalJournal =
        Arc::new(PgRetrievalJournal::new(pool.clone()));
    let retrieval_service = Arc::new(RetrievalService::new(text_retriever, retrieval_journal));

    let model_repository: vestrace_application::SharedModelRepository =
        Arc::new(PgModelRepository::new(pool.clone()));
    let agent_repository: vestrace_application::SharedAgentRepository =
        Arc::new(PgAgentRepository::new(pool.clone()));
    let skill_repository: vestrace_application::SharedSkillRepository =
        Arc::new(PgSkillRepository::new(pool.clone()));
    let workflow_repository: vestrace_application::SharedWorkflowRepository =
        Arc::new(PgWorkflowRepository::new(pool.clone()));
    let evaluation_repository: vestrace_application::SharedEvaluationRepository =
        Arc::new(PgEvaluationRepository::new(pool.clone()));
    let execution_history_repository: vestrace_application::SharedExecutionHistoryRepository =
        Arc::new(PgExecutionHistoryRepository::new(pool));

    let server = vestrace_mcp::McpServer::new(
        memory_use_cases,
        retrieval_service,
        model_repository,
        agent_repository,
        skill_repository,
        workflow_repository,
        evaluation_repository,
        execution_history_repository,
    );

    tracing::info!("mcp server started (stdio mode)");

    let tools = server.list_tools();
    let tools_json = serde_json::to_string_pretty(&serde_json::json!({
        "tools": tools.iter().map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema,
        })).collect::<Vec<_>>()
    }))
    .map_err(|e| anyhow::anyhow!("failed to serialize tools: {e}"))?;

    println!("{tools_json}");

    Ok(())
}
