use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use vestrace_domain::{MemoryId, now};

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

/// What a purge actually removed, per table.
///
/// Returned rather than discarded because the alternative was demonstrated
/// live: running unscoped against tables whose policy is forced, every `DELETE`
/// matched zero rows, the audit row inserted successfully, and the service
/// reported success. A purge that removes nothing must not be able to say it
/// worked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PurgeOutcome {
    pub removed: std::collections::BTreeMap<String, u64>,
}

impl PurgeOutcome {
    pub fn removed_anything(&self) -> bool {
        self.removed.values().any(|count| *count > 0)
    }

    pub fn count(&self, table: &str) -> u64 {
        self.removed.get(table).copied().unwrap_or(0)
    }
}

#[async_trait]
pub trait PurgeRepository: Send + Sync {
    /// Destroy a memory and everything that points at it, inside the caller's
    /// workspace scope.
    ///
    /// The workspace and principal come from the context rather than as
    /// arguments: they used to be passed separately, which allowed a caller to
    /// name one workspace and act in another, and left the adapter free to run
    /// on an unscoped connection — which is exactly what it did.
    async fn purge_memory(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
        reason: &str,
        approval_id: &str,
    ) -> Result<PurgeOutcome, ApplicationError>;
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
    ) -> Result<PurgeOutcome, ApplicationError> {
        let authorization = self
            .auth_port
            .authorize(ctx, cmd.memory_id, &cmd.reason)
            .await?;

        if authorization.expires_at < now() {
            return Err(ApplicationError::Policy(
                "purge authorization has expired".to_owned(),
            ));
        }

        let outcome = self
            .purge_repo
            .purge_memory(ctx, cmd.memory_id, &cmd.reason, &cmd.approval_id)
            .await?;

        // A purge that removed nothing is a purge that did not happen. Saying
        // so here means a caller cannot read success and believe the memory is
        // gone, and it is the difference between an irreversible act and a
        // report of one.
        if !outcome.removed_anything() {
            return Err(ApplicationError::Domain(
                vestrace_domain::DomainError::NotFound(format!(
                    "memory {} is not in this workspace, so nothing was purged",
                    cmd.memory_id
                )),
            ));
        }

        Ok(outcome)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vestrace_domain::id::PrincipalId;
    use vestrace_domain::{DomainError, WorkspaceId};

    /// Records what it was asked to purge, and removes whatever it was told to.
    struct RecordingPurge {
        calls: Mutex<Vec<(MemoryId, String, String)>>,
        outcome: PurgeOutcome,
    }

    impl RecordingPurge {
        fn removing(memories: u64) -> Self {
            let mut outcome = PurgeOutcome::default();
            outcome.removed.insert("memories".to_string(), memories);
            outcome
                .removed
                .insert("memory_revisions".to_string(), memories * 2);
            Self {
                calls: Mutex::new(Vec::new()),
                outcome,
            }
        }
    }

    #[async_trait]
    impl PurgeRepository for std::sync::Arc<RecordingPurge> {
        async fn purge_memory(
            &self,
            context: &RequestContext,
            memory_id: MemoryId,
            reason: &str,
            approval_id: &str,
        ) -> Result<PurgeOutcome, ApplicationError> {
            self.as_ref()
                .purge_memory(context, memory_id, reason, approval_id)
                .await
        }
    }

    #[async_trait]
    impl PurgeRepository for RecordingPurge {
        async fn purge_memory(
            &self,
            _context: &RequestContext,
            memory_id: MemoryId,
            reason: &str,
            approval_id: &str,
        ) -> Result<PurgeOutcome, ApplicationError> {
            self.calls.lock().unwrap().push((
                memory_id,
                reason.to_string(),
                approval_id.to_string(),
            ));
            Ok(self.outcome.clone())
        }
    }

    fn context() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    fn command(memory_id: MemoryId) -> HardPurgeMemoryCommand {
        HardPurgeMemoryCommand {
            memory_id,
            reason: "subject exercised erasure".to_owned(),
            approval_id: "approval-2026-08-14-001".to_owned(),
        }
    }

    /// The defect this exists to prevent: the repository ran unscoped against
    /// tables with forced row-level security, so every `DELETE` matched nothing
    /// and the service returned `Ok`. A caller reading that would believe the
    /// memory was destroyed.
    #[tokio::test]
    async fn a_purge_that_removed_nothing_is_not_reported_as_success() {
        let service = HardPurgeMemoryService::new(
            DeterministicPurgeAuthorizer,
            RecordingPurge::removing(0),
        );

        let error = service
            .execute(&context(), command(MemoryId::new()))
            .await
            .expect_err("removing nothing must not succeed");

        assert!(
            matches!(
                error,
                ApplicationError::Domain(DomainError::NotFound(ref message))
                    if message.contains("nothing was purged")
            ),
            "unexpected error: {error:?}"
        );
    }

    /// What was destroyed is returned, per table, so the caller and the audit
    /// are describing the same event.
    #[tokio::test]
    async fn a_purge_reports_what_it_removed() {
        let service = HardPurgeMemoryService::new(
            DeterministicPurgeAuthorizer,
            RecordingPurge::removing(1),
        );

        let outcome = service
            .execute(&context(), command(MemoryId::new()))
            .await
            .expect("a memory was removed");

        assert!(outcome.removed_anything());
        assert_eq!(outcome.count("memories"), 1);
        assert_eq!(outcome.count("memory_revisions"), 2);
        assert_eq!(outcome.count("a_table_nobody_touched"), 0);
    }

    /// The approval that permitted an irreversible act must reach the thing
    /// that records it. This used to end at `let _ = approval_id;` in the
    /// adapter, so no audit row could name what authorized the purge.
    #[tokio::test]
    async fn the_approval_reaches_the_repository() {
        let purge = std::sync::Arc::new(RecordingPurge::removing(1));
        let memory_id = MemoryId::new();
        let service = HardPurgeMemoryService::new(DeterministicPurgeAuthorizer, purge.clone());

        service
            .execute(&context(), command(memory_id))
            .await
            .expect("a memory was removed");

        let calls = purge.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, memory_id);
        assert_eq!(calls[0].1, "subject exercised erasure");
        assert_eq!(calls[0].2, "approval-2026-08-14-001");
    }
}

