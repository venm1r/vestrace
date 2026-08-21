//! The process that dies on purpose, and the map from a requested fault point
//! to the last lifecycle stage it is allowed to finish.
//!
//! Everything here runs against the real services: the real repository over a
//! real Postgres connection, the real `PerformExternalEffectService`, and the
//! deployment's own `HttpWebhookEffectAdapter` pointed at the scenario's stub.
//! A child that simulated any of those would produce an observation about the
//! simulation, which is the failure mode this whole crate exists to avoid.

use std::sync::Arc;

use secrecy::SecretString;
use vestrace_application::{
    AuthorizationBoundary, ConfiguredCapabilityPolicyEngine, ExternalEffectService,
    PerformExternalEffectService, RequestContext, SharedExternalEffectRepository,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectPrecondition, EvidenceStrength, ExternalEffectAdapter,
    ExternalEffectIntent, ObservedEffectState, reconcile_effect,
};
use vestrace_domain::id::AgentRunId;
use vestrace_domain::{AuthorizationRequest, PrincipalId, RiskCategory, WorkspaceId, now};
use vestrace_infrastructure::{
    DatabaseConfig, HttpWebhookEffectAdapter, PgExternalEffectRepository, PgStore,
};

use crate::ScenarioSettings;

/// The last stage the child completes before it stops existing.
///
/// A stage is named for what is *true when it finishes*, not for the call that
/// produces it: `IntentPersisted` is "the intent is in the database", which is
/// the state the observation is about. Naming the call instead would leave the
/// map ambiguous the moment a call does two things.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildStage {
    IntentPersisted,
    Authorized,
    Dispatched,
    ReceiptPersisted,
    OutcomeSettled,
}

/// Which stage the child finishes before aborting, for a requested fault point.
///
/// Exhaustive on purpose, with no `_ =>` arm: a wildcard would route a fault
/// point added later to whatever stage happened to be written last, and the
/// resulting observation would be filed under a point the process never
/// actually crashed at. The compiler refusing to build is the cheaper failure.
pub fn aborts_at(point: EffectFaultPoint) -> ChildStage {
    match point {
        EffectFaultPoint::AfterIntentPersistence => ChildStage::IntentPersisted,
        EffectFaultPoint::AfterAuthorizationBeforeDispatch => ChildStage::Authorized,
        EffectFaultPoint::AfterDispatchBeforeReceipt => ChildStage::Dispatched,
        EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation => ChildStage::ReceiptPersisted,
        EffectFaultPoint::AfterOutcomeBeforeRunCommit => ChildStage::OutcomeSettled,
    }
}

/// The workspace every scenario child writes into.
///
/// Fixed rather than random because the parent has to read the same rows back
/// after the child is gone, and a workspace the child invented would die with
/// it. Row-level security scopes every query by this value, so the parent has
/// to be able to name it without being told.
pub const SCENARIO_WORKSPACE_ID: &str = "01900000-0000-7000-8000-0000000fa001";

/// The principal the scenario acts as. Fixed for the same reason.
pub const SCENARIO_PRINCIPAL_ID: &str = "01900000-0000-7000-8000-0000000fa002";

/// The prefix of the one identifying line the child writes before it dies.
///
/// The effect id is generated inside `ExternalEffectIntent::new` and cannot be
/// chosen by the caller, so the parent learns it from the child's stderr. This
/// is an identifier, not an observation: every field of the observation is
/// still read back out of the database or counted by the stub.
pub const EFFECT_ID_MARKER: &str = "vestrace-fault-scenario: effect_id=";

/// The prefix of the line a child writes when it stopped short of its point.
///
/// A setup failure and a reached fault point both end in `abort()`, so the
/// process status cannot tell them apart. This line and
/// [`completion_marker`] are what can, and the parent requires the latter
/// before it will accept any reading at all.
pub const SETUP_FAILURE_MARKER: &str =
    "vestrace-fault-scenario: child could not reach its fault point: ";

/// The line the child writes once it has finished the stage its point names.
///
/// The parent must see exactly this before it treats anything in the database
/// as an observation of that point. Its absence means the child died somewhere
/// the experiment did not choose, and a reading taken then would describe where
/// the child broke rather than where it was told to crash.
pub fn completion_marker(stage: ChildStage) -> String {
    format!("completed {stage:?}, aborting")
}

