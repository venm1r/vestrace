use std::fmt;

use crate::models::ModelQualificationRevisionId;
use crate::{
    ConnectionRevisionId, CredentialActivationGuardId, CredentialRevisionId, CredentialSlotId,
    DomainError, NoAuthBindingRevisionId, WorkspaceId,
};

macro_rules! binding_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Eq,
            Hash,
            PartialEq,
            schemars::JsonSchema,
            serde::Deserialize,
            serde::Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }
            pub const fn from_uuid(value: uuid::Uuid) -> Self {
                Self(value)
            }
            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

binding_id!(ModelRevisionId);
binding_id!(ModelBindingSnapshotId);

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Chat,
    Embedding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelObservationSource {
    Provider,
    Operator,
}

#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
enum ModelObservationValue<T> {
    Unknown,
    Observed {
        value: T,
        qualification_revision_id: ModelQualificationRevisionId,
        source: ModelObservationSource,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ModelObservation<T>(ModelObservationValue<T>);

impl<T> ModelObservation<T> {
    pub fn unknown() -> Self {
        Self(ModelObservationValue::Unknown)
    }
    pub const fn is_unknown(&self) -> bool {
        matches!(self.0, ModelObservationValue::Unknown)
    }
    pub fn value(&self) -> Option<&T> {
        match &self.0 {
            ModelObservationValue::Unknown => None,
            ModelObservationValue::Observed { value, .. } => Some(value),
        }
    }

    pub fn qualification_revision_id(&self) -> Option<ModelQualificationRevisionId> {
        match &self.0 {
            ModelObservationValue::Unknown => None,
            ModelObservationValue::Observed {
                qualification_revision_id,
                ..
            } => Some(*qualification_revision_id),
        }
    }

    pub fn source(&self) -> Option<ModelObservationSource> {
        match &self.0 {
            ModelObservationValue::Unknown => None,
            ModelObservationValue::Observed { source, .. } => Some(*source),
        }
    }
}
impl ModelObservation<u32> {
    pub fn provider_observed(
        value: u32,
        qualification_revision_id: ModelQualificationRevisionId,
    ) -> Result<Self, DomainError> {
        Self::positive(
            value,
            qualification_revision_id,
            ModelObservationSource::Provider,
        )
    }
    pub fn operator_supplied(
        value: u32,
        qualification_revision_id: ModelQualificationRevisionId,
    ) -> Result<Self, DomainError> {
        Self::positive(
            value,
            qualification_revision_id,
            ModelObservationSource::Operator,
        )
    }
    fn positive(
        value: u32,
        qualification_revision_id: ModelQualificationRevisionId,
        source: ModelObservationSource,
    ) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidArgument(
                "observed model value must be positive".into(),
            ));
        }
        Ok(Self(ModelObservationValue::Observed {
            value,
            qualification_revision_id,
            source,
        }))
    }
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::ModelRevision>("{}");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ModelRevision {
    id: ModelRevisionId,
    workspace_id: WorkspaceId,
    connection_revision_id: ConnectionRevisionId,
    wire_model_id: String,
    kind: ModelKind,
    observed_context_window: ModelObservation<u32>,
    observed_embedding_dimension: ModelObservation<u32>,
}
impl ModelRevision {
    pub fn new(
        workspace_id: WorkspaceId,
        connection_revision_id: ConnectionRevisionId,
        wire_model_id: impl Into<String>,
        kind: ModelKind,
    ) -> Result<Self, DomainError> {
        let wire_model_id = wire_model_id.into();
        if wire_model_id.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "wire model id is required".into(),
            ));
        }
        Ok(Self {
            id: ModelRevisionId::new(),
            workspace_id,
            connection_revision_id,
            wire_model_id,
            kind,
            observed_context_window: ModelObservation::unknown(),
            observed_embedding_dimension: ModelObservation::unknown(),
        })
    }
    pub fn new_chat(
        workspace_id: WorkspaceId,
        connection_revision_id: ConnectionRevisionId,
        wire_model_id: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::new(
            workspace_id,
            connection_revision_id,
            wire_model_id,
            ModelKind::Chat,
        )
    }
    pub fn observed_context_window(&self) -> &ModelObservation<u32> {
        &self.observed_context_window
    }
    pub fn observed_embedding_dimension(&self) -> &ModelObservation<u32> {
        &self.observed_embedding_dimension
    }
    pub const fn id(&self) -> ModelRevisionId {
        self.id
    }
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    pub const fn connection_revision_id(&self) -> ConnectionRevisionId {
        self.connection_revision_id
    }
    pub fn wire_model_id(&self) -> &str {
        &self.wire_model_id
    }
    pub const fn kind(&self) -> ModelKind {
        self.kind
    }

    pub fn with_observations(
        mut self,
        observed_context_window: ModelObservation<u32>,
        observed_embedding_dimension: ModelObservation<u32>,
    ) -> Result<Self, DomainError> {
        self.observed_context_window = observed_context_window;
        self.observed_embedding_dimension = observed_embedding_dimension;
        Ok(self)
    }
    pub fn from_persisted(
        id: ModelRevisionId,
        workspace_id: WorkspaceId,
        connection_revision_id: ConnectionRevisionId,
        wire_model_id: impl Into<String>,
        kind: ModelKind,
        observed_context_window: ModelObservation<u32>,
        observed_embedding_dimension: ModelObservation<u32>,
    ) -> Result<Self, DomainError> {
        let mut result = Self::new(workspace_id, connection_revision_id, wire_model_id, kind)?;
        result.id = id;
        result.observed_context_window = observed_context_window;
        result.observed_embedding_dimension = observed_embedding_dimension;
        Ok(result)
    }
}

