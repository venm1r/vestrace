//! What `observe` reads back out of a database, against states that were put
//! there rather than described.
//!
//! Every case here seeds real rows through the real repository and then asks
//! `observe` what it can see. Nothing asserts that the answer matches the
//! conformance suite's `expected` set — that comparison belongs to the suite,
//! and making these tests agree with it would be the same as writing the
//! expected answer into the observation.
//!
//! # Running these
//!
//! ```text
//! DATABASE_URL=postgres://... cargo test -p vestrace-fault-scenario --test observation
//! ```
//!
//! Without `DATABASE_URL` every database-backed case returns early and prints
//! why — but libtest captures stdout for passing tests, so the notices are only
//! visible with `-- --nocapture`, and a fully skipped run otherwise reports the
//! same `passed` summary line as a fully exercised one. A caller that must not
//! be allowed to skip sets `VESTRACE_REQUIRE_DATABASE=1`, which turns the
//! absence into a failure; see `a_run_that_requires_a_database_cannot_skip_instead`.
//!
//! # Why this does not use `#[sqlx::test]`
//!
//! Every other database-backed test in this workspace does, and gets a fresh
//! provisioned database per test. It also *fails* when `DATABASE_URL` is
//! absent, and this file has to skip instead. So it connects the way the child
//! under test connects — `PgStore::connect` then `migrate` — and isolates
//! itself with a fresh `WorkspaceId` per test instead of a fresh database,
//! which the row level security on all three external-effect tables makes as
//! strong a boundary here.

use secrecy::SecretString;
use vestrace_application::{ExternalEffectRepository, RequestContext};
use vestrace_domain::external_effects::{
    EffectAuthorization, EffectFaultPoint, EffectLifecycleStatus, EffectPrecondition,
    EvidenceStrength, ExternalEffectAdapter, ExternalEffectIntent, ExternalEffectReceipt,
    ObservedEffectState, ReconciliationOutcome, reconcile_effect,
};
use vestrace_domain::id::AgentRunId;
use vestrace_domain::{ExternalEffectId, PrincipalId, RiskCategory, WorkspaceId, now};
use vestrace_fault_scenario::{AdapterStub, observe};
use vestrace_infrastructure::{
    DatabaseConfig, HttpWebhookEffectAdapter, PgExternalEffectRepository, PgStore,
};

/// The point every case is filed under. `observe` carries it through rather
/// than deriving it — it is the question that was asked, not part of the
/// answer — so which one it is has no bearing on any assertion below.
const ANY_POINT: EffectFaultPoint = EffectFaultPoint::AfterIntentPersistence;

const ADAPTER_NAME: &str = "observation-fixture-webhook";

/// The opt-in that turns a skip into a failure.
///
/// Named the way the scenario's own three variables are — one underscore,
/// because this selects a behaviour of the harness rather than setting a value
/// in the `VESTRACE_SECTION__FIELD` configuration tree.
const REQUIRE_DATABASE: &str = "VESTRACE_REQUIRE_DATABASE";

/// Everything a case needs: a live database, a live stub, and a workspace no
/// other case writes into.
struct Fixture {
    store: PgStore,
    context: RequestContext,
    adapter: HttpWebhookEffectAdapter,
    stub: AdapterStub,
}

impl Fixture {
    fn effects(&self) -> PgExternalEffectRepository {
        PgExternalEffectRepository::new(self.store.clone())
    }

    async fn shutdown(self) {
        self.stub.shutdown().await;
    }
}

/// `None` means there is nothing to run against, and the caller returns.
///
/// A skipped case prints why. A silent skip and a passing case look identical
/// in a summary line, and this file is the evidence for a release gate.
async fn fixture() -> Option<Fixture> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            println!(
                "skipping: DATABASE_URL is not set, so there is no persisted state to read an \
                 observation out of"
            );
            return None;
        }
    };

    let store = match PgStore::connect(&DatabaseConfig {
        url: SecretString::from(url),
        max_connections: 4,
    })
    .await
    {
        Ok(store) => store,
        Err(error) => {
            println!("skipping: the database named by DATABASE_URL is unreachable: {error}");
            return None;
        }
    };
    if let Err(error) = store.migrate().await {
        println!("skipping: the database named by DATABASE_URL could not be migrated: {error}");
        return None;
    }

    let stub = AdapterStub::start()
        .await
        .expect("the adapter stub binds to a loopback port");
    let adapter =
        HttpWebhookEffectAdapter::new(ADAPTER_NAME, stub.dispatch_url(), stub.read_back_url())
            .expect("the fixture adapter is configurable");

    Some(Fixture {
        store,
        context: RequestContext::new(WorkspaceId::new(), PrincipalId::new()),
        adapter,
        stub,
    })
}

