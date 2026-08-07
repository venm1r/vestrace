use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};
use vestrace_application::{ModelRecord, ProviderRecord};
use vestrace_domain::{ProviderLocality, time::now};

use crate::AppState;

use super::{ApiError, context::request_context};

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub locality: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub locality: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateModelRequest {
    pub provider_id: uuid::Uuid,
    pub model_name: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub id: uuid::Uuid,
    pub provider_id: uuid::Uuid,
    pub model_name: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
}

impl From<ProviderRecord> for ProviderResponse {
    fn from(p: ProviderRecord) -> Self {
        let locality = match p.locality {
            ProviderLocality::Local => "local",
            ProviderLocality::Remote => "remote",
        };
        Self {
            id: p.id.as_uuid(),
            name: p.name,
            locality: locality.to_string(),
        }
    }
}

impl From<ModelRecord> for ModelResponse {
    fn from(m: ModelRecord) -> Self {
        Self {
            id: m.id.as_uuid(),
            provider_id: m.provider_id.as_uuid(),
            model_name: m.model_name,
            context_window: m.context_window,
            input_cost_per_mtoken: m.input_cost_per_mtoken,
            output_cost_per_mtoken: m.output_cost_per_mtoken,
        }
    }
}

pub async fn create_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateProviderRequest>,
) -> Result<(axum::http::StatusCode, Json<ProviderResponse>), ApiError> {
    let context = request_context(&headers)?;
    let id = vestrace_domain::id::ProviderId::new();
    let locality = match req.locality.as_str() {
        "local" => ProviderLocality::Local,
        "remote" => ProviderLocality::Remote,
        _ => {
            return Err(ApiError::bad_request(
                "locality must be 'local' or 'remote'",
            ));
        }
    };
    state
        .provider_repository()
        .create(&context, id, &req.name, locality)
        .await
        .map_err(ApiError::from_application)?;

    let locality_str = match locality {
        ProviderLocality::Local => "local",
        ProviderLocality::Remote => "remote",
    };
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ProviderResponse {
            id: id.as_uuid(),
            name: req.name,
            locality: locality_str.to_string(),
        }),
    ))
}

pub async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProviderResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let providers = state
        .provider_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(
        providers.into_iter().map(ProviderResponse::from).collect(),
    ))
}

pub async fn create_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateModelRequest>,
) -> Result<(axum::http::StatusCode, Json<ModelResponse>), ApiError> {
    let context = request_context(&headers)?;
    let id = vestrace_domain::id::ModelId::new();
    let record = ModelRecord {
        id,
        provider_id: vestrace_domain::id::ProviderId::from_uuid(req.provider_id),
        workspace_id: context.workspace_id,
        model_name: req.model_name,
        context_window: req.context_window,
        input_cost_per_mtoken: req.input_cost_per_mtoken,
        output_cost_per_mtoken: req.output_cost_per_mtoken,
        created_at: now(),
    };
    state
        .model_repository()
        .create(&context, &record)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ModelResponse::from(record)),
    ))
}

pub async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ModelResponse>>, ApiError> {
    let context = request_context(&headers)?;
    let models = state
        .model_repository()
        .list(&context)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(models.into_iter().map(ModelResponse::from).collect()))
}
