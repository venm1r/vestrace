use std::collections::BTreeSet;

use chrono::{Duration, TimeZone, Utc};
use vestrace_domain::{
    enterprise::{
        CrossWorkspaceMemoryGrant, MemoryMount, MemoryMountAcceptance, MemoryShareGrant,
        MemoryShareGrantRevision, MemoryShareGrantRevisionSpec, ShareAccessReason, ShareOperation,
        ShareTarget, TargetSharePolicy, evaluate_share_access,
    },
    id::{MemoryGrantId, MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId},
};

fn operations(values: impl IntoIterator<Item = ShareOperation>) -> BTreeSet<ShareOperation> {
    values.into_iter().collect()
}

fn fixture(
    valid_until: Option<chrono::DateTime<Utc>>,
) -> (
    MemoryShareGrant,
    TargetSharePolicy,
    WorkspaceId,
    PrincipalId,
    chrono::DateTime<Utc>,
) {
    let at = fixed_at();
    let source_workspace_id = WorkspaceId::new();
    let target_workspace_id = WorkspaceId::new();
    let target_principal_id = PrincipalId::new();
    let revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id,
            target: ShareTarget::ExactWorkspace(target_workspace_id),
            memory_id: MemoryId::new(),
            memory_revision_id: MemoryRevisionId::new(),
            source_generation: "memory-generation-7".into(),
            operations: operations([ShareOperation::ReadContent, ShareOperation::IncludeContext]),
            valid_from: at,
            valid_until,
        },
        at,
    )
    .unwrap();
    let grant = MemoryShareGrant::issue(revision, at).unwrap();
    let target_policy = TargetSharePolicy::new(
        target_workspace_id,
        target_principal_id,
        operations([ShareOperation::ReadContent, ShareOperation::IncludeContext]),
        valid_until,
    )
    .unwrap();

    (
        grant,
        target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    )
}

fn accept_mount(
    grant: &MemoryShareGrant,
    target_policy: &TargetSharePolicy,
    target_workspace_id: WorkspaceId,
    target_principal_id: PrincipalId,
    at: chrono::DateTime<Utc>,
) -> MemoryMount {
    MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id,
            target_principal_id,
            operations: operations([ShareOperation::ReadContent]),
            valid_until: grant.revision().valid_until(),
        },
        grant,
        target_policy,
        at,
    )
    .unwrap()
}

#[test]
fn g4_two_sided_acceptance_returns_an_exact_namespaced_shared_ref() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );

    let decision = evaluate_share_access(
        &grant,
        &mount,
        &target_policy,
        ShareOperation::ReadContent,
        at,
    );
    assert!(decision.is_allowed());

    let shared_ref = mount.shared_ref(&grant, at).unwrap();
    assert_eq!(
        shared_ref.source_workspace_id(),
        grant.revision().source_workspace_id()
    );
    assert_eq!(shared_ref.source_memory_id(), grant.revision().memory_id());
    assert_eq!(
        shared_ref.memory_revision_id(),
        grant.revision().memory_revision_id()
    );
    assert_eq!(shared_ref.grant_revision_id(), grant.revision().id());
}

#[test]
fn g4_wildcard_target_and_broader_target_operations_are_rejected() {
    let at = fixed_at();
    let wildcard = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id: WorkspaceId::new(),
            target: ShareTarget::AnyWorkspace,
            memory_id: MemoryId::new(),
            memory_revision_id: MemoryRevisionId::new(),
            source_generation: "generation".into(),
            operations: operations([ShareOperation::ReadContent]),
            valid_from: at,
            valid_until: None,
        },
        at,
    );
    assert!(wildcard.is_err());

    let (grant, _, target_workspace_id, target_principal_id, at) = fixture(None);
    let target_policy = TargetSharePolicy::new(
        target_workspace_id,
        target_principal_id,
        operations([ShareOperation::ReadContent]),
        None,
    )
    .unwrap();
    let broader = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id,
            target_principal_id,
            operations: operations([ShareOperation::ReadContent, ShareOperation::IncludeContext]),
            valid_until: None,
        },
        &grant,
        &target_policy,
        at,
    );
    assert!(broader.is_err());
}

#[test]
fn g4_wrong_grant_revision_or_target_workspace_cannot_create_a_mount() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let wrong_revision = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: vestrace_domain::id::MemoryShareGrantRevisionId::new(),
            target_workspace_id,
            target_principal_id,
            operations: operations([ShareOperation::ReadContent]),
            valid_until: None,
        },
        &grant,
        &target_policy,
        at,
    );
    assert!(wrong_revision.is_err());

    let wrong_workspace = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id: WorkspaceId::new(),
            target_principal_id,
            operations: operations([ShareOperation::ReadContent]),
            valid_until: None,
        },
        &grant,
        &target_policy,
        at,
    );
    assert!(wrong_workspace.is_err());
}

#[test]
fn g4_authoritative_ids_are_generated_for_each_new_binding() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id: grant.revision().source_workspace_id(),
            target: ShareTarget::ExactWorkspace(grant.revision().target_workspace_id()),
            memory_id: grant.revision().memory_id(),
            memory_revision_id: grant.revision().memory_revision_id(),
            source_generation: grant.revision().source_generation().into(),
            operations: grant.revision().operations().clone(),
            valid_from: grant.revision().valid_from(),
            valid_until: grant.revision().valid_until(),
        },
        at,
    )
    .unwrap();
    let second_grant = MemoryShareGrant::issue(revision, at).unwrap();

    assert_ne!(grant.revision().id(), second_grant.revision().id());
    assert_ne!(grant.id(), second_grant.id());

    let first_mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );
    let second_mount = accept_mount(
        &second_grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );
    assert_ne!(first_mount.id(), second_mount.id());
}

