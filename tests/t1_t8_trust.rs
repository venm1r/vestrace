use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;
use vestrace_domain::conformance::{
    QualificationProfile,
    gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence},
};
use vestrace_domain::*;

fn workspace_scope(workspace_id: WorkspaceId) -> HealthScope {
    HealthScope::workspace(workspace_id)
}

#[test]
fn t1_incident_requires_containment_before_recovery_and_trust_is_explicit() {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let at = now();
    let mut incident = Incident::open(
        IncidentType::TrustBoundaryViolation,
        IncidentSeverity::Critical,
        workspace_scope(workspace_id),
        vec![HealthFindingId::new()],
        vec!["workspace:trust".into()],
        vec!["finding:trust-boundary".into()],
        at,
    )
    .unwrap();

    assert_eq!(incident.status(), IncidentStatus::Open);
    assert!(incident.begin_recovery(at).is_err());

    let action = ContainmentAction::new(
        ContainmentActionKind::SuspendCapabilities,
        "workspace:trust",
        principal_id,
        "isolate affected scope",
        "policy-v1",
        at,
    )
    .unwrap();
    incident.begin_containment(action).unwrap();
    incident.mark_contained(at).unwrap();
    incident.begin_recovery(at).unwrap();
    assert_eq!(incident.status(), IncidentStatus::Recovering);

    let revalidation = RevalidationRun::complete(
        Some(incident.id()),
        workspace_scope(workspace_id),
        RevalidationLevel::Workspace,
        "state:workspace:recovered",
        vec![RevalidationCheck::passing(
            "trust-boundary",
            vec!["evidence:revalidated".into()],
        )],
        vec!["evidence:revalidation".into()],
        RevalidationResult::Passed,
        at,
    )
    .unwrap();
    incident.begin_revalidation(revalidation.id(), at).unwrap();
    incident.apply_revalidation(&revalidation).unwrap();
    incident.mark_resolved(at).unwrap();
    incident
        .close(
            principal_id,
            "scope restored after evidence-backed validation",
            vec!["evidence:closure".into()],
            at,
        )
        .unwrap();
    assert_eq!(incident.status(), IncidentStatus::Closed);
    assert_eq!(incident.closure_evidence(), &["evidence:closure"]);

    let trust = TrustStateRecord::from_incident(
        workspace_scope(workspace_id),
        incident.id(),
        IncidentSeverity::Critical,
        at,
    );
    assert_eq!(trust.state(), TrustState::Untrusted);
}

#[test]
fn t2_recovery_classification_and_revalidation_gate_trust_restoration() {
    assert_eq!(
        classify_recovery(RecoveryTarget::RunningExecution),
        RecoveryClassification::SafeToResume
    );
    assert_eq!(
        classify_recovery(RecoveryTarget::DispatchingExternalEffect),
        RecoveryClassification::MustReconcile
    );
    assert_eq!(
        classify_recovery(RecoveryTarget::DivergentHistory),
        RecoveryClassification::HumanRequired
    );

    let workspace_id = WorkspaceId::new();
    let incident_id = IncidentId::new();
    let at = now();
    let run = RevalidationRun::complete(
        Some(incident_id),
        workspace_scope(workspace_id),
        RevalidationLevel::Workspace,
        "state:workspace:42",
        vec![RevalidationCheck::passing(
            "canonical-history",
            vec!["evidence:history".into()],
        )],
        vec!["evidence:revalidation".into()],
        RevalidationResult::Inconclusive,
        at,
    )
    .unwrap();
    let mut trust = TrustStateRecord::new(
        workspace_scope(workspace_id),
        TrustState::Revalidating,
        "post-incident recovery",
        Some(incident_id),
        at,
    )
    .unwrap();
    trust.begin_revalidation(run.id(), at).unwrap();
    trust.apply_revalidation(&run).unwrap();
    assert_ne!(trust.state(), TrustState::Trusted);
}

