#![forbid(unsafe_code)]

mod error;
mod id;
mod time;

pub use error::DomainError;
pub use id::{CorrelationId, OperationId, PrincipalId, RequestId, WorkspaceId};
pub use time::{Timestamp, now};

pub fn crate_name() -> &'static str {
    "vestrace-domain"
}

#[cfg(test)]
mod tests {
    use super::WorkspaceId;

    #[test]
    fn crate_is_linkable() {
        assert_eq!(super::crate_name(), "vestrace-domain");
    }

    #[test]
    fn workspace_id_round_trips_as_string() {
        let id = WorkspaceId::new();

        assert_eq!(id.to_string().parse::<WorkspaceId>().unwrap(), id);
    }
}