/// Purge as a use case, so a surface can hold one without knowing which
/// authorizer and which repository it was built from.
#[async_trait]
pub trait PurgeUseCase: Send + Sync {
    async fn purge(
        &self,
        context: &RequestContext,
        command: HardPurgeMemoryCommand,
    ) -> Result<PurgeOutcome, ApplicationError>;
}

pub type SharedPurgeUseCase = std::sync::Arc<dyn PurgeUseCase>;

#[async_trait]
impl<A, P> PurgeUseCase for HardPurgeMemoryService<A, P>
where
    A: PurgeAuthorizationPort,
    P: PurgeRepository,
{
    async fn purge(
        &self,
        context: &RequestContext,
        command: HardPurgeMemoryCommand,
    ) -> Result<PurgeOutcome, ApplicationError> {
        self.execute(context, command).await
    }
}

/// Authorization for a purge, taken from the same grant store every other
/// governed request consults.
///
/// # Why the deterministic authorizer could not be wired
///
/// `DeterministicPurgeAuthorizer` returns a fixed reference and an hour's
/// validity for anybody who asks. It is a fixture, and putting it behind an
/// HTTP surface would have meant the one irreversible operation in the system
/// authorizing itself — with an audit row naming `deterministic-test-approval`
/// as the thing that permitted it.
///
/// This asks the policy engine for `memory.purge` against the memory's own
/// scope, at critical risk, and carries the decision's identifier forward as the
/// authorization reference. So the audit names a decision that was really made,
/// against a grant that can be listed and revoked, and revoking it closes the
/// surface on the next request.
pub struct GrantedPurgeAuthorizer {
    boundary: crate::AuthorizationBoundary,
    validity: chrono::Duration,
}

impl GrantedPurgeAuthorizer {
    pub fn new(boundary: crate::AuthorizationBoundary) -> Self {
        Self {
            boundary,
            // Short: the authorization is checked immediately before the
            // deletion it permits, and a window is only useful to something
            // that might act later.
            validity: chrono::Duration::minutes(5),
        }
    }
}

#[async_trait]
impl PurgeAuthorizationPort for GrantedPurgeAuthorizer {
    async fn authorize(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
        reason: &str,
    ) -> Result<PurgeAuthorization, ApplicationError> {
        if reason.trim().is_empty() {
            return Err(ApplicationError::Domain(
                vestrace_domain::DomainError::InvalidArgument(
                    "a purge requires a reason".into(),
                ),
            ));
        }
        let request = vestrace_domain::AuthorizationRequest::new(
            vestrace_domain::Capability::MemoryPurge,
            "memory.purge",
            format!("memory://{memory_id}"),
            // Destroying a memory is the most consequential thing this system
            // can be asked to do, so it is requested at the top of the scale and
            // a grant with a lower ceiling refuses it.
            vestrace_domain::RiskCategory::Critical,
        );
        let decision = self.boundary.require(context, request).await?;
        Ok(PurgeAuthorization {
            authorization_reference: decision.id.to_string(),
            expires_at: now() + self.validity,
        })
    }
}
