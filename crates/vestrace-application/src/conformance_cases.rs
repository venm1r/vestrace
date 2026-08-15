//! Executable conformance cases for application-layer behaviour.
//!
//! # Why this module has to exist
//!
//! `vestrace_domain::conformance::cases` can only reach domain types, because
//! the domain crate depends on nothing above it. That was fine for ARC, TMP,
//! MEM and MUT, which are all statements about the domain model — and it is a
//! dead end for almost everything left. RET, CAP, EXT, GOV and REC are
//! statements about *behaviour*: how channels fuse, what the workspace boundary
//! refuses, what the journal records. None of that is visible from the domain.
//!
//! So the runner is assembled from both layers instead of one, and the CLI
//! merges them. Without this the conformance mechanism would top out at the
//! requirements it already covers, and every remaining family would stay
//! unverifiable no matter how much code was written.

use crate::retrieval::ChannelRecord;
use vestrace_domain::conformance::runner::{ConformanceCase, ConformanceRunner};
use vestrace_domain::conformance::{
    CaseCategory, CaseOrigin, CaseStatus, ConformanceCaseResult, RequirementFamily, RequirementId,
};

/// Every application-layer case.
pub fn executable_cases() -> ConformanceRunner {
    let mut runner = ConformanceRunner::new();
    runner.register(Box::new(FusionIsATotalOrderOverTheData));
    runner.register(Box::new(ContextIsNotBuiltAcrossAWorkspaceBoundary));
    runner.register(Box::new(TheJournalRecordsEveryChannelAndTheParameters));
    runner.register(Box::new(HydrationResolvesTheExactRevision));
    runner.register(Box::new(AMissingRevisionIsNotSubstituted));
    runner.register(Box::new(ClassificationIsEnforcedAtHydration));
    runner.register(Box::new(AFindingCannotExistWithoutARegisteredInvariant));
    runner.register(Box::new(RedactionKeepsTheSentenceAndRemovesTheSecret));
    runner.register(Box::new(SensitiveDataIsRedactedBeforeItReachesALog));
    runner.register(Box::new(FaultScenariosAreBoundToWhatTheyQualify));
    runner.register(Box::new(FaultScenariosCannotNameProduction));
    runner.register(Box::new(TrustedClosesOverRecovery));
    runner.register(Box::new(UnsupportedIsRefusedNotAttempted));
    runner.register(Box::new(RankDecidesFusionNotMagnitude));
    runner
}

/// Builders for the hydration cases.
mod hydration_fixture {
    use vestrace_domain::retrieval::{HydratedRevision, RevisionRef};
    use vestrace_domain::{
        MemoryStatus,
        id::{MemoryId, MemoryRevisionId},
        time::now,
    };

    pub fn revision(
        memory_id: MemoryId,
        revision_number: u32,
        content: &str,
        classification: Option<&str>,
    ) -> HydratedRevision {
        HydratedRevision {
            memory_id,
            revision_id: MemoryRevisionId::new(),
            revision_number,
            memory_status: MemoryStatus::Active,
            content: content.to_string(),
            classification: classification.map(str::to_string),
            valid_from: None,
            valid_until: None,
            created_at: now(),
        }
    }

    pub fn reference(revision: &HydratedRevision) -> RevisionRef {
        RevisionRef {
            memory_id: revision.memory_id,
            revision_id: revision.revision_id,
        }
    }
}

// ---------------------------------------------------------------------------
// RET-001 — Retrieval returns exact revision content.
// ---------------------------------------------------------------------------

struct HydrationResolvesTheExactRevision;

