use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use vestrace_domain::{MemoryId, WorkspaceId, now};

#[derive(Clone, Debug, PartialEq)]
pub struct PurgeAuthorization {
    pub authorization_reference: String,
    pub expires_at: vestrace_domain::time::Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HardPurgeMemoryCommand {
    pub memory_id: MemoryId,
    pub reason: String,
    pub approval_id: String,
}

#[async_trait]
pub trait PurgeAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
        reason: &str,
    ) -> Result<PurgeAuthorization, ApplicationError>;
}

#[async_trait]
pub trait PurgeRepository: Send + Sync {
    async fn purge_memory(
        &self,
        workspace_id: WorkspaceId,
        memory_id: MemoryId,
        principal_id: vestrace_domain::PrincipalId,
        reason: &str,
        approval_id: &str,
    ) -> Result<(), ApplicationError>;
}

pub struct HardPurgeMemoryService<A, P>
where
    A: PurgeAuthorizationPort,
    P: PurgeRepository,
{
    auth_port: A,
    purge_repo: P,
}

impl<A, P> HardPurgeMemoryService<A, P>
where
    A: PurgeAuthorizationPort,
    P: PurgeRepository,
{
    pub fn new(auth_port: A, purge_repo: P) -> Self {
        Self {
            auth_port,
            purge_repo,
        }
    }

    pub async fn execute(
        &self,
        ctx: &RequestContext,
        cmd: HardPurgeMemoryCommand,
    ) -> Result<(), ApplicationError> {
        let authorization = self
            .auth_port
            .authorize(ctx, cmd.memory_id, &cmd.reason)
            .await?;

        if authorization.expires_at < now() {
            return Err(ApplicationError::Policy(
                "purge authorization has expired".to_owned(),
            ));
        }

        self.purge_repo
            .purge_memory(
                ctx.workspace_id,
                cmd.memory_id,
                ctx.principal_id,
                &cmd.reason,
                &cmd.approval_id,
            )
            .await?;

        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeterministicPurgeAuthorizer;

#[async_trait]
impl PurgeAuthorizationPort for DeterministicPurgeAuthorizer {
    async fn authorize(
        &self,
        _context: &RequestContext,
        _memory_id: MemoryId,
        _reason: &str,
    ) -> Result<PurgeAuthorization, ApplicationError> {
        Ok(PurgeAuthorization {
            authorization_reference: "deterministic-test-approval".to_owned(),
            expires_at: now() + chrono::Duration::hours(1),
        })
    }
}
