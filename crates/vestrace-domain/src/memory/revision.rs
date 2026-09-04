use std::collections::BTreeSet;

use crate::{
    Confidence, DomainError, Importance, MemoryKind, MemoryStatus, StructuredMemory,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
    time::Timestamp,
};

/// The deployment-declared labels callers may state when a memory is created.
///
/// This is a set, not a scale. Its iteration order is deterministic only so
/// configuration fingerprints and refusal messages are stable; no label is
/// above or below another one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryLabelVocabulary {
    labels: BTreeSet<String>,
}

impl MemoryLabelVocabulary {
    pub fn new(labels: impl IntoIterator<Item = impl Into<String>>) -> Result<Self, DomainError> {
        let mut declared = BTreeSet::new();
        for label in labels {
            let label = label.into().trim().to_owned();
            if label.is_empty() {
                return Err(DomainError::InvalidArgument(
                    "policy.data.memory_labels contains a blank classification label".into(),
                ));
            }
            if !declared.insert(label.clone()) {
                return Err(DomainError::InvalidArgument(format!(
                    "policy.data.memory_labels contains duplicate classification label '{label}'"
                )));
            }
        }
        Ok(Self { labels: declared })
    }

    pub fn validate_creation(
        &self,
        classification: Option<&str>,
    ) -> Result<Option<String>, DomainError> {
        let Some(classification) = classification else {
            return Ok(None);
        };
        let classification = classification.trim();
        if classification.is_empty() {
            return Err(DomainError::InvalidArgument(
                "memory_revisions.classification must be non-blank after trimming".into(),
            ));
        }
        if !self.labels.contains(classification) {
            return Err(DomainError::PolicyViolation(format!(
                "memory_revisions.classification label '{classification}' is not declared by \
                 policy.data.memory_labels; configured labels: {:?}",
                self.labels.iter().collect::<Vec<_>>()
            )));
        }
        Ok(Some(classification.to_owned()))
    }

    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter().map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MemoryRevision {
    pub id: MemoryRevisionId,
    pub memory_id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub content: String,
    pub structured: Option<StructuredMemory>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub created_at: Timestamp,
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
    pub change_reason: Option<String>,
    pub canonical_hash: Option<String>,
    pub classification: Option<String>,
}

impl MemoryRevision {
    /// Validate constraints that are intrinsic to a stored label rather than
    /// deployment policy. Vocabulary membership is checked at creation; this
    /// keeps every non-null stored value distinguishable from unassessed data.
    pub fn validate_classification_shape(classification: Option<&str>) -> Result<(), DomainError> {
        let Some(classification) = classification else {
            return Ok(());
        };
        if classification.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "memory_revisions.classification must be non-blank".into(),
            ));
        }
        if classification.trim() != classification {
            return Err(DomainError::InvalidArgument(
                "memory_revisions.classification must be trimmed".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_temporal_range(&self) -> Result<(), DomainError> {
        if let (Some(from), Some(until)) = (self.valid_from, self.valid_until) {
            if until < from {
                return Err(DomainError::InvalidArgument(format!(
                    "valid_until ({}) must not precede valid_from ({})",
                    until, from
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod classification_vocabulary_tests {
    use super::{MemoryLabelVocabulary, MemoryRevision};

    #[test]
    fn labels_are_trimmed_and_exposed_as_an_unordered_set() {
        let vocabulary = MemoryLabelVocabulary::new([" restricted ", "internal"]).unwrap();

        assert_eq!(
            vocabulary.labels().collect::<Vec<_>>(),
            vec!["internal", "restricted"]
        );
        assert_eq!(
            vocabulary.validate_creation(Some(" internal ")).unwrap(),
            Some("internal".to_owned())
        );
    }

    #[test]
    fn blank_and_duplicate_declared_labels_name_the_configuration_key() {
        for labels in [vec!["   ", "internal"], vec!["internal", " internal "]] {
            let message = MemoryLabelVocabulary::new(labels).unwrap_err().to_string();
            assert!(message.contains("policy.data.memory_labels"), "{message}");
        }
    }

    #[test]
    fn an_empty_vocabulary_allows_absence_and_refuses_any_stated_label() {
        let vocabulary = MemoryLabelVocabulary::new(Vec::<String>::new()).unwrap();

        assert_eq!(vocabulary.validate_creation(None).unwrap(), None);
        let message = vocabulary
            .validate_creation(Some("internal"))
            .unwrap_err()
            .to_string();
        assert!(message.contains("internal"), "{message}");
        assert!(message.contains("policy.data.memory_labels"), "{message}");
        assert!(message.contains("[]"), "{message}");
    }

    #[test]
    fn an_undeclared_label_names_the_configured_set() {
        let vocabulary = MemoryLabelVocabulary::new(["internal", "restricted"]).unwrap();

        let message = vocabulary
            .validate_creation(Some("confidential"))
            .unwrap_err()
            .to_string();
        assert!(message.contains("confidential"), "{message}");
        assert!(message.contains("internal"), "{message}");
        assert!(message.contains("restricted"), "{message}");
    }

    #[test]
    fn stored_classification_shape_is_non_blank_and_already_trimmed() {
        assert!(MemoryRevision::validate_classification_shape(None).is_ok());
        assert!(MemoryRevision::validate_classification_shape(Some("internal")).is_ok());

        for invalid in ["", "   ", " internal", "internal "] {
            let message = MemoryRevision::validate_classification_shape(Some(invalid))
                .unwrap_err()
                .to_string();
            assert!(
                message.contains("memory_revisions.classification"),
                "{message}"
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Memory {
    pub id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub active_revision_id: Option<MemoryRevisionId>,
    pub state_revision: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Memory {
    pub fn new(id: MemoryId, workspace_id: WorkspaceId, kind: MemoryKind, at: Timestamp) -> Self {
        Self {
            id,
            workspace_id,
            kind,
            status: MemoryStatus::Candidate,
            active_revision_id: None,
            state_revision: 0,
            created_at: at,
            updated_at: at,
        }
    }

    /// Make `revision` the active one.
    ///
    /// Takes the revision rather than its id so that ownership can be checked.
    /// The previous signature accepted a bare `MemoryRevisionId` and could not
    /// verify anything about it, so a memory could be pointed at a revision
    /// belonging to another memory — or another workspace — and the domain
    /// would accept it. Reading that memory would then return content that was
    /// never written to it.
    pub fn activate(
        mut self,
        revision: &MemoryRevision,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if revision.memory_id != self.id {
            return Err(DomainError::InvalidArgument(
                "active revision must belong to the same memory".into(),
            ));
        }
        if revision.workspace_id != self.workspace_id {
            return Err(DomainError::PolicyViolation(
                "active revision must belong to the same workspace".into(),
            ));
        }

        match self.status {
            MemoryStatus::Candidate | MemoryStatus::Active => {
                self.status = MemoryStatus::Active;
                self.active_revision_id = Some(revision.id);
                self.state_revision += 1;
                self.updated_at = at;
                Ok(self)
            }
            MemoryStatus::Superseded
            | MemoryStatus::Rejected
            | MemoryStatus::Expired
            | MemoryStatus::Deleted => Err(DomainError::PolicyViolation(format!(
                "cannot activate memory in status {:?}",
                self.status
            ))),
        }
    }

    pub fn reject(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Candidate {
            self.status = MemoryStatus::Rejected;
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot reject memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn supersede(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Active {
            self.status = MemoryStatus::Superseded;
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot supersede memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn expire(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Active {
            self.status = MemoryStatus::Expired;
            self.active_revision_id = None;
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        } else {
            Err(DomainError::PolicyViolation(format!(
                "cannot expire memory in status {:?}",
                self.status
            )))
        }
    }

    pub fn soft_delete(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if self.status == MemoryStatus::Deleted {
            Err(DomainError::PolicyViolation(
                "memory is already deleted".into(),
            ))
        } else {
            self.status = MemoryStatus::Deleted;
            self.active_revision_id = None;
            self.state_revision += 1;
            self.updated_at = at;
            Ok(self)
        }
    }
}