/// An intent shaped by the adapter's own descriptor, so the real dispatch path
/// will accept it.
fn intent_for(fixture: &Fixture) -> ExternalEffectIntent {
    let descriptor = fixture.adapter.descriptor();
    ExternalEffectIntent::new(
        format!("run://{}", AgentRunId::new()),
        fixture.context.workspace_id,
        fixture.context.principal_id,
        descriptor.name(),
        "send",
        fixture.stub.dispatch_url(),
        "sha256:arguments",
        "deliver the observation fixture",
        vec![EffectPrecondition::new("resource-version", "v1").expect("a valid precondition")],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        descriptor.reversibility(),
        descriptor.idempotency_profile(),
        descriptor.delivery_semantics(),
        descriptor.required_capability(),
        None::<String>,
        None::<String>,
        now(),
    )
    .expect("the observation fixture intent is valid")
}

/// A receipt from a real dispatch to the real stub, which answers 200 — so
/// this is an *acknowledged* receipt, the same one points 4 and 5 leave behind.
fn acknowledged_receipt(fixture: &Fixture, intent: &ExternalEffectIntent) -> ExternalEffectReceipt {
    let authorization = EffectAuthorization::allow(
        "decision://observation-fixture",
        "observation-fixture-policy-v1",
        fixture.context.workspace_id,
        fixture.context.principal_id,
        intent.required_capability(),
        intent.operation().to_owned(),
        intent.target().to_owned(),
    );
    let authorized = intent
        .authorize(&authorization)
        .expect("the fixture authorization matches the intent");
    let receipt = authorized
        .dispatch(&fixture.adapter, intent.precondition_digest(), now())
        .expect("the fixture dispatch reaches the stub");
    assert_eq!(
        receipt.outcome_status(),
        EffectLifecycleStatus::Acknowledged,
        "the stub answers 200 and the adapter maps 2xx to acknowledged"
    );
    receipt
}

#[tokio::test]
async fn an_effect_that_is_not_persisted_is_not_an_observation() {
    let Some(fixture) = fixture().await else {
        return;
    };

    let error = observe(
        &fixture.store,
        &fixture.context,
        ExternalEffectId::new(),
        ANY_POINT,
        0,
    )
    .await
    .expect_err("an observation about an effect that is not there describes nothing");

    assert!(
        error.contains("no external effect"),
        "the refusal must say what it could not find: {error}"
    );

    fixture.shutdown().await;
}

#[tokio::test]
async fn an_intent_alone_and_nothing_dispatched_reads_back_as_prepared() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let intent = intent_for(&fixture);
    fixture
        .effects()
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 0)
        .await
        .expect("a persisted intent is observable");

    assert_eq!(observed.point, ANY_POINT);
    assert_eq!(observed.status, EffectLifecycleStatus::Prepared);
    assert!(!observed.receipt_persisted, "no receipt row was written");
    assert!(
        !observed.reconciliation_started,
        "nothing has enrolled this effect in reconciliation"
    );
    assert!(!observed.retry_attempted, "the stub counted no dispatch");

    fixture.shutdown().await;
}

#[tokio::test]
async fn a_young_persisted_dispatch_is_not_yet_enrolled_in_reconciliation() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let intent = intent_for(&fixture);
    fixture
        .effects()
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    fixture
        .effects()
        .record_dispatch_started(&fixture.context, intent.id(), now())
        .await
        .expect("the dispatch start is recorded");

    // The dispatch happened and the record of what came back did not survive.
    // The durable transition, not the stub's count, says where lifecycle got.
    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert_eq!(observed.status, EffectLifecycleStatus::Dispatching);
    assert!(!observed.receipt_persisted);
    assert!(
        !observed.reconciliation_started,
        "the real sweep does not declare a just-started dispatch lost"
    );
    assert!(!observed.retry_attempted, "one dispatch is not a retry");

    fixture.shutdown().await;
}