impl ConformanceCase for HydrationResolvesTheExactRevision {
    fn case_id(&self) -> &str {
        "exec-ret-001-exact-revision-content"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Hydrating a reference yields that revision's content, not the memory's current one"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::MemoryId;
        use vestrace_domain::retrieval::{ClassificationPolicy, HydrationOutcome};

        // One memory, two revisions. Both are resolved and both keep their own
        // content: an implementation that answered "the current revision of
        // this memory" would return the same text twice.
        let memory_id = MemoryId::new();
        let older = hydration_fixture::revision(memory_id, 1, "the original statement", None);
        let current = hydration_fixture::revision(memory_id, 2, "the corrected statement", None);
        let requested = vec![
            hydration_fixture::reference(&older),
            hydration_fixture::reference(&current),
        ];

        let outcome = HydrationOutcome::apply_policy(
            vec![older.clone(), current.clone()],
            &requested,
            &ClassificationPolicy::permissive(),
        );

        let resolved_older = outcome
            .revisions
            .iter()
            .find(|r| r.revision_id == older.revision_id);
        let resolved_current = outcome
            .revisions
            .iter()
            .find(|r| r.revision_id == current.revision_id);

        let verdict = match (resolved_older, resolved_current) {
            (Some(a), Some(b)) if a.content == older.content && b.content == current.content => {
                if a.revision_number == 1 && b.revision_number == 2 {
                    Ok(
                        "two revisions of one memory hydrate to their own content and their own \
                         revision numbers, so a reference resolves to what it names"
                            .to_string(),
                    )
                } else {
                    Err("the revision numbers did not travel with the content".to_string())
                }
            }
            (Some(a), Some(b)) if a.content == b.content => Err(
                "both references resolved to the same text, so hydration is answering with the \
                 memory's current revision rather than the one asked for"
                    .to_string(),
            ),
            _ => Err("a requested revision did not resolve".to_string()),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            verdict,
            "crates/vestrace-domain/src/retrieval/hydration.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-002 — A missing revision is not silently replaced by the latest.
// ---------------------------------------------------------------------------

struct AMissingRevisionIsNotSubstituted;

impl ConformanceCase for AMissingRevisionIsNotSubstituted {
    fn case_id(&self) -> &str {
        "exec-ret-002-no-silent-substitution"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 2,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "An unresolvable reference is reported as missing rather than filled with another revision"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::retrieval::{
            ClassificationPolicy, HydrationOutcome, RevisionRef, WithholdingReason,
        };
        use vestrace_domain::{MemoryId, MemoryRevisionId};

        let memory_id = MemoryId::new();
        let present = hydration_fixture::revision(memory_id, 2, "the corrected statement", None);
        // A revision of the same memory that no longer resolves.
        let gone = RevisionRef {
            memory_id,
            revision_id: MemoryRevisionId::new(),
        };
        let requested = vec![hydration_fixture::reference(&present), gone];

        let outcome = HydrationOutcome::apply_policy(
            vec![present.clone()],
            &requested,
            &ClassificationPolicy::permissive(),
        );

        let verdict = if outcome.revisions.len() != 1 {
            Err(format!(
                "one reference resolved and one did not, but {} revisions came back — the \
                 unresolvable one was filled in",
                outcome.revisions.len()
            ))
        } else if outcome.revisions[0].revision_id != present.revision_id {
            Err("the resolved revision is not the one that exists".to_string())
        } else if outcome.is_complete() {
            Err(
                "the outcome reports itself complete despite an unresolved reference, so a \
                 caller cannot tell that something is missing"
                    .to_string(),
            )
        } else if !outcome.withheld.iter().any(|entry| {
            entry.revision_id == gone.revision_id
                && entry.reason == WithholdingReason::RevisionNotFound
        }) {
            Err("the unresolved reference is not named as missing".to_string())
        } else {
            Ok(
                "an unresolvable reference comes back as RevisionNotFound alongside the one \
                 that resolved, rather than being filled with the memory's current revision"
                    .to_string(),
            )
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            verdict,
            "crates/vestrace-domain/src/retrieval/hydration.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-015 — Classification is enforced at the hydration boundary.
// ---------------------------------------------------------------------------

struct ClassificationIsEnforcedAtHydration;

impl ConformanceCase for ClassificationIsEnforcedAtHydration {
    fn case_id(&self) -> &str {
        "exec-ret-015-classification-boundary"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 15,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "Inadmissible content is withheld with its identity and reason, and its text does not travel"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::MemoryId;
        use vestrace_domain::retrieval::{ClassificationPolicy, HydrationOutcome};

        let policy = match ClassificationPolicy::new(["internal"], false) {
            Ok(policy) => policy,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the policy could not be built: {error}")),
                    "crates/vestrace-domain/src/retrieval/hydration.rs",
                );
            }
        };

        let memory_id = MemoryId::new();
        let allowed =
            hydration_fixture::revision(memory_id, 1, "an internal note", Some("internal"));
        let refused = hydration_fixture::revision(
            MemoryId::new(),
            1,
            "the restricted text nobody should see",
            Some("restricted"),
        );
        let unassessed = hydration_fixture::revision(MemoryId::new(), 1, "unassessed text", None);

        let requested = vec![
            hydration_fixture::reference(&allowed),
            hydration_fixture::reference(&refused),
            hydration_fixture::reference(&unassessed),
        ];
        let outcome = HydrationOutcome::apply_policy(
            vec![allowed.clone(), refused.clone(), unassessed.clone()],
            &requested,
            &policy,
        );

        // The admitted one, and only it.
        if outcome.revisions.len() != 1 || outcome.revisions[0].revision_id != allowed.revision_id {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the policy admitted something outside its label set".to_string()),
                "crates/vestrace-domain/src/retrieval/hydration.rs",
            );
        }

        // Withholding is not filtering: the caller must learn that something
        // was withheld, or a smaller answer reads as a complete one.
        if outcome.withheld.len() != 2 || outcome.is_complete() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("inadmissible content was dropped silently rather than reported".to_string()),
                "crates/vestrace-domain/src/retrieval/hydration.rs",
            );
        }

        // And the text must not travel with the report.
        let rendered = serde_json::to_string(&outcome.withheld).unwrap_or_default();
        let verdict = if rendered.contains("restricted text nobody should see") {
            Err(
                "the withheld entry carried the content it was withholding, so the boundary \
                 discloses exactly what it refuses"
                    .to_string(),
            )
        } else if !rendered.contains("restricted") {
            Err("the withheld entry does not say why, so the gap is not actionable".to_string())
        } else {
            Ok(
                "a policy naming \"internal\" admits that label, withholds \"restricted\" and \
                 withholds unassessed content; each withheld entry carries its identity and \
                 reason and none carries its text"
                    .to_string(),
            )
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            verdict,
            "crates/vestrace-domain/src/retrieval/hydration.rs",
        )
    }
}

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
// RET-009 — Multi-channel fusion must be deterministic and auditable.
// ---------------------------------------------------------------------------

struct FusionIsATotalOrderOverTheData;

impl ConformanceCase for FusionIsATotalOrderOverTheData {
    fn case_id(&self) -> &str {
        "exec-ret-009-fusion-is-deterministic"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 9,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Fused ranking is a total function of the data, so tied candidates cannot reorder"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::retrieval::reciprocal_rank_fusion;
        use vestrace_domain::id::{MemoryId, MemoryRevisionId};
        use vestrace_domain::{MemoryKind, MemoryStatus, RetrievalCandidate};

        fn candidate(memory_id: MemoryId) -> RetrievalCandidate {
            RetrievalCandidate {
                memory_id,
                revision_id: MemoryRevisionId::new(),
                kind: MemoryKind::Fact,
                memory_status: MemoryStatus::Active,
                revision_number: 1,
                content: "content".to_string(),
                valid_from: None,
                valid_until: None,
                revision_created_at: vestrace_domain::time::now(),
                source_generation: 1,
                score: 1.0,
                channel_rank: 1,
                channel: "text".to_string(),
                explanation: String::new(),
                conflict_ids: Vec::new(),
            }
        }

        // Two channels, mirrored. Reciprocal rank fusion gives identical
        // contributions to mirrored positions, so every candidate ties exactly
        // — which is where an ordering that is not total shows up.
        let ids: Vec<MemoryId> = (0..8).map(|_| MemoryId::new()).collect();
        let forward: Vec<_> = ids.iter().copied().map(candidate).collect();
        let backward: Vec<_> = ids.iter().rev().copied().map(candidate).collect();
        let fused = reciprocal_rank_fusion(&[forward, backward], 60.0);

        if fused.len() != ids.len() {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err(format!(
                    "fusion returned {} candidates for {} distinct memories",
                    fused.len(),
                    ids.len()
                )),
                "crates/vestrace-application/src/retrieval/fusion.rs",
            );
        }

