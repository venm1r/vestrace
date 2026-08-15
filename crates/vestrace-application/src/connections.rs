use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::connection::Connection;

use crate::{ApplicationError, RequestContext};

/// A connection together with the connector it belongs to.
///
/// **No credentials.** The `connections` table stores identity and state only;
/// there is no secret column and this port deliberately exposes none. Storing
/// credentials requires envelope encryption under a key that does not live
/// beside the ciphertext, which this system does not yet have.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionListing {
    pub connection: Connection,
    pub connector_name: String,
    pub provider_type: String,
}

#[async_trait]
pub trait ConnectionRepository: Send + Sync {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ConnectionListing>, ApplicationError>;
}

pub type SharedConnectionRepository = Arc<dyn ConnectionRepository>;
