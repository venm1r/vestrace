use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use vestrace_application::AgentRecord;
use vestrace_domain::time::now;

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
}

impl From<AgentRecord> for AgentResponse {
    fn from(a: AgentRecord) -> Self {
        Self {
            id: a.id.as_uuid(),
            name: a.name,
            description: a.description,
            system_prompt: a.system_prompt,
        }
    }
}

pub async fn create_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), ApiError> {
    let context = request_context(&headers)?;
    let id = vestrace_domain::id::AgentId::new();
    let record = AgentRecord {
        id,
        workspace_id: context.workspace_id,
        name: req.name,
        description: req.description,
        system_prompt: req.system_prompt,
        created_at: now(),
    };
    state
        .agent_repository()
        .create(&context, &record)
        .await
        .map_err(ApiError::from_application)?;
    Ok((StatusCode::CREATED, Json(AgentResponse::from(record))))
}

pub async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AgentResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let agents = state
        .agent_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(agents.into_iter().map(AgentResponse::from).collect()))
}