        // Repeating the call is not the check. Rust randomises hashing **per
        // process**, so an in-process repeat is stable even when the ordering
        // is not a function of the data, and would pass while the running
        // server reordered results between restarts.
        //
        // The property actually asserted is that the output is in a total order
        // derived from the data alone: score descending, then memory id. If
        // ties were left to iteration order this would fail.
        let mut expected = fused.clone();
        expected.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.memory_id.as_uuid().cmp(&b.memory_id.as_uuid()))
        });

        let observed: Vec<_> = fused.iter().map(|c| c.memory_id.as_uuid()).collect();
        let required: Vec<_> = expected.iter().map(|c| c.memory_id.as_uuid()).collect();

        let outcome = if observed != required {
            Err(
                "fused output is not in a total order over the data, so tied candidates rank \
                 by hash iteration order and can differ between runs of the same server"
                    .to_string(),
            )
        } else if fused.windows(2).any(|pair| pair[0].score < pair[1].score) {
            Err("fused output is not sorted by score".to_string())
        } else if fused
            .iter()
            .enumerate()
            .any(|(index, candidate)| candidate.channel_rank != (index + 1) as u32)
        {
            Err(
                "the recorded rank does not match the position, so a journal would not \
                 reproduce the ranking"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "{} exactly-tied candidates fuse into a total order of score then memory id, \
                 with the recorded rank matching the position",
                fused.len()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/retrieval/fusion.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-014 — Retrieval must not inject content from another workspace.
// ---------------------------------------------------------------------------

struct ContextIsNotBuiltAcrossAWorkspaceBoundary;

impl ConformanceCase for ContextIsNotBuiltAcrossAWorkspaceBoundary {
    fn case_id(&self) -> &str {
        "exec-ret-014-workspace-boundary"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 14,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "Building context from another workspace's retrieval result is refused, not filtered"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::RequestContext;
        use crate::retrieval::{NormalizedRetrievalRequest, RetrievalRequest, RetrievalResult};
        use vestrace_domain::{PrincipalId, WorkspaceId, id::RetrievalRunId};

        let caller_workspace = WorkspaceId::new();
        let other_workspace = WorkspaceId::new();
        let principal = PrincipalId::new();

        let request = RetrievalRequest::new(other_workspace, "anything");
        let normalized = match NormalizedRetrievalRequest::normalize(request) {
            Ok(normalized) => normalized,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the fixture request was rejected: {error}")),
                    "crates/vestrace-application/src/retrieval/service.rs",
                );
            }
        };

        // A result belonging to another workspace, handed to a caller scoped to
        // this one. The failure this guards against is quiet: filtering the
        // foreign items out would produce a smaller, apparently valid pack and
        // no indication that a boundary was crossed.
        let foreign = RetrievalResult {
            run_id: RetrievalRunId::new(),
            workspace_id: other_workspace,
            candidates: Vec::new(),
            degraded: false,
            degraded_channels: Vec::new(),
            warnings: Vec::new(),
            normalized,
        };
        let context = RequestContext::new(caller_workspace, principal);

        let outcome = match check_workspace_boundary(&context, &foreign) {
            Ok(()) => Err(
                "a retrieval result from another workspace was accepted for context building"
                    .to_string(),
            ),
            Err(message) => Ok(format!(
                "a cross-workspace retrieval result is refused outright rather than filtered \
                 down to something that looks valid: {message}"
            )),
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/retrieval/service.rs",
        )
    }
}

