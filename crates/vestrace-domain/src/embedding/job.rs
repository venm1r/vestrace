//! The embedding job and transition vocabulary.
//!
//! Every closed set below is quoted from the frozen spec, with the line it came
//! from. The point of declaring them before any table exists is that a later
//! migration cannot quietly invent a seventh barrier state in SQL: adding one
//! here is a compile error at every match site, and adding one only in SQL is
//! caught by the contract test that reads these sets.

/// Spec line 219, quoted: "Its closed kinds are `retrieval_query`, `delivery`,
/// and `rebuild` (backfill is `rebuild` mode)."
///
/// This set was first written with two members, because the later sentence "For
/// `delivery` or `rebuild`, the validated production response must match the
/// job's exact EmbeddingSpaceKey" was read as the closed set. It is not: that
/// sentence names the two kinds whose response is persisted as vectors. A
/// `retrieval_query` job embeds a query and persists no vector, and leaving it
/// out would have made the one kind retrieval actually issues unrepresentable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingJobKind {
    RetrievalQuery,
    Delivery,
    Rebuild,
}

impl EmbeddingJobKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RetrievalQuery => "retrieval_query",
            Self::Delivery => "delivery",
            Self::Rebuild => "rebuild",
        }
    }

    /// The exact set the database CHECK constraint must carry.
    pub const ALL: [Self; 3] = [Self::RetrievalQuery, Self::Delivery, Self::Rebuild];

    /// Line 219: "For `delivery` or `rebuild`, the validated production response
    /// must match the job's exact EmbeddingSpaceKey". Only these two kinds
    /// produce a persisted vector, so only they reach the corpus and its
    /// generation.
    pub const fn produces_persisted_vectors(self) -> bool {
        matches!(self, Self::Delivery | Self::Rebuild)
    }
}

/// What an unrecognised job kind was.
///
/// It carries the offending value so a refusal can name it. Clippy's
/// `should_implement_trait` is why this is `FromStr` rather than an inherent
/// `from_str`: two functions with the same name and different return types on
/// the same type is a trap for the next reader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownEmbeddingJobKind(pub String);

impl std::fmt::Display for UnknownEmbeddingJobKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "embedding job kind must be exactly `retrieval_query`, `delivery` or `rebuild`, \
             got `{}`",
            self.0
        )
    }
}

impl std::error::Error for UnknownEmbeddingJobKind {}

/// Exact, lowercase, and closed. A tolerant parse would let a misspelled column
/// value select a kind the writer did not mean.
impl std::str::FromStr for EmbeddingJobKind {
    type Err = UnknownEmbeddingJobKind;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "retrieval_query" => Ok(Self::RetrievalQuery),
            "delivery" => Ok(Self::Delivery),
            "rebuild" => Ok(Self::Rebuild),
            other => Err(UnknownEmbeddingJobKind(other.to_owned())),
        }
    }
}

/// Spec line 219, quoted: the job "records append-only `Requested -> Running ->
/// Succeeded | FailedDefinite | InconclusiveUnknown | Cancelled` transitions and
/// terminal structural evidence."
///
/// Six states, and only six. An earlier draft of this type carried ten, adding
/// `Waiting`, `Authorized`, `Dispatching` and `ResultPrepared` from line 256's
/// list of the phases new-version planning may find an overlapping predecessor
/// **physical attempt** in. Those are not job states. `Dispatching` is the
/// external effect's lifecycle state, which P03 already owns;
/// `waiting_for_result_keys` is the attempt's visible pre-dispatch phase; and
/// `ResultPrepared` is the immutable `EmbeddingJobResultPrepared` marker. Giving
/// the job its own copy of them would create a second answer to "has the
/// provider been reached", which is the one question this system may not have
/// two answers to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingJobState {
    Requested,
    Running,
    Succeeded,
    FailedDefinite,
    InconclusiveUnknown,
    Cancelled,
}

impl EmbeddingJobState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedDefinite => "failed_definite",
            Self::InconclusiveUnknown => "inconclusive_unknown",
            Self::Cancelled => "cancelled",
        }
    }

    /// The exact set the database CHECK constraint must carry. Declared here so
    /// the two cannot drift: a state added to one and not the other is caught
    /// by the schema contract test rather than by a production insert.
    pub const ALL: [Self; 6] = [
        Self::Requested,
        Self::Running,
        Self::Succeeded,
        Self::FailedDefinite,
        Self::InconclusiveUnknown,
        Self::Cancelled,
    ];

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::FailedDefinite | Self::InconclusiveUnknown | Self::Cancelled
        )
    }

    /// Exactly the arrows line 219 draws, and no others.
    ///
    /// The line reads `Requested -> Running -> Succeeded | FailedDefinite |
    /// InconclusiveUnknown | Cancelled`, so a job terminalizes from `Running`
    /// and from nowhere else. Cancelling a `Requested` job looks reasonable and
    /// is not written there; adding the edge here would be this type inventing
    /// lifecycle rather than recording it. If that edge is needed, it needs a
    /// spec line first.
    pub const fn may_advance_to(self, next: Self) -> bool {
        match (self, next) {
            (Self::Requested, Self::Running) => true,
            (Self::Running, next) => next.is_terminal(),
            _ => false,
        }
    }

    /// Line 219: "Neither workers nor recovery automatically retry a job after
    /// `Dispatching`."
    ///
    /// `InconclusiveUnknown` is terminal and is the only terminal state that
    /// does not say what happened. Line 257 makes that consequential: it is the
    /// sole state from which the authorized duplicate-charge acknowledgement may
    /// create a successor, and no scheduler may advance it.
    pub const fn may_have_a_successor(self) -> bool {
        matches!(self, Self::InconclusiveUnknown)
    }
}