/// Whether the child's stderr says it reached the point that was asked for.
///
/// This is the parent's admission check, not a field of the observation: it
/// establishes that there *is* an observation to take, and nothing it reads
/// becomes part of one. The stage is required to be the requested point's own
/// stage rather than merely some stage, so a child that completed the wrong one
/// is refused as loudly as a child that completed none.
///
/// The two failures this closes are the same failure. A child that could not
/// dispatch and a child that dispatched and died both leave an aborted process
/// and an announced effect id; without this the parent would read the first
/// one's database and file the result under the second one's fault point. Point
/// 5's precondition guard is the sharpest case — it refuses *after*
/// `insert_reconciliation` has already committed, so its rows are
/// byte-identical to a successful point-5 run, and the completion line is the
/// only thing left that differs.
pub fn confirm_reached_point(stderr: &str, point: EffectFaultPoint) -> Result<(), String> {
    let stage = aborts_at(point);
    if stderr.contains(&completion_marker(stage)) {
        return Ok(());
    }
    Err(format!(
        "the scenario child never reported completing {stage:?}, so it stopped somewhere \
         other than the fault point it was asked for and there is no observation of that \
         point to take; its stderr was: {}",
        stderr.trim()
    ))
}

/// The adapter this scenario configures, named the way a deployment would name
/// one of its own.
const SCENARIO_ADAPTER: &str = "fault-scenario-webhook";

const POLICY_VERSION: &str = "fault-scenario-policy-v1";

/// The context the child writes under, and the one the parent must read under.
pub fn scenario_context() -> RequestContext {
    let workspace_id: WorkspaceId = SCENARIO_WORKSPACE_ID
        .parse()
        .expect("the scenario workspace id is a literal and parses");
    let principal_id: PrincipalId = SCENARIO_PRINCIPAL_ID
        .parse()
        .expect("the scenario principal id is a literal and parses");
    RequestContext::new(workspace_id, principal_id)
}

/// The stub's read-back route, derived from its dispatch route.
///
/// `run_child` is handed only the dispatch URL, and
/// `HttpWebhookEffectAdapter::new` refuses to build without a read-back URL —
/// deliberately, since an adapter that cannot be asked what happened can never
/// resolve an unknown outcome. Both routes belong to the same stub, so one is
/// derivable from the other rather than needing a second argument.
pub fn read_back_url_for(dispatch_url: &str) -> Result<String, String> {
    dispatch_url
        .strip_suffix("/dispatch")
        .map(|base| format!("{base}/effects"))
        .ok_or_else(|| {
            format!(
                "dispatch url `{dispatch_url}` does not end in /dispatch, so the stub's \
                 read-back route cannot be derived from it"
            )
        })
}

/// The dispatch URL the parent passes on the command line.
///
/// It travels in argv rather than in the environment because the invoking
/// contract clears the environment, and unlike the database URL a loopback
/// address is not a credential.
pub fn dispatch_url_argument(args: impl Iterator<Item = String>) -> Result<String, String> {
    let mut args = args;
    let mut url = None;
    while let Some(arg) = args.next() {
        if arg == "--dispatch-url" {
            url = args.next();
        }
    }
    url.ok_or_else(|| "dispatch url is required: pass --dispatch-url".to_owned())
}

/// Perform a real external effect and stop existing partway through it.
///
/// Never returns. The process ends in `abort()` whichever way this goes: an
/// orderly `exit()` flushes buffers and lets transactions finish, which is a
/// shutdown path nobody asked a question about. What this scenario measures is
/// what survives a process that stopped without being asked.
pub async fn run_child(settings: &ScenarioSettings, dispatch_url: &str) -> ! {
    match drive(settings, dispatch_url).await {
        Ok(stage) => abort_after(stage),
        Err(error) => {
            // A setup failure is not a fault observation, and saying so on
            // stderr is the only way the parent can tell the two apart: it sees
            // an aborted process either way.
            eprintln!("{SETUP_FAILURE_MARKER}{error}");
            std::process::abort()
        }
    }
}

fn abort_after(stage: ChildStage) -> ! {
    eprintln!("vestrace-fault-scenario: {}", completion_marker(stage));
    std::process::abort()
}

