//! A cache only: every consumer must validate PostgreSQL generation authority.
use super::FlatEmbeddingIndex;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError,
    embedding::index::{EmbeddingIndexRegistryPort, LocalEmbeddingIndex},
};
use vestrace_domain::{WorkspaceId, embedding::CanonicalGenerationSnapshot};
#[derive(Default)]
pub struct EmbeddingIndexRegistry {
    entries: RwLock<HashMap<(WorkspaceId, Uuid), Arc<FlatEmbeddingIndex>>>,
}
impl EmbeddingIndexRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn remove_space_before_epoch(
        &self,
        workspace: WorkspaceId,
        registration: Uuid,
        committed_epoch: u64,
    ) {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (workspace, registration);
        if entries
            .get(&key)
            .is_some_and(|index| index.snapshot().generation_epoch < committed_epoch)
        {
            entries.remove(&key);
        }
    }
}
impl EmbeddingIndexRegistryPort<FlatEmbeddingIndex> for EmbeddingIndexRegistry {
    fn get(
        &self,
        snapshot: &CanonicalGenerationSnapshot,
        registration: Uuid,
    ) -> Option<Arc<FlatEmbeddingIndex>> {
        self.entries
            .read()
            .ok()?
            .get(&(snapshot.workspace_id, registration))
            .filter(|index| index.snapshot() == snapshot)
            .cloned()
    }
    fn install(&self, index: Arc<FlatEmbeddingIndex>) -> Result<(), ApplicationError> {
        let mut entries = self.entries.write().map_err(|_| {
            ApplicationError::Unavailable("embedding-index-registry-unavailable".into())
        })?;
        let key = (index.snapshot().workspace_id, index.space_registration_id());
        if entries.get(&key).is_some_and(|previous| {
            previous.snapshot().generation_epoch >= index.snapshot().generation_epoch
                && previous.snapshot() != index.snapshot()
        }) {
            return Err(ApplicationError::Unavailable(
                "embedding-generation-changed".into(),
            ));
        }
        entries.insert(key, index);
        Ok(())
    }
    fn remove_exact(&self, snapshot: &CanonicalGenerationSnapshot, registration: Uuid) {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (snapshot.workspace_id, registration);
        if entries
            .get(&key)
            .is_some_and(|index| index.snapshot() == snapshot)
        {
            entries.remove(&key);
        }
    }
    fn remove_space_before_epoch(
        &self,
        workspace: WorkspaceId,
        registration: Uuid,
        committed_epoch: u64,
    ) {
        EmbeddingIndexRegistry::remove_space_before_epoch(
            self,
            workspace,
            registration,
            committed_epoch,
        );
    }
}
