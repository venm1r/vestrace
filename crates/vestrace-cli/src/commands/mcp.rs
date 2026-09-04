use std::sync::Arc;

use vestrace_application::{DenyAllPolicyEngine, MemoryService, RetrievalService};
use vestrace_infrastructure::{
    AppConfig, PgAgentRepository, PgEvaluationRepository, PgEventRepository,
    PgExecutionHistoryRepository, PgIdempotencyRepository, PgMemoryRepository, PgModelRepository,
    PgOutboxRepository, PgProvenanceRepository, PgRelationRepository, PgRetrievalJournal,
    PgRevisionHydrator, PgSkillRepository, PgStore, PgTextRetriever, PgWorkflowRepository,
};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let retrieval_policy = config
        .retrieval_classification_policy()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow::anyhow!("database is unavailable"))?;
    match store.migrations_are_compatible().await {
        Ok(true) => {}
        Ok(false) => {
            return Err(anyhow::anyhow!(
                "database migration history is incompatible"
            ));
        }
        Err(_) => {
            return Err(anyhow::anyhow!(
                "database migration verification is unavailable"
            ));
        }
    }

    let store_for_journal = store.clone();
    let store_for_memory = store.clone();

    let memory_service = MemoryService::new(
        PgEventRepository::new(store_for_memory.clone()),
        PgMemoryRepository::new(store_for_memory.clone()),
        PgProvenanceRepository::new(store_for_memory.clone()),
        PgRelationRepository::new(store_for_memory.clone()),
        PgOutboxRepository::new(store.clone()),
        PgIdempotencyRepository::new(store.clone()),
        config
            .memory_label_vocabulary()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    );
    let memory_use_cases: vestrace_application::SharedMemoryUseCases = Arc::new(memory_service);

    let text_retriever: vestrace_application::SharedTextRetriever =
        Arc::new(PgTextRetriever::new(store.clone()));
    let retrieval_journal: vestrace_application::SharedRetrievalJournal =
        Arc::new(PgRetrievalJournal::new(store_for_journal));
    let retrieval_service = Arc::new(
        RetrievalService::new(text_retriever, retrieval_journal).with_hydration(
            Arc::new(PgRevisionHydrator::new(store.clone())),
            retrieval_policy,
            config.policy.version.clone(),
        ),
    );

    let model_repository: vestrace_application::SharedModelRepository =
        Arc::new(PgModelRepository::new(store.clone()));
    let agent_repository: vestrace_application::SharedAgentRepository =
        Arc::new(PgAgentRepository::new(store.clone()));
    let skill_repository: vestrace_application::SharedSkillRepository =
        Arc::new(PgSkillRepository::new(store.clone()));
    let workflow_repository: vestrace_application::SharedWorkflowRepository =
        Arc::new(PgWorkflowRepository::new(store.clone()));
    let evaluation_repository: vestrace_application::SharedEvaluationRepository =
        Arc::new(PgEvaluationRepository::new(store.clone()));
    let execution_history_repository: vestrace_application::SharedExecutionHistoryRepository =
        Arc::new(PgExecutionHistoryRepository::new(store.clone()));

    let server = vestrace_mcp::McpServer::new_with_policy(
        memory_use_cases,
        retrieval_service,
        model_repository,
        agent_repository,
        skill_repository,
        workflow_repository,
        evaluation_repository,
        execution_history_repository,
        Arc::new(DenyAllPolicyEngine),
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