/// Spec line 203, quoted: "Its closed lifecycle is `Planned -> Rebuilding ->
/// ReadyToActivate -> Activated | Stale | Failed`".
///
/// `Stale` and `Failed` are reachable from every nonterminal state: line 207
/// says a source dependency change, expiry, or another corpus mutation "makes
/// the version `Stale`", and that can happen while planning or rebuilding, not
/// only once ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingSpaceTransitionState {
    Planned,
    Rebuilding,
    ReadyToActivate,
    Activated,
    Stale,
    Failed,
}

impl EmbeddingSpaceTransitionState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Activated | Self::Stale | Self::Failed)
    }

    pub const fn may_advance_to(self, next: Self) -> bool {
        match (self, next) {
            (Self::Planned, Self::Rebuilding)
            | (Self::Rebuilding, Self::ReadyToActivate)
            | (Self::ReadyToActivate, Self::Activated) => true,
            // A version can go stale or fail at any point before it activates;
            // it can never leave a terminal state.
            (from, Self::Stale | Self::Failed) => !from.is_terminal(),
            _ => false,
        }
    }
}

/// Spec line 253: the carry header's closed state is
/// `AwaitingAcknowledgement -> SuccessorCreated | NoLongerRequired`.
///
/// Both edges are one-way. Reopening a resolved carry would give one ambiguity
/// head a second direct successor, which line 251 forbids in the same words for
/// both the same-version and the cross-version edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarryHeaderState {
    AwaitingAcknowledgement,
    SuccessorCreated,
    NoLongerRequired,
}

impl CarryHeaderState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::SuccessorCreated | Self::NoLongerRequired)
    }

    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::AwaitingAcknowledgement,
                Self::SuccessorCreated | Self::NoLongerRequired
            )
        )
    }
}

/// Spec line 253: "its immutable per-recipe mapping rows carry
/// `AwaitingAcknowledgement | NoLongerRequired` classification".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarryMappingState {
    AwaitingAcknowledgement,
    NoLongerRequired,
}

impl CarryMappingState {
    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::AwaitingAcknowledgement, Self::NoLongerRequired)
        )
    }
}

/// Spec line 256: "its entire dedicated batch is nondispatchable and
/// completeness-blocking while unrelated/new batches may proceed normally",
/// under the closed lifecycle `AwaitingPredecessorTerminal -> ResolvedToCarry |
/// ResolvedSatisfiedExisting | ResolvedDefinite | NoLongerRequired |
/// Superseded`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarrierState {
    AwaitingPredecessorTerminal,
    ResolvedToCarry,
    ResolvedSatisfiedExisting,
    ResolvedDefinite,
    NoLongerRequired,
    Superseded,
}

impl BarrierState {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::AwaitingPredecessorTerminal)
    }

    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(self, Self::AwaitingPredecessorTerminal) && next.is_terminal()
    }

    /// Line 256: while a barrier is open its whole dedicated batch is
    /// nondispatchable. Stated as a method so no caller has to remember which
    /// state means "still blocking".
    pub const fn blocks_dispatch(self) -> bool {
        matches!(self, Self::AwaitingPredecessorTerminal)
    }
}

/// Spec line 253 classifies "at recipe granularity against the predecessor's
/// fixed ordered wire batch", and line 205 forbids regrouping or reordering
/// after plan creation.
///
/// A recipe ordinal and an input ordinal are both small integers and mean
/// entirely different things; line 205 maps "old recipe/input ordinal -> new
/// recipe/new input ordinal", four values that a single integer type would let
/// a caller transpose silently. They are separate types so that transposition
/// does not compile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransitionRecipeOrdinal(u32);

impl TransitionRecipeOrdinal {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransitionInputOrdinal(u32);

impl TransitionInputOrdinal {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Spec line 203: "The plan also fixes transition version and restricted
/// derivation scope."
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransitionVersion(u64);

impl TransitionVersion {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