#[test]
fn t3_secret_refs_are_opaque_and_key_lifecycle_is_explicit() {
    let workspace_id = WorkspaceId::new();
    let secret = SecretRef::new(
        "secret://vault/workspace/provider",
        "vault",
        workspace_id,
        "model-provider",
        BTreeMap::from([("rotation".into(), "2026-08".into())]),
        Some("v2".into()),
    )
    .unwrap();
    let serialized = serde_json::to_string(&secret).unwrap();
    assert!(!serialized.contains("plaintext"));
    let lease = secret
        .authorize_resolution(&SecretResolutionRequest::new(
            workspace_id,
            "model-provider",
            "policy-decision:1",
        ))
        .unwrap();
    assert_eq!(lease.secret_ref_id(), secret.id());

    let mut key = KeyReference::new(
        "kms",
        "workspace-key",
        "v1",
        KeyPurpose::Storage,
        "workspace",
        "aes-gcm-256:v1",
    )
    .unwrap();
    assert!(key.is_usable());
    key.begin_rotation().unwrap();
    key.revoke().unwrap();
    assert!(!key.is_usable());
    assert!(key.destroy().is_ok());
}

#[test]
fn t4_classification_is_conservative_and_model_boundary_is_governed() {
    let source = DataClassification::new(
        Sensitivity::Restricted,
        vec!["credential_adjacent".into()],
        vec!["EU".into()],
        vec!["no_remote_provider".into()],
        "source:memory",
        "provenance:memory-revision",
    )
    .unwrap();
    let lineage = ClassificationLineage::derive(
        vec!["memory-revision:1".into()],
        vec![source.clone()],
        "policy-v1",
        "derivation:context-pack",
    )
    .unwrap();
    assert_eq!(
        lineage.effective_classification().sensitivity(),
        Sensitivity::Restricted
    );

    let policy = DataPolicy::new(
        DataPolicyId::new(),
        "policy-v1",
        Sensitivity::Restricted,
        BTreeSet::from([DataDestination::LocalModel]),
        Some(Capability::ContextRetrieve),
    )
    .unwrap();
    let denied = evaluate_model_boundary(
        &policy,
        lineage.effective_classification(),
        DataDestination::RemoteProvider,
        true,
    );
    assert!(!denied.is_allowed());

    let decision = DeclassificationDecision::new(
        "context-pack:1",
        Sensitivity::Restricted,
        Sensitivity::Confidential,
        "redaction:v1",
        vec!["evidence:redaction".into()],
        "policy-v1",
        PrincipalId::new(),
        PrincipalId::new(),
        now(),
    )
    .unwrap();
    assert_eq!(
        decision
            .apply(lineage.effective_classification())
            .unwrap()
            .sensitivity(),
        Sensitivity::Confidential
    );
}

#[test]
fn t5_retention_hold_and_deletion_verification_preserve_uncertainty() {
    let at = now();
    let policy = RetentionPolicy::new(
        Some(Duration::hours(1)),
        Some(Duration::hours(2)),
        RetentionTrigger::CreatedAt,
        DisposalMethod::PhysicalDelete,
        "policy-v1",
    )
    .unwrap();
    assert_eq!(
        policy.state_at(at - Duration::hours(3), at),
        RetentionState::Expired
    );
    let hold = DataHold::new(
        "workspace:1",
        "legal investigation",
        PrincipalId::new(),
        "policy-v1",
        at,
        None,
    )
    .unwrap();
    assert!(hold.is_active(at));

    let request = DeletionRequest::new(
        PrincipalId::new(),
        PrincipalId::new(),
        "memory:1",
        DeletionSemantics::PhysicalDelete,
        "plan:delete-1",
        "execution:delete-1",
        at,
    )
    .unwrap();
    let plan = DeletionPlan::new(
        request.id(),
        vec!["memory:1".into()],
        vec!["context-index:1".into(), "export:1".into()],
        vec![hold.id()],
        at,
    )
    .unwrap();
    let verification = verify_deletion(
        &request,
        &plan,
        &[hold],
        vec![
            "memory:1".into(),
            "context-index:1".into(),
            "export:1".into(),
        ],
        vec!["backup:1".into()],
        vec!["evidence:delete".into()],
        at,
    )
    .unwrap();
    assert!(!verification.is_complete());
    assert_eq!(
        verification.outcome(),
        DeletionVerificationOutcome::BlockedByHold
    );
}

