use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use std::collections::HashSet;
use vestrace_domain::{Capability, WorkspaceId};

#[async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError>;
}

pub struct AllowAllPolicyEngine;

#[async_trait]
impl PolicyEngine for AllowAllPolicyEngine {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        _capability: Capability,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

pub struct CapabilitySetPolicyEngine {
    capabilities: HashSet<Capability>,
}

impl CapabilitySetPolicyEngine {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }

    pub fn from_strings(strings: &[&str]) -> Result<Self, ApplicationError> {
        let mut caps = HashSet::new();
        for s in strings {
            caps.insert(s.parse::<Capability>().map_err(ApplicationError::Domain)?);
        }
        Ok(Self { capabilities: caps })
    }
}

#[async_trait]
impl PolicyEngine for CapabilitySetPolicyEngine {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(ApplicationError::Policy(format!(
                "missing required capability: {capability}"
            )))
        }
    }
}

pub struct WorkspaceScopedPolicyEngine {
    inner: CapabilitySetPolicyEngine,
    workspace_id: WorkspaceId,
}

impl WorkspaceScopedPolicyEngine {
    pub fn new(
        workspace_id: WorkspaceId,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            inner: CapabilitySetPolicyEngine::new(capabilities),
            workspace_id,
        }
    }
}

#[async_trait]
impl PolicyEngine for WorkspaceScopedPolicyEngine {
    async fn evaluate(
        &self,
        context: &RequestContext,
        capability: Capability,
    ) -> Result<(), ApplicationError> {
        if context.workspace_id != self.workspace_id {
            return Err(ApplicationError::Policy("workspace mismatch".to_owned()));
        }
        self.inner.evaluate(context, capability).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::{PrincipalId, WorkspaceId};

    fn ctx() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    #[tokio::test]
    async fn allow_all_permits_everything() {
        let engine = AllowAllPolicyEngine;
        let result = engine.evaluate(&ctx(), Capability::MemoryPurge).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn capability_set_denies_missing() {
        let engine = CapabilitySetPolicyEngine::new(vec![Capability::MemoryRead]);
        let result = engine.evaluate(&ctx(), Capability::MemoryPurge).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn capability_set_allows_present() {
        let engine =
            CapabilitySetPolicyEngine::new(vec![Capability::MemoryRead, Capability::MemoryWrite]);
        let result = engine.evaluate(&ctx(), Capability::MemoryRead).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn workspace_scoped_rejects_mismatch() {
        let ws = WorkspaceId::new();
        let engine = WorkspaceScopedPolicyEngine::new(ws, vec![Capability::MemoryRead]);
        let result = engine.evaluate(&ctx(), Capability::MemoryRead).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn workspace_scoped_allows_match() {
        let ws = WorkspaceId::new();
        let principal = PrincipalId::new();
        let context = RequestContext::new(ws, principal);
        let engine = WorkspaceScopedPolicyEngine::new(ws, vec![Capability::MemoryRead]);
        let result = engine.evaluate(&context, Capability::MemoryRead).await;
        assert!(result.is_ok());
    }
}
