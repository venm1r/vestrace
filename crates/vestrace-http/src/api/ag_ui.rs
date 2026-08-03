use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

use super::ApiError;

pub fn ag_ui_routes() -> Router<AppState> {
    Router::new()
        .route("/endpoints", get(unsupported_ag_ui).post(unsupported_ag_ui))
        .route("/events/stream", get(unsupported_ag_ui))
        .route("/run", post(unsupported_ag_ui))
}

async fn unsupported_ag_ui() -> ApiError {
    ApiError::not_implemented("AG-UI")
}
