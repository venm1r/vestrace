use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    Timestamp,
    enterprise::{
        MemoryMount, MemoryShareGrant, ShareDisclosure, ShareOperation, SharedMemoryRef,
        TargetSharePolicy, evaluate_share_access,
    },
    id::{MemoryId, MemoryRevisionId, MemoryShareGrantRevisionId, PrincipalId, WorkspaceId},
};

use crate::{ApplicationError, RequestContext};

pub trait SharedMemoryReadClock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub struct SystemSharedMemoryReadClock;

impl SystemSharedMemoryReadClock {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemSharedMemoryReadClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedMemoryReadClock for SystemSharedMemoryReadClock {
    fn now(&self) -> Timestamp {
        vestrace_domain::time::now()
    }
}

/// An exact, context-bound authority to read one shared memory revision.
///
/// ```compile_fail
/// use vestrace_application::SharedMemoryReadPermit;
/// use vestrace_domain::{
///     id::{MemoryId, MemoryRevisionId, MemoryShareGrantRevisionId, PrincipalId, WorkspaceId},
/// };
///
/// let _permit = SharedMemoryReadPermit {
///     source_workspace_id: todo!(),
///     source_memory_id: todo!(),
///     memory_revision_id: todo!(),
///     grant_revision_id: todo!(),
///     source_generation: todo!(),
///     target_workspace_id: todo!(),
///     target_principal_id: todo!(),
/// };
/// ```
///
/// ```compile_fail
/// use vestrace_application::SharedMemoryReadPermit;
///
/// let _permit = SharedMemoryReadPermit::issue(todo!(), todo!());
/// ```
pub struct SharedMemoryReadPermit {
    source_workspace_id: WorkspaceId,
    source_memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    grant_revision_id: MemoryShareGrantRevisionId,
    source_generation: String,
    target_workspace_id: WorkspaceId,
    target_principal_id: PrincipalId,
}

impl SharedMemoryReadPermit {
    fn issue(shared_ref: &SharedMemoryRef, context: &RequestContext) -> Self {
        Self {
            source_workspace_id: shared_ref.source_workspace_id(),
            source_memory_id: shared_ref.source_memory_id(),
            memory_revision_id: shared_ref.memory_revision_id(),
            grant_revision_id: shared_ref.grant_revision_id(),
            source_generation: shared_ref.source_generation().to_owned(),
            target_workspace_id: context.workspace_id,
            target_principal_id: context.principal_id,
        }
    }

    pub fn source_workspace_id(&self) -> WorkspaceId {
        self.source_workspace_id
    }

    pub fn source_memory_id(&self) -> MemoryId {
        self.source_memory_id
    }

    pub fn memory_revision_id(&self) -> MemoryRevisionId {
        self.memory_revision_id
    }

    pub fn grant_revision_id(&self) -> MemoryShareGrantRevisionId {
        self.grant_revision_id
    }

    pub fn source_generation(&self) -> &str {
        &self.source_generation
    }

    pub fn target_workspace_id(&self) -> WorkspaceId {
        self.target_workspace_id
    }

    pub fn target_principal_id(&self) -> PrincipalId {
        self.target_principal_id
    }
}

#[cfg(test)]
mod permit_unforgeability {
    use static_assertions::assert_not_impl_any;

    use super::SharedMemoryReadPermit;
    use vestrace_domain::{
        enterprise::SharedMemoryRef,
        id::{MemoryId, MemoryRevisionId, MemoryShareGrantRevisionId, PrincipalId, WorkspaceId},
    };

