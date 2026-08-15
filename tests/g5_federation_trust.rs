use std::collections::BTreeSet;

use chrono::{Duration, TimeZone, Utc};
use vestrace_domain::{
    enterprise::{
        FederatedAccessReason, FederationRelationship, FederationRelationshipSpec,
        FederationRelationshipStatus, FederationTrustPolicy, FederationTrustReason, MemoryMount,
        MemoryMountAcceptance, MemoryShareGrant, MemoryShareGrantRevision,
        MemoryShareGrantRevisionSpec, RemoteIdentity, RemoteQualificationEvidence, ShareOperation,
        ShareTarget, TargetSharePolicy, evaluate_federated_memory_access,
        evaluate_federation_trust,
    },
    id::{MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId},
};

fn operations(values: impl IntoIterator<Item = ShareOperation>) -> BTreeSet<ShareOperation> {
    values.into_iter().collect()
}

fn fixture() -> (
    FederationRelationship,
    RemoteIdentity,
    WorkspaceId,
    WorkspaceId,
    PrincipalId,
    chrono::DateTime<Utc>,
) {
    let at = fixed_at();
    let local_workspace_id = WorkspaceId::new();
    let remote_workspace_id = WorkspaceId::new();
    let remote_principal_id = PrincipalId::new();
    let identity = RemoteIdentity::new(
        remote_workspace_id,
        remote_principal_id,
        "issuer:g5",
        "subject:remote-agent",
    )
    .unwrap();
    let policy = FederationTrustPolicy::new(
        local_workspace_id,
        "issuer:g5",
        BTreeSet::from(["FEDERATION".to_string()]),
        operations([ShareOperation::ReadContent, ShareOperation::IncludeContext]),
        "federation-policy-v1",
    )
    .unwrap();
    let evidence = RemoteQualificationEvidence::new(
        "issuer:g5",
        "FEDERATION",
        "evidence:g5:remote-qualification",
        "remote-source-rev:g5",
    )
    .unwrap();
    let relationship = FederationRelationship::establish(
        FederationRelationshipSpec {
            local_workspace_id,
            remote_workspace_id,
            remote_identity: identity.clone(),
            policy,
            evidence,
            valid_from: at,
            valid_until: Some(at + Duration::seconds(60)),
        },
        at,
    )
    .unwrap();

    (
        relationship,
        identity,
        local_workspace_id,
        remote_workspace_id,
        remote_principal_id,
        at,
    )
}

fn local_share_fixture(
    local_workspace_id: WorkspaceId,
    remote_workspace_id: WorkspaceId,
    remote_principal_id: PrincipalId,
    at: chrono::DateTime<Utc>,
) -> (MemoryShareGrant, MemoryMount, TargetSharePolicy) {
    let revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id: remote_workspace_id,
            target: ShareTarget::ExactWorkspace(local_workspace_id),
            memory_id: MemoryId::new(),
            memory_revision_id: MemoryRevisionId::new(),
            source_generation: "remote-memory-generation-1".into(),
            operations: operations([ShareOperation::ReadContent]),
            valid_from: at,
            valid_until: None,
        },
        at,
    )
    .unwrap();
    let grant = MemoryShareGrant::issue(revision, at).unwrap();
    let target_policy = TargetSharePolicy::new(
        local_workspace_id,
        remote_principal_id,
        operations([ShareOperation::ReadContent]),
        None,
    )
    .unwrap();
    let mount = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id: local_workspace_id,
            target_principal_id: remote_principal_id,
            operations: operations([ShareOperation::ReadContent]),
            valid_until: None,
        },
        &grant,
        &target_policy,
        at,
    )
    .unwrap();
    (grant, mount, target_policy)
}

#[test]
fn g5_active_relationship_validates_exact_remote_identity_and_policy_version() {
    let (relationship, identity, local_workspace_id, _, _, at) = fixture();
    let decision = evaluate_federation_trust(
        &relationship,
        &identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v1",
        at,
    );
    assert!(decision.is_allowed());

    let wrong_identity = RemoteIdentity::new(
        identity.remote_workspace_id(),
        PrincipalId::new(),
        "issuer:g5",
        "subject:remote-agent",
    )
    .unwrap();
    let denied = evaluate_federation_trust(
        &relationship,
        &wrong_identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v1",
        at,
    );
    assert_eq!(
        denied.reason(),
        FederationTrustReason::RemoteIdentityMismatch
    );

    let stale_policy = evaluate_federation_trust(
        &relationship,
        &identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v2",
        at,
    );
    assert_eq!(
        stale_policy.reason(),
        FederationTrustReason::PolicyVersionMismatch
    );
}

