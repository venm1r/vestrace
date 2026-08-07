use serde::{Deserialize, Serialize};
use vestrace_domain::{MemoryKind, MemoryStatus, RetrievalIntent, TimePerspective, WorkspaceId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalRequest {
    pub query: String,
    pub intent: RetrievalIntent,
    pub time_perspective: TimePerspective,
    pub workspace_id: WorkspaceId,
    pub allowed_statuses: Vec<MemoryStatus>,
    pub allowed_kinds: Vec<MemoryKind>,
    pub limit: u32,
    pub token_budget: Option<u32>,
    pub include_explanation: bool,
}

impl RetrievalRequest {
    pub fn new(workspace_id: WorkspaceId, query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            intent: RetrievalIntent::SemanticRecall,
            time_perspective: TimePerspective::Current,
            workspace_id,
            allowed_statuses: vec![MemoryStatus::Active],
            allowed_kinds: Vec::new(),
            limit: 20,
            token_budget: None,
            include_explanation: false,
        }
    }

    pub fn with_intent(mut self, intent: RetrievalIntent) -> Self {
        self.intent = intent;
        self
    }

    pub fn with_time_perspective(mut self, perspective: TimePerspective) -> Self {
        self.time_perspective = perspective;
        self
    }

    pub fn with_token_budget(mut self, budget: u32) -> Self {
        self.token_budget = Some(budget);
        self
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedRetrievalRequest {
    pub query: String,
    pub intent: RetrievalIntent,
    pub time_perspective: TimePerspective,
    pub workspace_id: WorkspaceId,
    pub allowed_statuses: Vec<MemoryStatus>,
    pub allowed_kinds: Vec<MemoryKind>,
    pub channel_limit: u32,
    pub token_budget: Option<u32>,
    pub include_explanation: bool,
}

impl NormalizedRetrievalRequest {
    pub fn normalize(req: RetrievalRequest) -> Result<Self, crate::ApplicationError> {
        let query = req.query.trim().to_owned();
        if query.is_empty() {
            return Err(crate::ApplicationError::Domain(
                vestrace_domain::DomainError::InvalidArgument(
                    "retrieval query must not be empty".to_owned(),
                ),
            ));
        }

        let allowed_kinds = if req.allowed_kinds.is_empty() {
            intent_default_kinds(&req.intent)
        } else {
            req.allowed_kinds
        };

        let channel_limit = req.limit.clamp(1, 100);

        Ok(Self {
            query,
            intent: req.intent,
            time_perspective: req.time_perspective,
            workspace_id: req.workspace_id,
            allowed_statuses: req.allowed_statuses,
            allowed_kinds,
            channel_limit,
            token_budget: req.token_budget,
            include_explanation: req.include_explanation,
        })
    }
}

fn intent_default_kinds(intent: &RetrievalIntent) -> Vec<MemoryKind> {
    use vestrace_domain::MemoryKind::*;
    match intent {
        RetrievalIntent::CurrentState => vec![Fact, Preference, Constraint],
        RetrievalIntent::DecisionRecall => vec![Decision],
        RetrievalIntent::Timeline => vec![Decision, Outcome, Observation],
        RetrievalIntent::TaskResume => vec![Task, Procedure],
        RetrievalIntent::ProcedureLookup => vec![Procedure],
        RetrievalIntent::UserPreferences => vec![Preference],
        RetrievalIntent::SemanticRecall => Vec::new(),
        RetrievalIntent::ErrorRecovery => vec![Observation, Outcome],
        RetrievalIntent::ModelSelection => vec![Observation],
        RetrievalIntent::WorkflowContext => vec![Task, Procedure, Decision],
        RetrievalIntent::Exploration => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_empty_query() {
        let req = RetrievalRequest::new(WorkspaceId::new(), "   ");
        assert!(NormalizedRetrievalRequest::normalize(req).is_err());
    }

    #[test]
    fn normalize_clamps_channel_limit() {
        let req = RetrievalRequest::new(WorkspaceId::new(), "test").with_limit(200);
        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();
        assert_eq!(normalized.channel_limit, 100);
    }

    #[test]
    fn normalize_applies_intent_default_kinds() {
        let req = RetrievalRequest::new(WorkspaceId::new(), "test")
            .with_intent(RetrievalIntent::DecisionRecall);
        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();
        assert_eq!(normalized.allowed_kinds, vec![MemoryKind::Decision]);
    }
}