    assert_not_impl_any!(SharedMemoryReadPermit: Clone);
    assert_not_impl_any!(SharedMemoryReadPermit: Default);
    assert_not_impl_any!(SharedMemoryReadPermit: serde::Serialize);
    assert_not_impl_any!(SharedMemoryReadPermit: serde::de::DeserializeOwned);
    assert_not_impl_any!(SharedMemoryReadPermit: From<SharedMemoryRef>);
    assert_not_impl_any!(SharedMemoryReadPermit: From<WorkspaceId>);
    assert_not_impl_any!(SharedMemoryReadPermit: From<MemoryId>);
    assert_not_impl_any!(SharedMemoryReadPermit: From<MemoryRevisionId>);
    assert_not_impl_any!(SharedMemoryReadPermit: From<MemoryShareGrantRevisionId>);
    assert_not_impl_any!(SharedMemoryReadPermit: From<PrincipalId>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<SharedMemoryRef>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<WorkspaceId>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<MemoryId>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<MemoryRevisionId>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<MemoryShareGrantRevisionId>);
    assert_not_impl_any!(SharedMemoryReadPermit: Into<PrincipalId>);
}

#[async_trait]
pub trait SharedMemoryRevisionReader: Send + Sync {
    async fn read_exact(
        &self,
        permit: SharedMemoryReadPermit,
    ) -> Result<Option<SharedMemoryRevisionRecord>, ApplicationError>;
}

/// A reader-supplied record that remains namespaced to its source revision.
///
/// ```compile_fail
/// use vestrace_application::SharedMemoryRevisionRecord;
///
/// let _record = SharedMemoryRevisionRecord {
///     source_workspace_id: todo!(),
///     source_memory_id: todo!(),
///     memory_revision_id: todo!(),
///     content: todo!(),
/// };
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct SharedMemoryRevisionRecord {
    source_workspace_id: WorkspaceId,
    source_memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    content: String,
}

impl SharedMemoryRevisionRecord {
    pub fn new(
        source_workspace_id: WorkspaceId,
        source_memory_id: MemoryId,
        memory_revision_id: MemoryRevisionId,
        content: String,
    ) -> Self {
        Self {
            source_workspace_id,
            source_memory_id,
            memory_revision_id,
            content,
        }
    }

    pub fn source_workspace_id(&self) -> WorkspaceId {
        self.source_workspace_id
    }

    pub fn source_memory_id(&self) -> MemoryId {
        self.source_memory_id
    }

    pub fn memory_revision_id(&self) -> MemoryRevisionId {
        self.memory_revision_id
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SharedMemoryReadResult {
    shared_ref: SharedMemoryRef,
    content: String,
    disclosure: ShareDisclosure,
}

impl SharedMemoryReadResult {
    pub fn shared_ref(&self) -> &SharedMemoryRef {
        &self.shared_ref
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn disclosure(&self) -> &ShareDisclosure {
        &self.disclosure
    }
}

pub struct SharedMemoryReadService {
    reader: Arc<dyn SharedMemoryRevisionReader>,
    clock: Arc<dyn SharedMemoryReadClock>,
}

impl SharedMemoryReadService {
    pub fn new(
        reader: Arc<dyn SharedMemoryRevisionReader>,
        clock: Arc<dyn SharedMemoryReadClock>,
    ) -> Self {
        Self { reader, clock }
    }

    pub async fn read_shared(
        &self,
        context: &RequestContext,
        grant: &MemoryShareGrant,
        mount: &mut MemoryMount,
        target_policy: &TargetSharePolicy,
    ) -> Result<Option<SharedMemoryReadResult>, ApplicationError> {
        if context.workspace_id != mount.target_workspace_id()
            || context.principal_id != mount.target_principal_id()
        {
            return Err(ApplicationError::Policy(
                "shared read target identity does not match the accepted mount".into(),
            ));
        }

        let at = self.clock.now();
        let decision =
            evaluate_share_access(grant, mount, target_policy, ShareOperation::ReadContent, at);
        if !decision.is_allowed() {
            return Err(ApplicationError::Policy(format!(
                "shared read denied: {:?}",
                decision.reason()
            )));
        }

        let shared_ref = mount.shared_ref(grant, at)?;
        let permit = SharedMemoryReadPermit::issue(&shared_ref, context);
        let Some(record) = self.reader.read_exact(permit).await? else {
            return Ok(None);
        };
        if record.source_workspace_id() != shared_ref.source_workspace_id()
            || record.source_memory_id() != shared_ref.source_memory_id()
            || record.memory_revision_id() != shared_ref.memory_revision_id()
        {
            return Err(ApplicationError::Storage(
                "shared memory reader returned a revision outside the authorized namespace".into(),
            ));
        }

        let disclosure =
            mount.record_disclosure(grant, target_policy, ShareOperation::ReadContent, at)?;
        Ok(Some(SharedMemoryReadResult {
            shared_ref,
            content: record.content().to_owned(),
            disclosure,
        }))
    }
}
