use crate::{
    DomainError,
    id::{AgentId, AgentRevisionId, SkillId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Orchestrator,
    Specialist,
    Reviewer,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelRequirements {
    pub min_quality: f32,
    pub max_cost_per_mtoken: Option<f32>,
    pub context_tokens: u32,
    pub privacy_local_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillRef {
    pub skill_id: SkillId,
    pub skill_revision: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryScope {
    pub include_relations: bool,
    pub include_observations: bool,
    pub include_procedures: bool,
    pub include_outcomes: bool,
    pub max_items: u32,
}

impl Default for MemoryScope {
    fn default() -> Self {
        Self {
            include_relations: true,
            include_observations: true,
            include_procedures: true,
            include_outcomes: true,
            max_items: 20,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BudgetPolicy {
    pub max_tokens_per_run: u32,
    pub max_cost_per_run: Option<f32>,
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self {
            max_tokens_per_run: 100_000,
            max_cost_per_run: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct CapabilitySet {
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentDefinition {
    pub id: AgentId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub current_revision: u32,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentRevision {
    pub revision_id: AgentRevisionId,
    pub agent_id: AgentId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub role: AgentRole,
    pub instructions: String,
    pub model_requirements: ModelRequirements,
    pub skills: Vec<SkillRef>,
    pub memory_scope: MemoryScope,
    pub budget_policy: BudgetPolicy,
    pub requested_capabilities: CapabilitySet,
    pub created_at: Timestamp,
}

impl AgentRevision {
    pub fn validate(&self, known_skills: &[SkillId]) -> Result<(), DomainError> {
        if self.instructions.is_empty() {
            return Err(DomainError::InvalidArgument(
                "agent instructions must not be empty".to_owned(),
            ));
        }

        let mut seen = std::collections::HashSet::new();
        for skill_ref in &self.skills {
            if !known_skills.contains(&skill_ref.skill_id) {
                return Err(DomainError::InvalidArgument(format!(
                    "agent references unknown skill {}",
                    skill_ref.skill_id
                )));
            }
            if !seen.insert(skill_ref.skill_id) {
                return Err(DomainError::InvalidArgument(format!(
                    "agent references skill {} more than once",
                    skill_ref.skill_id
                )));
            }
        }

        if self.budget_policy.max_tokens_per_run == 0 {
            return Err(DomainError::InvalidArgument(
                "budget max_tokens_per_run must be > 0".to_owned(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AgentId, AgentRevisionId, SkillId, WorkspaceId};

    fn valid_revision() -> AgentRevision {
        AgentRevision {
            revision_id: AgentRevisionId::new(),
            agent_id: AgentId::new(),
            workspace_id: WorkspaceId::new(),
            revision_number: 1,
            role: AgentRole::Specialist,
            instructions: "You are a code reviewer.".to_owned(),
            model_requirements: ModelRequirements {
                min_quality: 0.8,
                max_cost_per_mtoken: Some(0.50),
                context_tokens: 8192,
                privacy_local_only: false,
            },
            skills: Vec::new(),
            memory_scope: MemoryScope::default(),
            budget_policy: BudgetPolicy::default(),
            requested_capabilities: CapabilitySet::default(),
            created_at: crate::time::now(),
        }
    }

    #[test]
    fn rejects_empty_instructions() {
        let mut rev = valid_revision();
        rev.instructions = String::new();
        assert!(rev.validate(&[]).is_err());
    }

    #[test]
    fn rejects_unknown_skill_reference() {
        let unknown = SkillId::new();
        let known = SkillId::new();
        let mut rev = valid_revision();
        rev.skills = vec![SkillRef {
            skill_id: unknown,
            skill_revision: 1,
        }];
        assert!(rev.validate(&[known]).is_err());
    }

    #[test]
    fn rejects_duplicate_skill_reference() {
        let skill = SkillId::new();
        let mut rev = valid_revision();
        rev.skills = vec![
            SkillRef {
                skill_id: skill,
                skill_revision: 1,
            },
            SkillRef {
                skill_id: skill,
                skill_revision: 2,
            },
        ];
        assert!(rev.validate(&[skill]).is_err());
    }

    #[test]
    fn rejects_zero_budget() {
        let mut rev = valid_revision();
        rev.budget_policy.max_tokens_per_run = 0;
        assert!(rev.validate(&[]).is_err());
    }

    #[test]
    fn accepts_valid_revision() {
        let skill = SkillId::new();
        let mut rev = valid_revision();
        rev.skills = vec![SkillRef {
            skill_id: skill,
            skill_revision: 1,
        }];
        assert!(rev.validate(&[skill]).is_ok());
    }
}