/// Walk the lifecycle up to and including the stage the point names.
///
/// Returns the stage it completed and lets the caller abort, so every fallible
/// step can use `?` and there is exactly one `abort()` on the success path.
///
/// # What no test proves about the `match stage` below
///
/// The design's §7 asks for a per-point test "driven by a fake service set
/// whose calls are recorded, proving the abort is where the table says". No
/// such test exists. `tests/child_points.rs` pins `aborts_at`, which is the
/// point-to-stage table, and `confirm_reached_point`, which is the parent's
/// admission check — neither observes a single call this function makes.
///
/// So the unproven property is precisely the one a drift would live in: that
/// each arm below issues the call sequence its stage names and stops there.
/// An arm that gained a call, lost one, or reordered two would move the abort
/// site by a stage while every test in this crate still passed, and the
/// resulting observation would be filed under a fault point the process never
/// crashed at.
///
/// It is unproven because proving it needs a recording double for
/// `SharedExternalEffectRepository`, `ExternalEffectService`,
/// `PerformExternalEffectService` and `ExternalEffectAdapter` at once, and
/// standing those up is a larger change than the wave that wrote this note
/// could carry honestly. Verified by reading in the meantime, which is weaker
/// than a test and is recorded as such here and in the delta document's
/// limitations rather than left to look covered.
async fn drive(settings: &ScenarioSettings, dispatch_url: &str) -> Result<ChildStage, String> {
    let stage = aborts_at(settings.point());

    let store = PgStore::connect(&DatabaseConfig {
        url: SecretString::from(settings.database_url().to_owned()),
        max_connections: 4,
    })
    .await
    .map_err(|error| format!("the scenario database is unreachable: {error}"))?;
    // The isolation is ephemeral by the time this runs — Task 1's guard refuses
    // anything else — so a schema that is not there yet is the normal case
    // rather than an operator's mistake.
    store
        .migrate()
        .await
        .map_err(|error| format!("the scenario database could not be migrated: {error}"))?;

    let effects: SharedExternalEffectRepository = Arc::new(PgExternalEffectRepository::new(store));

    let read_back_url = read_back_url_for(dispatch_url)?;
    let adapter = HttpWebhookEffectAdapter::new(SCENARIO_ADAPTER, dispatch_url, &read_back_url)
        .map_err(|error| format!("the scenario adapter is not configurable: {error}"))?;

    let authorization = AuthorizationBoundary::new(Arc::new(
        ConfiguredCapabilityPolicyEngine::new(
            POLICY_VERSION,
            [adapter.descriptor().required_capability()],
            RiskCategory::High,
        )
        .map_err(|error| format!("the scenario policy engine is not configurable: {error}"))?,
    ));
    let perform = PerformExternalEffectService::new(Arc::clone(&effects), authorization.clone());
    let granular = ExternalEffectService::new(Arc::clone(&effects), authorization);

    let context = scenario_context();
    let intent = scenario_intent(&context, &adapter, dispatch_url)?;
    // Announced before anything can fail, so the parent has the id even when
    // the child dies earlier than it meant to.
    eprintln!("{EFFECT_ID_MARKER}{}", intent.id());

    match stage {
        ChildStage::IntentPersisted => {
            effects
                .insert_intent(&context, &intent)
                .await
                .map_err(|error| format!("the intent could not be recorded: {error}"))?;
        }
        ChildStage::Authorized => {
            effects
                .insert_intent(&context, &intent)
                .await
                .map_err(|error| format!("the intent could not be recorded: {error}"))?;
            granular
                .authorize(&context, &intent, authorization_request(&intent))
                .await
                .map_err(|error| format!("the effect was not authorized: {error}"))?;
        }
        ChildStage::Dispatched => {
            effects
                .insert_intent(&context, &intent)
                .await
                .map_err(|error| format!("the intent could not be recorded: {error}"))?;
            let authorized = granular
                .authorize(&context, &intent, authorization_request(&intent))
                .await
                .map_err(|error| format!("the effect was not authorized: {error}"))?;
            // The receipt is produced and then dropped unwritten. That is the
            // whole of this fault point: the world was touched, and the only
            // record of what came back died with the process.
            granular
                .dispatch(
                    &context,
                    &authorized,
                    &adapter,
                    authorized.intent().precondition_digest(),
                    now(),
                )
                .await
                .map_err(|error| format!("the effect could not be dispatched: {error}"))?;
        }
        ChildStage::ReceiptPersisted => {
            perform
                .perform(&context, intent, &adapter, now())
                .await
                .map_err(|error| format!("the effect could not be performed: {error}"))?;
        }
        ChildStage::OutcomeSettled => {
            let receipt = perform
                .perform(&context, intent.clone(), &adapter, now())
                .await
                .map_err(|error| format!("the effect could not be performed: {error}"))?;

            let observation = read_back(&read_back_url, &intent).await?;
            let reconciliation = reconcile_effect(&intent, &receipt, vec![observation], now())
                .map_err(|error| format!("the outcome could not be reconciled: {error}"))?;
            effects
                .insert_reconciliation(&context, &reconciliation)
                .await
                .map_err(|error| format!("the reconciliation could not be recorded: {error}"))?;

            // The debt has to be outstanding for this fault point to mean
            // anything: an outcome that is settled and *not* owed is one the
            // run has already been told about, which is the state after the
            // commit rather than before it.
            //
            // This guard refuses *after* `insert_reconciliation` above has
            // already committed, so the rows it leaves behind are identical to
            // a successful point-5 run and no reading of the database can tell
            // the two apart. What tells them apart is upstream of the database:
            // the `Err` returned here takes the `SETUP_FAILURE_MARKER` path in
            // `run_child`, no completion line is ever written, and
            // `confirm_reached_point` refuses the run before the parent reads
            // anything. Without that check this refusal would be silent —
            // which is the failure it was added to prevent, reproduced by the
            // guard itself.
            let owed = effects
                .find_undelivered_outcomes(&context, 32)
                .await
                .map_err(|error| format!("the owed outcomes could not be read: {error}"))?;
            if !owed
                .iter()
                .any(|outcome| outcome.reconciliation.id() == reconciliation.id())
            {
                return Err(
                    "the settled outcome is owed to nothing, so there is no run commit \
                            to die before"
                        .to_owned(),
                );
            }
        }
    }

    Ok(stage)
}

