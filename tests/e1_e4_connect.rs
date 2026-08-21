use chrono::{TimeZone, Utc};
use serde::Deserialize;
use std::sync::Arc;

use vestrace_application::{
    ApplicationError, AuthorizationBoundary, DenyAllPolicyEngine, ExternalEffectService,
    RequestContext, SharedExternalEffectRepository,
};
use vestrace_domain::{
    AuthorizationRequest, Capability, PrincipalId, RiskCategory, WorkspaceId,
    conformance::gate::{
        ConnectGate, ConnectGateFailure, ConnectMilestone, EvidenceOrigin, GateEvidenceStatus,
        HardGateEvidence,
    },
    conformance::{RequirementFamily, RequirementId},
    external_effects::{
        AdapterContractError, AdapterDispatchResult, DeliverySemantics, DryRunMode,
        EffectAuthorization, EffectFaultPoint, EffectLifecycleStatus, EffectPrecondition,
        EffectReversibility, ExternalEffectAdapter, ExternalEffectAdapterDescriptor,
        ExternalEffectIntent, ExternalEffectReceipt, FaultObservation, IdempotencyProfile,
        ReconciliationOutcome, RetryDecision, evaluate_fault_suite, reconcile_effect,
        validate_adapter_descriptor,
    },
};
use vestrace_infrastructure::postgres::{PgExternalEffectRepository, PgStore};

const CONNECT_FIXTURE: &str = include_str!("fixtures/qualification/e1-e4-connect.json");

#[derive(Debug, Deserialize)]
struct ConnectFixture {
    schema_version: String,
    milestone: String,
    release_gate: String,
    status: String,
    required_requirements: Vec<String>,
    cases: Vec<ConnectFixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectFixtureCase {
    id: String,
    requirements: Vec<String>,
    test: String,
    status: String,
}

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn descriptor() -> ExternalEffectAdapterDescriptor {
    ExternalEffectAdapterDescriptor::new(
        "webhook-v1",
        DeliverySemantics::AtLeastOnce,
        IdempotencyProfile::ProviderKey,
        EffectReversibility::Compensatable,
        DryRunMode::Native,
        true,
        true,
        Capability::ExportRead,
    )
    .unwrap()
}

fn intent() -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "run://01900000-0000-7000-8000-000000000001",
        WorkspaceId::new(),
        PrincipalId::new(),
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        Some("budget:reservation-1"),
        Some("policy:decision-1"),
        at(10),
    )
    .unwrap()
}

fn authorization(effect: &ExternalEffectIntent) -> EffectAuthorization {
    EffectAuthorization::allow(
        "policy:decision-1",
        "policy-v1",
        effect.workspace_id(),
        effect.actor_id(),
        effect.required_capability(),
        effect.operation(),
        effect.target(),
    )
}

/// Supply the service's durable dependency without making this denial test
/// depend on a live database. Authorization must fail before dispatch can ask
/// the lazy pool for a connection.
fn unused_external_effect_repository() -> SharedExternalEffectRepository {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
        .expect("the fixed unused PostgreSQL URL is valid");
    Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(pool)))
}

#[tokio::test]
async fn e2_application_external_effects_use_the_shared_default_deny_boundary() {
    let service = ExternalEffectService::new(
        unused_external_effect_repository(),
        AuthorizationBoundary::new(Arc::new(DenyAllPolicyEngine)),
    );
    let effect = intent();
    let context = RequestContext::new(effect.workspace_id(), effect.actor_id());
    let request = AuthorizationRequest::new(
        Capability::ExportRead,
        effect.operation(),
        effect.target(),
        RiskCategory::Medium,
    );

    let error = service
        .authorize(&context, &effect, request)
        .await
        .unwrap_err();
    assert!(matches!(error, ApplicationError::Policy(message) if message.contains("DefaultDeny")));
}

struct FakeAdapter {
    descriptor: ExternalEffectAdapterDescriptor,
    result: AdapterDispatchResult,
}

impl ExternalEffectAdapter for FakeAdapter {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
        &self.descriptor
    }

    fn dispatch(
        &self,
        _intent: &ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, vestrace_domain::external_effects::AdapterError> {
        Ok(self.result.clone())
    }
}

#[test]
fn e1_intent_is_immutable_and_adapter_semantics_are_explicit() {
    let intent = intent();
    assert_ne!(intent.id(), vestrace_domain::ExternalEffectId::new());
    assert_eq!(intent.adapter(), "webhook-v1");
    assert_eq!(intent.delivery_semantics(), DeliverySemantics::AtLeastOnce);
    assert_eq!(
        intent.idempotency_profile(),
        IdempotencyProfile::ProviderKey
    );
    assert_eq!(intent.reversibility(), EffectReversibility::Compensatable);
    assert_eq!(validate_adapter_descriptor(&descriptor()), Ok(()));

    let invalid = ExternalEffectAdapterDescriptor::new(
        "webhook-v1",
        DeliverySemantics::EffectivelyOnce,
        IdempotencyProfile::None,
        EffectReversibility::Compensatable,
        DryRunMode::Native,
        true,
        true,
        Capability::ExportRead,
    )
    .unwrap();
    assert!(matches!(
        validate_adapter_descriptor(&invalid),
        Err(AdapterContractError::EffectivelyOnceRequiresProviderIdempotency)
    ));
}

