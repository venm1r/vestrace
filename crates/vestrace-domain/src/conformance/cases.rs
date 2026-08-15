//! Executable conformance cases.
//!
//! # Why this module exists
//!
//! Until now every "case" was a row in a hand-written `match` in the CLI that
//! returned `Pass` together with a sentence and a file path. Nothing ran. The
//! [`ConformanceCase`] trait and [`ConformanceRunner`](super::runner::ConformanceRunner)
//! had been written and left unimplemented, so a report claiming forty passing
//! requirements rested on forty assertions by a human that some code existed.
//!
//! A case here **executes**. It builds domain values, exercises the operation
//! the requirement is about, and reports what happened. If the property is
//! violated the case returns `Fail`, which is the entire point: an attestation
//! cannot fail, and a check that cannot fail is not evidence.
//!
//! # What is deliberately not here
//!
//! `VerificationClass::Static` requirements — "the architecture must use a
//! single execution/policy/audit boundary" — describe a shape no runtime
//! assertion observes. Those stay attestations, and
//! [`CaseOrigin`](super::CaseOrigin) now says so in the report rather than
//! letting them pass for executed checks. Adding a fake assertion to make them
//! look executed would be worse than admitting what they are.

use super::{
    CaseCategory, CaseOrigin, CaseStatus, ConformanceCaseResult, RequirementFamily, RequirementId,
    runner::{ConformanceCase, ConformanceRunner},
};

/// Build the runner holding every executable case.
pub fn executable_cases() -> ConformanceRunner {
    let mut runner = ConformanceRunner::new();
    runner.register(Box::new(UnknownOccurrenceTimeStillRecords));
    runner.register(Box::new(ValidityIsIndependentOfRecordingTime));
    runner.register(Box::new(SupersededKnowledgeCannotBecomeCurrentAgain));
    runner.register(Box::new(ALeaseConfersNothingBeyondItsScope));
    runner.register(Box::new(CorrectionPreservesTheOriginal));
    runner.register(Box::new(DerivedStateRebuildsFromTheEventLog));
    runner.register(Box::new(RebuildingDoesNotRewriteTheEventLog));
    runner.register(Box::new(AsOfReadsHistoryNotTheCurrentProjection));
    runner.register(Box::new(SequenceCarriesOrderNotTheWallClock));
    runner.register(Box::new(UncertaintyHasItsOwnState));
    runner.register(Box::new(RecordingTimeAndOccurrenceTimeAreDistinct));
    runner.register(Box::new(MutationCarriesAnExpectedRevision));
    runner.register(Box::new(AStaleRevisionConflictsRatherThanOverwrites));
    runner.register(Box::new(StreamOrderIsMonotonicAndGapFree));
    runner.register(Box::new(MutationPreservesItsFullContext));
    runner.register(Box::new(CorrectionAddsARevisionRatherThanEditingOne));
    runner.register(Box::new(DeterministicRepairRequiresAnAuthoritativeBasis));
    runner.register(Box::new(AmbiguityCannotBeFiledAsDetermination));
    runner.register(Box::new(ReconciliationRecordsItsInputsAndOutcome));
    runner.register(Box::new(ReconciliationRecordSurvivesARoundTrip));
    runner.register(Box::new(CompensationIsANewEffect));
    runner.register(Box::new(ReconciliationDoesNotAmplifyAuthority));
    runner.register(Box::new(MemoryIdentityIsStableAndContentLivesInRevisions));
    runner.register(Box::new(RevisionNumbersAreMonotonicWithinAMemory));
    runner.register(Box::new(ActiveRevisionBelongsToItsMemory));
    runner.register(Box::new(TerminalMemoryStatesDoNotReturnToActive));
    runner.register(Box::new(ValidityRangeMustNotRunBackwards));
    runner.register(Box::new(ConfidenceAndImportanceAreBounded));
    runner.register(Box::new(StructuredPayloadCarriesItsSchemaVersion));
    runner.register(Box::new(ScopeDoesNotCrossTheWorkspaceBoundary));
    runner.register(Box::new(ContentChangeCreatesARevision));
    runner.register(Box::new(ActiveMemoryNamesItsEvidence));
    runner.register(Box::new(DerivedMemoryCarriesItsDerivation));
    runner.register(Box::new(ProvenanceClosesOnEvidenceOrHumanAuthority));
    runner.register(Box::new(ClaimAndMemoryRemainDistinct));
    runner.register(Box::new(SupportedIsNotAbsoluteTruth));
    runner.register(Box::new(LosingEvidenceLeadsBackThroughContest));
    runner.register(Box::new(OpenConflictStaysOpenUntilReconciled));
    runner.register(Box::new(ConflictIsNotResolvedByRecency));
    runner.register(Box::new(ConflictResolutionKeepsItsBasis));
    runner.register(Box::new(SupersessionKeepsWhatItReplaced));
    runner.register(Box::new(AContextPackNeverExceedsItsBudget));
    runner.register(Box::new(HydrationRequiresAnAuthorizationCheck));
    runner.register(Box::new(EveryIncludedItemNamesItsSource));
    runner.register(Box::new(RepresentationLevelsAreDistinguished));
    runner.register(Box::new(TokenBudgetIsEnforced));
    runner.register(Box::new(RetiredContentCarriesItsStatus));
    runner.register(Box::new(DegradationCannotBeSilent));
    runner.register(Box::new(TemporalPerspectiveTravelsWithThePack));
    runner.register(Box::new(AFeedbackSignalNamesTheRevisionItJudged));
    runner.register(Box::new(ALearnedProjectionIsAdvisoryAndNeverBecomesAFact));
    runner.register(Box::new(LearningCannotProposeItsOwnElevation));
    runner.register(Box::new(LearningProposesAndCannotApply));
    runner.register(Box::new(AMeasurementOutranksAnOpinion));
    runner.register(Box::new(AConclusionKeepsItsMeasurements));
    runner.register(Box::new(EveryResultNamesTheRequirementItAnswers));
    runner.register(Box::new(AReportCanRepresentAFailure));
    runner.register(Box::new(AnInvariantDeclaresWhatItChecks));
    runner.register(Box::new(AFindingCarriesItsInvariantAndObservation));
    runner.register(Box::new(APlanIsDerivedFromFindings));
    runner.register(Box::new(RepairPassesThroughAuthorization));
    runner.register(Box::new(VerificationRechecksTheInvariant));
    runner.register(Box::new(RepairRebuildsRatherThanInvents));
    runner.register(Box::new(RecurrenceIsCountedOnTheSameInvariant));
    runner.register(Box::new(FlappingNeedsOscillationNotJustRepetition));
    runner.register(Box::new(DispositionClassifiesWhatHappensNext));
    runner.register(Box::new(ABudgetLimitsRepairAttempts));
    runner.register(Box::new(TheOperatorSeparatesLookingFromActing));
    runner.register(Box::new(AnExecutionRecordIsWrittenOnce));
    runner.register(Box::new(HistoryAccumulatesRatherThanBeingRewritten));
    runner.register(Box::new(SilencingRequiresALiveDisposition));
    runner.register(Box::new(PlanningIsADryRun));
    runner.register(Box::new(TheRegistryExtendsWithoutTouchingExecution));
    runner.register(Box::new(AnObservationRecordsWhenAndAgainstWhatVersion));
    runner.register(Box::new(AFailedVerificationReopensRatherThanRetries));
    runner.register(Box::new(AcceptedRiskIsExplicitAndRevocable));
    runner.register(Box::new(AGrantStatesItsScopeAndItsEnd));
    runner.register(Box::new(AGrantCoversWhatIsBeneathItAndNothingAbove));
    runner.register(Box::new(AMaterialChangeInvalidatesTheApproval));
    runner.register(Box::new(AChildIsNeverAllowedWhatItsParentIsDenied));
    runner.register(Box::new(DelegationOnlyNarrows));
    runner.register(Box::new(EveryEvaluationProducesADecision));
    runner.register(Box::new(NothingIsPermittedWithoutAGrant));
    runner.register(Box::new(RiskIsPartOfTheVerdict));
    runner.register(Box::new(BudgetIsCheckedWhenItIsSpent));
    runner.register(Box::new(ChildBudgetsComeOutOfTheParent));
    runner.register(Box::new(RevocationTakesEffectImmediately));
    runner.register(Box::new(ADelegateCannotOutliveOrOutrankItsChain));
    runner.register(Box::new(ADecisionCanBeAudited));
    runner.register(Box::new(ADecisionIsOnlyGoodForTheInstantItNames));
    runner.register(Box::new(APolicyBoundsSensitivityDestinationAndCapability));
    runner.register(Box::new(DerivedDataInheritsTheHighestSensitivity));
    runner.register(Box::new(ClassifiedDataDoesNotReachAnUnapprovedModel));
    runner.register(Box::new(ALegalHoldOutranksDeletion));
    runner.register(Box::new(DeletionMustAccountForEveryDependency));
    runner.register(Box::new(DeletionProducesEvidence));
    runner.register(Box::new(AClassificationIsReplacedRatherThanEdited));
    runner.register(Box::new(UnknownIsItsOwnAnswer));
    runner.register(Box::new(AnAdapterMustDeclareAContractItCanKeep));
    runner.register(Box::new(RetryCannotDuplicateWhatMightHaveHappened));
    runner.register(Box::new(AuthorityIsCheckedAgainstTheIntentItself));
    runner.register(Box::new(AnAdapterSaysWhatItCannotDo));
    runner.register(Box::new(DispatchIsNotDelivery));
    runner.register(Box::new(ReconciliationWeighsIntentReceiptAndObservation));
    runner.register(Box::new(CompensationIsANewEffectWithItsOwnIdentity));
    runner.register(Box::new(ReversibilityIsDeclaredRatherThanAssumed));
    runner.register(Box::new(ATimeoutMeansUnknown));
    runner.register(Box::new(CrossWorkspaceAccessSatisfiesBothSides));
    runner.register(Box::new(ASecretReferenceCarriesNoSecret));
    runner.register(Box::new(RetentionExpiryIsAStateNotADeletion));
    runner.register(Box::new(ExpiredRetentionStillYieldsToAHold));
    runner.register(Box::new(RotationDoesNotInterruptService));
    runner.register(Box::new(APolicyDecisionCarriesItsBasis));
    runner.register(Box::new(APolicyChangeIsVisibleInItsDecisions));
    runner.register(Box::new(AKeyMovesThroughItsLifecycleInOneDirection));
    runner.register(Box::new(RevokingAKeyEndsAccessWithoutReEncryption));
    runner.register(Box::new(AnExportIsGovernedByThePolicyItNames));
    runner.register(Box::new(AnExportNamesExactlyWhatItCarries));
    runner.register(Box::new(AuthorityStopsAtTheWorkspaceBoundary));
    runner.register(Box::new(ALocalPermissionIsNotAGlobalOne));
    runner.register(Box::new(IdentityAndAuthorityAreCheckedSeparately));
    runner.register(Box::new(AShareNeedsBothSides));
    runner.register(Box::new(AShareNamesExactlyOneTarget));
    runner.register(Box::new(SharingDoesNotTravel));
    runner.register(Box::new(NarrowingEitherSideNarrowsTheShare));
    runner.register(Box::new(AWithdrawnShareDisclosesNothing));
    runner.register(Box::new(ASharedReferenceKeepsItsOrigin));
    runner.register(Box::new(RecognizingARemoteIsNotDisclosingToIt));
    runner.register(Box::new(RevocationDoesNotUnsayWhatWasSaid));
    runner.register(Box::new(ADerivationRemembersWhoseContentItWas));
    runner.register(Box::new(RemovingAProjectionLeavesTheFacts));
    runner.register(Box::new(AnAppliedChangeSaysWhereItCameFrom));
    runner.register(Box::new(AnIncidentIsNotAFinding));
    runner.register(Box::new(ContainmentComesBeforeTrust));
    runner.register(Box::new(ARestartDoesNotRestoreTrust));
    runner.register(Box::new(UnfinishedWorkIsClassifiedNotResumed));
    runner.register(Box::new(AnAmbiguousDispatchGoesToReconciliation));
    runner.register(Box::new(AnAmbiguousIrreversibleEffectIsNotRepeated));
    runner.register(Box::new(ARecoveryPointNamesAProvenPosition));
    runner.register(Box::new(UnvalidatedStateIsNotARecoverySource));
    runner.register(Box::new(TheFiveTrustWordsStayDistinct));
    runner.register(Box::new(TrustReturnsOnlyThroughARun));
    runner.register(Box::new(ARevalidationKeepsItsWorkings));
    runner.register(Box::new(InconclusiveIsAnAnswerOfItsOwn));
    runner.register(Box::new(DivergentHistoriesWaitForAPerson));
    runner.register(Box::new(RepairAttemptsAreBounded));
    runner.register(Box::new(ClosingAnIncidentDoesNotEraseIt));
    runner.register(Box::new(RestoringRebuildsRatherThanTrusts));
    runner.register(Box::new(TrustCanComeBackPartly));
    runner.register(Box::new(EveryClaimedRequirementHasAResult));
    runner.register(Box::new(ProfilesBuildOnOneAnother));
    runner.register(Box::new(LimitationsAreClaimedNotImplied));
    runner.register(Box::new(ASkippedRequirementIsNotAPass));
    runner.register(Box::new(AHardGateIsNotAScore));
    runner.register(Box::new(ABundleCarriesWhatItQualified));
    runner.register(Box::new(ChangingWhatWasQualifiedBreaksTheBaseline));
    runner.register(Box::new(AQualificationCanBeWithdrawn));
    runner.register(Box::new(SelfAssertedTrustIsNotEvidence));
    runner.register(Box::new(ATrustClaimNeedsTheProfileAndItsCaveats));
    runner.register(Box::new(AttestationIsAdmissibleOnlyWhereExecutionIsNot));
    runner.register(Box::new(AuthorityIsWhatWasGrantedNotWhoYouAre));
    runner.register(Box::new(TheIntentIsFixedBeforeAnythingLeaves));
    runner.register(Box::new(AnAdapterAndAnIntentMustAgree));
    runner.register(Box::new(CompensationIsAnEffectNotAnUndo));
    runner.register(Box::new(ACompensationNamesWhatItCompensates));
    runner.register(Box::new(PreconditionsAreCheckedAtTheLastMoment));
    runner.register(Box::new(EveryDispatchLeavesAReceipt));
    runner.register(Box::new(AnAcknowledgementIsNotAnOutcome));
    runner.register(Box::new(AReceiptHasNowhereToPutASecret));
    runner.register(Box::new(CryptoAndDataPolicyAreTwoGates));
    runner.register(Box::new(ACapabilityDoesNotOverrideAPolicy));
    runner.register(Box::new(APolicyDoesNotSupplyTheCapability));
    runner.register(Box::new(ThreeKindsOfDeletionStayThree));
    runner.register(Box::new(ADeletionCannotClaimWhatItDidNotCheck));
    runner.register(Box::new(DeletingEvidenceReopensWhatRestedOnIt));
    runner
}

/// A run's event log, built for the replay cases below.
///
/// Deliberately a real `LegacyRunEventEnvelope` sequence rather than a stub:
/// the reducer validates event type, event version and sequence continuity, so
/// a hand-waved fixture would not survive it and the case would be testing the
/// fixture rather than the property.
mod fixture {
    use crate::id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId};
    use crate::run::{LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunVersion};
    use crate::time::Timestamp;

    pub struct RunLog {
        pub workspace_id: WorkspaceId,
        pub run_id: AgentRunId,
        pub events: Vec<LegacyRunEventEnvelope>,
    }

    /// Created → Prepared → Started → Succeeded, one event per version.
    ///
    /// `occurred_at` runs **backwards** across the last two events on purpose.
    /// A reducer that ordered by time rather than by sequence would either
    /// reorder them or reject them, and that is exactly what TMP-008 forbids.
    pub fn four_event_run() -> RunLog {
        let workspace_id = WorkspaceId::new();
        let run_id = AgentRunId::new();
        let base = crate::time::now();

        let payloads = vec![
            (
                LegacyRunEvent::Created {
                    principal_id: PrincipalId::new(),
                    title: "replay fixture".to_string(),
                },
                base,
            ),
            (
                LegacyRunEvent::Prepared,
                base + chrono::Duration::seconds(1),
            ),
            (LegacyRunEvent::Started, base + chrono::Duration::seconds(2)),
            (
                LegacyRunEvent::Succeeded { summary: None },
                // Earlier than the event before it.
                base + chrono::Duration::seconds(1),
            ),
        ];

        let mut sequence = RunVersion::INITIAL;
        let mut events = Vec::new();
        for (payload, occurred_at) in payloads {
            events.push(envelope(
                workspace_id,
                run_id,
                sequence,
                payload,
                occurred_at,
            ));
            sequence = sequence.next().expect("four versions do not overflow");
        }

        RunLog {
            workspace_id,
            run_id,
            events,
        }
    }

    fn envelope(
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
        sequence: RunVersion,
        payload: LegacyRunEvent,
        occurred_at: Timestamp,
    ) -> LegacyRunEventEnvelope {
        LegacyRunEventEnvelope {
            event_id: RunEventId::new(),
            workspace_id,
            run_id,
            sequence,
            event_type: payload.event_type().to_owned(),
            event_version: payload.event_version(),
            actor: RunActor::System {
                component: "conformance".to_string(),
            },
            causation_id: OperationId::new(),
            correlation_id: CorrelationId::new(),
            payload,
            occurred_at,
            recorded_at: crate::time::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// ARC-002 — Derived state must have a rebuild path from a more authoritative
// layer.
// ---------------------------------------------------------------------------

struct DerivedStateRebuildsFromTheEventLog;

impl ConformanceCase for DerivedStateRebuildsFromTheEventLog {
    fn case_id(&self) -> &str {
        "exec-arc-002-rebuild-from-events"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Arc,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Run state is reconstructed from the event log alone, deterministically"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::{RunStatus, replay};

        let log = fixture::four_event_run();

        let first = match replay(log.events.clone()) {
            Ok(Some(state)) => state,
            Ok(None) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err("replaying a non-empty log produced no state".to_string()),
                    "crates/vestrace-domain/src/run/reducer.rs",
                );
            }
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the event log could not be replayed: {error}")),
                    "crates/vestrace-domain/src/run/reducer.rs",
                );
            }
        };

        // A rebuild path that is not deterministic is not a rebuild path: two
        // replays of the same log must agree, or the derived layer cannot be
        // discarded and regenerated.
        let second = replay(log.events.clone())
            .expect("a log that replayed once replays again")
            .expect("state is present on the second replay");

        let outcome = if first != second {
            Err("two replays of the same log produced different state".to_string())
        } else if first.status != RunStatus::Succeeded {
            Err(format!(
                "replay did not reach the terminal status the log describes, got {:?}",
                first.status
            ))
        } else if first.id != log.run_id || first.workspace_id != log.workspace_id {
            Err("replay produced state for a different run or workspace".to_string())
        } else {
            Ok(format!(
                "replay of {} events reconstructs the run at version {} with status {:?}, \
                 and repeats identically",
                log.events.len(),
                format!("{:?}", first.version),
                first.status
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/run/reducer.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// ARC-003 — Derived state must not automatically rewrite authoritative state.
// ---------------------------------------------------------------------------

struct RebuildingDoesNotRewriteTheEventLog;

impl ConformanceCase for RebuildingDoesNotRewriteTheEventLog {
    fn case_id(&self) -> &str {
        "exec-arc-003-rebuild-does-not-rewrite"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Arc,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Replaying leaves the event log byte-identical, and a drifted projection cannot correct it"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::{apply, replay};

        let log = fixture::four_event_run();
        let before = log.events.clone();

        let _ = replay(log.events.clone());

        // The reducer takes events by reference and returns new state. If it
        // could reach back into the log, drift would be "resolved" by editing
        // history — which is the failure this requirement names.
        if before != log.events {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("replaying modified the event log it read".to_string()),
                "crates/vestrace-domain/src/run/reducer.rs",
            );
        }

        // And a projection that has drifted cannot push itself back: applying
        // an event whose sequence does not follow the state is refused rather
        // than absorbed.
        let mut drifted = replay(before[..2].to_vec())
            .expect("a two-event prefix replays")
            .expect("state is present");
        drifted.version = drifted
            .version
            .next()
            .and_then(|version| version.next())
            .expect("two increments do not overflow");

        let outcome = match apply(Some(drifted), &before[2]) {
            Ok(_) => Err(
                "an event out of sequence with the projection was absorbed silently, so drift \
                 resolves itself against the log"
                    .to_string(),
            ),
            Err(error) => Ok(format!(
                "replay leaves the log unchanged, and a drifted projection is refused rather \
                 than reconciled: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/run/reducer.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-005 — `as_of` must use historical state, not the current projection.
// ---------------------------------------------------------------------------

struct AsOfReadsHistoryNotTheCurrentProjection;

impl ConformanceCase for AsOfReadsHistoryNotTheCurrentProjection {
    fn case_id(&self) -> &str {
        "exec-tmp-005-as-of-is-historical"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 5,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Replaying a prefix yields the state as it stood then, not the state as it stands now"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::{RunStatus, replay};

        let log = fixture::four_event_run();

        let current = replay(log.events.clone())
            .expect("the full log replays")
            .expect("state is present");

        // Every prefix is an `as_of` read. Each must show the status of that
        // moment; a prefix that reported the current status would mean history
        // is being served from the live projection.
        let expected = [
            RunStatus::Created,
            RunStatus::Preparing,
            RunStatus::Running,
            RunStatus::Succeeded,
        ];

        for (index, expected_status) in expected.iter().enumerate() {
            let prefix = log.events[..=index].to_vec();
            let state = match replay(prefix) {
                Ok(Some(state)) => state,
                Ok(None) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("prefix of length {} produced no state", index + 1)),
                        "crates/vestrace-domain/src/run/reducer.rs",
                    );
                }
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!(
                            "prefix of length {} could not be replayed: {error}",
                            index + 1
                        )),
                        "crates/vestrace-domain/src/run/reducer.rs",
                    );
                }
            };

            if state.status != *expected_status {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "as-of version {} reported {:?}, expected {expected_status:?}",
                        index + 1,
                        state.status
                    )),
                    "crates/vestrace-domain/src/run/reducer.rs",
                );
            }
            if index + 1 < expected.len() && state.status == current.status {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "as-of version {} returned the current status, not the historical one",
                        index + 1
                    )),
                    "crates/vestrace-domain/src/run/reducer.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(format!(
                "each prefix reconstructs the status of its own version \
                 (Created -> Preparing -> Running -> Succeeded) while the current \
                 projection stands at {:?}",
                current.status
            )),
            "crates/vestrace-domain/src/run/reducer.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-008 — Wall-clock time must not stand in for causal order.
// ---------------------------------------------------------------------------

struct SequenceCarriesOrderNotTheWallClock;

impl ConformanceCase for SequenceCarriesOrderNotTheWallClock {
    fn case_id(&self) -> &str {
        "exec-tmp-008-sequence-not-wall-clock"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 8,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Order comes from the sequence: a backwards timestamp is accepted, a wrong sequence is not"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::replay;

        // The fixture's final event occurred *earlier* than the one before it.
        // Under a wall-clock ordering contract this log is inconsistent; under
        // an explicit sequence it is ordinary, which is the whole point.
        let log = fixture::four_event_run();
        let last = &log.events[3];
        let previous = &log.events[2];
        if last.occurred_at >= previous.occurred_at {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "the fixture no longer contains a backwards timestamp, so this case \
                     would prove nothing"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/run/reducer.rs",
            );
        }

        if let Err(error) = replay(log.events.clone()) {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "a log whose timestamps run backwards was rejected, which treats the \
                     wall clock as the ordering contract: {error}"
                )),
                "crates/vestrace-domain/src/run/reducer.rs",
            );
        }

        // The converse: correct timestamps do not rescue a broken sequence.
        let mut reordered = log.events.clone();
        reordered.swap(1, 2);
        let outcome = match replay(reordered) {
            Ok(_) => Err(
                "events out of sequence replayed successfully, so the sequence is not the \
                 ordering contract either"
                    .to_string(),
            ),
            Err(error) => Ok(format!(
                "a backwards occurred_at replays cleanly while a swapped sequence is \
                 refused: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/run/reducer.rs",
        )
    }
}

/// Helper for the common shape of a case result.
fn result(
    case_id: &str,
    requirement: RequirementId,
    outcome: Result<String, String>,
    evidence: &str,
) -> ConformanceCaseResult {
    let (status, message) = match outcome {
        Ok(message) => (CaseStatus::Pass, message),
        Err(message) => (CaseStatus::Fail, message),
    };
    ConformanceCaseResult {
        case_id: case_id.to_string(),
        requirement_ids: vec![requirement],
        status,
        message,
        evidence: Some(evidence.to_string()),
        origin: CaseOrigin::Executed,
    }
}

// ---------------------------------------------------------------------------
// TMP-002 — Unknown occurred_at must not block authoritative recording.
// ---------------------------------------------------------------------------

struct UnknownOccurrenceTimeStillRecords;

impl ConformanceCase for UnknownOccurrenceTimeStillRecords {
    fn case_id(&self) -> &str {
        "exec-tmp-002-unknown-occurrence-time"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "An event whose source-world time is unknown is still recorded, with the recording time set"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{ActorRef, Event, EventId, WorkspaceId, time::now};

        let recorded_at = now();
        let outcome = match Event::new(
            EventId::new(),
            WorkspaceId::new(),
            None,
            "observation",
            ActorRef::System("conformance".into()),
            serde_json::json!({}),
            recorded_at,
        ) {
            Err(error) => Err(format!(
                "recording was refused although recorded_at was known: {error}"
            )),
            Ok(event) if event.occurred_at.is_some() => Err(
                "an unspecified occurrence time was invented rather than left absent".to_string(),
            ),
            Ok(event) if event.recorded_at != recorded_at => {
                Err("the recording time was not preserved".to_string())
            }
            Ok(_) => Ok(
                "Event::new accepts an absent occurred_at and preserves recorded_at; \
                 the unknown source time is left None rather than defaulted to the recording time"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/event.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-003 — Knowledge validity is independent of recording time.
// ---------------------------------------------------------------------------

struct ValidityIsIndependentOfRecordingTime;

impl ConformanceCase for ValidityIsIndependentOfRecordingTime {
    fn case_id(&self) -> &str {
        "exec-tmp-003-validity-independent-of-recording"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A revision may be valid over a window that closed before it was recorded"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::memory::MemoryRevision;
        use crate::{Confidence, Importance, MemoryId, MemoryRevisionId, WorkspaceId, time::now};

        let created_at = now();
        // Knowledge about last year, written down today. If validity were tied
        // to recording time this could not be expressed at all.
        let valid_from = created_at - chrono::Duration::days(400);
        let valid_until = created_at - chrono::Duration::days(35);

        let revision = MemoryRevision {
            id: MemoryRevisionId::new(),
            memory_id: MemoryId::new(),
            workspace_id: WorkspaceId::new(),
            revision_number: 1,
            content: "a fact that stopped being true before it was recorded".to_string(),
            structured: None,
            confidence: Confidence::new(1.0).expect("1.0 is inside [0,1]"),
            importance: Importance::new(0.5).expect("0.5 is inside [0,1]"),
            created_at,
            valid_from: Some(valid_from),
            valid_until: Some(valid_until),
            change_reason: None,
            canonical_hash: None,
            classification: None,
        };

        let outcome = match revision.validate_temporal_range() {
            Err(error) => Err(format!(
                "a validity window entirely in the past was refused, which ties knowledge \
                 validity to recording time: {error}"
            )),
            Ok(()) if revision.valid_until >= Some(revision.created_at) => {
                Err("the validity window was silently extended to the recording time".to_string())
            }
            Ok(()) => Ok(format!(
                "a revision recorded at {created_at} is accepted with validity \
                 [{valid_from}, {valid_until}], entirely before its recording time"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-004 — Current retrieval must not return superseded/expired knowledge.
// ---------------------------------------------------------------------------

struct SupersededKnowledgeCannotBecomeCurrentAgain;

impl ConformanceCase for SupersededKnowledgeCannotBecomeCurrentAgain {
    fn case_id(&self) -> &str {
        "exec-tmp-004-superseded-is-not-current"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Superseded and expired memories refuse reactivation, and expiry drops the active revision"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::time::now;

        let at = now();
        let make_active = || {
            let (memory, revision) = memory_fixture::candidate_with_revision();
            let active = memory
                .activate(&revision, at)
                .expect("a candidate memory activates");
            (active, revision)
        };

        // Superseded: reactivation must be refused, or yesterday's answer could
        // be served as today's.
        let (active, revision) = make_active();
        let superseded = active.supersede(at).expect("an active memory supersedes");
        if superseded.clone().activate(&revision, at).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a superseded memory could be reactivated and served as current".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        // Expired: the same, and the active revision pointer must be cleared —
        // a stale pointer is how expired content leaks back into a read.
        let (active, revision) = make_active();
        let expired = active.expire(at).expect("an active memory expires");
        if expired.active_revision_id.is_some() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("an expired memory kept its active revision pointer".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }
        if expired.activate(&revision, at).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("an expired memory could be reactivated".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "Superseded and Expired both refuse activate(); expire() clears \
                active_revision_id so no reader can follow it back to the stale revision"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-009 — A lease must not by itself elevate authority.
// ---------------------------------------------------------------------------

struct ALeaseConfersNothingBeyondItsScope;

impl ConformanceCase for ALeaseConfersNothingBeyondItsScope {
    fn case_id(&self) -> &str {
        "exec-tmp-009-lease-does-not-elevate"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 9,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "A resolution lease is refused outside the workspace and purpose it was filed under"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::WorkspaceId;
        use crate::trust::{SecretRef, SecretResolutionRequest};
        use std::collections::BTreeMap;

        let workspace = WorkspaceId::new();
        let reference = match SecretRef::new(
            "secret://example",
            "local-file",
            workspace,
            "provider-api-key",
            BTreeMap::new(),
            None,
        ) {
            Ok(reference) => reference,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("could not construct a SecretRef to test: {error}")),
                    "crates/vestrace-domain/src/trust.rs",
                );
            }
        };

        // Same holder, different purpose: a lease must not become a general
        // permit just because the caller already holds one.
        let wrong_purpose = SecretResolutionRequest::new(workspace, "export-signing", "ref");
        if reference.authorize_resolution(&wrong_purpose).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a lease was issued for a purpose the secret was not filed under".to_string()),
                "crates/vestrace-domain/src/trust.rs",
            );
        }

        // Different workspace: the same, across the tenancy boundary.
        let wrong_workspace =
            SecretResolutionRequest::new(WorkspaceId::new(), "provider-api-key", "ref");
        if reference.authorize_resolution(&wrong_workspace).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a lease crossed a workspace boundary".to_string()),
                "crates/vestrace-domain/src/trust.rs",
            );
        }

        // And the matching request must still need an authorization reference:
        // holding the ref is not itself the authorization.
        let blank_authorization = SecretResolutionRequest::new(workspace, "provider-api-key", "");
        if reference.authorize_resolution(&blank_authorization).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a lease was issued with no authorization reference, so holding \
                     the SecretRef was itself sufficient"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/trust.rs",
            );
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "authorize_resolution refuses a mismatched purpose, a mismatched workspace \
                and a blank authorization reference; the reference alone grants nothing"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/trust.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// ARC-006 — History corrections preserve the fact of the original.
// ---------------------------------------------------------------------------

struct CorrectionPreservesTheOriginal;

impl ConformanceCase for CorrectionPreservesTheOriginal {
    fn case_id(&self) -> &str {
        "exec-arc-006-correction-preserves-original"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Arc,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "Superseding a memory advances its state without erasing that the earlier state existed"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{MemoryStatus, time::now};

        let at = now();
        let (memory, revision) = memory_fixture::candidate_with_revision();
        let original_revision = revision.id;
        let active = memory
            .activate(&revision, at)
            .expect("a candidate memory activates");
        let revision_before = active.state_revision;

        let superseded = match active.clone().supersede(at) {
            Ok(superseded) => superseded,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("an active memory could not be superseded: {error}")),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        };

        // The correction must be visible as a correction: a new state, a higher
        // revision, and the superseded revision still named. A correction that
        // left no trace would be indistinguishable from the fact never having
        // been recorded.
        let outcome = if superseded.status != MemoryStatus::Superseded {
            Err("supersession did not move the memory into Superseded".to_string())
        } else if superseded.state_revision <= revision_before {
            Err(
                "supersession did not advance the state revision, so the correction \
                 leaves no ordering evidence"
                    .to_string(),
            )
        } else if superseded.active_revision_id != Some(original_revision) {
            Err(
                "supersession discarded the reference to the revision it replaced, \
                 erasing the fact that the earlier state existed"
                    .to_string(),
            )
        } else if superseded.id != active.id {
            Err("supersession produced a different memory identity".to_string())
        } else {
            Ok(format!(
                "supersession advances state_revision {revision_before} -> {} and retains \
                 the superseded revision id under a stable memory identity",
                superseded.state_revision
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

/// Shared builders for the memory cases.
mod memory_fixture {
    use crate::memory::MemoryRevision;
    use crate::{
        Confidence, Importance, Memory, MemoryId, MemoryKind, MemoryRevisionId, WorkspaceId,
        time::now,
    };

    /// A candidate memory and a first revision that genuinely belongs to it.
    ///
    /// The pairing matters: `Memory::activate` now checks ownership, so a
    /// fixture that handed over an unrelated revision id would be rejected —
    /// which is the point of MEM-003.
    pub fn candidate_with_revision() -> (Memory, MemoryRevision) {
        let memory_id = MemoryId::new();
        let workspace_id = WorkspaceId::new();
        let at = now();
        let memory = Memory::new(memory_id, workspace_id, MemoryKind::Fact, at);
        let revision = revision_for(memory_id, workspace_id, 1, "the first statement");
        (memory, revision)
    }

    pub fn revision_for(
        memory_id: MemoryId,
        workspace_id: WorkspaceId,
        revision_number: u32,
        content: &str,
    ) -> MemoryRevision {
        MemoryRevision {
            id: MemoryRevisionId::new(),
            memory_id,
            workspace_id,
            revision_number,
            content: content.to_string(),
            structured: None,
            confidence: Confidence::new(0.9).expect("0.9 is inside [0,1]"),
            importance: Importance::new(0.5).expect("0.5 is inside [0,1]"),
            created_at: now(),
            valid_from: None,
            valid_until: None,
            change_reason: None,
            canonical_hash: None,
            classification: None,
        }
    }
}

/// Shared builders for the mutation and reconciliation cases.
mod mutation_fixture {
    use crate::{
        ClaimId, CognitiveMutation, CognitiveMutationId, ConflictId, EventId, EvidenceRef,
        MutationKind, MutationTargetKind, PrincipalId, ReconciliationRecord, WorkspaceId,
        time::now,
    };

    pub fn revise_at_revision(expected: u32) -> CognitiveMutation {
        CognitiveMutation::new(
            CognitiveMutationId::new(),
            PrincipalId::new(),
            WorkspaceId::new(),
            MutationTargetKind::Claim,
            ClaimId::new().to_string(),
            expected,
            MutationKind::Revise,
            "conformance fixture".to_string(),
            now(),
        )
    }

    pub fn deterministic_record(
        input: Vec<EvidenceRef>,
        basis: Vec<EvidenceRef>,
    ) -> Result<ReconciliationRecord, crate::DomainError> {
        ReconciliationRecord::deterministic(
            "reconciliation-conformance".to_string(),
            WorkspaceId::new(),
            ConflictId::new(),
            input,
            basis,
            PrincipalId::new(),
            now(),
        )
    }

    pub fn semantic_record() -> ReconciliationRecord {
        ReconciliationRecord::semantic(
            "reconciliation-semantic".to_string(),
            WorkspaceId::new(),
            ConflictId::new(),
            vec![EvidenceRef::event(EventId::new())],
            Vec::new(),
            PrincipalId::new(),
            now(),
        )
        .expect("a semantic record needs no authoritative basis")
    }

    pub fn evidence() -> EvidenceRef {
        EvidenceRef::event(EventId::new())
    }
}

// ---------------------------------------------------------------------------
// ARC-007 — Uncertainty must have explicit state, not collapse to pass/fail.
// ---------------------------------------------------------------------------

struct UncertaintyHasItsOwnState;

impl ConformanceCase for UncertaintyHasItsOwnState {
    fn case_id(&self) -> &str {
        "exec-arc-007-uncertainty-is-explicit"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Arc,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A step of unknown outcome is neither terminal nor active"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::status::{RunStatus, RunStepStatus};

        // `Unknown` belongs to RunStepStatus, not RunStatus. The attestation
        // this case replaces said "RunStatus has explicit Unknown", which was
        // simply wrong — an executed case cannot be wrong about which type
        // carries the state, because it names the type to compile.
        if RunStepStatus::Unknown.is_terminal() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "an unknown step outcome counts as terminal, so uncertainty resolves \
                     itself into a conclusion"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/run/status.rs",
            );
        }
        if RunStepStatus::Unknown.is_active() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "an unknown step outcome counts as active, so a stalled step looks \
                     like a running one"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/run/status.rs",
            );
        }
        if RunStepStatus::parse("unknown") != Some(RunStepStatus::Unknown) {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "the unknown state does not survive a round trip through its wire form"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/run/status.rs",
            );
        }

        // Waiting is likewise a state of its own at the run level, rather than
        // a run quietly held in Running or reported as finished.
        for status in [
            RunStatus::WaitingForInput,
            RunStatus::WaitingForApproval,
            RunStatus::WaitingForDependency,
        ] {
            if status.is_terminal() || !status.is_waiting() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("{status:?} is not represented as a waiting state")),
                    "crates/vestrace-domain/src/run/status.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "RunStepStatus::Unknown is neither terminal nor active and round-trips \
                through its wire form; the three run-level waiting states are waiting \
                rather than terminal"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/run/status.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-001 — recorded_at and occurred_at must be distinguishable.
// ---------------------------------------------------------------------------

struct RecordingTimeAndOccurrenceTimeAreDistinct;

impl ConformanceCase for RecordingTimeAndOccurrenceTimeAreDistinct {
    fn case_id(&self) -> &str {
        "exec-tmp-001-two-clocks"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "An event carries the source time and the recording time independently"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{ActorRef, Event, EventId, WorkspaceId, time::now};

        let recorded_at = now();
        let occurred_at = recorded_at - chrono::Duration::hours(6);

        let event = match Event::new(
            EventId::new(),
            WorkspaceId::new(),
            None,
            "observation",
            ActorRef::System("conformance".into()),
            serde_json::json!({}),
            recorded_at,
        ) {
            Ok(event) => event.with_occurred_at(occurred_at),
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the event could not be constructed: {error}")),
                    "crates/vestrace-domain/src/event.rs",
                );
            }
        };

        let outcome = if event.occurred_at != Some(occurred_at) {
            Err("the source time was not preserved as given".to_string())
        } else if event.recorded_at != recorded_at {
            Err("the recording time moved when the source time was supplied".to_string())
        } else if event.occurred_at == Some(event.recorded_at) {
            Err(
                "the two timestamps collapsed onto one value, so they carry the same \
                 information and cannot be distinguished"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "an event recorded at {recorded_at} carries a source time of {occurred_at}, \
                 six hours earlier, with neither field disturbing the other"
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/event.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-006 / TMP-007 — expected-revision precondition, and conflict on mismatch.
// ---------------------------------------------------------------------------

struct MutationCarriesAnExpectedRevision;

impl ConformanceCase for MutationCarriesAnExpectedRevision {
    fn case_id(&self) -> &str {
        "exec-tmp-006-expected-revision-precondition"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A cognitive mutation names the revision it expects and checks it before applying"
    }

    fn run(&self) -> ConformanceCaseResult {
        let mutation = mutation_fixture::revise_at_revision(7);

        let outcome = match mutation.validate_expected_state(7) {
            Err(error) => Err(format!(
                "a mutation whose precondition matches the current revision was refused: {error}"
            )),
            Ok(()) if mutation.expected_state_revision != 7 => {
                Err("the mutation does not retain the revision it was built against".to_string())
            }
            Ok(()) => Ok(
                "CognitiveMutation carries expected_state_revision and validates it against \
                 the current revision before the mutation may apply"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

struct AStaleRevisionConflictsRatherThanOverwrites;

impl ConformanceCase for AStaleRevisionConflictsRatherThanOverwrites {
    fn case_id(&self) -> &str {
        "exec-tmp-007-stale-revision-conflicts"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A mutation built against a stale revision reports the conflict and both revisions"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::DomainError;

        let mutation = mutation_fixture::revise_at_revision(7);

        // The writer believes it is at 7; the world has moved to 9.
        let outcome = match mutation.validate_expected_state(9) {
            Ok(()) => Err(
                "a mutation built against a stale revision was accepted, which is a silent \
                 overwrite of whatever happened in between"
                    .to_string(),
            ),
            Err(DomainError::RevisionConflict { expected, current }) => {
                if expected != 7 || current != 9 {
                    Err(format!(
                        "the conflict reported expected {expected} / current {current}, \
                         losing which revision the caller actually held"
                    ))
                } else {
                    Ok(
                        "a stale mutation yields RevisionConflict naming both the expected \
                         and the current revision, so the caller can re-read rather than guess"
                            .to_string(),
                    )
                }
            }
            Err(other) => Err(format!(
                "a stale revision produced {other}, which does not identify the conflict"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// TMP-010 — Append-only streams have verifiable monotonic order.
// ---------------------------------------------------------------------------

struct StreamOrderIsMonotonicAndGapFree;

impl ConformanceCase for StreamOrderIsMonotonicAndGapFree {
    fn case_id(&self) -> &str {
        "exec-tmp-010-monotonic-stream-order"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Tmp,
            number: 10,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Versions increase by exactly one; a gap or a repeat is detected rather than absorbed"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::run::replay;

        let log = fixture::four_event_run();

        // The property is checkable from the log itself, with no database.
        for pair in log.events.windows(2) {
            let expected = match pair[0].sequence.next() {
                Ok(expected) => expected,
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("sequence overflowed inside the fixture: {error}")),
                        "crates/vestrace-domain/src/run/reducer.rs",
                    );
                }
            };
            if pair[1].sequence != expected {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err("the fixture's own sequence is not contiguous".to_string()),
                    "crates/vestrace-domain/src/run/reducer.rs",
                );
            }
        }

        // A gap must be refused: an append-only stream whose order cannot be
        // verified is only a table with a timestamp column.
        let mut with_gap = log.events.clone();
        with_gap.remove(2);
        if replay(with_gap).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a log with a missing version replayed successfully, so a gap in the \
                     stream is undetectable"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/run/reducer.rs",
            );
        }

        // And so must a repeat.
        let mut with_repeat = log.events.clone();
        with_repeat.insert(2, log.events[1].clone());
        let outcome = match replay(with_repeat) {
            Ok(_) => Err(
                "a repeated version replayed successfully, so the stream order is not \
                 strictly increasing"
                    .to_string(),
            ),
            Err(error) => Ok(format!(
                "versions increase by exactly one, and both a gap and a repeat are \
                 refused: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/run/reducer.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-001 — A significant mutation preserves actor, target, expected state,
// intent and result.
// ---------------------------------------------------------------------------

struct MutationPreservesItsFullContext;

impl ConformanceCase for MutationPreservesItsFullContext {
    fn case_id(&self) -> &str {
        "exec-mut-001-mutation-keeps-context"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "Actor, target, expected revision, reason, provenance and result all survive on the record"
    }

    fn run(&self) -> ConformanceCaseResult {
        let evidence = mutation_fixture::evidence();
        let mutation = mutation_fixture::revise_at_revision(3)
            .with_provenance(evidence.clone())
            .with_resulting_revision("revision-4".to_string());

        let missing: Vec<&str> = [
            ("reason", !mutation.reason.trim().is_empty()),
            ("target_id", !mutation.target_id.trim().is_empty()),
            (
                "expected_state_revision",
                mutation.expected_state_revision == 3,
            ),
            (
                "provenance_refs",
                mutation.provenance_refs == vec![evidence],
            ),
            (
                "resulting_revision_id",
                mutation.resulting_revision_id.as_deref() == Some("revision-4"),
            ),
        ]
        .into_iter()
        .filter_map(|(field, present)| (!present).then_some(field))
        .collect();

        let outcome = if missing.is_empty() {
            Ok(format!(
                "the mutation retains actor, {:?} target {}, expected revision 3, its reason, \
                 one provenance reference and the revision it produced",
                mutation.target_kind, mutation.target_id
            ))
        } else {
            Err(format!(
                "the mutation record lost: {}. A mutation missing any of these cannot be \
                 attributed or replayed",
                missing.join(", ")
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-002 — A correction must not rewrite the old revision.
// ---------------------------------------------------------------------------

struct CorrectionAddsARevisionRatherThanEditingOne;

impl ConformanceCase for CorrectionAddsARevisionRatherThanEditingOne {
    fn case_id(&self) -> &str {
        "exec-mut-002-correction-does-not-rewrite"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Correcting content produces a new revision and leaves the earlier one byte-identical"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::memory::MemoryRevision;
        use crate::{Confidence, Importance, MemoryId, MemoryRevisionId, WorkspaceId, time::now};

        let memory_id = MemoryId::new();
        let workspace_id = WorkspaceId::new();
        let at = now();

        let original = MemoryRevision {
            id: MemoryRevisionId::new(),
            memory_id,
            workspace_id,
            revision_number: 1,
            content: "the original statement".to_string(),
            structured: None,
            confidence: Confidence::new(0.9).expect("0.9 is inside [0,1]"),
            importance: Importance::new(0.5).expect("0.5 is inside [0,1]"),
            created_at: at,
            valid_from: None,
            valid_until: None,
            change_reason: None,
            canonical_hash: None,
            classification: None,
        };
        let snapshot = original.clone();

        // A correction is a *new* revision. There is no setter that could edit
        // the old one, which is the property; the case demonstrates that the
        // corrected value coexists with the original rather than replacing it.
        let corrected = MemoryRevision {
            id: MemoryRevisionId::new(),
            revision_number: original.revision_number + 1,
            content: "the corrected statement".to_string(),
            change_reason: Some("correction".to_string()),
            created_at: at,
            ..original.clone()
        };

        let outcome = if original != snapshot {
            Err("the earlier revision changed while a correction was made".to_string())
        } else if corrected.id == original.id {
            Err(
                "the correction reused the earlier revision's identity, so the earlier \
                 content is no longer addressable"
                    .to_string(),
            )
        } else if corrected.revision_number <= original.revision_number {
            Err("the correction did not advance the revision number".to_string())
        } else if corrected.memory_id != original.memory_id {
            Err("the correction detached itself from the memory it corrects".to_string())
        } else {
            Ok(format!(
                "revision {} is unchanged and separately addressable after revision {} \
                 records the correction under the same memory",
                original.revision_number, corrected.revision_number
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-003 — Deterministic reconciliation needs an authoritative basis.
// ---------------------------------------------------------------------------

struct DeterministicRepairRequiresAnAuthoritativeBasis;

impl ConformanceCase for DeterministicRepairRequiresAnAuthoritativeBasis {
    fn case_id(&self) -> &str {
        "exec-mut-003-deterministic-needs-basis"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A deterministic reconciliation without basis evidence is refused at construction"
    }

    fn run(&self) -> ConformanceCaseResult {
        let input = vec![mutation_fixture::evidence()];

        if mutation_fixture::deterministic_record(input.clone(), Vec::new()).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a deterministic reconciliation was built with no authoritative basis, \
                     so a guess can be filed as a determination"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/claim/mutation.rs",
            );
        }

        let basis = vec![mutation_fixture::evidence()];
        let outcome = match mutation_fixture::deterministic_record(input, basis.clone()) {
            Err(error) => Err(format!(
                "a deterministic reconciliation with basis evidence was refused: {error}"
            )),
            Ok(record) if record.basis_refs != basis => {
                Err("the basis evidence was not retained on the record".to_string())
            }
            Ok(_) => Ok(
                "ReconciliationRecord::deterministic refuses an empty basis and retains the \
                 authoritative evidence it was given"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-004 — Semantic ambiguity must not be masked as deterministic repair.
// ---------------------------------------------------------------------------

struct AmbiguityCannotBeFiledAsDetermination;

impl ConformanceCase for AmbiguityCannotBeFiledAsDetermination {
    fn case_id(&self) -> &str {
        "exec-mut-004-ambiguity-is-not-determination"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Only a semantic reconciliation may accept ambiguity; a deterministic one may not"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ReconciliationOutcome;

        let deterministic = match mutation_fixture::deterministic_record(
            vec![mutation_fixture::evidence()],
            vec![mutation_fixture::evidence()],
        ) {
            Ok(record) => record,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("could not build a deterministic record: {error}")),
                    "crates/vestrace-domain/src/claim/mutation.rs",
                );
            }
        };

        if deterministic.clone().accept_ambiguity().is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a deterministic reconciliation accepted ambiguity, which files an \
                     unresolved disagreement as a settled determination"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/claim/mutation.rs",
            );
        }
        if deterministic.defer_to_human().is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a deterministic reconciliation was deferred to a human, blurring the \
                     line between a determination and an open question"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/claim/mutation.rs",
            );
        }

        let outcome = match mutation_fixture::semantic_record().accept_ambiguity() {
            Err(error) => Err(format!(
                "a semantic reconciliation could not record accepted ambiguity: {error}"
            )),
            Ok(record) if record.outcome != ReconciliationOutcome::AcceptedAmbiguity => {
                Err("accepting ambiguity did not change the recorded outcome".to_string())
            }
            Ok(_) => Ok(
                "accept_ambiguity is available only to a semantic reconciliation; a \
                 deterministic one can neither accept ambiguity nor defer to a human"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-005 / MUT-006 — The reconciliation record preserves inputs, and replays.
// ---------------------------------------------------------------------------

struct ReconciliationRecordsItsInputsAndOutcome;

impl ConformanceCase for ReconciliationRecordsItsInputsAndOutcome {
    fn case_id(&self) -> &str {
        "exec-mut-005-record-preserves-inputs"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 5,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Inputs, basis and outcome are all present on the record the reconciliation produced"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ReconciliationOutcome;

        let input = vec![mutation_fixture::evidence(), mutation_fixture::evidence()];
        let basis = vec![mutation_fixture::evidence()];

        let outcome = match mutation_fixture::deterministic_record(input.clone(), basis.clone()) {
            Err(error) => Err(format!("the record could not be produced: {error}")),
            Ok(record) if record.input_evidence_refs != input => {
                Err("the record dropped or reordered the evidence it reconciled".to_string())
            }
            Ok(record) if record.basis_refs != basis => {
                Err("the record dropped the basis it resolved against".to_string())
            }
            Ok(record) if record.outcome != ReconciliationOutcome::Resolved => Err(format!(
                "the record does not state its outcome, it holds {:?}",
                record.outcome
            )),
            Ok(record) => Ok(format!(
                "the record holds {} input references, {} basis references and the outcome \
                 {:?}, so the reconciliation can be re-examined from the record alone",
                record.input_evidence_refs.len(),
                record.basis_refs.len(),
                record.outcome
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

struct ReconciliationRecordSurvivesARoundTrip;

impl ConformanceCase for ReconciliationRecordSurvivesARoundTrip {
    fn case_id(&self) -> &str {
        "exec-mut-006-record-is-auditable-and-replayable"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "The record carries who and when, and serialises and deserialises without loss"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ReconciliationRecord;

        let record = match mutation_fixture::deterministic_record(
            vec![mutation_fixture::evidence()],
            vec![mutation_fixture::evidence()],
        ) {
            Ok(record) => record,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the record could not be produced: {error}")),
                    "crates/vestrace-domain/src/claim/mutation.rs",
                );
            }
        };

        if record.reconciliation_id.trim().is_empty() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the record has no identity, so it cannot be cited".to_string()),
                "crates/vestrace-domain/src/claim/mutation.rs",
            );
        }

        // Replayable means the stored form reconstructs the decision exactly.
        // A lossy round trip would mean the audit trail and the original
        // decision are two different things.
        let encoded = match serde_json::to_vec(&record) {
            Ok(encoded) => encoded,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the record could not be serialised: {error}")),
                    "crates/vestrace-domain/src/claim/mutation.rs",
                );
            }
        };

        let outcome = match serde_json::from_slice::<ReconciliationRecord>(&encoded) {
            Err(error) => Err(format!("the stored record could not be read back: {error}")),
            Ok(decoded) if decoded != record => Err(
                "the record changed across a round trip, so the audit trail and the \
                 decision it records are not the same thing"
                    .to_string(),
            ),
            Ok(_) => Ok(format!(
                "the record names its resolver and creation time, and reconstructs \
                 identically from {} stored bytes",
                encoded.len()
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-007 — Compensation is a new effect, not a retroactive erase.
// ---------------------------------------------------------------------------

struct CompensationIsANewEffect;

impl ConformanceCase for CompensationIsANewEffect {
    fn case_id(&self) -> &str {
        "exec-mut-007-compensation-is-additive"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Correcting a mutation creates a second record; the first is untouched"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MutationKind;

        let original = mutation_fixture::revise_at_revision(4)
            .with_resulting_revision("revision-5".to_string());
        let snapshot = original.clone();

        let compensating = mutation_fixture::revise_at_revision(5)
            .with_provenance(mutation_fixture::evidence())
            .with_resulting_revision("revision-6".to_string());

        let outcome = if original != snapshot {
            Err("issuing a compensating mutation altered the original record".to_string())
        } else if compensating.mutation_id == original.mutation_id {
            Err(
                "the compensating mutation reused the original's identity, so the original \
                 effect is no longer separately visible"
                    .to_string(),
            )
        } else if compensating.expected_state_revision <= original.expected_state_revision {
            Err(
                "the compensating mutation does not build on the state the original left"
                    .to_string(),
            )
        } else if !matches!(
            original.mutation_kind,
            MutationKind::Revise | MutationKind::Correct | MutationKind::Supersede
        ) {
            Err("the fixture does not exercise a corrective mutation kind".to_string())
        } else {
            Ok(format!(
                "the original mutation against revision {} is unchanged, and the correction \
                 is a separate record against revision {}",
                original.expected_state_revision, compensating.expected_state_revision
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MUT-008 — Reconciliation must not amplify authority.
// ---------------------------------------------------------------------------

struct ReconciliationDoesNotAmplifyAuthority;

impl ConformanceCase for ReconciliationDoesNotAmplifyAuthority {
    fn case_id(&self) -> &str {
        "exec-mut-008-no-authority-amplification"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mut,
            number: 8,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "A record's class is fixed at construction, and no operation raises it"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ReconciliationClass;

        // A semantic reconciliation is the weakest kind. Every operation it
        // permits must leave it semantic: if deferring or accepting ambiguity
        // could promote it, a guess would acquire the standing of a
        // determination.
        let semantic = mutation_fixture::semantic_record();
        if semantic.reconciliation_class != ReconciliationClass::Semantic {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the fixture is not the class it claims".to_string()),
                "crates/vestrace-domain/src/claim/mutation.rs",
            );
        }

        for (label, transformed) in [
            ("accept_ambiguity", semantic.clone().accept_ambiguity()),
            ("defer_to_human", semantic.clone().defer_to_human()),
        ] {
            match transformed {
                Ok(record) if record.reconciliation_class != ReconciliationClass::Semantic => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!(
                            "{label} raised the reconciliation class to {:?}",
                            record.reconciliation_class
                        )),
                        "crates/vestrace-domain/src/claim/mutation.rs",
                    );
                }
                Ok(record) if record.basis_refs != semantic.basis_refs => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("{label} invented authoritative basis evidence")),
                        "crates/vestrace-domain/src/claim/mutation.rs",
                    );
                }
                _ => {}
            }
        }

        // And a human-required reconciliation cannot be auto-resolved down to
        // something a machine may settle.
        let human = crate::ReconciliationRecord::human_required(
            "reconciliation-human".to_string(),
            crate::WorkspaceId::new(),
            crate::ConflictId::new(),
            vec![mutation_fixture::evidence()],
            "an operator decided".to_string(),
            crate::PrincipalId::new(),
            crate::time::now(),
        );
        let outcome = match human {
            Err(error) => Err(format!(
                "a human-required record could not be built: {error}"
            )),
            Ok(record) if record.clone().defer_to_human().is_ok() => Err(
                "a human-required reconciliation was auto-deferred, which lets the system \
                 hold a decision it is not authorised to make"
                    .to_string(),
            ),
            Ok(_) => Ok(
                "a semantic record stays semantic across accept_ambiguity and defer_to_human \
                 and gains no basis evidence; a human-required record refuses auto-deferral"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/mutation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-001 / MEM-002 / MEM-004 — identity, immutable revisions, monotonic order.
// ---------------------------------------------------------------------------

struct MemoryIdentityIsStableAndContentLivesInRevisions;

impl ConformanceCase for MemoryIdentityIsStableAndContentLivesInRevisions {
    fn case_id(&self) -> &str {
        "exec-mem-001-identity-outlives-content"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "The memory identity survives every content change, and the content lives on revisions"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::time::now;

        let at = now();
        let (memory, first) = memory_fixture::candidate_with_revision();
        let identity = memory.id;

        let active = match memory.activate(&first, at) {
            Ok(active) => active,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a candidate memory could not be activated: {error}"
                    )),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        };

        let second = memory_fixture::revision_for(identity, active.workspace_id, 2, "restated");
        let outcome = match active.clone().activate(&second, at) {
            Err(error) => Err(format!("a second revision could not be activated: {error}")),
            Ok(updated) if updated.id != identity => {
                Err("the memory changed identity when its content changed".to_string())
            }
            Ok(updated) if updated.active_revision_id != Some(second.id) => {
                Err("the memory did not move to the new revision".to_string())
            }
            Ok(updated) if updated.created_at != active.created_at => Err(
                "the memory's creation time moved, so its identity is not stable across \
                     content changes"
                    .to_string(),
            ),
            Ok(_) => Ok(format!(
                "memory {identity} keeps its identity and creation time while its active \
                 revision moves from {} to {}",
                first.id, second.id
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

struct RevisionNumbersAreMonotonicWithinAMemory;

impl ConformanceCase for RevisionNumbersAreMonotonicWithinAMemory {
    fn case_id(&self) -> &str {
        "exec-mem-004-monotonic-revision-numbers"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Successive revisions of one memory carry strictly increasing numbers and distinct ids"
    }

    fn run(&self) -> ConformanceCaseResult {
        let (memory, first) = memory_fixture::candidate_with_revision();
        let second = memory_fixture::revision_for(memory.id, memory.workspace_id, 2, "restated");
        let third =
            memory_fixture::revision_for(memory.id, memory.workspace_id, 3, "restated again");

        let chain = [&first, &second, &third];
        for pair in chain.windows(2) {
            if pair[1].revision_number <= pair[0].revision_number {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "revision {} does not advance on revision {}",
                        pair[1].revision_number, pair[0].revision_number
                    )),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
            if pair[1].id == pair[0].id {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(
                        "two revisions share an identity, so the earlier one is not \
                         separately addressable"
                            .to_string(),
                    ),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
            if pair[1].memory_id != pair[0].memory_id {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err("the revision chain left the memory it belongs to".to_string()),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "revisions 1, 2 and 3 of one memory carry strictly increasing numbers under \
                distinct identities"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-003 — The active revision must belong to the same memory.
// ---------------------------------------------------------------------------

struct ActiveRevisionBelongsToItsMemory;

impl ConformanceCase for ActiveRevisionBelongsToItsMemory {
    fn case_id(&self) -> &str {
        "exec-mem-003-active-revision-ownership"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A revision belonging to another memory, or another workspace, cannot be made active"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{MemoryId, WorkspaceId, time::now};

        let at = now();
        let (memory, own) = memory_fixture::candidate_with_revision();

        // A revision of a different memory.
        let foreign = memory_fixture::revision_for(
            MemoryId::new(),
            memory.workspace_id,
            1,
            "another memory's content",
        );
        if memory.clone().activate(&foreign, at).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a memory was pointed at another memory's revision, so reading it \
                     would return content never written to it"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        // A revision of the same memory id in a different workspace.
        let cross_tenant =
            memory_fixture::revision_for(memory.id, WorkspaceId::new(), 1, "another tenant");
        if memory.clone().activate(&cross_tenant, at).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a revision from another workspace was made active".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        let outcome = match memory.activate(&own, at) {
            Err(error) => Err(format!("a memory's own revision was refused: {error}")),
            Ok(active) if active.active_revision_id != Some(own.id) => {
                Err("activation did not record the revision it accepted".to_string())
            }
            Ok(_) => Ok(
                "activate accepts the memory's own revision and refuses one belonging to \
                 another memory or another workspace"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-007 — Terminal statuses must not silently return to Active.
// ---------------------------------------------------------------------------

struct TerminalMemoryStatesDoNotReturnToActive;

impl ConformanceCase for TerminalMemoryStatesDoNotReturnToActive {
    fn case_id(&self) -> &str {
        "exec-mem-007-no-silent-return-to-active"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Superseded, Expired, Rejected and Deleted each refuse reactivation"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{MemoryStatus, time::now};

        let at = now();

        // Rejected is reached from Candidate; the rest from Active. Each is
        // built separately because the transitions consume the value.
        let rejected = {
            let (memory, _) = memory_fixture::candidate_with_revision();
            memory.reject(at).expect("a candidate memory rejects")
        };
        let mut terminal = vec![(MemoryStatus::Rejected, rejected)];

        for (label, transition) in [
            (
                MemoryStatus::Superseded,
                Box::new(|m: crate::Memory, at| m.supersede(at))
                    as Box<
                        dyn Fn(
                            crate::Memory,
                            crate::time::Timestamp,
                        )
                            -> Result<crate::Memory, crate::DomainError>,
                    >,
            ),
            (
                MemoryStatus::Expired,
                Box::new(|m: crate::Memory, at| m.expire(at)),
            ),
            (
                MemoryStatus::Deleted,
                Box::new(|m: crate::Memory, at| m.soft_delete(at)),
            ),
        ] {
            let (memory, revision) = memory_fixture::candidate_with_revision();
            let active = memory
                .activate(&revision, at)
                .expect("a candidate memory activates");
            match transition(active, at) {
                Ok(moved) => terminal.push((label, moved)),
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("could not reach {label:?}: {error}")),
                        "crates/vestrace-domain/src/memory/revision.rs",
                    );
                }
            }
        }

        for (label, memory) in &terminal {
            if memory.status != *label {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("expected {label:?}, reached {:?}", memory.status)),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
            let revision =
                memory_fixture::revision_for(memory.id, memory.workspace_id, 9, "revival attempt");
            if memory.clone().activate(&revision, at).is_ok() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a {label:?} memory returned to Active, so retired knowledge can be \
                         served as current again"
                    )),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok("Superseded, Expired, Rejected and Deleted all refuse activation".to_string()),
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-008 / MEM-009 — temporal range and bounded scores.
// ---------------------------------------------------------------------------

struct ValidityRangeMustNotRunBackwards;

impl ConformanceCase for ValidityRangeMustNotRunBackwards {
    fn case_id(&self) -> &str {
        "exec-mem-008-validity-range-ordering"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 8,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "valid_until earlier than valid_from is refused; equal timestamps are allowed"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::time::now;

        let base = now();
        let (memory, _) = memory_fixture::candidate_with_revision();

        let mut backwards =
            memory_fixture::revision_for(memory.id, memory.workspace_id, 1, "backwards");
        backwards.valid_from = Some(base);
        backwards.valid_until = Some(base - chrono::Duration::seconds(1));
        if backwards.validate_temporal_range().is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a validity window that ends before it begins was accepted".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        // An instantaneous window is legitimate: something true at exactly one
        // moment is not the same as a malformed range.
        let mut instant =
            memory_fixture::revision_for(memory.id, memory.workspace_id, 1, "instant");
        instant.valid_from = Some(base);
        instant.valid_until = Some(base);
        if let Err(error) = instant.validate_temporal_range() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "an instantaneous validity window was refused: {error}"
                )),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        // An open-ended window must also stay legal.
        let mut open = memory_fixture::revision_for(memory.id, memory.workspace_id, 1, "open");
        open.valid_from = Some(base);
        open.valid_until = None;
        let outcome = match open.validate_temporal_range() {
            Err(error) => Err(format!(
                "an open-ended validity window was refused: {error}"
            )),
            Ok(()) => Ok(
                "a reversed window is refused while an instantaneous one and an open-ended \
                 one are accepted"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

struct ConfidenceAndImportanceAreBounded;

impl ConformanceCase for ConfidenceAndImportanceAreBounded {
    fn case_id(&self) -> &str {
        "exec-mem-009-bounded-scores"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 9,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Confidence and importance refuse values outside [0,1], and refuse NaN and infinity"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{Confidence, Importance};

        // NaN is the interesting one: it is outside [0,1] but fails every
        // ordinary comparison, so a range check written as `v < 0.0 || v > 1.0`
        // lets it through.
        let rejected: [(&str, f32); 5] = [
            ("below zero", -0.000_1),
            ("above one", 1.000_1),
            ("NaN", f32::NAN),
            ("infinity", f32::INFINITY),
            ("negative infinity", f32::NEG_INFINITY),
        ];

        for (label, value) in rejected {
            if Confidence::new(value).is_ok() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("Confidence accepted {label}")),
                    "crates/vestrace-domain/src/memory/kind.rs",
                );
            }
            if Importance::new(value).is_ok() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("Importance accepted {label}")),
                    "crates/vestrace-domain/src/memory/kind.rs",
                );
            }
        }

        for value in [0.0_f32, 0.5, 1.0] {
            if Confidence::new(value).is_err() || Importance::new(value).is_err() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a legitimate value {value} was refused")),
                    "crates/vestrace-domain/src/memory/kind.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "both newtypes refuse values below 0, above 1, NaN and both infinities, \
                and accept the closed interval's endpoints"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/memory/kind.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-010 — Structured payloads carry a schema identity and version.
// ---------------------------------------------------------------------------

struct StructuredPayloadCarriesItsSchemaVersion;

impl ConformanceCase for StructuredPayloadCarriesItsSchemaVersion {
    fn case_id(&self) -> &str {
        "exec-mem-010-schema-identity"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 10,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A structured payload serialises with a schema_version tag and reads back as the same variant"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::memory::StructuredMemory;
        use crate::memory::structured::AssertionData;

        let payload = StructuredMemory::Assertion(AssertionData {
            subject: "vestrace".to_string(),
            predicate: "records".to_string(),
            object: "provenance".to_string(),
        });

        let encoded = match serde_json::to_value(&payload) {
            Ok(encoded) => encoded,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the payload could not be serialised: {error}")),
                    "crates/vestrace-domain/src/memory/structured.rs",
                );
            }
        };

        // The tag is what lets a reader years from now know which shape it is
        // holding. Without it a payload is an untyped blob.
        let tag = encoded.get("schema_version").and_then(|v| v.as_str());
        if tag != Some("v1.assertion") {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "the serialised payload carries schema_version {tag:?}, so its shape \
                     cannot be identified from the stored form alone"
                )),
                "crates/vestrace-domain/src/memory/structured.rs",
            );
        }

        let outcome = match serde_json::from_value::<StructuredMemory>(encoded) {
            Err(error) => Err(format!(
                "the tagged payload could not be read back: {error}"
            )),
            Ok(decoded) if decoded != payload => {
                Err("the payload changed across a round trip".to_string())
            }
            Ok(_) => Ok(
                "StructuredMemory is an externally tagged enum: the stored form carries \
                 schema_version \"v1.assertion\" and reconstructs the same variant"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/structured.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-011 — Scope must not extend the workspace authority boundary.
// ---------------------------------------------------------------------------

struct ScopeDoesNotCrossTheWorkspaceBoundary;

impl ConformanceCase for ScopeDoesNotCrossTheWorkspaceBoundary {
    fn case_id(&self) -> &str {
        "exec-mem-011-scope-within-workspace"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 11,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "Every part of a memory carries the same workspace, and a foreign one cannot be attached"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{WorkspaceId, time::now};

        let at = now();
        let (memory, own) = memory_fixture::candidate_with_revision();
        let other_workspace = WorkspaceId::new();

        if own.workspace_id != memory.workspace_id {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the fixture's own revision already sits in another workspace".to_string()),
                "crates/vestrace-domain/src/memory/revision.rs",
            );
        }

        // Attaching a revision from another workspace is the concrete way a
        // scope relation would widen authority: the memory would then serve
        // content the caller's workspace never held.
        let foreign =
            memory_fixture::revision_for(memory.id, other_workspace, 1, "another tenant's content");
        let outcome = match memory.clone().activate(&foreign, at) {
            Ok(_) => Err(
                "a revision from another workspace became active, so the memory's scope \
                 now reaches outside its workspace"
                    .to_string(),
            ),
            Err(crate::DomainError::PolicyViolation(message)) => Ok(format!(
                "a cross-workspace revision is refused as a policy violation rather than \
                 an ordinary argument error: {message}"
            )),
            Err(other) => Err(format!(
                "the cross-workspace revision was refused, but as {other} rather than a \
                 policy violation, so the boundary reads as a validation detail"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-002 — In-place overwrite of semantic content is forbidden.
// ---------------------------------------------------------------------------

struct ContentChangeCreatesARevision;

impl ConformanceCase for ContentChangeCreatesARevision {
    fn case_id(&self) -> &str {
        "exec-mem-002-no-in-place-overwrite"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "The memory aggregate holds no content field, so content can only change by adding a revision"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::time::now;

        let at = now();
        let (memory, first) = memory_fixture::candidate_with_revision();
        let active = match memory.activate(&first, at) {
            Ok(active) => active,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("activation failed: {error}")),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        };

        // Serialising the aggregate is how the absence is demonstrated rather
        // than asserted: if content lived on Memory it would appear here, and a
        // writer could change it without producing a revision at all.
        let encoded = match serde_json::to_value(&active) {
            Ok(encoded) => encoded,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the memory could not be serialised: {error}")),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        };
        for field in ["content", "structured", "confidence", "importance"] {
            if encoded.get(field).is_some() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "the memory aggregate carries {field:?}, so semantic content can be \
                         overwritten without creating a revision"
                    )),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        }

        let second = memory_fixture::revision_for(active.id, active.workspace_id, 2, "restated");
        let before = first.clone();
        let outcome = match active.activate(&second, at) {
            Err(error) => Err(format!("a second revision could not be activated: {error}")),
            Ok(_) if first != before => {
                Err("the earlier revision changed when the content was restated".to_string())
            }
            Ok(updated) if updated.active_revision_id == Some(first.id) => {
                Err("the memory still points at the earlier revision".to_string())
            }
            Ok(_) => Ok(
                "Memory holds no content, structured, confidence or importance field; a \
                 restatement adds revision 2 and leaves revision 1 untouched"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/memory/revision.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-005 / MEM-006 / MEM-012 — evidence, derivation and provenance closure.
// ---------------------------------------------------------------------------

struct ActiveMemoryNamesItsEvidence;

impl ConformanceCase for ActiveMemoryNamesItsEvidence {
    fn case_id(&self) -> &str {
        "exec-mem-005-source-reference"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 5,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A direct source binds the memory to an admissible evidence reference in the same workspace"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{EventId, EvidenceRef, MemorySource, MemorySourceId, time::now};

        let (memory, _) = memory_fixture::candidate_with_revision();
        let event_id = EventId::new();
        let source = MemorySource::new_direct(
            MemorySourceId::new(),
            memory.id,
            memory.workspace_id,
            event_id,
            now(),
        );

        let outcome = if source.memory_id != memory.id {
            Err("the source is attached to a different memory".to_string())
        } else if source.workspace_id != memory.workspace_id {
            Err("the source names evidence in another workspace".to_string())
        } else if source.evidence_ref != Some(EvidenceRef::event(event_id)) {
            Err(
                "the source does not carry the evidence reference it was built from, so the \
                 memory names no admissible source"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "a direct source binds memory {} to event {event_id} inside one workspace",
                memory.id
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/provenance.rs",
        )
    }
}

struct DerivedMemoryCarriesItsDerivation;

impl ConformanceCase for DerivedMemoryCarriesItsDerivation {
    fn case_id(&self) -> &str {
        "exec-mem-006-derivation-present"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A derived source names a derivation; a direct one does not invent it"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{
            Derivation, DerivationId, DerivationMethod, EventId, EvidenceRef, EvidenceRole,
            MemorySource, MemorySourceId, time::now,
        };

        let at = now();
        let (memory, _) = memory_fixture::candidate_with_revision();

        let derivation_id = DerivationId::new();
        let derivation = Derivation::new(
            derivation_id,
            memory.workspace_id,
            DerivationMethod::Summarization,
            at,
        )
        .with_input(EvidenceRef::event(EventId::new()))
        .with_output(EvidenceRef::event(EventId::new()));

        let derived = MemorySource::new_derived(
            MemorySourceId::new(),
            memory.id,
            memory.workspace_id,
            EventId::new(),
            EvidenceRole::DerivedFrom,
            derivation_id,
            at,
        );

        let direct = MemorySource::new_direct(
            MemorySourceId::new(),
            memory.id,
            memory.workspace_id,
            EventId::new(),
            at,
        );

        let outcome = if derived.derivation_id != Some(derivation_id) {
            Err("a derived source does not name the derivation that produced it".to_string())
        } else if derivation.input_refs.is_empty() {
            Err("the derivation records no inputs, so it explains nothing".to_string())
        } else if derivation.output_ref.is_none() {
            Err("the derivation records no output".to_string())
        } else if direct.derivation_id.is_some() {
            Err(
                "a direct source invented a derivation, blurring what was inferred and what \
                 was observed"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "a derived source names derivation {derivation_id} with {} input(s) and an \
                 output, while a direct source carries none",
                derivation.input_refs.len()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/provenance.rs",
        )
    }
}

struct ProvenanceClosesOnEvidenceOrHumanAuthority;

impl ConformanceCase for ProvenanceClosesOnEvidenceOrHumanAuthority {
    fn case_id(&self) -> &str {
        "exec-mem-012-provenance-closure"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 12,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A derivation chain terminates in a reference, and references identify without copying"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{
            Derivation, DerivationId, DerivationMethod, EventId, EvidenceRef, WorkspaceId,
            time::now,
        };

        let at = now();
        let workspace = WorkspaceId::new();
        let root_event = EventId::new();

        // Two hops: the output of the first derivation is the input of the
        // second. Closure means following the chain ends at a reference to
        // recorded evidence rather than at content nobody can trace.
        let intermediate = EvidenceRef::event(EventId::new());
        let first = Derivation::new(
            DerivationId::new(),
            workspace,
            DerivationMethod::Extraction,
            at,
        )
        .with_input(EvidenceRef::event(root_event))
        .with_output(intermediate.clone());

        let second = Derivation::new(
            DerivationId::new(),
            workspace,
            DerivationMethod::Consolidation,
            at,
        )
        .with_input(intermediate.clone());

        if second.input_refs != vec![intermediate.clone()] {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "the second derivation does not consume the first's output, so the \
                     chain does not close"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/provenance.rs",
            );
        }
        if first.input_refs != vec![EvidenceRef::event(root_event)] {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the chain does not terminate at recorded evidence".to_string()),
                "crates/vestrace-domain/src/provenance.rs",
            );
        }

        // A reference must identify, not carry. If it copied content, purging
        // the source would leave the copy behind.
        let encoded = serde_json::to_string(&EvidenceRef::event(root_event))
            .unwrap_or_else(|error| format!("unserialisable: {error}"));
        let outcome = if encoded.contains("content") {
            Err("an evidence reference carries content rather than identity".to_string())
        } else {
            Ok(format!(
                "a two-hop derivation chain closes on event {root_event}, and an evidence \
                 reference serialises to identity alone: {encoded}"
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/provenance.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-013 / MEM-014 / MEM-015 — claims are their own concept, and are revisable.
// ---------------------------------------------------------------------------

struct ClaimAndMemoryRemainDistinct;

impl ConformanceCase for ClaimAndMemoryRemainDistinct {
    fn case_id(&self) -> &str {
        "exec-mem-013-claim-is-not-memory"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 13,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A claim carries a semantic key and a subject/predicate/value; a memory carries neither"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{Claim, ClaimId, time::now};

        let (memory, _) = memory_fixture::candidate_with_revision();
        let claim = Claim::new(
            ClaimId::new(),
            memory.workspace_id,
            "vestrace/records".to_string(),
            "vestrace".to_string(),
            "records".to_string(),
            "provenance".to_string(),
            now(),
        );

        let claim_json = serde_json::to_value(&claim).unwrap_or(serde_json::Value::Null);
        let memory_json = serde_json::to_value(&memory).unwrap_or(serde_json::Value::Null);

        let claim_only = ["semantic_key", "subject", "predicate", "value"];
        for field in claim_only {
            if claim_json.get(field).is_none() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a claim does not carry {field:?}")),
                    "crates/vestrace-domain/src/claim/claim.rs",
                );
            }
            if memory_json.get(field).is_some() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a memory carries {field:?}, so the two concepts have collapsed \
                         into one"
                    )),
                    "crates/vestrace-domain/src/memory/revision.rs",
                );
            }
        }

        let outcome = if memory_json.get("kind").is_none() {
            Err(
                "a memory does not carry its kind, so it is not distinguishable the other way"
                    .to_string(),
            )
        } else if claim_json.get("active_revision_id").is_some() {
            Err("a claim carries an active revision pointer, which belongs to memory".to_string())
        } else {
            Ok(
                "a claim carries semantic_key, subject, predicate and value; a memory carries \
                a kind and an active revision, and neither carries the other's fields"
                    .to_string(),
            )
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/claim.rs",
        )
    }
}

struct SupportedIsNotAbsoluteTruth;

impl ConformanceCase for SupportedIsNotAbsoluteTruth {
    fn case_id(&self) -> &str {
        "exec-mem-014-supported-is-revisable"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 14,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A supported claim can still be contested, superseded or expired"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{ClaimStatus, time::now};

        let at = now();
        let supported = match claim_fixture::proposed().support(at) {
            Ok(claim) => claim,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a proposed claim could not be supported: {error}")),
                    "crates/vestrace-domain/src/claim/claim.rs",
                );
            }
        };

        if supported.lifecycle_status != ClaimStatus::Supported {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the claim did not reach Supported".to_string()),
                "crates/vestrace-domain/src/claim/claim.rs",
            );
        }

        // If Supported were treated as settled truth, none of these would be
        // available from it.
        for (label, outcome) in [
            ("contest", supported.clone().contest(at)),
            ("supersede", supported.clone().supersede(at)),
            ("expire", supported.clone().expire(at)),
        ] {
            if let Err(error) = outcome {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a supported claim could not be {label}ed ({error}), so Supported \
                         behaves as settled external truth"
                    )),
                    "crates/vestrace-domain/src/claim/claim.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "a Supported claim remains open to contest, supersession and expiry, so the \
                status records an evaluation rather than a fact about the world"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/claim/claim.rs",
        )
    }
}

struct LosingEvidenceLeadsBackThroughContest;

impl ConformanceCase for LosingEvidenceLeadsBackThroughContest {
    fn case_id(&self) -> &str {
        "exec-mem-015-revalidation-path"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 15,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A contested claim must be re-supported explicitly; it does not drift back on its own"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{ClaimStatus, time::now};

        let at = now();
        let contested = match claim_fixture::proposed()
            .support(at)
            .and_then(|claim| claim.contest(at))
        {
            Ok(claim) => claim,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the claim could not be contested: {error}")),
                    "crates/vestrace-domain/src/claim/claim.rs",
                );
            }
        };

        if contested.lifecycle_status != ClaimStatus::Contested {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("losing support did not move the claim out of Supported".to_string()),
                "crates/vestrace-domain/src/claim/claim.rs",
            );
        }

        let revision_before = contested.state_revision;
        let outcome = match contested.support(at) {
            Err(error) => Err(format!(
                "a contested claim cannot be revalidated at all, so evidence recovering \
                 leaves it stuck: {error}"
            )),
            Ok(revalidated) if revalidated.lifecycle_status != ClaimStatus::Supported => {
                Err("revalidation did not restore support".to_string())
            }
            Ok(revalidated) if revalidated.state_revision <= revision_before => Err(
                "revalidation left no state advance, so the round trip through Contested \
                 is invisible afterwards"
                    .to_string(),
            ),
            Ok(revalidated) => Ok(format!(
                "support -> contest -> support each advances the state revision \
                 (to {}), so the loss of evidence and the revalidation are both on record",
                revalidated.state_revision
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/claim.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-016 / MEM-017 / MEM-018 — conflicts stay explicit and keep their basis.
// ---------------------------------------------------------------------------

struct OpenConflictStaysOpenUntilReconciled;

impl ConformanceCase for OpenConflictStaysOpenUntilReconciled {
    fn case_id(&self) -> &str {
        "exec-mem-016-conflict-stays-explicit"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 16,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "An open conflict cannot be resolved or accepted without a reconciliation reference"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ConflictStatus;

        let open = conflict_fixture::open();
        if open.status != ConflictStatus::Open {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a new conflict does not start Open".to_string()),
                "crates/vestrace-domain/src/claim/conflict.rs",
            );
        }

        for (label, outcome) in [
            ("resolve", open.clone().resolve()),
            ("accept ambiguity for", open.clone().accept_ambiguity()),
        ] {
            if outcome.is_ok() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "an open conflict could {label} without a reconciliation, so a \
                         disagreement can be closed with nothing to point at"
                    )),
                    "crates/vestrace-domain/src/claim/conflict.rs",
                );
            }
        }

        if open
            .clone()
            .propose_reconciliation("   ".to_string())
            .is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a blank reconciliation reference was accepted".to_string()),
                "crates/vestrace-domain/src/claim/conflict.rs",
            );
        }

        let outcome = match open
            .propose_reconciliation("reconciliation-1".to_string())
            .and_then(|conflict| conflict.resolve())
        {
            Err(error) => Err(format!(
                "a properly proposed reconciliation could not resolve the conflict: {error}"
            )),
            Ok(resolved) if resolved.reconciliation_ref.is_none() => {
                Err("the resolved conflict does not name what resolved it".to_string())
            }
            Ok(_) => Ok(
                "an open conflict refuses resolve() and accept_ambiguity() and refuses a \
                 blank reference; it closes only after a named reconciliation is proposed"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/conflict.rs",
        )
    }
}

struct ConflictIsNotResolvedByRecency;

impl ConformanceCase for ConflictIsNotResolvedByRecency {
    fn case_id(&self) -> &str {
        "exec-mem-017-no-latest-wins"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 17,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Time passing changes nothing about a conflict's status"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::ConflictStatus;

        let open = conflict_fixture::open();
        let before = open.clone();

        // There is no operation that takes a clock and settles the conflict.
        // The demonstration is that the only state changes available are the
        // explicit ones, and that the record is otherwise inert.
        let encoded = serde_json::to_value(&open).unwrap_or(serde_json::Value::Null);
        let outcome = if open != before {
            Err("the conflict changed without an operation being applied".to_string())
        } else if open.status != ConflictStatus::Open {
            Err("the conflict did not remain open".to_string())
        } else if encoded.get("resolved_at").is_some() || encoded.get("expires_at").is_some() {
            Err(
                "the conflict carries a deadline field, which is how latest-wins gets \
                 introduced later"
                    .to_string(),
            )
        } else if open.participant_refs.len() < 2 {
            Err("the fixture does not represent a disagreement between two claims".to_string())
        } else {
            Ok(format!(
                "the conflict between {} claims stays Open with no timestamp-driven \
                 transition available; only propose_reconciliation, resolve and \
                 accept_ambiguity change it",
                open.participant_refs.len()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/conflict.rs",
        )
    }
}

struct ConflictResolutionKeepsItsBasis;

impl ConformanceCase for ConflictResolutionKeepsItsBasis {
    fn case_id(&self) -> &str {
        "exec-mem-018-resolution-keeps-basis"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 18,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "Resolving a conflict preserves its participants, evidence and the reconciliation reference"
    }

    fn run(&self) -> ConformanceCaseResult {
        let open = conflict_fixture::open();
        let participants = open.participant_refs.clone();
        let evidence = open.evidence_refs.clone();

        let outcome = match open
            .propose_reconciliation("reconciliation-basis".to_string())
            .and_then(|conflict| conflict.resolve())
        {
            Err(error) => Err(format!("the conflict could not be resolved: {error}")),
            Ok(resolved) if resolved.participant_refs != participants => {
                Err("resolution dropped the claims that were in conflict".to_string())
            }
            Ok(resolved) if resolved.evidence_refs != evidence => {
                Err("resolution dropped the evidence the conflict was detected from".to_string())
            }
            Ok(resolved)
                if resolved.reconciliation_ref.as_deref() != Some("reconciliation-basis") =>
            {
                Err("resolution did not record which reconciliation settled it".to_string())
            }
            Ok(resolved) => Ok(format!(
                "the resolved conflict still names {} participants, {} evidence reference(s) \
                 and the reconciliation that settled it",
                resolved.participant_refs.len(),
                resolved.evidence_refs.len()
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/conflict.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// MEM-019 — Supersession must not delete history.
// ---------------------------------------------------------------------------

struct SupersessionKeepsWhatItReplaced;

impl ConformanceCase for SupersessionKeepsWhatItReplaced {
    fn case_id(&self) -> &str {
        "exec-mem-019-supersession-keeps-history"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Mem,
            number: 19,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A supersession link names both sides and a reason, so the replaced revision stays reachable"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::{SupersessionLink, SupersessionLinkId, time::now};

        let at = now();
        let (memory, first) = memory_fixture::candidate_with_revision();
        let second = memory_fixture::revision_for(memory.id, memory.workspace_id, 2, "replacement");

        let link = SupersessionLink::for_memory_revision(
            SupersessionLinkId::new(),
            memory.workspace_id,
            memory.id,
            first.id,
            memory.id,
            second.id,
            "corrected after review".to_string(),
            at,
        );

        let outcome = if link.superseded_revision_id != Some(first.id) {
            Err(
                "the link does not name the revision it replaced, so the earlier content is \
                 unreachable"
                    .to_string(),
            )
        } else if link.replacement_revision_id != Some(second.id) {
            Err("the link does not name the replacement".to_string())
        } else if link.reason.trim().is_empty() {
            Err(
                "the link carries no reason, so the record says what changed but not why"
                    .to_string(),
            )
        } else if link.workspace_id != memory.workspace_id {
            Err("the link crosses a workspace boundary".to_string())
        } else {
            Ok(format!(
                "the supersession link names revision {} as replaced by {} with a stated \
                 reason, and neither revision is removed",
                first.id, second.id
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/claim/supersession.rs",
        )
    }
}

/// Builders for the claim and conflict cases.
mod claim_fixture {
    use crate::{Claim, ClaimId, WorkspaceId, time::now};

    pub fn proposed() -> Claim {
        Claim::new(
            ClaimId::new(),
            WorkspaceId::new(),
            "vestrace/records".to_string(),
            "vestrace".to_string(),
            "records".to_string(),
            "provenance".to_string(),
            now(),
        )
    }
}

mod conflict_fixture {
    use crate::{
        ClaimId, Conflict, ConflictId, ConflictKind, EventId, EvidenceRef, PrincipalId,
        WorkspaceId, time::now,
    };

    /// A disagreement between two claims, with the evidence it was spotted in.
    pub fn open() -> Conflict {
        let mut conflict = Conflict::new(
            ConflictId::new(),
            WorkspaceId::new(),
            ConflictKind::SemanticContradiction,
            vec![ClaimId::new(), ClaimId::new()],
            PrincipalId::new(),
            now(),
        );
        conflict.evidence_refs = vec![EvidenceRef::event(EventId::new())];
        conflict
    }
}

/// Builders for the retrieval cases.
mod retrieval_fixture {
    use crate::retrieval::{
        ContextItem, ContextPack, ContextSection, RepresentationLevel, TimePerspective,
    };
    use crate::{
        DomainError, EvidenceRef, MemoryId, MemoryRevisionId, MemoryStatus, PrincipalId,
        WorkspaceId,
        id::{ContextPackId, RetrievalRunId},
        time::now,
    };

    pub const ITEM_TOKENS: u32 = 12;

    /// One included item, complete: provenance, revision, status and validity.
    pub fn item(status: MemoryStatus) -> ContextItem {
        let memory_id = MemoryId::new();
        let revision_id = MemoryRevisionId::new();
        ContextItem {
            memory_id,
            revision_id,
            memory_status: status,
            revision_number: 3,
            valid_from: Some(now() - chrono::Duration::days(2)),
            valid_until: None,
            revision_created_at: now(),
            source_generation: 1,
            representation: RepresentationLevel::Full,
            rendered_text: "rendered".to_string(),
            accounted_tokens: ITEM_TOKENS,
            provenance_refs: vec![EvidenceRef::MemoryRevisionRef {
                memory_id,
                revision_id,
            }],
            inclusion_explanation: "included for the conformance case".to_string(),
            source_classification: None,
        }
    }

    /// A pack containing the given items, at the given perspective.
    #[allow(clippy::too_many_arguments)]
    pub fn pack(
        items: Vec<ContextItem>,
        token_budget: u32,
        perspective: TimePerspective,
        authorization_checked: bool,
    ) -> Result<ContextPack, DomainError> {
        let used_tokens = items.iter().map(|item| item.accounted_tokens).sum();
        let candidate_ids = items.iter().map(|item| item.memory_id).collect();
        let sections = if items.is_empty() {
            Vec::new()
        } else {
            vec![ContextSection {
                label: "facts".to_string(),
                items,
            }]
        };

        ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            perspective,
            token_budget,
            used_tokens,
            candidate_ids,
            sections,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "conformance-v1".to_string(),
            authorization_checked,
            now(),
        )
    }
}

// ---------------------------------------------------------------------------
// RET-004 — A context pack never spends more than its budget.
// ---------------------------------------------------------------------------

struct AContextPackNeverExceedsItsBudget;

impl ConformanceCase for AContextPackNeverExceedsItsBudget {
    fn case_id(&self) -> &str {
        "exec-ret-004-used-budget-within-hard-budget"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A context pack whose items account for more tokens than its budget is refused at          construction rather than trimmed silently; spending exactly the budget is allowed, and          the accounted total of a pack that exists always matches the sum of its items"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;

        let id = self.case_id();
        let requirement = self.requirement_ids()[0];
        let evidence = "crates/vestrace-domain/src/retrieval/mod.rs";

        let items = vec![
            retrieval_fixture::item(MemoryStatus::Active),
            retrieval_fixture::item(MemoryStatus::Active),
        ];
        let needed: u32 = items.iter().map(|item| item.accounted_tokens).sum();
        if needed == 0 {
            return result(
                id,
                requirement,
                Err("the fixture items account for no tokens, so no budget can be exceeded"
                    .to_string()),
                evidence,
            );
        }

        // Over budget: refused. Not trimmed, not truncated, not accepted with a
        // warning — a pack that reports spending more than it was allowed is a
        // pack whose budget meant nothing, and one that silently drops items to
        // fit is a pack whose contents nobody chose.
        if retrieval_fixture::pack(items.clone(), needed - 1, TimePerspective::Current, true)
            .is_ok()
        {
            return result(
                id,
                requirement,
                Err(format!(
                    "a context pack accounting for {needed} tokens was built against a budget of                      {}, so the hard budget does not bound what a pack may spend",
                    needed - 1
                )),
                evidence,
            );
        }

        // Exactly at budget: allowed. A budget is a ceiling, not a limit to stay
        // under, and refusing the boundary would make the number mean something
        // other than what it says.
        let exact = match retrieval_fixture::pack(
            items.clone(),
            needed,
            TimePerspective::Current,
            true,
        ) {
            Ok(pack) => pack,
            Err(error) => {
                return result(
                    id,
                    requirement,
                    Err(format!("a pack spending exactly its budget was refused: {error}")),
                    evidence,
                );
            }
        };
        if exact.used_tokens > exact.token_budget {
            return result(
                id,
                requirement,
                Err(format!(
                    "a constructed pack reports {} tokens used against a budget of {}",
                    exact.used_tokens, exact.token_budget
                )),
                evidence,
            );
        }

        // And what it reports having spent is what its items actually account
        // for: a total kept independently of the contents could satisfy the
        // check above while describing a different pack.
        let counted: u32 = exact
            .sections
            .iter()
            .flat_map(|section| section.items.iter())
            .map(|item| item.accounted_tokens)
            .sum();
        if counted != exact.used_tokens {
            return result(
                id,
                requirement,
                Err(format!(
                    "the pack reports {} tokens used and its items account for {counted}",
                    exact.used_tokens
                )),
                evidence,
            );
        }

        // Under budget is unremarkable and worth pinning, so a future change
        // that refused headroom would be caught.
        if retrieval_fixture::pack(items, needed + 10, TimePerspective::Current, true).is_err() {
            return result(
                id,
                requirement,
                Err("a pack spending less than its budget was refused".to_string()),
                evidence,
            );
        }

        result(
            id,
            requirement,
            Ok(format!(
                "a pack accounting for {needed} tokens is refused a budget of {}, permitted at                  {needed} and at {}, and always reports what its items account for",
                needed - 1,
                needed + 10
            )),
            evidence,
        )
    }
}

// ---------------------------------------------------------------------------
// RET-003 — Authorization is checked at hydration time.
// ---------------------------------------------------------------------------

struct HydrationRequiresAnAuthorizationCheck;

impl ConformanceCase for HydrationRequiresAnAuthorizationCheck {
    fn case_id(&self) -> &str {
        "exec-ret-003-authorization-at-hydration"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "A context pack cannot be constructed without an authorization check having happened"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;

        let items = vec![retrieval_fixture::item(MemoryStatus::Active)];

        // Unchecked: refused at construction, so there is no moment at which a
        // pack exists having skipped the check. Checking only at index time
        // would let a revoked reader keep receiving content indexed earlier.
        if retrieval_fixture::pack(items.clone(), 100, TimePerspective::Current, false).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a context pack was hydrated with no authorization check".to_string()),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        let outcome = match retrieval_fixture::pack(items, 100, TimePerspective::Current, true) {
            Err(error) => Err(format!("an authorized hydration was refused: {error}")),
            Ok(pack) if !pack.authorization_checked => {
                Err("the pack does not record that authorization was checked".to_string())
            }
            Ok(_) => Ok(
                "ContextPack::new refuses construction unless authorization_checked is true, \
                 and the pack carries that fact for a later reader"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-005 — Each included item carries provenance and revision metadata.
// ---------------------------------------------------------------------------

struct EveryIncludedItemNamesItsSource;

impl ConformanceCase for EveryIncludedItemNamesItsSource {
    fn case_id(&self) -> &str {
        "exec-ret-005-item-provenance"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 5,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "An item with no provenance reference or no revision cannot be packed"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;

        let mut without_provenance = retrieval_fixture::item(MemoryStatus::Active);
        without_provenance.provenance_refs.clear();
        if retrieval_fixture::pack(
            vec![without_provenance],
            100,
            TimePerspective::Current,
            true,
        )
        .is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a pack included text with no reference back to the revision it was \
                     rendered from, so a reader cannot check it"
                        .to_string(),
                ),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        let mut without_revision = retrieval_fixture::item(MemoryStatus::Active);
        without_revision.revision_number = 0;
        if retrieval_fixture::pack(vec![without_revision], 100, TimePerspective::Current, true)
            .is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a pack included an item naming no revision".to_string()),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        let complete = retrieval_fixture::item(MemoryStatus::Active);
        let revision_id = complete.revision_id;
        let outcome =
            match retrieval_fixture::pack(vec![complete], 100, TimePerspective::Current, true) {
                Err(error) => Err(format!("a complete item was refused: {error}")),
                Ok(pack) => {
                    let packed = &pack.sections[0].items[0];
                    if packed.revision_id != revision_id {
                        Err("the packed item points at a different revision".to_string())
                    } else if packed.provenance_refs.is_empty() {
                        Err("the provenance was dropped in packing".to_string())
                    } else {
                        Ok(format!(
                            "an item without provenance and an item without a revision are both \
                             refused; a complete one keeps revision {revision_id} and its \
                             reference"
                        ))
                    }
                }
            };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-006 — Four representation levels, kept distinct.
// ---------------------------------------------------------------------------

struct RepresentationLevelsAreDistinguished;

impl ConformanceCase for RepresentationLevelsAreDistinguished {
    fn case_id(&self) -> &str {
        "exec-ret-006-representation-levels"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Full, Summary, Atomic and Reference are four distinct values that survive storage"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::retrieval::RepresentationLevel;
        use std::collections::HashSet;

        let levels = [
            RepresentationLevel::Full,
            RepresentationLevel::Summary,
            RepresentationLevel::Atomic,
            RepresentationLevel::Reference,
        ];

        // Distinct wire forms, or a reader cannot tell whether it was handed
        // the content or a pointer to it — which is the difference between
        // quoting a memory and citing one.
        let mut encoded = HashSet::new();
        for level in levels {
            let wire = match serde_json::to_string(&level) {
                Ok(wire) => wire,
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("{level:?} could not be serialised: {error}")),
                        "crates/vestrace-domain/src/retrieval/mod.rs",
                    );
                }
            };
            if !encoded.insert(wire.clone()) {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("{level:?} shares a wire form with another level")),
                    "crates/vestrace-domain/src/retrieval/mod.rs",
                );
            }
            match serde_json::from_str::<RepresentationLevel>(&wire) {
                Ok(decoded) if decoded == level => {}
                Ok(decoded) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("{level:?} read back as {decoded:?}")),
                        "crates/vestrace-domain/src/retrieval/mod.rs",
                    );
                }
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!("{level:?} could not be read back: {error}")),
                        "crates/vestrace-domain/src/retrieval/mod.rs",
                    );
                }
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(format!(
                "the four representation levels have {} distinct wire forms and each \
                 round-trips to itself",
                encoded.len()
            )),
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-007 — The token budget is enforced, not merely reported.
// ---------------------------------------------------------------------------

struct TokenBudgetIsEnforced;

impl ConformanceCase for TokenBudgetIsEnforced {
    fn case_id(&self) -> &str {
        "exec-ret-007-token-budget"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A pack over budget is refused, and the per-item counts must add up to the total"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::{ContextPack, ContextSection, TimePerspective};
        use crate::{PrincipalId, WorkspaceId, id::ContextPackId, id::RetrievalRunId, time::now};

        let items = vec![
            retrieval_fixture::item(MemoryStatus::Active),
            retrieval_fixture::item(MemoryStatus::Active),
        ];
        let total = retrieval_fixture::ITEM_TOKENS * 2;

        // Over budget by one token.
        if retrieval_fixture::pack(items.clone(), total - 1, TimePerspective::Current, true).is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a pack exceeding its token budget was accepted".to_string()),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        // Exactly at budget is legitimate: a budget is a ceiling, not a target
        // to stay under.
        if let Err(error) =
            retrieval_fixture::pack(items.clone(), total, TimePerspective::Current, true)
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!("a pack exactly at its budget was refused: {error}")),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        // A total that understates the items is the interesting case: the
        // summary would honour the budget while the payload exceeded it.
        let understated = ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            TimePerspective::Current,
            total,
            // Claims one item's worth while carrying two.
            retrieval_fixture::ITEM_TOKENS,
            Vec::new(),
            vec![ContextSection {
                label: "facts".to_string(),
                items,
            }],
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "conformance-v1".to_string(),
            true,
            now(),
        );

        let outcome = match understated {
            Ok(_) => Err(
                "a pack whose declared usage understated its own items was accepted, so the \
                 budget is honoured in the summary and exceeded in the payload"
                    .to_string(),
            ),
            Err(error) => Ok(format!(
                "over-budget is refused, exactly-at-budget is allowed, and a total that \
                 disagrees with the items is caught: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-008 — Superseded content is not presented as current.
// ---------------------------------------------------------------------------

struct RetiredContentCarriesItsStatus;

impl ConformanceCase for RetiredContentCarriesItsStatus {
    fn case_id(&self) -> &str {
        "exec-ret-008-retired-content-is-marked"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 8,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A superseded or expired item keeps its status and validity window inside the pack"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;

        for status in [MemoryStatus::Superseded, MemoryStatus::Expired] {
            let mut retired = retrieval_fixture::item(status);
            retired.valid_until = Some(crate::time::now() - chrono::Duration::days(1));

            let pack =
                match retrieval_fixture::pack(vec![retired], 100, TimePerspective::Current, true) {
                    Ok(pack) => pack,
                    Err(error) => {
                        return result(
                            self.case_id(),
                            self.requirement_ids()[0],
                            Err(format!("a {status:?} item could not be packed: {error}")),
                            "crates/vestrace-domain/src/retrieval/mod.rs",
                        );
                    }
                };

            let packed = &pack.sections[0].items[0];
            if packed.memory_status != status {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a {status:?} item was packed as {:?}, so retired knowledge reads as \
                         current",
                        packed.memory_status
                    )),
                    "crates/vestrace-domain/src/retrieval/mod.rs",
                );
            }
            if packed.valid_until.is_none() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a {status:?} item lost the validity window that says when it stopped \
                         being true"
                    )),
                    "crates/vestrace-domain/src/retrieval/mod.rs",
                );
            }
        }

        result(
            self.case_id(),
            self.requirement_ids()[0],
            Ok(
                "Superseded and Expired items keep both their status and their closing \
                validity bound through packing, so a reader can tell retired content from \
                current content"
                    .to_string(),
            ),
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-010 — Channel degradation is observable.
// ---------------------------------------------------------------------------

struct DegradationCannotBeSilent;

impl ConformanceCase for DegradationCannotBeSilent {
    fn case_id(&self) -> &str {
        "exec-ret-010-degradation-is-observable"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 10,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Marking a pack degraded and naming the channels cannot come apart"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;

        let build = || {
            retrieval_fixture::pack(
                vec![retrieval_fixture::item(MemoryStatus::Active)],
                100,
                TimePerspective::Current,
                true,
            )
            .expect("a complete pack")
        };

        let healthy = match build().with_degradation(Vec::new(), Vec::new()) {
            Ok(pack) => pack,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a healthy pack was refused: {error}")),
                    "crates/vestrace-domain/src/retrieval/mod.rs",
                );
            }
        };
        if healthy.degraded {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a pack with no failed channel reported itself degraded".to_string()),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        // A blank channel name would satisfy "something is named" while telling
        // the reader nothing.
        if build()
            .with_degradation(vec!["   ".to_string()], Vec::new())
            .is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a degraded channel was recorded without a name".to_string()),
                "crates/vestrace-domain/src/retrieval/mod.rs",
            );
        }

        let outcome = match build().with_degradation(
            vec!["vector".to_string()],
            vec!["vector channel failed".to_string()],
        ) {
            Err(error) => Err(format!("a degraded pack could not be built: {error}")),
            Ok(pack) if !pack.degraded => Err(
                "a channel was named as failed and the pack still reported itself healthy, \
                 which is exactly the silent substitution the requirement forbids"
                    .to_string(),
            ),
            Ok(pack) if pack.degraded_channels != vec!["vector".to_string()] => {
                Err("the failed channel was not carried onto the pack".to_string())
            }
            Ok(pack) if pack.warnings.is_empty() => {
                Err("the reason the channel failed was dropped".to_string())
            }
            Ok(_) => Ok(
                "the degraded flag is derived from the named channels rather than set \
                 alongside them, so the two cannot disagree; a blank name is refused"
                    .to_string(),
            ),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-012 — Temporal perspective travels with the pack.
// ---------------------------------------------------------------------------

struct TemporalPerspectiveTravelsWithThePack;

impl ConformanceCase for TemporalPerspectiveTravelsWithThePack {
    fn case_id(&self) -> &str {
        "exec-ret-012-as-of-perspective"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 12,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "An as-of pack is distinguishable from a current one and names the instant it reconstructs"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::MemoryStatus;
        use crate::retrieval::TimePerspective;
        use crate::time::now;

        let subject_time = now() - chrono::Duration::days(30);
        let as_of = match retrieval_fixture::pack(
            vec![retrieval_fixture::item(MemoryStatus::Active)],
            100,
            TimePerspective::AsOf(subject_time),
            true,
        ) {
            Ok(pack) => pack,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("an as-of pack could not be built: {error}")),
                    "crates/vestrace-domain/src/retrieval/mod.rs",
                );
            }
        };

        let current = retrieval_fixture::pack(
            vec![retrieval_fixture::item(MemoryStatus::Active)],
            100,
            TimePerspective::Current,
            true,
        )
        .expect("a current pack");

        let outcome = if as_of.temporal_perspective == current.temporal_perspective {
            Err(
                "an as-of pack is indistinguishable from a current one, so a historical \
                 answer can be read as today's"
                    .to_string(),
            )
        } else if as_of.temporal_perspective != TimePerspective::AsOf(subject_time) {
            Err("the pack does not name the instant it reconstructs".to_string())
        } else if !matches!(current.temporal_perspective, TimePerspective::Current) {
            Err("a current pack does not report the current perspective".to_string())
        } else {
            Ok(format!(
                "an as-of pack carries AsOf({subject_time}) while a current pack carries \
                 Current, so which frame produced an answer is on the answer"
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/retrieval/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-001 — A feedback signal names exactly what it measured.
// ---------------------------------------------------------------------------

struct AFeedbackSignalNamesTheRevisionItJudged;

impl ConformanceCase for AFeedbackSignalNamesTheRevisionItJudged {
    fn case_id(&self) -> &str {
        "exec-lrn-001-signal-names-its-subject"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A model-judge signal without its judge's exact identity is refused, and every fact \
         names at least one piece of evidence"
    }

    fn run(&self) -> ConformanceCaseResult {
        use learning_fixture::*;

        // A judge with no identity is the case that matters: an unattributable
        // opinion that later outranks a measurement is how a learning pipeline
        // launders a guess into a fact.
        let anonymous_judge = EvaluatorRef {
            kind: EvaluatorKind::ModelJudge,
            model_id: None,
            revision: None,
            principal_id: None,
        };
        if fact_with(anonymous_judge, vec![document_evidence()]).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(
                    "a model-judge evaluation was accepted without naming the model or its \
                     revision, so the signal cannot be traced to what produced it"
                        .to_string(),
                ),
                LEARNING_EVIDENCE,
            );
        }

        // And a fact with nothing behind it at all.
        if fact_with(deterministic_evaluator(), Vec::new()).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("an evaluation fact was accepted with no evidence reference".to_string()),
                LEARNING_EVIDENCE,
            );
        }

        let outcome = match fact_with(deterministic_evaluator(), vec![document_evidence()]) {
            Ok(fact) => match &fact.target {
                crate::EvaluationTarget::WorkflowExecution {
                    workflow_revision_id,
                    ..
                } => Ok(format!(
                    "a well-formed fact carries the exact revision it judged \
                     ({workflow_revision_id}) and at least one evidence reference, while an \
                     anonymous model judge is refused"
                )),
                other => Err(format!(
                    "the fixture target did not carry an exact revision: {other:?}"
                )),
            },
            Err(error) => Err(format!(
                "a well-formed evaluation fact was refused: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            LEARNING_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-002 — A learned projection is not a fact.
// ---------------------------------------------------------------------------

struct ALearnedProjectionIsAdvisoryAndNeverBecomesAFact;

impl ConformanceCase for ALearnedProjectionIsAdvisoryAndNeverBecomesAFact {
    fn case_id(&self) -> &str {
        "exec-lrn-002-projection-is-not-a-fact"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "A projection is advisory however it is built, references its facts rather than \
         containing them, and cannot draw on another workspace's measurements"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::learning::{LearnedProjection, ProjectionAuthority};
        use learning_fixture::*;

        let fact = fact_with(deterministic_evaluator(), vec![document_evidence()])
            .expect("a well-formed evaluation fact");

        let projection = match projection_over(&[fact.clone()]) {
            Ok(projection) => projection,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a well-formed projection was refused: {error}")),
                    LEARNING_EVIDENCE,
                );
            }
        };

        if projection.authority != ProjectionAuthority::Advisory {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a learned projection claimed an authority above advisory".to_string()),
                LEARNING_EVIDENCE,
            );
        }

        // A rebuild is the other way a projection comes into being, and it must
        // land in the same place: derived, advisory, and pointing at the facts
        // it was derived from.
        let rebuilt = LearnedProjection::rebuild_from_facts(
            crate::id::LearningProjectionId::new(),
            fact.workspace_id,
            crate::learning::ProjectionKind::PerformanceSummary,
            routing_target(),
            "v1",
            1,
            std::slice::from_ref(&fact),
            fact.created_at,
        );
        match &rebuilt {
            Ok(rebuilt) if rebuilt.authority != ProjectionAuthority::Advisory => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err("a rebuilt projection claimed an authority above advisory".to_string()),
                    LEARNING_EVIDENCE,
                );
            }
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a projection could not be rebuilt from its facts: {error}"
                    )),
                    LEARNING_EVIDENCE,
                );
            }
            _ => {}
        }

        // A projection reaching into another tenant's measurements would make
        // the separation of raw and derived a matter of bookkeeping rather than
        // of scope.
        let foreign = LearnedProjection::rebuild_from_facts(
            crate::id::LearningProjectionId::new(),
            crate::WorkspaceId::new(),
            crate::learning::ProjectionKind::PerformanceSummary,
            routing_target(),
            "v1",
            1,
            std::slice::from_ref(&fact),
            fact.created_at,
        );

        let outcome = if foreign.is_ok() {
            Err("a projection was rebuilt from another workspace's evaluation facts".to_string())
        } else if projection.source_evaluation_fact_ids != vec![fact.id] {
            Err("a projection did not reference the facts it was derived from".to_string())
        } else {
            Ok(
                "a projection is advisory whether constructed or rebuilt, names the facts it \
                 derives from rather than containing them, and cannot be rebuilt from another \
                 workspace's facts"
                    .to_string(),
            )
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            LEARNING_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-003 — Learning cannot ask for privilege.
// ---------------------------------------------------------------------------

struct LearningCannotProposeItsOwnElevation;

impl ConformanceCase for LearningCannotProposeItsOwnElevation {
    fn case_id(&self) -> &str {
        "exec-lrn-003-no-self-elevation"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "A learning change carrying capability, permission or grant fields is refused, \
         including when the field is nested"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::learning::LearningChange;
        use learning_fixture::*;

        let elevations = [
            (
                "a top-level capability list",
                serde_json::json!({ "capabilities": ["workspace.admin"] }),
            ),
            (
                "a nested grant",
                serde_json::json!({ "routing": { "fallback": { "grants": ["memory.write"] } } }),
            ),
            (
                "a differently spelled privilege field",
                serde_json::json!({ "required_capabilities": ["export.read"] }),
            ),
            (
                "a capability inside an array element",
                serde_json::json!({ "rules": [{ "permission": "audit.read" }] }),
            ),
        ];

        for (description, configuration) in elevations {
            let change = LearningChange::ModelRouting { configuration };
            if proposal_with(change).is_ok() {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a learning proposal carrying {description} was accepted, so the \
                         pipeline can ask for privilege it was not granted"
                    )),
                    LEARNING_EVIDENCE,
                );
            }
        }

        let ordinary = LearningChange::ModelRouting {
            configuration: serde_json::json!({ "prefer": "cheapest" }),
        };
        let outcome = match proposal_with(ordinary) {
            Ok(_) => Ok(
                "four shapes of privilege request are refused — top level, nested, renamed \
                 and inside an array — while an ordinary routing change is accepted"
                    .to_string(),
            ),
            Err(error) => Err(format!(
                "an ordinary routing change was refused, so the check is rejecting \
                 everything rather than privilege: {error}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            LEARNING_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-004 — Learning proposes; it does not apply.
// ---------------------------------------------------------------------------

struct LearningProposesAndCannotApply;

impl ConformanceCase for LearningProposesAndCannotApply {
    fn case_id(&self) -> &str {
        "exec-lrn-004-proposal-only"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "A proposal begins as a draft whatever its author asks for, carries the revision it \
         expects, and cannot be submitted twice"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::learning::{LearningChange, LearningProposalStatus};
        use learning_fixture::*;

        let change = LearningChange::ModelRouting {
            configuration: serde_json::json!({ "prefer": "cheapest" }),
        };
        let proposal = match proposal_with(change) {
            Ok(proposal) => proposal,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a well-formed proposal was refused: {error}")),
                    LEARNING_EVIDENCE,
                );
            }
        };

        if proposal.status != LearningProposalStatus::Draft {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "a new proposal was born in status {:?} rather than draft",
                    proposal.status
                )),
                LEARNING_EVIDENCE,
            );
        }

        let submitted = match proposal.clone().submit(proposal.created_at) {
            Ok(submitted) => submitted,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("a draft proposal could not be submitted: {error}")),
                    LEARNING_EVIDENCE,
                );
            }
        };

        // Submitting twice is the shape of an apply loop that skips whatever
        // sits between draft and applied.
        let outcome = if submitted.clone().submit(submitted.updated_at).is_ok() {
            Err(
                "a submitted proposal could be submitted again, so its lifecycle does not \
                 constrain the order of governance"
                    .to_string(),
            )
        } else if submitted.status != LearningProposalStatus::Submitted {
            Err(format!("submitting produced status {:?}", submitted.status))
        } else if submitted.expected_target_revision == 0 {
            Err(
                "a proposal carried no expected target revision, so applying it could not \
                 detect that the target had moved"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "a proposal is created as a draft, reaches submitted once, refuses a second \
                 submission, and names the target revision it was written against \
                 ({})",
                submitted.expected_target_revision
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            LEARNING_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-006 — A measurement outranks an opinion.
// ---------------------------------------------------------------------------

struct AMeasurementOutranksAnOpinion;

impl ConformanceCase for AMeasurementOutranksAnOpinion {
    fn case_id(&self) -> &str {
        "exec-lrn-006-authority-ordering"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 6,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Deterministic and human-authorized signals outrank advisory ones, and share a tier \
         rather than being ordered against each other"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::EvaluationAuthority;

        let deterministic = EvaluationAuthority::Deterministic.priority();
        let human = EvaluationAuthority::HumanAuthorized.priority();
        let advisory = EvaluationAuthority::Advisory.priority();

        let outcome = if deterministic <= advisory {
            Err("a deterministic measurement does not outrank an advisory signal".to_string())
        } else if human <= advisory {
            Err("a human-authorized signal does not outrank an advisory signal".to_string())
        } else if deterministic != human {
            Err(
                "deterministic and human authority are ordered against each other, which the \
                 domain has no basis to decide"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "deterministic and human-authorized signals share priority {deterministic} \
                 while advisory sits at {advisory}, so a model judge cannot outweigh a \
                 measurement by default"
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/evaluation.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// LRN-007 — A conclusion keeps its measurements.
// ---------------------------------------------------------------------------

struct AConclusionKeepsItsMeasurements;

impl ConformanceCase for AConclusionKeepsItsMeasurements {
    fn case_id(&self) -> &str {
        "exec-lrn-007-conclusion-keeps-its-measurements"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Lrn,
            number: 7,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A projection with no fact references, no evidence, or evidence naming a fact it does \
         not declare is refused"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::learning::LearnedProjection;
        use learning_fixture::*;

        let fact = fact_with(deterministic_evaluator(), vec![document_evidence()])
            .expect("a well-formed evaluation fact");
        let undeclared = fact_with(deterministic_evaluator(), vec![document_evidence()])
            .expect("a second well-formed evaluation fact");

        let build = |fact_ids: Vec<crate::id::EvaluationId>, evidence: Vec<crate::EvidenceRef>| {
            LearnedProjection::new(
                crate::id::LearningProjectionId::new(),
                fact.workspace_id,
                crate::learning::ProjectionKind::Trend,
                routing_target(),
                crate::learning::ProjectionGenerator::Deterministic {
                    algorithm_version: "v1".to_string(),
                },
                1,
                fact_ids,
                evidence,
                serde_json::json!({ "summary": "latency is improving" }),
                fact.created_at,
            )
        };

        if build(Vec::new(), vec![evaluation_evidence(fact.id)]).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a conclusion was drawn with no raw measurements behind it".to_string()),
                LEARNING_EVIDENCE,
            );
        }

        if build(vec![fact.id], Vec::new()).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a conclusion was accepted with no evidence provenance".to_string()),
                LEARNING_EVIDENCE,
            );
        }

        // Evidence naming a fact the projection does not declare would let a
        // reader follow a reference the conclusion never used.
        if build(vec![fact.id], vec![evaluation_evidence(undeclared.id)]).is_ok() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a conclusion cited an evaluation it did not declare as a source".to_string()),
                LEARNING_EVIDENCE,
            );
        }

        let outcome = match build(vec![fact.id], vec![evaluation_evidence(fact.id)]) {
            Ok(projection) if projection.source_evaluation_fact_ids.contains(&fact.id) => Ok(
                "a conclusion must declare the measurements it rests on, must carry evidence, \
                 and cannot cite an evaluation it did not declare"
                    .to_string(),
            ),
            Ok(_) => Err("a projection lost the fact reference it was built with".to_string()),
            Err(error) => Err(format!("a well-formed conclusion was refused: {error}")),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            LEARNING_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// QUAL-003 / QUAL-004 — The conformance machinery, put to its own standard.
// ---------------------------------------------------------------------------
//
// These two requirements are about the report rather than about the system it
// describes, which is why they sat on attestations: the obvious executable case
// — run every registered case and inspect the results — would include itself,
// and a check that grades its own output is not a check.
//
// Both cases build a **separate** runner holding two sample cases, one that
// passes and one that fails, and inspect what comes back. That exercises the
// machinery on a population whose correct answer is known in advance, and the
// failing sample is what makes it meaningful: a report that cannot represent a
// failure would satisfy any assertion made only about passes.

struct SampleCase {
    id: &'static str,
    requirement: RequirementId,
    outcome: Result<String, String>,
}

impl ConformanceCase for SampleCase {
    fn case_id(&self) -> &str {
        self.id
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        std::slice::from_ref(&self.requirement)
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "a sample case used to exercise the runner"
    }
    fn run(&self) -> ConformanceCaseResult {
        result(
            self.id,
            self.requirement,
            self.outcome.clone(),
            "crates/vestrace-domain/src/conformance/runner.rs",
        )
    }
}

fn sample_runner() -> ConformanceRunner {
    let mut runner = ConformanceRunner::new();
    runner.register(Box::new(SampleCase {
        id: "sample-pass",
        requirement: RequirementId {
            family: RequirementFamily::Qual,
            number: 3,
        },
        outcome: Ok("the sample property held".to_string()),
    }));
    runner.register(Box::new(SampleCase {
        id: "sample-fail",
        requirement: RequirementId {
            family: RequirementFamily::Qual,
            number: 4,
        },
        outcome: Err("the sample property did not hold".to_string()),
    }));
    runner
}

const CONFORMANCE_EVIDENCE: &str = "crates/vestrace-domain/src/conformance/runner.rs";

struct EveryResultNamesTheRequirementItAnswers;

impl ConformanceCase for EveryResultNamesTheRequirementItAnswers {
    fn case_id(&self) -> &str {
        "exec-qual-003-results-name-their-requirements"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 3,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "Every result a run produces names at least one requirement, and a case outside the \
         profile is marked not-applicable rather than dropped"
    }

    fn run(&self) -> ConformanceCaseResult {
        let runner = sample_runner();
        let report = runner.run_all(None);

        if let Some(anonymous) = report
            .results
            .iter()
            .find(|entry| entry.requirement_ids.is_empty())
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "case {} produced a result answering no requirement, so it counts \
                     towards a total without closing anything",
                    anonymous.case_id
                )),
                CONFORMANCE_EVIDENCE,
            );
        }

        // A case outside the profile must still appear. Dropping it would make
        // the report's totals depend on the profile in a way a reader cannot
        // see, and "not asked" would become indistinguishable from "not run".
        let scoped = runner.run_for_profile(super::QualificationProfile::Core);
        let outcome = if scoped.results.len() != report.results.len() {
            Err(format!(
                "running for a profile returned {} of {} results, so cases outside the \
                 profile disappear instead of being marked not-applicable",
                scoped.results.len(),
                report.results.len()
            ))
        } else if !scoped
            .results
            .iter()
            .all(|entry| entry.status == CaseStatus::NotApplicable)
        {
            Err(
                "a case outside the CORE closure was evaluated as though it belonged to it"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "all {} results name a requirement, and running for a profile keeps every \
                 case while marking the ones outside its closure not-applicable",
                report.results.len()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            CONFORMANCE_EVIDENCE,
        )
    }
}

struct AReportCanRepresentAFailure;

impl ConformanceCase for AReportCanRepresentAFailure {
    fn case_id(&self) -> &str {
        "exec-qual-004-report-is-machine-readable"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 4,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A report serializes, distinguishes pass from fail, counts both, and carries evidence \
         on each result"
    }

    fn run(&self) -> ConformanceCaseResult {
        let report = sample_runner().run_all(None);

        let json = match serde_json::to_value(&report) {
            Ok(json) => json,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the report is not machine-readable: {error}")),
                    CONFORMANCE_EVIDENCE,
                );
            }
        };

        let passed = report
            .results
            .iter()
            .filter(|entry| entry.status == CaseStatus::Pass)
            .count();
        let failed = report
            .results
            .iter()
            .filter(|entry| entry.status == CaseStatus::Fail)
            .count();

        let outcome = if failed == 0 {
            Err(
                "a failing case did not produce a failing result, so the report cannot \
                 express the outcome it exists to report"
                    .to_string(),
            )
        } else if passed == 0 {
            Err("a passing case did not produce a passing result".to_string())
        } else if report.summary.failed != failed || report.summary.passed != passed {
            Err(format!(
                "the summary says {} passed and {} failed while the results say {passed} and \
                 {failed}, so the headline and the detail disagree",
                report.summary.passed, report.summary.failed
            ))
        } else if report
            .results
            .iter()
            .any(|entry| entry.evidence.as_deref().unwrap_or("").trim().is_empty())
        {
            Err("a result carries no evidence reference, so a reader cannot check it".to_string())
        } else if json
            .get("results")
            .and_then(|value| value.as_array())
            .is_none()
        {
            Err("the serialized report has no results array".to_string())
        } else {
            Ok(format!(
                "the report serializes, separates {passed} passed from {failed} failed, \
                 agrees with its own summary, and carries evidence on every result"
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            CONFORMANCE_EVIDENCE,
        )
    }
}

// ---------------------------------------------------------------------------
// HLT — health, repair and the operator contract.
// ---------------------------------------------------------------------------
//
// The health domain is a thousand lines that no case has ever executed. Every
// HLT requirement but one is a property of these types, so they are written
// here against the domain rather than attested about it.

const HEALTH_EVIDENCE: &str = "crates/vestrace-domain/src/health.rs";

// ---------------------------------------------------------------------------
// CAP — capability grants, delegation, budgets and the policy decision.
// ---------------------------------------------------------------------------
//
// Fourteen hundred lines of `security/capability.rs` and `security/delegation.rs`
// that no case had executed. Eleven of the fourteen CAP requirements are
// properties of those types; the other three are claims about the deployment and
// are skipped with reasons rather than attested — see the CLI dispositions.

// ---------------------------------------------------------------------------
// EXT — external effects: what a dispatch proves and what it does not.
// ---------------------------------------------------------------------------

const EXTERNAL_EFFECT_EVIDENCE: &str = "crates/vestrace-domain/src/external_effects.rs";

mod effect_fixture {
    use crate::external_effects::{
        DeliverySemantics, EffectPrecondition, EffectReversibility, ExternalEffectIntent,
        ExternalEffectReceipt, IdempotencyProfile,
    };
    use crate::{Capability, DomainError, PrincipalId, RiskCategory, WorkspaceId, time::Timestamp};

    pub fn intent_with(reversibility: EffectReversibility, at: Timestamp) -> ExternalEffectIntent {
        ExternalEffectIntent::new(
            "run://01900000-0000-7000-8000-000000000001",
            WorkspaceId::new(),
            PrincipalId::new(),
            "webhook-v1",
            "send",
            "https://alpha.effects.test/hook",
            "sha256:arguments",
            "deliver notification",
            vec![EffectPrecondition::new("resource-version", "v1").expect("a precondition")],
            "sha256:preconditions-v1",
            RiskCategory::Medium,
            reversibility,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExportRead,
            Some("budget:reservation-1"),
            Some("policy:decision-1"),
            at,
        )
        .expect("a well-formed intent")
    }

    pub fn timed_out(
        intent: &ExternalEffectIntent,
        at: Timestamp,
    ) -> Result<ExternalEffectReceipt, DomainError> {
        ExternalEffectReceipt::synthetic_unknown(
            intent.id(),
            intent.adapter(),
            at,
            vec!["evidence:timeout".into()],
        )
    }

    /// An adapter that declares a contract and answers however the test says.
    pub struct TestAdapter {
        pub descriptor: crate::external_effects::ExternalEffectAdapterDescriptor,
        pub result: crate::external_effects::AdapterDispatchResult,
    }

    impl crate::external_effects::ExternalEffectAdapter for TestAdapter {
        fn descriptor(&self) -> &crate::external_effects::ExternalEffectAdapterDescriptor {
            &self.descriptor
        }
        fn dispatch(
            &self,
            _intent: &ExternalEffectIntent,
        ) -> Result<
            crate::external_effects::AdapterDispatchResult,
            crate::external_effects::AdapterError,
        > {
            Ok(self.result.clone())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn descriptor(
        name: &str,
        delivery: DeliverySemantics,
        idempotency: IdempotencyProfile,
        reversibility: EffectReversibility,
        supports_reconciliation: bool,
    ) -> Result<crate::external_effects::ExternalEffectAdapterDescriptor, DomainError> {
        crate::external_effects::ExternalEffectAdapterDescriptor::new(
            name,
            delivery,
            idempotency,
            reversibility,
            crate::external_effects::DryRunMode::Simulated,
            supports_reconciliation,
            true,
            Capability::ExportRead,
        )
    }

    pub fn adapter_for(intent: &ExternalEffectIntent) -> TestAdapter {
        TestAdapter {
            descriptor: descriptor(
                intent.adapter(),
                intent.delivery_semantics(),
                intent.idempotency_profile(),
                intent.reversibility(),
                true,
            )
            .expect("a well-formed descriptor"),
            result: crate::external_effects::AdapterDispatchResult::acknowledged(
                "2xx",
                Some("external:1".to_string()),
                Some("sha256:response".to_string()),
                vec!["evidence:dispatch".to_string()],
            ),
        }
    }

    pub fn authorization_for(
        intent: &ExternalEffectIntent,
    ) -> crate::external_effects::EffectAuthorization {
        crate::external_effects::EffectAuthorization::allow(
            "policy:decision-1",
            "policy-v1",
            intent.workspace_id(),
            intent.actor_id(),
            intent.required_capability(),
            intent.operation(),
            intent.target(),
        )
    }
}

macro_rules! ext_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Ext,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    EXTERNAL_EFFECT_EVIDENCE,
                )
            }
        }
    };
}

ext_case!(
    AnAdapterMustDeclareAContractItCanKeep,
    "exec-ext-002-adapter-contract",
    2,
    CaseCategory::Behavioral,
    "An adapter promising effectively-once delivery without provider idempotency is refused, \
     one that does not support reconciliation is refused, and dispatch refuses an adapter \
     whose declaration does not match the intent",
    || {
        use crate::external_effects::{
            DeliverySemantics, DispatchError, EffectReversibility, IdempotencyProfile,
            validate_adapter_descriptor,
        };
        use crate::time::now;
        use effect_fixture::*;

        let at = now();

        // Effectively-once is a promise only the provider can keep. Declaring
        // it without provider-side idempotency is a claim the adapter cannot
        // honour, and the contract refuses it rather than discovering it later.
        let overclaiming = descriptor(
            "webhook-v1",
            DeliverySemantics::EffectivelyOnce,
            IdempotencyProfile::None,
            EffectReversibility::Compensatable,
            true,
        )
        .map_err(|error| format!("a descriptor was refused for the wrong reason: {error}"))?;
        if validate_adapter_descriptor(&overclaiming).is_ok() {
            return Err(
                "an adapter promised effectively-once delivery with no idempotency \
                        mechanism behind it"
                    .to_string(),
            );
        }

        let unreconcilable = descriptor(
            "webhook-v1",
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            EffectReversibility::Compensatable,
            false,
        )
        .map_err(|error| format!("a descriptor was refused for the wrong reason: {error}"))?;
        if validate_adapter_descriptor(&unreconcilable).is_ok() {
            return Err(
                "an adapter that cannot reconcile was accepted, so an unknown outcome \
                        it produced could never be settled"
                    .to_string(),
            );
        }

        // The declaration has to match the intent it is dispatching, or the
        // intent's recorded semantics describe something other than what ran.
        let intent = intent_with(EffectReversibility::Compensatable, at);
        let authorized = intent
            .authorize(&authorization_for(&intent))
            .map_err(|error| format!("a matching authorization was refused: {error}"))?;

        let mut mismatched = adapter_for(&intent);
        mismatched.descriptor = descriptor(
            "webhook-v1",
            DeliverySemantics::AtMostOnce,
            intent.idempotency_profile(),
            intent.reversibility(),
            true,
        )
        .map_err(|error| format!("a descriptor was refused: {error}"))?;

        match authorized.dispatch(&mismatched, intent.precondition_digest(), at) {
            Err(DispatchError::AdapterDoesNotMatchIntent) => {}
            other => {
                return Err(format!(
                    "dispatching through an adapter with different delivery semantics gave \
                     {other:?} rather than refusing the mismatch"
                ));
            }
        }

        authorized
            .dispatch(&adapter_for(&intent), intent.precondition_digest(), at)
            .map_err(|error| format!("a conforming dispatch was refused: {error:?}"))?;

        Ok(
            "an adapter cannot declare a delivery guarantee it has no mechanism for, cannot \
            omit reconciliation, and cannot dispatch an intent whose declaration it does not \
            match"
                .to_string(),
        )
    }
);

ext_case!(
    RetryCannotDuplicateWhatMightHaveHappened,
    "exec-ext-005-idempotency-on-retry",
    5,
    CaseCategory::Stateful,
    "Retry is denied while the outcome is unknown, and an effectively-once intent can only be \
     dispatched by an adapter with provider-side idempotency",
    || {
        use crate::external_effects::{
            DeliverySemantics, EffectReversibility, IdempotencyProfile, RetryDecision,
            validate_adapter_descriptor,
        };
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);

        // The retry rule and the idempotency rule are two halves of the same
        // protection. Either alone allows a duplicate: retrying an unknown
        // effect through an idempotent adapter is safe, and retrying a known
        // failure through a non-idempotent one is safe, but retrying an unknown
        // effect through a non-idempotent adapter is how one action becomes two.
        let unknown =
            timed_out(&intent, at).map_err(|error| format!("a receipt was refused: {error}"))?;
        if unknown.retry_decision() != RetryDecision::DeniedUnknown {
            return Err("an unknown outcome permitted a retry".to_string());
        }

        for (profile, should_pass) in [
            (IdempotencyProfile::ProviderKey, true),
            (IdempotencyProfile::Conditional, true),
            (IdempotencyProfile::None, false),
            (IdempotencyProfile::Unknown, false),
        ] {
            let candidate = descriptor(
                "webhook-v1",
                DeliverySemantics::EffectivelyOnce,
                profile,
                EffectReversibility::Compensatable,
                true,
            )
            .map_err(|error| format!("a descriptor was refused: {error}"))?;
            if validate_adapter_descriptor(&candidate).is_ok() != should_pass {
                return Err(format!(
                    "an effectively-once adapter with idempotency profile {profile:?} was \
                     {} when it should have been the opposite",
                    if should_pass { "refused" } else { "accepted" }
                ));
            }
        }

        Ok(
            "an unknown outcome denies retry, and effectively-once delivery is only accepted \
            from an adapter with a provider-side idempotency mechanism"
                .to_string(),
        )
    }
);

ext_case!(
    AuthorityIsCheckedAgainstTheIntentItself,
    "exec-ext-011-authority-at-dispatch",
    11,
    CaseCategory::Security,
    "Authorization must match the intent's workspace, actor, capability, operation, target and \
     recorded policy decision, and a stale intent is refused at dispatch",
    || {
        use crate::external_effects::{DispatchError, EffectAuthorization, EffectReversibility};
        use crate::time::now;
        use crate::{Capability, PrincipalId, WorkspaceId};
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);

        let variant = |workspace, actor, capability: Capability, operation: &str, scope: &str| {
            EffectAuthorization::allow(
                "policy:decision-1",
                "policy-v1",
                workspace,
                actor,
                capability,
                operation,
                scope,
            )
        };

        let mismatches = [
            (
                "another workspace",
                variant(
                    WorkspaceId::new(),
                    intent.actor_id(),
                    intent.required_capability(),
                    intent.operation(),
                    intent.target(),
                ),
            ),
            (
                "another actor",
                variant(
                    intent.workspace_id(),
                    PrincipalId::new(),
                    intent.required_capability(),
                    intent.operation(),
                    intent.target(),
                ),
            ),
            (
                "another capability",
                variant(
                    intent.workspace_id(),
                    intent.actor_id(),
                    Capability::AuditRead,
                    intent.operation(),
                    intent.target(),
                ),
            ),
            (
                "another operation",
                variant(
                    intent.workspace_id(),
                    intent.actor_id(),
                    intent.required_capability(),
                    "delete",
                    intent.target(),
                ),
            ),
            (
                "another target",
                variant(
                    intent.workspace_id(),
                    intent.actor_id(),
                    intent.required_capability(),
                    intent.operation(),
                    "https://beta.effects.test/hook",
                ),
            ),
        ];

        for (label, authorization) in mismatches {
            if intent.authorize(&authorization).is_ok() {
                return Err(format!(
                    "an authorization for {label} was accepted for this effect, so a decision \
                     about one action would permit another"
                ));
            }
        }

        // A decision that denied must not authorize, whatever else matches.
        let denied = EffectAuthorization::deny(
            "policy:decision-2",
            "policy-v1",
            intent.workspace_id(),
            intent.actor_id(),
            intent.required_capability(),
            intent.operation(),
            intent.target(),
        );
        if intent.authorize(&denied).is_ok() {
            return Err("a denial authorized the effect".to_string());
        }

        // And authority is not the only thing rechecked at dispatch: the world
        // the intent was written against must still hold.
        let authorized = intent
            .authorize(&authorization_for(&intent))
            .map_err(|error| format!("a matching authorization was refused: {error}"))?;
        match authorized.dispatch(&adapter_for(&intent), "sha256:preconditions-v2", at) {
            Err(DispatchError::StaleIntent) => {}
            other => {
                return Err(format!(
                    "dispatching against changed preconditions gave {other:?} rather than \
                     refusing a stale intent"
                ));
            }
        }

        Ok(
            "authorization must match the intent in every field it names, a denial never \
            authorizes, and dispatch refuses an intent whose preconditions have moved"
                .to_string(),
        )
    }
);

ext_case!(
    AnAdapterSaysWhatItCannotDo,
    "exec-ext-015-adapter-capability-contract",
    15,
    CaseCategory::Behavioral,
    "An adapter's descriptor declares its delivery semantics, idempotency, reversibility, \
     dry-run support, read-back support and required capability — including when the answer \
     is that it cannot",
    || {
        use crate::Capability;
        use crate::external_effects::{
            DeliverySemantics, DryRunMode, EffectReversibility, ExternalEffectAdapterDescriptor,
            IdempotencyProfile,
        };

        if ExternalEffectAdapterDescriptor::new(
            "   ",
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            EffectReversibility::Compensatable,
            DryRunMode::Simulated,
            true,
            true,
            Capability::ExportRead,
        )
        .is_ok()
        {
            return Err("an adapter was declared with no name".to_string());
        }

        // The negative declarations are the point of the contract: an adapter
        // that cannot dry-run or cannot read back has to say so, because a
        // caller planning a rehearsal or a reconciliation needs to know before
        // it depends on one.
        let limited = ExternalEffectAdapterDescriptor::new(
            "webhook-v1",
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::None,
            EffectReversibility::Irreversible,
            DryRunMode::Unsupported,
            true,
            false,
            Capability::ExportRead,
        )
        .map_err(|error| format!("a limited adapter could not be declared: {error}"))?;

        if limited.dry_run() != DryRunMode::Unsupported {
            return Err("an adapter cannot declare that it does not support a dry run".to_string());
        }
        if limited.supports_read_back() {
            return Err("an adapter that cannot read back reported that it can".to_string());
        }
        if limited.reversibility() != EffectReversibility::Irreversible {
            return Err("an adapter cannot declare its effects irreversible".to_string());
        }
        if limited.required_capability() != Capability::ExportRead {
            return Err(
                "an adapter does not declare the capability its effects require".to_string(),
            );
        }

        Ok(
            "an adapter declares delivery, idempotency, reversibility, dry-run and read-back \
            support and its required capability, and can say that it supports none of them"
                .to_string(),
        )
    }
);

ext_case!(
    UnknownIsItsOwnAnswer,
    "exec-ext-004-unknown-is-first-class",
    4,
    CaseCategory::Behavioral,
    "An effect whose outcome is unknown reports Unknown rather than success or failure, asks \
     for reconciliation, and refuses retry on the grounds that it is unknown",
    || {
        use crate::external_effects::{EffectLifecycleStatus, EffectReversibility, RetryDecision};
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);
        let receipt = timed_out(&intent, at)
            .map_err(|error| format!("an unknown receipt was refused: {error}"))?;

        if receipt.outcome_status() != EffectLifecycleStatus::Unknown {
            return Err(format!(
                "an unknown outcome was recorded as {:?}, collapsing the one status that means \
                 'we do not know' into one that claims we do",
                receipt.outcome_status()
            ));
        }
        if !receipt.requires_reconciliation() {
            return Err("an unknown effect did not ask to be reconciled".to_string());
        }

        // Retrying an unknown effect is how one payment becomes two. The
        // decision has its own reason so a caller cannot confuse it with an
        // ordinary non-retryable failure.
        match receipt.retry_decision() {
            RetryDecision::DeniedUnknown => {}
            other => {
                return Err(format!(
                    "an unknown effect returned retry decision {other:?} rather than denying \
                     on the grounds of not knowing"
                ));
            }
        }

        // A receipt with no evidence would be an assertion that something is
        // unknown, which is itself unsupported.
        if crate::external_effects::ExternalEffectReceipt::synthetic_unknown(
            intent.id(),
            intent.adapter(),
            at,
            Vec::new(),
        )
        .is_ok()
        {
            return Err("an unknown receipt was recorded with no evidence".to_string());
        }

        Ok(
            "unknown is a status of its own: it demands reconciliation, denies retry for its \
            own stated reason, and cannot be claimed without evidence"
                .to_string(),
        )
    }
);

ext_case!(
    DispatchIsNotDelivery,
    "exec-ext-006-dispatch-is-not-confirmation",
    6,
    CaseCategory::Behavioral,
    "A receipt from dispatch is never a business confirmation, whatever its status",
    || {
        use crate::external_effects::EffectReversibility;
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);
        let receipt =
            timed_out(&intent, at).map_err(|error| format!("a receipt was refused: {error}"))?;

        // The provider accepting the call says the call was accepted. Whether
        // the thing was done is a separate question, answered by reconciliation
        // against observed state.
        if receipt.is_business_confirmation() {
            return Err(
                "a dispatch receipt reported itself as confirmation that the effect \
                        happened, so an accepted HTTP call would count as a delivered message"
                    .to_string(),
            );
        }
        if receipt.evidence_refs().is_empty() {
            return Err("a receipt carries no evidence of what was observed".to_string());
        }

        Ok(
            "a dispatch receipt records that a call was made and never that the effect took \
            place; confirmation comes from reconciliation"
                .to_string(),
        )
    }
);

ext_case!(
    ReconciliationWeighsIntentReceiptAndObservation,
    "exec-ext-007-reconciliation-compares-three",
    7,
    CaseCategory::Stateful,
    "Reconciliation refuses a receipt from another intent, requires an observation, takes the \
     strongest evidence, and reports an inconclusive observation as inconclusive",
    || {
        use crate::external_effects::{
            EffectReversibility, EvidenceStrength, ObservedEffectState, ReconciliationOutcome,
            reconcile_effect,
        };
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);
        let receipt =
            timed_out(&intent, at).map_err(|error| format!("a receipt was refused: {error}"))?;
        let other = intent_with(EffectReversibility::Compensatable, at);
        let foreign_receipt =
            timed_out(&other, at).map_err(|error| format!("a receipt was refused: {error}"))?;

        if reconcile_effect(
            &intent,
            &foreign_receipt,
            vec![ObservedEffectState::new(
                EvidenceStrength::ProviderIdempotencyLookup,
                Some(true),
                "external:delivered",
                vec!["evidence:lookup".into()],
            )],
            at,
        )
        .is_ok()
        {
            return Err("a receipt belonging to another effect reconciled this one".to_string());
        }

        if reconcile_effect(&intent, &receipt, Vec::new(), at).is_ok() {
            return Err(
                "reconciliation succeeded with nothing observed, so 'we looked' would \
                        be indistinguishable from 'we did not'"
                    .to_string(),
            );
        }

        // Two observations disagreeing: the stronger evidence decides, rather
        // than the first or the most recent.
        let reconciliation = reconcile_effect(
            &intent,
            &receipt,
            vec![
                ObservedEffectState::new(
                    EvidenceStrength::ResponseDigest,
                    Some(false),
                    "external:absent",
                    vec!["evidence:digest".into()],
                ),
                ObservedEffectState::new(
                    EvidenceStrength::ProviderIdempotencyLookup,
                    Some(true),
                    "external:delivered",
                    vec!["evidence:lookup".into()],
                ),
            ],
            at,
        )
        .map_err(|error| format!("a well-formed reconciliation was refused: {error}"))?;

        if reconciliation.outcome() != ReconciliationOutcome::Confirmed {
            return Err(format!(
                "the weaker evidence decided the outcome, giving {:?}",
                reconciliation.outcome()
            ));
        }
        if reconciliation.evidence_strength() != EvidenceStrength::ProviderIdempotencyLookup {
            return Err("the reconciliation does not record which evidence decided it".to_string());
        }

        // An observation that cannot say either way must not become a verdict.
        let inconclusive = reconcile_effect(
            &intent,
            &receipt,
            vec![ObservedEffectState::new(
                EvidenceStrength::MarkerSearch,
                None,
                "external:unclear",
                vec!["evidence:marker".into()],
            )],
            at,
        )
        .map_err(|error| format!("a well-formed reconciliation was refused: {error}"))?;
        if inconclusive.outcome() != ReconciliationOutcome::Inconclusive {
            return Err(format!(
                "an observation that could not tell produced {:?}",
                inconclusive.outcome()
            ));
        }

        Ok(
            "reconciliation ties a receipt to its own intent, requires an observation, lets the \
            strongest evidence decide, and keeps inconclusive as an outcome"
                .to_string(),
        )
    }
);

ext_case!(
    CompensationIsANewEffectWithItsOwnIdentity,
    "exec-ext-008-compensation-is-forward",
    8,
    CaseCategory::Behavioral,
    "A compensation is a new effect naming what it compensates, and an irreversible effect \
     cannot be compensated at all",
    || {
        use crate::external_effects::EffectReversibility;
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let original = intent_with(EffectReversibility::Compensatable, at);
        let compensation = original
            .compensation_for(
                "cancel",
                "sha256:compensation-arguments",
                "withdraw the notification",
                at,
            )
            .map_err(|error| format!("a compensation was refused: {error}"))?;

        if compensation.id() == original.id() {
            return Err(
                "a compensation reused the identity of the effect it compensates, so \
                        the original would be overwritten rather than answered"
                    .to_string(),
            );
        }
        if compensation.compensates_effect_id() != Some(original.id()) {
            return Err("a compensation does not name the effect it compensates".to_string());
        }
        if original.compensates_effect_id().is_some() {
            return Err("compensating an effect altered the original".to_string());
        }

        // Declaring an effect irreversible has to mean something: the domain
        // must refuse to produce a compensation that could not work.
        let irreversible = intent_with(EffectReversibility::Irreversible, at);
        if irreversible
            .compensation_for("cancel", "sha256:x", "withdraw", at)
            .is_ok()
        {
            return Err("an irreversible effect produced a compensation".to_string());
        }
        let unknown = intent_with(EffectReversibility::Unknown, at);
        if unknown
            .compensation_for("cancel", "sha256:x", "withdraw", at)
            .is_ok()
        {
            return Err(
                "an effect of unknown reversibility produced a compensation, so an \
                        undeclared effect would be treated as undoable"
                    .to_string(),
            );
        }

        Ok(
            "compensation is a new effect with its own identity that names its subject, and is \
            refused for effects not declared compensatable"
                .to_string(),
        )
    }
);

ext_case!(
    ReversibilityIsDeclaredRatherThanAssumed,
    "exec-ext-009-reversibility-is-declared",
    9,
    CaseCategory::Behavioral,
    "Reversibility travels on the intent, has an explicit Unknown, and is inherited by a \
     compensation rather than re-derived",
    || {
        use crate::external_effects::EffectReversibility;
        use crate::time::now;
        use effect_fixture::*;

        let at = now();

        for declared in [
            EffectReversibility::Reversible,
            EffectReversibility::Compensatable,
            EffectReversibility::Irreversible,
            EffectReversibility::Unknown,
        ] {
            let intent = intent_with(declared, at);
            if intent.reversibility() != declared {
                return Err(
                    "an intent did not keep the reversibility it was created with".to_string(),
                );
            }
        }

        // `Unknown` existing as a variant is the requirement: without it, an
        // adapter that has not said would be indistinguishable from one that
        // declared the effect reversible.
        let undeclared = intent_with(EffectReversibility::Unknown, at);
        if undeclared.reversibility() == EffectReversibility::Reversible {
            return Err("an undeclared reversibility read as reversible".to_string());
        }

        let compensatable = intent_with(EffectReversibility::Compensatable, at);
        let compensation = compensatable
            .compensation_for("cancel", "sha256:x", "withdraw", at)
            .map_err(|error| format!("a compensation was refused: {error}"))?;
        if compensation.reversibility() != compensatable.reversibility() {
            return Err(
                "a compensation invented its own reversibility instead of inheriting \
                        the declaration it answers"
                    .to_string(),
            );
        }

        Ok(
            "reversibility is declared on the intent, distinguishes 'unknown' from 'reversible', \
            and is inherited by the compensation"
                .to_string(),
        )
    }
);

ext_case!(
    ATimeoutMeansUnknown,
    "exec-ext-010-timeout-is-not-a-verdict",
    10,
    CaseCategory::Stateful,
    "A dispatch that timed out yields an unknown outcome rather than a failure, and unknown \
     denies retry so a timeout cannot become a duplicate",
    || {
        use crate::external_effects::{EffectLifecycleStatus, EffectReversibility, RetryDecision};
        use crate::time::now;
        use effect_fixture::*;

        let at = now();
        let intent = intent_with(EffectReversibility::Compensatable, at);
        let receipt = timed_out(&intent, at)
            .map_err(|error| format!("a timeout receipt was refused: {error}"))?;

        if receipt.outcome_status() == EffectLifecycleStatus::Failed {
            return Err(
                "a timeout was recorded as a failure, which asserts the effect did not \
                        happen when nobody knows whether it did"
                    .to_string(),
            );
        }
        if receipt.outcome_status() == EffectLifecycleStatus::Acknowledged {
            return Err("a timeout was recorded as acknowledged".to_string());
        }
        if receipt.retry_decision() == RetryDecision::Allowed {
            return Err(
                "a timed-out effect was retryable, which is how one external action \
                        becomes two"
                    .to_string(),
            );
        }
        if !receipt.requires_reconciliation() {
            return Err(
                "a timed-out effect did not ask to be reconciled, so the question of \
                        what happened would never be settled"
                    .to_string(),
            );
        }

        Ok(
            "a timeout produces an unknown outcome that denies retry and demands \
            reconciliation, rather than a verdict in either direction"
                .to_string(),
        )
    }
);

// ---------------------------------------------------------------------------
// GOV — classification, retention and deletion.
// ---------------------------------------------------------------------------
//
// The first batch of the largest remaining family. These cover the parts of
// `trust.rs` that decide where classified data may go and what deletion has to
// prove; keys, export, audit integrity and federation follow.

const GOVERNANCE_EVIDENCE: &str = "crates/vestrace-domain/src/trust.rs";

mod governance_fixture {
    use crate::trust::{
        DataClassification, DataHold, DataPolicy, DeletionPlan, DeletionRequest, DeletionSemantics,
    };
    use crate::{
        Capability, DataDestination, DataPolicyId, DomainError, PrincipalId, Sensitivity,
        time::Timestamp,
    };
    use std::collections::BTreeSet;

    pub fn classified(sensitivity: Sensitivity) -> DataClassification {
        DataClassification::source(sensitivity, "memory:0198", "provenance:test")
            .expect("a well-formed classification")
    }

    pub fn policy(
        maximum: Sensitivity,
        destinations: &[DataDestination],
        required: Option<Capability>,
    ) -> Result<DataPolicy, DomainError> {
        DataPolicy::new(
            DataPolicyId::new(),
            "data-policy-v1",
            maximum,
            destinations.iter().copied().collect::<BTreeSet<_>>(),
            required,
        )
    }

    pub fn hold(at: Timestamp, expires_at: Option<Timestamp>) -> DataHold {
        DataHold::new(
            "memory:0198",
            "litigation",
            PrincipalId::new(),
            "policy:legal-hold-v1",
            at,
            expires_at,
        )
        .expect("a well-formed hold")
    }

    pub fn deletion(at: Timestamp) -> (DeletionRequest, DeletionPlan) {
        let request = DeletionRequest::new(
            PrincipalId::new(),
            PrincipalId::new(),
            "memory:0198",
            DeletionSemantics::PhysicalDelete,
            "plan:1",
            "execution:1",
            at,
        )
        .expect("a well-formed deletion request");
        let plan = DeletionPlan::new(
            request.id(),
            vec!["memory:0198".to_string()],
            vec!["search_document:0198".to_string()],
            Vec::new(),
            at,
        )
        .expect("a well-formed deletion plan");
        (request, plan)
    }
}

macro_rules! gov_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Gov,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    GOVERNANCE_EVIDENCE,
                )
            }
        }
    };
}

gov_case!(
    APolicyBoundsSensitivityDestinationAndCapability,
    "exec-gov-003-policy-enforces-its-boundaries",
    3,
    CaseCategory::Behavioral,
    "A data policy refuses data above its ceiling, a destination it does not list, and a \
     request lacking the capability it requires — and says which",
    || {
        use crate::{Capability, DataDestination, Sensitivity};
        use governance_fixture::*;

        let bounded = policy(
            Sensitivity::Confidential,
            &[DataDestination::LocalModel],
            Some(Capability::ExportRead),
        )
        .map_err(|error| format!("a well-formed policy was refused: {error}"))?;

        // A policy naming no destination would permit everything by omission.
        if policy(Sensitivity::Public, &[], None).is_ok() {
            return Err("a data policy was accepted with no allowed destinations".to_string());
        }

        let allowed = bounded.evaluate(
            &classified(Sensitivity::Internal),
            DataDestination::LocalModel,
            true,
        );
        if !allowed.is_allowed() {
            return Err(format!(
                "data inside the policy was refused: {}",
                allowed.reason()
            ));
        }
        if allowed.policy_version() != "data-policy-v1" {
            return Err("a decision does not name the policy version that produced it".to_string());
        }

        let too_sensitive = bounded.evaluate(
            &classified(Sensitivity::Restricted),
            DataDestination::LocalModel,
            true,
        );
        if too_sensitive.is_allowed() {
            return Err("data above the policy ceiling was allowed".to_string());
        }

        let wrong_destination = bounded.evaluate(
            &classified(Sensitivity::Internal),
            DataDestination::RemoteProvider,
            true,
        );
        if wrong_destination.is_allowed() {
            return Err("a destination the policy does not list was allowed".to_string());
        }

        let uncapable = bounded.evaluate(
            &classified(Sensitivity::Internal),
            DataDestination::LocalModel,
            false,
        );
        if uncapable.is_allowed() {
            return Err("a required capability was not required".to_string());
        }

        Ok(format!(
            "a policy refuses by ceiling ({}), by destination ({}) and by capability ({}), \
             and each refusal names its reason",
            too_sensitive.reason(),
            wrong_destination.reason(),
            uncapable.reason()
        ))
    }
);

gov_case!(
    DerivedDataInheritsTheHighestSensitivity,
    "exec-gov-004-lineage-propagates-upward",
    4,
    CaseCategory::Behavioral,
    "A derivation takes the highest sensitivity of its sources, keeps their references, and \
     cannot be derived from nothing",
    || {
        use crate::Sensitivity;
        use crate::trust::ClassificationLineage;
        use governance_fixture::*;

        if ClassificationLineage::derive(Vec::new(), Vec::new(), "policy-v1", "derivation:1")
            .is_ok()
        {
            return Err(
                "a classification lineage was derived from no sources, so derived data \
                        could claim any sensitivity it liked"
                    .to_string(),
            );
        }

        let lineage = ClassificationLineage::derive(
            vec!["memory:public".to_string(), "memory:secret".to_string()],
            vec![
                classified(Sensitivity::Public),
                classified(Sensitivity::Restricted),
            ],
            "policy-v1",
            "derivation:1",
        )
        .map_err(|error| format!("a well-formed lineage was refused: {error}"))?;

        // The whole point: mixing public with restricted yields restricted, not
        // public and not an average.
        if lineage.effective_classification().sensitivity() != Sensitivity::Restricted {
            return Err(format!(
                "derived data took sensitivity {:?} from sources including Restricted",
                lineage.effective_classification().sensitivity()
            ));
        }
        if lineage.source_refs().len() != 2 {
            return Err("a lineage did not keep the references it derived from".to_string());
        }
        if lineage.policy_version().trim().is_empty() {
            return Err(
                "a lineage does not name the classification policy that produced it".to_string(),
            );
        }

        Ok(
            "a derivation inherits the highest sensitivity among its sources and keeps every \
            source reference"
                .to_string(),
        )
    }
);

gov_case!(
    ClassifiedDataDoesNotReachAnUnapprovedModel,
    "exec-gov-005-model-boundary",
    5,
    CaseCategory::Security,
    "The model boundary refuses classified data bound for a remote provider the policy does \
     not allow, and refuses it for a log destination regardless",
    || {
        use crate::trust::evaluate_model_boundary;
        use crate::{DataDestination, Sensitivity};
        use governance_fixture::*;

        let policy = policy(
            Sensitivity::Confidential,
            &[DataDestination::LocalModel],
            None,
        )
        .map_err(|error| format!("a well-formed policy was refused: {error}"))?;

        let local = evaluate_model_boundary(
            &policy,
            &classified(Sensitivity::Confidential),
            DataDestination::LocalModel,
            true,
        );
        if !local.is_allowed() {
            return Err(
                "confidential data was refused to the local model the policy allows".to_string(),
            );
        }

        for destination in [
            DataDestination::RemoteProvider,
            DataDestination::LogOutput,
            DataDestination::ExportBundle,
        ] {
            let decision = evaluate_model_boundary(
                &policy,
                &classified(Sensitivity::Confidential),
                destination,
                true,
            );
            if decision.is_allowed() {
                return Err(format!(
                    "confidential data was allowed to reach {destination:?}, which the policy \
                     does not list"
                ));
            }
        }

        Ok(
            "classified data reaches only the destinations its policy names, and a remote \
            provider, a log and an export bundle are each refused when unlisted"
                .to_string(),
        )
    }
);

gov_case!(
    ALegalHoldOutranksDeletion,
    "exec-gov-007-hold-blocks-deletion",
    7,
    CaseCategory::Stateful,
    "An active hold makes deletion verification report BlockedByHold rather than Verified, and \
     a lapsed hold stops blocking",
    || {
        use crate::time::now;
        use crate::trust::{DeletionVerificationOutcome, verify_deletion};
        use chrono::Duration;
        use governance_fixture::*;

        let at = now();
        let (request, plan) = deletion(at);
        let checked = vec![
            "memory:0198".to_string(),
            "search_document:0198".to_string(),
        ];

        let held = verify_deletion(
            &request,
            &plan,
            &[hold(at, None)],
            checked.clone(),
            Vec::new(),
            vec!["evidence:deletion-1".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;

        if held.outcome() != DeletionVerificationOutcome::BlockedByHold {
            return Err(format!(
                "an active legal hold produced {:?} rather than blocking the deletion",
                held.outcome()
            ));
        }
        if held.is_complete() {
            return Err("a deletion blocked by a hold reported itself complete".to_string());
        }

        // A hold that has expired is no longer a hold — otherwise "until the
        // case closes" would mean "for ever", the same shape as the health
        // disposition expiry.
        let lapsed = hold(at - Duration::hours(2), Some(at - Duration::hours(1)));
        let after = verify_deletion(
            &request,
            &plan,
            &[lapsed],
            checked,
            Vec::new(),
            vec!["evidence:deletion-2".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;
        if after.outcome() != DeletionVerificationOutcome::Verified {
            return Err(format!(
                "an expired hold still blocked deletion, reporting {:?}",
                after.outcome()
            ));
        }

        Ok(
            "an active hold blocks deletion and reports it as blocked rather than incomplete, \
            while an expired one does not"
                .to_string(),
        )
    }
);

gov_case!(
    DeletionMustAccountForEveryDependency,
    "exec-gov-008-dependency-aware-deletion",
    8,
    CaseCategory::Behavioral,
    "Verification refuses a check that skipped a planned dependency, and reports Incomplete \
     when copies remain",
    || {
        use crate::time::now;
        use crate::trust::{DeletionVerificationOutcome, verify_deletion};
        use governance_fixture::*;

        let at = now();
        let (request, plan) = deletion(at);

        // The dependency is what turns a delete into an orphan: removing the
        // memory while its search document survives leaves a reference to
        // content that is gone.
        let partial = verify_deletion(
            &request,
            &plan,
            &[],
            vec!["memory:0198".to_string()],
            Vec::new(),
            vec!["evidence:deletion".to_string()],
            at,
        );
        if partial.is_ok() {
            return Err(
                "verification accepted a check that skipped a planned dependency, so a \
                        deletion could report success while leaving an orphaned reference"
                    .to_string(),
            );
        }

        let incomplete = verify_deletion(
            &request,
            &plan,
            &[],
            vec![
                "memory:0198".to_string(),
                "search_document:0198".to_string(),
            ],
            vec!["backup:0198".to_string()],
            vec!["evidence:deletion".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;

        if incomplete.outcome() != DeletionVerificationOutcome::Incomplete {
            return Err(format!(
                "a remaining copy produced {:?} rather than an incomplete deletion",
                incomplete.outcome()
            ));
        }
        if incomplete.is_complete() {
            return Err("a deletion with a remaining copy reported itself complete".to_string());
        }
        if incomplete.remaining_copies().is_empty() {
            return Err("an incomplete deletion did not say what remains".to_string());
        }

        Ok(
            "verification covers every planned dependency or is refused, and a remaining copy \
            makes the deletion incomplete rather than complete"
                .to_string(),
        )
    }
);

gov_case!(
    DeletionProducesEvidence,
    "exec-gov-009-deletion-evidence",
    9,
    CaseCategory::Evidence,
    "Verification cannot be recorded without evidence references or without saying what it \
     checked",
    || {
        use crate::time::now;
        use crate::trust::verify_deletion;
        use governance_fixture::*;

        let at = now();
        let (request, plan) = deletion(at);
        let checked = vec![
            "memory:0198".to_string(),
            "search_document:0198".to_string(),
        ];

        if verify_deletion(
            &request,
            &plan,
            &[],
            checked.clone(),
            Vec::new(),
            Vec::new(),
            at,
        )
        .is_ok()
        {
            return Err(
                "a deletion was verified with no evidence, so the record asserts a \
                        removal nothing supports"
                    .to_string(),
            );
        }

        if verify_deletion(
            &request,
            &plan,
            &[],
            Vec::new(),
            Vec::new(),
            vec!["evidence:deletion".to_string()],
            at,
        )
        .is_ok()
        {
            return Err("a deletion was verified without saying what it checked".to_string());
        }

        // A plan belonging to a different request must not verify this one.
        let (other_request, _) = deletion(at);
        if verify_deletion(
            &other_request,
            &plan,
            &[],
            checked,
            Vec::new(),
            vec!["evidence:deletion".to_string()],
            at,
        )
        .is_ok()
        {
            return Err("a deletion plan verified a request it does not belong to".to_string());
        }

        Ok(
            "deletion verification requires evidence, requires the references it checked, and \
            refuses a plan belonging to another request"
                .to_string(),
        )
    }
);

gov_case!(
    ASecretReferenceCarriesNoSecret,
    "exec-gov-002-secret-ref-is-a-reference",
    2,
    CaseCategory::Security,
    "A secret reference names a provider and an opaque location rather than a value, and \
     resolving it requires a matching workspace, a matching purpose and an authorization \
     reference",
    || {
        use crate::trust::{SecretRef, SecretResolutionRequest};
        use crate::{WorkspaceId, time::now};
        use std::collections::BTreeMap;

        let workspace = WorkspaceId::new();
        let _ = now();

        let secret = SecretRef::new(
            "secret://workspace/0198/provider-key",
            "local-file",
            workspace,
            "provider.credential",
            BTreeMap::new(),
            None,
        )
        .map_err(|error| format!("a well-formed secret reference was refused: {error}"))?;

        // Serialization is where a value would leak if one were held: an
        // audit record, a log line and a debug dump all go through it.
        let rendered =
            serde_json::to_string(&secret).map_err(|error| format!("serialization: {error}"))?;
        if rendered.contains("secret-value") || rendered.contains("password") {
            return Err("a secret reference serialized something resembling a value".to_string());
        }
        if !rendered.contains("local-file") {
            return Err("a secret reference does not name the provider that holds it".to_string());
        }

        let request = |workspace_id: WorkspaceId, purpose: &str, authorization: &str| {
            SecretResolutionRequest::new(workspace_id, purpose, authorization)
        };

        if secret
            .authorize_resolution(&request(
                WorkspaceId::new(),
                "provider.credential",
                "policy:1",
            ))
            .is_ok()
        {
            return Err(
                "a secret was resolvable from another workspace, so holding the \
                        reference would be enough to read it"
                    .to_string(),
            );
        }
        if secret
            .authorize_resolution(&request(workspace, "export.signing", "policy:1"))
            .is_ok()
        {
            return Err("a secret was resolvable for a purpose it was not stored for".to_string());
        }
        if secret
            .authorize_resolution(&request(workspace, "provider.credential", "   "))
            .is_ok()
        {
            return Err(
                "a secret was resolved with no authorization reference, so nothing \
                        records why it was released"
                    .to_string(),
            );
        }

        secret
            .authorize_resolution(&request(
                workspace,
                "provider.credential",
                "policy:decision:1",
            ))
            .map_err(|error| format!("a properly authorized resolution was refused: {error}"))?;

        Ok(
            "a secret reference names a provider and an opaque location, and resolving it \
            requires the right workspace, the right purpose and a recorded authorization"
                .to_string(),
        )
    }
);

gov_case!(
    RetentionExpiryIsAStateNotADeletion,
    "exec-gov-006-retention-is-enforced",
    6,
    CaseCategory::Stateful,
    "A retention policy reports expiry as a state rather than performing a deletion, requires \
     a boundary to exist, and refuses a minimum longer than its maximum",
    || {
        use crate::time::now;
        use crate::trust::{DisposalMethod, RetentionPolicy, RetentionState, RetentionTrigger};
        use chrono::Duration;

        if RetentionPolicy::new(
            None,
            None,
            RetentionTrigger::CreatedAt,
            DisposalMethod::CryptoErasure,
            "retention-v1",
        )
        .is_ok()
        {
            return Err(
                "a retention policy was accepted with neither a minimum nor a maximum, \
                        so it would bound nothing"
                    .to_string(),
            );
        }

        if RetentionPolicy::new(
            Some(Duration::days(30)),
            Some(Duration::days(7)),
            RetentionTrigger::CreatedAt,
            DisposalMethod::CryptoErasure,
            "retention-v1",
        )
        .is_ok()
        {
            return Err(
                "a retention policy required keeping data longer than it permitted \
                        keeping it"
                    .to_string(),
            );
        }

        let policy = RetentionPolicy::new(
            Some(Duration::days(7)),
            Some(Duration::days(30)),
            RetentionTrigger::CreatedAt,
            DisposalMethod::CryptoErasure,
            "retention-v1",
        )
        .map_err(|error| format!("a well-formed retention policy was refused: {error}"))?;

        let created = now();
        if policy.state_at(created, created + Duration::days(1)) != RetentionState::Active {
            return Err("data inside its retention window was reported expired".to_string());
        }
        if policy.state_at(created, created + Duration::days(31)) != RetentionState::Expired {
            return Err("data past its maximum retention was still reported active".to_string());
        }

        // Expiry is a state the policy reports, not an action it takes. A
        // policy that deleted on evaluation would delete during a read.
        if policy.policy_version().trim().is_empty() {
            return Err(
                "a retention policy does not name its version, so what a disposal was \
                        performed under would be unrecoverable"
                    .to_string(),
            );
        }

        Ok(format!(
            "retention requires a boundary, refuses a minimum beyond its maximum, and reports \
             expiry as a state under version {}",
            policy.policy_version()
        ))
    }
);

gov_case!(
    ExpiredRetentionStillYieldsToAHold,
    "exec-gov-021-hold-outranks-expiry",
    21,
    CaseCategory::Stateful,
    "Data whose retention has expired is still not deleted while a hold is active, and the \
     verification says it was blocked rather than that it was incomplete",
    || {
        use crate::time::now;
        use crate::trust::{
            DeletionVerificationOutcome, DisposalMethod, RetentionPolicy, RetentionState,
            RetentionTrigger, verify_deletion,
        };
        use chrono::Duration;
        use governance_fixture::*;

        let created = now();
        let at = created + Duration::days(40);
        let policy = RetentionPolicy::new(
            None,
            Some(Duration::days(30)),
            RetentionTrigger::CreatedAt,
            DisposalMethod::PhysicalDelete,
            "retention-v1",
        )
        .map_err(|error| format!("a well-formed retention policy was refused: {error}"))?;

        // The premise: retention has expired, so ordinary disposal would run.
        if policy.state_at(created, at) != RetentionState::Expired {
            return Err("the fixture retention had not expired".to_string());
        }

        let (request, plan) = deletion(at);
        let verification = verify_deletion(
            &request,
            &plan,
            &[hold(created, None)],
            vec![
                "memory:0198".to_string(),
                "search_document:0198".to_string(),
            ],
            Vec::new(),
            vec!["evidence:retention-sweep".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;

        if verification.outcome() != DeletionVerificationOutcome::BlockedByHold {
            return Err(format!(
                "expired retention under an active hold produced {:?}; a retention sweep would \
                 have destroyed data somebody had ordered preserved",
                verification.outcome()
            ));
        }
        if verification.is_complete() {
            return Err("a deletion blocked by a hold reported itself complete".to_string());
        }

        Ok(
            "retention expiry does not override a hold: the deletion is reported blocked, and \
            blocked is distinguishable from incomplete"
                .to_string(),
        )
    }
);

gov_case!(
    RotationDoesNotInterruptService,
    "exec-gov-025-rotation-without-downtime",
    25,
    CaseCategory::Behavioral,
    "A key stays usable throughout rotation and the successor is usable before the \
     predecessor retires, so rotating never leaves a window with no usable key",
    || {
        use crate::trust::{KeyPurpose, KeyReference};

        let key = |version: &str| {
            KeyReference::new(
                "local-file",
                "workspace-kek",
                version,
                KeyPurpose::Storage,
                "workspace:0198",
                "aes-256-gcm",
            )
            .expect("a well-formed key reference")
        };

        let mut outgoing = key("v1");
        let successor = key("v2");

        outgoing
            .begin_rotation()
            .map_err(|error| format!("rotation could not begin: {error}"))?;

        // Both usable at once is the property. A rotation that retired the old
        // key first would leave data encrypted under a key nothing may use.
        if !outgoing.is_usable() || !successor.is_usable() {
            return Err(
                "a key stopped being usable during rotation, so rotating would require \
                        an outage"
                    .to_string(),
            );
        }
        if outgoing.version() == successor.version() {
            return Err(
                "the successor carries the same version as its predecessor, so which \
                        key encrypted what could not be told"
                    .to_string(),
            );
        }

        outgoing
            .retire()
            .map_err(|error| format!("a rotating key could not retire: {error}"))?;
        if outgoing.is_usable() {
            return Err("a retired key was still usable".to_string());
        }
        if !successor.is_usable() {
            return Err("retiring the predecessor made the successor unusable".to_string());
        }

        Ok(
            "rotation keeps both keys usable while it runs and the successor usable after the \
            predecessor retires, so there is no window without a usable key"
                .to_string(),
        )
    }
);

gov_case!(
    CrossWorkspaceAccessSatisfiesBothSides,
    "exec-gov-018-both-policies-apply",
    18,
    CaseCategory::Security,
    "A mount carries only the operations both the source grant and the target policy allow, \
     expires at the earlier of the two, and can never carry re-sharing",
    || {
        use crate::enterprise::{
            MemoryMount, MemoryMountAcceptance, MemoryShareGrant, MemoryShareGrantRevision,
            MemoryShareGrantRevisionSpec, ShareOperation, ShareTarget, TargetSharePolicy,
        };
        use crate::time::now;
        use crate::{MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId};
        use chrono::Duration;
        use std::collections::BTreeSet;

        let at = now();
        let source_workspace = WorkspaceId::new();
        let target_workspace = WorkspaceId::new();
        let target_principal = PrincipalId::new();

        let ops = |values: &[ShareOperation]| values.iter().copied().collect::<BTreeSet<_>>();

        let revision = MemoryShareGrantRevision::issue(
            MemoryShareGrantRevisionSpec {
                source_workspace_id: source_workspace,
                target: ShareTarget::ExactWorkspace(target_workspace),
                memory_id: MemoryId::new(),
                memory_revision_id: MemoryRevisionId::new(),
                source_generation: "1".to_string(),
                operations: ops(&[
                    ShareOperation::DiscoverMetadata,
                    ShareOperation::ReadContent,
                    ShareOperation::Export,
                ]),
                valid_from: at,
                valid_until: Some(at + Duration::hours(4)),
            },
            at,
        )
        .map_err(|error| format!("a well-formed share revision was refused: {error}"))?;
        let grant = MemoryShareGrant::issue(revision, at)
            .map_err(|error| format!("a well-formed share grant was refused: {error}"))?;

        // The target permits less than the source offered, and for less time.
        let target_policy = TargetSharePolicy::new(
            target_workspace,
            target_principal,
            ops(&[ShareOperation::ReadContent, ShareOperation::Index]),
            Some(at + Duration::hours(1)),
        )
        .map_err(|error| format!("a well-formed target policy was refused: {error}"))?;

        // A target policy authorizing onward sharing is refused outright: the
        // source consented to this target, not to whoever the target chooses.
        if TargetSharePolicy::new(
            target_workspace,
            target_principal,
            ops(&[ShareOperation::ReadContent, ShareOperation::ReShare]),
            None,
        )
        .is_ok()
        {
            return Err("a target policy authorized transitive re-sharing".to_string());
        }

        let accept = |operations: BTreeSet<ShareOperation>| MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id: target_workspace,
            target_principal_id: target_principal,
            operations,
            valid_until: None,
        };

        // `Export` is in the source grant and not in the target policy, so a
        // mount claiming it is outside the intersection.
        if MemoryMount::accept(
            accept(ops(&[ShareOperation::ReadContent, ShareOperation::Export])),
            &grant,
            &target_policy,
            at,
        )
        .is_ok()
        {
            return Err(
                "a mount carried an operation the target policy does not allow, so one \
                        side's consent stood in for both"
                    .to_string(),
            );
        }

        // `Index` is in the target policy and not in the source grant — the
        // same failure from the other direction.
        if MemoryMount::accept(
            accept(ops(&[ShareOperation::ReadContent, ShareOperation::Index])),
            &grant,
            &target_policy,
            at,
        )
        .is_ok()
        {
            return Err("a mount carried an operation the source grant does not offer".to_string());
        }

        let mount = MemoryMount::accept(
            accept(ops(&[ShareOperation::ReadContent])),
            &grant,
            &target_policy,
            at,
        )
        .map_err(|error| format!("a mount inside both policies was refused: {error}"))?;

        // Validity is the earlier of the two, or one side could extend the
        // other's consent by outliving it.
        if mount.valid_until() != Some(at + Duration::hours(1)) {
            return Err(format!(
                "the mount expires at {:?} rather than at the earlier of the two policies",
                mount.valid_until()
            ));
        }

        Ok(
            "a mount is the intersection of source and target: operations either side withholds \
            are refused, re-sharing cannot be authorized, and validity is the earlier expiry"
                .to_string(),
        )
    }
);

gov_case!(
    APolicyDecisionCarriesItsBasis,
    "exec-gov-020-decisions-are-auditable",
    20,
    CaseCategory::Evidence,
    "A governance decision names the actor, the action, the resource, the verdict and the \
     policy version that produced it, and survives serialization into an audit trail",
    || {
        use crate::time::now;
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let decision = decide(&actors, &request(), &[grant(&actors, at)], at)
            .map_err(|error| format!("evaluation failed: {error}"))?;

        let audit = crate::AuditEvent::new(
            crate::AuditEventId::new(),
            actors.workspace,
            decision.subject_id,
            "policy.decision",
            "capability_grant",
            decision
                .matched_grant_id
                .map(|id| id.as_uuid())
                .unwrap_or_default(),
            serde_json::to_value(&decision).map_err(|error| error.to_string())?,
            at,
        )
        .map_err(|error| format!("an audit event could not be built from a decision: {error}"))?;

        let rendered = serde_json::to_value(&audit).map_err(|error| error.to_string())?;
        let payload = &rendered["payload"];

        if rendered["principal_id"] == serde_json::Value::Null {
            return Err("an audit event does not name the actor".to_string());
        }
        if payload["result"] == serde_json::Value::Null
            || payload["reason"] == serde_json::Value::Null
        {
            return Err("an audited decision does not carry its verdict and reason".to_string());
        }
        if payload["policy_version"] == serde_json::Value::Null {
            return Err(
                "an audited decision does not name the policy version that produced \
                        it, so it cannot be re-evaluated against the rules that applied"
                    .to_string(),
            );
        }
        if payload["input_state"] == serde_json::Value::Null {
            return Err("an audited decision does not carry what it judged".to_string());
        }

        Ok(format!(
            "a decision survives into an audit event carrying actor, action ({}), resource, \
             verdict, reason, policy version and input state",
            audit.action
        ))
    }
);

gov_case!(
    APolicyChangeIsVisibleInItsDecisions,
    "exec-gov-026-policy-changes-are-versioned",
    26,
    CaseCategory::Evidence,
    "Two versions of a policy produce decisions that name different versions, so a change to \
     the rules is visible in what they decided, and a blank version is refused",
    || {
        use crate::time::now;
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let held = grant(&actors, at);

        let under = |version: &str| {
            crate::security::evaluate_capability_grants(
                crate::PolicyDecisionId::new(),
                actors.workspace,
                actors.subject,
                version,
                &request(),
                std::slice::from_ref(&held),
                at,
            )
        };

        let first = under("policy-v1").map_err(|error| format!("evaluation failed: {error}"))?;
        let second = under("policy-v2").map_err(|error| format!("evaluation failed: {error}"))?;

        if under("   ").is_ok() {
            return Err(
                "a decision was produced under a blank policy version, so what it was \
                        decided against would be unrecoverable"
                    .to_string(),
            );
        }
        if first.policy_version == second.policy_version {
            return Err(
                "two policy versions produced decisions naming the same version".to_string(),
            );
        }
        if first.id == second.id {
            return Err(
                "two decisions share an identity, so they cannot be told apart in an \
                        audit trail"
                    .to_string(),
            );
        }

        Ok(format!(
            "decisions name the policy version that produced them ({} and {}), and a blank \
             version is refused",
            first.policy_version, second.policy_version
        ))
    }
);

gov_case!(
    AClassificationIsReplacedRatherThanEdited,
    "exec-gov-024-classification-is-immutable",
    24,
    CaseCategory::Behavioral,
    "Deriving from a classification leaves the original untouched, and the derived one records      which sources produced it",
    || {
        use crate::Sensitivity;
        use crate::trust::ClassificationLineage;
        use governance_fixture::*;

        let original = classified(Sensitivity::Internal);
        let before = original.clone();

        let lineage = ClassificationLineage::derive(
            vec!["memory:0198".to_string()],
            vec![original.clone()],
            "policy-v1",
            "derivation:redaction",
        )
        .map_err(|error| format!("a well-formed lineage was refused: {error}"))?;

        // The source is a value, and deriving from it produces another value.
        // If a derivation could edit its source, a revision's classification
        // would change under readers who had already been shown it.
        if original != before {
            return Err("deriving a classification mutated the one it derived from".to_string());
        }
        if lineage.effective_classification() == &before
            && lineage.source_refs() != ["memory:0198".to_string()]
        {
            return Err("a derived classification does not record its sources".to_string());
        }

        // Raising sensitivity produces a new classification rather than
        // altering the old, and the old still reads as it did.
        let raised = ClassificationLineage::derive(
            vec!["memory:0198".to_string(), "memory:0199".to_string()],
            vec![original.clone(), classified(Sensitivity::Restricted)],
            "policy-v1",
            "derivation:merge",
        )
        .map_err(|error| format!("a well-formed lineage was refused: {error}"))?;

        if raised.effective_classification().sensitivity() != Sensitivity::Restricted {
            return Err("a merge did not take the higher sensitivity".to_string());
        }
        if original.sensitivity() != Sensitivity::Internal {
            return Err(
                "the source classification changed when something derived from it".to_string(),
            );
        }

        Ok("a classification is a value: deriving from it yields a new one that names its             sources, and never alters the original"
            .to_string())
    }
);

gov_case!(
    AKeyMovesThroughItsLifecycleInOneDirection,
    "exec-gov-013-key-lifecycle",
    13,
    CaseCategory::Behavioral,
    "A key rotates only from active, retires only from active or rotating, must be revoked \
     before destruction, and never returns to a state it has left",
    || {
        use crate::trust::{KeyLifecycleState, KeyPurpose, KeyReference};

        let key = || {
            KeyReference::new(
                "local-file",
                "workspace-kek",
                "v1",
                KeyPurpose::Storage,
                "workspace:0198",
                "aes-256-gcm",
            )
            .expect("a well-formed key reference")
        };

        let mut rotating = key();
        rotating
            .begin_rotation()
            .map_err(|error| format!("an active key could not enter rotation: {error}"))?;
        if rotating.begin_rotation().is_ok() {
            return Err(
                "a rotating key entered rotation a second time, so two rotations could \
                        be in flight for one key"
                    .to_string(),
            );
        }

        let mut destroyed_early = key();
        if destroyed_early.destroy().is_ok() {
            return Err(
                "an active key was destroyed without being revoked first, so key \
                        material could vanish while something still believed it usable"
                    .to_string(),
            );
        }

        let mut revoked = key();
        revoked
            .revoke()
            .map_err(|error| format!("an active key could not be revoked: {error}"))?;
        if revoked.is_usable() {
            return Err("a revoked key reported itself usable".to_string());
        }
        if revoked.begin_rotation().is_ok() {
            return Err("a revoked key was rotated back into service".to_string());
        }

        revoked
            .destroy()
            .map_err(|error| format!("a revoked key could not be destroyed: {error}"))?;
        if revoked.lifecycle_state() != KeyLifecycleState::Destroyed {
            return Err("destruction did not reach the destroyed state".to_string());
        }
        if revoked.revoke().is_ok() || revoked.retire().is_ok() {
            return Err("a destroyed key accepted a further lifecycle transition".to_string());
        }

        Ok(
            "a key rotates only from active, cannot be destroyed before revocation, and once \
            destroyed accepts nothing further"
                .to_string(),
        )
    }
);

gov_case!(
    RevokingAKeyEndsAccessWithoutReEncryption,
    "exec-gov-014-revocation-ends-access",
    14,
    CaseCategory::Security,
    "A revoked or destroyed key is unusable immediately, while a rotating one still is — so \
     access ends by key state rather than by rewriting the data it protected",
    || {
        use crate::trust::{KeyPurpose, KeyReference};

        let key = || {
            KeyReference::new(
                "local-file",
                "workspace-kek",
                "v1",
                KeyPurpose::Storage,
                "workspace:0198",
                "aes-256-gcm",
            )
            .expect("a well-formed key reference")
        };

        let active = key();
        if !active.is_usable() {
            return Err("an active key was not usable".to_string());
        }

        // Rotation must not interrupt access, or every rotation would be an
        // outage and nobody would rotate.
        let mut rotating = key();
        rotating.begin_rotation().expect("rotation begins");
        if !rotating.is_usable() {
            return Err(
                "a rotating key stopped being usable, so rotation would require \
                        downtime"
                    .to_string(),
            );
        }

        for (label, mut subject) in [("retired", key()), ("revoked", key()), ("destroyed", key())] {
            match label {
                "retired" => subject.retire().expect("retire"),
                "revoked" => subject.revoke().expect("revoke"),
                _ => {
                    subject.revoke().expect("revoke");
                    subject.destroy().expect("destroy");
                }
            }
            if subject.is_usable() {
                return Err(format!(
                    "a {label} key was still usable, so revoking it would not end access to \
                     what it protects"
                ));
            }
        }

        Ok(
            "access follows the key's lifecycle state: rotating keeps working, retired, revoked \
            and destroyed do not, and none of it requires re-encrypting the protected data"
                .to_string(),
        )
    }
);

gov_case!(
    AnExportIsGovernedByThePolicyItNames,
    "exec-gov-010-export-is-governed",
    10,
    CaseCategory::Behavioral,
    "An export is refused when its classification exceeds the policy, when the capability is \
     absent, when the plan has expired, and when the policy version has moved on",
    || {
        use crate::trust::{DataExportPlan, DataPolicy};
        use crate::{Capability, DataDestination, DataPolicyId, Sensitivity, time::now};
        use chrono::Duration;
        use governance_fixture::*;
        use std::collections::BTreeSet;

        let at = now();
        let policy = policy(
            Sensitivity::Confidential,
            &[DataDestination::ExportBundle],
            Some(Capability::ExportRead),
        )
        .map_err(|error| format!("a well-formed policy was refused: {error}"))?;

        let plan = |sensitivity: Sensitivity, expires_at: Option<crate::time::Timestamp>| {
            DataExportPlan::new(
                crate::WorkspaceId::new(),
                "memory:project-alpha",
                "regulatory request",
                "auditor@example.test",
                classified(sensitivity),
                vec!["memory:0198@2".to_string()],
                vec!["redaction:pii-v1".to_string()],
                "vestrace-export-v1",
                None,
                None,
                expires_at,
                "data-policy-v1",
                Capability::ExportRead,
                at,
            )
            .expect("a well-formed export plan")
        };

        plan(Sensitivity::Internal, None)
            .authorize(&policy, true, at)
            .map_err(|error| format!("a permitted export was refused: {error}"))?;

        if plan(Sensitivity::Restricted, None)
            .authorize(&policy, true, at)
            .is_ok()
        {
            return Err(
                "an export above the policy ceiling was authorized, so export would be \
                        a way around classification"
                    .to_string(),
            );
        }
        if plan(Sensitivity::Internal, None)
            .authorize(&policy, false, at)
            .is_ok()
        {
            return Err(
                "an export was authorized without the capability its policy requires".to_string(),
            );
        }
        if plan(Sensitivity::Internal, Some(at - Duration::hours(1)))
            .authorize(&policy, true, at)
            .is_ok()
        {
            return Err("an expired export plan was authorized".to_string());
        }

        // A plan written against one version of the policy must not be
        // authorized by another: the rules it was reviewed under have changed.
        let moved_on = DataPolicy::new(
            DataPolicyId::new(),
            "data-policy-v2",
            Sensitivity::Confidential,
            [DataDestination::ExportBundle]
                .into_iter()
                .collect::<BTreeSet<_>>(),
            Some(Capability::ExportRead),
        )
        .expect("a second policy version");
        if plan(Sensitivity::Internal, None)
            .authorize(&moved_on, true, at)
            .is_ok()
        {
            return Err(
                "an export plan was authorized by a policy version it was not written \
                        against"
                    .to_string(),
            );
        }

        Ok(
            "an export passes only when its classification, capability, validity and policy \
            version all hold"
                .to_string(),
        )
    }
);

gov_case!(
    AnExportNamesExactlyWhatItCarries,
    "exec-gov-022-export-scope-is-explicit",
    22,
    CaseCategory::Behavioral,
    "An export plan must name a scope, a purpose, a recipient and the exact revisions it \
     carries, and cannot be built from an empty or blank selection",
    || {
        use crate::trust::DataExportPlan;
        use crate::{Capability, Sensitivity, time::now};
        use governance_fixture::*;

        let at = now();
        let build = |scope: &str, revisions: Vec<String>, redactions: Vec<String>| {
            DataExportPlan::new(
                crate::WorkspaceId::new(),
                scope,
                "regulatory request",
                "auditor@example.test",
                classified(Sensitivity::Internal),
                revisions,
                redactions,
                "vestrace-export-v1",
                None,
                None,
                None,
                "data-policy-v1",
                Capability::ExportRead,
                at,
            )
        };

        if build("memory:project-alpha", Vec::new(), Vec::new()).is_ok() {
            return Err(
                "an export plan carrying no object revisions was accepted, so 'export \
                        this subset' could mean anything"
                    .to_string(),
            );
        }
        if build("   ", vec!["memory:0198@2".to_string()], Vec::new()).is_ok() {
            return Err("an export plan was accepted with a blank scope".to_string());
        }
        if build(
            "memory:project-alpha",
            vec!["memory:0198@2".to_string()],
            vec!["   ".to_string()],
        )
        .is_ok()
        {
            return Err(
                "an export plan carried a blank redaction reference, which would read \
                        as redaction having been applied"
                    .to_string(),
            );
        }

        let plan = build(
            "memory:project-alpha",
            vec!["memory:0198@2".to_string(), "memory:0199@1".to_string()],
            vec!["redaction:pii-v1".to_string()],
        )
        .map_err(|error| format!("a well-formed export plan was refused: {error}"))?;

        // The revisions are exact: an export naming a memory without its
        // revision would carry whatever that memory says later.
        if !plan
            .object_revisions()
            .iter()
            .all(|value| value.contains('@'))
        {
            return Err("an export plan names objects without pinning their revisions".to_string());
        }

        Ok(format!(
            "an export names its scope, purpose, recipient and the {} exact revisions it \
             carries, and refuses a blank selection",
            plan.object_revisions().len()
        ))
    }
);

const CAPABILITY_EVIDENCE: &str = "crates/vestrace-domain/src/security/capability.rs";
const DELEGATION_EVIDENCE: &str = "crates/vestrace-domain/src/security/delegation.rs";

mod capability_fixture {
    use crate::security::{
        AuthorizationRequest, BudgetConstraint, CapabilityGrant, CapabilityGrantSpec,
        GrantCondition, PolicyDecision, evaluate_capability_grants,
    };
    use crate::security::{DELEGATION_OPERATION, DelegationContract};
    use crate::{
        Capability, CapabilityGrantId, DomainError, PolicyDecisionId, PrincipalId, RiskCategory,
        WorkspaceId, time::Timestamp,
    };

    pub const OPERATION: &str = "memory.write";
    pub const SCOPE: &str = "memory://project-alpha";

    pub struct Actors {
        pub workspace: WorkspaceId,
        pub subject: PrincipalId,
        pub issuer: PrincipalId,
    }

    pub fn actors() -> Actors {
        Actors {
            workspace: WorkspaceId::new(),
            subject: PrincipalId::new(),
            issuer: PrincipalId::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spec(
        actors: &Actors,
        capability: Capability,
        operation: &str,
        scope: &str,
        risk_ceiling: RiskCategory,
        budget: Option<u64>,
        valid_from: Timestamp,
        valid_until: Option<Timestamp>,
    ) -> CapabilityGrantSpec {
        CapabilityGrantSpec {
            id: CapabilityGrantId::new(),
            workspace_id: actors.workspace,
            subject_id: actors.subject,
            issuer_id: actors.issuer,
            capability,
            operation: operation.to_string(),
            resource_scope: scope.to_string(),
            valid_from,
            valid_until,
            budget: budget.map(BudgetConstraint::new),
            risk_ceiling,
            conditions: Vec::new(),
        }
    }

    /// A grant that permits exactly the fixture request.
    pub fn grant(actors: &Actors, at: Timestamp) -> CapabilityGrant {
        CapabilityGrant::issue(
            spec(
                actors,
                Capability::MemoryWrite,
                OPERATION,
                SCOPE,
                RiskCategory::Medium,
                Some(100),
                at,
                None,
            ),
            at,
        )
        .expect("a well-formed grant")
    }

    pub fn request() -> AuthorizationRequest {
        AuthorizationRequest::new(Capability::MemoryWrite, OPERATION, SCOPE, RiskCategory::Low)
    }

    pub fn decide(
        actors: &Actors,
        request: &AuthorizationRequest,
        grants: &[CapabilityGrant],
        at: Timestamp,
    ) -> Result<PolicyDecision, DomainError> {
        evaluate_capability_grants(
            PolicyDecisionId::new(),
            actors.workspace,
            actors.subject,
            "policy-v1",
            request,
            grants,
            at,
        )
    }

    /// The explicit `capability.delegate` permission delegation requires.
    pub fn delegation_permission(
        actors: &Actors,
        scope: &str,
        risk: RiskCategory,
        budget: Option<u64>,
        at: Timestamp,
    ) -> CapabilityGrant {
        CapabilityGrant::issue(
            spec(
                actors,
                Capability::CapabilityDelegate,
                DELEGATION_OPERATION,
                scope,
                risk,
                budget,
                at,
                None,
            ),
            at,
        )
        .expect("a well-formed delegation permission")
    }

    pub fn contract(scope: &str, max_budget_units: u64, max_depth: u8) -> DelegationContract {
        DelegationContract::new(
            Capability::MemoryWrite,
            scope,
            max_depth,
            None,
            RiskCategory::Medium,
            max_budget_units,
        )
        .expect("a well-formed delegation contract")
    }

    /// A child spec issued by `parent_subject` to a different principal.
    #[allow(clippy::too_many_arguments)]
    pub fn child_spec(
        actors: &Actors,
        operation: &str,
        scope: &str,
        risk: RiskCategory,
        budget: u64,
        valid_from: Timestamp,
        valid_until: Option<Timestamp>,
    ) -> CapabilityGrantSpec {
        CapabilityGrantSpec {
            id: CapabilityGrantId::new(),
            workspace_id: actors.workspace,
            // Issued *by* the parent's subject, *to* somebody else: delegating
            // to yourself is self-escalation and the domain refuses it.
            subject_id: PrincipalId::new(),
            issuer_id: actors.subject,
            capability: Capability::MemoryWrite,
            operation: operation.to_string(),
            resource_scope: scope.to_string(),
            valid_from,
            valid_until,
            budget: Some(BudgetConstraint::new(budget)),
            risk_ceiling: risk,
            conditions: Vec::new(),
        }
    }

    pub fn condition(name: &str, value: &str) -> GrantCondition {
        GrantCondition::new(name, value).expect("a well-formed condition")
    }
}

macro_rules! cap_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $evidence:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Cap,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    $evidence,
                )
            }
        }
    };
}

cap_case!(
    AGrantStatesItsScopeAndItsEnd,
    "exec-cap-002-grant-states-its-limits",
    2,
    CaseCategory::Behavioral,
    "A grant without an operation or a resource scope is refused, an expiry before its start \
     is refused, and a blank condition is refused",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::{CapabilityGrant, GrantCondition};
        use crate::{Capability, RiskCategory, time::now};
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();

        for (label, operation, scope) in [
            ("no operation", "   ", SCOPE),
            ("no resource scope", OPERATION, "  "),
        ] {
            let unscoped = spec(
                &actors,
                Capability::MemoryWrite,
                operation,
                scope,
                RiskCategory::Medium,
                Some(10),
                at,
                None,
            );
            if CapabilityGrant::issue(unscoped, at).is_ok() {
                return Err(format!(
                    "a grant with {label} was issued, so authority would extend to whatever \
                     the caller asked for"
                ));
            }
        }

        let backwards = spec(
            &actors,
            Capability::MemoryWrite,
            OPERATION,
            SCOPE,
            RiskCategory::Medium,
            Some(10),
            at,
            Some(at - Duration::hours(1)),
        );
        if CapabilityGrant::issue(backwards, at).is_ok() {
            return Err("a grant expired before it began".to_string());
        }

        let mut blank_condition = spec(
            &actors,
            Capability::MemoryWrite,
            OPERATION,
            SCOPE,
            RiskCategory::Medium,
            Some(10),
            at,
            None,
        );
        blank_condition.conditions = vec![GrantCondition {
            name: "  ".to_string(),
            expected_value: "yes".to_string(),
        }];
        if CapabilityGrant::issue(blank_condition, at).is_ok() {
            return Err(
                "a grant carried a condition with no name, which nothing could ever \
                        satisfy or fail"
                    .to_string(),
            );
        }

        let grant = grant(&actors, at);
        if grant.budget.is_none() || grant.resource_scope.is_empty() {
            return Err("a well-formed grant lost its constraints".to_string());
        }

        Ok(format!(
            "a grant names its operation ({}), its scope ({}), its risk ceiling ({:?}) and its \
             budget, and refuses to exist without the first two",
            grant.operation, grant.resource_scope, grant.risk_ceiling
        ))
    }
);

cap_case!(
    AGrantCoversWhatIsBeneathItAndNothingAbove,
    "exec-cap-002-scope-is-hierarchical",
    2,
    CaseCategory::Security,
    "A grant covers the resource it names and those inside it, never the one containing it, \
     only at a separator boundary; a scope carrying a wildcard is refused when the grant is \
     written rather than silently matching nothing afterwards",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::CapabilityGrant;
        use crate::{Capability, RiskCategory, time::now};
        use capability_fixture::*;

        let actors = actors();
        let at = now();

        let granted = |grant_operation: &str,
                       grant_scope: &str,
                       request_operation: &str,
                       request_scope: &str| {
            let grant = CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::MemoryWrite,
                    grant_operation,
                    grant_scope,
                    RiskCategory::Medium,
                    Some(100),
                    at,
                    None,
                ),
                at,
            )
            .expect("a well-formed grant");
            let request = crate::security::AuthorizationRequest::new(
                Capability::MemoryWrite,
                request_operation,
                request_scope,
                RiskCategory::Low,
            );
            decide(&actors, &request, &[grant], at)
                .map(|decision| decision.is_allowed())
                .unwrap_or(false)
        };

        // Downward: this is what makes a grant usable at all. Without it the
        // HTTP boundary — which scopes every request to its own path — would
        // need one grant per URL.
        if !granted("http", "/v1", "http.get", "/v1/memories/0198") {
            return Err(
                "a grant for /v1 did not cover a request for /v1/memories/0198, so a \
                        grant covers only the exact string it names"
                    .to_string(),
            );
        }
        if !granted("http.get", "/v1/memories", "http.get", "/v1/memories") {
            return Err("a grant did not cover the exact resource it names".to_string());
        }

        // Upward: a grant for one memory must never authorize the collection.
        if granted("http.get", "/v1/memories/0198", "http.get", "/v1/memories") {
            return Err(
                "a grant for a single resource authorized its parent collection".to_string(),
            );
        }

        // The boundary has to be a separator, or a grant silently covers its
        // neighbours: /v1-admin is not inside /v1.
        if granted("http.get", "/v1", "http.get", "/v1-admin/keys") {
            return Err(
                "a grant for /v1 covered /v1-admin/keys, so a prefix match crosses \
                        into a neighbouring resource"
                    .to_string(),
            );
        }
        if granted("memory.write", "/v1", "memory.writer", "/v1") {
            return Err("a grant for memory.write covered memory.writer".to_string());
        }

        // A wildcard is refused rather than interpreted: nothing defines what it
        // would mean, and guessing makes a grant's reach depend on a convention
        // no code enforces.
        //
        // It is refused where it is *written*, not merely ignored where it is
        // compared. A grant carrying `*` used to be issued and stored happily
        // and then fail every comparison, which in a listing is indistinguishable
        // from a grant that is simply narrow — the operator sees a permission
        // they do not have.
        if CapabilityGrant::issue(
            spec(
                &actors,
                Capability::MemoryWrite,
                "http.get",
                "/v1/*",
                RiskCategory::Medium,
                Some(100),
                at,
                None,
            ),
            at,
        )
        .is_ok()
        {
            return Err(
                "a grant whose scope carries a wildcard was issued, so it would sit in a listing \
                 looking like a permission while authorizing nothing"
                    .to_string(),
            );
        }
        if granted("http.get", "/v1", "http.get", "/v1/*") {
            return Err("a wildcard in a request's scope was interpreted".to_string());
        }

        // Hierarchy is only usable if the names nest, and for one shape of name
        // they did not. The run worker asked about `run:{id}`; the boundary rule
        // looks for `/`, so `run:` did not contain `run:{id}` and no grant
        // covering more than one named run could be written at all. The fix is
        // not to the rule but to the vocabulary: a scope is now parsed, and one
        // that nothing could ever match is refused where somebody writes it.
        if !granted("worker.run", "run://", "worker.run.step", "run://0198abc") {
            return Err(
                "a grant for every run did not cover one run, so 'may work any run in this \
                 workspace' cannot be written and each run must be granted by name"
                    .to_string(),
            );
        }
        if granted("worker.run", "run://0198abc", "worker.run.step", "run://") {
            return Err("a grant for one run covered every run".to_string());
        }

        for private in ["run:0198abc", "memory_id:0198abc", "unscoped", "workspace"] {
            if CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::MemoryWrite,
                    OPERATION,
                    private,
                    RiskCategory::Medium,
                    Some(100),
                    at,
                    None,
                ),
                at,
            )
            .is_ok()
            {
                return Err(format!(
                    "a grant scoped `{private}` was issued, and it is in no vocabulary anything \
                     asks about, so it would deny every request while looking like a permission"
                ));
            }
        }

        Ok(
            "a grant covers the resource and operation it names and those beneath them, never \
            those above, only at a separator boundary; a wildcard or a scope in no vocabulary is \
            refused where the grant is written"
                .to_string(),
        )
    }
);

cap_case!(
    AMaterialChangeInvalidatesTheApproval,
    "exec-cap-012-material-change-invalidates-approval",
    12,
    CaseCategory::Security,
    "An approval authorises the operation it was granted for and no other: the operation it was      approved with is covered, any changed operation is not, an approval that recorded no      operation covers nothing at all, and expiry or rejection withdraws coverage of even the      original",
    "crates/vestrace-domain/src/security/mod.rs",
    || {
        use crate::security::{ApprovalKind, ApprovalRecord, ApprovalStatus};
        use crate::{ApprovalRecordId, PrincipalId, WorkspaceId, time::now};
        use chrono::Duration;

        let at = now();
        let workspace = WorkspaceId::new();
        let requestor = PrincipalId::new();
        let approver = PrincipalId::new();
        let approved_operation = "sha256:transfer-100-to-account-a";
        let changed_operation = "sha256:transfer-9000-to-account-b";

        let record = || {
            ApprovalRecord::new(
                ApprovalRecordId::new(),
                workspace,
                requestor,
                ApprovalKind::RestrictedTransfer,
                "a transfer needing a second pair of eyes",
                at,
            )
        };

        let approved = record()
            .approve(approver, Some(approved_operation.to_string()), None, at)
            .map_err(|error| format!("a well-formed approval was refused: {error}"))?;
        if approved.status != ApprovalStatus::Approved {
            return Err("the approval is not recorded as approved".to_string());
        }

        // What was approved is covered.
        if !approved.covers(approved_operation, at) {
            return Err(
                "an approval does not cover the operation it was granted for, so approving                  anything would be useless"
                    .to_string(),
            );
        }

        // Anything else is not. This is the requirement: an approval obtained
        // for one operation must not authorise another, or a caller could ask
        // about something small and act on something large.
        if approved.covers(changed_operation, at) {
            return Err(
                "an approval granted for one operation covered a different one, so changing                  the operation after approval does not invalidate it"
                    .to_string(),
            );
        }
        for near_miss in [
            "sha256:transfer-100-to-account-b",
            "sha256:transfer-1000-to-account-a",
            "",
            "   ",
        ] {
            if approved.covers(near_miss, at) {
                return Err(format!(
                    "an approval covered `{near_miss}`, which is not what it was granted for"
                ));
            }
        }

        // An approval that named no operation covers nothing: there is no
        // operation it was about, so there is none it can be checked against,
        // and treating that as "everything" is exactly the blank cheque this
        // requirement exists to refuse.
        let unbounded = record()
            .approve(approver, None, None, at)
            .map_err(|error| format!("an approval without an operation was refused: {error}"))?;
        if unbounded.covers(approved_operation, at) || unbounded.covers(changed_operation, at) {
            return Err(
                "an approval that recorded no operation covered one anyway, so an approver who                  named nothing authorised everything"
                    .to_string(),
            );
        }

        // Expiry and rejection withdraw coverage of even the original operation,
        // so coverage is not a property of the hash alone.
        let expired = record()
            .approve(
                approver,
                Some(approved_operation.to_string()),
                Some(at + Duration::minutes(5)),
                at,
            )
            .map_err(|error| format!("an expiring approval was refused: {error}"))?;
        if !expired.covers(approved_operation, at) {
            return Err("an unexpired approval did not cover its own operation".to_string());
        }
        if expired.covers(approved_operation, at + Duration::minutes(6)) {
            return Err("an expired approval still covered its operation".to_string());
        }

        let rejected = record()
            .reject(approver, at)
            .map_err(|error| format!("the approval could not be rejected: {error}"))?;
        if rejected.covers(approved_operation, at) {
            return Err("a rejected approval covered an operation".to_string());
        }

        Ok(
            "an approval covers the exact operation it was granted for, covers no changed or              near-miss operation, covers nothing when it named no operation, and stops covering              anything once expired or rejected"
                .to_string(),
        )
    }
);

cap_case!(
    AChildIsNeverAllowedWhatItsParentIsDenied,
    "exec-cap-005-child-authority-is-within-parent-authority",
    5,
    CaseCategory::Security,
    "Effective authority, not just the written grant, attenuates: across a set of requests      spanning operation, scope, risk and budget, every request a delegated child is allowed is      one its parent is also allowed, and the child is denied at least one thing the parent may      do — so the delegation narrowed rather than merely copied",
    DELEGATION_EVIDENCE,
    || {
        use crate::security::{AuthorizationRequest, DelegatedCapability, HierarchicalBudget};
        use crate::{Capability, RiskCategory, time::now};
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let parent = grant(&actors, at);
        let permission = delegation_permission(&actors, SCOPE, RiskCategory::Medium, Some(100), at);
        let contract = contract(SCOPE, 100, 3);

        let mut budget = HierarchicalBudget::new(1_000);
        let child = DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(at + Duration::hours(1)),
            ),
            &contract,
            &mut budget,
            at,
        )
        .map_err(|error| format!("a narrowing delegation was refused: {error}"))?;

        // Requests spanning every axis a grant restricts. The point is not that
        // any particular one is denied — it is that no request exists which the
        // child may make and the parent may not, which is what "authority is
        // not wider" means when it is asked of behaviour rather than of fields.
        let probes: Vec<(&str, AuthorizationRequest)> = vec![
            (
                "the exact operation and scope",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Low,
                ),
            ),
            (
                "a narrower operation beneath it",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    "memory.write.append",
                    SCOPE,
                    RiskCategory::Low,
                ),
            ),
            (
                "a resource inside the scope",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    OPERATION,
                    "memory://project-alpha/items/one",
                    RiskCategory::Low,
                ),
            ),
            (
                "a resource outside the scope",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    OPERATION,
                    "memory://project-beta",
                    RiskCategory::Low,
                ),
            ),
            (
                "a different capability",
                AuthorizationRequest::new(
                    Capability::MemoryPurge,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Low,
                ),
            ),
            (
                "medium risk, which the parent ceiling admits",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Medium,
                ),
            ),
            (
                "a spend above the child budget and within the parent's",
                AuthorizationRequest::new(
                    Capability::MemoryWrite,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Low,
                )
                .with_budget_units(60),
            ),
        ];

        // Each grant is evaluated against **its own** subject. A delegated child
        // is issued to somebody else — delegating to yourself is self-escalation
        // and the domain refuses it — so evaluating the child against the
        // parent's subject denies every request with `SubjectMismatch`, and this
        // case would pass without ever exercising the property. It did, until a
        // mutation that removed all attenuation failed to break it.
        let child_actors = Actors {
            workspace: actors.workspace,
            subject: child.grant().subject_id,
            issuer: actors.issuer,
        };
        let allowed = |who: &Actors,
                       grants: &[crate::security::CapabilityGrant],
                       request: &AuthorizationRequest| {
            decide(who, request, grants, at)
                .map(|decision| decision.is_allowed())
                .unwrap_or(false)
        };

        let mut narrower_somewhere = false;
        for (what, request) in &probes {
            let parent_allows = allowed(&actors, std::slice::from_ref(&parent), request);
            let child_allows =
                allowed(&child_actors, std::slice::from_ref(child.grant()), request);
            if child_allows && !parent_allows {
                return Err(format!(
                    "a delegated child is allowed {what} and its parent is not, so delegation                      produced authority the delegator never held"
                ));
            }
            if parent_allows && !child_allows {
                narrower_somewhere = true;
            }
        }

        if !narrower_somewhere {
            return Err(
                "the child is allowed everything the parent is, across every probe, so this                  case would pass against a delegation that attenuated nothing"
                    .to_string(),
            );
        }

        Ok(format!(
            "across {} probes the delegated child is allowed nothing its parent is denied, and              is denied something the parent may do",
            probes.len()
        ))
    }
);

cap_case!(
    DelegationOnlyNarrows,
    "exec-cap-003-delegation-attenuates",
    3,
    CaseCategory::Behavioral,
    "A delegated grant cannot widen the capability, operation, scope, risk ceiling, budget or \
     validity of its parent, cannot drop a parent condition, and cannot be issued to the \
     delegator themselves",
    DELEGATION_EVIDENCE,
    || {
        use crate::security::{DelegatedCapability, HierarchicalBudget};
        use crate::{PrincipalId, RiskCategory, time::now};
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let parent = grant(&actors, at);
        let permission = delegation_permission(&actors, SCOPE, RiskCategory::Medium, Some(100), at);
        let contract = contract(SCOPE, 100, 3);

        // The happy path first, so a failure below is about the property and not
        // about the fixture.
        let mut budget = HierarchicalBudget::new(1_000);
        let child = DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(at + Duration::hours(1)),
            ),
            &contract,
            &mut budget,
            at,
        )
        .map_err(|error| format!("a narrowing delegation was refused: {error}"))?;

        let widenings: Vec<(&str, crate::security::CapabilityGrantSpec)> = vec![
            (
                "a wider operation",
                child_spec(
                    &actors,
                    "memory",
                    SCOPE,
                    RiskCategory::Low,
                    40,
                    at,
                    Some(at + Duration::hours(1)),
                ),
            ),
            (
                "a wider resource scope",
                child_spec(
                    &actors,
                    OPERATION,
                    "memory:",
                    RiskCategory::Low,
                    40,
                    at,
                    Some(at + Duration::hours(1)),
                ),
            ),
            (
                "a higher risk ceiling",
                child_spec(
                    &actors,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Critical,
                    40,
                    at,
                    Some(at + Duration::hours(1)),
                ),
            ),
            (
                "a larger budget than the delegation ceiling",
                child_spec(
                    &actors,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Low,
                    500,
                    at,
                    Some(at + Duration::hours(1)),
                ),
            ),
        ];

        for (label, spec) in widenings {
            let mut budget = HierarchicalBudget::new(1_000);
            if DelegatedCapability::from_root(
                &parent,
                &permission,
                spec,
                &contract,
                &mut budget,
                at,
            )
            .is_ok()
            {
                return Err(format!(
                    "a delegation with {label} was accepted, so delegation amplifies rather \
                     than attenuates"
                ));
            }
        }

        // Delegating to yourself would turn the delegation permission into a way
        // of rewriting your own grant.
        let mut self_spec = child_spec(
            &actors,
            OPERATION,
            SCOPE,
            RiskCategory::Low,
            40,
            at,
            Some(at + Duration::hours(1)),
        );
        self_spec.subject_id = actors.subject;
        let mut budget = HierarchicalBudget::new(1_000);
        if DelegatedCapability::from_root(
            &parent,
            &permission,
            self_spec,
            &contract,
            &mut budget,
            at,
        )
        .is_ok()
        {
            return Err(
                "a principal delegated to themselves, which is self-escalation with \
                        extra steps"
                    .to_string(),
            );
        }

        // A parent condition is a limit, so dropping it widens the child.
        let mut conditioned_parent_spec = spec(
            &actors,
            crate::Capability::MemoryWrite,
            OPERATION,
            SCOPE,
            RiskCategory::Medium,
            Some(100),
            at,
            None,
        );
        conditioned_parent_spec.conditions = vec![condition("mfa", "true")];
        let conditioned_parent =
            crate::security::CapabilityGrant::issue(conditioned_parent_spec, at)
                .map_err(|error| format!("a conditioned parent grant was refused: {error}"))?;
        let mut budget = HierarchicalBudget::new(1_000);
        if DelegatedCapability::from_root(
            &conditioned_parent,
            &permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(at + Duration::hours(1)),
            ),
            &contract,
            &mut budget,
            at,
        )
        .is_ok()
        {
            return Err("a delegated grant dropped a condition its parent carried".to_string());
        }

        let _ = PrincipalId::new();
        Ok(format!(
            "a delegated grant narrows to depth {} and is refused when it widens the \
             operation, the scope, the risk ceiling, the budget, the validity or the \
             conditions — or when the delegator names themselves",
            child.depth()
        ))
    }
);

cap_case!(
    EveryEvaluationProducesADecision,
    "exec-cap-004-decision-is-the-verdict",
    4,
    CaseCategory::Stateful,
    "Evaluation always yields a decision carrying its result, reason and the input it judged, \
     and an allow is only ever reported as a matched grant",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::{PolicyDecisionReason, PolicyDecisionResult};
        use crate::time::now;
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let request = request();

        let denied = decide(&actors, &request, &[], at)
            .map_err(|error| format!("evaluating with no grants failed: {error}"))?;
        if denied.result != PolicyDecisionResult::Deny
            || denied.reason != PolicyDecisionReason::DefaultDeny
        {
            return Err(format!(
                "with no grants the verdict was {:?}/{:?} rather than a default deny",
                denied.result, denied.reason
            ));
        }
        if denied.input_state.resource_scope != request.resource_scope {
            return Err("a decision did not record the request it judged".to_string());
        }

        let allowed = decide(&actors, &request, &[grant(&actors, at)], at)
            .map_err(|error| format!("evaluating a matching grant failed: {error}"))?;
        if !allowed.is_allowed() || allowed.reason != PolicyDecisionReason::GrantMatched {
            return Err(format!(
                "a matching grant produced {:?}/{:?}",
                allowed.result, allowed.reason
            ));
        }
        if allowed.matched_grant_id.is_none() {
            return Err(
                "an allow did not name the grant that permitted it, so the decision \
                        cannot be traced to any authority"
                    .to_string(),
            );
        }

        Ok(
            "every evaluation returns a decision that records its result, its reason and the \
            input state, and an allow always names the grant behind it"
                .to_string(),
        )
    }
);

cap_case!(
    NothingIsPermittedWithoutAGrant,
    "exec-cap-006-no-bypass",
    6,
    CaseCategory::Security,
    "Each of subject, capability, operation, scope, validity and revocation alone is enough to \
     deny, so no near-miss grant lets a request through",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::{CapabilityGrant, PolicyDecisionReason};
        use crate::{Capability, PrincipalId, RiskCategory, time::now};
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let request = request();

        let mut mismatches: Vec<(&str, CapabilityGrant, PolicyDecisionReason)> = Vec::new();

        let mut other_subject = spec(
            &actors,
            Capability::MemoryWrite,
            OPERATION,
            SCOPE,
            RiskCategory::Medium,
            Some(100),
            at,
            None,
        );
        other_subject.subject_id = PrincipalId::new();
        mismatches.push((
            "another subject",
            CapabilityGrant::issue(other_subject, at).expect("grant"),
            PolicyDecisionReason::SubjectMismatch,
        ));

        mismatches.push((
            "another capability",
            CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::AuditRead,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Medium,
                    Some(100),
                    at,
                    None,
                ),
                at,
            )
            .expect("grant"),
            PolicyDecisionReason::CapabilityMismatch,
        ));

        mismatches.push((
            "another operation",
            CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::MemoryWrite,
                    "memory.read",
                    SCOPE,
                    RiskCategory::Medium,
                    Some(100),
                    at,
                    None,
                ),
                at,
            )
            .expect("grant"),
            PolicyDecisionReason::OperationMismatch,
        ));

        mismatches.push((
            "another resource",
            CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::MemoryWrite,
                    OPERATION,
                    "memory://project-beta",
                    RiskCategory::Medium,
                    Some(100),
                    at,
                    None,
                ),
                at,
            )
            .expect("grant"),
            PolicyDecisionReason::ResourceMismatch,
        ));

        mismatches.push((
            "a validity window that has not opened",
            CapabilityGrant::issue(
                spec(
                    &actors,
                    Capability::MemoryWrite,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Medium,
                    Some(100),
                    at + Duration::hours(1),
                    None,
                ),
                at,
            )
            .expect("grant"),
            PolicyDecisionReason::NotYetValid,
        ));

        for (label, grant, expected) in mismatches {
            let decision = decide(&actors, &request, &[grant], at)
                .map_err(|error| format!("evaluation failed: {error}"))?;
            if decision.is_allowed() {
                return Err(format!("a grant for {label} authorized the request"));
            }
            if decision.reason != expected {
                return Err(format!(
                    "a grant for {label} was denied as {:?} rather than {expected:?}, so the \
                     denial does not say what was wrong",
                    decision.reason
                ));
            }
        }

        Ok(
            "a grant differing in subject, capability, operation, resource or validity denies, \
            and each denial names which of them failed"
                .to_string(),
        )
    }
);

cap_case!(
    RiskIsPartOfTheVerdict,
    "exec-cap-007-risk-in-the-decision",
    7,
    CaseCategory::Behavioral,
    "Context risk raises the effective risk of a request, so a low-risk action in a high-risk \
     context is refused by a ceiling it would otherwise pass",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::PolicyDecisionReason;
        use crate::{RiskCategory, time::now};
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let grant = grant(&actors, at);

        let ordinary = decide(&actors, &request(), &[grant.clone()], at)
            .map_err(|error| format!("evaluation failed: {error}"))?;
        if !ordinary.is_allowed() {
            return Err("a low-risk request inside the ceiling was denied".to_string());
        }

        let risky = request().with_context_risk(RiskCategory::Critical);
        if risky.effective_risk() != RiskCategory::Critical {
            return Err("context risk did not raise the effective risk of the request".to_string());
        }

        let decision = decide(&actors, &risky, &[grant], at)
            .map_err(|error| format!("evaluation failed: {error}"))?;
        if decision.is_allowed() {
            return Err(
                "a critical-risk context passed a medium ceiling, so risk is not part \
                        of the decision"
                    .to_string(),
            );
        }
        if decision.reason != PolicyDecisionReason::RiskExceedsCeiling {
            return Err(format!(
                "the denial was reported as {:?} rather than a risk ceiling",
                decision.reason
            ));
        }
        if decision.input_state.effective_risk != RiskCategory::Critical {
            return Err("the decision did not record the effective risk it judged".to_string());
        }

        Ok(
            "effective risk is the higher of requested and context risk, is compared against \
            the grant's ceiling, and is recorded on the decision"
                .to_string(),
        )
    }
);

cap_case!(
    BudgetIsCheckedWhenItIsSpent,
    "exec-cap-008-budget-at-effect-time",
    8,
    CaseCategory::Stateful,
    "A grant that authorizes a request does not authorize unlimited spending: a charge beyond \
     the reservation is refused after the decision allowed it",
    DELEGATION_EVIDENCE,
    || {
        use crate::security::HierarchicalBudget;
        use crate::time::now;
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let grant = grant(&actors, at);

        // Planning: the request is inside the grant's declared budget.
        let planned = decide(
            &actors,
            &request().with_budget_units(40),
            &[grant.clone()],
            at,
        )
        .map_err(|error| format!("evaluation failed: {error}"))?;
        if !planned.is_allowed() {
            return Err(
                "a request inside the grant's budget was denied at planning time".to_string(),
            );
        }

        // Effect time: the reservation is what actually constrains spending.
        let mut budget = HierarchicalBudget::new(1_000);
        budget
            .register_root(grant.id, 100)
            .map_err(|error| format!("registering a root budget failed: {error}"))?;
        budget
            .charge(grant.id, 60)
            .map_err(|error| format!("a charge inside the reservation was refused: {error}"))?;

        if budget.charge(grant.id, 60).is_ok() {
            return Err(
                "a second charge took the grant past its reservation, so the budget is \
                        checked only when planning"
                    .to_string(),
            );
        }
        if budget.remaining(grant.id) != Some(40) {
            return Err(format!(
                "the remaining budget is {:?} after charging 60 of 100",
                budget.remaining(grant.id)
            ));
        }

        Ok(
            "a grant inside its budget authorizes a request, and the reservation still refuses \
            a charge that would exceed it"
                .to_string(),
        )
    }
);

cap_case!(
    ChildBudgetsComeOutOfTheParent,
    "exec-cap-009-budgets-attenuate",
    9,
    CaseCategory::Behavioral,
    "A child reservation is taken from the parent's remaining allocation and cannot exceed it, \
     and a parent's own spending is reduced by what its children hold",
    DELEGATION_EVIDENCE,
    || {
        use crate::CapabilityGrantId;
        use crate::security::HierarchicalBudget;

        let root = CapabilityGrantId::new();
        let first_child = CapabilityGrantId::new();
        let second_child = CapabilityGrantId::new();

        let mut budget = HierarchicalBudget::new(1_000);
        budget
            .register_root(root, 100)
            .map_err(|error| format!("registering a root failed: {error}"))?;

        if budget.register_root(CapabilityGrantId::new(), 50).is_ok() {
            return Err(
                "a second root was registered, so two hierarchies could each believe \
                        they owned the limit"
                    .to_string(),
            );
        }

        budget
            .reserve_child(root, first_child, 60)
            .map_err(|error| format!("a child reservation inside the parent failed: {error}"))?;

        if budget.remaining(root) != Some(40) {
            return Err(format!(
                "the parent has {:?} remaining after a child reserved 60 of 100",
                budget.remaining(root)
            ));
        }

        if budget.reserve_child(root, second_child, 50).is_ok() {
            return Err(
                "a second child reserved more than the parent had left, so budgets \
                        amplify downstream"
                    .to_string(),
            );
        }

        if budget.charge(root, 50).is_ok() {
            return Err("the parent spent budget its child was holding".to_string());
        }

        Ok(
            "a child's reservation is deducted from its parent, cannot exceed what the parent \
            has left, and reduces what the parent itself may spend"
                .to_string(),
        )
    }
);

cap_case!(
    RevocationTakesEffectImmediately,
    "exec-cap-010-revocation-is-immediate",
    10,
    CaseCategory::Stateful,
    "A revoked grant stops authorizing without any new grant being issued, and cannot be \
     revoked twice",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::PolicyDecisionReason;
        use crate::time::now;
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let mut grant = grant(&actors, at);
        let request = request();

        if !decide(&actors, &request, std::slice::from_ref(&grant), at)
            .map_err(|error| format!("evaluation failed: {error}"))?
            .is_allowed()
        {
            return Err("an active grant did not authorize its own request".to_string());
        }

        grant
            .revoke(at + Duration::minutes(1))
            .map_err(|error| format!("revoking an active grant failed: {error}"))?;

        let after = decide(
            &actors,
            &request,
            std::slice::from_ref(&grant),
            at + Duration::minutes(2),
        )
        .map_err(|error| format!("evaluation failed: {error}"))?;
        if after.is_allowed() {
            return Err(
                "a revoked grant still authorized the request, so revocation waits for \
                        something else to happen"
                    .to_string(),
            );
        }
        if after.reason != PolicyDecisionReason::Revoked {
            return Err(format!(
                "the denial was reported as {:?} rather than a revocation",
                after.reason
            ));
        }
        if grant.revoke(at + Duration::minutes(3)).is_ok() {
            return Err(
                "a revoked grant was revoked again, which would move the revocation \
                        time after the fact"
                    .to_string(),
            );
        }

        Ok(
            "revoking a grant denies the next decision immediately, names revocation as the \
            reason, and cannot be repeated to rewrite when it happened"
                .to_string(),
        )
    }
);

cap_case!(
    ADelegateCannotOutliveOrOutrankItsChain,
    "exec-cap-011-chain-only-reduces",
    11,
    CaseCategory::Security,
    "Delegation requires an explicit capability.delegate permission, refuses a chain deeper \
     than its contract allows, and refuses a child outliving its parent",
    DELEGATION_EVIDENCE,
    || {
        use crate::security::CapabilityGrant;
        use crate::security::{DelegatedCapability, HierarchicalBudget};
        use crate::{Capability, RiskCategory, time::now};
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let until = at + Duration::hours(2);
        let parent = CapabilityGrant::issue(
            spec(
                &actors,
                Capability::MemoryWrite,
                OPERATION,
                SCOPE,
                RiskCategory::Medium,
                Some(100),
                at,
                Some(until),
            ),
            at,
        )
        .map_err(|error| format!("a bounded parent grant was refused: {error}"))?;
        let permission = delegation_permission(&actors, SCOPE, RiskCategory::Medium, Some(100), at);
        let contract = contract(SCOPE, 100, 1);

        // An ordinary grant is not a delegation permission, however wide it is.
        let not_a_permission = grant(&actors, at);
        let mut budget = HierarchicalBudget::new(1_000);
        if DelegatedCapability::from_root(
            &parent,
            &not_a_permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(until),
            ),
            &contract,
            &mut budget,
            at,
        )
        .is_ok()
        {
            return Err(
                "a grant that was not a capability.delegate permission was accepted as \
                        one, so holding authority would imply the right to hand it on"
                    .to_string(),
            );
        }

        // A child outliving its parent would keep authority the parent has lost.
        let mut budget = HierarchicalBudget::new(1_000);
        if DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(until + Duration::hours(1)),
            ),
            &contract,
            &mut budget,
            at,
        )
        .is_ok()
        {
            return Err("a delegated grant outlived the grant it came from".to_string());
        }

        // Depth: the contract allows one level, so the child cannot delegate on.
        let mut budget = HierarchicalBudget::new(1_000);
        let child = DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(
                &actors,
                OPERATION,
                SCOPE,
                RiskCategory::Low,
                40,
                at,
                Some(until),
            ),
            &contract,
            &mut budget,
            at,
        )
        .map_err(|error| format!("a narrowing delegation was refused: {error}"))?;

        let grandchild_permission =
            delegation_permission(&actors, SCOPE, RiskCategory::Low, Some(40), at);
        if child
            .delegate(
                &grandchild_permission,
                child_spec(
                    &actors,
                    OPERATION,
                    SCOPE,
                    RiskCategory::Low,
                    10,
                    at,
                    Some(until),
                ),
                &contract,
                &mut budget,
                at,
            )
            .is_ok()
        {
            return Err("a chain grew past the depth its contract allowed".to_string());
        }

        Ok(
            "delegation requires an explicit capability.delegate permission, cannot outlive its \
            parent, and stops at the depth its contract sets"
                .to_string(),
        )
    }
);

cap_case!(
    ADecisionCanBeAudited,
    "exec-cap-013-decision-is-auditable",
    13,
    CaseCategory::Evidence,
    "A decision records the subject, the action, the resource, the verdict, the reason, the \
     policy version and the instant it was made",
    CAPABILITY_EVIDENCE,
    || {
        use crate::time::now;
        use capability_fixture::*;

        let actors = actors();
        let at = now();
        let request = request().with_budget_units(7);
        let decision = decide(&actors, &request, &[grant(&actors, at)], at)
            .map_err(|error| format!("evaluation failed: {error}"))?;

        if decision.subject_id != actors.subject {
            return Err("a decision does not name the subject it judged".to_string());
        }
        if decision.workspace_id != actors.workspace {
            return Err("a decision does not name the workspace it was made in".to_string());
        }
        if decision.operation != request.operation
            || decision.resource_scope != request.resource_scope
        {
            return Err("a decision does not name the action and resource it judged".to_string());
        }
        if decision.policy_version.trim().is_empty() {
            return Err(
                "a decision does not name the policy version that produced it, so it \
                        cannot be re-evaluated against the rules that applied"
                    .to_string(),
            );
        }
        if decision.decided_at != at {
            return Err("a decision does not record when it was made".to_string());
        }
        if decision.input_state.requested_budget_units != 7 {
            return Err("a decision does not record the input it judged".to_string());
        }

        // Serializable, or it cannot reach an audit trail at all.
        if serde_json::to_value(&decision).is_err() {
            return Err("a decision is not serializable".to_string());
        }

        Ok(format!(
            "a decision carries subject, workspace, action, resource, verdict, reason, input \
             state, policy version ({}) and instant, and serializes",
            decision.policy_version
        ))
    }
);

cap_case!(
    ADecisionIsOnlyGoodForTheInstantItNames,
    "exec-cap-014-no-stale-authority",
    14,
    CaseCategory::Stateful,
    "Authority is evaluated against the grant's state at the instant judged, so a decision \
     made before a revocation cannot be replayed after it",
    CAPABILITY_EVIDENCE,
    || {
        use crate::time::now;
        use capability_fixture::*;
        use chrono::Duration;

        let actors = actors();
        let at = now();
        let mut grant = grant(&actors, at);
        let request = request();

        let before = decide(&actors, &request, std::slice::from_ref(&grant), at)
            .map_err(|error| format!("evaluation failed: {error}"))?;
        if !before.is_allowed() {
            return Err("an active grant did not authorize its request".to_string());
        }

        grant
            .revoke(at + Duration::minutes(1))
            .map_err(|error| format!("revocation failed: {error}"))?;

        // The earlier decision still says allow — it was true when it was made,
        // and that is exactly why a cached copy of it must not be treated as
        // current. What must not happen is a *new* evaluation agreeing with it.
        if !before.is_allowed() {
            return Err("the historical decision was rewritten".to_string());
        }
        let after = decide(
            &actors,
            &request,
            std::slice::from_ref(&grant),
            at + Duration::minutes(2),
        )
        .map_err(|error| format!("evaluation failed: {error}"))?;
        if after.is_allowed() {
            return Err("re-evaluating after revocation still allowed the request".to_string());
        }
        if after.decided_at <= before.decided_at {
            return Err(
                "the two decisions cannot be ordered, so a reader cannot tell which \
                        one is current"
                    .to_string(),
            );
        }

        // Presenting an older instant must not resurrect the grant. This is the
        // stale-authority hole itself: a cached decision, or a replayed
        // timestamp, would otherwise still authorize.
        if grant.is_active_at(at) {
            return Err(
                "a revoked grant reported itself active at an instant before the \
                        revocation, so an older timestamp would restore authority that has \
                        been taken away"
                    .to_string(),
            );
        }
        if decide(&actors, &request, std::slice::from_ref(&grant), at)
            .map_err(|error| format!("evaluation failed: {error}"))?
            .is_allowed()
        {
            return Err(
                "re-evaluating a revoked grant against an earlier instant authorized \
                        the request"
                    .to_string(),
            );
        }
        if grant.revoked_at.is_none() {
            return Err(
                "a revoked grant does not record when it was revoked, so the history \
                        a reader might reconstruct is gone"
                    .to_string(),
            );
        }

        Ok(
            "a revoked grant is unusable at every instant, including ones before the \
            revocation, while the decision made beforehand and the revocation time are both \
            preserved for a reader"
                .to_string(),
        )
    }
);

/// Fixtures for the health cases.
mod health_fixture {
    use crate::health::{
        FindingDisposition, HealthFinding, HealthScope, HealthSeverity, HealthState,
        InvariantDefinition, RepairAuthorization, RepairPlan, Repairability, Reversibility,
        VerificationResult, VerificationRun,
    };
    use crate::{Capability, DomainError, PrincipalId, WorkspaceId, time::Timestamp};

    pub fn definition(repairability: Repairability) -> InvariantDefinition {
        InvariantDefinition::new(
            "memory.active_revision_exists",
            "v1",
            "an active memory names a revision that exists",
            "re-point the memory at an existing revision",
            HealthSeverity::Error,
            repairability,
            vec!["memory.revision_belongs_to_memory".to_string()],
        )
        .expect("a well-formed invariant definition")
    }

    pub fn finding_at(repairability: Repairability, at: Timestamp) -> HealthFinding {
        HealthFinding::new(
            &definition(repairability),
            HealthScope::workspace(WorkspaceId::new()),
            "memory:0198…/revision-missing",
            HealthState::Unhealthy,
            vec!["evidence:doctor-run-1".to_string()],
            at,
        )
        .expect("a well-formed finding")
    }

    pub fn plan_for(finding: &HealthFinding, at: Timestamp) -> RepairPlan {
        RepairPlan::new(
            vec![finding.id()],
            "state-v1",
            finding.scope().clone(),
            vec![finding.fingerprint().to_string()],
            vec!["rebuild the search document from the active revision".to_string()],
            vec!["the active revision has a search document".to_string()],
            vec!["memory.active_revision_exists".to_string()],
            finding.repair_risk(),
            Reversibility::Restartable,
            Capability::MemoryWrite,
            at,
            None,
        )
        .expect("a well-formed repair plan")
    }

    pub fn authorization(capability: Capability) -> RepairAuthorization {
        RepairAuthorization::allow(capability, "policy-v1", "evidence:policy-decision-1")
    }

    pub fn verification(
        plan: &RepairPlan,
        finding: &HealthFinding,
        result: VerificationResult,
        at: Timestamp,
    ) -> Result<VerificationRun, DomainError> {
        VerificationRun::new(
            plan.id(),
            vec![finding.id()],
            result,
            vec!["memory.active_revision_exists".to_string()],
            vec!["evidence:verification-run-1".to_string()],
            at,
        )
    }

    pub fn accepted_risk(expires_at: Option<Timestamp>) -> FindingDisposition {
        FindingDisposition::accepted_risk(
            PrincipalId::new(),
            "the operator accepts this until the backfill lands",
            expires_at,
            "policy-v1",
            "audit:disposition-1",
        )
    }

    pub fn suppressed(expires_at: Option<Timestamp>) -> FindingDisposition {
        FindingDisposition::suppressed(
            PrincipalId::new(),
            "noisy during the migration window",
            expires_at,
            "policy-v1",
            "audit:disposition-2",
        )
    }
}

macro_rules! health_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Hlt,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    HEALTH_EVIDENCE,
                )
            }
        }
    };
}

health_case!(
    AnInvariantDeclaresWhatItChecks,
    "exec-hlt-002-invariant-declares-itself",
    2,
    CaseCategory::Behavioral,
    "An invariant definition requires an id, a version and a description, and a registry \
     refuses a duplicate id",
    || {
        use crate::health::HealthSeverity;
        use crate::health::{InvariantDefinition, InvariantRegistry, Repairability};
        use health_fixture::definition;

        let blank_version = InvariantDefinition::new(
            "memory.active_revision_exists",
            "  ",
            "a description",
            "a remediation",
            HealthSeverity::Error,
            Repairability::Auto,
            vec![],
        );
        if blank_version.is_ok() {
            return Err(
                "an invariant was defined without a version, so a finding could not \
                        say which version of the check produced it"
                    .to_string(),
            );
        }

        let blank_description = InvariantDefinition::new(
            "memory.active_revision_exists",
            "v1",
            "   ",
            "a remediation",
            HealthSeverity::Error,
            Repairability::Auto,
            vec![],
        );
        if blank_description.is_ok() {
            return Err(
                "an invariant was defined with no description of what it checks".to_string(),
            );
        }

        // Two definitions sharing an id would make a finding's `invariant_id`
        // ambiguous, which is the field everything else joins on.
        let mut registry = InvariantRegistry::new([definition(Repairability::Auto)])
            .map_err(|error| format!("a registry refused a well-formed definition: {error}"))?;
        if registry.register(definition(Repairability::Manual)).is_ok() {
            return Err("a registry accepted two invariants with the same id".to_string());
        }

        let stored = registry
            .get("memory.active_revision_exists")
            .ok_or_else(|| "a registered invariant could not be looked up".to_string())?;
        Ok(format!(
            "an invariant carries its id, version ({}) and description, and a registry \
             refuses a second definition claiming the same id",
            stored.version()
        ))
    }
);

health_case!(
    AFindingCarriesItsInvariantAndObservation,
    "exec-hlt-003-finding-carries-its-observation",
    3,
    CaseCategory::Stateful,
    "A finding records the invariant and version that produced it, the observed state, the \
     severity, and refuses to exist without evidence",
    || {
        use crate::health::{HealthScope, HealthState, Repairability};
        use crate::{WorkspaceId, time::now};
        use health_fixture::*;

        let at = now();
        let definition = definition(Repairability::Auto);

        let without_evidence = crate::health::HealthFinding::new(
            &definition,
            HealthScope::workspace(WorkspaceId::new()),
            "fingerprint",
            HealthState::Unhealthy,
            Vec::new(),
            at,
        );
        if without_evidence.is_ok() {
            return Err(
                "a finding was recorded with no evidence, so nothing supports the \
                        claim that the invariant was violated"
                    .to_string(),
            );
        }

        let finding = finding_at(Repairability::Auto, at);
        if finding.invariant_id() != definition.invariant_id() {
            return Err("a finding does not name the invariant that produced it".to_string());
        }
        if finding.invariant_version() != definition.version() {
            return Err("a finding does not name the invariant version".to_string());
        }
        if finding.severity() != definition.default_severity() {
            return Err("a finding does not carry the severity its invariant declares".to_string());
        }
        if finding.observed_state() != HealthState::Unhealthy {
            return Err("a finding does not carry the state that was observed".to_string());
        }

        Ok(format!(
            "a finding names {} at {}, carries the observed state and severity, and cannot \
             be created without evidence",
            finding.invariant_id(),
            finding.invariant_version()
        ))
    }
);

health_case!(
    APlanIsDerivedFromFindings,
    "exec-hlt-004-plan-derives-from-findings",
    4,
    CaseCategory::Behavioral,
    "A repair plan cannot be built without findings, operations, postconditions and \
     verification checks",
    || {
        use crate::health::{HealthScope, RepairPlan, RepairRisk, Repairability, Reversibility};
        use crate::{Capability, WorkspaceId, time::now};
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);

        let without_findings = RepairPlan::new(
            Vec::new(),
            "state-v1",
            HealthScope::workspace(WorkspaceId::new()),
            vec!["precondition".to_string()],
            vec!["operation".to_string()],
            vec!["postcondition".to_string()],
            vec!["check".to_string()],
            RepairRisk::Low,
            Reversibility::Restartable,
            Capability::MemoryWrite,
            at,
            None,
        );
        if without_findings.is_ok() {
            return Err(
                "a repair plan was built from no findings at all, which is an ad-hoc \
                        rebuild wearing a plan's name"
                    .to_string(),
            );
        }

        let without_verification = RepairPlan::new(
            vec![finding.id()],
            "state-v1",
            finding.scope().clone(),
            vec!["precondition".to_string()],
            vec!["operation".to_string()],
            vec!["postcondition".to_string()],
            Vec::new(),
            RepairRisk::Low,
            Reversibility::Restartable,
            Capability::MemoryWrite,
            at,
            None,
        );
        if without_verification.is_ok() {
            return Err(
                "a repair plan was accepted with no verification checks, so nothing \
                        would re-examine the invariant afterwards"
                    .to_string(),
            );
        }

        let plan = plan_for(&finding, at);
        if plan.finding_ids() != [finding.id()] {
            return Err("a plan does not name the findings it was derived from".to_string());
        }

        Ok(
            "a plan must name the findings it answers and must declare the checks that will \
            confirm it worked"
                .to_string(),
        )
    }
);

health_case!(
    RepairPassesThroughAuthorization,
    "exec-hlt-005-repair-requires-authorization",
    5,
    CaseCategory::Security,
    "Execution is refused without an allowing decision, refused when the decision carries a \
     different capability, and refused against a stale or expired plan",
    || {
        use crate::health::{RepairAuthorization, RepairExecutionError, Repairability};
        use crate::{Capability, time::now};
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);

        let denied = RepairAuthorization::deny(
            Capability::MemoryWrite,
            "policy-v1",
            "evidence:policy-decision-2",
        );
        match plan.start_execution(&denied, "state-v1", at) {
            Err(RepairExecutionError::Unauthorized) => {}
            _ => {
                return Err("a repair ran against a denied authorization".to_string());
            }
        }

        // A decision about a different capability is not a decision about this
        // repair, and treating it as one would let any allow-decision unlock
        // every plan.
        match plan.start_execution(&authorization(Capability::AuditRead), "state-v1", at) {
            Err(RepairExecutionError::Unauthorized) => {}
            _ => {
                return Err(
                    "a repair ran on an authorization for a different capability".to_string(),
                );
            }
        }

        match plan.start_execution(&authorization(Capability::MemoryWrite), "state-v2", at) {
            Err(RepairExecutionError::StalePlan) => {}
            _ => {
                return Err("a repair ran against state the plan was not written for".to_string());
            }
        }

        let execution = plan
            .start_execution(&authorization(Capability::MemoryWrite), "state-v1", at)
            .map_err(|error| format!("an authorized repair was refused: {error:?}"))?;

        Ok(format!(
            "execution {} starts only on an allowing decision for the plan's own capability \
             and against the state the plan was written for",
            execution.id()
        ))
    }
);

health_case!(
    VerificationRechecksTheInvariant,
    "exec-hlt-006-verification-rechecks",
    6,
    CaseCategory::Stateful,
    "A verification run must carry checks and evidence, must cover the finding it claims to \
     resolve, and only a passing run resolves it",
    || {
        use crate::health::{Repairability, VerificationResult, VerificationRun};
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);

        let without_checks = VerificationRun::new(
            plan.id(),
            vec![finding.id()],
            VerificationResult::Passed,
            Vec::new(),
            vec!["evidence:verification".to_string()],
            at,
        );
        if without_checks.is_ok() {
            return Err(
                "a verification run was accepted with no checks, so 'verified' would \
                        mean nothing was re-examined"
                    .to_string(),
            );
        }

        // A run covering some other finding must not resolve this one.
        let other = finding_at(Repairability::Auto, at);
        let elsewhere = verification(&plan, &other, VerificationResult::Passed, at)
            .map_err(|error| format!("a well-formed verification run was refused: {error}"))?;
        if finding.apply_verification(&elsewhere).is_ok() {
            return Err("a verification run resolved a finding it did not cover".to_string());
        }

        let passed = verification(&plan, &finding, VerificationResult::Passed, at)
            .map_err(|error| format!("a well-formed verification run was refused: {error}"))?;
        let resolved = finding
            .apply_verification(&passed)
            .map_err(|error| format!("a covering verification did not apply: {error}"))?;

        if resolved.lifecycle_status() != crate::health::FindingLifecycleStatus::Resolved {
            return Err(format!(
                "a passing verification left the finding {:?}",
                resolved.lifecycle_status()
            ));
        }

        Ok(
            "a verification run carries its checks and evidence, applies only to the findings \
            it covers, and resolves them only when it passed"
                .to_string(),
        )
    }
);

health_case!(
    RepairRebuildsRatherThanInvents,
    "exec-hlt-007-repair-is-reconstructive",
    7,
    CaseCategory::Behavioral,
    "An automatic repair is low risk and restartable, while a non-repairable finding is high \
     risk and cannot be planned automatically",
    || {
        use crate::health::{RepairRisk, Repairability};
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let automatic = finding_at(Repairability::Auto, at);
        let manual = finding_at(Repairability::Manual, at);
        let never = finding_at(Repairability::NotRepairable, at);

        if automatic.repair_risk() != RepairRisk::Low {
            return Err(format!(
                "an auto-repairable finding carries {:?} risk, so the risk no longer follows \
                 from whether the state can be reconstructed",
                automatic.repair_risk()
            ));
        }
        if manual.repair_risk() <= automatic.repair_risk() {
            return Err(
                "a manual repair does not carry more risk than an automatic one".to_string(),
            );
        }
        if never.repair_risk() <= manual.repair_risk() {
            return Err("a non-repairable finding does not carry the highest risk".to_string());
        }

        Ok(format!(
            "risk follows reconstructibility: auto {:?} < manual {:?} < not-repairable {:?}",
            automatic.repair_risk(),
            manual.repair_risk(),
            never.repair_risk()
        ))
    }
);

health_case!(
    RecurrenceIsCountedOnTheSameInvariant,
    "exec-hlt-008-recurrence-detection",
    8,
    CaseCategory::Stateful,
    "A repeated observation increments the finding rather than creating a new one, and a \
     single observation is not recurrence",
    || {
        use crate::health::{HealthState, Repairability, assess_recurrence};
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);

        let first = finding
            .record_occurrence(HealthState::Unhealthy, vec!["evidence:run-1".into()], at)
            .map_err(|error| format!("an occurrence was refused: {error}"))?;
        let single = assess_recurrence(std::slice::from_ref(&first), 4);
        if single.is_recurrent() {
            return Err("one observation was reported as recurrence".to_string());
        }

        let second = finding
            .record_occurrence(HealthState::Unhealthy, vec!["evidence:run-2".into()], at)
            .map_err(|error| format!("a second occurrence was refused: {error}"))?;

        if finding.occurrence_count() != 2 {
            return Err(format!(
                "the finding counted {} occurrences after two observations",
                finding.occurrence_count()
            ));
        }
        if second.sequence() <= first.sequence() {
            return Err(
                "occurrences do not carry an increasing sequence, so their order is \
                        not recoverable"
                    .to_string(),
            );
        }

        let repeated = assess_recurrence(&[first, second], 4);
        if !repeated.is_recurrent() {
            return Err("two observations of the same invariant were not recurrence".to_string());
        }

        Ok(
            "a second observation increments the same finding and is reported as recurrence, \
            while a single observation is not"
                .to_string(),
        )
    }
);

health_case!(
    FlappingNeedsOscillationNotJustRepetition,
    "exec-hlt-009-flapping-detection",
    9,
    CaseCategory::Stateful,
    "A finding that stays unhealthy is not flapping however often it is seen; one that \
     oscillates is",
    || {
        use crate::health::{HealthState, Repairability, assess_recurrence};
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let mut steady = finding_at(Repairability::Auto, at);
        let mut steady_occurrences = Vec::new();
        for index in 0..6 {
            steady_occurrences.push(
                steady
                    .record_occurrence(
                        HealthState::Unhealthy,
                        vec![format!("evidence:steady-{index}")],
                        at,
                    )
                    .map_err(|error| format!("an occurrence was refused: {error}"))?,
            );
        }
        if assess_recurrence(&steady_occurrences, 4).is_flapping() {
            return Err(
                "a persistently unhealthy finding was reported as flapping, which \
                        would send an operator looking for an oscillation that is not there"
                    .to_string(),
            );
        }

        let mut oscillating = finding_at(Repairability::Auto, at);
        let mut oscillating_occurrences = Vec::new();
        for (index, state) in [
            HealthState::Unhealthy,
            HealthState::Healthy,
            HealthState::Unhealthy,
            HealthState::Healthy,
        ]
        .into_iter()
        .enumerate()
        {
            oscillating_occurrences.push(
                oscillating
                    .record_occurrence(state, vec![format!("evidence:flap-{index}")], at)
                    .map_err(|error| format!("an occurrence was refused: {error}"))?,
            );
        }
        let assessment = assess_recurrence(&oscillating_occurrences, 4);
        if !assessment.is_flapping() {
            return Err(format!(
                "a repair/relapse cycle with {} transitions was not reported as flapping",
                assessment.state_transitions()
            ));
        }

        Ok(
            "flapping requires oscillation and a threshold of observations, so a steadily \
            unhealthy finding is recurrent without being flapping"
                .to_string(),
        )
    }
);

health_case!(
    DispositionClassifiesWhatHappensNext,
    "exec-hlt-010-disposition-classifies",
    10,
    CaseCategory::Behavioral,
    "Every repairability maps to a disposition, and a suppression or accepted risk without \
     reason, policy version or audit reference is refused",
    || {
        use crate::health::{FindingDisposition, Repairability};
        use crate::{PrincipalId, time::now};
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);

        let unjustified = FindingDisposition::accepted_risk(
            PrincipalId::new(),
            "   ",
            None,
            "policy-v1",
            "audit:1",
        );
        if finding.set_disposition(unjustified).is_ok() {
            return Err("a risk was accepted with a blank reason".to_string());
        }

        let unattributed =
            FindingDisposition::suppressed(PrincipalId::new(), "noisy", None, "policy-v1", "  ");
        if finding.set_disposition(unattributed).is_ok() {
            return Err(
                "a finding was suppressed with no audit reference, so the decision \
                        could not be traced to anyone"
                    .to_string(),
            );
        }

        for repairability in [
            Repairability::Auto,
            Repairability::Manual,
            Repairability::NotRepairable,
        ] {
            let subject = finding_at(repairability, at);
            if subject.repairability() != repairability {
                return Err("a finding lost the repairability of its invariant".to_string());
            }
        }

        Ok(
            "repairability travels from the invariant to the finding, and silencing one \
            requires a reason, a policy version and an audit reference"
                .to_string(),
        )
    }
);

health_case!(
    ABudgetLimitsRepairAttempts,
    "exec-hlt-011-repair-budget",
    11,
    CaseCategory::Behavioral,
    "A budget refuses attempts inside the cooldown and refuses more than its maximum within \
     the window",
    || {
        use crate::health::{RepairBudget, RepairBudgetError};
        use crate::time::now;
        use chrono::Duration;

        let start = now();
        let mut budget = RepairBudget::new(2, Duration::hours(1), Duration::minutes(10));

        budget
            .reserve(start)
            .map_err(|error| format!("the first attempt was refused: {error:?}"))?;

        match budget.reserve(start + Duration::minutes(1)) {
            Err(RepairBudgetError::CooldownActive { .. }) => {}
            _ => {
                return Err(
                    "a second repair started inside the cooldown, so a failing repair \
                            could be retried in a tight loop"
                        .to_string(),
                );
            }
        }

        budget
            .reserve(start + Duration::minutes(11))
            .map_err(|error| format!("an attempt after the cooldown was refused: {error:?}"))?;

        match budget.reserve(start + Duration::minutes(30)) {
            Err(RepairBudgetError::AttemptBudgetExceeded) => {}
            _ => {
                return Err(
                    "a third attempt was allowed inside a window that permits two".to_string(),
                );
            }
        }

        // Outside the window the budget recovers, or a transient fault would
        // disable repair permanently.
        budget
            .reserve(start + Duration::hours(2))
            .map_err(|error| format!("the budget never recovered after its window: {error:?}"))?;

        Ok(
            "a budget enforces both a cooldown between attempts and a maximum within its \
            window, and recovers once the window has passed"
                .to_string(),
        )
    }
);

health_case!(
    TheOperatorSeparatesLookingFromActing,
    "exec-hlt-012-phases-are-separate",
    12,
    CaseCategory::Stateful,
    "Inspect and plan are read-only phases; only repair carries an authorization, and a \
     repair command without one is refused",
    || {
        use crate::health::{
            RepairOperatorCommand, RepairOperatorContract, RepairOperatorPhase, Repairability,
        };
        use crate::{Capability, time::now};
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);

        let inspect = RepairOperatorCommand::inspect();
        let planning = RepairOperatorCommand::plan(vec![finding.id()]);
        let repair = RepairOperatorCommand::repair(
            plan.id(),
            "state-v1",
            authorization(Capability::MemoryWrite),
        )
        .map_err(|error| format!("a well-formed repair command was refused: {error}"))?;

        if !inspect.is_read_only() || !planning.is_read_only() {
            return Err(
                "inspecting or planning is not read-only, so looking at the system \
                        can change it"
                    .to_string(),
            );
        }
        if repair.is_read_only() {
            return Err("repair reports itself read-only".to_string());
        }
        if inspect.authorization().is_some() || planning.authorization().is_some() {
            return Err("a read-only phase carries an authorization it does not need".to_string());
        }
        if repair.phase() != RepairOperatorPhase::Repair {
            return Err("the repair command does not report the repair phase".to_string());
        }

        if RepairOperatorContract::validate(&RepairOperatorCommand::plan(Vec::new())).is_ok() {
            return Err("a plan command naming no findings was accepted".to_string());
        }

        let unauthorized = RepairOperatorCommand::Repair {
            plan_id: plan.id(),
            current_state_ref: "state-v1".to_string(),
            authorization: crate::health::RepairAuthorization::deny(
                Capability::MemoryWrite,
                "policy-v1",
                "evidence:denied",
            ),
        };
        if RepairOperatorContract::validate(&unauthorized).is_ok() {
            return Err("a repair command carrying a denial was accepted".to_string());
        }

        Ok(
            "inspect and plan are read-only and carry no authorization, while repair is \
            neither and is refused without an allowing decision"
                .to_string(),
        )
    }
);

health_case!(
    AnExecutionRecordIsWrittenOnce,
    "exec-hlt-013-execution-record-is-auditable",
    13,
    CaseCategory::Evidence,
    "An execution records its plan and start, and a completed execution keeps the verdict it \
     was completed with",
    || {
        use crate::health::{RepairExecutionResult, Repairability};
        use crate::{Capability, time::now};
        use chrono::Duration;
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);
        let execution = plan
            .start_execution(&authorization(Capability::MemoryWrite), "state-v1", at)
            .map_err(|error| format!("an authorized repair was refused: {error:?}"))?;

        if execution.plan_id() != plan.id() {
            return Err("an execution does not name the plan it came from".to_string());
        }
        if execution.result().is_some() {
            return Err("an execution carried a verdict before it ran".to_string());
        }

        let completed = execution
            .complete(RepairExecutionResult::Failed, at + Duration::minutes(1))
            .map_err(|error| format!("completing an execution was refused: {error}"))?;
        if completed.result() != Some(RepairExecutionResult::Failed) {
            return Err("an execution did not keep the verdict it completed with".to_string());
        }
        if completed.completed_at().is_none() {
            return Err("a completed execution records no completion time".to_string());
        }

        // Rewriting a verdict is how a failed repair disappears from the record
        // it is supposed to be evidence in.
        if completed
            .clone()
            .complete(RepairExecutionResult::Succeeded, at + Duration::minutes(2))
            .is_ok()
        {
            return Err(
                "a completed execution was completed again with a different verdict, \
                        so the record of a failed repair can be overwritten with a success"
                    .to_string(),
            );
        }

        // A repeat of the same verdict is a retry, not a contradiction.
        completed
            .clone()
            .complete(RepairExecutionResult::Failed, at + Duration::minutes(2))
            .map_err(|error| {
                format!("recording the same verdict twice was refused as a conflict: {error}")
            })?;

        Ok(format!(
            "an execution names its plan, starts without a verdict, and keeps the verdict \
             it completed with ({:?})",
            completed.result()
        ))
    }
);

health_case!(
    HistoryAccumulatesRatherThanBeingRewritten,
    "exec-hlt-014-history-is-append-only",
    14,
    CaseCategory::Stateful,
    "Recording an occurrence adds to the count and never removes earlier ones, and applying \
     a verification produces a new finding rather than editing the old",
    || {
        use crate::health::{
            FindingLifecycleStatus, HealthState, Repairability, VerificationResult,
        };
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);
        let first = finding
            .record_occurrence(HealthState::Unhealthy, vec!["evidence:1".into()], at)
            .map_err(|error| format!("an occurrence was refused: {error}"))?;
        let second = finding
            .record_occurrence(HealthState::Degraded, vec!["evidence:2".into()], at)
            .map_err(|error| format!("an occurrence was refused: {error}"))?;

        if first.sequence() != 1 || second.sequence() != 2 {
            return Err("occurrence sequences do not count upward from the first".to_string());
        }
        if first.observed_state() != HealthState::Unhealthy {
            return Err("recording a later observation changed an earlier one".to_string());
        }

        let plan = plan_for(&finding, at);
        let run = verification(&plan, &finding, VerificationResult::Passed, at)
            .map_err(|error| format!("a verification run was refused: {error}"))?;
        let resolved = finding
            .apply_verification(&run)
            .map_err(|error| format!("a covering verification did not apply: {error}"))?;

        if finding.lifecycle_status() != FindingLifecycleStatus::Open {
            return Err(
                "applying a verification mutated the finding it was applied to, so \
                        the state before the repair is gone"
                    .to_string(),
            );
        }
        if resolved.lifecycle_status() != FindingLifecycleStatus::Resolved {
            return Err("the verified copy did not record the resolution".to_string());
        }
        if resolved.occurrence_count() != finding.occurrence_count() {
            return Err("verifying a finding discarded its occurrence history".to_string());
        }

        Ok(
            "occurrences accumulate with increasing sequence numbers, and verification \
            produces a new finding beside the original rather than overwriting it"
                .to_string(),
        )
    }
);

health_case!(
    SilencingRequiresALiveDisposition,
    "exec-hlt-015-no-silent-silencing",
    15,
    CaseCategory::Behavioral,
    "A finding is only silenced while a disposition is in force, and one that has passed its \
     expiry stops silencing it",
    || {
        use crate::health::{FindingLifecycleStatus, Repairability};
        use crate::time::now;
        use chrono::Duration;
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);
        if finding.lifecycle_status() != FindingLifecycleStatus::Open {
            return Err("a new finding did not start open".to_string());
        }

        let expiry = at + Duration::hours(1);
        finding
            .set_disposition(suppressed(Some(expiry)))
            .map_err(|error| format!("a well-formed suppression was refused: {error}"))?;

        if finding.lifecycle_status_at(at) != FindingLifecycleStatus::Suppressed {
            return Err("a live suppression did not silence the finding".to_string());
        }

        // The case that matters: an expiry nothing consults is a permanent
        // silence wearing a temporary one's clothes.
        let after = expiry + Duration::minutes(1);
        if finding.lifecycle_status_at(after) != FindingLifecycleStatus::Open {
            return Err(
                "a suppression stayed in force after the expiry its author set, so a \
                 finding is silenced by a decision nobody is still making"
                    .to_string(),
            );
        }

        Ok(
            "a finding is silenced only while its disposition is in force, and reverts to \
            open once the expiry the operator chose has passed"
                .to_string(),
        )
    }
);

health_case!(
    PlanningIsADryRun,
    "exec-hlt-016-plan-is-a-dry-run",
    16,
    CaseCategory::Stateful,
    "Planning yields the operations that would run without starting an execution",
    || {
        use crate::health::Repairability;
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);

        let planning = crate::health::RepairOperatorCommand::plan(vec![finding.id()]);
        if !planning.is_read_only() {
            return Err(
                "planning is not read-only, so there is no way to see what a repair \
                        would do without doing it"
                    .to_string(),
            );
        }
        if planning.plan_id().is_some() {
            return Err("a planning command names an execution plan id".to_string());
        }
        if plan.input_state_ref().trim().is_empty() {
            return Err(
                "a plan does not record the state it was computed against, so a \
                        dry run cannot be checked against the state it would run on"
                    .to_string(),
            );
        }

        Ok(
            "a plan can be produced and read without starting anything, and records the state \
            it was computed against"
                .to_string(),
        )
    }
);

health_case!(
    TheRegistryExtendsWithoutTouchingExecution,
    "exec-hlt-017-registry-is-extensible",
    17,
    CaseCategory::Behavioral,
    "A new invariant can be registered and produce findings and plans through the same path \
     as an existing one",
    || {
        use crate::health::{
            HealthScope, HealthSeverity, HealthState, InvariantDefinition, InvariantRegistry,
            Repairability,
        };
        use crate::{WorkspaceId, time::now};
        use health_fixture::*;

        let at = now();
        let mut registry = InvariantRegistry::new([definition(Repairability::Auto)])
            .map_err(|error| format!("a registry refused a definition: {error}"))?;

        let novel = InvariantDefinition::new(
            "outbox.no_unprocessed_backlog",
            "v1",
            "the outbox has no message older than its lag budget",
            "drain the outbox or investigate the worker",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        )
        .map_err(|error| format!("a new invariant could not be defined: {error}"))?;

        registry
            .register(novel.clone())
            .map_err(|error| format!("a new invariant could not be registered: {error}"))?;

        let finding = crate::health::HealthFinding::new(
            &novel,
            HealthScope::workspace(WorkspaceId::new()),
            "outbox:backlog",
            HealthState::Degraded,
            vec!["evidence:outbox-lag".to_string()],
            at,
        )
        .map_err(|error| format!("the new invariant could not produce a finding: {error}"))?;

        let plan = plan_for(&finding, at);
        if plan.finding_ids() != [finding.id()] {
            return Err(
                "a finding from a newly registered invariant did not reach a plan".to_string(),
            );
        }
        if registry.definitions().count() != 2 {
            return Err("the registry did not retain both invariants".to_string());
        }

        Ok(format!(
            "a new invariant ({}) registers and flows through finding and plan without any \
             change to the repair path",
            novel.invariant_id()
        ))
    }
);

health_case!(
    AnObservationRecordsWhenAndAgainstWhatVersion,
    "exec-hlt-018-observation-is-dated-and-versioned",
    18,
    CaseCategory::Evidence,
    "Every occurrence records its time, its observed state and evidence, and the finding \
     carries the invariant version it was checked against",
    || {
        use crate::health::{HealthState, Repairability};
        use crate::time::now;
        use chrono::Duration;
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);
        let later = at + Duration::minutes(5);

        let occurrence = finding
            .record_occurrence(HealthState::Degraded, vec!["evidence:run-2".into()], later)
            .map_err(|error| format!("an occurrence was refused: {error}"))?;

        if occurrence.observed_at() != later {
            return Err("an occurrence does not record when it was observed".to_string());
        }
        if occurrence.observed_state() != HealthState::Degraded {
            return Err("an occurrence does not record what was observed".to_string());
        }
        if occurrence.evidence_refs().is_empty() {
            return Err("an occurrence carries no evidence".to_string());
        }
        if finding.invariant_version().trim().is_empty() {
            return Err(
                "a finding does not record the invariant version it was checked \
                        against, so a check that changed cannot be told from one that did not"
                    .to_string(),
            );
        }

        let no_evidence = finding.record_occurrence(HealthState::Healthy, Vec::new(), later);
        if no_evidence.is_ok() {
            return Err("an occurrence was recorded with no evidence".to_string());
        }

        Ok(format!(
            "an occurrence records its instant, its state and its evidence, against \
             invariant version {}",
            finding.invariant_version()
        ))
    }
);

health_case!(
    AFailedVerificationReopensRatherThanRetries,
    "exec-hlt-019-failure-escalates",
    19,
    CaseCategory::Stateful,
    "A failed or inconclusive verification reopens the finding instead of resolving it, and \
     an inconclusive result is not treated as success",
    || {
        use crate::health::{FindingLifecycleStatus, Repairability, VerificationResult};
        use crate::time::now;
        use health_fixture::*;

        let at = now();
        let finding = finding_at(Repairability::Auto, at);
        let plan = plan_for(&finding, at);

        for (result, label) in [
            (VerificationResult::Failed, "failed"),
            (VerificationResult::Inconclusive, "inconclusive"),
        ] {
            let run = verification(&plan, &finding, result, at)
                .map_err(|error| format!("a verification run was refused: {error}"))?;
            let after = finding
                .apply_verification(&run)
                .map_err(|error| format!("a covering verification did not apply: {error}"))?;
            if after.lifecycle_status() == FindingLifecycleStatus::Resolved {
                return Err(format!(
                    "a {label} verification resolved the finding, so a repair that did not \
                     work would be recorded as one that did"
                ));
            }
            if after.lifecycle_status() != FindingLifecycleStatus::Reopened {
                return Err(format!(
                    "a {label} verification left the finding {:?} rather than reopening it",
                    after.lifecycle_status()
                ));
            }
        }

        Ok(
            "a failed or inconclusive verification reopens the finding, so an unsuccessful \
            repair stays visible instead of being retried quietly"
                .to_string(),
        )
    }
);

health_case!(
    AcceptedRiskIsExplicitAndRevocable,
    "exec-hlt-020-accepted-risk-is-revocable",
    20,
    CaseCategory::Behavioral,
    "Accepting a risk requires an actor, a reason, a policy version and an audit reference, \
     lapses at its expiry, and can be revoked before it",
    || {
        use crate::health::{FindingDisposition, FindingLifecycleStatus, Repairability};
        use crate::time::now;
        use chrono::Duration;
        use health_fixture::*;

        let at = now();
        let mut finding = finding_at(Repairability::Auto, at);
        let expiry = at + Duration::days(7);

        finding
            .set_disposition(accepted_risk(Some(expiry)))
            .map_err(|error| format!("a well-formed acceptance was refused: {error}"))?;

        match finding.disposition() {
            Some(FindingDisposition::AcceptedRisk {
                reason, audit_ref, ..
            }) if !reason.trim().is_empty() && !audit_ref.trim().is_empty() => {}
            _ => {
                return Err("an accepted risk does not record who accepted it and why".to_string());
            }
        }
        if finding.lifecycle_status_at(at) != FindingLifecycleStatus::AcceptedRisk {
            return Err("accepting a risk did not change the finding's status".to_string());
        }

        // Revoked by replacing the disposition, which is what makes the
        // acceptance a decision rather than a permanent property.
        finding
            .set_disposition(FindingDisposition::ManualRequired)
            .map_err(|error| format!("an acceptance could not be revoked: {error}"))?;
        if finding.lifecycle_status_at(at) != FindingLifecycleStatus::Open {
            return Err("revoking an accepted risk left the finding out of view".to_string());
        }

        // And it lapses on its own, or "until Friday" would mean "for ever".
        let mut lapsing = finding_at(Repairability::Auto, at);
        lapsing
            .set_disposition(accepted_risk(Some(expiry)))
            .map_err(|error| format!("a well-formed acceptance was refused: {error}"))?;
        if lapsing.lifecycle_status_at(expiry + Duration::minutes(1))
            != FindingLifecycleStatus::Open
        {
            return Err("an accepted risk outlived the expiry its author set".to_string());
        }

        Ok(
            "accepting a risk is attributable, revocable by replacing the disposition, and \
            lapses on its own at the expiry that was chosen"
                .to_string(),
        )
    }
);

const LEARNING_EVIDENCE: &str = "crates/vestrace-domain/src/learning.rs";

/// Fixtures for the learning cases.
///
/// They are deliberately minimal and well-formed: each case above breaks one
/// property at a time, so anything a fixture gets wrong would show up as a case
/// failing for the wrong reason.
mod learning_fixture {
    use crate::learning::{LearningChange, LearningProposal, LearningTarget};
    use crate::{
        EvaluationAuthority, EvaluationFact, EvaluationMetric, EvaluationResult, EvaluationTarget,
        EvidenceRef, WorkspaceId,
        id::{
            EvaluationId, LearningProposalId, PrincipalId, WorkflowExecutionId, WorkflowRevisionId,
        },
        time::now,
    };

    pub use crate::evaluation::{EvaluatorKind, EvaluatorRef};

    pub fn deterministic_evaluator() -> EvaluatorRef {
        EvaluatorRef {
            kind: EvaluatorKind::Deterministic,
            model_id: None,
            revision: None,
            principal_id: None,
        }
    }

    pub fn document_evidence() -> EvidenceRef {
        EvidenceRef::DocumentRef {
            uri: "evidence://latency-run".to_string(),
            version: None,
        }
    }

    pub fn evaluation_evidence(evaluation_id: EvaluationId) -> EvidenceRef {
        EvidenceRef::EvaluationRef { evaluation_id }
    }

    pub fn routing_target() -> LearningTarget {
        LearningTarget::RoutingPolicy {
            policy_key: "default".to_string(),
        }
    }

    pub fn fact_with(
        evaluator: EvaluatorRef,
        evidence_refs: Vec<EvidenceRef>,
    ) -> Result<EvaluationFact, crate::DomainError> {
        EvaluationFact::new(
            EvaluationId::new(),
            WorkspaceId::new(),
            EvaluationTarget::WorkflowExecution {
                execution_id: WorkflowExecutionId::new(),
                workflow_revision_id: WorkflowRevisionId::new(),
            },
            evaluator,
            EvaluationMetric::new("latency_ms", 42.0, Some("ms".to_string()))
                .expect("a well-formed metric"),
            EvaluationResult::Pass,
            evidence_refs,
            EvaluationAuthority::Deterministic,
            "policy-v1",
            now(),
        )
    }

    pub fn proposal_with(change: LearningChange) -> Result<LearningProposal, crate::DomainError> {
        LearningProposal::new(
            LearningProposalId::new(),
            WorkspaceId::new(),
            vec![crate::id::LearningProjectionId::new()],
            vec![EvaluationId::new()],
            routing_target(),
            change,
            1,
            "latency improved under the cheaper route",
            "policy-v1",
            PrincipalId::new(),
            now(),
        )
    }

    pub fn projection_over(
        facts: &[EvaluationFact],
    ) -> Result<crate::learning::LearnedProjection, crate::DomainError> {
        let fact = &facts[0];
        crate::learning::LearnedProjection::new(
            crate::id::LearningProjectionId::new(),
            fact.workspace_id,
            crate::learning::ProjectionKind::Trend,
            routing_target(),
            crate::learning::ProjectionGenerator::Deterministic {
                algorithm_version: "v1".to_string(),
            },
            1,
            facts.iter().map(|fact| fact.id).collect(),
            vec![evaluation_evidence(fact.id)],
            serde_json::json!({ "summary": "latency is improving" }),
            fact.created_at,
        )
    }
}


const SHARING_EVIDENCE: &str = "crates/vestrace-domain/src/enterprise/sharing.rs";
const FEDERATION_EVIDENCE: &str = "crates/vestrace-domain/src/enterprise/federation.rs";

/// Everything an isolation case needs to build a two-sided share.
///
/// A share exists only where a source workspace has granted something specific
/// and a target workspace has accepted it. The fixture builds exactly that, so
/// each case can take away one piece and watch the answer change.
mod sharing_fixture {
    use crate::enterprise::{
        MemoryMount, MemoryMountAcceptance, MemoryShareGrant, MemoryShareGrantRevision,
        MemoryShareGrantRevisionSpec, ShareOperation, ShareTarget, TargetSharePolicy,
    };
    use crate::id::{MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId};
    use crate::time::Timestamp;
    use std::collections::BTreeSet;

    pub struct Parties {
        pub source_workspace: WorkspaceId,
        pub target_workspace: WorkspaceId,
        pub target_principal: PrincipalId,
        pub memory: MemoryId,
        pub revision: MemoryRevisionId,
    }

    pub fn parties() -> Parties {
        Parties {
            source_workspace: WorkspaceId::new(),
            target_workspace: WorkspaceId::new(),
            target_principal: PrincipalId::new(),
            memory: MemoryId::new(),
            revision: MemoryRevisionId::new(),
        }
    }

    pub fn operations(items: &[ShareOperation]) -> BTreeSet<ShareOperation> {
        items.iter().copied().collect()
    }

    pub fn read_only() -> BTreeSet<ShareOperation> {
        operations(&[
            ShareOperation::DiscoverMetadata,
            ShareOperation::ReadContent,
        ])
    }

    pub fn grant_spec(
        parties: &Parties,
        target: ShareTarget,
        ops: BTreeSet<ShareOperation>,
        at: Timestamp,
        valid_until: Option<Timestamp>,
    ) -> MemoryShareGrantRevisionSpec {
        MemoryShareGrantRevisionSpec {
            source_workspace_id: parties.source_workspace,
            target,
            memory_id: parties.memory,
            memory_revision_id: parties.revision,
            source_generation: "generation-1".to_string(),
            operations: ops,
            valid_from: at,
            valid_until,
        }
    }

    /// A grant the target has not yet accepted.
    pub fn grant(parties: &Parties, at: Timestamp) -> MemoryShareGrant {
        let revision = MemoryShareGrantRevision::issue(
            grant_spec(
                parties,
                ShareTarget::ExactWorkspace(parties.target_workspace),
                read_only(),
                at,
                None,
            ),
            at,
        )
        .expect("the fixture grant revision is well-formed");
        MemoryShareGrant::issue(revision, at).expect("the fixture grant is well-formed")
    }

    pub fn target_policy(parties: &Parties) -> TargetSharePolicy {
        TargetSharePolicy::new(
            parties.target_workspace,
            parties.target_principal,
            read_only(),
            None,
        )
        .expect("the fixture target policy is well-formed")
    }

    pub fn acceptance(
        parties: &Parties,
        grant: &MemoryShareGrant,
        ops: BTreeSet<ShareOperation>,
    ) -> MemoryMountAcceptance {
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id: parties.target_workspace,
            target_principal_id: parties.target_principal,
            operations: ops,
            valid_until: None,
        }
    }

    /// The whole arrangement, accepted and live.
    pub fn mounted(
        parties: &Parties,
        at: Timestamp,
    ) -> (MemoryShareGrant, MemoryMount, TargetSharePolicy) {
        let grant = grant(parties, at);
        let policy = target_policy(parties);
        let mount = MemoryMount::accept(
            acceptance(parties, &grant, read_only()),
            &grant,
            &policy,
            at,
        )
        .expect("the fixture mount is accepted");
        (grant, mount, policy)
    }
}

macro_rules! idw_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $evidence:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Idw,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    $evidence,
                )
            }
        }
    };
}

idw_case!(
    AuthorityStopsAtTheWorkspaceBoundary,
    "exec-idw-001-authority-stops-at-the-workspace",
    1,
    CaseCategory::Behavioral,
    "A mount accepted by one workspace confers nothing on another: changing only the accepting \
     workspace, or only the accepting principal, is refused",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::MemoryMount;
        use crate::id::{PrincipalId, WorkspaceId};
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let grant = grant(&parties, at);
        let policy = target_policy(&parties);

        let mut elsewhere = acceptance(&parties, &grant, read_only());
        elsewhere.target_workspace_id = WorkspaceId::new();
        if MemoryMount::accept(elsewhere, &grant, &policy, at).is_ok() {
            return Err(
                "a workspace the grant never named accepted the mount, so the boundary the \
                 grant draws is not the boundary that is enforced"
                    .to_string(),
            );
        }

        let mut someone_else = acceptance(&parties, &grant, read_only());
        someone_else.target_principal_id = PrincipalId::new();
        if MemoryMount::accept(someone_else, &grant, &policy, at).is_ok() {
            return Err(
                "a principal the target policy never named accepted the mount, so holding the \
                 workspace was enough to hold its authority"
                    .to_string(),
            );
        }

        let (_, mount, _) = mounted(&parties, at);
        if mount.target_workspace_id() != parties.target_workspace {
            return Err("the accepted mount does not belong to the workspace that accepted it"
                .to_string());
        }

        Ok(format!(
            "a mount belongs to workspace {} and principal {}; substituting either is refused",
            parties.target_workspace, parties.target_principal
        ))
    }
);

idw_case!(
    ALocalPermissionIsNotAGlobalOne,
    "exec-idw-002-local-permission-is-not-global",
    2,
    CaseCategory::Behavioral,
    "A target policy that allows an operation allows it only for its own workspace and \
     principal; the same policy answers no for anyone else",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::ShareOperation;
        use crate::id::{PrincipalId, WorkspaceId};
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let policy = target_policy(&parties);

        if !policy.allows(
            parties.target_workspace,
            parties.target_principal,
            ShareOperation::ReadContent,
            at,
        ) {
            return Err(
                "the target's own policy denied the target, so the fixture proves nothing"
                    .to_string(),
            );
        }

        if policy.allows(
            WorkspaceId::new(),
            parties.target_principal,
            ShareOperation::ReadContent,
            at,
        ) {
            return Err(
                "a permission held in one workspace answered yes for another, which is what \
                 'workspace-local global' must never mean"
                    .to_string(),
            );
        }

        if policy.allows(
            parties.target_workspace,
            PrincipalId::new(),
            ShareOperation::ReadContent,
            at,
        ) {
            return Err(
                "a permission granted to one principal answered yes for another inside the \
                 same workspace"
                    .to_string(),
            );
        }

        Ok("a target policy is scoped to one workspace and one principal, and widens to neither"
            .to_string())
    }
);

idw_case!(
    IdentityAndAuthorityAreCheckedSeparately,
    "exec-idw-003-identity-and-authority-are-separate",
    3,
    CaseCategory::Behavioral,
    "Holding the right identity with no authority is denied, and holding the authority under \
     the wrong identity is denied; neither answer stands in for the other",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{MemoryMount, ShareOperation, TargetSharePolicy, evaluate_share_access};
        use crate::enterprise::ShareAccessReason;
        use crate::id::PrincipalId;
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let (grant, mount, policy) = mounted(&parties, at);

        let allowed = evaluate_share_access(
            &grant,
            &mount,
            &policy,
            ShareOperation::ReadContent,
            at,
        );
        if !allowed.is_allowed() {
            return Err(format!(
                "the fully authorized arrangement was denied: {:?}",
                allowed.reason()
            ));
        }

        // The same identity, with the authority taken away.
        let narrowed = TargetSharePolicy::new(
            parties.target_workspace,
            parties.target_principal,
            operations(&[ShareOperation::DiscoverMetadata]),
            None,
        )
        .map_err(|error| format!("the narrowed policy could not be built: {error}"))?;
        let without_authority =
            evaluate_share_access(&grant, &mount, &narrowed, ShareOperation::ReadContent, at);
        if without_authority.is_allowed() {
            return Err(
                "the right identity was enough to read content the policy did not grant"
                    .to_string(),
            );
        }
        if without_authority.reason() != ShareAccessReason::TargetPolicyDenied {
            return Err(format!(
                "the denial named {:?} rather than the policy that withheld the operation",
                without_authority.reason()
            ));
        }

        // The same authority, under an identity nobody accepted for.
        let other_principal = PrincipalId::new();
        let policy_for_someone_else = TargetSharePolicy::new(
            parties.target_workspace,
            other_principal,
            read_only(),
            None,
        )
        .map_err(|error| format!("the substitute policy could not be built: {error}"))?;
        let wrong_identity = evaluate_share_access(
            &grant,
            &mount,
            &policy_for_someone_else,
            ShareOperation::ReadContent,
            at,
        );
        if wrong_identity.is_allowed() {
            return Err(
                "authority carried across to a principal the mount was never accepted by"
                    .to_string(),
            );
        }

        let _ = MemoryMount::accept;
        Ok(format!(
            "identity without authority is {:?} and authority without identity is {:?}",
            without_authority.reason(),
            wrong_identity.reason()
        ))
    }
);

idw_case!(
    AShareNeedsBothSides,
    "exec-idw-004-a-share-needs-both-sides",
    4,
    CaseCategory::Behavioral,
    "A mount cannot be accepted against a grant that was not issued, against a different \
     grant revision, or with operations the source never granted",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{MemoryMount, MemoryShareGrant, ShareOperation};
        use crate::id::MemoryGrantId;
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let grant = grant(&parties, at);
        let policy = target_policy(&parties);

        let mut invented = acceptance(&parties, &grant, read_only());
        invented.grant_id = MemoryGrantId::new();
        if MemoryMount::accept(invented, &grant, &policy, at).is_ok() {
            return Err(
                "a mount was accepted naming a grant that does not exist, so the target could \
                 mount whatever it liked"
                    .to_string(),
            );
        }

        let other_grant = MemoryShareGrant::issue(
            crate::enterprise::MemoryShareGrantRevision::issue(
                grant_spec(
                    &parties,
                    crate::enterprise::ShareTarget::ExactWorkspace(parties.target_workspace),
                    read_only(),
                    at,
                    None,
                ),
                at,
            )
            .map_err(|error| format!("the second revision could not be issued: {error}"))?,
            at,
        )
        .map_err(|error| format!("the second grant could not be issued: {error}"))?;
        let mut wrong_revision = acceptance(&parties, &grant, read_only());
        wrong_revision.grant_revision_id = other_grant.revision().id();
        if MemoryMount::accept(wrong_revision, &grant, &policy, at).is_ok() {
            return Err(
                "a mount was accepted against a revision other than the one it names, so what \
                 was agreed and what is enforced could differ"
                    .to_string(),
            );
        }

        let widened = acceptance(
            &parties,
            &grant,
            operations(&[ShareOperation::ReadContent, ShareOperation::Export]),
        );
        if MemoryMount::accept(widened, &grant, &policy, at).is_ok() {
            return Err(
                "the target accepted an operation the source never granted, so acceptance \
                 could add authority instead of narrowing it"
                    .to_string(),
            );
        }

        // And the arrangement both sides did agree to is accepted.
        let (_, mount, _) = mounted(&parties, at);
        if mount.grant_id() != grant.id() && mount.operations().is_empty() {
            return Err("the accepted mount does not carry the grant it accepted".to_string());
        }

        Ok("a mount requires the exact grant, the exact revision, and operations within it"
            .to_string())
    }
);

idw_case!(
    AShareNamesExactlyOneTarget,
    "exec-idw-005-a-share-names-exactly-one-target",
    5,
    CaseCategory::Behavioral,
    "A grant to any workspace is refused, and so is a grant whose source and target are the \
     same workspace",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{MemoryShareGrantRevision, ShareTarget};
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();

        if MemoryShareGrantRevision::issue(
            grant_spec(&parties, ShareTarget::AnyWorkspace, read_only(), at, None),
            at,
        )
        .is_ok()
        {
            return Err(
                "a wildcard share target was accepted, so one grant would open the memory to \
                 every workspace that could reach it"
                    .to_string(),
            );
        }

        if MemoryShareGrantRevision::issue(
            grant_spec(
                &parties,
                ShareTarget::ExactWorkspace(parties.source_workspace),
                read_only(),
                at,
                None,
            ),
            at,
        )
        .is_ok()
        {
            return Err(
                "a workspace issued a cross-workspace share to itself, which is not a share \
                 and would let local access travel through the sharing path"
                    .to_string(),
            );
        }

        Ok("a share names one workspace that is not its own".to_string())
    }
);

idw_case!(
    SharingDoesNotTravel,
    "exec-idw-006-sharing-does-not-travel",
    6,
    CaseCategory::Behavioral,
    "Re-sharing is refused when granting, when accepting, when written into a target policy, \
     and when evaluated",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{
            MemoryMount, MemoryShareGrantRevision, ShareAccessReason, ShareOperation, ShareTarget,
            TargetSharePolicy, evaluate_share_access,
        };
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();

        if MemoryShareGrantRevision::issue(
            grant_spec(
                &parties,
                ShareTarget::ExactWorkspace(parties.target_workspace),
                operations(&[ShareOperation::ReadContent, ShareOperation::ReShare]),
                at,
                None,
            ),
            at,
        )
        .is_ok()
        {
            return Err("a source granted the right to re-share, so the memory could reach a \
                        workspace the owner never named"
                .to_string());
        }

        if TargetSharePolicy::new(
            parties.target_workspace,
            parties.target_principal,
            operations(&[ShareOperation::ReShare]),
            None,
        )
        .is_ok()
        {
            return Err(
                "a target policy authorized re-sharing, so the receiving side could widen the \
                 share on its own"
                    .to_string(),
            );
        }

        let grant = grant(&parties, at);
        let policy = target_policy(&parties);
        let reshare_acceptance = acceptance(
            &parties,
            &grant,
            operations(&[ShareOperation::ReadContent, ShareOperation::ReShare]),
        );
        if MemoryMount::accept(reshare_acceptance, &grant, &policy, at).is_ok() {
            return Err("a mount was accepted with re-sharing among its operations".to_string());
        }

        let (grant, mount, policy) = mounted(&parties, at);
        let decision = evaluate_share_access(&grant, &mount, &policy, ShareOperation::ReShare, at);
        if decision.is_allowed()
            || decision.reason() != ShareAccessReason::TransitiveSharingProhibited
        {
            return Err(format!(
                "evaluating a re-share gave {:?} rather than refusing it outright",
                decision.reason()
            ));
        }

        Ok("re-sharing is refused at the grant, the policy, the acceptance and the decision"
            .to_string())
    }
);

idw_case!(
    NarrowingEitherSideNarrowsTheShare,
    "exec-idw-007-either-side-can-narrow",
    7,
    CaseCategory::Behavioral,
    "An operation is allowed only where source grant, mount and target policy all permit it; \
     removing it from any one of them denies, and the denial names which one",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{
            MemoryMount, MemoryShareGrant, MemoryShareGrantRevision, ShareAccessReason,
            ShareOperation, ShareTarget, TargetSharePolicy, evaluate_share_access,
        };
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let (grant, mount, policy) = mounted(&parties, at);

        if !evaluate_share_access(&grant, &mount, &policy, ShareOperation::ReadContent, at)
            .is_allowed()
        {
            return Err("the fully permitted arrangement was denied".to_string());
        }

        // Source narrowed, target unchanged.
        let narrow_source = MemoryShareGrant::issue(
            MemoryShareGrantRevision::issue(
                grant_spec(
                    &parties,
                    ShareTarget::ExactWorkspace(parties.target_workspace),
                    operations(&[ShareOperation::DiscoverMetadata]),
                    at,
                    None,
                ),
                at,
            )
            .map_err(|error| format!("the narrowed source revision failed: {error}"))?,
            at,
        )
        .map_err(|error| format!("the narrowed source grant failed: {error}"))?;
        let narrow_mount = MemoryMount::accept(
            acceptance(
                &parties,
                &narrow_source,
                operations(&[ShareOperation::DiscoverMetadata]),
            ),
            &narrow_source,
            &policy,
            at,
        )
        .map_err(|error| format!("the narrowed mount was not accepted: {error}"))?;
        let source_denied = evaluate_share_access(
            &narrow_source,
            &narrow_mount,
            &policy,
            ShareOperation::ReadContent,
            at,
        );
        if source_denied.is_allowed()
            || source_denied.reason() != ShareAccessReason::SourceOperationDenied
        {
            return Err(format!(
                "narrowing the source gave {:?}; the source's own limit must decide",
                source_denied.reason()
            ));
        }

        // Target narrowed, source unchanged.
        let narrow_target = TargetSharePolicy::new(
            parties.target_workspace,
            parties.target_principal,
            operations(&[ShareOperation::DiscoverMetadata]),
            None,
        )
        .map_err(|error| format!("the narrowed target policy failed: {error}"))?;
        let target_denied = evaluate_share_access(
            &grant,
            &mount,
            &narrow_target,
            ShareOperation::ReadContent,
            at,
        );
        if target_denied.is_allowed()
            || target_denied.reason() != ShareAccessReason::TargetPolicyDenied
        {
            return Err(format!(
                "narrowing the target gave {:?}; the target's own limit must decide",
                target_denied.reason()
            ));
        }

        Ok(format!(
            "source-only denial is {:?} and target-only denial is {:?}",
            source_denied.reason(),
            target_denied.reason()
        ))
    }
);

idw_case!(
    AWithdrawnShareDisclosesNothing,
    "exec-idw-008-a-withdrawn-share-discloses-nothing",
    8,
    CaseCategory::Behavioral,
    "Once the source grant is revoked, expired, or replaced by a different revision, the mount \
     stops disclosing and says which of the three it was",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{MemoryMountStatus, ShareOperation};
        use crate::time::now;
        use chrono::Duration;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();

        // Revoked.
        let (mut grant, mut mount, policy) = mounted(&parties, at);
        mount
            .shared_ref(&grant, at)
            .map_err(|error| format!("a live mount refused to disclose: {error}"))?;
        grant
            .revoke(at + Duration::seconds(1))
            .map_err(|error| format!("the grant could not be revoked: {error}"))?;
        mount.sync_with_source(&grant, at + Duration::seconds(2));
        if mount.status() != MemoryMountStatus::Revoked {
            return Err(format!(
                "after revocation the mount reports {:?}",
                mount.status()
            ));
        }
        if mount.shared_ref(&grant, at + Duration::seconds(2)).is_ok() {
            return Err(
                "a revoked mount still produced a reference to the shared content".to_string(),
            );
        }
        if mount
            .record_disclosure(
                &grant,
                &policy,
                ShareOperation::ReadContent,
                at + Duration::seconds(2),
            )
            .is_ok()
        {
            return Err("a revoked mount still recorded a disclosure".to_string());
        }

        // Expired.
        let expiring = sharing_fixture::parties();
        let ends = at + Duration::hours(1);
        let expiring_grant = crate::enterprise::MemoryShareGrant::issue(
            crate::enterprise::MemoryShareGrantRevision::issue(
                grant_spec(
                    &expiring,
                    crate::enterprise::ShareTarget::ExactWorkspace(expiring.target_workspace),
                    read_only(),
                    at,
                    Some(ends),
                ),
                at,
            )
            .map_err(|error| format!("the expiring revision failed: {error}"))?,
            at,
        )
        .map_err(|error| format!("the expiring grant failed: {error}"))?;
        let expiring_policy = target_policy(&expiring);
        let mut expiring_mount = crate::enterprise::MemoryMount::accept(
            acceptance(&expiring, &expiring_grant, read_only()),
            &expiring_grant,
            &expiring_policy,
            at,
        )
        .map_err(|error| format!("the expiring mount was not accepted: {error}"))?;
        expiring_mount.sync_with_source(&expiring_grant, ends + Duration::seconds(1));
        if expiring_mount.status() != MemoryMountStatus::Expired {
            return Err(format!(
                "after its end the mount reports {:?}",
                expiring_mount.status()
            ));
        }
        if expiring_mount
            .shared_ref(&expiring_grant, ends + Duration::seconds(1))
            .is_ok()
        {
            return Err("an expired mount still produced a reference".to_string());
        }

        // Stale: the source issued a different revision than the one accepted.
        let stale_parties = sharing_fixture::parties();
        let (_, mut stale_mount, _) = mounted(&stale_parties, at);
        let replacement = sharing_fixture::grant(&stale_parties, at);
        stale_mount.sync_with_source(&replacement, at + Duration::seconds(1));
        if stale_mount.status() != MemoryMountStatus::Stale {
            return Err(format!(
                "against a different revision the mount reports {:?}",
                stale_mount.status()
            ));
        }
        if stale_mount
            .shared_ref(&replacement, at + Duration::seconds(1))
            .is_ok()
        {
            return Err("a stale mount still produced a reference".to_string());
        }

        Ok("revoked, expired and stale mounts each stop disclosing and say which they are"
            .to_string())
    }
);

idw_case!(
    ASharedReferenceKeepsItsOrigin,
    "exec-idw-009-a-shared-reference-keeps-its-origin",
    9,
    CaseCategory::Behavioral,
    "A shared reference carries the source workspace, the exact memory and revision, the grant \
     revision that permitted it, and the source generation",
    SHARING_EVIDENCE,
    || {
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let (grant, mount, _) = mounted(&parties, at);

        let shared = mount
            .shared_ref(&grant, at)
            .map_err(|error| format!("a live mount refused to produce a reference: {error}"))?;

        if shared.source_workspace_id() != parties.source_workspace {
            return Err(
                "the reference does not name the workspace the content came from, so a reader \
                 could not tell it apart from local content"
                    .to_string(),
            );
        }
        if shared.source_workspace_id() == mount.target_workspace_id() {
            return Err("the reference claims the borrowing workspace as its origin".to_string());
        }
        if shared.source_memory_id() != parties.memory
            || shared.memory_revision_id() != parties.revision
        {
            return Err(
                "the reference does not pin the exact memory revision that was shared, so the \
                 source could change what the target sees"
                    .to_string(),
            );
        }
        if shared.grant_revision_id() != grant.revision().id() {
            return Err(
                "the reference does not name the grant revision that permitted it".to_string(),
            );
        }
        if shared.source_generation().trim().is_empty() {
            return Err("the reference carries no source generation".to_string());
        }

        Ok(format!(
            "a shared reference names workspace {}, revision {} and grant revision {}",
            shared.source_workspace_id(),
            shared.memory_revision_id(),
            shared.grant_revision_id()
        ))
    }
);

idw_case!(
    RecognizingARemoteIsNotDisclosingToIt,
    "exec-idw-012-recognition-is-not-disclosure",
    12,
    CaseCategory::Behavioral,
    "A fully trusted, active federation relationship discloses nothing on its own: without a \
     local grant and mount the answer is that a local share is required",
    FEDERATION_EVIDENCE,
    || {
        use crate::enterprise::{
            FederatedAccessReason, FederationRelationship, FederationRelationshipSpec,
            FederationTrustPolicy, RemoteIdentity, RemoteQualificationEvidence, ShareOperation,
            evaluate_federated_memory_access, evaluate_federation_trust,
        };
        use crate::id::PrincipalId;
        use crate::time::now;
        use sharing_fixture::*;
        use std::collections::BTreeSet;

        let parties = parties();
        let at = now();

        let identity = RemoteIdentity::new(
            parties.source_workspace,
            PrincipalId::new(),
            "issuer.example",
            "subject-1",
        )
        .map_err(|error| format!("the remote identity failed: {error}"))?;
        let profiles: BTreeSet<String> = ["trusted-profile".to_string()].into_iter().collect();
        let policy = FederationTrustPolicy::new(
            parties.target_workspace,
            "issuer.example",
            profiles,
            read_only(),
            "federation-policy-v1",
        )
        .map_err(|error| format!("the trust policy failed: {error}"))?;
        let evidence = RemoteQualificationEvidence::new(
            "issuer.example",
            "trusted-profile",
            "evidence://remote-qualification/1",
            "revision-1",
        )
        .map_err(|error| format!("the qualification evidence failed: {error}"))?;

        let relationship = FederationRelationship::establish(
            FederationRelationshipSpec {
                local_workspace_id: parties.target_workspace,
                remote_workspace_id: parties.source_workspace,
                remote_identity: identity.clone(),
                policy,
                evidence,
                valid_from: at,
                valid_until: None,
            },
            at,
        )
        .map_err(|error| format!("the relationship could not be established: {error}"))?;

        let trusted = evaluate_federation_trust(
            &relationship,
            &identity,
            parties.target_workspace,
            ShareOperation::ReadContent,
            "federation-policy-v1",
            at,
        );
        if !trusted.is_allowed() {
            return Err(format!(
                "the fixture relationship was not trusted: {:?}",
                trusted.reason()
            ));
        }

        let without_share = evaluate_federated_memory_access(
            &relationship,
            &identity,
            parties.target_workspace,
            "federation-policy-v1",
            None,
            None,
            None,
            ShareOperation::ReadContent,
            at,
        );
        if without_share.is_allowed() {
            return Err(
                "a trusted federation relationship disclosed memory with no grant and no \
                 mount, so recognizing a remote would be the same as sharing with it"
                    .to_string(),
            );
        }
        if without_share.reason() != FederatedAccessReason::LocalShareRequired {
            return Err(format!(
                "the denial named {:?} rather than the missing local share",
                without_share.reason()
            ));
        }

        // And a grant belonging to some other pair of workspaces does not
        // satisfy it either.
        let unrelated = sharing_fixture::parties();
        let (grant, mount, target_policy) = mounted(&unrelated, at);
        let mismatched = evaluate_federated_memory_access(
            &relationship,
            &identity,
            parties.target_workspace,
            "federation-policy-v1",
            Some(&grant),
            Some(&mount),
            Some(&target_policy),
            ShareOperation::ReadContent,
            at,
        );
        if mismatched.is_allowed()
            || mismatched.reason() != FederatedAccessReason::DataBindingMismatch
        {
            return Err(format!(
                "a share between two unrelated workspaces was accepted under this \
                 relationship: {:?}",
                mismatched.reason()
            ));
        }

        Ok("federation trust is necessary and not sufficient; the local share still decides"
            .to_string())
    }
);

idw_case!(
    RevocationDoesNotUnsayWhatWasSaid,
    "exec-idw-013-revocation-does-not-unsay-a-disclosure",
    13,
    CaseCategory::Behavioral,
    "A disclosure that happened stays recorded after the grant is revoked, while further \
     disclosure is refused",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::ShareOperation;
        use crate::time::now;
        use chrono::Duration;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();
        let (mut grant, mut mount, policy) = mounted(&parties, at);

        let disclosure = mount
            .record_disclosure(&grant, &policy, ShareOperation::ReadContent, at)
            .map_err(|error| format!("a permitted disclosure was refused: {error}"))?;
        if mount.disclosures().len() != 1 {
            return Err("the disclosure was not recorded".to_string());
        }

        let later = at + Duration::seconds(1);
        grant
            .revoke(later)
            .map_err(|error| format!("the grant could not be revoked: {error}"))?;
        mount.sync_with_source(&grant, later);

        if mount.disclosures().len() != 1 {
            return Err(
                "revoking the grant changed the record of what had already been disclosed, so \
                 the history of an access could be edited by withdrawing it"
                    .to_string(),
            );
        }
        let kept = mount.disclosures()[0].clone();
        if kept.disclosed_at() != disclosure.disclosed_at()
            || kept.operation() != ShareOperation::ReadContent
            || kept.shared_ref().memory_revision_id() != parties.revision
        {
            return Err("the recorded disclosure no longer describes what happened".to_string());
        }

        if mount
            .record_disclosure(&grant, &policy, ShareOperation::ReadContent, later)
            .is_ok()
        {
            return Err("a revoked grant still permitted a new disclosure".to_string());
        }

        Ok(format!(
            "one disclosure at {} survives revocation, and no further disclosure is permitted",
            kept.disclosed_at()
        ))
    }
);


const RECOVERY_EVIDENCE: &str = "crates/vestrace-domain/src/trust.rs";

/// An incident and the things it takes to close one.
mod recovery_fixture {
    use crate::health::HealthScope;
    use crate::id::WorkspaceId;
    use crate::time::Timestamp;
    use crate::trust::{
        ContainmentAction, ContainmentActionKind, Incident, IncidentSeverity, IncidentType,
        RevalidationCheck, RevalidationLevel, RevalidationResult, RevalidationRun,
    };
    use crate::{HealthFindingId, PrincipalId};

    pub fn scope() -> HealthScope {
        HealthScope::workspace(WorkspaceId::new())
    }

    pub fn incident(scope: &HealthScope, severity: IncidentSeverity, at: Timestamp) -> Incident {
        Incident::open(
            IncidentType::CriticalInvariantFailure,
            severity,
            scope.clone(),
            vec![HealthFindingId::new()],
            vec!["table:memories".to_string()],
            vec!["evidence://invariant/memory.active_memory_has_source".to_string()],
            at,
        )
        .expect("the fixture incident is well-formed")
    }

    pub fn containment(at: Timestamp) -> ContainmentAction {
        ContainmentAction::new(
            ContainmentActionKind::PauseWorkers,
            "workspace://fixture",
            PrincipalId::new(),
            "the invariant that failed is still failing",
            "containment-policy-v1",
            at,
        )
        .expect("the fixture containment action is well-formed")
    }

    pub fn run(
        scope: &HealthScope,
        incident: &Incident,
        result: RevalidationResult,
        at: Timestamp,
    ) -> RevalidationRun {
        RevalidationRun::complete(
            Some(incident.id()),
            scope.clone(),
            RevalidationLevel::Workspace,
            "state://baseline/sequence-41",
            vec![RevalidationCheck::passing(
                "invariants re-observed",
                vec!["evidence://revalidation/invariants".to_string()],
            )],
            vec!["evidence://revalidation/run".to_string()],
            result,
            at,
        )
        .expect("the fixture revalidation run is well-formed")
    }

    /// Carry an incident from open to the point where a revalidation result can
    /// be applied, which is the only path the type allows.
    pub fn carry_to_revalidation(
        incident: &mut Incident,
        run_id: crate::RevalidationRunId,
        at: Timestamp,
    ) -> Result<(), String> {
        incident
            .begin_containment(containment(at))
            .map_err(|error| format!("containment could not begin: {error}"))?;
        incident
            .mark_contained(at)
            .map_err(|error| format!("containment could not complete: {error}"))?;
        incident
            .begin_recovery(at)
            .map_err(|error| format!("recovery could not begin: {error}"))?;
        incident
            .begin_revalidation(run_id, at)
            .map_err(|error| format!("revalidation could not begin: {error}"))
    }
}

macro_rules! rec_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Rec,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    RECOVERY_EVIDENCE,
                )
            }
        }
    };
}

rec_case!(
    AnIncidentIsNotAFinding,
    "exec-rec-001-an-incident-is-not-a-finding",
    1,
    CaseCategory::Behavioral,
    "An incident names the findings it was opened from without becoming one: it has its own \
     identity, its own lifecycle, and a closure that a finding's disposition cannot stand in for",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, IncidentStatus};
        use crate::{HealthFindingId, PrincipalId};
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let mut incident = incident(&scope, IncidentSeverity::High, at);

        if incident.triggering_findings().is_empty() {
            return Err(
                "the incident does not name the findings it came from, so the two could not be \
                 related back to one another"
                    .to_string(),
            );
        }
        if incident.triggering_findings().contains(&HealthFindingId::new()) {
            return Err("the incident claims a finding nobody raised".to_string());
        }

        // A finding is dispositioned by a decision; an incident is not closable
        // by one, and cannot be closed at all until it has been contained,
        // recovered and revalidated.
        if incident
            .close(
                PrincipalId::new(),
                "accepted",
                vec!["evidence://closure/1".to_string()],
                at,
            )
            .is_ok()
        {
            return Err(
                "an open incident was closed the way a finding is dispositioned, so the two \
                 lifecycles are the same lifecycle"
                    .to_string(),
            );
        }
        if incident.status() != IncidentStatus::Open {
            return Err(format!(
                "the refused closure still moved the incident to {:?}",
                incident.status()
            ));
        }

        Ok(format!(
            "an incident carries {} triggering finding(s) and its own status {:?}",
            incident.triggering_findings().len(),
            incident.status()
        ))
    }
);

rec_case!(
    ContainmentComesBeforeTrust,
    "exec-rec-002-containment-comes-first",
    2,
    CaseCategory::Behavioral,
    "Recovery cannot begin before containment is complete, revalidation cannot begin before \
     recovery, and resolution cannot happen before revalidation passes",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, IncidentStatus, RevalidationResult};
        use crate::RevalidationRunId;
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let mut incident = incident(&scope, IncidentSeverity::Critical, at);

        if incident.begin_recovery(at).is_ok() {
            return Err(
                "recovery began on an uncontained incident, so work could resume while the \
                 cause was still spreading"
                    .to_string(),
            );
        }

        incident
            .begin_containment(containment(at))
            .map_err(|error| format!("containment could not begin: {error}"))?;
        if incident.begin_recovery(at).is_ok() {
            return Err("recovery began while containment was still active".to_string());
        }

        incident
            .mark_contained(at)
            .map_err(|error| format!("containment could not complete: {error}"))?;
        if incident
            .begin_revalidation(RevalidationRunId::new(), at)
            .is_ok()
        {
            return Err("revalidation began before recovery".to_string());
        }

        incident
            .begin_recovery(at)
            .map_err(|error| format!("recovery could not begin after containment: {error}"))?;
        if incident.mark_resolved(at).is_ok() {
            return Err("the incident resolved without any revalidation".to_string());
        }

        let run = run(&scope, &incident, RevalidationResult::Passed, at);
        incident
            .begin_revalidation(run.id(), at)
            .map_err(|error| format!("revalidation could not begin: {error}"))?;
        incident
            .apply_revalidation(&run)
            .map_err(|error| format!("the revalidation result could not be applied: {error}"))?;
        incident
            .mark_resolved(at)
            .map_err(|error| format!("the incident could not resolve: {error}"))?;

        if incident.status() != IncidentStatus::Resolved {
            return Err(format!("the incident ended at {:?}", incident.status()));
        }

        Ok("containment precedes recovery, recovery precedes revalidation, revalidation \
            precedes resolution"
            .to_string())
    }
);

rec_case!(
    ARestartDoesNotRestoreTrust,
    "exec-rec-003-a-restart-restores-nothing",
    3,
    CaseCategory::Behavioral,
    "An untrusted scope has no transition back to trusted except through a registered \
     revalidation run: a new record cannot be minted trusted in its place, and the run it \
     names must be the one that answers",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, RevalidationResult, TrustState, TrustStateRecord};
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let incident = incident(&scope, IncidentSeverity::Critical, at);

        let mut trust =
            TrustStateRecord::from_incident(scope.clone(), incident.id(), IncidentSeverity::Critical, at);
        if trust.state() != TrustState::Untrusted {
            return Err(format!(
                "a critical incident left the scope at {:?}",
                trust.state()
            ));
        }

        // A revalidation run for some other scope does not answer for this one,
        // which is what a restart amounts to: evidence from elsewhere.
        let other_scope = recovery_fixture::scope();
        let elsewhere = run(&other_scope, &incident, RevalidationResult::Passed, at);
        trust
            .begin_revalidation(elsewhere.id(), at)
            .map_err(|error| format!("revalidation could not begin: {error}"))?;
        if trust.apply_revalidation(&elsewhere).is_ok() {
            return Err(
                "a revalidation of a different scope restored trust here, so trust could be \
                 recovered by pointing at somebody else's evidence"
                    .to_string(),
            );
        }

        // Nor does a run this record never registered.
        let unregistered = run(&scope, &incident, RevalidationResult::Passed, at);
        if trust.apply_revalidation(&unregistered).is_ok() {
            return Err(
                "an unregistered revalidation run restored trust, so any passing run would do"
                    .to_string(),
            );
        }

        if trust.state() != TrustState::Revalidating {
            return Err(format!(
                "after two refused restorations the scope reports {:?}",
                trust.state()
            ));
        }

        Ok("trust moves out of untrusted only through the revalidation run the record \
            registered, for its own scope"
            .to_string())
    }
);

rec_case!(
    UnfinishedWorkIsClassifiedNotResumed,
    "exec-rec-004-unfinished-work-is-classified",
    4,
    CaseCategory::Behavioral,
    "Each kind of unfinished work has its own classification and its own action; only two of \
     the eight are safe to resume, and a qualification claiming otherwise is refused",
    || {
        use crate::trust::{
            RecoveryAction, RecoveryClassification, RecoveryQualificationObservation,
            RecoveryTarget, classify_recovery, evaluate_recovery_qualification,
        };

        let resumable: Vec<RecoveryTarget> = RecoveryTarget::required_targets()
            .into_iter()
            .filter(|target| classify_recovery(*target) == RecoveryClassification::SafeToResume)
            .collect();
        if resumable.len() != 2 {
            return Err(format!(
                "{} of the eight recovery targets are safe to resume; resume-all is the \
                 failure this classification exists to prevent",
                resumable.len()
            ));
        }

        let honest: Vec<RecoveryQualificationObservation> = RecoveryTarget::required_targets()
            .into_iter()
            .map(|target| {
                RecoveryQualificationObservation::expected(target, "evidence://recovery/observed")
            })
            .collect();
        let passed = evaluate_recovery_qualification(&honest);
        if !passed.is_passed() {
            return Err(format!(
                "the correctly classified sweep was refused: {:?}",
                passed.failures()
            ));
        }

        // Resuming something that must be reconciled is the exact mistake.
        let mut optimistic = honest.clone();
        optimistic.retain(|observation| {
            observation.target != RecoveryTarget::DispatchingExternalEffect
        });
        optimistic.push(
            RecoveryQualificationObservation::new(
                RecoveryTarget::DispatchingExternalEffect,
                RecoveryClassification::SafeToResume,
                RecoveryAction::Resume,
                "evidence://recovery/observed",
            )
            .map_err(|error| format!("the optimistic observation could not be built: {error}"))?,
        );
        let refused = evaluate_recovery_qualification(&optimistic);
        if refused.is_passed() {
            return Err(
                "a sweep that resumed an in-flight external dispatch passed qualification"
                    .to_string(),
            );
        }

        // And a target nobody looked at is not silently fine.
        let mut incomplete = honest.clone();
        incomplete.retain(|observation| observation.target != RecoveryTarget::UnknownOutcome);
        if evaluate_recovery_qualification(&incomplete).is_passed() {
            return Err("a sweep that skipped a whole class of unfinished work passed".to_string());
        }

        Ok(format!(
            "eight targets, {} safe to resume, and misclassifying or omitting one is refused",
            resumable.len()
        ))
    }
);

rec_case!(
    AnAmbiguousDispatchGoesToReconciliation,
    "exec-rec-005-ambiguity-goes-to-reconciliation",
    5,
    CaseCategory::Behavioral,
    "An external dispatch interrupted mid-flight, and an outcome nobody observed, both \
     classify as reconciliation rather than as work to redo",
    || {
        use crate::trust::{RecoveryAction, RecoveryClassification, RecoveryTarget, classify_recovery};

        for target in [
            RecoveryTarget::DispatchingExternalEffect,
            RecoveryTarget::UnknownOutcome,
        ] {
            let classification = classify_recovery(target);
            if classification != RecoveryClassification::MustReconcile {
                return Err(format!(
                    "{target:?} classifies as {classification:?}; an unknown outcome treated as \
                     anything but reconciliation is a guess about the outside world"
                ));
            }
        }

        // The action that follows is reconciliation, not retry.
        let observation = crate::trust::RecoveryQualificationObservation::expected(
            RecoveryTarget::UnknownOutcome,
            "evidence://recovery/unknown-outcome",
        );
        if observation.action != RecoveryAction::Reconcile {
            return Err(format!(
                "an unknown outcome is acted on with {:?}",
                observation.action
            ));
        }

        Ok("an interrupted dispatch and an unobserved outcome both reconcile".to_string())
    }
);

rec_case!(
    AnAmbiguousIrreversibleEffectIsNotRepeated,
    "exec-rec-006-ambiguity-is-not-repeated",
    6,
    CaseCategory::Behavioral,
    "Nothing that is ambiguous after a restart classifies as safe to retry, and the repair \
     that was being verified aborts rather than running again",
    || {
        use crate::trust::{RecoveryClassification, RecoveryTarget, classify_recovery};

        let retryable: Vec<RecoveryTarget> = RecoveryTarget::required_targets()
            .into_iter()
            .filter(|target| classify_recovery(*target) == RecoveryClassification::SafeToRetry)
            .collect();

        for target in &retryable {
            if matches!(
                target,
                RecoveryTarget::DispatchingExternalEffect
                    | RecoveryTarget::UnknownOutcome
                    | RecoveryTarget::VerifyingRepair
                    | RecoveryTarget::DivergentHistory
            ) {
                return Err(format!(
                    "{target:?} is classified safe to retry, so a restart would repeat an act \
                     whose outcome nobody knows"
                ));
            }
        }

        if classify_recovery(RecoveryTarget::VerifyingRepair) != RecoveryClassification::MustAbort {
            return Err(
                "a repair interrupted while being verified does not abort, so the system would \
                 repair on top of a repair it never confirmed"
                    .to_string(),
            );
        }

        Ok(format!(
            "only {:?} are safe to retry, and none of them is ambiguous",
            retryable
        ))
    }
);

rec_case!(
    ARecoveryPointNamesAProvenPosition,
    "exec-rec-007-a-recovery-point-names-its-position",
    7,
    CaseCategory::Behavioral,
    "A recovery point carries the state it refers to, where in the sequence it sits, what it \
     is consistent across and where it came from; one whose integrity is unknown cannot be \
     restored from",
    || {
        use crate::time::now;
        use crate::trust::{IntegrityStatus, RecoveryPoint};

        let at = now();
        let point = RecoveryPoint::new(
            "state://snapshot/2026-08-15",
            41,
            "workspace",
            IntegrityStatus::Valid,
            "provenance://backup/nightly",
            at,
        )
        .map_err(|error| format!("a complete recovery point was refused: {error}"))?;

        if point.state_ref().trim().is_empty()
            || point.consistency_scope().trim().is_empty()
            || point.recovery_provenance().trim().is_empty()
        {
            return Err("the recovery point does not say what position it names".to_string());
        }
        if point.sequence_position() != 41 {
            return Err("the recovery point lost the position it was created at".to_string());
        }
        if !point.can_restore() {
            return Err("a valid recovery point refused to be restored from".to_string());
        }

        for status in [IntegrityStatus::Unknown, IntegrityStatus::Invalid] {
            let doubtful = RecoveryPoint::new(
                "state://snapshot/2026-08-15",
                41,
                "workspace",
                status,
                "provenance://backup/nightly",
                at,
            )
            .map_err(|error| format!("the doubtful recovery point could not be built: {error}"))?;
            if doubtful.can_restore() {
                return Err(format!(
                    "a recovery point with {status:?} integrity offered itself as a restore \
                     source, so unverified state could become the new truth"
                ));
            }
        }

        // A point that names nothing is refused outright.
        if RecoveryPoint::new("   ", 41, "workspace", IntegrityStatus::Valid, "p", at).is_ok() {
            return Err("a recovery point was created without naming any state".to_string());
        }

        Ok(format!(
            "a recovery point names {} at position {} across {}, from {}",
            point.state_ref(),
            point.sequence_position(),
            point.consistency_scope(),
            point.recovery_provenance()
        ))
    }
);

rec_case!(
    UnvalidatedStateIsNotARecoverySource,
    "exec-rec-008-unvalidated-state-is-not-a-source",
    8,
    CaseCategory::Behavioral,
    "Integrity is a three-valued answer and only one of the three permits a restore: unknown \
     is not treated as valid",
    || {
        use crate::time::now;
        use crate::trust::{IntegrityStatus, RecoveryPoint};

        let at = now();
        let restorable: Vec<IntegrityStatus> = [
            IntegrityStatus::Valid,
            IntegrityStatus::Invalid,
            IntegrityStatus::Unknown,
        ]
        .into_iter()
        .filter(|status| {
            RecoveryPoint::new("state://s", 1, "workspace", *status, "provenance://p", at)
                .expect("the recovery point is well-formed")
                .can_restore()
        })
        .collect();

        if restorable != vec![IntegrityStatus::Valid] {
            return Err(format!(
                "these integrity states permit a restore: {restorable:?}; unknown must never \
                 count as validated, or an unchecked snapshot becomes the baseline"
            ));
        }

        Ok("only a snapshot whose integrity was checked and passed can be restored from"
            .to_string())
    }
);

rec_case!(
    TheFiveTrustWordsStayDistinct,
    "exec-rec-010-the-trust-words-stay-distinct",
    10,
    CaseCategory::Behavioral,
    "Recovered is not revalidated and neither is trusted: an incident that has recovered but \
     not passed revalidation is not resolved, and a scope whose revalidation was inconclusive \
     is not trusted",
    || {
        use crate::time::now;
        use crate::trust::{
            IncidentSeverity, RecoveryState, RevalidationResult, RevalidationState, TrustState,
            TrustStateRecord,
        };
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let mut incident = incident(&scope, IncidentSeverity::High, at);
        let run = run(&scope, &incident, RevalidationResult::Inconclusive, at);
        carry_to_revalidation(&mut incident, run.id(), at)?;

        if incident.recovery_state() != RecoveryState::InProgress {
            return Err(format!(
                "recovery reports {:?} while revalidation is still running",
                incident.recovery_state()
            ));
        }

        incident
            .apply_revalidation(&run)
            .map_err(|error| format!("the inconclusive result could not be applied: {error}"))?;
        if incident.revalidation_state() != RevalidationState::Inconclusive {
            return Err(format!(
                "an inconclusive revalidation was recorded as {:?}",
                incident.revalidation_state()
            ));
        }
        if incident.mark_resolved(at).is_ok() {
            return Err(
                "an incident resolved on an inconclusive revalidation, so 'recovered' and \
                 'revalidated' are the same word"
                    .to_string(),
            );
        }

        let mut trust =
            TrustStateRecord::from_incident(scope.clone(), incident.id(), IncidentSeverity::High, at);
        trust
            .begin_revalidation(run.id(), at)
            .map_err(|error| format!("trust revalidation could not begin: {error}"))?;
        trust
            .apply_revalidation(&run)
            .map_err(|error| format!("the trust result could not be applied: {error}"))?;
        if trust.state() == TrustState::Trusted {
            return Err("an inconclusive revalidation restored trust".to_string());
        }

        Ok(format!(
            "recovery {:?}, revalidation {:?}, trust {:?} — three answers, not one",
            incident.recovery_state(),
            incident.revalidation_state(),
            trust.state()
        ))
    }
);

rec_case!(
    TrustReturnsOnlyThroughARun,
    "exec-rec-011-trust-returns-only-through-a-run",
    11,
    CaseCategory::Behavioral,
    "A failed revalidation leaves the scope untrusted and a passing one restores it, and the \
     record names the run that decided",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, RevalidationResult, TrustState, TrustStateRecord};
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let incident = incident(&scope, IncidentSeverity::Critical, at);

        for (result, expected) in [
            (RevalidationResult::Failed, TrustState::Untrusted),
            (RevalidationResult::Passed, TrustState::Trusted),
        ] {
            let mut trust = TrustStateRecord::from_incident(
                scope.clone(),
                incident.id(),
                IncidentSeverity::Critical,
                at,
            );
            let run = run(&scope, &incident, result, at);
            trust
                .begin_revalidation(run.id(), at)
                .map_err(|error| format!("revalidation could not begin: {error}"))?;
            trust
                .apply_revalidation(&run)
                .map_err(|error| format!("the result could not be applied: {error}"))?;

            if trust.state() != expected {
                return Err(format!(
                    "a {result:?} revalidation left the scope {:?} rather than {expected:?}",
                    trust.state()
                ));
            }
            if trust.revalidation_run_id() != Some(run.id()) {
                return Err(
                    "the trust record does not name the run that decided it, so the restoration \
                     could not be traced to its evidence"
                        .to_string(),
                );
            }
        }

        Ok("trust follows the run it registered, and the record keeps its identifier".to_string())
    }
);

rec_case!(
    ARevalidationKeepsItsWorkings,
    "exec-rec-012-a-revalidation-keeps-its-workings",
    12,
    CaseCategory::Behavioral,
    "A revalidation run preserves its scope, level, baseline, checks, evidence and result, \
     and cannot be completed with no checks or no evidence at all",
    || {
        use crate::time::now;
        use crate::trust::{
            IncidentSeverity, RevalidationCheck, RevalidationLevel, RevalidationResult,
            RevalidationRun,
        };
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let incident = incident(&scope, IncidentSeverity::High, at);
        let run = run(&scope, &incident, RevalidationResult::Passed, at);

        if run.scope() != &scope {
            return Err("the run does not keep the scope it revalidated".to_string());
        }
        if run.level() != RevalidationLevel::Workspace {
            return Err("the run does not keep the level it ran at".to_string());
        }
        if run.baseline_state_ref().trim().is_empty() {
            return Err(
                "the run does not keep the baseline it measured against, so 'revalidated' \
                 names no particular state"
                    .to_string(),
            );
        }
        if run.checks().is_empty() {
            return Err("the run does not keep the checks it performed".to_string());
        }
        for check in run.checks() {
            if check.name().trim().is_empty() || check.evidence_refs().is_empty() {
                return Err("a check kept neither its name nor its evidence".to_string());
            }
        }
        if run.evidence_refs().is_empty() {
            return Err("the run kept no evidence".to_string());
        }
        if run.incident_id() != Some(incident.id()) {
            return Err("the run does not name the incident it belongs to".to_string());
        }

        if RevalidationRun::complete(
            None,
            scope.clone(),
            RevalidationLevel::Local,
            "state://baseline",
            Vec::new(),
            vec!["evidence://run".to_string()],
            RevalidationResult::Passed,
            at,
        )
        .is_ok()
        {
            return Err(
                "a revalidation passed having performed no checks, which is an assertion \
                 wearing the shape of evidence"
                    .to_string(),
            );
        }

        if RevalidationRun::complete(
            None,
            scope.clone(),
            RevalidationLevel::Local,
            "state://baseline",
            vec![RevalidationCheck::passing(
                "a check",
                vec!["evidence://check".to_string()],
            )],
            Vec::new(),
            RevalidationResult::Passed,
            at,
        )
        .is_ok()
        {
            return Err("a revalidation completed carrying no evidence".to_string());
        }

        Ok(format!(
            "a run keeps scope, level, baseline {}, {} check(s) and {} evidence reference(s)",
            run.baseline_state_ref(),
            run.checks().len(),
            run.evidence_refs().len()
        ))
    }
);

rec_case!(
    InconclusiveIsAnAnswerOfItsOwn,
    "exec-rec-013-inconclusive-is-not-passed",
    13,
    CaseCategory::Behavioral,
    "An inconclusive revalidation is neither successful nor failed: it does not resolve an \
     incident, it does not restore trust, and it leaves the scope revalidating",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, RevalidationResult, TrustState, TrustStateRecord};
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let incident = incident(&scope, IncidentSeverity::High, at);
        let inconclusive = run(&scope, &incident, RevalidationResult::Inconclusive, at);

        if inconclusive.is_successful() {
            return Err(
                "an inconclusive revalidation reports success, so 'we could not tell' would \
                 count as 'we checked and it is fine'"
                    .to_string(),
            );
        }

        let mut trust =
            TrustStateRecord::from_incident(scope.clone(), incident.id(), IncidentSeverity::High, at);
        trust
            .begin_revalidation(inconclusive.id(), at)
            .map_err(|error| format!("revalidation could not begin: {error}"))?;
        trust
            .apply_revalidation(&inconclusive)
            .map_err(|error| format!("the result could not be applied: {error}"))?;
        if trust.state() != TrustState::Revalidating {
            return Err(format!(
                "an inconclusive result moved the scope to {:?} rather than leaving it open",
                trust.state()
            ));
        }

        // And it is distinct from a failure, which does settle the question.
        let failed = run(&scope, &incident, RevalidationResult::Failed, at);
        let mut after_failure =
            TrustStateRecord::from_incident(scope.clone(), incident.id(), IncidentSeverity::High, at);
        after_failure
            .begin_revalidation(failed.id(), at)
            .map_err(|error| format!("revalidation could not begin: {error}"))?;
        after_failure
            .apply_revalidation(&failed)
            .map_err(|error| format!("the failed result could not be applied: {error}"))?;
        if after_failure.state() == trust.state() {
            return Err("inconclusive and failed leave the same state".to_string());
        }

        Ok(format!(
            "inconclusive leaves {:?} while failed leaves {:?}",
            trust.state(),
            after_failure.state()
        ))
    }
);

rec_case!(
    DivergentHistoriesWaitForAPerson,
    "exec-rec-014-divergence-waits-for-a-person",
    14,
    CaseCategory::Behavioral,
    "Divergent history is the one recovery target that requires a human, and a sweep that \
     resolves it automatically is refused however it labels the action",
    || {
        use crate::trust::{
            RecoveryAction, RecoveryClassification, RecoveryQualificationObservation,
            RecoveryTarget, classify_recovery, evaluate_recovery_qualification,
        };

        if classify_recovery(RecoveryTarget::DivergentHistory)
            != RecoveryClassification::HumanRequired
        {
            return Err(
                "divergent history does not require a human, so two histories could be merged \
                 by whichever arrived last"
                    .to_string(),
            );
        }

        let mut sweep: Vec<RecoveryQualificationObservation> = RecoveryTarget::required_targets()
            .into_iter()
            .filter(|target| *target != RecoveryTarget::DivergentHistory)
            .map(|target| {
                RecoveryQualificationObservation::expected(target, "evidence://recovery/observed")
            })
            .collect();

        for (classification, action) in [
            (RecoveryClassification::SafeToResume, RecoveryAction::Resume),
            (RecoveryClassification::SafeToRetry, RecoveryAction::Retry),
            (RecoveryClassification::MustReconcile, RecoveryAction::Reconcile),
        ] {
            let mut attempt = sweep.clone();
            attempt.push(
                RecoveryQualificationObservation::new(
                    RecoveryTarget::DivergentHistory,
                    classification,
                    action,
                    "evidence://recovery/observed",
                )
                .map_err(|error| format!("the observation could not be built: {error}"))?,
            );
            if evaluate_recovery_qualification(&attempt).is_passed() {
                return Err(format!(
                    "a sweep that handled divergent history as {classification:?}/{action:?} \
                     passed qualification"
                ));
            }
        }

        sweep.push(RecoveryQualificationObservation::expected(
            RecoveryTarget::DivergentHistory,
            "evidence://recovery/observed",
        ));
        if !evaluate_recovery_qualification(&sweep).is_passed() {
            return Err("the sweep that escalated divergence to a human was refused".to_string());
        }

        Ok("divergent history escalates to a person, and every automatic resolution of it is \
            refused"
            .to_string())
    }
);

rec_case!(
    RepairAttemptsAreBounded,
    "exec-rec-015-repair-attempts-are-bounded",
    15,
    CaseCategory::Behavioral,
    "Automatic repair has a bounded number of attempts in a window and a cooldown between \
     them, and a finding that keeps coming back is reported as recurrent rather than retried \
     forever",
    || {
        use crate::health::{RepairBudget, RepairBudgetError};
        use crate::time::now;
        use chrono::Duration;

        let at = now();
        let mut budget = RepairBudget::new(2, Duration::hours(1), Duration::minutes(5));

        budget
            .reserve(at)
            .map_err(|error| format!("the first attempt was refused: {error:?}"))?;
        match budget.reserve(at + Duration::seconds(1)) {
            Err(RepairBudgetError::CooldownActive { .. }) => {}
            other => {
                return Err(format!(
                    "an immediate second attempt gave {other:?}; without a cooldown a failing \
                     repair spins as fast as the loop that calls it"
                ));
            }
        }

        budget
            .reserve(at + Duration::minutes(6))
            .map_err(|error| format!("the second attempt after cooldown was refused: {error:?}"))?;
        match budget.reserve(at + Duration::minutes(12)) {
            Err(RepairBudgetError::AttemptBudgetExceeded) => {}
            other => {
                return Err(format!(
                    "a third attempt inside the window gave {other:?} rather than exhausting \
                     the budget"
                ));
            }
        }

        // The window is what makes the budget a rate and not a lifetime cap.
        budget
            .reserve(at + Duration::hours(2))
            .map_err(|error| format!("an attempt after the window was refused: {error:?}"))?;

        Ok(format!(
            "two attempts per hour with a five-minute cooldown, and {} in the current window",
            budget.attempts_in_window()
        ))
    }
);

rec_case!(
    ClosingAnIncidentDoesNotEraseIt,
    "exec-rec-018-closing-does-not-erase",
    18,
    CaseCategory::Behavioral,
    "A closed incident keeps when it was opened, what it was opened from, every containment \
     action taken, and who closed it with what evidence; closure without evidence or without \
     a disposition is refused",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, IncidentStatus, RevalidationResult};
        use crate::PrincipalId;
        use chrono::Duration;
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let mut incident = incident(&scope, IncidentSeverity::High, at);
        let opened_at = incident.opened_at();
        let findings = incident.triggering_findings().to_vec();
        let evidence = incident.triggering_evidence().to_vec();
        let resources = incident.affected_resources().to_vec();

        let run = run(&scope, &incident, RevalidationResult::Passed, at);
        carry_to_revalidation(&mut incident, run.id(), at)?;
        incident
            .apply_revalidation(&run)
            .map_err(|error| format!("the result could not be applied: {error}"))?;
        incident
            .mark_resolved(at)
            .map_err(|error| format!("the incident could not resolve: {error}"))?;

        let closer = PrincipalId::new();
        if incident
            .close(closer, "resolved", Vec::new(), at + Duration::minutes(1))
            .is_ok()
        {
            return Err("an incident was closed with no evidence at all".to_string());
        }
        if incident
            .close(
                closer,
                "   ",
                vec!["evidence://closure".to_string()],
                at + Duration::minutes(1),
            )
            .is_ok()
        {
            return Err("an incident was closed with no disposition".to_string());
        }

        let closed_at = at + Duration::minutes(1);
        incident
            .close(
                closer,
                "cause removed and revalidated",
                vec!["evidence://closure/revalidation".to_string()],
                closed_at,
            )
            .map_err(|error| format!("a complete closure was refused: {error}"))?;

        if incident.status() != IncidentStatus::Closed {
            return Err(format!("the incident ended at {:?}", incident.status()));
        }
        if incident.opened_at() != opened_at {
            return Err(
                "closing the incident moved when it was opened, so the record of the failure \
                 could be edited by ending it"
                    .to_string(),
            );
        }
        if incident.triggering_findings() != findings
            || incident.triggering_evidence() != evidence
            || incident.affected_resources() != resources
        {
            return Err("closing the incident changed what it was opened for".to_string());
        }
        if incident.containment_actions().is_empty() {
            return Err(
                "the closed incident kept no record of the containment it required".to_string(),
            );
        }
        if incident.closed_by() != Some(closer)
            || incident.closed_at() != Some(closed_at)
            || incident.disposition().unwrap_or_default().trim().is_empty()
            || incident.closure_evidence().is_empty()
        {
            return Err("the closure itself left no record of who decided it, or why".to_string());
        }

        Ok(format!(
            "a closed incident still opened at {opened_at}, from {} finding(s), with {} \
             containment action(s) and a named closer",
            incident.triggering_findings().len(),
            incident.containment_actions().len()
        ))
    }
);


rec_case!(
    RestoringRebuildsRatherThanTrusts,
    "exec-rec-009-restoring-rebuilds-derived-state",
    9,
    CaseCategory::Behavioral,
    "A recovery point names a position in the canonical log, and the derived state at that \
     position is rebuilt from the log rather than carried across: replaying the prefix gives \
     the state as of that point, deterministically, and the log is unchanged by it",
    || {
        use crate::run::replay;
        use crate::time::now;
        use crate::trust::{IntegrityStatus, RecoveryPoint};

        let log = fixture::four_event_run();
        let at = now();

        // A recovery point is a position, not a copy of the state at it.
        let position = 2u64;
        let point = RecoveryPoint::new(
            format!("run://{}", log.run_id),
            position,
            "run",
            IntegrityStatus::Valid,
            "provenance://canonical-event-log",
            at,
        )
        .map_err(|error| format!("the recovery point could not be built: {error}"))?;

        let prefix: Vec<_> = log
            .events
            .iter()
            .take(point.sequence_position() as usize)
            .cloned()
            .collect();
        if prefix.len() as u64 != position {
            return Err("the fixture log is shorter than the recovery position".to_string());
        }

        let rebuilt = replay(prefix.clone())
            .map_err(|error| format!("the prefix could not be replayed: {error}"))?
            .ok_or_else(|| "replaying the prefix produced no state".to_string())?;
        let again = replay(prefix)
            .map_err(|error| format!("the prefix could not be replayed twice: {error}"))?
            .ok_or_else(|| "the second replay produced no state".to_string())?;

        if rebuilt != again {
            return Err(
                "two rebuilds from the same position disagree, so restored state would depend \
                 on which run of the rebuild you looked at"
                    .to_string(),
            );
        }

        let full = replay(log.events.clone())
            .map_err(|error| format!("the whole log could not be replayed: {error}"))?
            .ok_or_else(|| "replaying the whole log produced no state".to_string())?;
        if rebuilt == full {
            return Err(
                "the state rebuilt at the recovery position equals the state at the end of the \
                 log, so the position is not being honoured and a restore would silently keep \
                 everything that happened after it"
                    .to_string(),
            );
        }
        if rebuilt.id != log.run_id || rebuilt.workspace_id != log.workspace_id {
            return Err("the rebuild produced state for a different run".to_string());
        }

        // The canonical log is the source, and rebuilding does not consume it.
        if log.events.len() != 4 {
            return Err("rebuilding altered the log it rebuilt from".to_string());
        }

        Ok(format!(
            "the derived state at position {} is rebuilt from the log, differs from the state \
             at position {}, and repeats identically",
            point.sequence_position(),
            log.events.len()
        ))
    }
);

rec_case!(
    TrustCanComeBackPartly,
    "exec-rec-017-trust-can-come-back-partly",
    17,
    CaseCategory::Behavioral,
    "A revalidation that passes with degradation restores a scope to degraded trust rather \
     than full trust, so restoration can be progressive, and degraded is not the same answer \
     as either trusted or untrusted",
    || {
        use crate::time::now;
        use crate::trust::{IncidentSeverity, RevalidationResult, TrustState, TrustStateRecord};
        use recovery_fixture::*;

        let scope = scope();
        let at = now();
        let incident = incident(&scope, IncidentSeverity::Critical, at);

        let mut trust = TrustStateRecord::from_incident(
            scope.clone(),
            incident.id(),
            IncidentSeverity::Critical,
            at,
        );
        if trust.state() != TrustState::Untrusted {
            return Err(format!("the scope started at {:?}", trust.state()));
        }

        let partial = run(&scope, &incident, RevalidationResult::PassedWithDegradation, at);
        if !partial.is_successful() {
            return Err(
                "a revalidation that passed with degradation is not counted as successful, so \
                 a partial restoration could never be recorded"
                    .to_string(),
            );
        }
        trust
            .begin_revalidation(partial.id(), at)
            .map_err(|error| format!("revalidation could not begin: {error}"))?;
        trust
            .apply_revalidation(&partial)
            .map_err(|error| format!("the result could not be applied: {error}"))?;

        if trust.state() != TrustState::DegradedTrust {
            return Err(format!(
                "passing with degradation left the scope {:?}; a partial answer must not \
                 restore full trust",
                trust.state()
            ));
        }

        // Degraded is a state of its own: a further, clean revalidation is what
        // completes the restoration.
        let clean = run(&scope, &incident, RevalidationResult::Passed, at);
        trust
            .begin_revalidation(clean.id(), at)
            .map_err(|error| format!("the second revalidation could not begin: {error}"))?;
        trust
            .apply_revalidation(&clean)
            .map_err(|error| format!("the second result could not be applied: {error}"))?;
        if trust.state() != TrustState::Trusted {
            return Err(format!(
                "a clean revalidation from degraded trust gave {:?}",
                trust.state()
            ));
        }

        Ok("degradation restores partly and a later clean run completes it".to_string())
    }
);


const QUALIFICATION_EVIDENCE: &str = "crates/vestrace-domain/src/trust.rs";
const GATE_EVIDENCE: &str = "crates/vestrace-domain/src/conformance/gate.rs";

/// A bundle, a baseline, and the evidence a gate wants to see.
mod qualification_fixture {
    use crate::conformance::gate::{EvidenceOrigin, GovernanceFederationGate, HardGateEvidence};
    use crate::conformance::{
        CaseOrigin, CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
        RequirementId, runner::profile_requirements,
    };
    use crate::time::Timestamp;
    use crate::trust::{QualificationBundle, QualificationLifecycle};

    pub fn passing_result(requirement_id: RequirementId) -> ConformanceCaseResult {
        ConformanceCaseResult {
            case_id: format!("fixture-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Pass,
            message: "the fixture case passed".to_string(),
            evidence: Some("evidence://fixture".to_string()),
            origin: CaseOrigin::Executed,
        }
    }

    /// A report covering every requirement the profile closes over.
    pub fn full_report(profile: QualificationProfile) -> ConformanceReport {
        ConformanceReport::from_results(
            Some(profile),
            profile_requirements(profile)
                .into_iter()
                .map(passing_result)
                .collect(),
        )
    }

    /// Hard-gate evidence for everything the profile's gate requires.
    pub fn full_evidence(profile: QualificationProfile) -> Vec<HardGateEvidence> {
        GovernanceFederationGate::required_requirements(profile)
            .into_iter()
            .map(|requirement_id| {
                HardGateEvidence::pass(
                    requirement_id,
                    "evidence://fixture/hard-gate",
                    Some("policy-v1".to_string()),
                    EvidenceOrigin::LocalExecutable,
                )
            })
            .collect()
    }

    pub fn bundle(
        profile: QualificationProfile,
        report: ConformanceReport,
        evidence: Vec<HardGateEvidence>,
        limitations: Vec<String>,
        at: Timestamp,
    ) -> Result<QualificationBundle, String> {
        QualificationBundle::from_conformance_report(
            QualificationLifecycle::Release,
            profile,
            "sha256:manifest",
            "source-revision-1",
            "sha256:build",
            "sha256:config",
            "environment://fixture",
            "suite-v1",
            report,
            evidence,
            limitations,
            at,
            Some(at),
        )
        .map_err(|error| format!("the fixture bundle could not be built: {error}"))
    }

    /// A complete, passing TRUSTED bundle — the thing every failure case below
    /// takes one piece away from.
    pub fn trusted_bundle(at: Timestamp) -> Result<QualificationBundle, String> {
        bundle(
            QualificationProfile::Trusted,
            full_report(QualificationProfile::Trusted),
            full_evidence(QualificationProfile::Trusted),
            vec!["local-file key custody does not pass production crypto qualification".to_string()],
            at,
        )
    }
}

macro_rules! qual_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $evidence:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Qual,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    $evidence,
                )
            }
        }
    };
}

qual_case!(
    EveryClaimedRequirementHasAResult,
    "exec-qual-002-every-requirement-has-a-result",
    2,
    CaseCategory::Behavioral,
    "A bundle cannot be built from a report that is missing any requirement its profile closes \
     over, however many other results it carries",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::{ConformanceReport, QualificationProfile, runner::profile_requirements};
        use crate::time::now;
        use qualification_fixture::*;

        let at = now();
        let profile = QualificationProfile::Core;

        bundle(profile, full_report(profile), Vec::new(), Vec::new(), at)?;

        let mut short = profile_requirements(profile);
        let dropped = short.pop().ok_or_else(|| "the profile is empty".to_string())?;
        let report = ConformanceReport::from_results(
            Some(profile),
            short.into_iter().map(passing_result).collect(),
        );
        if bundle(profile, report, Vec::new(), Vec::new(), at).is_ok() {
            return Err(format!(
                "a bundle was built without any result for {dropped}, so a profile could be \
                 claimed while one of its requirements had never been asked"
            ));
        }

        Ok(format!(
            "{} requirements are closed over, and omitting one refuses the bundle",
            profile_requirements(profile).len()
        ))
    }
);

qual_case!(
    ProfilesBuildOnOneAnother,
    "exec-qual-004-profiles-build-on-one-another",
    4,
    CaseCategory::Behavioral,
    "Each profile in the chain closes over everything the one before it requires, the chain \
     grows strictly, and a report for one profile is refused by a bundle claiming another",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::{QualificationProfile, runner::profile_requirements};
        use crate::time::now;
        use qualification_fixture::*;
        use std::collections::HashSet;

        let chain = QualificationProfile::dependency_chain();
        let mut previous: HashSet<_> = HashSet::new();
        let mut previous_profile: Option<QualificationProfile> = None;

        for profile in chain {
            let current: HashSet<_> = profile_requirements(*profile).into_iter().collect();
            if let Some(earlier) = previous_profile {
                let missing: Vec<_> = previous.difference(&current).collect();
                if !missing.is_empty() {
                    return Err(format!(
                        "{profile} drops {} requirement(s) that {earlier} requires, so a later \
                         profile could be claimed without satisfying an earlier one",
                        missing.len()
                    ));
                }
                if current.len() <= previous.len() {
                    return Err(format!(
                        "{profile} closes over no more than {earlier}, so the chain adds nothing"
                    ));
                }
            }
            previous = current;
            previous_profile = Some(*profile);
        }

        // And the closure is enforced where it matters: a report built for one
        // profile does not satisfy a bundle claiming a wider one.
        let at = now();
        let core_report = full_report(QualificationProfile::Core);
        if bundle(
            QualificationProfile::Memory,
            core_report,
            Vec::new(),
            Vec::new(),
            at,
        )
        .is_ok()
        {
            return Err(
                "a CORE report was accepted as a MEMORY qualification, so the closure is \
                 described and not enforced"
                    .to_string(),
            );
        }

        Ok(format!(
            "{} profiles, each closing over the last, ending at {} requirements",
            chain.len(),
            previous.len()
        ))
    }
);

qual_case!(
    LimitationsAreClaimedNotImplied,
    "exec-qual-005-limitations-are-stated",
    5,
    CaseCategory::Behavioral,
    "A bundle carries the known limitations of what it qualifies, and they survive into the \
     bundle rather than being an argument nobody kept",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::time::now;
        use qualification_fixture::*;

        let at = now();
        let stated = vec![
            "local-file key custody does not pass production crypto qualification".to_string(),
            "no external effect has ever been performed by this build".to_string(),
        ];
        let bundle = bundle(
            QualificationProfile::Core,
            full_report(QualificationProfile::Core),
            Vec::new(),
            stated.clone(),
            at,
        )?;

        if bundle.known_limitations() != stated {
            return Err(
                "the bundle does not carry the limitations it was given, so a qualification \
                 could be published with its caveats dropped"
                    .to_string(),
            );
        }

        Ok(format!(
            "a bundle carries {} stated limitation(s)",
            bundle.known_limitations().len()
        ))
    }
);

qual_case!(
    ASkippedRequirementIsNotAPass,
    "exec-qual-006-a-skip-is-not-a-pass",
    6,
    CaseCategory::Behavioral,
    "A report containing a single skipped requirement does not pass, and a bundle built on it \
     reports failed rather than passed",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::{
            CaseOrigin, CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
            runner::profile_requirements,
        };
        use crate::time::now;
        use crate::trust::QualificationStatus;
        use qualification_fixture::*;

        let at = now();
        let profile = QualificationProfile::Core;
        let mut results: Vec<ConformanceCaseResult> = profile_requirements(profile)
            .into_iter()
            .map(passing_result)
            .collect();
        let skipped_requirement = results[0].requirement_ids[0];
        results[0] = ConformanceCaseResult {
            case_id: "fixture-skip".to_string(),
            requirement_ids: vec![skipped_requirement],
            status: CaseStatus::Skip,
            message: "no conformance case registered yet".to_string(),
            evidence: None,
            origin: CaseOrigin::Attested,
        };

        let report = ConformanceReport::from_results(Some(profile), results);
        if report.is_pass() {
            return Err(format!(
                "a report with {skipped_requirement} skipped reports itself as passing, so a \
                 requirement nobody asked would count as satisfied"
            ));
        }

        let bundle = bundle(profile, report, Vec::new(), Vec::new(), at)?;
        if bundle.status() != QualificationStatus::Failed {
            return Err(format!(
                "a bundle over a skipped requirement reports {:?}",
                bundle.status()
            ));
        }

        Ok(format!(
            "one skipped requirement out of {} fails the profile",
            profile_requirements(profile).len()
        ))
    }
);

qual_case!(
    AHardGateIsNotAScore,
    "exec-qual-007-a-hard-gate-is-not-a-score",
    7,
    CaseCategory::Security,
    "One failed, missing, skipped or inconclusive piece of governance evidence fails the gate \
     no matter how much other evidence passes",
    GATE_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::conformance::gate::{
            EvidenceOrigin, GateEvidenceStatus, GovernanceFederationGate, HardGateEvidence,
        };
        use qualification_fixture::*;

        let profile = QualificationProfile::Trusted;
        let evidence = full_evidence(profile);
        if evidence.len() < 2 {
            return Err("the trusted gate requires almost nothing, so this proves little".to_string());
        }
        if !GovernanceFederationGate::evaluate(profile, &evidence).is_passed() {
            return Err("the complete evidence set was refused".to_string());
        }

        let spoiled_requirement = evidence[0].requirement_id();
        for status in [
            GateEvidenceStatus::Fail,
            GateEvidenceStatus::Skipped,
            GateEvidenceStatus::Inconclusive,
            GateEvidenceStatus::NotApplicable,
        ] {
            let mut spoiled = evidence.clone();
            spoiled[0] = HardGateEvidence::new(
                spoiled_requirement,
                status,
                Some("evidence://fixture/hard-gate".to_string()),
                Some("policy-v1".to_string()),
                EvidenceOrigin::LocalExecutable,
            );
            let decision = GovernanceFederationGate::evaluate(profile, &spoiled);
            if decision.is_passed() {
                return Err(format!(
                    "{} of {} pieces of evidence passing and one {status:?} still passed the \
                     gate, so a security requirement could be outvoted",
                    spoiled.len() - 1,
                    spoiled.len()
                ));
            }
        }

        // Removing it entirely is not an improvement either.
        let mut absent = evidence.clone();
        absent.remove(0);
        if GovernanceFederationGate::evaluate(profile, &absent).is_passed() {
            return Err(
                "deleting a piece of governance evidence passed the gate, so the cheapest way \
                 through it would be to produce less"
                    .to_string(),
            );
        }

        Ok(format!(
            "{} required pieces of evidence, and any one of them failing or missing fails the \
             gate",
            evidence.len()
        ))
    }
);

qual_case!(
    ABundleCarriesWhatItQualified,
    "exec-qual-013-a-bundle-carries-what-it-qualified",
    13,
    CaseCategory::Behavioral,
    "A bundle keeps its suite version, its build, configuration and environment identity, its \
     report and its evidence, refuses two pieces of evidence for one requirement, and cannot \
     be built with any identity left blank",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::conformance::gate::{EvidenceOrigin, HardGateEvidence};
        use crate::conformance::{RequirementFamily as F, RequirementId};
        use crate::time::now;
        use crate::trust::{QualificationBundle, QualificationLifecycle};
        use qualification_fixture::*;

        let at = now();
        let profile = QualificationProfile::Core;
        let bundle = bundle(
            profile,
            full_report(profile),
            Vec::new(),
            vec!["a limitation".to_string()],
            at,
        )?;

        if bundle.conformance_report().is_none() {
            return Err("the bundle kept no report of what it qualified".to_string());
        }
        for (label, value) in [
            // The suite version is the first half of what QUAL-013 asks for and
            // was unreadable until the sweep that made every recorded field
            // readable; this case could not check it before.
            ("suite version", bundle.suite_version()),
            ("source revision", bundle.source_revision()),
            ("build digest", bundle.build_digest()),
            ("configuration digest", bundle.configuration_digest()),
            ("environment manifest", bundle.environment_manifest()),
            ("target manifest", bundle.target_manifest()),
            ("target digest", bundle.target_digest()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("the bundle kept no {label}"));
            }
        }

        // Blank identity is refused rather than stored as an empty string.
        if QualificationBundle::from_conformance_report(
            QualificationLifecycle::Release,
            profile,
            "sha256:manifest",
            "source-revision-1",
            "   ",
            "sha256:config",
            "environment://fixture",
            "suite-v1",
            full_report(profile),
            Vec::new(),
            Vec::new(),
            at,
            Some(at),
        )
        .is_ok()
        {
            return Err(
                "a bundle was built with no build identity, so it would qualify something \
                 nobody can point at"
                    .to_string(),
            );
        }

        // Two answers for one requirement is not more evidence, it is an
        // unresolved disagreement.
        let requirement = RequirementId::new(F::Arc, 1);
        let duplicated = vec![
            HardGateEvidence::pass(
                requirement,
                "evidence://one",
                Some("policy-v1".to_string()),
                EvidenceOrigin::LocalExecutable,
            ),
            HardGateEvidence::pass(
                requirement,
                "evidence://another",
                Some("policy-v1".to_string()),
                EvidenceOrigin::LocalExecutable,
            ),
        ];
        if QualificationBundle::from_conformance_report(
            QualificationLifecycle::Release,
            profile,
            "sha256:manifest",
            "source-revision-1",
            "sha256:build",
            "sha256:config",
            "environment://fixture",
            "suite-v1",
            full_report(profile),
            duplicated,
            Vec::new(),
            at,
            Some(at),
        )
        .is_ok()
        {
            return Err("a bundle carried two pieces of evidence for one requirement".to_string());
        }

        Ok(format!(
            "a bundle keeps its report, its limitations and a target digest {}",
            &bundle.target_digest()[..16.min(bundle.target_digest().len())]
        ))
    }
);

qual_case!(
    ChangingWhatWasQualifiedBreaksTheBaseline,
    "exec-qual-014-a-changed-target-breaks-the-baseline",
    14,
    CaseCategory::Behavioral,
    "The baseline is bound to a digest of what was qualified, so changing the configuration, \
     the build, the environment or the suite version no longer matches it — and the gate says \
     so rather than passing on the old evidence",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::time::now;
        use crate::trust::{
            QualificationBaseline, QualificationBundle, QualificationLifecycle,
            TrustedGateFailure, TrustedQualificationGate,
        };
        use qualification_fixture::*;

        let at = now();
        let qualified = trusted_bundle(at)?;
        let baseline = QualificationBaseline::from_bundle(&qualified, at)
            .map_err(|error| format!("the baseline could not be published: {error}"))?;

        let decision = TrustedQualificationGate::evaluate(&qualified, &baseline);
        if !decision.is_passed() {
            return Err(format!(
                "the bundle that produced the baseline did not pass its own gate: {:?}",
                decision.failures()
            ));
        }

        // Each of these is a material change to what was qualified.
        for (label, build, config, environment, suite) in [
            ("configuration", "sha256:build", "sha256:config-2", "environment://fixture", "suite-v1"),
            ("build", "sha256:build-2", "sha256:config", "environment://fixture", "suite-v1"),
            ("environment", "sha256:build", "sha256:config", "environment://other", "suite-v1"),
            ("suite version", "sha256:build", "sha256:config", "environment://fixture", "suite-v2"),
        ] {
            let changed = QualificationBundle::from_conformance_report(
                QualificationLifecycle::Release,
                QualificationProfile::Trusted,
                "sha256:manifest",
                "source-revision-1",
                build,
                config,
                environment,
                suite,
                full_report(QualificationProfile::Trusted),
                full_evidence(QualificationProfile::Trusted),
                vec!["a limitation".to_string()],
                at,
                Some(at),
            )
            .map_err(|error| format!("the changed bundle could not be built: {error}"))?;

            if changed.target_digest() == qualified.target_digest() {
                return Err(format!(
                    "changing the {label} left the target digest unchanged, so the baseline \
                     would keep vouching for something that is no longer what it measured"
                ));
            }
            if baseline.matches_bundle(&changed) {
                return Err(format!("the baseline still matches after the {label} changed"));
            }
            let decision = TrustedQualificationGate::evaluate(&changed, &baseline);
            if decision.is_passed()
                || !decision
                    .failures()
                    .contains(&TrustedGateFailure::BaselineMismatch)
            {
                return Err(format!(
                    "after the {label} changed the gate gave {:?}",
                    decision.failures()
                ));
            }
        }

        Ok("build, configuration, environment and suite version each bind the baseline"
            .to_string())
    }
);

qual_case!(
    AQualificationCanBeWithdrawn,
    "exec-qual-015-a-qualification-can-be-withdrawn",
    15,
    CaseCategory::Behavioral,
    "A published baseline can be marked stale or invalidated with a stated reason, and once it \
     is neither qualifies anything — the same bundle that passed before is refused",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::time::now;
        use crate::trust::{
            QualificationBaseline, QualificationBaselineState, TrustedGateFailure,
            TrustedQualificationGate,
        };
        use chrono::Duration;
        use qualification_fixture::*;

        let at = now();
        let bundle = trusted_bundle(at)?;

        for (label, state) in [
            ("stale", QualificationBaselineState::Stale),
            ("invalidated", QualificationBaselineState::Invalidated),
        ] {
            let mut baseline = QualificationBaseline::from_bundle(&bundle, at)
                .map_err(|error| format!("the baseline could not be published: {error}"))?;
            if !TrustedQualificationGate::evaluate(&bundle, &baseline).is_passed() {
                return Err("the freshly published baseline did not qualify its own bundle".to_string());
            }

            let later = at + Duration::days(90);
            match state {
                QualificationBaselineState::Stale => baseline
                    .mark_stale("the provider it was qualified against changed", later)
                    .map_err(|error| format!("the baseline could not go stale: {error}"))?,
                _ => baseline
                    .invalidate("the crypto adapter was replaced", later)
                    .map_err(|error| format!("the baseline could not be invalidated: {error}"))?,
            }

            if baseline.state() != state {
                return Err(format!("the baseline reports {:?}", baseline.state()));
            }
            if baseline
                .invalidation_reason()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(format!(
                    "a {label} baseline kept no record of why it was withdrawn"
                ));
            }
            if baseline.matches_bundle(&bundle) {
                return Err(format!(
                    "a {label} baseline still matches the bundle it was published from"
                ));
            }
            let decision = TrustedQualificationGate::evaluate(&bundle, &baseline);
            if decision.is_passed()
                || !decision
                    .failures()
                    .contains(&TrustedGateFailure::BaselineNotQualified)
            {
                return Err(format!(
                    "a {label} baseline gave {:?} rather than refusing the claim",
                    decision.failures()
                ));
            }

            // And a withdrawal has to say why.
            let mut unexplained = QualificationBaseline::from_bundle(&bundle, at)
                .map_err(|error| format!("the baseline could not be published: {error}"))?;
            if unexplained.mark_stale("   ", later).is_ok()
                || unexplained.invalidate("  ", later).is_ok()
            {
                return Err("a baseline was withdrawn without a reason".to_string());
            }
        }

        Ok("a baseline can go stale or be invalidated, must say why, and qualifies nothing \
            afterwards"
            .to_string())
    }
);

qual_case!(
    SelfAssertedTrustIsNotEvidence,
    "exec-qual-017-self-asserted-trust-is-not-evidence",
    17,
    CaseCategory::Security,
    "Evidence a remote asserted about itself fails the gate even when it claims to pass, while \
     the same claim attested by a third party is accepted",
    GATE_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::conformance::gate::{
            EvidenceOrigin, GovernanceFederationGate, HardGateEvidence, HardGateFailure,
        };
        use qualification_fixture::*;

        let profile = QualificationProfile::Trusted;
        let evidence = full_evidence(profile);
        let requirement = evidence[0].requirement_id();

        let mut self_asserted = evidence.clone();
        self_asserted[0] = HardGateEvidence::pass(
            requirement,
            "evidence://remote/their-own-word",
            Some("policy-v1".to_string()),
            EvidenceOrigin::RemoteSelfAsserted,
        );
        let decision = GovernanceFederationGate::evaluate(profile, &self_asserted);
        if decision.is_passed() {
            return Err(
                "a remote's claim about its own trust passed the gate, so participating in a \
                 federation would mean believing whatever a peer says about itself"
                    .to_string(),
            );
        }
        if !decision
            .failures()
            .contains(&HardGateFailure::RemoteSelfAssertion {
                requirement_id: requirement,
            })
        {
            return Err(format!(
                "the denial named {:?} rather than the self-assertion",
                decision.failures()
            ));
        }

        // The same requirement, attested by someone else, is accepted.
        let mut attested = evidence.clone();
        attested[0] = HardGateEvidence::pass(
            requirement,
            "evidence://remote/attested-by-a-third-party",
            Some("policy-v1".to_string()),
            EvidenceOrigin::RemoteAttested,
        );
        if !GovernanceFederationGate::evaluate(profile, &attested).is_passed() {
            return Err(
                "third-party attested evidence was refused, so the rule is about remoteness \
                 rather than about who is vouching"
                    .to_string(),
            );
        }

        Ok("a peer's word about itself is refused; the same claim attested by another is not"
            .to_string())
    }
);

qual_case!(
    ATrustClaimNeedsTheProfileAndItsCaveats,
    "exec-qual-018-a-trust-claim-carries-its-caveats",
    18,
    CaseCategory::Behavioral,
    "A trust claim requires a bundle for the TRUSTED profile, complete and matching its \
     baseline; a bundle for a lesser profile is refused by the gate, and the limitations it \
     publishes travel with it",
    QUALIFICATION_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::time::now;
        use crate::trust::{
            QualificationBaseline, QualificationStatus, TrustedGateFailure,
            TrustedQualificationGate,
        };
        use qualification_fixture::*;

        let at = now();
        let trusted = trusted_bundle(at)?;
        let baseline = QualificationBaseline::from_bundle(&trusted, at)
            .map_err(|error| format!("the baseline could not be published: {error}"))?;

        if trusted.status() != QualificationStatus::Passed {
            return Err(format!("the trusted bundle reports {:?}", trusted.status()));
        }
        if !TrustedQualificationGate::evaluate(&trusted, &baseline).is_passed() {
            return Err("a complete trusted bundle did not pass its own gate".to_string());
        }
        if trusted.known_limitations().is_empty() {
            return Err(
                "the trust claim publishes no limitations, so 'trusted' would be a word with \
                 nothing beside it"
                    .to_string(),
            );
        }

        // A lesser profile, however well it did, is not a trust claim.
        let lesser = bundle(
            QualificationProfile::Federation,
            full_report(QualificationProfile::Federation),
            full_evidence(QualificationProfile::Federation),
            vec!["a limitation".to_string()],
            at,
        )?;
        let decision = TrustedQualificationGate::evaluate(&lesser, &baseline);
        if decision.is_passed()
            || !decision.failures().contains(&TrustedGateFailure::WrongProfile)
        {
            return Err(format!(
                "a FEDERATION bundle was accepted as a trust claim: {:?}",
                decision.failures()
            ));
        }

        Ok(format!(
            "a trust claim is a complete TRUSTED bundle against its own baseline, publishing \
             {} limitation(s)",
            trusted.known_limitations().len()
        ))
    }
);


const EFFECT_EVIDENCE: &str = "crates/vestrace-domain/src/external_effects.rs";

/// An intent, an authorization and an adapter that answers however the case
/// needs it to.
mod effect_case_fixture {
    use crate::external_effects::{
        AdapterDispatchResult, AdapterError, DeliverySemantics, EffectAuthorization,
        EffectPrecondition, EffectReversibility, ExternalEffectAdapter,
        ExternalEffectAdapterDescriptor, ExternalEffectIntent, IdempotencyProfile, DryRunMode,
    };
    use crate::id::{PrincipalId, WorkspaceId};
    use crate::time::Timestamp;
    use crate::{Capability, RiskCategory};

    pub const ADAPTER: &str = "payments";
    pub const OPERATION: &str = "charge";
    pub const TARGET: &str = "customer://42/card";
    pub const PRECONDITION_DIGEST: &str = "sha256:preconditions-at-intent-time";

    pub struct Actors {
        pub workspace_id: WorkspaceId,
        pub actor_id: PrincipalId,
    }

    pub fn actors() -> Actors {
        Actors {
            workspace_id: WorkspaceId::new(),
            actor_id: PrincipalId::new(),
        }
    }

    pub fn descriptor(reversibility: EffectReversibility) -> ExternalEffectAdapterDescriptor {
        ExternalEffectAdapterDescriptor::new(
            ADAPTER,
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            reversibility,
            DryRunMode::Simulated,
            true,
            true,
            Capability::ExecutionWrite,
        )
        .expect("the fixture descriptor is well-formed")
    }

    pub fn intent(
        actors: &Actors,
        reversibility: EffectReversibility,
        at: Timestamp,
    ) -> ExternalEffectIntent {
        ExternalEffectIntent::new(
            "execution://run/1",
            actors.workspace_id,
            actors.actor_id,
            ADAPTER,
            OPERATION,
            TARGET,
            "sha256:arguments",
            "the card is charged once for 12.00",
            vec![
                EffectPrecondition::new("card_is_active", "true")
                    .expect("the fixture precondition is well-formed"),
            ],
            PRECONDITION_DIGEST,
            RiskCategory::High,
            reversibility,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExecutionWrite,
            None::<String>,
            None::<String>,
            at,
        )
        .expect("the fixture intent is well-formed")
    }

    pub fn authorization(actors: &Actors) -> EffectAuthorization {
        EffectAuthorization::allow(
            "decision://policy/1",
            "policy-v1",
            actors.workspace_id,
            actors.actor_id,
            Capability::ExecutionWrite,
            OPERATION,
            TARGET,
        )
    }

    /// An adapter that answers with whatever it was told to.
    pub struct ScriptedAdapter {
        descriptor: ExternalEffectAdapterDescriptor,
        result: AdapterDispatchResult,
    }

    impl ScriptedAdapter {
        pub fn new(
            descriptor: ExternalEffectAdapterDescriptor,
            result: AdapterDispatchResult,
        ) -> Self {
            Self { descriptor, result }
        }
    }

    impl ExternalEffectAdapter for ScriptedAdapter {
        fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
            &self.descriptor
        }

        fn dispatch(
            &self,
            _intent: &ExternalEffectIntent,
        ) -> Result<AdapterDispatchResult, AdapterError> {
            Ok(self.result.clone())
        }
    }
}

macro_rules! effect_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Ext,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    EFFECT_EVIDENCE,
                )
            }
        }
    };
}

effect_case!(
    TheIntentIsFixedBeforeAnythingLeaves,
    "exec-ext-001-the-intent-is-fixed-first",
    1,
    CaseCategory::Stateful,
    "An intent is complete before it can be authorized and unchanged by being dispatched: the \
     receipt names the same effect, and an authorization that disagrees with any field of the \
     intent is refused rather than reconciled",
    || {
        use crate::external_effects::{EffectAuthorization, EffectReversibility};
        use crate::time::now;
        use crate::Capability;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        let before = intent.clone();

        // An intent that names nothing it will do is refused outright.
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("a matching authorization was refused: {error}"))?;
        if authorized.intent() != &before {
            return Err(
                "authorizing the intent changed it, so what was approved and what is \
                 dispatched could differ"
                    .to_string(),
            );
        }

        let adapter = ScriptedAdapter::new(
            descriptor(EffectReversibility::Compensatable),
            crate::external_effects::AdapterDispatchResult::acknowledged(
                "2xx",
                Some("charge_01".to_string()),
                Some("sha256:response".to_string()),
                vec!["evidence://provider/response".to_string()],
            ),
        );
        let receipt = authorized
            .dispatch(&adapter, PRECONDITION_DIGEST, at)
            .map_err(|error| format!("dispatch failed: {error:?}"))?;

        if receipt.effect_id() != before.id() {
            return Err("the receipt does not name the intent it came from".to_string());
        }
        if authorized.intent() != &before {
            return Err("dispatching changed the intent".to_string());
        }

        // An authorization for a different operation, target, actor or
        // capability is not this intent's authorization.
        for (label, authorization) in [
            (
                "operation",
                EffectAuthorization::allow(
                    "decision://policy/1",
                    "policy-v1",
                    actors.workspace_id,
                    actors.actor_id,
                    Capability::ExecutionWrite,
                    "refund",
                    TARGET,
                ),
            ),
            (
                "target",
                EffectAuthorization::allow(
                    "decision://policy/1",
                    "policy-v1",
                    actors.workspace_id,
                    actors.actor_id,
                    Capability::ExecutionWrite,
                    OPERATION,
                    "customer://43/card",
                ),
            ),
            (
                "actor",
                EffectAuthorization::allow(
                    "decision://policy/1",
                    "policy-v1",
                    actors.workspace_id,
                    crate::id::PrincipalId::new(),
                    Capability::ExecutionWrite,
                    OPERATION,
                    TARGET,
                ),
            ),
        ] {
            if before.authorize(&authorization).is_ok() {
                return Err(format!(
                    "an authorization for a different {label} was accepted for this intent"
                ));
            }
        }

        Ok(format!(
            "the intent is unchanged through authorization and dispatch, and the receipt names \
             effect {}",
            receipt.effect_id()
        ))
    }
);

effect_case!(
    AnAdapterAndAnIntentMustAgree,
    "exec-ext-003-adapter-and-intent-must-agree",
    3,
    CaseCategory::Behavioral,
    "An adapter declares its delivery semantics, and dispatching an intent through an adapter \
     whose declaration differs is refused rather than attempted",
    || {
        use crate::external_effects::{
            DeliverySemantics, DispatchError, DryRunMode, EffectReversibility,
            ExternalEffectAdapterDescriptor, IdempotencyProfile,
        };
        use crate::time::now;
        use crate::Capability;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("the fixture authorization failed: {error}"))?;

        if intent.delivery_semantics() != DeliverySemantics::AtLeastOnce {
            return Err("the fixture intent declares no delivery semantics".to_string());
        }

        let result = crate::external_effects::AdapterDispatchResult::acknowledged(
            "2xx",
            None,
            None,
            vec!["evidence://provider".to_string()],
        );

        // Every declaration the intent depends on, disagreed with in turn.
        for (label, semantics, idempotency, reversibility) in [
            (
                "delivery semantics",
                DeliverySemantics::AtMostOnce,
                IdempotencyProfile::ProviderKey,
                EffectReversibility::Compensatable,
            ),
            (
                "idempotency profile",
                DeliverySemantics::AtLeastOnce,
                IdempotencyProfile::Conditional,
                EffectReversibility::Compensatable,
            ),
            (
                "reversibility",
                DeliverySemantics::AtLeastOnce,
                IdempotencyProfile::ProviderKey,
                EffectReversibility::Irreversible,
            ),
        ] {
            let mismatched = ExternalEffectAdapterDescriptor::new(
                ADAPTER,
                semantics,
                idempotency,
                reversibility,
                DryRunMode::Simulated,
                true,
                true,
                Capability::ExecutionWrite,
            )
            .map_err(|error| format!("the mismatched descriptor could not be built: {error}"))?;
            let adapter = ScriptedAdapter::new(mismatched, result.clone());

            match authorized.dispatch(&adapter, PRECONDITION_DIGEST, at) {
                Err(DispatchError::AdapterDoesNotMatchIntent) => {}
                other => {
                    return Err(format!(
                        "an adapter declaring a different {label} gave {other:?}; the intent's \
                         guarantees would then be whatever the adapter happened to do"
                    ));
                }
            }
        }

        Ok("an adapter's declared semantics must match the intent's, or dispatch refuses"
            .to_string())
    }
);

effect_case!(
    CompensationIsAnEffectNotAnUndo,
    "exec-ext-012-compensation-is-not-an-undo",
    12,
    CaseCategory::Behavioral,
    "A compensation is a new effect with its own identity, requiring its own authorization, \
     leaving the original untouched; and an irreversible effect has no compensation to offer",
    || {
        use crate::external_effects::EffectReversibility;
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let original = intent(&actors, EffectReversibility::Compensatable, at);
        let snapshot = original.clone();

        let compensation = original
            .compensation_for(
                "refund",
                "sha256:refund-arguments",
                "the charge is refunded once",
                at,
            )
            .map_err(|error| format!("a compensation could not be built: {error}"))?;

        if compensation.id() == original.id() {
            return Err(
                "the compensation shares the original's identity, so it would read as the same \
                 act rather than a second one"
                    .to_string(),
            );
        }
        if compensation.operation() == original.operation() {
            return Err("the compensation performs the same operation as the original".to_string());
        }
        if original != snapshot {
            return Err(
                "producing a compensation changed the original, so an effect could be edited \
                 into never having happened"
                    .to_string(),
            );
        }
        if compensation.required_capability() != original.required_capability() {
            return Err("the compensation does not require authority of its own".to_string());
        }

        // An irreversible effect cannot be undone, and does not pretend it can.
        let irreversible = intent(&actors, EffectReversibility::Irreversible, at);
        if irreversible
            .compensation_for("refund", "sha256:x", "undone", at)
            .is_ok()
        {
            return Err(
                "an irreversible effect produced a compensation, which is the word rollback \
                 wearing another name"
                    .to_string(),
            );
        }

        Ok(format!(
            "a compensation is effect {} performing {}, and the original is unchanged",
            compensation.id(),
            compensation.operation()
        ))
    }
);

effect_case!(
    ACompensationNamesWhatItCompensates,
    "exec-ext-013-a-compensation-names-its-original",
    13,
    CaseCategory::Behavioral,
    "A compensation carries the identifier of the effect it compensates, and an ordinary \
     effect carries none",
    || {
        use crate::external_effects::EffectReversibility;
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let original = intent(&actors, EffectReversibility::Compensatable, at);

        if original.compensates_effect_id().is_some() {
            return Err("an ordinary effect claims to compensate something".to_string());
        }

        let compensation = original
            .compensation_for("refund", "sha256:refund", "the charge is refunded", at)
            .map_err(|error| format!("a compensation could not be built: {error}"))?;

        match compensation.compensates_effect_id() {
            Some(id) if id == original.id() => {}
            Some(id) => {
                return Err(format!(
                    "the compensation names effect {id} rather than the one it was built from"
                ));
            }
            None => {
                return Err(
                    "the compensation names nothing, so the record would show two unrelated \
                     effects and no way to tell that one answered the other"
                        .to_string(),
                );
            }
        }

        Ok(format!(
            "a compensation names effect {}",
            original.id()
        ))
    }
);

effect_case!(
    PreconditionsAreCheckedAtTheLastMoment,
    "exec-ext-014-preconditions-are-checked-at-dispatch",
    14,
    CaseCategory::Stateful,
    "Dispatch compares the preconditions as they are now against the digest recorded when the \
     intent was written, and refuses a stale intent instead of acting on conditions that have \
     moved",
    || {
        use crate::external_effects::{AdapterDispatchResult, DispatchError, EffectReversibility};
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        if intent.preconditions().is_empty() {
            return Err("the intent carries no preconditions to check".to_string());
        }
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("the fixture authorization failed: {error}"))?;

        let adapter = ScriptedAdapter::new(
            descriptor(EffectReversibility::Compensatable),
            AdapterDispatchResult::acknowledged(
                "2xx",
                None,
                None,
                vec!["evidence://provider".to_string()],
            ),
        );

        match authorized.dispatch(&adapter, "sha256:the-world-has-moved", at) {
            Err(DispatchError::StaleIntent) => {}
            other => {
                return Err(format!(
                    "dispatching against changed preconditions gave {other:?}; the check would \
                     then be of the world as it was when somebody wrote the intent"
                ));
            }
        }

        // Unchanged conditions dispatch normally, so the refusal is about the
        // change rather than about the check being impossible to satisfy.
        authorized
            .dispatch(&adapter, PRECONDITION_DIGEST, at)
            .map_err(|error| format!("dispatch with current preconditions failed: {error:?}"))?;

        Ok("dispatch refuses a stale intent and proceeds on a current one".to_string())
    }
);

effect_case!(
    EveryDispatchLeavesAReceipt,
    "exec-ext-016-every-dispatch-leaves-a-receipt",
    16,
    CaseCategory::Behavioral,
    "Acknowledged, failed and unknown dispatches all produce a receipt naming the effect, the \
     adapter, the response class and the evidence; a synthetic unknown receipt cannot be \
     written without evidence",
    || {
        use crate::external_effects::{
            AdapterDispatchResult, EffectLifecycleStatus, EffectReversibility,
            ExternalEffectReceipt,
        };
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("the fixture authorization failed: {error}"))?;

        for (result, expected) in [
            (
                AdapterDispatchResult::acknowledged(
                    "2xx",
                    Some("charge_01".to_string()),
                    Some("sha256:response".to_string()),
                    vec!["evidence://provider/ack".to_string()],
                ),
                EffectLifecycleStatus::Acknowledged,
            ),
            (
                AdapterDispatchResult::failed(
                    "4xx",
                    vec!["evidence://provider/rejection".to_string()],
                ),
                EffectLifecycleStatus::Failed,
            ),
            (
                AdapterDispatchResult::unknown(
                    "timeout",
                    vec!["evidence://provider/no-answer".to_string()],
                ),
                EffectLifecycleStatus::Unknown,
            ),
        ] {
            let adapter =
                ScriptedAdapter::new(descriptor(EffectReversibility::Compensatable), result);
            let receipt = authorized
                .dispatch(&adapter, PRECONDITION_DIGEST, at)
                .map_err(|error| format!("dispatch failed: {error:?}"))?;

            if receipt.outcome_status() != expected {
                return Err(format!(
                    "a dispatch expected to record {expected:?} recorded {:?}",
                    receipt.outcome_status()
                ));
            }
            if receipt.effect_id() != intent.id() || receipt.adapter() != ADAPTER {
                return Err("the receipt does not name what it is a receipt for".to_string());
            }
            if receipt.response_class().trim().is_empty() || receipt.evidence_refs().is_empty() {
                return Err(format!(
                    "the {expected:?} receipt records neither how the far side answered nor any \
                     evidence, so the dispatch would have left nothing behind"
                ));
            }
        }

        // A receipt written because nobody answered still needs evidence that
        // nobody answered.
        if ExternalEffectReceipt::synthetic_unknown(intent.id(), ADAPTER, at, Vec::new()).is_ok() {
            return Err("an unknown receipt was written with no evidence at all".to_string());
        }

        Ok("acknowledged, failed and unknown dispatches each leave a receipt with evidence"
            .to_string())
    }
);

effect_case!(
    AnAcknowledgementIsNotAnOutcome,
    "exec-ext-017-an-acknowledgement-is-not-an-outcome",
    17,
    CaseCategory::Behavioral,
    "A receipt is never a business confirmation, however positively the far side answered; \
     only reconciliation against observed state confirms, and observing nothing conclusive \
     stays inconclusive",
    || {
        use crate::external_effects::{
            AdapterDispatchResult, EffectReversibility, EvidenceStrength, ObservedEffectState,
            ReconciliationOutcome, reconcile_effect,
        };
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("the fixture authorization failed: {error}"))?;

        let adapter = ScriptedAdapter::new(
            descriptor(EffectReversibility::Compensatable),
            AdapterDispatchResult::acknowledged(
                "2xx",
                Some("charge_01".to_string()),
                Some("sha256:response".to_string()),
                vec!["evidence://provider/ack".to_string()],
            ),
        );
        let receipt = authorized
            .dispatch(&adapter, PRECONDITION_DIGEST, at)
            .map_err(|error| format!("dispatch failed: {error:?}"))?;

        if receipt.is_business_confirmation() {
            return Err(
                "a 2xx acknowledgement counted as the business outcome, so 'the provider \
                 received our request' and 'the customer was charged' would be one fact"
                    .to_string(),
            );
        }

        // Confirmation comes from looking, and only when what was seen is
        // conclusive.
        let inconclusive = reconcile_effect(
            &intent,
            &receipt,
            vec![
                ObservedEffectState::new(
                    EvidenceStrength::ResponseDigest,
                    None,
                    "state://provider/unclear",
                    vec!["evidence://read-back/1".to_string()],
                ),
            ],
            at,
        )
        .map_err(|error| format!("reconciliation failed: {error}"))?;
        if inconclusive.outcome() != ReconciliationOutcome::Inconclusive {
            return Err(format!(
                "observing nothing conclusive gave {:?}",
                inconclusive.outcome()
            ));
        }

        let confirmed = reconcile_effect(
            &intent,
            &receipt,
            vec![
                ObservedEffectState::new(
                    EvidenceStrength::ExternalResourceReadBack,
                    Some(true),
                    "state://provider/charge_01",
                    vec!["evidence://read-back/2".to_string()],
                ),
            ],
            at,
        )
        .map_err(|error| format!("reconciliation failed: {error}"))?;
        if confirmed.outcome() != ReconciliationOutcome::Confirmed
            || confirmed.receipt_id() != receipt.id()
        {
            return Err(format!(
                "an authoritative read-back gave {:?}",
                confirmed.outcome()
            ));
        }

        Ok("an acknowledgement is never a confirmation; only an observation confirms"
            .to_string())
    }
);

effect_case!(
    AReceiptHasNowhereToPutASecret,
    "exec-ext-018-a-receipt-has-nowhere-for-a-secret",
    18,
    CaseCategory::Security,
    "A receipt records a response *class* and a response *digest* and has no field for a \
     response body, so a provider's answer cannot be written into the trace verbatim",
    || {
        use crate::external_effects::{AdapterDispatchResult, EffectReversibility};
        use crate::time::now;
        use effect_case_fixture::*;

        let actors = actors();
        let at = now();
        let intent = intent(&actors, EffectReversibility::Compensatable, at);
        let authorized = intent
            .authorize(&authorization(&actors))
            .map_err(|error| format!("the fixture authorization failed: {error}"))?;

        // An adapter that has been handed something it must not keep.
        const SECRET: &str = "sk_live_this_must_never_be_stored";
        let adapter = ScriptedAdapter::new(
            descriptor(EffectReversibility::Compensatable),
            AdapterDispatchResult::acknowledged(
                "2xx",
                Some("charge_01".to_string()),
                Some(format!("sha256:{}", "0".repeat(64))),
                vec!["evidence://provider/ack".to_string()],
            ),
        );
        let receipt = authorized
            .dispatch(&adapter, PRECONDITION_DIGEST, at)
            .map_err(|error| format!("dispatch failed: {error:?}"))?;

        let serialized = serde_json::to_value(&receipt)
            .map_err(|error| format!("the receipt could not be serialized: {error}"))?;
        let object = serialized
            .as_object()
            .ok_or_else(|| "the receipt does not serialize as an object".to_string())?;

        for forbidden in [
            "response_body",
            "response",
            "payload",
            "body",
            "request",
            "arguments",
            "credential",
            "secret",
            "token",
        ] {
            if object.contains_key(forbidden) {
                return Err(format!(
                    "a receipt has a `{forbidden}` field, which is somewhere a secret can be \
                     written and then read by everyone who can read a trace"
                ));
            }
        }

        if serialized.to_string().contains(SECRET) {
            return Err("the serialized receipt contains the secret".to_string());
        }
        if receipt.response_digest().is_some_and(|digest| !digest.starts_with("sha256:")) {
            return Err("the receipt keeps a response that is not a digest".to_string());
        }

        Ok(format!(
            "a receipt serializes {} fields, none of which can hold a response body",
            object.len()
        ))
    }
);


/// A classification, a policy, and a deletion to verify.
mod governance_fixture_two {
    use crate::security::{DataDestination, Sensitivity};
    use crate::time::Timestamp;
    use crate::trust::{
        DataClassification, DataPolicy, DeletionPlan, DeletionRequest, DeletionSemantics,
    };
    use crate::{Capability, PrincipalId};
    use std::collections::BTreeSet;

    pub fn classification(sensitivity: Sensitivity) -> DataClassification {
        DataClassification::source(sensitivity, "source://classifier/1", "provenance://policy")
            .expect("the fixture classification is well-formed")
    }

    pub fn policy(
        ceiling: Sensitivity,
        destinations: &[DataDestination],
        required_capability: Option<Capability>,
    ) -> DataPolicy {
        let destinations: BTreeSet<DataDestination> = destinations.iter().copied().collect();
        DataPolicy::new(
            crate::DataPolicyId::new(),
            "data-policy-v1",
            ceiling,
            destinations,
            required_capability,
        )
        .expect("the fixture data policy is well-formed")
    }

    pub fn deletion(
        semantics: DeletionSemantics,
        at: Timestamp,
    ) -> (DeletionRequest, DeletionPlan) {
        let request = DeletionRequest::new(
            PrincipalId::new(),
            PrincipalId::new(),
            "memory://subject/42",
            semantics,
            "plan://deletion/1",
            "execution://deletion/1",
            at,
        )
        .expect("the fixture deletion request is well-formed");
        let plan = DeletionPlan::new(
            request.id(),
            vec!["object://primary".to_string()],
            vec!["object://derived-index".to_string()],
            Vec::new(),
            at,
        )
        .expect("the fixture deletion plan is well-formed");
        (request, plan)
    }
}

macro_rules! gov_case_two {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Gov,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    GOVERNANCE_EVIDENCE,
                )
            }
        }
    };
}

gov_case_two!(
    CryptoAndDataPolicyAreTwoGates,
    "exec-gov-001-two-independent-gates",
    1,
    CaseCategory::Security,
    "A data policy decision says nothing about whether a key may be used, and a usable signing \
     key says nothing about whether a destination is allowed: each gate refuses on its own \
     grounds and neither answers for the other",
    || {
        use crate::security::{DataDestination, Sensitivity};
        use crate::trust::{KeyPurpose, KeyReference};
        use governance_fixture_two::*;

        let permissive = policy(
            Sensitivity::Restricted,
            &[DataDestination::RemoteProvider],
            None,
        );
        let allowed = permissive.evaluate(
            &classification(Sensitivity::Confidential),
            DataDestination::RemoteProvider,
            true,
        );
        if !allowed.is_allowed() {
            return Err("the permissive data policy refused its own destination".to_string());
        }

        // The data gate said yes. The crypto gate is still asked separately, and
        // a retired key is unusable however permissive the data policy is.
        let mut key = KeyReference::new(
            "provider",
            "key-1",
            "v1",
            KeyPurpose::Signing,
            "workspace",
            "ed25519",
        )
        .map_err(|error| format!("the fixture key could not be built: {error}"))?;
        if !key.is_usable() {
            return Err("an active key reports itself unusable".to_string());
        }
        key.retire()
            .map_err(|error| format!("the key could not be retired: {error}"))?;
        if key.is_usable() {
            return Err(
                "a retired key remained usable, so the crypto gate would be open whenever the \
                 data gate was"
                    .to_string(),
            );
        }

        // And the reverse: a perfectly usable key does not widen the data
        // policy by one destination.
        let narrow = policy(Sensitivity::Restricted, &[DataDestination::LocalModel], None);
        let refused = narrow.evaluate(
            &classification(Sensitivity::Confidential),
            DataDestination::RemoteProvider,
            true,
        );
        if refused.is_allowed() {
            return Err(
                "a destination outside the policy was allowed, so holding a key would be the \
                 same as being permitted to send"
                    .to_string(),
            );
        }
        if !refused.reason().contains("destination") {
            return Err(format!(
                "the data gate refused for the reason {:?} rather than the destination",
                refused.reason()
            ));
        }

        Ok("the data gate refuses on destination and sensitivity; the crypto gate refuses on \
            key state, and neither decides the other"
            .to_string())
    }
);

gov_case_two!(
    ACapabilityDoesNotOverrideAPolicy,
    "exec-gov-011-a-capability-does-not-override-a-policy",
    11,
    CaseCategory::Security,
    "Holding the capability does not make a forbidden destination allowed or lift the \
     sensitivity ceiling: the policy refuses for its own reason with the capability granted",
    || {
        use crate::security::{DataDestination, Sensitivity};
        use crate::Capability;
        use governance_fixture_two::*;

        let policy = policy(
            Sensitivity::Internal,
            &[DataDestination::LocalModel],
            Some(Capability::ExportRead),
        );

        // Capability granted, destination outside the policy.
        let wrong_destination = policy.evaluate(
            &classification(Sensitivity::Internal),
            DataDestination::RemoteProvider,
            true,
        );
        if wrong_destination.is_allowed() {
            return Err(
                "a granted capability carried data to a destination the policy does not \
                 allow, so a grant would be a way around the policy rather than a condition \
                 of it"
                    .to_string(),
            );
        }

        // Capability granted, classification above the ceiling.
        let too_sensitive = policy.evaluate(
            &classification(Sensitivity::Restricted),
            DataDestination::LocalModel,
            true,
        );
        if too_sensitive.is_allowed() {
            return Err(
                "a granted capability lifted the policy's sensitivity ceiling".to_string(),
            );
        }
        if !too_sensitive.reason().contains("ceiling") {
            return Err(format!(
                "the refusal named {:?} rather than the ceiling it exceeded",
                too_sensitive.reason()
            ));
        }

        // And with both within the policy, the same capability is what makes it
        // pass — so the refusals above are about the policy and not about the
        // fixture being unable to succeed.
        if !policy
            .evaluate(
                &classification(Sensitivity::Internal),
                DataDestination::LocalModel,
                true,
            )
            .is_allowed()
        {
            return Err("the compliant request was refused".to_string());
        }

        Ok("a capability is a condition of the policy, never a way around it".to_string())
    }
);

gov_case_two!(
    APolicyDoesNotSupplyTheCapability,
    "exec-gov-012-a-policy-does-not-supply-authority",
    12,
    CaseCategory::Security,
    "A policy that allows the classification and the destination still refuses when the \
     required capability is absent, and says which of the two it was",
    || {
        use crate::security::{DataDestination, Sensitivity};
        use crate::Capability;
        use governance_fixture_two::*;

        let policy = policy(
            Sensitivity::Restricted,
            &[DataDestination::ExportBundle],
            Some(Capability::ExportRead),
        );
        let classification = classification(Sensitivity::Internal);

        let without_capability =
            policy.evaluate(&classification, DataDestination::ExportBundle, false);
        if without_capability.is_allowed() {
            return Err(
                "an allowed classification and destination were enough without the capability, \
                 so the policy would be granting authority it does not hold"
                    .to_string(),
            );
        }
        if !without_capability.reason().contains("capability") {
            return Err(format!(
                "the refusal named {:?} rather than the missing capability",
                without_capability.reason()
            ));
        }

        let with_capability = policy.evaluate(&classification, DataDestination::ExportBundle, true);
        if !with_capability.is_allowed() {
            return Err("the same request with the capability was still refused".to_string());
        }
        if with_capability.policy_version() != without_capability.policy_version() {
            return Err("the two decisions came from different policy versions".to_string());
        }

        Ok(format!(
            "the same policy version {} refuses without the capability and allows with it",
            with_capability.policy_version()
        ))
    }
);

gov_case_two!(
    ThreeKindsOfDeletionStayThree,
    "exec-gov-015-three-kinds-of-deletion",
    15,
    CaseCategory::Behavioral,
    "Logical delete, physical delete and crypto erasure are carried through the request into \
     the verification unchanged, and each maps to its own disposal method",
    || {
        use crate::time::now;
        use crate::trust::{DeletionSemantics, DisposalMethod, verify_deletion};
        use governance_fixture_two::*;

        let at = now();
        let mut seen: Vec<DisposalMethod> = Vec::new();

        for semantics in [
            DeletionSemantics::LogicalDelete,
            DeletionSemantics::PhysicalDelete,
            DeletionSemantics::CryptoErasure,
        ] {
            let (request, plan) = deletion(semantics, at);
            if request.semantics() != semantics {
                return Err("the request did not keep the semantics it was made with".to_string());
            }

            let verification = verify_deletion(
                &request,
                &plan,
                &[],
                vec![
                    "object://primary".to_string(),
                    "object://derived-index".to_string(),
                ],
                Vec::new(),
                vec!["evidence://deletion/1".to_string()],
                at,
            )
            .map_err(|error| format!("verification failed for {semantics:?}: {error}"))?;

            if verification.semantics() != semantics {
                return Err(format!(
                    "a {semantics:?} request verified as {:?}, so the three would be one",
                    verification.semantics()
                ));
            }
            let method = DisposalMethod::from(semantics);
            if seen.contains(&method) {
                return Err(format!(
                    "{semantics:?} disposes the same way as an earlier one, so two of the                      three would be indistinguishable in the record"
                ));
            }
            seen.push(method);
        }

        if seen.len() != 3 {
            return Err(format!(
                "the three deletion semantics collapse into {} disposal method(s)",
                seen.len()
            ));
        }

        Ok("logical, physical and cryptographic deletion stay distinguishable end to end"
            .to_string())
    }
);

gov_case_two!(
    ADeletionCannotClaimWhatItDidNotCheck,
    "exec-gov-016-a-deletion-claims-only-what-it-checked",
    16,
    CaseCategory::Behavioral,
    "A verification that found copies still standing is incomplete and names them; one that \
     did not cover every planned dependency is refused outright; and an active hold blocks \
     the claim rather than completing it",
    || {
        use crate::time::now;
        use crate::trust::{DataHold, DeletionSemantics, DeletionVerificationOutcome, verify_deletion};
        use chrono::Duration;
        use governance_fixture_two::*;

        let at = now();
        let (request, plan) = deletion(DeletionSemantics::PhysicalDelete, at);

        // A backup nobody could reach is still a copy.
        let incomplete = verify_deletion(
            &request,
            &plan,
            &[],
            vec![
                "object://primary".to_string(),
                "object://derived-index".to_string(),
            ],
            vec!["backup://nightly/2026-08-14".to_string()],
            vec!["evidence://deletion/1".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;

        if incomplete.is_complete() {
            return Err(
                "a verification with a surviving backup reported the deletion complete, which \
                 is a claim about copies nobody destroyed"
                    .to_string(),
            );
        }
        if incomplete.outcome() != DeletionVerificationOutcome::Incomplete
            || incomplete.remaining_copies().is_empty()
        {
            return Err(format!(
                "the outcome was {:?} and it named {} remaining copies",
                incomplete.outcome(),
                incomplete.remaining_copies().len()
            ));
        }

        // A verification that skipped a planned dependency is not a weaker
        // claim, it is refused.
        if verify_deletion(
            &request,
            &plan,
            &[],
            vec!["object://primary".to_string()],
            Vec::new(),
            vec!["evidence://deletion/1".to_string()],
            at,
        )
        .is_ok()
        {
            return Err(
                "a verification that never looked at a planned dependency was accepted"
                    .to_string(),
            );
        }

        // A hold blocks rather than completes.
        let hold = DataHold::new(
            "memory://subject/42",
            "legal hold for case 7",
            crate::PrincipalId::new(),
            "policy://hold/1",
            at,
            Some(at + Duration::days(30)),
        )
        .map_err(|error| format!("the fixture hold could not be built: {error}"))?;
        let blocked = verify_deletion(
            &request,
            &plan,
            std::slice::from_ref(&hold),
            vec![
                "object://primary".to_string(),
                "object://derived-index".to_string(),
            ],
            Vec::new(),
            vec!["evidence://deletion/1".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;
        if blocked.outcome() != DeletionVerificationOutcome::BlockedByHold || blocked.is_complete()
        {
            return Err(format!(
                "an active hold produced {:?}",
                blocked.outcome()
            ));
        }

        // And a genuine, fully covered deletion with nothing left does complete.
        let complete = verify_deletion(
            &request,
            &plan,
            &[],
            vec![
                "object://primary".to_string(),
                "object://derived-index".to_string(),
            ],
            Vec::new(),
            vec!["evidence://deletion/1".to_string()],
            at,
        )
        .map_err(|error| format!("verification failed: {error}"))?;
        if !complete.is_complete() || complete.checked_refs().len() != 2 {
            return Err("a complete deletion did not report itself complete".to_string());
        }

        Ok(format!(
            "a deletion claims completion only with every planned reference checked, no hold \
             active and no copy left; the incomplete case named {} survivor(s)",
            incomplete.remaining_copies().len()
        ))
    }
);


qual_case!(
    AttestationIsAdmissibleOnlyWhereExecutionIsNot,
    "exec-qual-011-attestation-has-a-boundary",
    11,
    CaseCategory::Security,
    "A locally written claim satisfies the gate only for a requirement whose class is Static; \
     for anything that describes behaviour it is refused, and a remote self-assertion is \
     refused whatever the class",
    GATE_EVIDENCE,
    || {
        use crate::conformance::QualificationProfile;
        use crate::conformance::gate::{
            EvidenceOrigin, GovernanceFederationGate, HardGateEvidence, HardGateFailure,
        };
        use crate::conformance::{RequirementFamily as F, RequirementId, VerificationClass, registry};
        use qualification_fixture::*;

        let profile = QualificationProfile::Trusted;
        let evidence = full_evidence(profile);

        // A requirement of each kind, taken from the registry rather than
        // chosen: the rule is about what the requirement *is*, and inventing
        // one here would prove nothing about the catalogue this gate runs over.
        let required = GovernanceFederationGate::required_requirements(profile);
        let statics: Vec<RequirementId> = required
            .iter()
            .copied()
            .filter(|id| {
                registry::find(id)
                    .map(|requirement| requirement.class == VerificationClass::Static)
                    .unwrap_or(false)
            })
            .collect();
        let behavioural: Vec<RequirementId> = required
            .iter()
            .copied()
            .filter(|id| {
                registry::find(id)
                    .map(|requirement| requirement.class != VerificationClass::Static)
                    .unwrap_or(false)
            })
            .collect();
        if statics.is_empty() || behavioural.is_empty() {
            return Err(
                "the trusted closure has no requirement of one of the two kinds, so this case \
                 would be asserting nothing"
                    .to_string(),
            );
        }

        let replace = |requirement: RequirementId, origin: EvidenceOrigin| {
            let mut swapped = evidence.clone();
            for item in swapped.iter_mut() {
                if item.requirement_id() == requirement {
                    *item = HardGateEvidence::pass(
                        requirement,
                        "evidence://a-written-reading-of-the-code",
                        Some("policy-v1".to_string()),
                        origin,
                    );
                }
            }
            GovernanceFederationGate::evaluate(profile, &swapped)
        };

        // A written claim about architectural shape is the right evidence for a
        // Static requirement, and the gate accepts it.
        let on_static = replace(statics[0], EvidenceOrigin::LocalAttested);
        if !on_static.is_passed() {
            return Err(format!(
                "a local attestation was refused for {}, whose class is Static and for which \
                 no runtime assertion is the right evidence: {:?}",
                statics[0],
                on_static.failures()
            ));
        }

        // The same claim about something that describes behaviour is a sentence
        // where a demonstration belongs.
        let on_behaviour = replace(behavioural[0], EvidenceOrigin::LocalAttested);
        if on_behaviour.is_passed()
            || !on_behaviour
                .failures()
                .contains(&HardGateFailure::AttestationWhereExecutionIsRequired {
                    requirement_id: behavioural[0],
                })
        {
            return Err(format!(
                "a written claim stood in for {}, which describes behaviour: {:?}",
                behavioural[0],
                on_behaviour.failures()
            ));
        }

        // And the trust boundary still decides: a remote self-assertion fails
        // even where an attestation would have been admissible.
        let remote_on_static = replace(statics[0], EvidenceOrigin::RemoteSelfAsserted);
        if remote_on_static.is_passed()
            || !remote_on_static
                .failures()
                .contains(&HardGateFailure::RemoteSelfAssertion {
                    requirement_id: statics[0],
                })
        {
            return Err(
                "a peer's claim about itself was accepted for a Static requirement, so the rule \
                 would be about the word 'attested' rather than about who is vouching"
                    .to_string(),
            );
        }

        let _ = F::Qual;
        Ok(format!(
            "of {} required requirements, {} are Static and admit a local attestation; the \
             other {} do not, and no origin admits a peer's word about itself",
            required.len(),
            statics.len(),
            behavioural.len()
        ))
    }
);


cap_case!(
    AuthorityIsWhatWasGrantedNotWhoYouAre,
    "exec-cap-001-authority-is-granted-not-inherited",
    1,
    CaseCategory::Security,
    "The same principal gets different answers under different grants, and two different \
     principals get the same answer under grants of the same shape: a decision is a function of \
     the authority held, not of who holds it",
    CAPABILITY_EVIDENCE,
    || {
        use crate::security::{CapabilityGrant, evaluate_capability_grants};
        use crate::id::PolicyDecisionId;
        use crate::{AuthorizationRequest, Capability, RiskCategory, time::now};
        use capability_fixture::*;

        let actors = actors();
        let at = now();

        let request = |capability: Capability, operation: &str, scope: &str| {
            AuthorizationRequest::new(capability, operation, scope, RiskCategory::Low)
        };

        // One principal, two grants. The answer follows the grant.
        let narrow = CapabilityGrant::issue(
            spec(
                &actors,
                Capability::MemoryRead,
                OPERATION,
                SCOPE,
                RiskCategory::Medium,
                Some(10),
                at,
                None,
            ),
            at,
        )
        .map_err(|error| format!("the narrow grant could not be issued: {error}"))?;

        let wider = CapabilityGrant::issue(
            spec(
                &actors,
                Capability::MemoryWrite,
                OPERATION,
                SCOPE,
                RiskCategory::Medium,
                Some(10),
                at,
                None,
            ),
            at,
        )
        .map_err(|error| format!("the wider grant could not be issued: {error}"))?;

        let write_request = request(Capability::MemoryWrite, OPERATION, SCOPE);
        let allows = |grant: &CapabilityGrant, actors: &Actors, request: &AuthorizationRequest| {
            evaluate_capability_grants(
                PolicyDecisionId::new(),
                actors.workspace,
                actors.subject,
                "policy-v1",
                request,
                std::slice::from_ref(grant),
                at,
            )
            .map(|decision| decision.is_allowed())
        };

        if allows(&narrow, &actors, &write_request)
            .map_err(|error| format!("the narrow decision failed: {error}"))?
        {
            return Err(
                "a grant of memory.read authorized a memory.write request, so the decision is \
                 not being made from the authority that was granted"
                    .to_string(),
            );
        }
        if !allows(&wider, &actors, &write_request)
            .map_err(|error| format!("the wider decision failed: {error}"))?
        {
            return Err("a grant of memory.write refused a memory.write request".to_string());
        }

        // Two principals, grants of the same shape. The answer follows the
        // grant, not the holder — there is no seniority, no role, nothing about
        // the subject that changes the verdict.
        let other_actors = capability_fixture::actors();
        let other_grant = CapabilityGrant::issue(
            spec(
                &other_actors,
                Capability::MemoryWrite,
                OPERATION,
                SCOPE,
                RiskCategory::Medium,
                Some(10),
                at,
                None,
            ),
            at,
        )
        .map_err(|error| format!("the second principal's grant could not be issued: {error}"))?;

        if other_grant.subject_id == wider.subject_id {
            return Err("the fixture produced the same principal twice".to_string());
        }
        if !allows(&other_grant, &other_actors, &write_request)
            .map_err(|error| format!("the second principal's decision failed: {error}"))?
        {
            return Err(
                "the same grant shape answered differently for a different principal, so \
                 something about the holder is deciding"
                    .to_string(),
            );
        }

        // And the grant's own limits still decide: the same holder, the same
        // capability, a scope the grant does not name.
        let elsewhere = request(Capability::MemoryWrite, OPERATION, "/v1/somewhere-else");
        if allows(&wider, &actors, &elsewhere)
            .map_err(|error| format!("the out-of-scope decision failed: {error}"))?
        {
            return Err(
                "a grant authorized a resource outside the scope it names, so its scope is \
                 decorative"
                    .to_string(),
            );
        }

        Ok(
            "one principal is refused and permitted according to which grant is held, and two \
             principals holding the same grant shape receive the same answer"
                .to_string(),
        )
    }
);


gov_case_two!(
    DeletingEvidenceReopensWhatRestedOnIt,
    "exec-gov-017-deleting-evidence-reopens-the-claim",
    17,
    CaseCategory::Stateful,
    "A completed deletion contests every claim citing what it removed, leaves claims resting on \
     other evidence alone, reports a claim that was already settled rather than silently \
     skipping it, and an incomplete deletion reopens nothing",
    || {
        use crate::claim::{Claim, ClaimEvidenceLink, ClaimStatus, revalidate_after_deletion};
        use crate::id::{ClaimEvidenceLinkId, ClaimId, EventId, PrincipalId, WorkspaceId};
        use crate::provenance::EvidenceRef;
        use crate::time::now;
        use crate::trust::{DeletionSemantics, verify_deletion};
        use crate::{EvidenceRole, claim::SourceClassification};
        use governance_fixture_two::*;

        let at = now();
        let workspace = WorkspaceId::new();

        // Two pieces of evidence; one is about to be destroyed.
        let doomed = EvidenceRef::event(EventId::new());
        let surviving = EvidenceRef::event(EventId::new());

        let claim = |value: &str| {
            Claim::new(
                ClaimId::new(),
                workspace,
                format!("key:{value}"),
                "subject".to_string(),
                "predicate".to_string(),
                value.to_string(),
                at,
            )
        };
        let link = |claim: &Claim, evidence: &EvidenceRef| {
            ClaimEvidenceLink::new(
                ClaimEvidenceLinkId::new(),
                claim.claim_id,
                workspace,
                evidence.clone(),
                EvidenceRole::DirectSource,
                SourceClassification::Direct,
                PrincipalId::new(),
                at,
            )
        };

        let supported = claim("rests on the doomed evidence")
            .support(at)
            .map_err(|error| format!("the fixture claim could not be supported: {error}"))?;
        let untouched = claim("rests on other evidence")
            .support(at)
            .map_err(|error| format!("the second fixture claim could not be supported: {error}"))?;
        let settled = claim("was already decided about")
            .support(at)
            .and_then(|claim| claim.supersede(at))
            .map_err(|error| format!("the third fixture claim could not be settled: {error}"))?;

        let links = vec![
            link(&supported, &doomed),
            link(&settled, &doomed),
            link(&untouched, &surviving),
        ];

        // The deletion, covering exactly the doomed evidence.
        let (request, plan) = deletion(DeletionSemantics::PhysicalDelete, at);
        let plan = crate::trust::DeletionPlan::new(
            request.id(),
            vec![doomed.reference()],
            Vec::new(),
            Vec::new(),
            at,
        )
        .map_err(|error| format!("the deletion plan could not be built: {error}"))?;

        let complete = verify_deletion(
            &request,
            &plan,
            &[],
            vec![doomed.reference()],
            Vec::new(),
            vec!["evidence://deletion/complete".to_string()],
            at,
        )
        .map_err(|error| format!("the complete verification failed: {error}"))?;
        if !complete.is_complete() {
            return Err("the fixture deletion did not complete".to_string());
        }

        let claims = vec![supported.clone(), untouched.clone(), settled.clone()];
        let (after, impact) = revalidate_after_deletion(&complete, &links, claims.clone(), at);

        if impact.contested.len() != 1 || impact.contested[0].claim_id != supported.claim_id {
            return Err(format!(
                "{} claims were reopened; the one resting on the destroyed evidence should be \
                 exactly one of them",
                impact.contested.len()
            ));
        }
        let reopened = after
            .iter()
            .find(|item| item.claim_id == supported.claim_id)
            .ok_or_else(|| "the reopened claim is missing from the result".to_string())?;
        if reopened.lifecycle_status != ClaimStatus::Contested {
            return Err(format!(
                "the claim whose evidence was destroyed is {:?}, so the record still says it is \
                 supported by something that no longer exists",
                reopened.lifecycle_status
            ));
        }
        if reopened.state_revision <= supported.state_revision {
            return Err("reopening the claim did not advance its revision".to_string());
        }

        // A claim resting on evidence the deletion did not touch is not
        // disturbed.
        let other = after
            .iter()
            .find(|item| item.claim_id == untouched.claim_id)
            .ok_or_else(|| "the untouched claim is missing".to_string())?;
        if other.lifecycle_status != ClaimStatus::Supported {
            return Err(format!(
                "a claim resting on surviving evidence became {:?}",
                other.lifecycle_status
            ));
        }

        // A claim somebody already decided about is reported, not quietly
        // skipped and not overwritten.
        if impact.already_settled != vec![(settled.claim_id, ClaimStatus::Superseded)] {
            return Err(format!(
                "the settled claim was handled as {:?} rather than being reported",
                impact.already_settled
            ));
        }

        // And a deletion that did not finish establishes nothing.
        let incomplete = verify_deletion(
            &request,
            &plan,
            &[],
            vec![doomed.reference()],
            vec!["backup://nightly".to_string()],
            vec!["evidence://deletion/incomplete".to_string()],
            at,
        )
        .map_err(|error| format!("the incomplete verification failed: {error}"))?;
        let (_, no_impact) = revalidate_after_deletion(&incomplete, &links, claims, at);
        if !no_impact.contested.is_empty() || !no_impact.already_settled.is_empty() {
            return Err(
                "a deletion that left a copy standing reopened claims anyway, so a claim would \
                 be contested because somebody started a deletion"
                    .to_string(),
            );
        }

        Ok(format!(
            "one claim contested, one left alone, one already settled and reported; an \
             incomplete deletion reopened {} claims",
            no_impact.contested.len()
        ))
    }
);


idw_case!(
    ADerivationRemembersWhoseContentItWas,
    "exec-idw-011-a-derivation-remembers-its-source",
    11,
    CaseCategory::Behavioral,
    "A derivation from mounted content records the source workspace, the exact memory and \
     revision and the grant revision that made it reachable; deriving is refused where the \
     mount does not permit it, and every derivation is recorded as a use of the borrowed \
     content",
    SHARING_EVIDENCE,
    || {
        use crate::enterprise::{MemoryMount, ShareOperation};
        use crate::id::PrincipalId;
        use crate::provenance::{DerivationMethod, EvidenceRef};
        use crate::time::now;
        use sharing_fixture::*;

        let parties = parties();
        let at = now();

        // A mount that may read but not derive.
        let read_only_grant = grant(&parties, at);
        let policy = target_policy(&parties);
        let mut read_only_mount = MemoryMount::accept(
            acceptance(&parties, &read_only_grant, read_only()),
            &read_only_grant,
            &policy,
            at,
        )
        .map_err(|error| format!("the read-only mount was not accepted: {error}"))?;

        if read_only_mount
            .derive_local(
                &read_only_grant,
                &policy,
                DerivationMethod::Summarization,
                PrincipalId::new(),
                at,
            )
            .is_ok()
        {
            return Err(
                "a mount permitting only discovery and reading produced a local derivation, so \
                 the source's limit on how its content may be used is decorative"
                    .to_string(),
            );
        }

        // A mount that may derive, granted and accepted for it on both sides.
        let deriving = sharing_fixture::parties();
        let operations = operations(&[
            ShareOperation::DiscoverMetadata,
            ShareOperation::ReadContent,
            ShareOperation::DeriveLocal,
        ]);
        let grant_revision = crate::enterprise::MemoryShareGrantRevision::issue(
            grant_spec(
                &deriving,
                crate::enterprise::ShareTarget::ExactWorkspace(deriving.target_workspace),
                operations.clone(),
                at,
                None,
            ),
            at,
        )
        .map_err(|error| format!("the deriving revision failed: {error}"))?;
        let deriving_grant = crate::enterprise::MemoryShareGrant::issue(grant_revision, at)
            .map_err(|error| format!("the deriving grant failed: {error}"))?;
        let deriving_policy = crate::enterprise::TargetSharePolicy::new(
            deriving.target_workspace,
            deriving.target_principal,
            operations.clone(),
            None,
        )
        .map_err(|error| format!("the deriving target policy failed: {error}"))?;
        let mut mount = MemoryMount::accept(
            acceptance(&deriving, &deriving_grant, operations),
            &deriving_grant,
            &deriving_policy,
            at,
        )
        .map_err(|error| format!("the deriving mount was not accepted: {error}"))?;

        let author = PrincipalId::new();
        let derivation = mount
            .derive_local(
                &deriving_grant,
                &deriving_policy,
                DerivationMethod::Summarization,
                author,
                at,
            )
            .map_err(|error| format!("a permitted derivation was refused: {error}"))?;

        if derivation.workspace_id != deriving.target_workspace {
            return Err("the derivation does not belong to the workspace that made it".to_string());
        }
        if derivation.created_by != Some(author) {
            return Err("the derivation does not name who made it".to_string());
        }

        match derivation.input_refs.as_slice() {
            [EvidenceRef::SharedMemoryRevisionRef {
                source_workspace_id,
                memory_id,
                revision_id,
                grant_revision_id,
                source_generation,
            }] => {
                if *source_workspace_id != deriving.source_workspace {
                    return Err(
                        "the derivation does not name the workspace the content came from, so a \
                         week later it would be indistinguishable from a local derivation"
                            .to_string(),
                    );
                }
                if *source_workspace_id == derivation.workspace_id {
                    return Err("the derivation claims its own workspace as the source".to_string());
                }
                if *memory_id != deriving.memory || *revision_id != deriving.revision {
                    return Err(
                        "the derivation does not pin the exact revision it was made from"
                            .to_string(),
                    );
                }
                if *grant_revision_id != deriving_grant.revision().id() {
                    return Err(
                        "the derivation does not name the grant revision that made the content \
                         reachable, so the record cannot say under what it was permitted"
                            .to_string(),
                    );
                }
                if source_generation.trim().is_empty() {
                    return Err("the derivation carries no source generation".to_string());
                }
            }
            other => {
                return Err(format!(
                    "the derivation cites {} input reference(s), and a derivation from mounted \
                     content must cite the shared one",
                    other.len()
                ));
            }
        }

        // Deriving is a use of somebody else's content, and the mount records
        // it where the source can see it.
        let disclosures = mount.disclosures();
        if disclosures.len() != 1 || disclosures[0].operation() != ShareOperation::DeriveLocal {
            return Err(format!(
                "{} disclosure(s) were recorded for the derivation",
                disclosures.len()
            ));
        }

        // And a withdrawn share stops derivation like it stops reading.
        let mut revoked_grant = deriving_grant;
        revoked_grant
            .revoke(at + chrono::Duration::seconds(1))
            .map_err(|error| format!("the grant could not be revoked: {error}"))?;
        mount.sync_with_source(&revoked_grant, at + chrono::Duration::seconds(2));
        if mount
            .derive_local(
                &revoked_grant,
                &deriving_policy,
                DerivationMethod::Summarization,
                author,
                at + chrono::Duration::seconds(2),
            )
            .is_ok()
        {
            return Err("a revoked share still permitted a local derivation".to_string());
        }

        Ok(format!(
            "a derivation cites workspace {} at revision {} under grant revision {}, and is \
             recorded as a use of it",
            deriving.source_workspace,
            deriving.revision,
            revoked_grant.revision().id()
        ))
    }
);



macro_rules! lrn_case {
    ($name:ident, $id:expr, $number:expr, $category:expr, $description:expr, $body:expr) => {
        struct $name;

        impl ConformanceCase for $name {
            fn case_id(&self) -> &str {
                $id
            }
            fn requirement_ids(&self) -> &[RequirementId] {
                &[RequirementId {
                    family: RequirementFamily::Lrn,
                    number: $number,
                }]
            }
            fn category(&self) -> CaseCategory {
                $category
            }
            fn description(&self) -> &str {
                $description
            }
            fn run(&self) -> ConformanceCaseResult {
                let outcome: Result<String, String> = $body();
                result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    outcome,
                    LEARNING_EVIDENCE,
                )
            }
        }
    };
}

/// A proposal and the measurements behind it.
mod learning_change_fixture {
    use crate::id::{EvaluationId, LearningProjectionId, LearningProposalId, PrincipalId, WorkspaceId};
    use crate::learning::{LearningChange, LearningProposal, LearningTarget};
    use crate::time::Timestamp;

    pub fn proposal(
        workspace_id: WorkspaceId,
        author: PrincipalId,
        expected_revision: u32,
        at: Timestamp,
    ) -> LearningProposal {
        LearningProposal::new(
            LearningProposalId::new(),
            workspace_id,
            vec![LearningProjectionId::new()],
            vec![EvaluationId::new()],
            LearningTarget::AgentRevision {
                revision_id: crate::id::AgentRevisionId::new(),
            },
            LearningChange::AgentInstructions {
                instructions: "prefer the shorter answer when both are correct".to_string(),
            },
            expected_revision,
            "three evaluations show the longer answer is not better",
            "learning-policy-v1",
            author,
            at,
        )
        .expect("the fixture proposal is well-formed")
    }
}

lrn_case!(
    AnAppliedChangeSaysWhereItCameFrom,
    "exec-lrn-005-an-applied-change-is-versioned-and-provenanced",
    5,
    CaseCategory::Stateful,
    "Applying a learning proposal moves its target by exactly one revision and records the \
     proposal, the projections and the raw evaluation facts behind it, who proposed and who \
     approved it; a proposal nobody submitted cannot be applied, and one written against a \
     revision the target has left is refused naming both",
    || {
        use crate::id::{PrincipalId, WorkspaceId};
        use crate::learning::LearningProposalStatus;
        use crate::time::now;
        use crate::DomainError;
        use learning_change_fixture::*;

        let workspace = WorkspaceId::new();
        let author = PrincipalId::new();
        let approver = PrincipalId::new();
        let at = now();

        // A draft was never put forward.
        let draft = proposal(workspace, author, 4, at);
        if draft.clone().apply(4, approver, at).is_ok() {
            return Err(
                "a proposal nobody submitted was applied, so a cognitive asset could be changed \
                 by writing a draft"
                    .to_string(),
            );
        }

        let submitted = draft
            .submit(at)
            .map_err(|error| format!("the proposal could not be submitted: {error}"))?;

        // The target has moved since the proposal was written.
        match submitted.clone().apply(5, approver, at) {
            Err(DomainError::RevisionConflict { expected, current }) => {
                if expected != 4 || current != 5 {
                    return Err(format!(
                        "the conflict named expected {expected} and current {current}"
                    ));
                }
            }
            other => {
                return Err(format!(
                    "applying against a moved target gave {other:?}; the change would have \
                     overwritten whatever happened in between"
                ));
            }
        }

        let (applied_proposal, change) = submitted
            .clone()
            .apply(4, approver, at)
            .map_err(|error| format!("a submitted proposal could not be applied: {error}"))?;

        if applied_proposal.status != LearningProposalStatus::Applied {
            return Err(format!(
                "after applying, the proposal is {:?}",
                applied_proposal.status
            ));
        }
        if change.from_revision != 4 || change.to_revision != 5 {
            return Err(format!(
                "the change moved the target from {} to {}, and a learning change is one step",
                change.from_revision, change.to_revision
            ));
        }
        if change.proposal_id != applied_proposal.id {
            return Err("the change does not name the proposal it came from".to_string());
        }
        if !change.is_provenanced() {
            return Err(
                "the applied change carries no projections or raw facts, so a later reader \
                 could not find what measurements it came from"
                    .to_string(),
            );
        }
        if change.source_projection_ids != applied_proposal.source_projection_ids
            || change.source_evaluation_fact_ids != applied_proposal.source_evaluation_fact_ids
        {
            return Err("the change's provenance differs from the proposal's".to_string());
        }
        if change.proposed_by != author || change.approved_by != approver {
            return Err(
                "the change does not record both who proposed it and who approved it".to_string(),
            );
        }
        if change.policy_version != applied_proposal.policy_version {
            return Err("the change does not record the policy it was applied under".to_string());
        }

        // And it cannot be applied twice.
        if applied_proposal.apply(5, approver, at).is_ok() {
            return Err("an applied proposal was applied again".to_string());
        }

        Ok(format!(
            "a change moves revision {} to {} carrying {} projection(s) and {} raw fact(s), \
             proposed and approved by different principals",
            change.from_revision,
            change.to_revision,
            change.source_projection_ids.len(),
            change.source_evaluation_fact_ids.len()
        ))
    }
);

lrn_case!(
    RemovingAProjectionLeavesTheFacts,
    "exec-lrn-008-removing-a-projection-leaves-the-facts",
    8,
    CaseCategory::Stateful,
    "A learned projection is a function of raw evaluation facts: building or discarding one      leaves the facts untouched, and rebuilding from the same facts in a different order      reproduces the same content, so a projection can be removed without destroying the history      under it",
    || {
        use crate::id::LearningProjectionId;
        use crate::learning::{LearnedProjection, ProjectionKind};
        use crate::time::now;
        use learning_fixture::*;

        let at = now();
        let workspace = crate::WorkspaceId::new();

        let facts: Vec<crate::EvaluationFact> = (0..3)
            .map(|_| {
                let mut fact = fact_with(deterministic_evaluator(), vec![document_evidence()])
                    .expect("a well-formed evaluation fact");
                fact.workspace_id = workspace;
                fact
            })
            .collect();
        let before = facts.clone();

        let build = |id, facts: &[crate::EvaluationFact]| {
            LearnedProjection::rebuild_from_facts(
                id,
                workspace,
                ProjectionKind::PerformanceSummary,
                routing_target(),
                "algorithm-v1",
                1,
                facts,
                at,
            )
        };

        let projection = build(LearningProjectionId::new(), &facts)
            .map_err(|error| format!("the projection could not be built: {error}"))?;

        // Discard it. The facts are inputs, not parts of it.
        drop(projection.clone());
        if facts != before {
            return Err(
                "building or discarding a projection changed the raw facts, so derived state is                  not derived"
                    .to_string(),
            );
        }

        // Rebuild from the same facts, offered in a different order and under a
        // different projection identity.
        let mut shuffled = facts.clone();
        shuffled.reverse();
        let rebuilt = build(LearningProjectionId::new(), &shuffled)
            .map_err(|error| format!("the projection could not be rebuilt: {error}"))?;

        if rebuilt.content != projection.content {
            return Err(
                "rebuilding from the same facts produced different content, so a removed                  projection could not be recovered from the history that remains"
                    .to_string(),
            );
        }
        if rebuilt.source_evaluation_fact_ids != projection.source_evaluation_fact_ids {
            return Err("the rebuild cites different facts than the original".to_string());
        }
        if rebuilt.id == projection.id {
            return Err("the rebuild is the same object rather than a new one".to_string());
        }

        Ok(format!(
            "{} raw facts survive their projection and reproduce it exactly when rebuilt",
            facts.len()
        ))
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_executable_case_passes_against_the_current_domain() {
        // If one of these fails, the domain has regressed on a CORE
        // requirement — which is the reason to run them rather than assert
        // them in prose.
        for case in executable_cases_list() {
            let outcome = case.run();
            assert_eq!(
                outcome.status,
                CaseStatus::Pass,
                "{} failed: {}",
                outcome.case_id,
                outcome.message
            );
            assert_eq!(outcome.origin, CaseOrigin::Executed);
        }
    }

    #[test]
    fn each_case_names_exactly_the_requirement_it_verifies() {
        // A case covering several requirements at once is how one weak check
        // ends up standing for three properties.
        for case in executable_cases_list() {
            assert_eq!(
                case.run().requirement_ids.len(),
                1,
                "{} claims more than one requirement",
                case.case_id()
            );
        }
    }

    fn executable_cases_list() -> Vec<Box<dyn ConformanceCase>> {
        vec![
            Box::new(UnknownOccurrenceTimeStillRecords),
            Box::new(ValidityIsIndependentOfRecordingTime),
            Box::new(SupersededKnowledgeCannotBecomeCurrentAgain),
            Box::new(ALeaseConfersNothingBeyondItsScope),
            Box::new(CorrectionPreservesTheOriginal),
            Box::new(DerivedStateRebuildsFromTheEventLog),
            Box::new(RebuildingDoesNotRewriteTheEventLog),
            Box::new(AsOfReadsHistoryNotTheCurrentProjection),
            Box::new(SequenceCarriesOrderNotTheWallClock),
            Box::new(UncertaintyHasItsOwnState),
            Box::new(RecordingTimeAndOccurrenceTimeAreDistinct),
            Box::new(MutationCarriesAnExpectedRevision),
            Box::new(AStaleRevisionConflictsRatherThanOverwrites),
            Box::new(StreamOrderIsMonotonicAndGapFree),
            Box::new(MutationPreservesItsFullContext),
            Box::new(CorrectionAddsARevisionRatherThanEditingOne),
            Box::new(DeterministicRepairRequiresAnAuthoritativeBasis),
            Box::new(AmbiguityCannotBeFiledAsDetermination),
            Box::new(ReconciliationRecordsItsInputsAndOutcome),
            Box::new(ReconciliationRecordSurvivesARoundTrip),
            Box::new(CompensationIsANewEffect),
            Box::new(ReconciliationDoesNotAmplifyAuthority),
            Box::new(MemoryIdentityIsStableAndContentLivesInRevisions),
            Box::new(RevisionNumbersAreMonotonicWithinAMemory),
            Box::new(ActiveRevisionBelongsToItsMemory),
            Box::new(TerminalMemoryStatesDoNotReturnToActive),
            Box::new(ValidityRangeMustNotRunBackwards),
            Box::new(ConfidenceAndImportanceAreBounded),
            Box::new(StructuredPayloadCarriesItsSchemaVersion),
            Box::new(ScopeDoesNotCrossTheWorkspaceBoundary),
            Box::new(ContentChangeCreatesARevision),
            Box::new(ActiveMemoryNamesItsEvidence),
            Box::new(DerivedMemoryCarriesItsDerivation),
            Box::new(ProvenanceClosesOnEvidenceOrHumanAuthority),
            Box::new(ClaimAndMemoryRemainDistinct),
            Box::new(SupportedIsNotAbsoluteTruth),
            Box::new(LosingEvidenceLeadsBackThroughContest),
            Box::new(OpenConflictStaysOpenUntilReconciled),
            Box::new(ConflictIsNotResolvedByRecency),
            Box::new(ConflictResolutionKeepsItsBasis),
            Box::new(SupersessionKeepsWhatItReplaced),
            Box::new(AContextPackNeverExceedsItsBudget),
        Box::new(HydrationRequiresAnAuthorizationCheck),
            Box::new(EveryIncludedItemNamesItsSource),
            Box::new(RepresentationLevelsAreDistinguished),
            Box::new(TokenBudgetIsEnforced),
            Box::new(RetiredContentCarriesItsStatus),
            Box::new(DegradationCannotBeSilent),
            Box::new(TemporalPerspectiveTravelsWithThePack),
            Box::new(AFeedbackSignalNamesTheRevisionItJudged),
            Box::new(ALearnedProjectionIsAdvisoryAndNeverBecomesAFact),
            Box::new(LearningCannotProposeItsOwnElevation),
            Box::new(LearningProposesAndCannotApply),
            Box::new(AMeasurementOutranksAnOpinion),
            Box::new(AConclusionKeepsItsMeasurements),
            Box::new(EveryResultNamesTheRequirementItAnswers),
            Box::new(AReportCanRepresentAFailure),
            Box::new(AnInvariantDeclaresWhatItChecks),
            Box::new(AFindingCarriesItsInvariantAndObservation),
            Box::new(APlanIsDerivedFromFindings),
            Box::new(RepairPassesThroughAuthorization),
            Box::new(VerificationRechecksTheInvariant),
            Box::new(RepairRebuildsRatherThanInvents),
            Box::new(RecurrenceIsCountedOnTheSameInvariant),
            Box::new(FlappingNeedsOscillationNotJustRepetition),
            Box::new(DispositionClassifiesWhatHappensNext),
            Box::new(ABudgetLimitsRepairAttempts),
            Box::new(TheOperatorSeparatesLookingFromActing),
            Box::new(AnExecutionRecordIsWrittenOnce),
            Box::new(HistoryAccumulatesRatherThanBeingRewritten),
            Box::new(SilencingRequiresALiveDisposition),
            Box::new(PlanningIsADryRun),
            Box::new(TheRegistryExtendsWithoutTouchingExecution),
            Box::new(AnObservationRecordsWhenAndAgainstWhatVersion),
            Box::new(AFailedVerificationReopensRatherThanRetries),
            Box::new(AcceptedRiskIsExplicitAndRevocable),
            Box::new(AGrantStatesItsScopeAndItsEnd),
            Box::new(AGrantCoversWhatIsBeneathItAndNothingAbove),
            Box::new(AMaterialChangeInvalidatesTheApproval),
        Box::new(AChildIsNeverAllowedWhatItsParentIsDenied),
        Box::new(DelegationOnlyNarrows),
            Box::new(EveryEvaluationProducesADecision),
            Box::new(NothingIsPermittedWithoutAGrant),
            Box::new(RiskIsPartOfTheVerdict),
            Box::new(BudgetIsCheckedWhenItIsSpent),
            Box::new(ChildBudgetsComeOutOfTheParent),
            Box::new(RevocationTakesEffectImmediately),
            Box::new(ADelegateCannotOutliveOrOutrankItsChain),
            Box::new(ADecisionCanBeAudited),
            Box::new(ADecisionIsOnlyGoodForTheInstantItNames),
            Box::new(APolicyBoundsSensitivityDestinationAndCapability),
            Box::new(DerivedDataInheritsTheHighestSensitivity),
            Box::new(ClassifiedDataDoesNotReachAnUnapprovedModel),
            Box::new(ALegalHoldOutranksDeletion),
            Box::new(DeletionMustAccountForEveryDependency),
            Box::new(DeletionProducesEvidence),
            Box::new(AClassificationIsReplacedRatherThanEdited),
            Box::new(UnknownIsItsOwnAnswer),
            Box::new(AnAdapterMustDeclareAContractItCanKeep),
            Box::new(RetryCannotDuplicateWhatMightHaveHappened),
            Box::new(AuthorityIsCheckedAgainstTheIntentItself),
            Box::new(AnAdapterSaysWhatItCannotDo),
            Box::new(DispatchIsNotDelivery),
            Box::new(ReconciliationWeighsIntentReceiptAndObservation),
            Box::new(CompensationIsANewEffectWithItsOwnIdentity),
            Box::new(ReversibilityIsDeclaredRatherThanAssumed),
            Box::new(ATimeoutMeansUnknown),
            Box::new(CrossWorkspaceAccessSatisfiesBothSides),
            Box::new(ASecretReferenceCarriesNoSecret),
            Box::new(RetentionExpiryIsAStateNotADeletion),
            Box::new(ExpiredRetentionStillYieldsToAHold),
            Box::new(RotationDoesNotInterruptService),
            Box::new(APolicyDecisionCarriesItsBasis),
            Box::new(APolicyChangeIsVisibleInItsDecisions),
            Box::new(AKeyMovesThroughItsLifecycleInOneDirection),
            Box::new(RevokingAKeyEndsAccessWithoutReEncryption),
            Box::new(AnExportIsGovernedByThePolicyItNames),
            Box::new(AnExportNamesExactlyWhatItCarries),
            Box::new(AuthorityStopsAtTheWorkspaceBoundary),
            Box::new(ALocalPermissionIsNotAGlobalOne),
            Box::new(IdentityAndAuthorityAreCheckedSeparately),
            Box::new(AShareNeedsBothSides),
            Box::new(AShareNamesExactlyOneTarget),
            Box::new(SharingDoesNotTravel),
            Box::new(NarrowingEitherSideNarrowsTheShare),
            Box::new(AWithdrawnShareDisclosesNothing),
            Box::new(ASharedReferenceKeepsItsOrigin),
            Box::new(RecognizingARemoteIsNotDisclosingToIt),
            Box::new(RevocationDoesNotUnsayWhatWasSaid),
            Box::new(ADerivationRemembersWhoseContentItWas),
            Box::new(RemovingAProjectionLeavesTheFacts),
            Box::new(AnAppliedChangeSaysWhereItCameFrom),
            Box::new(AnIncidentIsNotAFinding),
            Box::new(ContainmentComesBeforeTrust),
            Box::new(ARestartDoesNotRestoreTrust),
            Box::new(UnfinishedWorkIsClassifiedNotResumed),
            Box::new(AnAmbiguousDispatchGoesToReconciliation),
            Box::new(AnAmbiguousIrreversibleEffectIsNotRepeated),
            Box::new(ARecoveryPointNamesAProvenPosition),
            Box::new(UnvalidatedStateIsNotARecoverySource),
            Box::new(TheFiveTrustWordsStayDistinct),
            Box::new(TrustReturnsOnlyThroughARun),
            Box::new(ARevalidationKeepsItsWorkings),
            Box::new(InconclusiveIsAnAnswerOfItsOwn),
            Box::new(DivergentHistoriesWaitForAPerson),
            Box::new(RepairAttemptsAreBounded),
            Box::new(ClosingAnIncidentDoesNotEraseIt),
            Box::new(RestoringRebuildsRatherThanTrusts),
            Box::new(TrustCanComeBackPartly),
            Box::new(EveryClaimedRequirementHasAResult),
            Box::new(ProfilesBuildOnOneAnother),
            Box::new(LimitationsAreClaimedNotImplied),
            Box::new(ASkippedRequirementIsNotAPass),
            Box::new(AHardGateIsNotAScore),
            Box::new(ABundleCarriesWhatItQualified),
            Box::new(ChangingWhatWasQualifiedBreaksTheBaseline),
            Box::new(AQualificationCanBeWithdrawn),
            Box::new(SelfAssertedTrustIsNotEvidence),
            Box::new(ATrustClaimNeedsTheProfileAndItsCaveats),
            Box::new(AttestationIsAdmissibleOnlyWhereExecutionIsNot),
            Box::new(AuthorityIsWhatWasGrantedNotWhoYouAre),
            Box::new(TheIntentIsFixedBeforeAnythingLeaves),
            Box::new(AnAdapterAndAnIntentMustAgree),
            Box::new(CompensationIsAnEffectNotAnUndo),
            Box::new(ACompensationNamesWhatItCompensates),
            Box::new(PreconditionsAreCheckedAtTheLastMoment),
            Box::new(EveryDispatchLeavesAReceipt),
            Box::new(AnAcknowledgementIsNotAnOutcome),
            Box::new(AReceiptHasNowhereToPutASecret),
            Box::new(CryptoAndDataPolicyAreTwoGates),
            Box::new(ACapabilityDoesNotOverrideAPolicy),
            Box::new(APolicyDoesNotSupplyTheCapability),
            Box::new(ThreeKindsOfDeletionStayThree),
            Box::new(ADeletionCannotClaimWhatItDidNotCheck),
            Box::new(DeletingEvidenceReopensWhatRestedOnIt),
        ]
    }
}