/// The authorization the lifecycle asks for, built from the intent exactly as
/// `PerformExternalEffectService::perform` builds it.
fn authorization_request(intent: &ExternalEffectIntent) -> AuthorizationRequest {
    AuthorizationRequest::new(
        intent.required_capability(),
        intent.operation().to_owned(),
        intent.target().to_owned(),
        intent.risk(),
    )
}

/// The intent the scenario performs, shaped like the fixture in
/// `tests/e1_e4_connect.rs` and pointed at the stub.
///
/// Every guarantee comes off the adapter's own descriptor rather than being
/// written here, which is what the HTTP surface does at
/// `crates/vestrace-http/src/api/effects.rs`: a caller that could state its own
/// reversibility or idempotency would be claiming a property of somebody else's
/// system.
fn scenario_intent(
    context: &RequestContext,
    adapter: &HttpWebhookEffectAdapter,
    dispatch_url: &str,
) -> Result<ExternalEffectIntent, String> {
    let descriptor = adapter.descriptor();
    ExternalEffectIntent::new(
        // A fresh run each time: one fault point's settled outcome is not owed
        // to the next one's run.
        format!("run://{}", AgentRunId::new()),
        context.workspace_id,
        context.principal_id,
        descriptor.name(),
        "send",
        dispatch_url,
        "sha256:arguments",
        "deliver the scenario effect to the adapter stub",
        vec![
            EffectPrecondition::new("resource-version", "v1")
                .map_err(|error| format!("the scenario precondition is invalid: {error}"))?,
        ],
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
    .map_err(|error| format!("the scenario intent is invalid: {error}"))
}

/// Ask the stub what it received.
///
/// The stub answers `{"dispatches":N}` rather than the production read-back
/// vocabulary, so this reads that route directly instead of going through
/// `HttpExternalEffectReadBackAdapter`. It is still an observation made by the
/// party on the receiving end: a dispatch the stub counted is one that arrived,
/// and one it did not count is one that did not.
async fn read_back(
    read_back_url: &str,
    intent: &ExternalEffectIntent,
) -> Result<ObservedEffectState, String> {
    let body: serde_json::Value = reqwest::get(read_back_url)
        .await
        .map_err(|error| format!("the stub could not be read back: {error}"))?
        .json()
        .await
        .map_err(|error| format!("the stub's read-back answer is not JSON: {error}"))?;
    let dispatches = body
        .get("dispatches")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("the stub's read-back answer names no dispatches: {body}"))?;

    Ok(ObservedEffectState::new(
        EvidenceStrength::ExternalResourceReadBack,
        Some(dispatches > 0),
        format!("{read_back_url}#dispatches={dispatches}"),
        vec![format!("effect://{}/read-back", intent.id())],
    ))
}
