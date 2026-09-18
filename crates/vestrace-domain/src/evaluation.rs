use crate::{
    DomainError, EvidenceRef, Timestamp,
    id::{
        EvaluationId, ModelExecutionAttemptId, ModelExecutionId, ModelId, PrincipalId,
        ToolDefinitionId, ToolInvocationId, WorkflowExecutionId, WorkflowRevisionId, WorkspaceId,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorKind {
    Deterministic,
    Human,
    Heuristic,
    ModelJudge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluatorRef {
    pub kind: EvaluatorKind,
    pub model_id: Option<ModelId>,
    pub revision: Option<String>,
    pub principal_id: Option<PrincipalId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationMetric {
    pub kind: String,
    pub value: f64,
    pub unit: Option<String>,
}

impl EvaluationMetric {
    pub fn new(
        kind: impl Into<String>,
        value: f64,
        unit: Option<String>,
    ) -> Result<Self, DomainError> {
        let kind = kind.into();
        if kind.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "evaluation metric kind must not be blank".into(),
            ));
        }
        if !value.is_finite() {
            return Err(DomainError::InvalidArgument(
                "evaluation metric value must be finite".into(),
            ));
        }
        Ok(Self { kind, value, unit })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationResult {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationAuthority {
    Deterministic,
    HumanAuthorized,
    Advisory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CognitiveEvaluationCriterion {
    PropertyCheck,
    StructuralMetric,
    EvidenceCitation,
    SchemaValidation,
    ExactWordingComparison,
}

impl CognitiveEvaluationCriterion {
    pub fn is_allowed_for_qualification(self) -> bool {
        !matches!(self, Self::ExactWordingComparison)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CognitiveQualificationCheck {
    pub criterion: CognitiveEvaluationCriterion,
    pub property_name: String,
    pub metric: EvaluationMetric,
    pub evidence_refs: Vec<EvidenceRef>,
}

impl CognitiveQualificationCheck {
    pub fn new(
        criterion: CognitiveEvaluationCriterion,
        property_name: impl Into<String>,
        metric: EvaluationMetric,
        evidence_refs: Vec<EvidenceRef>,
    ) -> Result<Self, DomainError> {
        let property_name = property_name.into();
        if property_name.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "cognitive qualification property name must not be blank".into(),
            ));
        }
        if evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "cognitive qualification check requires at least one evidence reference".into(),
            ));
        }
        if !criterion.is_allowed_for_qualification() {
            return Err(DomainError::PolicyViolation(
                "cognitive qualification must evaluate properties/evidence rather than exact LLM wording".into(),
            ));
        }
        Ok(Self {
            criterion,
            property_name,
            metric,
            evidence_refs,
        })
    }
}

impl EvaluationAuthority {
    /// Returns whether `self` has the default authority required to outrank an
    /// advisory/model-judge signal. Deterministic and human-authorized signals
    /// intentionally share the same higher tier; the domain does not invent an
    /// ordering between those two independent authorities.
    pub const fn priority(self) -> u8 {
        match self {
            Self::Advisory => 1,
            Self::Deterministic | Self::HumanAuthorized => 2,
        }
    }

    pub const fn is_stronger_than(self, other: Self) -> bool {
        self.priority() > other.priority()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EvaluationTarget {
    ModelExecution {
        execution_id: ModelExecutionId,
        attempt_id: Option<ModelExecutionAttemptId>,
        model_id: ModelId,
        model_revision: String,
    },
    WorkflowExecution {
        execution_id: WorkflowExecutionId,
        workflow_revision_id: WorkflowRevisionId,
    },
    ToolInvocation {
        invocation_id: ToolInvocationId,
        tool_definition_id: ToolDefinitionId,
        tool_revision: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationFact {
    pub id: EvaluationId,
    pub workspace_id: WorkspaceId,
    pub target: EvaluationTarget,
    pub evaluator: EvaluatorRef,
    pub metric: EvaluationMetric,
    pub result: EvaluationResult,
    pub evidence_refs: Vec<EvidenceRef>,
    pub authority: EvaluationAuthority,
    pub policy_version: String,
    pub created_at: Timestamp,
}

impl EvaluationFact {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: EvaluationId,
        workspace_id: WorkspaceId,
        target: EvaluationTarget,
        evaluator: EvaluatorRef,
        metric: EvaluationMetric,
        result: EvaluationResult,
        evidence_refs: Vec<EvidenceRef>,
        authority: EvaluationAuthority,
        policy_version: impl Into<String>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let policy_version = policy_version.into();
        let fact = Self {
            id,
            workspace_id,
            target,
            evaluator,
            metric,
            result,
            evidence_refs,
            authority,
            policy_version,
            created_at,
        };
        fact.validate()?;
        Ok(fact)
    }

    /// Validate facts received from a deserializer or another domain boundary.
    /// Public fields remain serializable for the HTTP/storage contracts, so
    /// rebuild paths must not assume that `EvaluationFact::new` was used.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "evaluation fact requires at least one evidence reference".into(),
            ));
        }
        if self.metric.kind.trim().is_empty() || !self.metric.value.is_finite() {
            return Err(DomainError::InvalidArgument(
                "evaluation fact metric must have a non-blank kind and finite value".into(),
            ));
        }
        if self.policy_version.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "evaluation fact policy version must not be blank".into(),
            ));
        }
        if self
            .evaluator
            .revision
            .as_deref()
            .is_some_and(|revision| revision.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "evaluation evaluator revision must not be blank".into(),
            ));
        }

        match self.evaluator.kind {
            EvaluatorKind::ModelJudge
                if self.evaluator.model_id.is_none() || self.evaluator.revision.is_none() =>
            {
                return Err(DomainError::InvalidArgument(
                    "model-judge evaluation requires exact model and evaluator revision".into(),
                ));
            }
            EvaluatorKind::Human if self.evaluator.principal_id.is_none() => {
                return Err(DomainError::InvalidArgument(
                    "human evaluation requires an evaluator principal".into(),
                ));
            }
            _ => {}
        }

        let expected_authority = match self.evaluator.kind {
            EvaluatorKind::Deterministic => EvaluationAuthority::Deterministic,
            EvaluatorKind::Human => EvaluationAuthority::HumanAuthorized,
            EvaluatorKind::Heuristic | EvaluatorKind::ModelJudge => EvaluationAuthority::Advisory,
        };
        if self.authority != expected_authority {
            return Err(DomainError::PolicyViolation(format!(
                "evaluation authority {:?} is incompatible with evaluator kind {:?}",
                self.authority, self.evaluator.kind
            )));
        }

        match &self.target {
            EvaluationTarget::ModelExecution { model_revision, .. } => {
                if model_revision.trim().is_empty() {
                    return Err(DomainError::InvalidArgument(
                        "model execution target requires an exact model revision".into(),
                    ));
                }
            }
            EvaluationTarget::WorkflowExecution { .. } => {}
            EvaluationTarget::ToolInvocation { tool_revision, .. } => {
                if tool_revision.trim().is_empty() {
                    return Err(DomainError::InvalidArgument(
                        "tool invocation target requires an exact tool revision".into(),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::EventId;

    #[test]
    fn metric_rejects_non_finite_values() {
        assert!(EvaluationMetric::new("quality", f64::NAN, None).is_err());
        assert!(EvaluationMetric::new("quality", f64::INFINITY, None).is_err());
    }

    #[test]
    fn fact_requires_evidence_and_policy_version() {
        let result = EvaluationFact::new(
            EvaluationId::new(),
            WorkspaceId::new(),
            EvaluationTarget::ToolInvocation {
                invocation_id: ToolInvocationId::new(),
                tool_definition_id: ToolDefinitionId::new(),
                tool_revision: "tool-v1".to_owned(),
            },
            EvaluatorRef {
                kind: EvaluatorKind::Deterministic,
                model_id: None,
                revision: None,
                principal_id: None,
            },
            EvaluationMetric::new("quality", 1.0, None).unwrap(),
            EvaluationResult::Pass,
            vec![EvidenceRef::event(EventId::new())],
            EvaluationAuthority::Deterministic,
            "policy-v1",
            crate::now(),
        );

        assert!(result.is_ok());
    }
}
