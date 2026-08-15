use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vestrace_application::{
    AgentRepository, ApplicationError, AuthorizationBoundary, DenyAllPolicyEngine,
    EvaluationRepository, ExecutionHistoryRepository, MemoryUseCases, ModelRepository,
    RequestContext, RetrievalRequest, RetrievalService, SharedPolicyDecisionEngine,
    SkillRepository, WorkflowRepository,
};
use vestrace_domain::{
    AuthorizationRequest, Capability, PrincipalId, ResourceKind, ResourceScope, RiskCategory,
    WorkspaceId,
};

pub struct McpServer {
    memory_use_cases: Arc<dyn MemoryUseCases>,
    retrieval_service: Arc<RetrievalService>,
    model_repository: Arc<dyn ModelRepository>,
    agent_repository: Arc<dyn AgentRepository>,
    skill_repository: Arc<dyn SkillRepository>,
    workflow_repository: Arc<dyn WorkflowRepository>,
    evaluation_repository: Arc<dyn EvaluationRepository>,
    execution_history_repository: Arc<dyn ExecutionHistoryRepository>,
    authorization_boundary: AuthorizationBoundary,
}

impl McpServer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory_use_cases: Arc<dyn MemoryUseCases>,
        retrieval_service: Arc<RetrievalService>,
        model_repository: Arc<dyn ModelRepository>,
        agent_repository: Arc<dyn AgentRepository>,
        skill_repository: Arc<dyn SkillRepository>,
        workflow_repository: Arc<dyn WorkflowRepository>,
        evaluation_repository: Arc<dyn EvaluationRepository>,
        execution_history_repository: Arc<dyn ExecutionHistoryRepository>,
    ) -> Self {
        Self::new_with_policy(
            memory_use_cases,
            retrieval_service,
            model_repository,
            agent_repository,
            skill_repository,
            workflow_repository,
            evaluation_repository,
            execution_history_repository,
            Arc::new(DenyAllPolicyEngine),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_policy(
        memory_use_cases: Arc<dyn MemoryUseCases>,
        retrieval_service: Arc<RetrievalService>,
        model_repository: Arc<dyn ModelRepository>,
        agent_repository: Arc<dyn AgentRepository>,
        skill_repository: Arc<dyn SkillRepository>,
        workflow_repository: Arc<dyn WorkflowRepository>,
        evaluation_repository: Arc<dyn EvaluationRepository>,
        execution_history_repository: Arc<dyn ExecutionHistoryRepository>,
        policy_engine: SharedPolicyDecisionEngine,
    ) -> Self {
        Self {
            memory_use_cases,
            retrieval_service,
            model_repository,
            agent_repository,
            skill_repository,
            workflow_repository,
            evaluation_repository,
            execution_history_repository,
            authorization_boundary: AuthorizationBoundary::new(policy_engine),
        }
    }

    pub async fn handle_tool_call(
        &self,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<McpToolResult, ApplicationError> {
        let ctx = RequestContext::new(workspace_id, principal_id);

        let authorization_request = mcp_authorization_request(tool, arguments)
            .ok_or_else(|| ApplicationError::Policy("unknown MCP tool denied".to_owned()))?;
        self.authorization_boundary
            .require(&ctx, authorization_request)
            .await?;

        match tool {
            "search_memories" => {
                let query = arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ApplicationError::Internal("missing 'query' argument".to_owned())
                    })?;

                let request = RetrievalRequest::new(workspace_id, query);
                let result = self.retrieval_service.search(&ctx, request).await?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "candidates": result.candidates.iter().map(|c| {
                            serde_json::json!({
                                "memory_id": c.memory_id.as_uuid(),
                                "score": c.score,
                                "explanation": c.explanation,
                            })
                        }).collect::<Vec<_>>(),
                        "degraded": result.degraded,
                    }),
                    is_error: false,
                })
            }
            "get_memory" => {
                let memory_id = arguments
                    .get("memory_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::MemoryId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'memory_id' argument".to_owned(),
                        )
                    })?;

                let memory = self
                    .memory_use_cases
                    .find_memory(&ctx, memory_id)
                    .await?
                    .ok_or_else(|| {
                        ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                            "memory not found".to_owned(),
                        ))
                    })?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "id": memory.id.as_uuid(),
                        "kind": format!("{:?}", memory.kind).to_lowercase(),
                        "status": format!("{:?}", memory.status).to_lowercase(),
                    }),
                    is_error: false,
                })
            }
            "vestrace_models_list" => {
                let models = self.model_repository.list(&ctx).await?;
                Ok(McpToolResult {
                    content: serde_json::json!({
                        "models": models.iter().map(|m| {
                            serde_json::json!({
                                "id": m.id.as_uuid(),
                                "name": m.model_name,
                                "provider_id": m.provider_id.as_uuid(),
                            })
                        }).collect::<Vec<_>>(),
                    }),
                    is_error: false,
                })
            }
            "vestrace_agent_get" => {
                let agent_id = arguments
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::AgentId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'agent_id' argument".to_owned(),
                        )
                    })?;

                let agent = self
                    .agent_repository
                    .find_by_id(&ctx, agent_id)
                    .await?
                    .ok_or_else(|| {
                        ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                            "agent not found".to_owned(),
                        ))
                    })?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "id": agent.id.as_uuid(),
                        "name": agent.name,
                        "description": agent.description,
                        "system_prompt": agent.system_prompt,
                    }),
                    is_error: false,
                })
            }
            "vestrace_skill_get" => {
                let skill_id = arguments
                    .get("skill_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::SkillId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'skill_id' argument".to_owned(),
                        )
                    })?;

                let skills = self.skill_repository.list(&ctx).await?;
                let skill = skills
                    .into_iter()
                    .find(|s| s.id == skill_id)
                    .ok_or_else(|| {
                        ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                            "skill not found".to_owned(),
                        ))
                    })?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "id": skill.id.as_uuid(),
                        "name": skill.name,
                        "instructions": skill.instructions,
                    }),
                    is_error: false,
                })
            }
            "vestrace_workflow_get" => {
                let workflow_id = arguments
                    .get("workflow_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'workflow_id' argument".to_owned(),
                        )
                    })?;

                let workflow = self
                    .workflow_repository
                    .find_by_id(&ctx, workflow_id)
                    .await?
                    .ok_or_else(|| {
                        ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                            "workflow not found".to_owned(),
                        ))
                    })?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "id": workflow.id.as_uuid(),
                        "name": workflow.name,
                        "current_revision": workflow.current_revision,
                    }),
                    is_error: false,
                })
            }
            "vestrace_workflow_context" => {
                let workflow_id = arguments
                    .get("workflow_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'workflow_id' argument".to_owned(),
                        )
                    })?;
                let revision_number = arguments
                    .get("revision_number")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;

                let revision = self
                    .workflow_repository
                    .get_revision(&ctx, workflow_id, revision_number)
                    .await?
                    .ok_or_else(|| {
                        ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                            "workflow revision not found".to_owned(),
                        ))
                    })?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "revision_id": revision.revision_id.as_uuid(),
                        "workflow_id": revision.workflow_id.as_uuid(),
                        "revision_number": revision.revision_number,
                        "definition": revision.definition,
                    }),
                    is_error: false,
                })
            }
            "vestrace_execution_start" => {
                let workflow_id = arguments
                    .get("workflow_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'workflow_id' argument".to_owned(),
                        )
                    })?;
                let attempt = arguments
                    .get("attempt")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;

                let id = vestrace_domain::id::WorkflowExecutionId::new();
                let now = vestrace_domain::time::now();

                let record = vestrace_application::WorkflowExecutionRecord {
                    id,
                    workspace_id: ctx.workspace_id,
                    workflow_id,
                    workflow_revision: 1,
                    workflow_revision_id: vestrace_domain::id::WorkflowRevisionId::new(),
                    status: vestrace_domain::ExecutionStatus::Queued,
                    attempt,
                    started_at: now,
                    completed_at: None,
                    correlation_id: None,
                    causation_id: None,
                    run_id: None,
                };

                self.execution_history_repository
                    .start_workflow_execution(&ctx, &record)
                    .await?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "execution_id": id.as_uuid(),
                    }),
                    is_error: false,
                })
            }
            "vestrace_step_record" => {
                let execution_id = arguments
                    .get("execution_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowExecutionId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'execution_id' argument".to_owned(),
                        )
                    })?;
                let node_id = arguments
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowNodeId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'node_id' argument".to_owned(),
                        )
                    })?;
                let node_label = arguments
                    .get("node_label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let kind_str = arguments
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("agent");
                let kind = match kind_str {
                    "agent" => vestrace_domain::StepKind::Agent,
                    "skill" => vestrace_domain::StepKind::Skill,
                    "tool" => vestrace_domain::StepKind::Tool,
                    "decision" => vestrace_domain::StepKind::Decision,
                    "parallel" => vestrace_domain::StepKind::Parallel,
                    "join" => vestrace_domain::StepKind::Join,
                    "human_approval" => vestrace_domain::StepKind::HumanApproval,
                    "sub_workflow" => vestrace_domain::StepKind::SubWorkflow,
                    "end" => vestrace_domain::StepKind::End,
                    _ => vestrace_domain::StepKind::Agent,
                };
                let attempt = arguments
                    .get("attempt")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;

                let id = vestrace_domain::id::StepExecutionId::new();
                let now = vestrace_domain::time::now();

                let record = vestrace_application::StepExecutionRecord {
                    id,
                    workflow_execution_id: execution_id,
                    workspace_id: ctx.workspace_id,
                    node_id,
                    node_label,
                    kind,
                    agent_ref: None,
                    attempt,
                    status: vestrace_domain::ExecutionStatus::Queued,
                    input_artifact_id: None,
                    output_artifact_id: None,
                    model_attempt_id: None,
                    tool_invocation_id: None,
                    error_message: None,
                    started_at: now,
                    completed_at: None,
                    run_id: None,
                };

                self.execution_history_repository
                    .record_step(&ctx, &record)
                    .await?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "step_id": id.as_uuid(),
                    }),
                    is_error: false,
                })
            }
            "vestrace_execution_complete" => {
                let execution_id = arguments
                    .get("execution_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::WorkflowExecutionId>().ok())
                    .ok_or_else(|| {
                        ApplicationError::Internal(
                            "missing or invalid 'execution_id' argument".to_owned(),
                        )
                    })?;
                let status_str = arguments
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("succeeded");
                let status = match status_str {
                    "succeeded" => vestrace_domain::ExecutionStatus::Succeeded,
                    "failed" => vestrace_domain::ExecutionStatus::Failed,
                    "cancelled" => vestrace_domain::ExecutionStatus::Cancelled,
                    _ => vestrace_domain::ExecutionStatus::Succeeded,
                };
                let now = vestrace_domain::time::now();

                self.execution_history_repository
                    .update_workflow_execution_status(&ctx, execution_id, status, Some(now))
                    .await?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "execution_id": execution_id.as_uuid(),
                        "status": status_str,
                    }),
                    is_error: false,
                })
            }
            "vestrace_model_evaluation_record" => {
                let name = arguments
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed")
                    .to_string();
                let model_id = arguments
                    .get("model_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<vestrace_domain::id::ModelId>().ok());
                let score = arguments.get("score").and_then(|v| v.as_f64());
                let summary = arguments
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let id = vestrace_domain::id::EvaluationId::new();
                let now = vestrace_domain::time::now();

                let record = vestrace_application::EvaluationRecord {
                    id,
                    workspace_id: ctx.workspace_id,
                    model_id,
                    name,
                    status: "completed".to_string(),
                    score,
                    summary,
                    created_at: now,
                };

                self.evaluation_repository.create(&ctx, &record).await?;

                Ok(McpToolResult {
                    content: serde_json::json!({
                        "evaluation_id": id.as_uuid(),
                    }),
                    is_error: false,
                })
            }
            _ => Ok(McpToolResult {
                content: serde_json::json!({
                    "error": format!("unknown tool: {tool}")
                }),
                is_error: true,
            }),
        }
    }

    pub fn list_tools(&self) -> Vec<McpToolDefinition> {
        Self::tool_definitions()
    }

    pub fn tool_definitions() -> Vec<McpToolDefinition> {
        vec![
            McpToolDefinition {
                name: "search_memories".to_owned(),
                description: "Search memories by text query using hybrid retrieval".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query text"
                        }
                    },
                    "required": ["query"]
                }),
            },
            McpToolDefinition {
                name: "get_memory".to_owned(),
                description: "Retrieve a memory by its UUID".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "memory_id": {
                            "type": "string",
                            "description": "Memory UUID"
                        }
                    },
                    "required": ["memory_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_models_list".to_owned(),
                description: "List all registered AI models".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpToolDefinition {
                name: "vestrace_agent_get".to_owned(),
                description: "Get an agent definition by ID".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "Agent UUID"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_skill_get".to_owned(),
                description: "Get a skill definition by ID".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "Skill UUID"
                        }
                    },
                    "required": ["skill_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_workflow_get".to_owned(),
                description: "Get a workflow definition by ID".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_id": {
                            "type": "string",
                            "description": "Workflow UUID"
                        }
                    },
                    "required": ["workflow_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_workflow_context".to_owned(),
                description: "Get a workflow revision definition with nodes and transitions"
                    .to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_id": {
                            "type": "string",
                            "description": "Workflow UUID"
                        },
                        "revision_number": {
                            "type": "integer",
                            "description": "Revision number (default: 1)"
                        }
                    },
                    "required": ["workflow_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_execution_start".to_owned(),
                description: "Start a new workflow execution".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_id": {
                            "type": "string",
                            "description": "Workflow UUID"
                        },
                        "attempt": {
                            "type": "integer",
                            "description": "Attempt number (default: 1)"
                        }
                    },
                    "required": ["workflow_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_step_record".to_owned(),
                description: "Record a step execution within a workflow execution".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Workflow execution UUID"
                        },
                        "node_id": {
                            "type": "string",
                            "description": "Workflow node UUID"
                        },
                        "node_label": {
                            "type": "string",
                            "description": "Node label"
                        },
                        "kind": {
                            "type": "string",
                            "description": "Step kind: agent, skill, tool, decision, parallel, join, human_approval, sub_workflow, end"
                        },
                        "attempt": {
                            "type": "integer",
                            "description": "Attempt number (default: 1)"
                        }
                    },
                    "required": ["execution_id", "node_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_execution_complete".to_owned(),
                description: "Complete a workflow execution with final status".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Workflow execution UUID"
                        },
                        "status": {
                            "type": "string",
                            "description": "Final status: succeeded, failed, cancelled"
                        }
                    },
                    "required": ["execution_id"]
                }),
            },
            McpToolDefinition {
                name: "vestrace_model_evaluation_record".to_owned(),
                description: "Record a model evaluation result".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Evaluation name"
                        },
                        "model_id": {
                            "type": "string",
                            "description": "Model UUID (optional)"
                        },
                        "score": {
                            "type": "number",
                            "description": "Evaluation score (optional)"
                        },
                        "summary": {
                            "type": "string",
                            "description": "Evaluation summary (optional)"
                        }
                    },
                    "required": ["name"]
                }),
            },
        ]
    }
}

