mod assessment;
#[allow(clippy::module_inception)]
mod claim;
mod conflict;
mod evidence;
mod mutation;
mod revalidation;
mod supersession;

pub use assessment::{AssessmentKind, ClaimAssessment};
pub use claim::{Claim, ClaimStatus};
pub use conflict::{Conflict, ConflictKind, ConflictStatus};
pub use evidence::{ClaimEvidenceLink, SourceClassification};
pub use mutation::{
    CognitiveMutation, MutationKind, MutationTargetKind, ReconciliationClass,
    ReconciliationOutcome, ReconciliationRecord,
};
pub use revalidation::{RevalidationImpact, claims_losing_evidence, revalidate_after_deletion};
pub use supersession::{SupersessionLink, SupersessionTargetKind};
