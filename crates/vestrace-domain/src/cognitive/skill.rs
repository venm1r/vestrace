use crate::{
    DomainError,
    id::{SkillId, SkillRevisionId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    Prompt,
    McpTool,
    Http,
    Composite,
    Human,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillDependency {
    pub skill_id: SkillId,
    pub skill_revision: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApplicabilityCondition {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillExample {
    pub input: String,
    pub output: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromptImplementation {
    pub template: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct McpToolImplementation {
    pub server_name: String,
    pub tool_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HttpImplementation {
    pub method: String,
    pub url_template: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompositeImplementation {
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HumanImplementation {
    pub prompt: String,
    pub approval_required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillImplementation {
    Prompt(PromptImplementation),
    McpTool(McpToolImplementation),
    Http(HttpImplementation),
    Composite(CompositeImplementation),
    Human(HumanImplementation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillDefinition {
    pub id: SkillId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub current_revision: u32,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillRevision {
    pub revision_id: SkillRevisionId,
    pub skill_id: SkillId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub kind: SkillKind,
    pub implementation: SkillImplementation,
    pub required_capabilities: Vec<String>,
    pub dependencies: Vec<SkillDependency>,
    pub applicability_conditions: Vec<ApplicabilityCondition>,
    pub examples: Vec<SkillExample>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

impl SkillRevision {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.input_schema.is_some() && !is_valid_json_schema(self.input_schema.as_ref().unwrap())
        {
            return Err(DomainError::InvalidArgument(
                "skill input_schema is not a valid JSON Schema".to_owned(),
            ));
        }
        if self.output_schema.is_some()
            && !is_valid_json_schema(self.output_schema.as_ref().unwrap())
        {
            return Err(DomainError::InvalidArgument(
                "skill output_schema is not a valid JSON Schema".to_owned(),
            ));
        }

        let mut seen = std::collections::HashSet::new();
        for dep in &self.dependencies {
            if !seen.insert(dep.skill_id) {
                return Err(DomainError::InvalidArgument(format!(
                    "skill dependency {} is listed more than once",
                    dep.skill_id
                )));
            }
        }

        if matches!(self.kind, SkillKind::Composite) {
            if let SkillImplementation::Composite(impl_) = &self.implementation {
                if impl_.steps.is_empty() {
                    return Err(DomainError::InvalidArgument(
                        "composite skill must have at least one step".to_owned(),
                    ));
                }
            }
        }

        if matches!(self.kind, SkillKind::Prompt) {
            if let SkillImplementation::Prompt(impl_) = &self.implementation {
                if impl_.template.is_empty() {
                    return Err(DomainError::InvalidArgument(
                        "prompt skill template must not be empty".to_owned(),
                    ));
                }
            }
        }

        if matches!(self.kind, SkillKind::Http) {
            if let SkillImplementation::Http(impl_) = &self.implementation {
                if impl_.url_template.is_empty() {
                    return Err(DomainError::InvalidArgument(
                        "http skill url_template must not be empty".to_owned(),
                    ));
                }
                if !impl_.method.is_empty()
                    && !["GET", "POST", "PUT", "PATCH", "DELETE"]
                        .iter()
                        .any(|m| impl_.method.eq_ignore_ascii_case(m))
                {
                    return Err(DomainError::InvalidArgument(format!(
                        "http skill method '{}' is not recognised",
                        impl_.method
                    )));
                }
            }
        }

        Ok(())
    }
}

fn is_valid_json_schema(value: &serde_json::Value) -> bool {
    if let Some(obj) = value.as_object() {
        obj.contains_key("type") || obj.contains_key("properties") || obj.contains_key("$ref")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{SkillId, SkillRevisionId, WorkspaceId};

    fn valid_revision(kind: SkillKind, impl_: SkillImplementation) -> SkillRevision {
        SkillRevision {
            revision_id: SkillRevisionId::new(),
            skill_id: SkillId::new(),
            workspace_id: WorkspaceId::new(),
            revision_number: 1,
            kind,
            implementation: impl_,
            required_capabilities: vec![],
            dependencies: vec![],
            applicability_conditions: vec![],
            examples: vec![],
            input_schema: None,
            output_schema: None,
            created_at: crate::time::now(),
        }
    }

    #[test]
    fn rejects_invalid_input_schema() {
        let mut rev = valid_revision(
            SkillKind::Prompt,
            SkillImplementation::Prompt(PromptImplementation {
                template: "test".to_owned(),
            }),
        );
        rev.input_schema = Some(serde_json::json!({}));
        assert!(rev.validate().is_err());
    }

    #[test]
    fn accepts_valid_input_schema() {
        let mut rev = valid_revision(
            SkillKind::Prompt,
            SkillImplementation::Prompt(PromptImplementation {
                template: "test".to_owned(),
            }),
        );
        rev.input_schema = Some(serde_json::json!({"type": "object"}));
        assert!(rev.validate().is_ok());
    }

    #[test]
    fn rejects_empty_prompt_template() {
        let rev = valid_revision(
            SkillKind::Prompt,
            SkillImplementation::Prompt(PromptImplementation {
                template: String::new(),
            }),
        );
        assert!(rev.validate().is_err());
    }

    #[test]
    fn rejects_empty_composite_steps() {
        let rev = valid_revision(
            SkillKind::Composite,
            SkillImplementation::Composite(CompositeImplementation { steps: vec![] }),
        );
        assert!(rev.validate().is_err());
    }

    #[test]
    fn rejects_empty_http_url() {
        let rev = valid_revision(
            SkillKind::Http,
            SkillImplementation::Http(HttpImplementation {
                method: "GET".to_owned(),
                url_template: String::new(),
            }),
        );
        assert!(rev.validate().is_err());
    }

    #[test]
    fn rejects_invalid_http_method() {
        let rev = valid_revision(
            SkillKind::Http,
            SkillImplementation::Http(HttpImplementation {
                method: "BREW".to_owned(),
                url_template: "https://example.com/api".to_owned(),
            }),
        );
        assert!(rev.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_dependency() {
        let dep_id = SkillId::new();
        let mut rev = valid_revision(
            SkillKind::Prompt,
            SkillImplementation::Prompt(PromptImplementation {
                template: "test".to_owned(),
            }),
        );
        rev.dependencies = vec![
            SkillDependency {
                skill_id: dep_id,
                skill_revision: 1,
            },
            SkillDependency {
                skill_id: dep_id,
                skill_revision: 2,
            },
        ];
        assert!(rev.validate().is_err());
    }

    #[test]
    fn accepts_valid_skill() {
        let rev = valid_revision(
            SkillKind::McpTool,
            SkillImplementation::McpTool(McpToolImplementation {
                server_name: "filesystem".to_owned(),
                tool_name: "read_file".to_owned(),
            }),
        );
        assert!(rev.validate().is_ok());
    }
}
