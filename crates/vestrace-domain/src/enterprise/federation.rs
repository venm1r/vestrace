use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::sharing::{
    MemoryMount, MemoryShareGrant, ShareAccessDecision, ShareAccessReason, ShareOperation,
    TargetSharePolicy, evaluate_share_access,
};
use crate::{
    DomainError,
    id::{FederationRelationshipId, FederationTrustDecisionId, PrincipalId, WorkspaceId},
    time::Timestamp,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteIdentity {
    remote_workspace_id: WorkspaceId,
    remote_principal_id: PrincipalId,
    issuer: String,
    subject: String,
}

impl RemoteIdentity {
    pub fn new(
        remote_workspace_id: WorkspaceId,
        remote_principal_id: PrincipalId,
        issuer: impl Into<String>,
        subject: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let issuer = issuer.into();
        let subject = subject.into();
        if issuer.trim().is_empty() || subject.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "remote identity issuer and subject must not be empty".into(),
            ));
        }
        Ok(Self {
            remote_workspace_id,
            remote_principal_id,
            issuer,
            subject,
        })
    }

    pub fn remote_workspace_id(&self) -> WorkspaceId {
        self.remote_workspace_id
    }

    pub fn remote_principal_id(&self) -> PrincipalId {
        self.remote_principal_id
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FederationTrustPolicy {
    local_workspace_id: WorkspaceId,
    trusted_issuer: String,
    accepted_remote_profiles: BTreeSet<String>,
    operations: BTreeSet<ShareOperation>,
    version: String,
}

impl FederationTrustPolicy {
    pub fn new(
        local_workspace_id: WorkspaceId,
        trusted_issuer: impl Into<String>,
        accepted_remote_profiles: BTreeSet<String>,
        operations: BTreeSet<ShareOperation>,
        version: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let trusted_issuer = trusted_issuer.into();
        let version = version.into();
        if trusted_issuer.trim().is_empty() || version.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "federation trust issuer and policy version must not be empty".into(),
            ));
        }
        if accepted_remote_profiles.is_empty() || operations.is_empty() {
            return Err(DomainError::InvalidArgument(
                "federation trust policy must contain profiles and operations".into(),
            ));
        }
        if operations.contains(&ShareOperation::ReShare) {
            return Err(DomainError::PolicyViolation(
                "federation trust cannot authorize transitive re-sharing".into(),
            ));
        }
        Ok(Self {
            local_workspace_id,
            trusted_issuer,
            accepted_remote_profiles,
            operations,
            version,
        })
    }

    pub fn local_workspace_id(&self) -> WorkspaceId {
        self.local_workspace_id
    }

    pub fn trusted_issuer(&self) -> &str {
        &self.trusted_issuer
    }

    pub fn accepted_remote_profiles(&self) -> &BTreeSet<String> {
        &self.accepted_remote_profiles
    }

    pub fn operations(&self) -> &BTreeSet<ShareOperation> {
        &self.operations
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteQualificationEvidence {
    issuer: String,
    profile: String,
    evidence_ref: String,
    source_revision: String,
}

impl RemoteQualificationEvidence {
    pub fn new(
        issuer: impl Into<String>,
        profile: impl Into<String>,
        evidence_ref: impl Into<String>,
        source_revision: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let issuer = issuer.into();
        let profile = profile.into();
        let evidence_ref = evidence_ref.into();
        let source_revision = source_revision.into();
        if issuer.trim().is_empty()
            || profile.trim().is_empty()
            || evidence_ref.trim().is_empty()
            || source_revision.trim().is_empty()
        {
            return Err(DomainError::InvalidArgument(
                "remote qualification evidence must be complete".into(),
            ));
        }
        Ok(Self {
            issuer,
            profile,
            evidence_ref,
            source_revision,
        })
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn evidence_ref(&self) -> &str {
        &self.evidence_ref
    }

    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FederationRelationshipSpec {
    pub local_workspace_id: WorkspaceId,
    pub remote_workspace_id: WorkspaceId,
    pub remote_identity: RemoteIdentity,
    pub policy: FederationTrustPolicy,
    pub evidence: RemoteQualificationEvidence,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FederationRelationshipStatus {
    Active,
    Suspended,
    Revoked,
    Expired,
    Deleted,
}

#[derive(Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct FederationRelationship {
    id: FederationRelationshipId,
    local_workspace_id: WorkspaceId,
    remote_workspace_id: WorkspaceId,
    remote_identity: RemoteIdentity,
    policy: FederationTrustPolicy,
    evidence: RemoteQualificationEvidence,
    valid_from: Timestamp,
    valid_until: Option<Timestamp>,
    created_at: Timestamp,
    status: FederationRelationshipStatus,
    suspended_at: Option<Timestamp>,
    revoked_at: Option<Timestamp>,
}

impl FederationRelationship {
    pub fn establish(
        spec: FederationRelationshipSpec,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if spec.local_workspace_id == spec.remote_workspace_id
            || spec.remote_workspace_id != spec.remote_identity.remote_workspace_id()
            || spec.local_workspace_id != spec.policy.local_workspace_id()
        {
            return Err(DomainError::PolicyViolation(
                "federation relationship workspace binding is inconsistent".into(),
            ));
        }
        if spec.remote_identity.issuer() != spec.policy.trusted_issuer()
            || spec.evidence.issuer() != spec.policy.trusted_issuer()
            || !spec
                .policy
                .accepted_remote_profiles()
                .contains(spec.evidence.profile())
        {
            return Err(DomainError::PolicyViolation(
                "remote identity or qualification evidence is not trusted by local policy".into(),
            ));
        }
        if spec
            .valid_until
            .map(|valid_until| valid_until <= spec.valid_from)
            .unwrap_or(false)
        {
            return Err(DomainError::InvalidArgument(
                "federation relationship valid_until must be after valid_from".into(),
            ));
        }
        if created_at < spec.valid_from {
            return Err(DomainError::InvalidArgument(
                "federation relationship cannot be created before valid_from".into(),
            ));
        }

        Ok(Self {
            id: FederationRelationshipId::new(),
            local_workspace_id: spec.local_workspace_id,
            remote_workspace_id: spec.remote_workspace_id,
            remote_identity: spec.remote_identity,
            policy: spec.policy,
            evidence: spec.evidence,
            valid_from: spec.valid_from,
            valid_until: spec.valid_until,
            created_at,
            status: FederationRelationshipStatus::Active,
            suspended_at: None,
            revoked_at: None,
        })
    }

    pub fn id(&self) -> FederationRelationshipId {
        self.id
    }

    pub fn local_workspace_id(&self) -> WorkspaceId {
        self.local_workspace_id
    }

    pub fn remote_workspace_id(&self) -> WorkspaceId {
        self.remote_workspace_id
    }

    pub fn remote_identity(&self) -> &RemoteIdentity {
        &self.remote_identity
    }

    pub fn policy(&self) -> &FederationTrustPolicy {
        &self.policy
    }

    pub fn remote_qualification_evidence(&self) -> &RemoteQualificationEvidence {
        &self.evidence
    }

    pub fn status(&self) -> FederationRelationshipStatus {
        self.status
    }

    pub fn valid_from(&self) -> Timestamp {
        self.valid_from
    }

    pub fn valid_until(&self) -> Option<Timestamp> {
        self.valid_until
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn suspended_at(&self) -> Option<Timestamp> {
        self.suspended_at
    }

    pub fn revoked_at(&self) -> Option<Timestamp> {
        self.revoked_at
    }

    pub fn suspend(&mut self, at: Timestamp) -> Result<(), DomainError> {
        if self.status != FederationRelationshipStatus::Active {
            return Err(DomainError::PolicyViolation(format!(
                "cannot suspend federation relationship in status {:?}",
                self.status
            )));
        }
        self.status = FederationRelationshipStatus::Suspended;
        self.suspended_at = Some(at);
        Ok(())
    }

    pub fn resume(&mut self, _at: Timestamp) -> Result<(), DomainError> {
        if self.status != FederationRelationshipStatus::Suspended {
            return Err(DomainError::PolicyViolation(format!(
                "cannot resume federation relationship in status {:?}",
                self.status
            )));
        }
        self.status = FederationRelationshipStatus::Active;
        Ok(())
    }

    pub fn revoke(&mut self, at: Timestamp) -> Result<(), DomainError> {
        if !matches!(
            self.status,
            FederationRelationshipStatus::Active | FederationRelationshipStatus::Suspended
        ) {
            return Err(DomainError::PolicyViolation(format!(
                "cannot revoke federation relationship in status {:?}",
                self.status
            )));
        }
        self.status = FederationRelationshipStatus::Revoked;
        self.revoked_at = Some(at);
        Ok(())
    }

    fn is_active_at(&self, at: Timestamp) -> bool {
        self.status == FederationRelationshipStatus::Active
            && at >= self.valid_from
            && !self
                .valid_until
                .map(|valid_until| at >= valid_until)
                .unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FederationTrustReason {
    Allowed,
    LocalWorkspaceMismatch,
    RemoteIdentityMismatch,
    PolicyVersionMismatch,
    RelationshipSuspended,
    RelationshipRevoked,
    RelationshipExpired,
    RelationshipInactive,
    OperationDenied,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct FederationTrustDecision {
    allowed: bool,
    reason: FederationTrustReason,
    decision_id: FederationTrustDecisionId,
    relationship_id: FederationRelationshipId,
    local_workspace_id: WorkspaceId,
    remote_identity: RemoteIdentity,
    operation: ShareOperation,
    policy_version: String,
    decided_at: Timestamp,
}

impl FederationTrustDecision {
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn reason(&self) -> FederationTrustReason {
        self.reason
    }

    pub fn decision_id(&self) -> FederationTrustDecisionId {
        self.decision_id
    }

    pub fn relationship_id(&self) -> FederationRelationshipId {
        self.relationship_id
    }

    pub fn local_workspace_id(&self) -> WorkspaceId {
        self.local_workspace_id
    }

    pub fn remote_identity(&self) -> &RemoteIdentity {
        &self.remote_identity
    }

    pub fn operation(&self) -> ShareOperation {
        self.operation
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn decided_at(&self) -> Timestamp {
        self.decided_at
    }
}

pub fn evaluate_federation_trust(
    relationship: &FederationRelationship,
    identity: &RemoteIdentity,
    local_workspace_id: WorkspaceId,
    operation: ShareOperation,
    policy_version: &str,
    at: Timestamp,
) -> FederationTrustDecision {
    let reason = if relationship.local_workspace_id() != local_workspace_id {
        FederationTrustReason::LocalWorkspaceMismatch
    } else if relationship.remote_identity() != identity {
        FederationTrustReason::RemoteIdentityMismatch
    } else if relationship.policy().version() != policy_version {
        FederationTrustReason::PolicyVersionMismatch
    } else if relationship.status() == FederationRelationshipStatus::Suspended {
        FederationTrustReason::RelationshipSuspended
    } else if relationship.status() == FederationRelationshipStatus::Revoked {
        FederationTrustReason::RelationshipRevoked
    } else if relationship
        .valid_until()
        .map(|valid_until| at >= valid_until)
        .unwrap_or(false)
    {
        FederationTrustReason::RelationshipExpired
    } else if !relationship.is_active_at(at) {
        FederationTrustReason::RelationshipInactive
    } else if !relationship.policy().operations().contains(&operation) {
        FederationTrustReason::OperationDenied
    } else {
        FederationTrustReason::Allowed
    };

    FederationTrustDecision {
        allowed: reason == FederationTrustReason::Allowed,
        reason,
        decision_id: FederationTrustDecisionId::new(),
        relationship_id: relationship.id(),
        local_workspace_id,
        remote_identity: identity.clone(),
        operation,
        policy_version: policy_version.to_owned(),
        decided_at: at,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub enum FederatedAccessReason {
    Allowed,
    FederationTrustDenied(FederationTrustReason),
    LocalShareRequired,
    LocalShareDenied(ShareAccessReason),
    DataBindingMismatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct FederatedAccessDecision {
    allowed: bool,
    reason: FederatedAccessReason,
    federation: FederationTrustDecision,
    local_share: Option<ShareAccessDecision>,
}

impl FederatedAccessDecision {
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn reason(&self) -> FederatedAccessReason {
        self.reason
    }

    pub fn federation(&self) -> &FederationTrustDecision {
        &self.federation
    }

    pub fn local_share(&self) -> Option<&ShareAccessDecision> {
        self.local_share.as_ref()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate_federated_memory_access(
    relationship: &FederationRelationship,
    identity: &RemoteIdentity,
    local_workspace_id: WorkspaceId,
    policy_version: &str,
    grant: Option<&MemoryShareGrant>,
    mount: Option<&MemoryMount>,
    target_policy: Option<&TargetSharePolicy>,
    operation: ShareOperation,
    at: Timestamp,
) -> FederatedAccessDecision {
    let federation = evaluate_federation_trust(
        relationship,
        identity,
        local_workspace_id,
        operation,
        policy_version,
        at,
    );
    if !federation.is_allowed() {
        return FederatedAccessDecision {
            allowed: false,
            reason: FederatedAccessReason::FederationTrustDenied(federation.reason()),
            federation,
            local_share: None,
        };
    }

    let (Some(grant), Some(mount), Some(target_policy)) = (grant, mount, target_policy) else {
        return FederatedAccessDecision {
            allowed: false,
            reason: FederatedAccessReason::LocalShareRequired,
            federation,
            local_share: None,
        };
    };
    if grant.revision().source_workspace_id() != identity.remote_workspace_id()
        || grant.revision().target_workspace_id() != local_workspace_id
    {
        return FederatedAccessDecision {
            allowed: false,
            reason: FederatedAccessReason::DataBindingMismatch,
            federation,
            local_share: None,
        };
    }

    let local_share = evaluate_share_access(grant, mount, target_policy, operation, at);
    let allowed = local_share.is_allowed();
    FederatedAccessDecision {
        allowed,
        reason: if allowed {
            FederatedAccessReason::Allowed
        } else {
            FederatedAccessReason::LocalShareDenied(local_share.reason())
        },
        federation,
        local_share: Some(local_share),
    }
}
