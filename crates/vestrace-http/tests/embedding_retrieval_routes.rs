//! What the retrieval surfaces promise before anyone calls them.
//!
//! The route inventory is the one place a route's exposure, capability and risk
//! are declared, and the router refuses to mount a path the inventory does not
//! name. So a route that is wrong here is wrong everywhere, and it is wrong
//! before a single request is served -- which is the only time this particular
//! mistake is cheap.
//!
//! Handler behaviour -- confirmation, path and body agreement, idempotency,
//! conflict on a stale version -- is proven beside the handler, where the
//! router state fixture lives.

use axum::http::Method;
use vestrace_domain::{Capability, RiskCategory};
use vestrace_http::route_inventory::{
    RouteDecision, RouteExposure, inventory_lookup, route_descriptor, route_inventory,
};

const RETRY_PATH: &str = "/v1/embedding-jobs/{id}/retry-generation-changed";
const VIEW_PATH: &str = "/v1/embedding-jobs/{id}/retrieval";

/// The retry is a governed mutation, not a read, and it is not public.
///
/// A retrieval retry spends a further provider call against a corpus the caller
/// has already been told moved. Exposing it unauthenticated, or under a
/// capability a reader would hold, would let anyone turn one declined answer
/// into an unbounded series of charges.
#[test]
fn the_retrieval_retry_is_a_critical_governed_mutation() {
    let descriptor = route_descriptor(&Method::POST, RETRY_PATH);

    assert_eq!(
        descriptor.exposure,
        RouteExposure::Governed,
        "a retry that spends a provider call is never public"
    );
    assert_eq!(
        descriptor.capability,
        Capability::EmbeddingRetryRetrievalGenerationChanged,
        "the retry carries its own capability rather than borrowing another retry's"
    );
    assert_eq!(
        descriptor.risk,
        RiskCategory::Critical,
        "spending an unplanned provider call is the risk this records"
    );
}

/// A concrete request for it resolves to a governed decision naming that
/// capability, and not to the default denial an unknown path gets.
#[test]
fn a_concrete_retry_request_resolves_to_its_governed_decision() {
    let path = format!(
        "/v1/embedding-jobs/{}/retry-generation-changed",
        uuid::Uuid::now_v7()
    );
    match inventory_lookup(&Method::POST, &path) {
        RouteDecision::Governed(request) => {
            assert_eq!(
                request.capability,
                Capability::EmbeddingRetryRetrievalGenerationChanged
            );
        }
        other => panic!("a mounted retry must resolve to a governed decision, got {other:?}"),
    }

    // The same path under a read verb is not this route, and resolves to
    // nothing rather than to something adjacent.
    assert_eq!(
        inventory_lookup(&Method::GET, &path),
        RouteDecision::NotInInventory,
        "a retry is a POST; no other verb may borrow its authorization"
    );
}

/// It does not share a capability with the two retries that already exist.
///
/// They follow an outcome nobody observed; this follows a definite answer that
/// arrived against a corpus that had already changed. Granting one must not
/// grant the others, or an operator authorizing a recovery would be authorizing
/// a spend.
#[test]
fn the_retrieval_retry_capability_is_its_own() {
    let paths: Vec<&str> = route_inventory()
        .iter()
        .filter(|descriptor| {
            descriptor.capability == Capability::EmbeddingRetryRetrievalGenerationChanged
        })
        .map(|descriptor| descriptor.path_pattern)
        .collect();
    assert_eq!(
        paths,
        vec![RETRY_PATH],
        "exactly one route may be authorized by this capability"
    );

    for wire in [
        "embedding.retry_after_unknown",
        "embedding.retry_carried_transition_batch_after_unknown",
    ] {
        let other: Capability = wire.parse().expect("an existing retry capability");
        assert_ne!(
            other,
            Capability::EmbeddingRetryRetrievalGenerationChanged,
            "{wire} must not be the same decision as a retrieval retry"
        );
    }
}

/// The capability round-trips through the wire name an operator writes in a
/// grant. A name that parses to nothing is a grant that authorizes nothing, and
/// the route would refuse every caller who held it.
#[test]
fn the_retrieval_retry_capability_round_trips_its_wire_name() {
    let capability = Capability::EmbeddingRetryRetrievalGenerationChanged;
    assert_eq!(
        capability.to_string(),
        "embedding.retry_retrieval_generation_changed"
    );
    let parsed: Capability = capability
        .to_string()
        .parse()
        .expect("the wire name must parse back");
    assert_eq!(parsed, capability);
}

/// Reading an attempt is governed, and it is not the retry.
///
/// The separation is the point. An operator has to be able to see that a retry
/// is available -- which attempt, against which generation, with which reason --
/// in order to decide whether to authorize one. If reading required the retry's
/// capability, the only principal who could look would already be the principal
/// who could spend, and the decision would have no one to make it.
#[test]
fn reading_an_attempt_is_governed_and_is_not_the_retry() {
    let descriptor = route_descriptor(&Method::GET, VIEW_PATH);

    assert_eq!(
        descriptor.exposure,
        RouteExposure::Governed,
        "an attempt names memories; it is never public"
    );
    assert_eq!(
        descriptor.capability,
        Capability::ContextRetrieve,
        "reading what a retrieval did is the entitlement that made it"
    );
    assert_ne!(
        descriptor.capability,
        Capability::EmbeddingRetryRetrievalGenerationChanged,
        "seeing that a retry is available must not require being able to spend one"
    );
    assert_eq!(
        descriptor.risk,
        RiskCategory::Low,
        "a read that spends nothing and returns counts is not a critical action"
    );
}

/// A concrete read resolves to its governed decision, and the same path under
/// the retry's verb is a different route entirely.
#[test]
fn a_concrete_attempt_read_resolves_to_its_own_governed_decision() {
    let path = format!("/v1/embedding-jobs/{}/retrieval", uuid::Uuid::now_v7());
    match inventory_lookup(&Method::GET, &path) {
        RouteDecision::Governed(request) => {
            assert_eq!(request.capability, Capability::ContextRetrieve);
        }
        other => panic!("a mounted read must resolve to a governed decision, got {other:?}"),
    }

    // A POST to the read's path is not the retry under another name. If it
    // resolved to anything, a caller could reach a mutation by guessing a
    // read's URL.
    assert_eq!(
        inventory_lookup(&Method::POST, &path),
        RouteDecision::NotInInventory,
        "the read's path has no mutation behind it"
    );
}

/// And the inventory as a whole still agrees with itself.
///
/// Adding a route is the moment this can break: `route_inventory` validates on
/// every read, and a governed route with no capability is a route nobody can be
/// denied.
#[test]
fn the_inventory_still_validates_with_the_retry_in_it() {
    let inventory = route_inventory();
    for path in [RETRY_PATH, VIEW_PATH] {
        assert!(
            inventory
                .iter()
                .any(|descriptor| descriptor.path_pattern == path),
            "{path} must be declared, not merely mounted"
        );
    }
    let public: Vec<&str> = inventory
        .iter()
        .filter(|descriptor| descriptor.exposure == RouteExposure::PublicBounded)
        .map(|descriptor| descriptor.path_pattern)
        .collect();
    assert_eq!(
        public,
        vec!["/health/live", "/health/ready"],
        "adding an embedding route must not widen what is reachable unauthenticated"
    );
}
