//! How memory-graph values are spelled in the database.
//!
//! Extracted so the atomic `save_new_memory` and the single-row writers cannot
//! drift apart: two copies of a `match` that maps an enum to a string are two
//! places for a new variant to be forgotten, and the failure would be a row
//! written under the wrong label rather than a compile error.

use vestrace_domain::{EvidenceRole, MemoryKind, MemoryStatus};

pub(crate) fn status_str(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Active => "active",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Expired => "expired",
        MemoryStatus::Deleted => "deleted",
    }
}

pub(crate) fn kind_str(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Fact => "fact",
        MemoryKind::Preference => "preference",
        MemoryKind::Constraint => "constraint",
        MemoryKind::Decision => "decision",
        MemoryKind::Task => "task",
        MemoryKind::Procedure => "procedure",
        MemoryKind::Observation => "observation",
        MemoryKind::Outcome => "outcome",
        MemoryKind::Summary => "summary",
    }
}

pub(crate) fn evidence_role_str(role: EvidenceRole) -> &'static str {
    match role {
        EvidenceRole::DirectSource | EvidenceRole::Primary => "primary",
        EvidenceRole::SupportingContext | EvidenceRole::Supporting => "supporting",
        EvidenceRole::ContradictingEvidence | EvidenceRole::Contradicting => "contradicting",
        EvidenceRole::Contextual => "contextual",
        EvidenceRole::DerivedFrom => "derived_from",
    }
}
