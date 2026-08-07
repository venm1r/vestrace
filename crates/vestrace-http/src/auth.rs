use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use vestrace_application::RequestContext;
use vestrace_domain::{PrincipalId, WorkspaceId};

use crate::AppState;

const BEARER_PREFIX: &str = "Bearer ";
const WORKSPACE_ID_HEADER: &str = "x-workspace-id";
const PRINCIPAL_ID_HEADER: &str = "x-principal-id";

/// Local trusted authentication: extracts workspace_id and principal_id from
/// explicit headers. If a Bearer token is present, it is accepted but token
/// validation against access_tokens is a future enhancement.
///
/// In local-first deployment mode, the caller is trusted to provide correct
/// identity headers. RLS remains the second isolation layer.
pub async fn local_trusted_auth(
    State(_state): State<AppState>,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let workspace_id = extract_uuid_header(&headers, WORKSPACE_ID_HEADER);
    let principal_id = extract_uuid_header(&headers, PRINCIPAL_ID_HEADER);

    match (workspace_id, principal_id) {
        (Some(ws), Some(pid)) => {
            let ctx = RequestContext::new(WorkspaceId::from_uuid(ws), PrincipalId::from_uuid(pid));
            request.extensions_mut().insert(ctx);
            next.run(request).await
        }
        _ => {
            let mut response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from(
                    r#"{"code":"unauthorized","message":"missing workspace or principal identity"}"#,
                ))
                .unwrap();
            response
                .headers_mut()
                .insert("content-type", "application/json".parse().unwrap());
            response
        }
    }
}

fn extract_uuid_header(headers: &HeaderMap, name: &str) -> Option<uuid::Uuid> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
}

/// Extracts a bearer token from the Authorization header if present.
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.starts_with(BEARER_PREFIX))
        .map(|s| &s[BEARER_PREFIX.len()..])
}
