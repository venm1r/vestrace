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
    DISPATCH_CONSIDERED_LOST_AFTER, ExternalEffectRecoveryCandidate, ExternalEffectRepository,
    RECONCILIATION_RETRY_AFTER, RequestContext,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, ExternalReconciliation, FaultObservation,
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
/// asked, and an answer cannot establish its own question. Lifecycle, receipt
/// and reconciliation fields are read back from persisted evidence.
///
/// `dispatch_count` arrives as a plain number because the party that counted it
/// is the stub, and by the time an observation is being assembled the only
/// thing worth passing on is the count itself. Its only job here is deciding
/// whether the dispatched-to party observed a retry; it is not a lifecycle
/// oracle.
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

    let receipt = effects
        .find_receipt_by_effect(context, intent_id)
        .await
        .map_err(|error| format!("the effect receipt could not be read: {error:?}"))?;
    let status = effects
        .find_lifecycle_status(context, intent_id)
        .await
        .map_err(|error| format!("the effect lifecycle could not be read: {error:?}"))?
        .ok_or_else(|| {
            format!("external effect {intent_id} has no persisted lifecycle evidence")
        })?;

    // Both halves are readings of persisted state, and they say different
    // things. A reconciliation row is recovery having run; a candidate is the
    // effect being enrolled in the sweep that runs it. Neither is the parent
    // saying it called something.
    let reconciliation_started = reconciliation.is_some() || candidate.is_some();

    Ok(FaultObservation {
        point,
        status,
        // Only the dispatched-to party can say the world was touched twice.
        retry_attempted: dispatch_count > 1,
        reconciliation_started,
        // A row was read or it was not. The parent asserting this would be a
        // claim rather than a reading.
        receipt_persisted: receipt.is_some(),
    })
}

/// The reconciliation recorded for this effect, read back by its own id.
///
/// Found through the outcomes the workspace still owes its runs, because that
/// is the only query on this port that reaches a reconciliation without already
/// knowing its id. Having found the id it is read again through
/// `find_reconciliation`, which re-checks the row's indexed columns against its
/// payload — a check the joined listing does not perform.
///
/// # What this route cannot see
///
/// `find_undelivered_outcomes` is a debt query, not a reconciliation query. Its
/// predicate is `notified_at IS NULL AND outcome = ANY(settled_names)`, so two
/// kinds of reconciliation row are invisible to it:
///
/// - an **unsettled** one — `Inconclusive`, the provider answered and could not
///   tell us — which is a reconciliation that plainly ran; and
/// - an **already notified** one, whose run has been told, which is a
///   reconciliation that ran and finished.
///
/// The candidate route covers neither gap unless the receipt is `unknown`,
/// because `find_reconciliation_candidates` selects on exactly that status. So
/// an effect carrying an acknowledged receipt and either kind of row reads back
/// as `reconciliation_started: false` with the row sitting in the table.
///
/// This is harmless for the five fault points as they stand: point 5 leaves a
/// `Confirmed` reconciliation that nothing has marked delivered, which is
/// precisely the one shape this query does return. It is written down because
/// that is a coincidence of the scenario rather than a property of the code —
/// a read-back that came back inconclusive, or a sweep that paid the debt
/// before the parent looked, would turn the observation silently false.
/// `tests/observation.rs` pins both shapes so the day it stops being harmless
/// is a failing test rather than a quiet one.
///
/// Widening the query would be a change to the port, which is not this
/// function's business.
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
/// Membership is decided by the receipt: `find_reconciliation_candidates`
/// selects effects whose receipt is `unknown`, and an effect that has never
/// been reconciled is in the set regardless of any cutoff.
///
/// `retry_unsettled_before` is narrower than it sounds. It is a re-ask cadence
/// and it applies only to an effect that *already* has a reconciliation whose
/// latest attempt settled nothing: that one comes back only once the attempt is
/// older than the cutoff. Observation uses the same two cutoffs as the real
/// sweep: otherwise a young `Dispatching` transition would be reported as
/// enrolled even though recovery deliberately still treats it as in flight.
async fn sweep_candidate(
    effects: &PgExternalEffectRepository,
    context: &RequestContext,
    intent_id: ExternalEffectId,
) -> Result<Option<ExternalEffectRecoveryCandidate>, String> {
    let observed_at = now();
    let candidates = effects
        .find_reconciliation_candidates(
            context,
            observed_at - RECONCILIATION_RETRY_AFTER,
            observed_at - DISPATCH_CONSIDERED_LOST_AFTER,
        )
        .await
        .map_err(|error| format!("the reconciliation candidates could not be read: {error:?}"))?;
    Ok(candidates
        .into_iter()
        .find(|candidate| candidate.intent().id() == intent_id))
}