#[tokio::test]
async fn only_a_second_counted_dispatch_makes_it_a_retry() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let intent = intent_for(&fixture);
    fixture
        .effects()
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");

    let once = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");
    let twice = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 2)
        .await
        .expect("a persisted intent is observable");

    assert!(
        !once.retry_attempted,
        "the world was touched once, which is the first attempt"
    );
    assert!(
        twice.retry_attempted,
        "the world was touched twice, which only the dispatched-to party can say"
    );

    fixture.shutdown().await;
}

#[tokio::test]
async fn a_persisted_unknown_receipt_is_found_through_the_candidate_query() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let effects = fixture.effects();
    let intent = intent_for(&fixture);
    effects
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        now(),
        vec![format!("effect://{}/timeout", intent.id())],
    )
    .expect("a synthetic unknown receipt is valid");
    effects
        .insert_receipt(&fixture.context, &receipt)
        .await
        .expect("the fixture receipt is recorded");

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert!(
        observed.receipt_persisted,
        "the receipt row is there and the candidate query names it"
    );
    assert_eq!(
        observed.status,
        EffectLifecycleStatus::Unknown,
        "the status is the one the persisted receipt carries"
    );
    assert!(
        observed.reconciliation_started,
        "an UNKNOWN receipt puts the effect in the set the reconciliation sweep owns"
    );

    fixture.shutdown().await;
}

/// Multi-threaded because the domain's `dispatch` is synchronous and blocks
/// the caller until the far side answers, while the far side here is the stub
/// running on this very runtime. On a current-thread runtime the two deadlock
/// until the adapter's timeout turns a 200 into an UNKNOWN.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconciled_effect_is_found_through_the_outcome_its_run_is_owed() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let effects = fixture.effects();
    let intent = intent_for(&fixture);
    effects
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    let receipt = acknowledged_receipt(&fixture, &intent);
    effects
        .insert_receipt(&fixture.context, &receipt)
        .await
        .expect("the fixture receipt is recorded");
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ExternalResourceReadBack,
            Some(true),
            format!("{}#dispatches=1", fixture.stub.read_back_url()),
            vec![format!("effect://{}/read-back", intent.id())],
        )],
        now(),
    )
    .expect("the fixture reconciliation is valid");
    effects
        .insert_reconciliation(&fixture.context, &reconciliation)
        .await
        .expect("the fixture reconciliation is recorded");

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert!(
        observed.reconciliation_started,
        "a reconciliation row exists and is read back by its own id"
    );
    assert!(
        observed.receipt_persisted,
        "the reconciliation names the receipt, which is how it is reachable at all"
    );
    assert_eq!(
        observed.status,
        EffectLifecycleStatus::Reconciling,
        "a settled reconciliation appends the lifecycle evidence the observation reads"
    );

    fixture.shutdown().await;
}

/// A persisted acknowledged receipt is reachable from its effect id through
/// the receipt-recorded lifecycle transition.
/// Multi-threaded for the same reason as the case above.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_acknowledged_receipt_with_no_reconciliation_is_reachable_by_effect_id() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let effects = fixture.effects();
    let intent = intent_for(&fixture);
    effects
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    let receipt = acknowledged_receipt(&fixture, &intent);
    effects
        .insert_receipt(&fixture.context, &receipt)
        .await
        .expect("the fixture receipt is recorded");

    // The row is there: read by its own id it comes straight back.
    assert!(
        effects
            .find_receipt(&fixture.context, receipt.id())
            .await
            .expect("the receipt reads back by its own id")
            .is_some()
    );

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert!(observed.receipt_persisted);
    assert!(!observed.reconciliation_started);
    assert_eq!(observed.status, EffectLifecycleStatus::Acknowledged);

    fixture.shutdown().await;
}