#[test]
fn e2_dispatch_requires_authorization_and_rechecks_preconditions_before_receipt() {
    let intent = intent();
    let authorized = intent.authorize(&authorization(&intent)).unwrap();
    let adapter = FakeAdapter {
        descriptor: descriptor(),
        result: AdapterDispatchResult::unknown(
            "transport-timeout",
            vec!["evidence:timeout".into()],
        ),
    };

    let stale = authorized.dispatch(&adapter, "sha256:preconditions-v2", at(20));
    assert!(matches!(
        stale,
        Err(vestrace_domain::external_effects::DispatchError::StaleIntent)
    ));

    let receipt = authorized
        .dispatch(&adapter, "sha256:preconditions-v1", at(20))
        .unwrap();
    assert_eq!(receipt.outcome_status(), EffectLifecycleStatus::Unknown);
    assert!(receipt.requires_reconciliation());
    assert!(!receipt.is_business_confirmation());
    assert_eq!(receipt.retry_decision(), RetryDecision::DeniedUnknown);
}

#[test]
fn e3_reconciliation_uses_strongest_evidence_and_compensation_is_a_new_effect() {
    let intent = intent();
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        "webhook-v1",
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap();
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![
            vestrace_domain::external_effects::ObservedEffectState::new(
                vestrace_domain::external_effects::EvidenceStrength::ResponseDigest,
                Some(false),
                "external:missing",
                vec!["evidence:weak".into()],
            ),
            vestrace_domain::external_effects::ObservedEffectState::new(
                vestrace_domain::external_effects::EvidenceStrength::ProviderIdempotencyLookup,
                Some(true),
                "external:delivered",
                vec!["evidence:provider-lookup".into()],
            ),
        ],
        at(30),
    )
    .unwrap();
    assert_eq!(reconciliation.outcome(), ReconciliationOutcome::Confirmed);
    assert_eq!(
        reconciliation.evidence_strength(),
        vestrace_domain::external_effects::EvidenceStrength::ProviderIdempotencyLookup
    );

    let compensation = intent
        .compensation_for(
            "withdraw-notification",
            "sha256:compensation-args",
            "neutralize notification",
            at(40),
        )
        .unwrap();
    assert_ne!(compensation.id(), intent.id());
    assert_eq!(compensation.compensates_effect_id(), Some(intent.id()));
    assert_eq!(intent.compensates_effect_id(), None);
}

#[test]
fn e4_fault_suite_and_connect_gate_reject_unsafe_or_incomplete_evidence() {
    let expected = EffectFaultPoint::required_points()
        .into_iter()
        .map(FaultObservation::expected)
        .collect::<Vec<_>>();
    assert!(evaluate_fault_suite(&expected).is_passed());

    let mut unsafe_retry = FaultObservation::expected(EffectFaultPoint::AfterDispatchBeforeReceipt);
    unsafe_retry.retry_attempted = true;
    let decision = evaluate_fault_suite(&[unsafe_retry]);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .iter()
            .any(|failure| failure.contains("retry"))
    );

    let evidence = ConnectGate::required_requirements()
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::pass(
                requirement_id,
                format!("evidence:e4:{requirement_id}"),
                None,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect::<Vec<_>>();
    let gate = ConnectGate::evaluate(&evidence);
    assert!(gate.is_passed());
    assert_eq!(gate.milestone(), ConnectMilestone::Connect);

    let missing_id = RequirementId::new(RequirementFamily::Ext, 7);
    let mut missing = evidence;
    missing.retain(|item| item.requirement_id() != missing_id);
    let denied = ConnectGate::evaluate(&missing);
    assert!(!denied.is_passed());
    assert!(
        denied
            .failures()
            .contains(&ConnectGateFailure::MissingEvidence {
                requirement_id: missing_id,
            })
    );
    assert_eq!(
        ConnectGate::required_requirements().len(),
        19,
        "EXT-001..018 plus QUAL-008"
    );
    assert_eq!(GateEvidenceStatus::Pass, GateEvidenceStatus::Pass);
}

#[test]
fn e4_fixture_is_traceable_without_claiming_connect_qualification() {
    let fixture: ConnectFixture = serde_json::from_str(CONNECT_FIXTURE).unwrap();
    assert_eq!(fixture.schema_version, "vestrace.e1-e4.connect-fixture.v1");
    assert_eq!(fixture.milestone, "Connect");
    assert_eq!(fixture.release_gate, "v0.6 Connect");
    assert_eq!(fixture.status, "evidence_fixture_only");
    assert_eq!(fixture.required_requirements.len(), 19);
    assert!(!fixture.known_limitations.is_empty());
    for case in fixture.cases {
        assert!(!case.id.is_empty());
        assert!(!case.requirements.is_empty());
        assert!(case.test.starts_with("e"));
        assert_eq!(case.status, "covered");
    }
}
