//! Resolving a revision reference into the content it names.
//!
//! # Why this is a step and not a field
//!
//! A retrieval candidate carries a `revision_id`. Content reached the caller
//! because the retriever happened to select it alongside the reference, which
//! means the two could differ without anything noticing: nothing checked that
//! the text handed to a reader was the text of the revision the reference
//! named. Hydration makes that a step with a stated contract — resolve *this*
//! revision, or say you could not.
//!
//! # Withholding is not filtering
//!
//! When a revision may not be disclosed, its identity and the reason are
//! returned; the content is not. A boundary that silently drops rows produces a
//! smaller answer that looks complete, and a reader cannot distinguish "there
//! was nothing" from "there was something you may not see" — which is the
//! difference between an answer and a misleading one.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    DomainError, MemoryStatus,
    id::{MemoryId, MemoryRevisionId},
    time::Timestamp,
};

/// A reference to one exact revision of one memory.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RevisionRef {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
}

/// Which classifications a caller may receive.
///
/// There is no "allow everything" variant that a deployment can drift into by
/// accident. [`ClassificationPolicy::permissive`] exists, is named for what it
/// is, and has to be chosen.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClassificationPolicy {
    admissible: BTreeSet<String>,
    /// Whether content carrying no classification at all may be disclosed.
    ///
    /// Separate from the label set because "unclassified" is not a label: a
    /// revision with `NULL` classification has not been assessed, which is not
    /// the same as having been assessed as public.
    allow_unclassified: bool,
}

impl ClassificationPolicy {
    pub fn new(
        admissible: impl IntoIterator<Item = impl Into<String>>,
        allow_unclassified: bool,
    ) -> Result<Self, DomainError> {
        let admissible: BTreeSet<String> = admissible
            .into_iter()
            .map(|label| label.into().trim().to_string())
            .collect();
        if admissible.iter().any(|label| label.is_empty()) {
            return Err(DomainError::InvalidArgument(
                "a classification label must not be blank".into(),
            ));
        }
        Ok(Self {
            admissible,
            allow_unclassified,
        })
    }

    /// Admits everything, including unassessed content.
    ///
    /// For development and for workspaces that have not classified anything.
    /// Named rather than implied so that a deployment running without a
    /// classification boundary is doing so on purpose and can be found by
    /// searching for this call.
    pub fn permissive() -> Self {
        Self {
            admissible: BTreeSet::new(),
            allow_unclassified: true,
        }
    }

    /// Whether this policy admits everything it is shown.
    pub fn is_permissive(&self) -> bool {
        self.admissible.is_empty() && self.allow_unclassified
    }

    pub fn admits(&self, classification: Option<&str>) -> bool {
        match classification.map(str::trim) {
            None | Some("") => self.allow_unclassified,
            Some(label) => {
                // A permissive policy names no labels and admits all of them;
                // a policy that names some admits only those.
                self.is_permissive() || self.admissible.contains(label)
            }
        }
    }

    pub fn admissible_labels(&self) -> impl Iterator<Item = &str> {
        self.admissible.iter().map(String::as_str)
    }
}

/// A revision resolved from its reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HydratedRevision {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
    pub revision_number: u32,
    pub memory_status: MemoryStatus,
    pub content: String,
    pub classification: Option<String>,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub created_at: Timestamp,
}

/// Why a referenced revision did not come back.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum WithholdingReason {
    /// The reference names nothing in this workspace.
    ///
    /// Reported rather than resolved to the memory's current revision: quietly
    /// substituting the latest is how a caller asking about the past is served
    /// the present.
    RevisionNotFound,
    /// The classification is outside what this caller may receive.
    ClassificationNotAdmissible { classification: String },
}

/// A reference that was not hydrated, and why. Carries no content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WithheldRevision {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
    #[serde(flatten)]
    pub reason: WithholdingReason,
}

/// The result of hydrating a set of references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HydrationOutcome {
    pub revisions: Vec<HydratedRevision>,
    pub withheld: Vec<WithheldRevision>,
}

impl HydrationOutcome {
    /// Apply a classification policy to already-resolved revisions.
    ///
    /// Kept in the domain rather than the adapter so the rule is one thing in
    /// one place, and so a future adapter cannot quietly implement a laxer
    /// version of it.
    pub fn apply_policy(
        resolved: Vec<HydratedRevision>,
        requested: &[RevisionRef],
        policy: &ClassificationPolicy,
    ) -> Self {
        let mut revisions = Vec::new();
        let mut withheld = Vec::new();

        for revision in resolved {
            if policy.admits(revision.classification.as_deref()) {
                revisions.push(revision);
            } else {
                withheld.push(WithheldRevision {
                    memory_id: revision.memory_id,
                    revision_id: revision.revision_id,
                    reason: WithholdingReason::ClassificationNotAdmissible {
                        // The label, not the content. Knowing that something
                        // was withheld as "restricted" is what makes the gap
                        // actionable; the text is what must not travel.
                        classification: revision
                            .classification
                            .unwrap_or_else(|| "unclassified".to_string()),
                    },
                });
            }
        }

        // Anything asked for and not resolved is reported as missing rather
        // than left out of the answer.
        for reference in requested {
            let present = revisions
                .iter()
                .any(|revision| revision.revision_id == reference.revision_id)
                || withheld
                    .iter()
                    .any(|entry| entry.revision_id == reference.revision_id);
            if !present {
                withheld.push(WithheldRevision {
                    memory_id: reference.memory_id,
                    revision_id: reference.revision_id,
                    reason: WithholdingReason::RevisionNotFound,
                });
            }
        }

        Self {
            revisions,
            withheld,
        }
    }

