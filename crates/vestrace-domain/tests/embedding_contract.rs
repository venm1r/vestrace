//! The embedding vocabulary, before any table or service exists.
//!
//! Every closed set here is quoted from the frozen spec rather than designed:
//! the transition lifecycle is line 203, the carry header and mapping states are
//! line 253, the barrier states are line 256, and generation readiness is lines
//! 207 and 219. A state that cannot be traced to one of those lines does not
//! belong in these types, and a state named there and missing here is a defect.

use vestrace_domain::WorkspaceId;
use vestrace_domain::embedding::{
    BarrierState, CarryHeaderState, CarryMappingState, CorpusGenerationState, EmbeddingJobKind,
    EmbeddingJobState, EmbeddingSpaceKey, EmbeddingSpaceTransitionState, TransitionInputOrdinal,
    TransitionRecipeOrdinal, UnknownEmbeddingJobKind,
};

fn space() -> EmbeddingSpaceKey {
    EmbeddingSpaceKey::new(
        WorkspaceId::new(),
        "nomic-768",
        "text-embedding-nomic-embed-text-v1.5",
        768,
    )
    .expect("a complete space key is lawful")
}

/// A space key is the whole tuple or it is nothing.
///
/// The lazy `ensure_space(context, name, model, dimensions)` this package
/// replaces could mint a space as a side effect of its first write, which left
/// no moment at which anyone authorized it. A key that cannot be built from a
/// name alone removes that path at the type level.
#[test]
fn a_space_key_cannot_be_built_from_a_name_alone() {
    assert!(EmbeddingSpaceKey::new(WorkspaceId::new(), "", "model", 768).is_err());
    assert!(EmbeddingSpaceKey::new(WorkspaceId::new(), "name", "", 768).is_err());
    assert!(EmbeddingSpaceKey::new(WorkspaceId::new(), "name", "model", 0).is_err());
    let key = space();
    assert_eq!(key.dimensions(), 768);
}

/// Two spaces that differ only in their model are different spaces.
///
/// P03's branch isolation established the shape of this hazard: two Connections
/// advertising the same wire model id must stay separate, so identity comes from
/// the pinned tuple and never from a display string.
#[test]
fn space_keys_differ_when_any_component_differs() {
    let workspace = WorkspaceId::new();
    let base = EmbeddingSpaceKey::new(workspace, "space", "model-a", 768).unwrap();
    for other in [
        EmbeddingSpaceKey::new(workspace, "space", "model-b", 768).unwrap(),
        EmbeddingSpaceKey::new(workspace, "other", "model-a", 768).unwrap(),
        EmbeddingSpaceKey::new(workspace, "space", "model-a", 1536).unwrap(),
        EmbeddingSpaceKey::new(WorkspaceId::new(), "space", "model-a", 768).unwrap(),
    ] {
        assert_ne!(base, other);
    }
}

/// Spec line 203, quoted: `Planned -> Rebuilding -> ReadyToActivate ->
/// Activated | Stale | Failed`.
#[test]
fn the_transition_lifecycle_is_exactly_the_spec_line() {
    use EmbeddingSpaceTransitionState::*;
    for (from, to) in [
        (Planned, Rebuilding),
        (Rebuilding, ReadyToActivate),
        (ReadyToActivate, Activated),
        (ReadyToActivate, Stale),
        (ReadyToActivate, Failed),
        (Planned, Stale),
        (Rebuilding, Stale),
        (Planned, Failed),
        (Rebuilding, Failed),
    ] {
        assert!(from.may_advance_to(to), "{from:?} -> {to:?} must be lawful");
    }
    for (from, to) in [
        (Activated, Rebuilding),
        (Activated, Stale),
        (Stale, Activated),
        (Failed, ReadyToActivate),
        (ReadyToActivate, Planned),
        (Rebuilding, Planned),
    ] {
        assert!(
            !from.may_advance_to(to),
            "{from:?} -> {to:?} must be refused"
        );
    }
    assert!(Activated.is_terminal() && Stale.is_terminal() && Failed.is_terminal());
    assert!(!Planned.is_terminal() && !Rebuilding.is_terminal() && !ReadyToActivate.is_terminal());
}

/// Spec line 253: the carry header closes as
/// `AwaitingAcknowledgement -> SuccessorCreated | NoLongerRequired`.
#[test]
fn the_carry_header_closes_where_the_spec_closes_it() {
    use CarryHeaderState::*;
    assert!(AwaitingAcknowledgement.may_advance_to(SuccessorCreated));
    assert!(AwaitingAcknowledgement.may_advance_to(NoLongerRequired));
    // A resolved carry is history. Reopening it would give one ambiguity head a
    // second direct successor, which line 251 forbids outright.
    assert!(!SuccessorCreated.may_advance_to(AwaitingAcknowledgement));
    assert!(!NoLongerRequired.may_advance_to(AwaitingAcknowledgement));
    assert!(!SuccessorCreated.may_advance_to(NoLongerRequired));
    assert!(!NoLongerRequired.may_advance_to(SuccessorCreated));
}

