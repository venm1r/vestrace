use vestrace_domain::DomainError;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("policy failure: {0}")]
    Policy(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("internal failure: {0}")]
    Internal(String),
}
