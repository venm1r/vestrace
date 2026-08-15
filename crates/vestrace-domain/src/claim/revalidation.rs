//! What happens to a claim when the evidence under it is destroyed.
//!
//! # The gap this closes
//!
//! GOV-017 requires evidence deletion to trigger revalidation of the claims and
//! derived state that rested on it. Every part existed and none of them were
//! joined: `DeletionPlan` carries `dependency_refs` and `verify_deletion`
//! refuses a verification that did not cover every one, so the *scope* of a
//! deletion already reached derived state; `Claim` has a lifecycle with
//! `contest`, `supersede` and `expire`; `RevalidationRun` exists in the trust
//! module. But nothing took a completed deletion and reopened the claims whose
//! evidence it removed, and `ClaimEvidenceLink` had no traversal from an
//! evidence reference back to the claims that cite it.
//!
//! # Why only a complete verification triggers anything
//!
//! A `DeletionVerification` that is `Incomplete` found copies still standing,
//! and one that is `BlockedByHold` did not delete at all. Neither establishes
//! that the evidence is gone, so neither has any business changing what a claim
//! is believed to rest on. Acting on them would mean a claim contested because
//! somebody *started* a deletion.
//!
//! # Why contested rather than rejected
//!
//! Losing the evidence for a claim does not make the claim false. It makes it
//! unsupported, which is exactly what `Contested` means: the claim stands, its
//! basis does not, and somebody has to look. Rejecting it would be a judgement
//! nobody made, and leaving it `Supported` would be a record that says the
//! evidence is still there.

use std::collections::HashSet;

use crate::claim::{Claim, ClaimEvidenceLink, ClaimStatus};
use crate::id::ClaimId;
use crate::provenance::EvidenceRef;
use crate::time::Timestamp;
use crate::trust::DeletionVerification;

impl EvidenceRef {
    /// A canonical, comparable reference for this piece of evidence.
    ///
    /// A deletion records what it removed as opaque strings — it works over
    /// storage, not over the claim model — so the two can only be matched if a
    /// typed evidence reference has one agreed textual form. Without this,
    /// "the deletion removed the evidence this claim cites" is not a question
    /// anything can ask.
    pub fn reference(&self) -> String {
        match self {
            Self::EventRef { event_id } => format!("event://{event_id}"),
            Self::ArtifactRevisionRef {
                artifact_id,
                revision_id,
            } => format!("artifact://{artifact_id}/{revision_id}"),
            Self::DocumentRef { uri, version } => match version {
                Some(version) => format!("document://{uri}@{version}"),
                None => format!("document://{uri}"),
            },
            Self::MemoryRevisionRef {
                memory_id,
                revision_id,
            } => format!("memory://{memory_id}/{revision_id}"),
            Self::SharedMemoryRevisionRef {
                source_workspace_id,
                memory_id,
                revision_id,
                ..
            } => format!("shared-memory://{source_workspace_id}/{memory_id}/{revision_id}"),
            Self::ModelExecutionRef { attempt_id } => format!("model-execution://{attempt_id}"),
            Self::ToolResultRef { invocation_id } => format!("tool-invocation://{invocation_id}"),
            Self::EvaluationRef { evaluation_id } => format!("evaluation://{evaluation_id}"),
            Self::ExternalEffectReceiptRef { receipt_id, .. } => {
                format!("effect-receipt://{receipt_id}")
            }
            Self::HumanFeedbackRef { feedback_id, .. } => format!("feedback://{feedback_id}"),
            Self::FederatedEvidenceRef {
                federation_id,
                remote_evidence_id,
                remote_workspace_id,
            } => format!("federated://{federation_id}/{remote_workspace_id}/{remote_evidence_id}"),
            Self::ExternalReference { uri, .. } => format!("external://{uri}"),
        }
    }
}

/// The claims whose evidence a completed deletion removed.
///
/// Empty when the verification did not complete: nothing has been established,
/// so nothing follows.
pub fn claims_losing_evidence(
    verification: &DeletionVerification,
    links: &[ClaimEvidenceLink],
) -> HashSet<ClaimId> {

    if !verification.is_complete() {
        return HashSet::new();
    }
    let removed: HashSet<&str> = verification
        .checked_refs()
        .iter()
        .map(String::as_str)
        .collect();
    links
        .iter()
        .filter(|link| removed.contains(link.evidence_ref.reference().as_str()))
        .map(|link| link.claim_id)
        .collect()
}

/// What the deletion did to the claims that cited what it removed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RevalidationImpact {
    /// Claims moved to `Contested`: they stand, their basis does not.
    pub contested: Vec<Claim>,
    /// Claims that lost evidence and could not be contested, with the status
    /// they were in.
    ///
    /// A superseded or rejected claim has already been decided about, and
    /// re-opening it would undo somebody's judgement. Reported rather than
    /// silently skipped, because "we deleted the evidence for a claim nobody
    /// re-examined" is exactly the thing an auditor asks about.
    pub already_settled: Vec<(ClaimId, ClaimStatus)>,
}

/// Reopen the claims a completed deletion left without their evidence.
///
/// Claims not affected are returned unchanged, so a caller can hand in a
/// workspace's claims and store the result without deciding which is which.
pub fn revalidate_after_deletion(
    verification: &DeletionVerification,
    links: &[ClaimEvidenceLink],
    claims: Vec<Claim>,
    at: Timestamp,
) -> (Vec<Claim>, RevalidationImpact) {
    let affected = claims_losing_evidence(verification, links);
    let mut impact = RevalidationImpact::default();
    let mut result = Vec::with_capacity(claims.len());

    for claim in claims {
        if !affected.contains(&claim.claim_id) {
            result.push(claim);
            continue;
        }
        let status = claim.lifecycle_status;
        let id = claim.claim_id;
        match claim.clone().contest(at) {
            Ok(contested) => {
                impact.contested.push(contested.clone());
                result.push(contested);
            }
            Err(_) => {
                impact.already_settled.push((id, status));
                result.push(claim);
            }
        }
    }

    (result, impact)
}