fn mcp_authorization_request(
    tool: &str,
    arguments: &serde_json::Value,
) -> Option<AuthorizationRequest> {
    // The resource key names the argument to read the id out of; the kind names
    // what that id *is*. They were the same thing until the scope vocabulary was
    // made explicit, and the scope was built as `{key}:{value}` — so a memory
    // reached through MCP was `memory_id:0198…` while the same memory reached
    // through the purge was `memory://0198…`. A grant written for one authorised
    // nothing in the other, and neither `memory_id:` nor `workspace` could be
    // contained by any grant that did not spell them identically.
    let (capability, resource) = match tool {
        "search_memories" => (Capability::ContextRetrieve, None),
        "get_memory" => (
            Capability::MemoryRead,
            Some(("memory_id", ResourceKind::Memory)),
        ),
        "vestrace_models_list" => (Capability::ModelRead, None),
        "vestrace_agent_get" => (
            Capability::AgentRead,
            Some(("agent_id", ResourceKind::Agent)),
        ),
        "vestrace_skill_get" => (
            Capability::SkillRead,
            Some(("skill_id", ResourceKind::Skill)),
        ),
        "vestrace_workflow_get" | "vestrace_workflow_context" => (
            Capability::WorkflowRead,
            Some(("workflow_id", ResourceKind::Workflow)),
        ),
        "vestrace_execution_start" => (
            Capability::ExecutionWrite,
            Some(("workflow_id", ResourceKind::Workflow)),
        ),
        "vestrace_step_record" | "vestrace_execution_complete" => (
            Capability::ExecutionWrite,
            Some(("execution_id", ResourceKind::Execution)),
        ),
        "vestrace_model_evaluation_record" => (Capability::EvaluationWrite, None),
        _ => return None,
    };

    let resource_scope = resource
        .and_then(|(key, kind)| {
            arguments
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| ResourceScope::one(kind, value.trim()))
        })
        // A tool that is not about one resource is about the workspace, and
        // says so in the same vocabulary rather than in a bare word.
        .unwrap_or_else(ResourceScope::workspace)
        .to_string();

    let risk = match tool {
        "vestrace_execution_start" | "vestrace_step_record" | "vestrace_execution_complete" => {
            RiskCategory::High
        }
        "vestrace_model_evaluation_record" => RiskCategory::Medium,
        _ => RiskCategory::Low,
    };

    Some(AuthorizationRequest::new(
        capability,
        format!("mcp.{tool}"),
        resource_scope,
        risk,
    ))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: serde_json::Value,
    pub is_error: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use vestrace_application::{AgentRecord, SkillRecord};
    use vestrace_application::{
        ApplicationError, DenyAllPolicyEngine, EvaluationRecord, NormalizedRetrievalRequest,
        NullExecutionHistoryRepository, RequestContext, TextRetriever, WorkflowDefinitionRecord,
        WorkflowRevisionRecord,
    };
    use vestrace_domain::id::{CapabilityGrantId, PolicyDecisionId};
    use vestrace_domain::{
        CapabilityGrant, CapabilityGrantSpec, PolicyDecision, evaluate_capability_grants,
    };

    struct StubMemory;
    #[async_trait]
    impl MemoryUseCases for StubMemory {
        async fn record_event(
            &self,
            _: &RequestContext,
            _: vestrace_application::RecordEventCommand,
        ) -> Result<vestrace_domain::Event, ApplicationError> {
            unreachable!()
        }
        async fn remember_memory(
            &self,
            _: &RequestContext,
            _: vestrace_application::RememberMemoryCommand,
        ) -> Result<vestrace_domain::Memory, ApplicationError> {
            unreachable!()
        }
        async fn revise_memory(
            &self,
            _: &RequestContext,
            _: vestrace_application::ReviseMemoryCommand,
        ) -> Result<vestrace_domain::Memory, ApplicationError> {
            unreachable!()
        }
        async fn link_knowledge(
            &self,
            _: &RequestContext,
            _: vestrace_application::LinkKnowledgeCommand,
        ) -> Result<vestrace_domain::KnowledgeRelation, ApplicationError> {
            unreachable!()
        }
        async fn find_memory(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::MemoryId,
        ) -> Result<Option<vestrace_domain::Memory>, ApplicationError> {
            Ok(None)
        }
    }

    struct StubTextRetriever;
    #[async_trait]
    impl TextRetriever for StubTextRetriever {
        async fn search(
            &self,
            _: &RequestContext,
            _: &NormalizedRetrievalRequest,
        ) -> Result<Vec<vestrace_domain::RetrievalCandidate>, ApplicationError> {
            Ok(Vec::new())
        }
    }

    struct StubJournal;
    #[async_trait]
    impl vestrace_application::RetrievalJournal for StubJournal {
        async fn record_run(
            &self,
            _: &RequestContext,
            _: &vestrace_application::retrieval::RetrievalRunRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn record_context_pack(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::ContextPackId,
            _: vestrace_domain::id::RetrievalRunId,
            _: vestrace_domain::WorkspaceId,
            _: u32,
            _: u32,
            _: &serde_json::Value,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    struct StubModelRepo;
    #[async_trait]
    impl vestrace_application::ModelRepository for StubModelRepo {
        async fn create(
            &self,
            _: &RequestContext,
            _: &vestrace_application::ModelRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn list(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<vestrace_application::ModelRecord>, ApplicationError> {
            Ok(vec![])
        }
        async fn find_by_id(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::ModelId,
        ) -> Result<Option<vestrace_application::ModelRecord>, ApplicationError> {
            Ok(None)
        }
    }

    struct StubAgentRepo;
    #[async_trait]
    impl vestrace_application::AgentRepository for StubAgentRepo {
        async fn create(
            &self,
            _: &RequestContext,
            _: &AgentRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn list(&self, _: &RequestContext) -> Result<Vec<AgentRecord>, ApplicationError> {
            Ok(vec![])
        }
        async fn find_by_id(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::AgentId,
        ) -> Result<Option<AgentRecord>, ApplicationError> {
            Ok(None)
        }
    }

    struct StubSkillRepo;
    #[async_trait]
    impl vestrace_application::SkillRepository for StubSkillRepo {
        async fn create(
            &self,
            _: &RequestContext,
            _: &SkillRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn list(&self, _: &RequestContext) -> Result<Vec<SkillRecord>, ApplicationError> {
            Ok(vec![])
        }
    }

    struct StubWorkflowRepo;
    #[async_trait]
    impl vestrace_application::WorkflowRepository for StubWorkflowRepo {
        async fn create(
            &self,
            _: &RequestContext,
            _: &WorkflowDefinitionRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn list(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError> {
            Ok(vec![])
        }
        async fn find_by_id(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::WorkflowId,
        ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError> {
            Ok(None)
        }
        async fn save_revision(
            &self,
            _: &RequestContext,
            _: &WorkflowRevisionRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn get_revision(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::WorkflowId,
            _: u32,
        ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError> {
            Ok(None)
        }
    }

    struct StubEvaluationRepo;
    #[async_trait]
    impl vestrace_application::EvaluationRepository for StubEvaluationRepo {
        async fn create(
            &self,
            _: &RequestContext,
            _: &EvaluationRecord,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
        async fn list(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<EvaluationRecord>, ApplicationError> {
            Ok(vec![])
        }
        async fn find_by_id(
            &self,
            _: &RequestContext,
            _: vestrace_domain::id::EvaluationId,
        ) -> Result<Option<EvaluationRecord>, ApplicationError> {
            Ok(None)
        }
    }

    fn make_server() -> McpServer {
        McpServer::new_with_policy(
            Arc::new(StubMemory),
            Arc::new(RetrievalService::new(
                Arc::new(StubTextRetriever),
                Arc::new(StubJournal),
            )),
            Arc::new(StubModelRepo),
            Arc::new(StubAgentRepo),
            Arc::new(StubSkillRepo),
            Arc::new(StubWorkflowRepo),
            Arc::new(StubEvaluationRepo),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(TestAllowPolicy),
        )
    }

    struct TestAllowPolicy;

    #[async_trait]
    impl vestrace_application::PolicyDecisionEngine for TestAllowPolicy {
        async fn decide(
            &self,
            context: &RequestContext,
            request: vestrace_domain::AuthorizationRequest,
        ) -> Result<PolicyDecision, ApplicationError> {
            let at = vestrace_domain::now();
            let grant = CapabilityGrant::issue(
                CapabilityGrantSpec {
                    id: CapabilityGrantId::new(),
                    workspace_id: context.workspace_id,
                    subject_id: context.principal_id,
                    issuer_id: context.principal_id,
                    capability: request.capability.clone(),
                    operation: request.operation.clone(),
                    resource_scope: request.resource_scope.clone(),
                    valid_from: at,
                    valid_until: None,
                    budget: None,
                    risk_ceiling: vestrace_domain::RiskCategory::Critical,
                    conditions: request.conditions.clone(),
                },
                at,
            )?;

            Ok(evaluate_capability_grants(
                PolicyDecisionId::new(),
                context.workspace_id,
                context.principal_id,
                "test-allow-v1",
                &request,
                &[grant],
                at,
            )?)
        }
    }

    fn make_denying_server() -> McpServer {
        McpServer::new_with_policy(
            Arc::new(StubMemory),
            Arc::new(RetrievalService::new(
                Arc::new(StubTextRetriever),
                Arc::new(StubJournal),
            )),
            Arc::new(StubModelRepo),
            Arc::new(StubAgentRepo),
            Arc::new(StubSkillRepo),
            Arc::new(StubWorkflowRepo),
            Arc::new(StubEvaluationRepo),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(DenyAllPolicyEngine),
        )
    }

    #[tokio::test]
    async fn list_tools_returns_all_tools() {
        let server = make_server();
        let tools = server.list_tools();
        assert_eq!(tools.len(), 11);
        assert_eq!(tools[0].name, "search_memories");
        assert_eq!(tools[1].name, "get_memory");
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let server = make_server();
        let error = server
            .handle_tool_call(
                WorkspaceId::new(),
                PrincipalId::new(),
                "nonexistent",
                &serde_json::json!({}),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, ApplicationError::Policy(message) if message.contains("unknown MCP tool"))
        );
    }

    #[tokio::test]
    async fn denied_tool_call_stops_before_dispatch() {
        let server = make_denying_server();
        let error = server
            .handle_tool_call(
                WorkspaceId::new(),
                PrincipalId::new(),
                "vestrace_models_list",
                &serde_json::json!({}),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Policy(message) if message.contains("DefaultDeny"))
        );
    }

    #[tokio::test]
    async fn unknown_tool_is_denied_before_unknown_tool_fallback() {
        let server = make_denying_server();
        let error = server
            .handle_tool_call(
                WorkspaceId::new(),
                PrincipalId::new(),
                "unregistered_tool",
                &serde_json::json!({}),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Policy(message) if message.contains("unknown MCP tool"))
        );
    }

    #[tokio::test]
    async fn vestrace_models_list_returns_empty() {
        let server = make_server();
        let result = server
            .handle_tool_call(
                WorkspaceId::new(),
                PrincipalId::new(),
                "vestrace_models_list",
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content["models"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn vestrace_execution_start_returns_id() {
        let server = make_server();
        let wf_id = vestrace_domain::id::WorkflowId::new();
        let result = server
            .handle_tool_call(
                WorkspaceId::new(),
                PrincipalId::new(),
                "vestrace_execution_start",
                &serde_json::json!({
                    "workflow_id": wf_id.as_uuid().to_string()
                }),
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content["execution_id"].as_str().is_some());
    }
}
