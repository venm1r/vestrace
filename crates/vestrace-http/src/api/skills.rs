use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use vestrace_application::SkillRecord;
use vestrace_domain::time::now;

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Serialize)]
pub struct SkillResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub instructions: String,
}

impl From<SkillRecord> for SkillResponse {
    fn from(s: SkillRecord) -> Self {
        Self {
            id: s.id.as_uuid(),
            name: s.name,
            instructions: s.instructions,
        }
    }
}

pub async fn create_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateSkillRequest>,
) -> Result<(StatusCode, Json<SkillResponse>), ApiError> {
    let context = request_context(&headers)?;
    let id = vestrace_domain::id::SkillId::new();
    let record = SkillRecord {
        id,
        workspace_id: context.workspace_id,
        name: req.name,
        instructions: req.instructions,
        created_at: now(),
    };
    state
        .skill_repository()
        .create(&context, &record)
        .await
        .map_err(ApiError::from_application)?;
    Ok((StatusCode::CREATED, Json(SkillResponse::from(record))))
}

pub async fn list_skills(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SkillResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let skills = state
        .skill_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(skills.into_iter().map(SkillResponse::from).collect()))
}
