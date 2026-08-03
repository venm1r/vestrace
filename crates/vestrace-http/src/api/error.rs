use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use vestrace_application::ApplicationError;

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
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn not_found(resource: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{resource} was not found"),
        )
    }

    pub fn not_implemented(feature: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "not_implemented",
            format!("{feature} is not implemented in the P0 foundation"),
        )
    }

    pub fn from_application(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Domain(error) => Self::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.to_string(),
            ),
            ApplicationError::Conflict(message) => {
                Self::new(StatusCode::CONFLICT, "conflict", message)
            }
            ApplicationError::Policy(message) => {
                Self::new(StatusCode::FORBIDDEN, "forbidden", message)
            }
            ApplicationError::Unavailable(message) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", message)
            }
            ApplicationError::Storage(message) => {
                tracing::error!(error = %message, "run storage request failed");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failure",
                    "the requested data could not be stored or retrieved",
                )
            }
            ApplicationError::Internal(message) => {
                tracing::error!(error = %message, "internal application request failed");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_failure",
                    "the request could not be completed",
                )
            }
        }
    }

    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self.body)).into_response()
    }
}