/// Spec line 253: a per-recipe mapping row carries
/// `AwaitingAcknowledgement | NoLongerRequired` and nothing else.
#[test]
fn a_carry_mapping_has_two_states_and_no_more() {
    use CarryMappingState::*;
    assert!(AwaitingAcknowledgement.may_advance_to(NoLongerRequired));
    assert!(!NoLongerRequired.may_advance_to(AwaitingAcknowledgement));
}

/// SQL persists each of these three closed vocabularies independently.  Their
/// `ALL` sets are the contract-test authority that prevents a migration from
/// accepting a state Rust cannot name (or the reverse).
#[test]
fn persisted_transition_state_vocabularies_are_closed() {
    assert_eq!(
        CarryHeaderState::ALL,
        [
            CarryHeaderState::AwaitingAcknowledgement,
            CarryHeaderState::SuccessorCreated,
            CarryHeaderState::NoLongerRequired,
        ]
    );
    assert_eq!(
        CarryHeaderState::ALL.map(CarryHeaderState::as_str),
        [
            "awaiting_acknowledgement",
            "successor_created",
            "no_longer_required",
        ]
    );
    assert_eq!(
        CarryMappingState::ALL,
        [
            CarryMappingState::AwaitingAcknowledgement,
            CarryMappingState::NoLongerRequired,
        ]
    );
    assert_eq!(
        CarryMappingState::ALL.map(CarryMappingState::as_str),
        ["awaiting_acknowledgement", "no_longer_required"]
    );
    assert_eq!(
        BarrierState::ALL,
        [
            BarrierState::AwaitingPredecessorTerminal,
            BarrierState::ResolvedToCarry,
            BarrierState::ResolvedSatisfiedExisting,
            BarrierState::ResolvedDefinite,
            BarrierState::NoLongerRequired,
            BarrierState::Superseded,
        ]
    );
    assert_eq!(
        BarrierState::ALL.map(BarrierState::as_str),
        [
            "awaiting_predecessor_terminal",
            "resolved_to_carry",
            "resolved_satisfied_existing",
            "resolved_definite",
            "no_longer_required",
            "superseded",
        ]
    );
}

/// Spec line 256: `AwaitingPredecessorTerminal -> ResolvedToCarry |
/// ResolvedSatisfiedExisting | ResolvedDefinite | NoLongerRequired | Superseded`.
#[test]
fn the_barrier_lifecycle_is_exactly_the_spec_line() {
    use BarrierState::*;
    for terminal in [
        ResolvedToCarry,
        ResolvedSatisfiedExisting,
        ResolvedDefinite,
        NoLongerRequired,
        Superseded,
    ] {
        assert!(AwaitingPredecessorTerminal.may_advance_to(terminal));
        assert!(terminal.is_terminal(), "{terminal:?} must be terminal");
        // "Superseded barriers and batches are immutable history and cannot be
        // reused, dispatched, or classified" — line 256.
        assert!(!terminal.may_advance_to(ResolvedToCarry) || terminal == ResolvedToCarry);
    }
    assert!(!AwaitingPredecessorTerminal.is_terminal());
    assert!(!Superseded.may_advance_to(ResolvedToCarry));
    assert!(!ResolvedDefinite.may_advance_to(Superseded));
}

/// Spec line 253 requires classification against "the predecessor's fixed
/// ordered wire batch". A bare integer index invites the reorder the same line
/// forbids, so recipe and input ordinals are distinct types that cannot be
/// compared or swapped.
#[test]
fn recipe_and_input_ordinals_are_distinct_types() {
    let recipe = TransitionRecipeOrdinal::new(3);
    let input = TransitionInputOrdinal::new(3);
    assert_eq!(recipe.value(), input.value());
    // The types are distinct, so this is the only lawful way to relate them and
    // a caller must say which it means. `assert_eq!(recipe, input)` does not
    // compile, which is the property under test.
    assert_eq!(recipe, TransitionRecipeOrdinal::new(3));
    assert_ne!(recipe, TransitionRecipeOrdinal::new(4));
}