/// ```compile_fail
/// use vestrace_domain::{NoAuthBindingRevisionId, QualificationTargetBinding};
/// let _ = QualificationTargetBinding::NoAuth {
///     binding_revision_id: NoAuthBindingRevisionId::new(),
///     slot_id: unreachable!(),
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub enum QualificationTargetBinding {
    Credential {
        revision_id: CredentialRevisionId,
        slot_id: CredentialSlotId,
        activation_guard_id: CredentialActivationGuardId,
        expected_slot_version: u64,
    },
    NoAuth {
        binding_revision_id: NoAuthBindingRevisionId,
    },
}
impl QualificationTargetBinding {
    pub const fn is_no_auth(&self) -> bool {
        matches!(self, Self::NoAuth { .. })
    }
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::ModelBindingSnapshot>("{}");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ModelBindingSnapshot {
    id: ModelBindingSnapshotId,
    workspace_id: WorkspaceId,
    connection_revision_id: ConnectionRevisionId,
    connection_qualification_revision_id: crate::models::ConnectionQualificationRevisionId,
    model_revision_id: ModelRevisionId,
    model_qualification_revision_id: ModelQualificationRevisionId,
    qualification_target: QualificationTargetBinding,
}
impl ModelBindingSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_id: WorkspaceId,
        connection_revision_id: ConnectionRevisionId,
        connection_qualification_revision_id: crate::models::ConnectionQualificationRevisionId,
        model_revision_id: ModelRevisionId,
        model_qualification_revision_id: ModelQualificationRevisionId,
        qualification_target: QualificationTargetBinding,
    ) -> Self {
        Self {
            id: ModelBindingSnapshotId::new(),
            workspace_id,
            connection_revision_id,
            connection_qualification_revision_id,
            model_revision_id,
            model_qualification_revision_id,
            qualification_target,
        }
    }
    pub fn from_persisted(
        id: ModelBindingSnapshotId,
        workspace_id: WorkspaceId,
        connection_revision_id: ConnectionRevisionId,
        connection_qualification_revision_id: crate::models::ConnectionQualificationRevisionId,
        model_revision_id: ModelRevisionId,
        model_qualification_revision_id: ModelQualificationRevisionId,
        qualification_target: QualificationTargetBinding,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            workspace_id,
            connection_revision_id,
            connection_qualification_revision_id,
            model_revision_id,
            model_qualification_revision_id,
            qualification_target,
        })
    }
    pub fn qualification_target(&self) -> &QualificationTargetBinding {
        &self.qualification_target
    }

    pub const fn id(&self) -> ModelBindingSnapshotId {
        self.id
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn connection_revision_id(&self) -> ConnectionRevisionId {
        self.connection_revision_id
    }

    pub const fn connection_qualification_revision_id(
        &self,
    ) -> crate::models::ConnectionQualificationRevisionId {
        self.connection_qualification_revision_id
    }

    pub const fn model_revision_id(&self) -> ModelRevisionId {
        self.model_revision_id
    }

    pub const fn model_qualification_revision_id(&self) -> ModelQualificationRevisionId {
        self.model_qualification_revision_id
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_type_ne_all;

    use crate::{
        ConnectionQualificationRevisionId, ConnectionRevisionId, ModelBindingSnapshot,
        ModelQualificationRevisionId, ModelRevisionId, NoAuthBindingRevisionId,
        QualificationTargetBinding, WorkspaceId,
    };

    #[test]
    fn qualification_target_is_a_strict_xor() {
        let target = QualificationTargetBinding::NoAuth {
            binding_revision_id: NoAuthBindingRevisionId::new(),
        };

        assert!(target.is_no_auth());
    }

    #[test]
    fn model_binding_snapshot_has_one_auth_branch() {
        assert_type_ne_all!(QualificationTargetBinding, NoAuthBindingRevisionId);

        let snapshot = ModelBindingSnapshot::new(
            WorkspaceId::new(),
            ConnectionRevisionId::new(),
            ConnectionQualificationRevisionId::new(),
            ModelRevisionId::new(),
            ModelQualificationRevisionId::new(),
            QualificationTargetBinding::NoAuth {
                binding_revision_id: NoAuthBindingRevisionId::new(),
            },
        );

        assert!(snapshot.qualification_target().is_no_auth());
    }

    #[test]
    fn model_revision_preserves_unknown_instead_of_zero() {
        let revision = crate::ModelRevision::new_chat(
            WorkspaceId::new(),
            crate::ConnectionRevisionId::new(),
            "model-a",
        )
        .expect("wire model id is valid");

        assert!(revision.observed_context_window().is_unknown());
    }

    #[test]
    fn model_revision_has_validated_provenance_bearing_observations() {
        let observation =
            crate::ModelObservation::provider_observed(0, ModelQualificationRevisionId::new());

        assert!(observation.is_err());
    }

    #[test]
    fn observations_are_positive_and_model_hydration_preserves_its_identity() {
        let qualification_revision_id = ModelQualificationRevisionId::new();
        let zero = crate::ModelObservation::provider_observed(0, qualification_revision_id);
        let revision = crate::ModelRevision::new_chat(
            WorkspaceId::new(),
            ConnectionRevisionId::new(),
            "model-a",
        )
        .unwrap()
        .with_observations(
            crate::ModelObservation::unknown(),
            crate::ModelObservation::unknown(),
        );

        assert!(zero.is_err());
        assert!(revision.is_ok());
    }

    #[test]
    fn observed_model_values_expose_their_safe_provenance() {
        let qualification_revision_id = ModelQualificationRevisionId::new();
        let observed = crate::ModelObservation::operator_supplied(512, qualification_revision_id)
            .expect("positive operator observation is valid");
        let unknown = crate::ModelObservation::<u32>::unknown();

        assert_eq!(
            observed.qualification_revision_id(),
            Some(qualification_revision_id)
        );
        assert_eq!(
            observed.source(),
            Some(crate::ModelObservationSource::Operator)
        );
        assert_eq!(unknown.qualification_revision_id(), None);
        assert_eq!(unknown.source(), None);
    }

    #[test]
    fn persisted_snapshot_preserves_its_complete_tuple() {
        let snapshot_id = crate::ModelBindingSnapshotId::from_uuid(uuid::Uuid::now_v7());
        let connection_revision_id = ConnectionRevisionId::new();
        let connection_qualification_revision_id = ConnectionQualificationRevisionId::new();
        let model_revision_id = ModelRevisionId::new();
        let model_qualification_revision_id = ModelQualificationRevisionId::new();
        let snapshot = ModelBindingSnapshot::from_persisted(
            snapshot_id,
            WorkspaceId::new(),
            connection_revision_id,
            connection_qualification_revision_id,
            model_revision_id,
            model_qualification_revision_id,
            QualificationTargetBinding::NoAuth {
                binding_revision_id: NoAuthBindingRevisionId::new(),
            },
        )
        .expect("persisted complete tuple is valid");

        assert_eq!(snapshot.id(), snapshot_id);
        assert_eq!(snapshot.connection_revision_id(), connection_revision_id);
        assert_eq!(
            snapshot.connection_qualification_revision_id(),
            connection_qualification_revision_id
        );
        assert_eq!(snapshot.model_revision_id(), model_revision_id);
        assert_eq!(
            snapshot.model_qualification_revision_id(),
            model_qualification_revision_id
        );
    }
}
