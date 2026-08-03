use axum::http::HeaderMap;
use uuid::Uuid;
use vestrace_application::RequestContext;
use vestrace_domain::{PrincipalId, WorkspaceId};

use super::ApiError;

const WORKSPACE_ID_HEADER: &str = "x-workspace-id";
const PRINCIPAL_ID_HEADER: &str = "x-principal-id";

pub fn request_context(headers: &HeaderMap) -> Result<RequestContext, ApiError> {
    let workspace_id = required_uuid_header(headers, WORKSPACE_ID_HEADER)?;
    let principal_id = required_uuid_header(headers, PRINCIPAL_ID_HEADER)?;

    Ok(RequestContext::new(
        WorkspaceId::from_uuid(workspace_id),
        PrincipalId::from_uuid(principal_id),
    ))
}

fn required_uuid_header(headers: &HeaderMap, name: &'static str) -> Result<Uuid, ApiError> {
    let value = headers
        .get(name)
        .ok_or_else(|| ApiError::bad_request(format!("missing {name} header")))?;
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request(format!("{name} header must be valid UTF-8")))?;

    Uuid::parse_str(value)
        .map_err(|_| ApiError::bad_request(format!("{name} header must be a UUID")))
}
