use std::fmt;

use crate::models::{
    ConnectionQualificationRevisionId, ModelBindingSnapshotId, ModelQualificationRevisionId,
};
use crate::{
    ConnectionRevisionId, DomainError, ExternalEffectId, ModelRevisionId, QualificationJobId,
    WorkspaceId,
};

macro_rules! evidence_reference_id {
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
    };
}

evidence_reference_id!(RequestShapeRevisionId);
evidence_reference_id!(SamplingRevisionId);
evidence_reference_id!(LimitsRevisionId);
evidence_reference_id!(ToolSchemaRevisionId);
evidence_reference_id!(QualificationTargetBindingId);
evidence_reference_id!(GovernedInputMaterialId);

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
pub struct ModelRequestEvidenceId(uuid::Uuid);
impl ModelRequestEvidenceId {
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
impl Default for ModelRequestEvidenceId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for ModelRequestEvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelRequestEvidenceStatus {
    Complete,
    Incomplete,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct QualificationProbeOrdinal(u8);

impl QualificationProbeOrdinal {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let ordinal = match value {
            "00" => 0,
            "10" => 10,
            "15" => 15,
            "20" => 20,
            "30" => 30,
            "35" => 35,
            "40" => 40,
            "50" => 50,
            "60" => 60,
            "70" => 70,
            "80" => 80,
            "90" => 90,
            _ => {
                return Err(DomainError::InvalidArgument(
                    "qualification probe ordinal is not in the pinned q1 vocabulary".into(),
                ));
            }
        };
        Ok(Self(ordinal))
    }

    pub const fn as_str(self) -> &'static str {
        match self.0 {
            0 => "00",
            10 => "10",
            15 => "15",
            20 => "20",
            30 => "30",
            35 => "35",
            40 => "40",
            50 => "50",
            60 => "60",
            70 => "70",
            80 => "80",
            90 => "90",
            _ => unreachable!(),
        }
    }
}

/// ```compile_fail
/// use vestrace_domain::{ModelRequestEvidenceNode, QualificationJobId};
/// let _ = ModelRequestEvidenceNode::QualificationProbe {
///     job_id: QualificationJobId::new(),
///     ordinal: "90".to_string(),
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub enum ModelRequestEvidenceNode {
    ConnectionRevision {
        revision_id: ConnectionRevisionId,
    },
    ModelRevision {
        revision_id: ModelRevisionId,
    },
    ConnectionQualificationRevision {
        revision_id: ConnectionQualificationRevisionId,
    },
    ModelQualificationRevision {
        revision_id: ModelQualificationRevisionId,
    },
    BindingSnapshot {
        snapshot_id: ModelBindingSnapshotId,
    },
    Effect {
        effect_id: ExternalEffectId,
    },
    QualificationProbe {
        job_id: QualificationJobId,
        ordinal: QualificationProbeOrdinal,
    },
    QualificationTarget {
        binding_id: QualificationTargetBindingId,
    },
    GovernedInputMaterial {
        material_id: GovernedInputMaterialId,
        ordinal: u32,
    },
    ToolSchemaRevision {
        revision_id: ToolSchemaRevisionId,
        version: u64,
        ordinal: u32,
    },
    SamplingRevision {
        revision_id: SamplingRevisionId,
        version: u64,
    },
    LimitsRevision {
        revision_id: LimitsRevisionId,
        version: u64,
    },
    RequestShapeRevision {
        revision_id: RequestShapeRevisionId,
        version: u64,
    },
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::ModelRequestEvidence>("{}");
/// ```
/// ```compile_fail
/// let _ = vestrace_domain::ModelRequestEvidenceCurrentStatus::expired(
///     vestrace_domain::ErasureReceipt::new(),
/// );
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ModelRequestEvidence {
    id: ModelRequestEvidenceId,
    workspace_id: WorkspaceId,
    nodes: Vec<ModelRequestEvidenceNode>,
}
impl ModelRequestEvidence {
    pub fn new(
        workspace_id: WorkspaceId,
        nodes: Vec<ModelRequestEvidenceNode>,
    ) -> Result<Self, DomainError> {
        if nodes.is_empty() {
            return Err(DomainError::InvalidArgument(
                "model request evidence requires typed references".into(),
            ));
        }
        Ok(Self {
            id: ModelRequestEvidenceId::new(),
            workspace_id,
            nodes,
        })
    }
    pub fn from_persisted(
        id: ModelRequestEvidenceId,
        workspace_id: WorkspaceId,
        nodes: Vec<ModelRequestEvidenceNode>,
    ) -> Result<Self, DomainError> {
        if nodes.is_empty() {
            return Err(DomainError::InvalidArgument(
                "model request evidence requires typed references".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            nodes,
        })
    }
    pub const fn id(&self) -> ModelRequestEvidenceId {
        self.id
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn nodes(&self) -> &[ModelRequestEvidenceNode] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ConnectionRevisionId, ModelRequestEvidence, ModelRequestEvidenceNode,
        QualificationProbeOrdinal, WorkspaceId,
    };

    #[test]
    fn model_request_evidence_contains_only_typed_references() {
        let evidence = ModelRequestEvidence::new(
            WorkspaceId::new(),
            vec![ModelRequestEvidenceNode::ConnectionRevision {
                revision_id: ConnectionRevisionId::new(),
            }],
        )
        .expect("one typed reference is required");

        assert_eq!(evidence.nodes().len(), 1);
    }

    #[test]
    fn qualification_probe_ordinal_rejects_non_ordinal_data() {
        assert!(QualificationProbeOrdinal::parse("Bearer secret-value").is_err());
        assert_eq!(
            QualificationProbeOrdinal::parse("90").unwrap().as_str(),
            "90"
        );
    }

    #[test]
    fn persisted_evidence_preserves_its_identity_without_assigning_status() {
        let id = crate::ModelRequestEvidenceId::from_uuid(uuid::Uuid::now_v7());
        let evidence = ModelRequestEvidence::from_persisted(
            id,
            WorkspaceId::new(),
            vec![ModelRequestEvidenceNode::ConnectionRevision {
                revision_id: ConnectionRevisionId::new(),
            }],
        )
        .expect("typed persisted evidence is valid");

        assert_eq!(evidence.id(), id);
    }
}
