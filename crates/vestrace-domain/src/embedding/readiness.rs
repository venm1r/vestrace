//! Why an embedding channel is not ready, in the names every surface uses.
//!
//! These are wire names rather than an adapter's phrasing: migration 0197
//! already raises `embedding-legacy-adoption-required` as a SQL exception
//! message, and `vector_retriever` already matches on it. Two places agreeing
//! on a string literal is an agreement nothing enforces, which is what this
//! type replaces.
//!
//! The vocabulary is closed. A surface that could invent a seventh reason would
//! give an operator something to read and nothing to do about it, and a client
//! implementing "retry only when the corpus moved" could never know it had seen
//! every case.

use std::fmt;
use std::str::FromStr;

/// A closed reason an embedding channel cannot serve.
///
/// Distinct from `EmbeddingRetrievalDegradation`, which says why one *attempt*
/// produced no result. This says why the channel itself is not ready, which is
/// what Doctor and Models report and what an operator acts on. The two overlap
/// in substance and not in use: a degradation is per-request and transient, a
/// readiness reason is a standing condition of the installation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EmbeddingReadinessReason {
    /// Legacy plaintext vectors have not finished governed adoption, so no
    /// canonical generation can answer.
    LegacyAdoptionRequired,
    /// No process in this deployment has built the index for the current
    /// generation.
    ///
    /// The one member no storage query may claim -- see
    /// [`EmbeddingReadinessReason::is_observable_from_storage`].
    IndexNotLoaded,
    /// The active space has no Ready current generation.
    GenerationNotReady,
    /// A transition owns the space and has not been activated.
    QualificationTransitionNotReady,
    /// A source erasure has been prepared and its projections have not yet been
    /// retired, so the corpus would answer from material that is being removed.
    ErasurePending,
    /// The most recent index build for the current generation failed and
    /// nothing has succeeded since.
    IndexBuildFailed,
}

impl EmbeddingReadinessReason {
    pub const ALL: [Self; 6] = [
        Self::LegacyAdoptionRequired,
        Self::IndexNotLoaded,
        Self::GenerationNotReady,
        Self::QualificationTransitionNotReady,
        Self::ErasurePending,
        Self::IndexBuildFailed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyAdoptionRequired => "embedding-legacy-adoption-required",
            Self::IndexNotLoaded => "embedding-index-not-loaded",
            Self::GenerationNotReady => "embedding-generation-not-ready",
            Self::QualificationTransitionNotReady => "qualification-transition-not-ready",
            Self::ErasurePending => "embedding-erasure-pending",
            Self::IndexBuildFailed => "embedding-index-build-failed",
        }
    }

    /// Whether a database transaction may assert this reason.
    ///
    /// False for exactly one member. An index is built into a process's own
    /// memory and registered there; no row records that it happened, and no row
    /// could, because the process holding it may die between the write and the
    /// read. A projection built from storage that claimed
    /// `embedding-index-not-loaded` would be reporting a guess as a fact, and
    /// an operator would rebuild an index that was already loaded -- or, worse,
    /// a projection claiming the opposite would report readiness the deployment
    /// does not have.
    ///
    /// Only a process that looked in its own registry may say it, which is why
    /// it reaches a caller as a retrieval degradation rather than as a
    /// readiness reason: the attempt that failed to find an index is the one
    /// thing that knows.
    pub const fn is_observable_from_storage(self) -> bool {
        !matches!(self, Self::IndexNotLoaded)
    }
}

impl fmt::Display for EmbeddingReadinessReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A name outside the vocabulary.
///
/// Carries no payload: the only useful thing to say about an unrecognised
/// reason is that it is not one of these, and echoing the text back would
/// invite a caller to handle it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownEmbeddingReadinessReason;

impl fmt::Display for UnknownEmbeddingReadinessReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not an embedding readiness reason")
    }
}

impl std::error::Error for UnknownEmbeddingReadinessReason {}

impl FromStr for EmbeddingReadinessReason {
    type Err = UnknownEmbeddingReadinessReason;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == value)
            .ok_or(UnknownEmbeddingReadinessReason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name round-trips, and no two members share one.
    ///
    /// A collision would make two different conditions indistinguishable in a
    /// report, which is the whole failure this vocabulary exists to prevent.
    #[test]
    fn every_reason_round_trips_through_a_distinct_name() {
        let names: Vec<&str> = EmbeddingReadinessReason::ALL
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        assert_eq!(
            names.iter().collect::<std::collections::HashSet<_>>().len(),
            names.len(),
            "two readiness reasons must never share a name"
        );
        for reason in EmbeddingReadinessReason::ALL {
            assert_eq!(reason.as_str().parse(), Ok(reason));
            assert_eq!(reason.to_string(), reason.as_str());
        }
        assert!(
            "embedding-something-else"
                .parse::<EmbeddingReadinessReason>()
                .is_err()
        );
    }

    /// The names migration 0197 raises are in the vocabulary verbatim.
    ///
    /// The SQL raises these as exception messages and `vector_retriever` matches
    /// on the text. If this type spelled one differently the match would stop
    /// firing silently, and a legacy deployment would report an unclassified
    /// storage error instead of the one condition an operator can fix.
    #[test]
    fn the_names_the_migrations_raise_are_in_the_vocabulary() {
        assert_eq!(
            EmbeddingReadinessReason::LegacyAdoptionRequired.as_str(),
            "embedding-legacy-adoption-required"
        );
    }

    /// Exactly one member is outside what storage can answer.
    #[test]
    fn exactly_one_reason_is_not_observable_from_storage() {
        let unobservable: Vec<EmbeddingReadinessReason> = EmbeddingReadinessReason::ALL
            .into_iter()
            .filter(|reason| !reason.is_observable_from_storage())
            .collect();
        assert_eq!(
            unobservable,
            vec![EmbeddingReadinessReason::IndexNotLoaded],
            "a database transaction cannot see another process's memory, and \
             only this reason depends on that"
        );
    }
}
