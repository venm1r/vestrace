//! Read the observation back out of what survived the child.
//!
//! The child process is gone by the time this runs, and everything it believed
//! about the effect went with it. What is left is rows in a database and a
//! count held by the party that was dispatched to, and those are the only two
//! things this module is allowed to consult. It never constructs the suite's
//! `expected` answer — a test in `tests/contract.rs` refuses a build that does
//! — and it never takes a caller's word for a field the world can be asked
//! about instead.

use vestrace_application::{
    ExternalEffectRecoveryCandidate, ExternalEffectRepository, RequestContext,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, EffectLifecycleStatus, ExternalEffectReceipt, ExternalReconciliation,
    FaultObservation,
};
use vestrace_domain::{ExternalEffectId, now};
use vestrace_infrastructure::{PgExternalEffectRepository, PgStore};

/// How many owed outcomes are read while looking for this effect's.
///
/// The port offers no "the reconciliation for this effect" query, only "the
/// settled outcomes this workspace still owes its runs", so the row is found by
/// filtering that page. A ceiling can hide a row, and this one is safe for the
/// reason the settings guard already enforces: the scenario refuses to run
/// against anything but an ephemeral database, so the workspace holds the five
/// runs of one invocation and nothing else.
const OWED_OUTCOME_PAGE: u32 = 256;

/// What is true about one external effect after the process that was
/// performing it stopped existing.
///
/// `point` is carried through rather than derived: it is the question that was
/// asked, and an answer cannot establish its own question. Every other field is
/// read back.
///
/// `dispatch_count` arrives as a plain number because the party that counted it
/// is the stub, and by the time an observation is being assembled the only
/// thing worth passing on is the count itself. It is load-bearing twice over —
/// once for `retry_attempted`, and once because a dispatch nobody kept a
/// receipt for is indistinguishable in the database from an effect that was
/// never dispatched at all.
pub async fn observe(
    store: &PgStore,
    context: &RequestContext,
    intent_id: ExternalEffectId,
    point: EffectFaultPoint,
    dispatch_count: usize,
) -> Result<FaultObservation, String> {
    let effects = PgExternalEffectRepository::new(store.clone());

    // An observation about an effect that is not there describes nothing, so
    // this refuses rather than returning a default that would read as "the
    // lifecycle got no further than Prepared".
    if effects
        .find_intent(context, intent_id)
        .await
        .map_err(|error| format!("the effect intent could not be read: {error:?}"))?
        .is_none()
    {
        return Err(format!(
            "no external effect {intent_id} is persisted in this workspace, so there is \
             nothing to observe"
        ));
    }

    let reconciliation = owed_reconciliation(&effects, context, intent_id).await?;
    let candidate = sweep_candidate(&effects, context, intent_id).await?;

    let receipt = receipt_of(
        &effects,
        context,
        reconciliation.as_ref(),
        candidate.as_ref(),
    )
    .await?;

    // Both halves are readings of persisted state, and they say different
    // things. A reconciliation row is recovery having run; a candidate is the
    // effect being enrolled in the sweep that runs it. Neither is the parent
    // saying it called something.
    let reconciliation_started = reconciliation.is_some() || candidate.is_some();

    Ok(FaultObservation {
        point,
        status: status_of(receipt.as_ref(), dispatch_count),
        // Only the dispatched-to party can say the world was touched twice.
        retry_attempted: dispatch_count > 1,
        reconciliation_started,
        // A row was read or it was not. The parent asserting this would be a
        // claim rather than a reading.
        receipt_persisted: receipt.is_some(),
    })
}

/// The lifecycle status the persisted evidence supports.
///
/// A persisted receipt carries a status and that is the answer. Without one
/// there are two different states that look identical in the database, and the
/// stub separates them: a dispatch it counted is an effect that reached the
/// world with no surviving record of what came back, which is precisely
/// UNKNOWN. No counted dispatch and the effect never left, which is PREPARED.
///
/// Nothing here consults what the lifecycle *should* have reached. An
/// authorization leaves no row anywhere in this system, so PREPARED and
/// AUTHORIZED are not distinguishable by reading, and this reports the one it
/// can actually see.
fn status_of(
    receipt: Option<&ExternalEffectReceipt>,
    dispatch_count: usize,
) -> EffectLifecycleStatus {
    match receipt {
        Some(receipt) => receipt.outcome_status(),
        None if dispatch_count > 0 => EffectLifecycleStatus::Unknown,
        None => EffectLifecycleStatus::Prepared,
    }
}

/// The receipt, by whichever route can reach it from an effect id.
///
/// `find_receipt` takes a receipt id, which nothing outside the dead process
/// ever held, so the id has to come from another row first: a reconciliation
/// names the receipt it is about, and a sweep candidate carries the receipt it
/// was selected on. When neither exists the receipt is unreachable — which is
/// not the same as absent, and is reported as "not read" rather than guessed
/// at.
async fn receipt_of(
    effects: &PgExternalEffectRepository,
    context: &RequestContext,
    reconciliation: Option<&ExternalReconciliation>,
    candidate: Option<&ExternalEffectRecoveryCandidate>,
) -> Result<Option<ExternalEffectReceipt>, String> {
    if let Some(reconciliation) = reconciliation {
        return effects
            .find_receipt(context, reconciliation.receipt_id())
            .await
            .map_err(|error| format!("the effect receipt could not be read: {error:?}"));
    }
    Ok(candidate.map(|candidate| candidate.receipt().clone()))
}

/// The reconciliation recorded for this effect, read back by its own id.
///
/// Found through the outcomes the workspace still owes its runs, because that
/// is the only query on this port that reaches a reconciliation without already
/// knowing its id. Having found the id it is read again through
/// `find_reconciliation`, which re-checks the row's indexed columns against its
/// payload — a check the joined listing does not perform.
async fn owed_reconciliation(
    effects: &PgExternalEffectRepository,
    context: &RequestContext,
    intent_id: ExternalEffectId,
) -> Result<Option<ExternalReconciliation>, String> {
    let owed = effects
        .find_undelivered_outcomes(context, OWED_OUTCOME_PAGE)
        .await
        .map_err(|error| format!("the owed outcomes could not be read: {error:?}"))?;
    let Some(outcome) = owed
        .into_iter()
        .find(|outcome| outcome.reconciliation.effect_id() == intent_id)
    else {
        return Ok(None);
    };

    effects
        .find_reconciliation(context, outcome.reconciliation.id())
        .await
        .map_err(|error| format!("the reconciliation could not be read: {error:?}"))
}

/// This effect's place in the reconciliation sweep, if it has one.
///
/// The cutoff is the present moment because the question is what the sweep owns
/// *now*: an effect whose last attempt settled nothing and is older than now is
/// one the sweep would pick up on its next tick.
async fn sweep_candidate(
    effects: &PgExternalEffectRepository,
    context: &RequestContext,
    intent_id: ExternalEffectId,
) -> Result<Option<ExternalEffectRecoveryCandidate>, String> {
    let candidates = effects
        .find_reconciliation_candidates(context, now())
        .await
        .map_err(|error| format!("the reconciliation candidates could not be read: {error:?}"))?;
    Ok(candidates
        .into_iter()
        .find(|candidate| candidate.intent().id() == intent_id))
}
