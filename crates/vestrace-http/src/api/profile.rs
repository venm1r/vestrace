use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method},
    routing::get,
};
use serde::Serialize;

use crate::{
    AppState,
    route_inventory::{mount, route_descriptor},
};

use super::{ApiError, context::request_context};

/// The identity the current request is acting under.
///
/// With `auth.admin_token` configured, the identity is the single administrator
/// the presented bearer token maps to. Without it, the identity comes from
/// request headers and nothing verifies it — the response reports which case
/// applies rather than presenting an unverified header as a logged-in account.
///
/// No API keys are issued either way: there is one account, and issuing keys
/// against it would add credentials without adding accountability.
#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub workspace_id: uuid::Uuid,
    pub principal_id: uuid::Uuid,
    /// How the identity was established. Always `request_header` in this build.
    pub identity_source: &'static str,
    /// `false` until authentication exists; clients must not treat this
    /// identity as proven.
    pub authenticated: bool,
}

pub fn profile_routes() -> axum::Router<AppState> {
    mount(
        axum::Router::new(),
        route_descriptor(&Method::GET, "/v1/profile"),
        get(get_profile),
    )
}

async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, ApiError> {
    let context = request_context(&headers)?;
    let authenticated = state.is_authenticated();
    Ok(Json(ProfileResponse {
        workspace_id: context.workspace_id.as_uuid(),
        principal_id: context.principal_id.as_uuid(),
        identity_source: if authenticated {
            "administrator_bearer_token"
        } else {
            "request_header"
        },
        authenticated,
    }))
}