#[test]
fn t6_export_is_governed_and_audit_chain_is_tamper_evident() {
    let workspace_id = WorkspaceId::new();
    let policy = DataPolicy::new(
        DataPolicyId::new(),
        "policy-v1",
        Sensitivity::Confidential,
        BTreeSet::from([DataDestination::ExportBundle]),
        Some(Capability::ExportRead),
    )
    .unwrap();
    let classification =
        DataClassification::source(Sensitivity::Confidential, "memory:1", "provenance:memory")
            .unwrap();
    let plan = DataExportPlan::new(
        workspace_id,
        "workspace:1",
        "support review",
        "recipient:auditor",
        classification,
        vec!["memory-revision:1".into()],
        vec!["redaction:secrets".into()],
        "vestrace-export:v1",
        None,
        Some("signing-key:v1".into()),
        Some(now() + Duration::hours(1)),
        "policy-v1",
        Capability::ExportRead,
        now(),
    )
    .unwrap();
    let authorized = plan.authorize(&policy, true, now()).unwrap();
    let bundle = ExportBundle::from_authorized(
        &authorized,
        vec!["provenance:memory".into()],
        BTreeMap::from([("memory-revision:1".into(), "sha256:abc".into())]),
        Some("sig:bundle".into()),
        Some("envelope:aes-gcm".into()),
        now(),
    )
    .unwrap();
    assert!(!bundle.authority_transferred());
    assert!(bundle.manifest_digest().starts_with("sha256:"));

    let mut chain = AuditIntegrityChain::new();
    chain.append(
        workspace_id,
        PrincipalId::new(),
        "export.authorize",
        "export:1",
        "digest:1",
        now(),
    );
    chain.append(
        workspace_id,
        PrincipalId::new(),
        "export.create",
        "export:1",
        "digest:2",
        now(),
    );
    assert!(chain.verify().is_ok());
}

#[test]
fn t7_qualification_bundle_is_target_bound_and_baseline_drift_is_explicit() {
    let evidence = vec![HardGateEvidence::pass(
        conformance::RequirementId::new(conformance::RequirementFamily::Qual, 1),
        "evidence:qual-1",
        None,
        EvidenceOrigin::LocalExecutable,
    )];
    let bundle = QualificationBundle::new(
        QualificationProfile::Trusted,
        "manifest:v1",
        "source:revision",
        "build:digest",
        "config:digest",
        "environment:manifest",
        "suite:v1",
        evidence,
        vec!["provider deployment remains external".into()],
        now(),
        Some(now()),
    )
    .unwrap();
    assert!(!bundle.target_digest().is_empty());
    let mut baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    assert_eq!(baseline.state(), QualificationBaselineState::Qualified);
    baseline.invalidate("schema drift", now()).unwrap();
    assert_eq!(baseline.state(), QualificationBaselineState::Invalidated);
}

#[test]
fn t8_trusted_gate_requires_full_local_evidence_and_qualified_baseline() {
    let evidence = TrustedQualificationGate::required_requirements()
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::pass(
                requirement_id,
                format!("evidence:{requirement_id}"),
                Some("policy-v1".into()),
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect::<Vec<_>>();
    let mut remote_evidence = evidence.clone();
    let remote_requirement =
        conformance::RequirementId::new(conformance::RequirementFamily::Rec, 1);
    let remote_index = remote_evidence
        .iter()
        .position(|item| item.requirement_id() == remote_requirement)
        .unwrap();
    remote_evidence[remote_index] = HardGateEvidence::new(
        remote_requirement,
        GateEvidenceStatus::Pass,
        Some("remote:self".into()),
        Some("policy-v1".into()),
        EvidenceOrigin::RemoteSelfAsserted,
    );
    let bundle = QualificationBundle::new(
        QualificationProfile::Trusted,
        "manifest:v1",
        "source:revision",
        "build:digest",
        "config:digest",
        "environment:manifest",
        "suite:v1",
        evidence,
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap();
    let baseline = QualificationBaseline::from_bundle(&bundle, now()).unwrap();
    let decision = TrustedQualificationGate::evaluate(&bundle, &baseline);
    assert!(decision.is_passed(), "failures: {:?}", decision.failures());

    let remote_bundle = QualificationBundle::new(
        QualificationProfile::Trusted,
        "manifest:v1",
        "source:revision",
        "build:digest",
        "config:digest",
        "environment:manifest",
        "suite:v1",
        remote_evidence,
        Vec::new(),
        now(),
        Some(now()),
    )
    .unwrap();
    let remote_baseline = QualificationBaseline::from_bundle(&remote_bundle, now()).unwrap();
    let remote_decision = TrustedQualificationGate::evaluate(&remote_bundle, &remote_baseline);
    assert!(remote_decision.failures().iter().any(|failure| matches!(
        failure,
        TrustedGateFailure::HardGate(
            conformance::gate::HardGateFailure::RemoteSelfAssertion { .. }
        )
    )));
}
