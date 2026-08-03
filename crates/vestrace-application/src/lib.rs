#![forbid(unsafe_code)]

mod context;
mod error;
mod health;
pub mod jobs;
pub mod memory;
pub mod outbox;
mod ports;
pub mod providers;
pub mod runs;

pub use context::RequestContext;
pub use error::ApplicationError;
pub use health::HealthRepository;
pub use jobs::*;
pub use memory::*;
pub use outbox::OutboxMessage;
pub use ports::{TransactionManager, UnitOfWork};
pub use providers::*;
pub use runs::*;

#[cfg(test)]
mod tests {
    use super::{ApplicationError, RequestContext};
    use vestrace_domain::{DomainError, PrincipalId, WorkspaceId};

    #[test]
    fn request_context_is_workspace_bound() {
        let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

        assert_ne!(ctx.workspace_id.as_uuid(), uuid::Uuid::nil());
    }

    #[test]
    fn application_error_preserves_domain_failure() {
        let error = ApplicationError::from(DomainError::InvalidArgument(
            "workspace name is required".to_owned(),
        ));

        assert!(matches!(
            error,
            ApplicationError::Domain(DomainError::InvalidArgument(message))
                if message == "workspace name is required"
        ));
    }
}
