use crate::id::{AgentId, MemoryId, ModelId, SkillId, WorkflowId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRef {
    Memory(MemoryId),
    Agent(AgentId),
    Skill(SkillId),
    Model(ModelId),
    Workflow(WorkflowId),
    Migration(u64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagnosticFinding {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub object: Option<KnowledgeRef>,
    pub message: String,
    pub remediation: String,
}

impl DiagnosticFinding {
    pub fn error(
        code: &str,
        object: Option<KnowledgeRef>,
        message: &str,
        remediation: &str,
    ) -> Self {
        Self {
            code: code.to_string(),
            severity: DiagnosticSeverity::Error,
            object,
            message: message.to_string(),
            remediation: remediation.to_string(),
        }
    }

    pub fn warning(
        code: &str,
        object: Option<KnowledgeRef>,
        message: &str,
        remediation: &str,
    ) -> Self {
        Self {
            code: code.to_string(),
            severity: DiagnosticSeverity::Warning,
            object,
            message: message.to_string(),
            remediation: remediation.to_string(),
        }
    }

    pub fn info(
        code: &str,
        object: Option<KnowledgeRef>,
        message: &str,
        remediation: &str,
    ) -> Self {
        Self {
            code: code.to_string(),
            severity: DiagnosticSeverity::Info,
            object,
            message: message.to_string(),
            remediation: remediation.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub findings: Vec<DiagnosticFinding>,
}

impl DiagnosticReport {
    pub fn new(findings: Vec<DiagnosticFinding>) -> Self {
        Self { findings }
    }

    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == DiagnosticSeverity::Error)
    }

    pub fn errors(&self) -> Vec<&DiagnosticFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == DiagnosticSeverity::Error)
            .collect()
    }

    pub fn warnings(&self) -> Vec<&DiagnosticFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == DiagnosticSeverity::Warning)
            .collect()
    }

    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_with_error_has_errors() {
        let report = DiagnosticReport::new(vec![
            DiagnosticFinding::error("MIGRATION_MISMATCH", None, "incompatible", "run migrate"),
            DiagnosticFinding::warning("OUTBOX_LAG", None, "2 messages", "process outbox"),
        ]);
        assert!(report.has_errors());
        assert_eq!(report.errors().len(), 1);
        assert_eq!(report.warnings().len(), 1);
        assert!(!report.is_clean());
    }

    #[test]
    fn report_with_only_warnings_has_no_errors() {
        let report = DiagnosticReport::new(vec![DiagnosticFinding::warning(
            "OUTBOX_LAG",
            None,
            "2 messages",
            "process outbox",
        )]);
        assert!(!report.has_errors());
        assert!(!report.is_clean());
    }

    #[test]
    fn empty_report_is_clean() {
        let report = DiagnosticReport::new(vec![]);
        assert!(report.is_clean());
        assert!(!report.has_errors());
    }
}