#[test]
fn g4_mismatched_source_identity_marks_mount_stale_and_denies_access() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let mut mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );
    let (other_grant, other_policy, _, _, _) = fixture(None);

    mount.sync_with_source(&other_grant, at_plus(1));

    assert_eq!(
        mount.status(),
        vestrace_domain::enterprise::MemoryMountStatus::Stale
    );
    assert!(mount.shared_ref(&grant, at_plus(1)).is_err());
    assert!(mount.shared_ref(&other_grant, at_plus(1)).is_err());
    let stale_decision = evaluate_share_access(
        &grant,
        &mount,
        &target_policy,
        ShareOperation::ReadContent,
        at_plus(1),
    );
    assert_eq!(stale_decision.reason(), ShareAccessReason::MountInactive);
    let decision = evaluate_share_access(
        &other_grant,
        &mount,
        &other_policy,
        ShareOperation::ReadContent,
        at_plus(1),
    );
    assert_eq!(decision.reason(), ShareAccessReason::GrantRevisionMismatch);
}

#[test]
fn g4_source_and_target_policy_are_checked_independently() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );

    let source_denial = evaluate_share_access(
        &grant,
        &mount,
        &target_policy,
        ShareOperation::SendToProvider,
        at,
    );
    assert_eq!(
        source_denial.reason(),
        ShareAccessReason::SourceOperationDenied
    );

    let target_denial_policy = TargetSharePolicy::new(
        target_workspace_id,
        target_principal_id,
        operations([ShareOperation::IncludeContext]),
        None,
    )
    .unwrap();
    let target_denial = evaluate_share_access(
        &grant,
        &mount,
        &target_denial_policy,
        ShareOperation::ReadContent,
        at,
    );
    assert_eq!(
        target_denial.reason(),
        ShareAccessReason::TargetPolicyDenied
    );
}

#[test]
fn g4_revoke_and_expiry_make_mount_stale_and_deny_future_refs() {
    let (mut grant, target_policy, target_workspace_id, target_principal_id, at) =
        fixture(Some(at_plus(60)));
    let mut mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );

    grant.revoke(at_plus(1)).unwrap();
    mount.sync_with_source(&grant, at_plus(1));
    assert_eq!(
        mount.status(),
        vestrace_domain::enterprise::MemoryMountStatus::Revoked
    );
    assert!(mount.shared_ref(&grant, at_plus(1)).is_err());

    let (grant, target_policy, target_workspace_id, target_principal_id, at) =
        fixture(Some(at_plus(1)));
    let mut mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );
    mount.sync_with_source(&grant, at_plus(1));
    assert_eq!(
        mount.status(),
        vestrace_domain::enterprise::MemoryMountStatus::Expired
    );
    assert!(mount.shared_ref(&grant, at_plus(1)).is_err());
}

#[test]
fn g4_mount_expiry_denies_access_before_explicit_sync() {
    let (grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let mount = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id,
            target_principal_id,
            operations: operations([ShareOperation::ReadContent]),
            valid_until: Some(at_plus(1)),
        },
        &grant,
        &target_policy,
        at,
    )
    .unwrap();

    let decision = evaluate_share_access(
        &grant,
        &mount,
        &target_policy,
        ShareOperation::ReadContent,
        at_plus(1),
    );
    assert_eq!(decision.reason(), ShareAccessReason::MountInactive);
}

#[test]
fn g4_historical_disclosure_survives_source_revoke() {
    let (mut grant, target_policy, target_workspace_id, target_principal_id, at) = fixture(None);
    let mut mount = accept_mount(
        &grant,
        &target_policy,
        target_workspace_id,
        target_principal_id,
        at,
    );
    let original_ref = mount.shared_ref(&grant, at).unwrap();
    mount
        .record_disclosure(
            &grant,
            &target_policy,
            ShareOperation::ReadContent,
            at_plus(1),
        )
        .unwrap();

    grant.revoke(at_plus(2)).unwrap();
    assert!(
        mount
            .record_disclosure(
                &grant,
                &target_policy,
                ShareOperation::ReadContent,
                at_plus(2)
            )
            .is_err()
    );
    mount.sync_with_source(&grant, at_plus(2));

    assert_eq!(mount.disclosures().len(), 1);
    assert_eq!(mount.disclosures()[0].shared_ref(), &original_ref);
    assert!(mount.shared_ref(&grant, at_plus(2)).is_err());
}

#[test]
fn g4_shared_mount_cannot_be_used_as_a_transitive_source() {
    let at = fixed_at();
    let revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id: WorkspaceId::new(),
            target: ShareTarget::ExactWorkspace(WorkspaceId::new()),
            memory_id: MemoryId::new(),
            memory_revision_id: MemoryRevisionId::new(),
            source_generation: "generation".into(),
            operations: operations([ShareOperation::ReShare]),
            valid_from: at,
            valid_until: None,
        },
        at,
    );
    assert!(revision.is_err());
}

#[test]
fn g4_legacy_one_sided_grant_does_not_auto_upgrade_to_a_mount() {
    let legacy = CrossWorkspaceMemoryGrant {
        id: MemoryGrantId::new(),
        owner_workspace_id: WorkspaceId::new(),
        target_workspace_id: WorkspaceId::new(),
        memory_id: MemoryId::new(),
        granted_at: fixed_at(),
    };

    let result = MemoryShareGrant::from_legacy(&legacy);
    assert!(result.is_err());
}

fn at_plus(seconds: i64) -> chrono::DateTime<Utc> {
    fixed_at() + Duration::seconds(seconds)
}

fn fixed_at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 12, 0, 0, 0).single().unwrap()
}
