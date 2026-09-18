//! Barrier dispatch refusal derives from the domain lifecycle vocabulary.

use async_trait::async_trait;
use vestrace_domain::embedding::BarrierState;

use crate::{ApplicationError, RequestContext};

/// A caller that is about to create work for a dedicated barrier batch. Exact
/// batch/recipe identities are derived by the guarded SQL mapping resolver,
/// not asserted at this application boundary.
#[derive(Clone, Copy, Debug)]
pub struct BarrierBatchDispatch {
    pub state: BarrierState,
}

impl BarrierBatchDispatch {
    /// Rust callers must not duplicate the open-state literal.  SQL has its
    /// own closed CHECK constraint; this boundary deliberately derives its
    /// refusal from the domain method.
    pub fn require_dispatchable(self) -> Result<(), ApplicationError> {
        if self.state.blocks_dispatch() {
            return Err(ApplicationError::Policy(
                "EMBEDDING_TRANSITION_BARRIER_BATCH_IS_NOT_DISPATCHABLE".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
pub trait EmbeddingTransitionBarrierRepository: Send + Sync {
    async fn require_dispatchable(
        &self,
        _context: RequestContext,
        dispatch: BarrierBatchDispatch,
    ) -> Result<(), ApplicationError> {
        dispatch.require_dispatchable()
    }
}