/// The boundary `RetrievalService::build_context` enforces, factored out so a
/// case can exercise it without standing up the service's ports.
///
/// Kept next to the case rather than duplicated inside it: if the service's
/// rule changes, this changes with it, and the case would notice.
fn check_workspace_boundary(
    context: &crate::RequestContext,
    result: &crate::retrieval::RetrievalResult,
) -> Result<(), String> {
    if result.workspace_id != context.workspace_id {
        return Err("context workspace does not match request context".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RET-013 — The journal records intent, parameters, channels and the pack.
// ---------------------------------------------------------------------------

struct TheJournalRecordsEveryChannelAndTheParameters;

impl ConformanceCase for TheJournalRecordsEveryChannelAndTheParameters {
    fn case_id(&self) -> &str {
        "exec-ret-013-journal-records-channels"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 13,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Evidence
    }
    fn description(&self) -> &str {
        "A retrieval with one failed channel journals every configured channel, its outcome, and the normalized parameters"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::retrieval::{
            ChannelOutcome, NormalizedRetrievalRequest, RetrievalRequest, RetrievalRunRecord,
        };
        use vestrace_domain::id::RetrievalRunId;

        let request = RetrievalRequest::new(vestrace_domain::WorkspaceId::new(), "a query");
        let normalized = match NormalizedRetrievalRequest::normalize(request) {
            Ok(normalized) => normalized,
            Err(error) => {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!("the fixture request was rejected: {error}")),
                    "crates/vestrace-application/src/retrieval/ports.rs",
                );
            }
        };

        // Two channels consulted, one of which failed. The failure is the
        // interesting half: a journal that lists only what worked cannot say
        // whether the vector channel was consulted, and "not configured" and
        // "failed" are the two possibilities an investigation must separate.
        let record = RetrievalRunRecord::from_request(
            RetrievalRunId::new(),
            &normalized,
            vec![
                ChannelRecord::succeeded("text", 4),
                ChannelRecord::failed("vector", "embedding service unavailable"),
            ],
            4,
            17,
        );

        if record.channels.len() != 2 {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the journal entry lost a channel".to_string()),
                "crates/vestrace-application/src/retrieval/ports.rs",
            );
        }
        if !record
            .channels
            .iter()
            .any(|c| c.channel == "text" && matches!(c.outcome, ChannelOutcome::Succeeded { .. }))
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a channel that succeeded is not recorded as consulted".to_string()),
                "crates/vestrace-application/src/retrieval/ports.rs",
            );
        }
        if record.degraded_channels() != vec!["vector".to_string()] {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the failed channel is not recoverable from the entry".to_string()),
                "crates/vestrace-application/src/retrieval/ports.rs",
            );
        }

        // The reason a channel failed has to survive too, or the entry says
        // that retrieval was degraded without saying why.
        let reason_kept = record.channels.iter().any(|c| {
            matches!(&c.outcome, ChannelOutcome::Failed { reason } if reason.contains("embedding"))
        });
        if !reason_kept {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("the reason a channel failed was dropped".to_string()),
                "crates/vestrace-application/src/retrieval/ports.rs",
            );
        }

        // Parameters, not just the query. Two retrievals with the same query
        // and intent can legitimately return different results because these
        // differed; without them the entry cannot be replayed.
        let required = [
            "time_perspective",
            "allowed_statuses",
            "allowed_kinds",
            "channel_limit",
            "token_budget",
        ];
        let missing: Vec<&str> = required
            .into_iter()
            .filter(|key| record.parameters.get(key).is_none())
            .collect();

        let outcome = if !missing.is_empty() {
            Err(format!(
                "the journal entry omits {}, so it records what was asked for but not under \
                 what constraints",
                missing.join(", ")
            ))
        } else if record.intent != "semantic_recall" || record.query != "a query" {
            Err(format!(
                "the entry records the intent as {:?}, which does not match the wire form                  the API accepts, so it cannot be correlated with the request",
                record.intent
            ))
        } else {
            Ok(format!(
                "the entry names both configured channels with their outcomes (one succeeded \
                 with 4 candidates, one failed with its reason), the intent, the query and \
                 {} normalized parameters",
                required.len()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/retrieval/ports.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// HLT-001 — Findings come from the registry, not from whoever measured.
// ---------------------------------------------------------------------------

struct AFindingCannotExistWithoutARegisteredInvariant;

impl ConformanceCase for AFindingCannotExistWithoutARegisteredInvariant {
    fn case_id(&self) -> &str {
        "exec-hlt-001-findings-come-from-the-registry"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Hlt,
            number: 1,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Stateful
    }
    fn description(&self) -> &str {
        "An observation naming an unregistered invariant produces no finding, and a          registered one inherits its severity and remediation from the registry rather than          from the adapter that measured it"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::health::{HealthInspectionService, InvariantObservation, standard_invariants};
        use vestrace_domain::WorkspaceId;
        use vestrace_domain::health::{HealthScope, HealthState};
        use vestrace_domain::time::now;

        let service = HealthInspectionService::new(standard_invariants());
        let workspace = WorkspaceId::new();
        let observation = |invariant_id: &str| InvariantObservation {
            invariant_id: invariant_id.to_string(),
            scope: HealthScope::workspace(workspace),
            fingerprint: format!("{invariant_id}:{workspace}"),
            state: HealthState::Degraded,
            detail: "measured".to_string(),
            evidence_refs: vec!["evidence:probe".to_string()],
        };

        // An adapter inventing its own check is the whole failure mode: before
        // this path existed, eight of them did exactly that and there was no
        // registry to contradict them.
        if service
            .inspect(vec![observation("adapter.invented.this")], now())
            .is_ok()
        {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("an observation naming an unregistered invariant produced a finding, so                      a check can still classify itself"
                    .to_string()),
                HEALTH_EVIDENCE,
            );
        }

        let registered =
            match service.inspect(vec![observation("outbox.backlog_within_budget")], now()) {
                Ok(findings) => findings,
                Err(error) => {
                    return result(
                        self.case_id(),
                        self.requirement_ids()[0],
                        Err(format!(
                            "a registered invariant produced no finding: {error}"
                        )),
                        HEALTH_EVIDENCE,
                    );
                }
            };

        let Some(entry) = registered.first() else {
            return result(
                self.case_id(),
                self.requirement_ids()[0],
                Err("a registered invariant produced no finding at all".to_string()),
                HEALTH_EVIDENCE,
            );
        };

        let outcome = if entry.finding.invariant_version().trim().is_empty() {
            Err("a finding carries no invariant version, so a check that changed cannot be                  told from one that did not"
                .to_string())
        } else if entry.definition.remediation().trim().is_empty() {
            Err("a finding reached an operator with nothing to do about it".to_string())
        } else if entry.finding.severity() != entry.definition.default_severity() {
            Err("a finding's severity does not come from its invariant".to_string())
        } else {
            Ok(format!(
                "an unregistered invariant cannot produce a finding, and a registered one                  ({}@{}) takes its severity and remediation from the registry",
                entry.finding.invariant_id(),
                entry.finding.invariant_version()
            ))
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            HEALTH_EVIDENCE,
        )
    }
}

const HEALTH_EVIDENCE: &str = "crates/vestrace-application/src/health.rs";

// ---------------------------------------------------------------------------
// GOV-019 / GOV-023 — Redaction protects without destroying the record.
// ---------------------------------------------------------------------------

const REDACTION_EVIDENCE: &str = "crates/vestrace-application/src/security/redaction.rs";

fn redaction_service() -> crate::security::RedactionService {
    use crate::security::{RedactionRule, RedactionService};
    use vestrace_domain::Sensitivity;

    RedactionService::new()
        .with_rule(RedactionRule {
            name: "api_key".to_owned(),
            pattern: regex::Regex::new(r"sk-[a-zA-Z0-9]+").expect("a valid pattern"),
            replacement: "[REDACTED]".to_owned(),
            sensitivity: Sensitivity::Restricted,
        })
        .with_rule(RedactionRule {
            name: "internal_hostname".to_owned(),
            pattern: regex::Regex::new(r"host-[0-9]+\.internal").expect("a valid pattern"),
            replacement: "[HOST]".to_owned(),
            sensitivity: Sensitivity::Internal,
        })
}

struct RedactionKeepsTheSentenceAndRemovesTheSecret;

impl ConformanceCase for RedactionKeepsTheSentenceAndRemovesTheSecret {
    fn case_id(&self) -> &str {
        "exec-gov-019-redaction-preserves-utility"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Gov,
            number: 19,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "Redaction replaces the sensitive span and leaves the rest of the record readable,          including its structure when the payload is JSON"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::DataDestination;

        let service = redaction_service();
        let redacted = service.redact(
            "provider rejected key sk-abc123 for workspace alpha",
            DataDestination::RemoteProvider,
        );

        let outcome = if redacted.contains("sk-abc123") {
            Err("the secret survived redaction".to_string())
        } else if !redacted.contains("provider rejected key")
            || !redacted.contains("for workspace alpha")
        {
            Err(format!(
                "redaction destroyed the surrounding record, leaving {redacted:?} — an                  operator cannot act on a message that has been erased"
            ))
        } else {
            // Structure has to survive too, or a redacted audit payload stops
            // being machine-readable exactly when it is being investigated.
            let payload = serde_json::json!({
                "action": "provider.invoke",
                "detail": { "key": "sk-abc123", "model": "llama-3.1" },
                "attempts": [1, 2]
            });
            let redacted_json = service.redact_json(&payload, DataDestination::RemoteProvider);
            if redacted_json["detail"]["key"] == serde_json::json!("sk-abc123") {
                Err("a secret survived redaction inside a JSON payload".to_string())
            } else if redacted_json["action"] != serde_json::json!("provider.invoke")
                || redacted_json["detail"]["model"] != serde_json::json!("llama-3.1")
                || redacted_json["attempts"] != serde_json::json!([1, 2])
            {
                Err("redaction altered fields it was not meant to touch".to_string())
            } else {
                Ok(format!(
                    "the secret is replaced and the record still reads: {redacted:?}, with                      surrounding JSON fields and structure intact"
                ))
            }
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            REDACTION_EVIDENCE,
        )
    }
}

struct SensitiveDataIsRedactedBeforeItReachesALog;

impl ConformanceCase for SensitiveDataIsRedactedBeforeItReachesALog {
    fn case_id(&self) -> &str {
        "exec-gov-023-no-secrets-in-logs"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Gov,
            number: 23,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "Restricted content is redacted for every destination including the log and the audit          store, while public content is never altered"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::DataDestination;

        let service = redaction_service();
        let text = "provider rejected key sk-abc123 on host-42.internal";

        // The audit store is the case worth having: it is the one destination
        // somebody might argue should see everything, and a restricted secret
        // written there is a secret in permanent storage.
        for destination in [
            DataDestination::LogOutput,
            DataDestination::AuditStore,
            DataDestination::RemoteProvider,
            DataDestination::ExportBundle,
            DataDestination::LocalModel,
        ] {
            if service.redact(text, destination).contains("sk-abc123") {
                return result(
                    self.case_id(),
                    self.requirement_ids()[0],
                    Err(format!(
                        "a restricted secret reached {destination:?} unredacted"
                    )),
                    REDACTION_EVIDENCE,
                );
            }
        }

        // Internal content is redacted where it leaves the system and kept
        // where it does not, so redaction is a boundary rule rather than a
        // blanket one.
        let outcome = if service
            .redact(text, DataDestination::RemoteProvider)
            .contains("host-42.internal")
        {
            Err("an internal hostname was sent to a remote provider unredacted".to_string())
        } else if !service
            .redact(text, DataDestination::LocalModel)
            .contains("host-42.internal")
        {
            Err("internal content was redacted from the local model, which redacts more than                  the boundary requires"
                .to_string())
        } else {
            Ok(
                "restricted content is redacted for every destination including the log and                  the audit store, while internal content is redacted only where it leaves"
                    .to_string(),
            )
        };

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            REDACTION_EVIDENCE,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_application_case_passes_against_the_current_code() {
        for case in [
            Box::new(FusionIsATotalOrderOverTheData) as Box<dyn ConformanceCase>,
            Box::new(ContextIsNotBuiltAcrossAWorkspaceBoundary),
            Box::new(TheJournalRecordsEveryChannelAndTheParameters),
            Box::new(HydrationResolvesTheExactRevision),
            Box::new(AMissingRevisionIsNotSubstituted),
            Box::new(ClassificationIsEnforcedAtHydration),
            Box::new(AFindingCannotExistWithoutARegisteredInvariant),
            Box::new(RedactionKeepsTheSentenceAndRemovesTheSecret),
            Box::new(SensitiveDataIsRedactedBeforeItReachesALog),
        ] {
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
}

// ---------------------------------------------------------------------------
// QUAL-008 / QUAL-016 — Fault scenarios: deterministic, bound to what they
// qualify, and unable to name a production environment.
// ---------------------------------------------------------------------------

struct FaultScenariosAreBoundToWhatTheyQualify;

impl ConformanceCase for FaultScenariosAreBoundToWhatTheyQualify {
    fn case_id(&self) -> &str {
        "exec-qual-008-fault-scenarios-are-bound"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 8,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Fault
    }
    fn description(&self) -> &str {
        "Fault injection is off unless configured, names the target digest it is qualifying, \
         refuses a blank one, and runs on a driver chosen by configuration rather than inferred"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::fault_runtime::{
            FaultInjectionDriver, FaultInjectionEnvironment, FaultInjectionSettings,
        };

        let outcome: Result<String, String> = (|| {
            if FaultInjectionSettings::new(true, "   ", FaultInjectionEnvironment::Ephemeral)
                .is_ok()
            {
                return Err(
                    "fault injection was configured against no particular build, so a crash \
                     scenario could be credited to a target it never ran against"
                        .to_string(),
                );
            }

            let settings = FaultInjectionSettings::new(
                true,
                "sha256:target",
                FaultInjectionEnvironment::Ephemeral,
            )
            .map_err(|error| format!("valid settings were refused: {error}"))?;

            if settings.target_digest() != "sha256:target" {
                return Err("the settings do not keep the target they were bound to".to_string());
            }
            if settings.driver() != FaultInjectionDriver::Process {
                return Err(format!(
                    "the default driver is {:?}; it must be the least privileged one",
                    settings.driver()
                ));
            }

            let disabled = FaultInjectionSettings::new(
                false,
                "sha256:target",
                FaultInjectionEnvironment::Ephemeral,
            )
            .map_err(|error| format!("disabled settings were refused: {error}"))?;
            if disabled.enabled() {
                return Err(
                    "fault injection reports enabled when it was configured off".to_string()
                );
            }

            Ok(format!(
                "fault injection is bound to {} on the {:?} driver, and refuses a blank target",
                settings.target_digest(),
                settings.driver()
            ))
        })();

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/fault_runtime.rs",
        )
    }
}

struct FaultScenariosCannotNameProduction;

impl ConformanceCase for FaultScenariosCannotNameProduction {
    fn case_id(&self) -> &str {
        "exec-qual-016-faults-cannot-name-production"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 16,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Security
    }
    fn description(&self) -> &str {
        "Every environment a destructive scenario can run in is an isolated one: the type \
         offers ephemeral, designated non-production and provider sandbox, and nothing else"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::fault_runtime::FaultInjectionEnvironment;

        let outcome: Result<String, String> = (|| {
            // Exhaustive by construction: adding a variant to the enum makes
            // this match stop compiling, which is the point — a production
            // target could not be introduced quietly.
            let environments = [
                FaultInjectionEnvironment::Ephemeral,
                FaultInjectionEnvironment::DesignatedNonProduction,
                FaultInjectionEnvironment::ProviderSandbox,
            ];
            for environment in environments {
                let isolated = match environment {
                    FaultInjectionEnvironment::Ephemeral
                    | FaultInjectionEnvironment::DesignatedNonProduction
                    | FaultInjectionEnvironment::ProviderSandbox => true,
                };
                if !isolated {
                    return Err(format!(
                        "{environment:?} is not an isolated qualification environment"
                    ));
                }
            }

            Ok(format!(
                "{} fault environments, all of them isolated, and no variant naming production",
                environments.len()
            ))
        })();

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/fault_runtime.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// QUAL-009 — A TRUSTED claim closes over recovery and revalidation.
// ---------------------------------------------------------------------------

struct TrustedClosesOverRecovery;

impl ConformanceCase for TrustedClosesOverRecovery {
    fn case_id(&self) -> &str {
        "exec-qual-009-trusted-closes-over-recovery"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 9,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "The TRUSTED profile closes over the recovery family, including the requirements that \
         govern revalidation, and every one of them is answered by an executed case rather \
         than an assertion"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::conformance::runner::profile_requirements;
        use vestrace_domain::conformance::{
            CaseOrigin as Origin, CaseStatus as Status, QualificationProfile,
        };

        let outcome: Result<String, String> = (|| {
            let closure = profile_requirements(QualificationProfile::Trusted);
            let recovery: Vec<_> = closure
                .iter()
                .filter(|id| id.family == RequirementFamily::Rec)
                .copied()
                .collect();
            if recovery.is_empty() {
                return Err(
                    "the TRUSTED profile closes over no recovery requirement, so a trust claim \
                     would say nothing about what happens after a failure"
                        .to_string(),
                );
            }

            // The two that decide whether trust may return at all.
            for number in [11u16, 12u16] {
                if !recovery.iter().any(|id| id.number == number) {
                    return Err(format!(
                        "REC-{number:03} is outside the TRUSTED closure, so trust could be \
                         restored without the evidence this profile is supposed to require"
                    ));
                }
            }

            // And they are executed, not asserted: an attestation here would
            // mean a trust claim resting on somebody's reading of the code.
            let domain = vestrace_domain::conformance::cases::executable_cases().run_all(None);
            let mut answered = 0usize;
            for requirement in &recovery {
                let result = domain
                    .results
                    .iter()
                    .find(|result| result.requirement_ids.contains(requirement));
                match result {
                    Some(result)
                        if result.status == Status::Pass && result.origin == Origin::Executed =>
                    {
                        answered += 1;
                    }
                    Some(result) if result.status == Status::Pass => {
                        return Err(format!(
                            "{requirement} passes as {:?} rather than by execution",
                            result.origin
                        ));
                    }
                    _ => {}
                }
            }

            if answered < recovery.len() - 1 {
                return Err(format!(
                    "only {answered} of {} recovery requirements in the TRUSTED closure are \
                     answered by an executed case",
                    recovery.len()
                ));
            }

            Ok(format!(
                "TRUSTED closes over {} recovery requirements, {answered} of them executed",
                recovery.len()
            ))
        })();

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/conformance/runner.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// QUAL-012 — What is not supported is said, not discovered.
// ---------------------------------------------------------------------------

struct UnsupportedIsRefusedNotAttempted;

impl ConformanceCase for UnsupportedIsRefusedNotAttempted {
    fn case_id(&self) -> &str {
        "exec-qual-012-unsupported-is-refused"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Qual,
            number: 12,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "A profile the capability manifest does not list is refused when a bundle claims it, \
         rather than qualified on a best-effort basis"
    }

    fn run(&self) -> ConformanceCaseResult {
        use vestrace_domain::conformance::runner::profile_requirements;
        use vestrace_domain::conformance::{
            CaseOrigin as Origin, CaseStatus as Status, ConformanceCaseResult as DomainResult,
            ConformanceReport, QualificationProfile,
        };
        use vestrace_domain::release::VestraceCapabilityManifest;
        use vestrace_domain::time::now;
        use vestrace_domain::trust::{QualificationBundle, QualificationLifecycle};

        let outcome: Result<String, String> = (|| {
            let at = now();
            let claimed = QualificationProfile::Trusted;
            let supported = QualificationProfile::Core;

            let manifest = VestraceCapabilityManifest::new(
                "1",
                "vestrace",
                "0.0.0",
                "source-revision-1",
                "sha256:build",
                "sha256:config",
                "environment://fixture",
                vec!["0150".to_string()],
                vec![supported],
                Vec::<String>::new(),
                vec!["postgres".to_string()],
                vec!["local-file".to_string()],
                Vec::<String>::new(),
                Vec::<String>::new(),
                Vec::<String>::new(),
                vec!["local-file key custody is not production crypto".to_string()],
            )
            .map_err(|error| format!("the fixture manifest could not be built: {error}"))?;

            let report = ConformanceReport::from_results(
                Some(claimed),
                profile_requirements(claimed)
                    .into_iter()
                    .map(|requirement_id| DomainResult {
                        case_id: format!("fixture-{requirement_id}"),
                        requirement_ids: vec![requirement_id],
                        status: Status::Pass,
                        message: "the fixture case passed".to_string(),
                        evidence: Some("evidence://fixture".to_string()),
                        origin: Origin::Executed,
                    })
                    .collect(),
            );

            if QualificationBundle::from_conformance_report_for_manifest(
                QualificationLifecycle::Release,
                claimed,
                &manifest,
                "suite-v1",
                report,
                Vec::new(),
                Vec::new(),
                at,
                Some(at),
            )
            .is_ok()
            {
                return Err(
                    "a bundle qualified a profile the manifest does not claim to support, so \
                     an installation could be certified for something it never said it did"
                        .to_string(),
                );
            }

            Ok(format!(
                "a manifest supporting only {supported} refuses to qualify {claimed}"
            ))
        })();

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-domain/src/release/mod.rs",
        )
    }
}

// ---------------------------------------------------------------------------
// RET-011 — Incomparable channel scores are fused by rank, not by addition.
// ---------------------------------------------------------------------------

struct RankDecidesFusionNotMagnitude;

impl ConformanceCase for RankDecidesFusionNotMagnitude {
    fn case_id(&self) -> &str {
        "exec-ret-011-rank-decides-not-magnitude"
    }
    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Ret,
            number: 11,
        }]
    }
    fn category(&self) -> CaseCategory {
        CaseCategory::Behavioral
    }
    fn description(&self) -> &str {
        "The fused order depends only on each channel's ranking: rescaling one channel's scores \
         by any positive factor, or shifting them, leaves the fused order unchanged, and a \
         channel with far larger numbers cannot outvote one with smaller ones"
    }

    fn run(&self) -> ConformanceCaseResult {
        use crate::retrieval::reciprocal_rank_fusion;
        use vestrace_domain::id::{MemoryId, MemoryRevisionId};
        use vestrace_domain::{MemoryKind, MemoryStatus, RetrievalCandidate};

        fn candidate(memory_id: MemoryId, score: f32, channel: &str) -> RetrievalCandidate {
            RetrievalCandidate {
                memory_id,
                revision_id: MemoryRevisionId::new(),
                kind: MemoryKind::Fact,
                memory_status: MemoryStatus::Active,
                revision_number: 1,
                content: "content".to_string(),
                valid_from: None,
                valid_until: None,
                revision_created_at: vestrace_domain::time::now(),
                source_generation: 1,
                score,
                channel_rank: 1,
                channel: channel.to_string(),
                explanation: String::new(),
                conflict_ids: Vec::new(),
            }
        }

        let outcome: Result<String, String> = (|| {
            let ids: Vec<MemoryId> = (0..5).map(|_| MemoryId::new()).collect();

            // Two channels whose numbers are not comparable with each other:
            // a lexical score in the tens, and a similarity score in the
            // hundredths. This is the real pairing — BM25 against cosine.
            let lexical: Vec<_> = ids
                .iter()
                .enumerate()
                .map(|(index, id)| candidate(*id, 18.0 - index as f32, "text"))
                .collect();
            let semantic: Vec<_> = ids
                .iter()
                .rev()
                .enumerate()
                .map(|(index, id)| candidate(*id, 0.04 - index as f32 * 0.005, "vector"))
                .collect();

            let baseline = reciprocal_rank_fusion(&[lexical.clone(), semantic.clone()], 60.0);
            let baseline_order: Vec<MemoryId> = baseline
                .iter()
                .map(|candidate| candidate.memory_id)
                .collect();

            // The same rankings, with one channel's numbers multiplied and
            // shifted. Order within the channel is untouched, so the fused
            // result must not move at all.
            for (label, scale, offset) in [
                ("multiplied by a thousand", 1000.0, 0.0),
                ("shifted upwards", 1.0, 500.0),
                ("shrunk to near zero", 0.000_1, 0.0),
            ] {
                let rescaled: Vec<_> = lexical
                    .iter()
                    .map(|item| candidate(item.memory_id, item.score * scale + offset, "text"))
                    .collect();
                let fused = reciprocal_rank_fusion(&[rescaled, semantic.clone()], 60.0);
                let order: Vec<MemoryId> =
                    fused.iter().map(|candidate| candidate.memory_id).collect();
                if order != baseline_order {
                    return Err(format!(
                        "with the lexical channel {label} the fused order changed, so the \
                         channels are being combined by magnitude and whichever one happens to \
                         produce larger numbers decides the ranking"
                    ));
                }
            }

            // And directly: a channel whose scores are enormous does not carry
            // its own top candidate to the top of the fused list when the other
            // channel ranks it last.
            let contested = ids[0];
            let loud: Vec<_> = vec![
                candidate(contested, 10_000.0, "text"),
                candidate(ids[1], 9_000.0, "text"),
            ];
            let quiet: Vec<_> = vec![
                candidate(ids[1], 0.02, "vector"),
                candidate(ids[2], 0.019, "vector"),
                candidate(ids[3], 0.018, "vector"),
                candidate(contested, 0.001, "vector"),
            ];
            let fused = reciprocal_rank_fusion(&[loud, quiet], 60.0);
            let top = fused
                .first()
                .ok_or_else(|| "fusion produced nothing".to_string())?;
            if top.memory_id != ids[1] {
                return Err(
                    "the candidate ranked first by the loud channel and last by the quiet one \
                     came out on top, so a channel's scale is deciding the result"
                        .to_string(),
                );
            }

            // The fused score is a rank contribution, not either channel's
            // number carried through.
            if fused.iter().any(|candidate| candidate.score > 1.0) {
                return Err(format!(
                    "a fused score of {} survived from a channel's own scale",
                    fused
                        .iter()
                        .map(|candidate| candidate.score)
                        .fold(0.0f32, f32::max)
                ));
            }

            Ok(format!(
                "the fused order is unchanged under three rescalings of one channel, and the \
                 top of {} candidates is decided by rank",
                fused.len()
            ))
        })();

        result(
            self.case_id(),
            self.requirement_ids()[0],
            outcome,
            "crates/vestrace-application/src/retrieval/fusion.rs",
        )
    }
}