    /// Whether anything asked for did not come back.
    pub fn is_complete(&self) -> bool {
        self.withheld.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::now;

    fn revision(classification: Option<&str>) -> HydratedRevision {
        HydratedRevision {
            memory_id: MemoryId::new(),
            revision_id: MemoryRevisionId::new(),
            revision_number: 2,
            memory_status: MemoryStatus::Active,
            content: "content".to_string(),
            classification: classification.map(str::to_string),
            valid_from: None,
            valid_until: None,
            created_at: now(),
        }
    }

    #[test]
    fn a_named_policy_admits_only_what_it_names() {
        let policy = ClassificationPolicy::new(["internal"], false).unwrap();
        assert!(policy.admits(Some("internal")));
        assert!(!policy.admits(Some("restricted")));
        // Unassessed content is not public content.
        assert!(!policy.admits(None));
    }

    #[test]
    fn unclassified_is_governed_separately_from_the_label_set() {
        let strict = ClassificationPolicy::new(["internal"], false).unwrap();
        let lenient = ClassificationPolicy::new(["internal"], true).unwrap();
        assert!(!strict.admits(None));
        assert!(lenient.admits(None));
        // Neither admits a label outside the set, whatever the unclassified
        // setting is.
        assert!(!strict.admits(Some("secret")));
        assert!(!lenient.admits(Some("secret")));
    }

    #[test]
    fn an_empty_classification_string_counts_as_unassessed() {
        // A column that is `''` rather than NULL must not become a label that
        // no policy names and everything therefore rejects — or worse, one that
        // a permissive policy silently admits under a different rule.
        let strict = ClassificationPolicy::new(["internal"], false).unwrap();
        assert!(!strict.admits(Some("   ")));
        let lenient = ClassificationPolicy::new(["internal"], true).unwrap();
        assert!(lenient.admits(Some("   ")));
    }

    #[test]
    fn a_permissive_policy_is_recognisable_as_one() {
        let policy = ClassificationPolicy::permissive();
        assert!(policy.is_permissive());
        assert!(policy.admits(Some("anything")));
        assert!(policy.admits(None));
        assert!(
            !ClassificationPolicy::new(["internal"], true)
                .unwrap()
                .is_permissive()
        );
    }

    #[test]
    fn a_blank_label_is_refused() {
        assert!(ClassificationPolicy::new(["  "], true).is_err());
    }

    #[test]
    fn withheld_content_is_named_but_not_disclosed() {
        let policy = ClassificationPolicy::new(["internal"], true).unwrap();
        let secret = revision(Some("restricted"));
        let secret_id = secret.revision_id;
        let requested = vec![RevisionRef {
            memory_id: secret.memory_id,
            revision_id: secret_id,
        }];

        let outcome = HydrationOutcome::apply_policy(vec![secret], &requested, &policy);

        assert!(outcome.revisions.is_empty());
        assert_eq!(outcome.withheld.len(), 1);
        assert!(!outcome.is_complete());

        let rendered = serde_json::to_string(&outcome.withheld).unwrap();
        assert!(
            !rendered.contains("content"),
            "the withheld entry disclosed the content it was withholding"
        );
        assert!(rendered.contains("restricted"));
    }

    #[test]
    fn a_reference_that_resolves_to_nothing_is_reported_not_substituted() {
        // The failure this guards against: asking for revision 2 and being
        // handed revision 7 because 2 was gone.
        let policy = ClassificationPolicy::permissive();
        let missing = RevisionRef {
            memory_id: MemoryId::new(),
            revision_id: MemoryRevisionId::new(),
        };

        let outcome = HydrationOutcome::apply_policy(Vec::new(), &[missing], &policy);

        assert!(outcome.revisions.is_empty());
        assert_eq!(
            outcome.withheld,
            vec![WithheldRevision {
                memory_id: missing.memory_id,
                revision_id: missing.revision_id,
                reason: WithholdingReason::RevisionNotFound,
            }]
        );
    }

    #[test]
    fn an_admitted_revision_comes_back_whole() {
        let policy = ClassificationPolicy::new(["internal"], true).unwrap();
        let allowed = revision(Some("internal"));
        let expected = allowed.clone();
        let requested = vec![RevisionRef {
            memory_id: allowed.memory_id,
            revision_id: allowed.revision_id,
        }];

        let outcome = HydrationOutcome::apply_policy(vec![allowed], &requested, &policy);

        assert_eq!(outcome.revisions, vec![expected]);
        assert!(outcome.is_complete());
    }
}
