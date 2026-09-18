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

    /// A refusal that is a property of the surface rather than of the caller's
    /// authorization, so it carries its own code instead of the generic
    /// `forbidden` an authorization failure produces.
    pub fn refused(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(resource: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{resource} was not found"),
        )
    }

    /// The surface is reserved by the HTTP adapter but has no implementation.
    ///
    /// The message names the build rather than a phase: phase labels go stale
    /// and send operators looking for a milestone that no longer exists in the
    /// roadmap.
    pub fn not_implemented(feature: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "not_implemented",
            format!("{feature} is not implemented in this build"),
        )
    }

    /// A domain refusal, mapped the same way it would be inside an
    /// `ApplicationError::Domain`. Handlers that call the domain directly need
    /// this so a validation failure and a policy violation do not both become
    /// `400`.
    pub fn from_domain(error: vestrace_domain::DomainError) -> Self {
        Self::from_application(ApplicationError::Domain(error))
    }

    pub fn from_application(error: ApplicationError) -> Self {
        match error {
            // Every domain error used to collapse into `400 invalid_request`,
            // which made four distinct outcomes indistinguishable to a client.
            // Two of them matter:
            //
            // - a missing resource answered 400 on every write path while the
            //   equivalent `GET` answered 404, so a cross-tenant cancel and a
            //   malformed body looked the same;
            // - a revision conflict answered 400, so a caller could not tell
            //   "your copy is stale, re-read and retry" — the one outcome that
            //   has an obvious next step — from "your request was wrong".
            //
            // The domain's own `code()` is used rather than a second spelling,
            // so the code a client matches on cannot drift from the variant.
            ApplicationError::Domain(error) => {
                let status = match error {
                    vestrace_domain::DomainError::NotFound(_) => StatusCode::NOT_FOUND,
                    vestrace_domain::DomainError::RevisionConflict { .. } => StatusCode::CONFLICT,
                    vestrace_domain::DomainError::PolicyViolation(_) => StatusCode::FORBIDDEN,
                    vestrace_domain::DomainError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
                };
                Self::new(status, error.code(), error.to_string())
            }
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
            ApplicationError::InvalidConfiguration(message) => {
                tracing::error!(error = %message, "invalid configuration");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid_configuration",
                    "the server is misconfigured",
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

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::DomainError;

    /// Every domain error mapped to `400 invalid_request`, and nothing asserted
    /// otherwise — which is why a cross-tenant write and a malformed body were
    /// indistinguishable for as long as they were.
    #[test]
    fn a_domain_error_keeps_its_own_meaning_in_the_status() {
        let cases = [
            (
                DomainError::NotFound("run does not exist".into()),
                StatusCode::NOT_FOUND,
                "not_found",
            ),
            (
                DomainError::RevisionConflict {
                    expected: 1,
                    current: 2,
                },
                StatusCode::CONFLICT,
                "revision_conflict",
            ),
            (
                DomainError::PolicyViolation("memory belongs to another workspace".into()),
                StatusCode::FORBIDDEN,
                "policy_violation",
            ),
            (
                DomainError::InvalidArgument("kind must not be blank".into()),
                StatusCode::BAD_REQUEST,
                "invalid_argument",
            ),
        ];

        for (error, expected_status, expected_code) in cases {
            let mapped = ApiError::from_application(ApplicationError::Domain(error));
            assert_eq!(mapped.status, expected_status, "status for {expected_code}");
            assert_eq!(mapped.body.code, expected_code);
        }
    }

    /// A stale revision has an obvious next step — re-read and retry — and a
    /// client can only take it if the conflict is distinguishable from a bad
    /// request. This is the case the memory revision route depends on.
    #[test]
    fn a_revision_conflict_reports_both_revisions() {
        let mapped =
            ApiError::from_application(ApplicationError::Domain(DomainError::RevisionConflict {
                expected: 1,
                current: 4,
            }));

        assert_eq!(mapped.status, StatusCode::CONFLICT);
        assert!(
            mapped.body.message.contains('1') && mapped.body.message.contains('4'),
            "the conflict message must name the revision the caller sent and the \
             one that is current, or a retry is a guess: {}",
            mapped.body.message
        );
    }
}
