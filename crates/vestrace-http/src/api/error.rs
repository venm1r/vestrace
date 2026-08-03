use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    pub fn not_implemented(feature: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            body: ErrorBody {
                code: "not_implemented",
                message: format!("{feature} is not implemented in the P0 foundation"),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self.body)).into_response()
    }
}