/// A reconciliation that settled nothing is invisible to the query this
/// observation reaches reconciliations through, and the observation reports
/// that it read none rather than claiming a reading it did not make.
///
/// `find_undelivered_outcomes` selects `outcome = ANY(settled_names)`, and
/// `Inconclusive` — the provider answered and could not tell us — is not
/// settled. The candidate route cannot cover for it either, because that query
/// selects `outcome_status = 'unknown'` and this receipt is acknowledged.
///
/// No fault point produces this shape today: point 5's read-back answers
/// definitely, so its reconciliation is `Confirmed`. A read-back that came back
/// inconclusive would land here instead, and this case exists so that the day
/// it does is a failing test rather than a quiet one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unsettled_reconciliation_is_invisible_to_the_debt_query() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let effects = fixture.effects();
    let intent = intent_for(&fixture);
    effects
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    let receipt = acknowledged_receipt(&fixture, &intent);
    effects
        .insert_receipt(&fixture.context, &receipt)
        .await
        .expect("the fixture receipt is recorded");
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ExternalResourceReadBack,
            // The far side answered and would not say whether it applied.
            None,
            format!("{}#dispatches=unknown", fixture.stub.read_back_url()),
            vec![format!("effect://{}/read-back", intent.id())],
        )],
        now(),
    )
    .expect("the fixture reconciliation is valid");
    assert_eq!(
        reconciliation.outcome(),
        ReconciliationOutcome::Inconclusive,
        "an observation that does not say whether the effect applied settles nothing"
    );
    effects
        .insert_reconciliation(&fixture.context, &reconciliation)
        .await
        .expect("the fixture reconciliation is recorded");

    // The row is there: read by its own id it comes straight back.
    assert!(
        effects
            .find_reconciliation(&fixture.context, reconciliation.id())
            .await
            .expect("the reconciliation reads back by its own id")
            .is_some()
    );

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert!(
        !observed.reconciliation_started,
        "a debt query does not reach an unsettled reconciliation, and the observation must not \
         claim a reading it did not make"
    );
    assert!(observed.receipt_persisted);
    assert_eq!(observed.status, EffectLifecycleStatus::Acknowledged);

    fixture.shutdown().await;
}

/// A reconciliation whose run has already been told is invisible for the same
/// reason: `find_undelivered_outcomes` selects `notified_at IS NULL`, and a
/// paid debt is not owed.
///
/// This is the shape point 5 would take if anything marked the outcome
/// delivered between the child's abort and the parent's read. The case asserts
/// the observation before and after the marking, so what it pins is the effect
/// of paying the debt and nothing else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_already_notified_reconciliation_is_invisible_to_the_debt_query() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let effects = fixture.effects();
    let intent = intent_for(&fixture);
    effects
        .insert_intent(&fixture.context, &intent)
        .await
        .expect("the fixture intent is recorded");
    let receipt = acknowledged_receipt(&fixture, &intent);
    effects
        .insert_receipt(&fixture.context, &receipt)
        .await
        .expect("the fixture receipt is recorded");
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ExternalResourceReadBack,
            Some(true),
            format!("{}#dispatches=1", fixture.stub.read_back_url()),
            vec![format!("effect://{}/read-back", intent.id())],
        )],
        now(),
    )
    .expect("the fixture reconciliation is valid");
    effects
        .insert_reconciliation(&fixture.context, &reconciliation)
        .await
        .expect("the fixture reconciliation is recorded");

    let before = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");
    assert!(
        before.reconciliation_started,
        "a settled and undelivered outcome is the one shape the debt query does return"
    );

    effects
        .mark_outcome_delivered(&fixture.context, reconciliation.id(), now())
        .await
        .expect("the outcome is marked delivered");

    let observed = observe(&fixture.store, &fixture.context, intent.id(), ANY_POINT, 1)
        .await
        .expect("a persisted intent is observable");

    assert!(
        !observed.reconciliation_started,
        "paying the debt removed the only route this observation had to the row, and nothing \
         about the reconciliation itself changed"
    );
    assert!(observed.receipt_persisted);
    assert_eq!(observed.status, EffectLifecycleStatus::Confirmed);

    fixture.shutdown().await;
}

/// A run that was told to use a database may not report success having skipped.
///
/// Every database-backed case above returns early and prints why when there is
/// nothing to run against, and libtest captures that print for a passing test —
/// so a fully skipped run and a fully exercised one produce the same summary
/// line. That is right for a developer without Postgres and useless as
/// release-gate evidence.
///
/// Setting `VESTRACE_REQUIRE_DATABASE` says the caller expects a database, and
/// this case fails when there is not one. It checks a live connection rather
/// than the presence of `DATABASE_URL`, because a URL naming an unreachable
/// server skips just as quietly as no URL at all.
#[tokio::test]
async fn a_run_that_requires_a_database_cannot_skip_instead() {
    let demanded = match std::env::var(REQUIRE_DATABASE) {
        Ok(value) => !matches!(value.trim(), "" | "0" | "false"),
        Err(_) => false,
    };
    if !demanded {
        return;
    }

    let fixture = fixture().await;
    assert!(
        fixture.is_some(),
        "{REQUIRE_DATABASE} is set, so every case in this file must run against a real database; \
         the notice printed above says which part was unavailable"
    );
    fixture.expect("the fixture is present").shutdown().await;
}