#[test]
fn g5_suspended_revoked_and_expired_relationships_fail_closed() {
    let (mut relationship, identity, local_workspace_id, _, _, at) = fixture();
    relationship.suspend(at + Duration::seconds(1)).unwrap();
    let suspended = evaluate_federation_trust(
        &relationship,
        &identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v1",
        at + Duration::seconds(1),
    );
    assert_eq!(
        suspended.reason(),
        FederationTrustReason::RelationshipSuspended
    );

    relationship.resume(at + Duration::seconds(2)).unwrap();
    relationship.revoke(at + Duration::seconds(3)).unwrap();
    let revoked = evaluate_federation_trust(
        &relationship,
        &identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v1",
        at + Duration::seconds(3),
    );
    assert_eq!(revoked.reason(), FederationTrustReason::RelationshipRevoked);

    let (expired_relationship, identity, local_workspace_id, _, _, at) = fixture();
    let expired = evaluate_federation_trust(
        &expired_relationship,
        &identity,
        local_workspace_id,
        ShareOperation::ReadContent,
        "federation-policy-v1",
        at + Duration::seconds(60),
    );
    assert_eq!(expired.reason(), FederationTrustReason::RelationshipExpired);
}

#[test]
fn g5_federation_trust_alone_never_authorizes_local_memory_access() {
    let (relationship, identity, local_workspace_id, _, _, at) = fixture();
    let decision = evaluate_federated_memory_access(
        &relationship,
        &identity,
        local_workspace_id,
        "federation-policy-v1",
        None,
        None,
        None,
        ShareOperation::ReadContent,
        at,
    );
    assert!(!decision.is_allowed());
    assert_eq!(decision.reason(), FederatedAccessReason::LocalShareRequired);
}

#[test]
fn g5_federated_memory_access_requires_both_trust_and_g4_local_share() {
    let (relationship, identity, local_workspace_id, remote_workspace_id, remote_principal_id, at) =
        fixture();
    let (grant, mount, target_policy) = local_share_fixture(
        local_workspace_id,
        remote_workspace_id,
        remote_principal_id,
        at,
    );

    let allowed = evaluate_federated_memory_access(
        &relationship,
        &identity,
        local_workspace_id,
        "federation-policy-v1",
        Some(&grant),
        Some(&mount),
        Some(&target_policy),
        ShareOperation::ReadContent,
        at,
    );
    assert!(allowed.is_allowed());

    let mut revoked_grant = grant;
    revoked_grant.revoke(at + Duration::seconds(1)).unwrap();
    let denied = evaluate_federated_memory_access(
        &relationship,
        &identity,
        local_workspace_id,
        "federation-policy-v1",
        Some(&revoked_grant),
        Some(&mount),
        Some(&target_policy),
        ShareOperation::ReadContent,
        at + Duration::seconds(1),
    );
    assert!(!denied.is_allowed());
    assert!(matches!(
        denied.reason(),
        FederatedAccessReason::LocalShareDenied(_)
    ));
}

#[test]
fn g5_remote_identity_must_bind_to_the_shared_source_workspace() {
    let (relationship, identity, local_workspace_id, remote_workspace_id, remote_principal_id, at) =
        fixture();
    let (grant, mount, target_policy) = local_share_fixture(
        local_workspace_id,
        WorkspaceId::new(),
        remote_principal_id,
        at,
    );
    let denied = evaluate_federated_memory_access(
        &relationship,
        &identity,
        local_workspace_id,
        "federation-policy-v1",
        Some(&grant),
        Some(&mount),
        Some(&target_policy),
        ShareOperation::ReadContent,
        at,
    );
    assert!(!denied.is_allowed());
    assert_eq!(denied.reason(), FederatedAccessReason::DataBindingMismatch);
    assert_ne!(grant.revision().source_workspace_id(), remote_workspace_id);
}

#[test]
fn g5_relationship_lifecycle_is_explicit_and_not_transitive() {
    let (relationship, _, _, _, _, _) = fixture();
    assert_eq!(relationship.status(), FederationRelationshipStatus::Active);
    assert!(
        relationship
            .remote_qualification_evidence()
            .evidence_ref()
            .contains("g5")
    );
}

fn fixed_at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 12, 0, 0, 0).single().unwrap()
}