/// Spec line 219: "Its closed kinds are `retrieval_query`, `delivery`, and
/// `rebuild` (backfill is `rebuild` mode)."
///
/// The first version of this test asserted two kinds, because the sentence "For
/// `delivery` or `rebuild`, the validated production response must match the
/// job's exact EmbeddingSpaceKey" was mistaken for the closed set. That sentence
/// names the kinds whose response is persisted as vectors, which is a different
/// question, and the omission made `retrieval_query` -- the kind retrieval
/// issues -- unrepresentable.
#[test]
fn the_job_kinds_are_the_three_the_spec_closes_over() {
    assert_eq!(EmbeddingJobKind::RetrievalQuery.as_str(), "retrieval_query");
    assert_eq!(EmbeddingJobKind::Delivery.as_str(), "delivery");
    assert_eq!(EmbeddingJobKind::Rebuild.as_str(), "rebuild");
    assert_eq!(EmbeddingJobKind::ALL.len(), 3);
    for kind in EmbeddingJobKind::ALL {
        assert_eq!(kind.as_str().parse(), Ok(kind));
    }
    // Only these two reach the corpus. A retrieval query embeds and persists
    // nothing, so a fence that treated it as a writer would fence the wrong set.
    assert!(EmbeddingJobKind::Delivery.produces_persisted_vectors());
    assert!(EmbeddingJobKind::Rebuild.produces_persisted_vectors());
    assert!(!EmbeddingJobKind::RetrievalQuery.produces_persisted_vectors());
    // Closed and case-exact: the refusal names what it was given, so a
    // misspelled column value cannot be mistaken for an absent one.
    assert_eq!(
        "Delivery".parse::<EmbeddingJobKind>(),
        Err(UnknownEmbeddingJobKind("Delivery".to_owned()))
    );
    assert_eq!(
        "transition".parse::<EmbeddingJobKind>(),
        Err(UnknownEmbeddingJobKind("transition".to_owned()))
    );
}

/// Frozen spec section 11.6, line 225: a requested job may start, be cancelled
/// before dispatch, or fail definitely before dispatch. Once running, it has
/// the same four terminal outcomes.
///
/// Six states. An earlier draft carried ten, having taken `Waiting`,
/// `Authorized`, `Dispatching` and `ResultPrepared` from line 256's list of the
/// phases an overlapping predecessor *physical attempt* may be found in. Those
/// belong to the effect, the attempt's pre-dispatch phase, and the
/// `EmbeddingJobResultPrepared` marker. A job that also carried `Dispatching`
/// would be a second answer to whether the provider was reached.
#[test]
fn the_job_lifecycle_has_only_the_explicitly_allowed_edges() {
    use EmbeddingJobState::*;

    assert_eq!(EmbeddingJobState::ALL.len(), 6);

    let allowed = [
        (Requested, Running),
        (Requested, FailedDefinite),
        (Requested, Cancelled),
        (Running, Succeeded),
        (Running, FailedDefinite),
        (Running, InconclusiveUnknown),
        (Running, Cancelled),
    ];
    for from in EmbeddingJobState::ALL {
        for to in EmbeddingJobState::ALL {
            assert_eq!(
                from.may_advance_to(to),
                allowed.contains(&(from, to)),
                "{from:?} -> {to:?} must match the explicit allowed-edge list"
            );
        }
    }

    for terminal in [Succeeded, FailedDefinite, InconclusiveUnknown, Cancelled] {
        assert!(terminal.is_terminal(), "{terminal:?}");
    }
    assert!(!Requested.is_terminal() && !Running.is_terminal());

    // Line 257: the ambiguity head is the only state an authorized
    // acknowledgement may give a successor, and no scheduler may advance it.
    assert!(InconclusiveUnknown.may_have_a_successor());
    for state in [Requested, Running, Succeeded, FailedDefinite, Cancelled] {
        assert!(!state.may_have_a_successor(), "{state:?}");
    }
}

/// Frozen spec section 11.6, line 225 allows a durable cancellation command
/// to terminalize a requested job before dispatch. This state predicate grants
/// no authority and proves no effect guard: authorization, expected version,
/// the absence of Dispatching, lease release, and reservation abandonment
/// remain duties of that guarded command.
#[test]
fn a_requested_job_may_be_cancelled_before_dispatch() {
    assert!(EmbeddingJobState::Requested.may_advance_to(EmbeddingJobState::Cancelled));
}

/// Frozen spec section 11.6, line 225 also permits a proved pre-dispatch
/// failure to be recorded as definite.
#[test]
fn a_requested_job_may_fail_definitely_before_dispatch() {
    assert!(EmbeddingJobState::Requested.may_advance_to(EmbeddingJobState::FailedDefinite));
}

/// Spec line 207: a Ready generation is CAS-published, and line 219: the
/// finalizer "marks any current Ready generation for that space Stale/not
/// current". Ready and Stale are the two states those lines name.
#[test]
fn a_generation_goes_ready_then_stale_and_never_back() {
    use CorpusGenerationState::*;
    assert!(Ready.may_advance_to(Stale));
    assert!(!Stale.may_advance_to(Ready));
    assert!(!Ready.may_advance_to(Ready));
}

/// A space key is not `Deserialize`, so no HTTP body, config file, or A2A
/// payload can mint one. This mirrors P03's deliberate refusal to derive
/// `Deserialize` on `QualificationTargetBinding`.
///
/// The proof is a `compile_fail` doc test on `EmbeddingSpaceKey` itself, not
/// here: doc tests do not run for integration test targets, so a `compile_fail`
/// block in this file would assert nothing while looking like it did. What this
/// test covers is the accessor surface, so the type stays usable without being
/// constructible from untrusted input.
#[test]
fn the_space_key_exposes_its_tuple_without_being_constructible_from_input() {
    let key = space();
    assert!(!key.model().is_empty());
    assert!(!key.name().is_empty());
    assert_eq!(key.dimensions(), 768);
}
