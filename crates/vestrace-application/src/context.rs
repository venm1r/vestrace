use vestrace_domain::{CorrelationId, PrincipalId, RequestId, WorkspaceId};

#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub correlation_id: CorrelationId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
}

impl RequestContext {
    pub fn new(workspace_id: WorkspaceId, principal_id: PrincipalId) -> Self {
        Self {
            request_id: RequestId::new(),
            correlation_id: CorrelationId::new(),
            workspace_id,
            principal_id,
        }
    }
}
