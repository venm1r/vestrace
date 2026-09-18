use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::CrossWorkspaceMemoryGrant;
use crate::{
    DomainError,
    id::{
        MemoryGrantId, MemoryId, MemoryMountId, MemoryRevisionId, MemoryShareGrantRevisionId,
        PrincipalId, WorkspaceId,
    },
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ShareOperation {
    DiscoverMetadata,
    ReadContent,
    IncludeContext,
    SendToProvider,
    Index,
    Cache,
    DeriveLocal,
    Export,
    ReShare,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShareTarget {
    ExactWorkspace(WorkspaceId),
    AnyWorkspace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryShareGrantRevisionSpec {
    pub source_workspace_id: WorkspaceId,
    pub target: ShareTarget,
    pub memory_id: MemoryId,
    pub memory_revision_id: MemoryRevisionId,
    pub source_generation: String,
    pub operations: BTreeSet<ShareOperation>,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct MemoryShareGrantRevision {
    id: MemoryShareGrantRevisionId,
    source_workspace_id: WorkspaceId,
    target_workspace_id: WorkspaceId,
    memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    source_generation: String,
    operations: BTreeSet<ShareOperation>,
    valid_from: Timestamp,
    valid_until: Option<Timestamp>,
}

impl MemoryShareGrantRevision {
    pub fn issue(
        spec: MemoryShareGrantRevisionSpec,
        _created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let target_workspace_id = match spec.target {
            ShareTarget::ExactWorkspace(workspace_id) => workspace_id,
            ShareTarget::AnyWorkspace => {
                return Err(DomainError::PolicyViolation(
                    "wildcard share targets are prohibited".into(),
                ));
            }
        };
        if spec.source_workspace_id == target_workspace_id {
            return Err(DomainError::InvalidArgument(
                "cross-workspace share requires distinct source and target workspaces".into(),
            ));
        }
        if spec.operations.is_empty() {
            return Err(DomainError::InvalidArgument(
                "share grant revision must contain at least one operation".into(),
            ));
        }
        if spec.operations.contains(&ShareOperation::ReShare) {
            return Err(DomainError::PolicyViolation(
                "mounted memory cannot be granted as a transitive source".into(),
            ));
        }
        if spec.source_generation.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "share grant source generation must not be empty".into(),
            ));
        }
        if spec
            .valid_until
            .map(|valid_until| valid_until <= spec.valid_from)
            .unwrap_or(false)
        {
            return Err(DomainError::InvalidArgument(
                "share grant valid_until must be after valid_from".into(),
            ));
        }

        Ok(Self {
            id: MemoryShareGrantRevisionId::new(),
            source_workspace_id: spec.source_workspace_id,
            target_workspace_id,
            memory_id: spec.memory_id,
            memory_revision_id: spec.memory_revision_id,
            source_generation: spec.source_generation,
            operations: spec.operations,
            valid_from: spec.valid_from,
            valid_until: spec.valid_until,
        })
    }

    pub fn id(&self) -> MemoryShareGrantRevisionId {
        self.id
    }

    pub fn source_workspace_id(&self) -> WorkspaceId {
        self.source_workspace_id
    }

    pub fn target_workspace_id(&self) -> WorkspaceId {
        self.target_workspace_id
    }

    pub fn memory_id(&self) -> MemoryId {
        self.memory_id
    }

    pub fn memory_revision_id(&self) -> MemoryRevisionId {
        self.memory_revision_id
    }

    pub fn source_generation(&self) -> &str {
        &self.source_generation
    }

    pub fn operations(&self) -> &BTreeSet<ShareOperation> {
        &self.operations
    }

    pub fn valid_from(&self) -> Timestamp {
        self.valid_from
    }

    pub fn valid_until(&self) -> Option<Timestamp> {
        self.valid_until
    }

    fn allows(&self, operation: ShareOperation, at: Timestamp) -> bool {
        at >= self.valid_from
            && self
                .valid_until
                .map(|valid_until| at < valid_until)
                .unwrap_or(true)
            && self.operations.contains(&operation)
    }

    fn shared_ref(&self) -> SharedMemoryRef {
        SharedMemoryRef {
            source_workspace_id: self.source_workspace_id,
            memory_id: self.memory_id,
            memory_revision_id: self.memory_revision_id,
            grant_revision_id: self.id,
            source_generation: self.source_generation.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShareGrantStatus {
    Active,
    Suspended,
    Revoked,
    Expired,
    Deleted,
}

#[derive(Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct MemoryShareGrant {
    id: MemoryGrantId,
    revision: MemoryShareGrantRevision,
    status: MemoryShareGrantStatus,
    created_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

impl MemoryShareGrant {
    pub fn issue(
        revision: MemoryShareGrantRevision,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if created_at < revision.valid_from() {
            return Err(DomainError::InvalidArgument(
                "share grant cannot be created before its revision valid_from".into(),
            ));
        }
        Ok(Self {
            id: MemoryGrantId::new(),
            revision,
            status: MemoryShareGrantStatus::Active,
            created_at,
            revoked_at: None,
        })
    }

    pub fn from_legacy(_legacy: &CrossWorkspaceMemoryGrant) -> Result<Self, DomainError> {
        Err(DomainError::PolicyViolation(
            "legacy one-sided grants require a new explicit source revision and target acceptance"
                .into(),
        ))
    }

    pub fn id(&self) -> MemoryGrantId {
        self.id
    }

    pub fn revision(&self) -> &MemoryShareGrantRevision {
        &self.revision
    }

    pub fn status(&self) -> MemoryShareGrantStatus {
        self.status
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn revoked_at(&self) -> Option<Timestamp> {
        self.revoked_at
    }

    pub fn revoke(&mut self, at: Timestamp) -> Result<(), DomainError> {
        if self.status != MemoryShareGrantStatus::Active {
            return Err(DomainError::PolicyViolation(format!(
                "cannot revoke share grant in status {:?}",
                self.status
            )));
        }
        self.status = MemoryShareGrantStatus::Revoked;
        self.revoked_at = Some(at);
        Ok(())
    }

    pub fn is_expired_at(&self, at: Timestamp) -> bool {
        self.status == MemoryShareGrantStatus::Expired
            || self
                .revision
                .valid_until()
                .map(|valid_until| at >= valid_until)
                .unwrap_or(false)
    }

    pub fn is_active_at(&self, at: Timestamp) -> bool {
        self.status == MemoryShareGrantStatus::Active
            && at >= self.revision.valid_from()
            && !self.is_expired_at(at)
    }

    fn allows(&self, operation: ShareOperation, at: Timestamp) -> bool {
        self.is_active_at(at) && self.revision.allows(operation, at)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TargetSharePolicy {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    operations: BTreeSet<ShareOperation>,
    valid_until: Option<Timestamp>,
}

impl TargetSharePolicy {
    pub fn new(
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        operations: BTreeSet<ShareOperation>,
        valid_until: Option<Timestamp>,
    ) -> Result<Self, DomainError> {
        if operations.is_empty() {
            return Err(DomainError::InvalidArgument(
                "target share policy must contain at least one operation".into(),
            ));
        }
        if operations.contains(&ShareOperation::ReShare) {
            return Err(DomainError::PolicyViolation(
                "target share policy cannot authorize transitive re-sharing".into(),
            ));
        }
        Ok(Self {
            workspace_id,
            principal_id,
            operations,
            valid_until,
        })
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub fn operations(&self) -> &BTreeSet<ShareOperation> {
        &self.operations
    }

    pub fn valid_until(&self) -> Option<Timestamp> {
        self.valid_until
    }

    /// Whether this policy permits one principal, in one workspace, to perform
    /// one operation at one moment.
    ///
    /// Public because it is the statement "a permission held here is held only
    /// here", and a conformance case has to be able to ask it directly rather
    /// than infer it from a decision that folds four checks into one answer.
    pub fn allows(
        &self,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        operation: ShareOperation,
        at: Timestamp,
    ) -> bool {
        self.workspace_id == workspace_id
            && self.principal_id == principal_id
            && self.operations.contains(&operation)
            && self
                .valid_until
                .map(|valid_until| at < valid_until)
                .unwrap_or(true)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryMountAcceptance {
    pub grant_id: MemoryGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub target_workspace_id: WorkspaceId,
    pub target_principal_id: PrincipalId,
    pub operations: BTreeSet<ShareOperation>,
    pub valid_until: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMountStatus {
    Active,
    Suspended,
    Stale,
    Revoked,
    Expired,
    Deleted,
}

#[derive(Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct MemoryMount {
    id: MemoryMountId,
    target_workspace_id: WorkspaceId,
    target_principal_id: PrincipalId,
    grant_id: MemoryGrantId,
    grant_revision_id: MemoryShareGrantRevisionId,
    operations: BTreeSet<ShareOperation>,
    accepted_at: Timestamp,
    valid_until: Option<Timestamp>,
    status: MemoryMountStatus,
    shared_ref: SharedMemoryRef,
    disclosures: Vec<ShareDisclosure>,
}

impl MemoryMount {
    pub fn accept(
        acceptance: MemoryMountAcceptance,
        grant: &MemoryShareGrant,
        target_policy: &TargetSharePolicy,
        accepted_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if !grant.is_active_at(accepted_at) {
            return Err(DomainError::PolicyViolation(
                "cannot accept a mount from an inactive source grant".into(),
            ));
        }
        if acceptance.grant_id != grant.id()
            || acceptance.grant_revision_id != grant.revision().id()
        {
            return Err(DomainError::PolicyViolation(
                "mount must accept the exact source grant revision".into(),
            ));
        }
        if acceptance.target_workspace_id != grant.revision().target_workspace_id()
            || acceptance.target_workspace_id != target_policy.workspace_id()
            || acceptance.target_principal_id != target_policy.principal_id()
        {
            return Err(DomainError::PolicyViolation(
                "mount target does not match the exact grant and target policy".into(),
            ));
        }
        if acceptance.operations.is_empty()
            || acceptance.operations.contains(&ShareOperation::ReShare)
            || !acceptance
                .operations
                .is_subset(grant.revision().operations())
            || !acceptance.operations.is_subset(target_policy.operations())
        {
            return Err(DomainError::PolicyViolation(
                "mount operations must be a non-empty intersection of source and target policy"
                    .into(),
            ));
        }
        let valid_until = min_timestamp(
            acceptance.valid_until,
            min_timestamp(grant.revision().valid_until(), target_policy.valid_until()),
        );
        if valid_until
            .map(|valid_until| valid_until <= accepted_at)
            .unwrap_or(false)
        {
            return Err(DomainError::InvalidArgument(
                "mount valid_until must be after accepted_at".into(),
            ));
        }

        Ok(Self {
            id: MemoryMountId::new(),
            target_workspace_id: acceptance.target_workspace_id,
            target_principal_id: acceptance.target_principal_id,
            grant_id: acceptance.grant_id,
            grant_revision_id: acceptance.grant_revision_id,
            operations: acceptance.operations,
            accepted_at,
            valid_until,
            status: MemoryMountStatus::Active,
            shared_ref: grant.revision().shared_ref(),
            disclosures: Vec::new(),
        })
    }

    pub fn id(&self) -> MemoryMountId {
        self.id
    }

    pub fn target_workspace_id(&self) -> WorkspaceId {
        self.target_workspace_id
    }

    pub fn target_principal_id(&self) -> PrincipalId {
        self.target_principal_id
    }

    pub fn grant_id(&self) -> MemoryGrantId {
        self.grant_id
    }

    pub fn grant_revision_id(&self) -> MemoryShareGrantRevisionId {
        self.grant_revision_id
    }

    pub fn operations(&self) -> &BTreeSet<ShareOperation> {
        &self.operations
    }

    pub fn accepted_at(&self) -> Timestamp {
        self.accepted_at
    }

    pub fn valid_until(&self) -> Option<Timestamp> {
        self.valid_until
    }

    pub fn status(&self) -> MemoryMountStatus {
        self.status
    }

    pub fn sync_with_source(&mut self, grant: &MemoryShareGrant, at: Timestamp) {
        if self.status != MemoryMountStatus::Active {
            return;
        }
        if self.grant_id != grant.id() || self.grant_revision_id != grant.revision().id() {
            self.status = MemoryMountStatus::Stale;
        } else if grant.status() == MemoryShareGrantStatus::Revoked {
            self.status = MemoryMountStatus::Revoked;
        } else if grant.is_expired_at(at)
            || self
                .valid_until
                .map(|valid_until| at >= valid_until)
                .unwrap_or(false)
        {
            self.status = MemoryMountStatus::Expired;
        } else if !grant.is_active_at(at) {
            self.status = MemoryMountStatus::Stale;
        }
    }

    pub fn shared_ref(
        &self,
        grant: &MemoryShareGrant,
        at: Timestamp,
    ) -> Result<SharedMemoryRef, DomainError> {
        if self.grant_id != grant.id() || self.grant_revision_id != grant.revision().id() {
            return Err(DomainError::PolicyViolation(
                "mount must reference the exact source grant revision".into(),
            ));
        }
        if self.status != MemoryMountStatus::Active
            || !grant.is_active_at(at)
            || at < self.accepted_at
            || self
                .valid_until
                .map(|valid_until| at >= valid_until)
                .unwrap_or(false)
        {
            return Err(DomainError::PolicyViolation(
                "inactive, revoked, expired, or stale mount cannot disclose content".into(),
            ));
        }
        Ok(self.shared_ref.clone())
    }

    pub fn record_disclosure(
        &mut self,
        grant: &MemoryShareGrant,
        target_policy: &TargetSharePolicy,
        operation: ShareOperation,
        at: Timestamp,
    ) -> Result<ShareDisclosure, DomainError> {
        let decision = evaluate_share_access(grant, self, target_policy, operation, at);
        if !decision.is_allowed() {
            return Err(DomainError::PolicyViolation(format!(
                "share disclosure denied: {:?}",
                decision.reason()
            )));
        }
        let disclosure = ShareDisclosure {
            shared_ref: self.shared_ref.clone(),
            operation,
            disclosed_at: at,
        };
        self.disclosures.push(disclosure.clone());
        Ok(disclosure)
    }

    /// Derive something local from mounted content.
    ///
    /// # Why this is the only way
    ///
    /// IDW-011 requires a local derivation from mounted content to preserve its
    /// source provenance, and until now nothing derived from a mount at all:
    /// `ShareOperation::DeriveLocal` could be granted and nothing consumed it,
    /// and `Derivation` could only cite a `MemoryRevisionRef`, which names a
    /// memory and a revision and says nothing about whose they are. A week
    /// later that record is indistinguishable from a derivation from local
    /// content.
    ///
    /// So the derivation is produced *by the mount*: the input reference is
    /// built here from what the mount knows — source workspace, exact memory
    /// and revision, the grant revision that made it reachable — and there is
    /// no constructor that takes a bare local identifier instead.
    ///
    /// It is also a disclosure. Deriving from borrowed content is a use of it,
    /// and the same evaluation that governs reading governs this: a mount whose
    /// operations do not include `DeriveLocal` is refused, and the use is
    /// recorded where the source can see it.
    pub fn derive_local(
        &mut self,
        grant: &MemoryShareGrant,
        target_policy: &TargetSharePolicy,
        method: crate::provenance::DerivationMethod,
        created_by: PrincipalId,
        at: Timestamp,
    ) -> Result<crate::provenance::Derivation, DomainError> {
        let disclosure =
            self.record_disclosure(grant, target_policy, ShareOperation::DeriveLocal, at)?;
        let shared = disclosure.shared_ref();

        let mut derivation = crate::provenance::Derivation::new(
            crate::id::DerivationId::new(),
            self.target_workspace_id,
            method,
            at,
        );
        derivation.input_refs = vec![crate::provenance::EvidenceRef::SharedMemoryRevisionRef {
            source_workspace_id: shared.source_workspace_id(),
            memory_id: shared.source_memory_id(),
            revision_id: shared.memory_revision_id(),
            grant_revision_id: shared.grant_revision_id(),
            source_generation: shared.source_generation().to_owned(),
        }];
        derivation.created_by = Some(created_by);
        Ok(derivation)
    }

    pub fn disclosures(&self) -> &[ShareDisclosure] {
        &self.disclosures
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct SharedMemoryRef {
    source_workspace_id: WorkspaceId,
    memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    grant_revision_id: MemoryShareGrantRevisionId,
    source_generation: String,
}

impl SharedMemoryRef {
    pub fn source_workspace_id(&self) -> WorkspaceId {
        self.source_workspace_id
    }

    pub fn source_memory_id(&self) -> MemoryId {
        self.memory_id
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
}

mod idw_010_compile_time_boundary {
    use std::{borrow::Borrow, ops::Deref};

    use static_assertions::assert_not_impl_any;

    use super::SharedMemoryRef;
    use crate::id::MemoryId;

    assert_not_impl_any!(SharedMemoryRef:
        Into<MemoryId>,
        AsRef<MemoryId>,
        Borrow<MemoryId>,
        Deref<Target = MemoryId>
    );
    assert_not_impl_any!(MemoryId:
        From<SharedMemoryRef>,
        From<&'static SharedMemoryRef>
    );
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ShareDisclosure {
    shared_ref: SharedMemoryRef,
    operation: ShareOperation,
    disclosed_at: Timestamp,
}

impl ShareDisclosure {
    pub fn shared_ref(&self) -> &SharedMemoryRef {
        &self.shared_ref
    }

    pub fn operation(&self) -> ShareOperation {
        self.operation
    }

    pub fn disclosed_at(&self) -> Timestamp {
        self.disclosed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShareAccessReason {
    Allowed,
    SourceGrantInactive,
    SourceOperationDenied,
    GrantRevisionMismatch,
    MountInactive,
    MountOperationDenied,
    TargetPolicyDenied,
    TransitiveSharingProhibited,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ShareAccessDecision {
    allowed: bool,
    reason: ShareAccessReason,
    grant_id: MemoryGrantId,
    grant_revision_id: MemoryShareGrantRevisionId,
    mount_id: MemoryMountId,
    decided_at: Timestamp,
}

impl ShareAccessDecision {
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn reason(&self) -> ShareAccessReason {
        self.reason
    }

    pub fn grant_id(&self) -> MemoryGrantId {
        self.grant_id
    }

    pub fn grant_revision_id(&self) -> MemoryShareGrantRevisionId {
        self.grant_revision_id
    }

    pub fn mount_id(&self) -> MemoryMountId {
        self.mount_id
    }

    pub fn decided_at(&self) -> Timestamp {
        self.decided_at
    }
}

pub fn evaluate_share_access(
    grant: &MemoryShareGrant,
    mount: &MemoryMount,
    target_policy: &TargetSharePolicy,
    operation: ShareOperation,
    at: Timestamp,
) -> ShareAccessDecision {
    let reason = if operation == ShareOperation::ReShare {
        ShareAccessReason::TransitiveSharingProhibited
    } else if !grant.is_active_at(at) {
        ShareAccessReason::SourceGrantInactive
    } else if !grant.allows(operation, at) {
        ShareAccessReason::SourceOperationDenied
    } else if mount.grant_id != grant.id() || mount.grant_revision_id != grant.revision().id() {
        ShareAccessReason::GrantRevisionMismatch
    } else if !mount.is_active_at(at) {
        ShareAccessReason::MountInactive
    } else if !mount.operations.contains(&operation) {
        ShareAccessReason::MountOperationDenied
    } else if !target_policy.allows(
        mount.target_workspace_id,
        mount.target_principal_id,
        operation,
        at,
    ) {
        ShareAccessReason::TargetPolicyDenied
    } else {
        ShareAccessReason::Allowed
    };

    ShareAccessDecision {
        allowed: reason == ShareAccessReason::Allowed,
        reason,
        grant_id: grant.id(),
        grant_revision_id: grant.revision().id(),
        mount_id: mount.id(),
        decided_at: at,
    }
}

impl MemoryMount {
    fn is_active_at(&self, at: Timestamp) -> bool {
        self.status == MemoryMountStatus::Active
            && at >= self.accepted_at
            && self
                .valid_until
                .map(|valid_until| at < valid_until)
                .unwrap_or(true)
    }
}

fn min_timestamp(left: Option<Timestamp>, right: Option<Timestamp>) -> Option<Timestamp> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
