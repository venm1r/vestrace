#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("revision conflict: expected {expected}, current {current}")]
    RevisionConflict { expected: u32, current: u32 },
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

impl DomainError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "invalid_argument",
            Self::NotFound(_) => "not_found",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::PolicyViolation(_) => "policy_violation",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DomainError;

    #[test]
    fn domain_error_codes_are_stable() {
        assert_eq!(
            DomainError::InvalidArgument("value".into()).code(),
            "invalid_argument"
        );
        assert_eq!(DomainError::NotFound("record".into()).code(), "not_found");
        assert_eq!(
            DomainError::RevisionConflict {
                expected: 1,
                current: 2,
            }
            .code(),
            "revision_conflict"
        );
        assert_eq!(
            DomainError::PolicyViolation("rule".into()).code(),
            "policy_violation"
        );
    }
}
