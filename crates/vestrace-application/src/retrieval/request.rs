use serde::{Deserialize, Serialize};
use vestrace_domain::{
    MemoryKind, MemoryStatus, RetrievalIntent, TimePerspective, WorkspaceId, id::RetrievalRunId,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalRequest {
    pub query: String,
    pub intent: RetrievalIntent,
    pub time_perspective: TimePerspective,
    pub workspace_id: WorkspaceId,
    pub allowed_statuses: Vec<MemoryStatus>,
    #[serde(skip)]
    statuses_explicit: bool,
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
            statuses_explicit: false,
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

    pub fn with_allowed_statuses(mut self, statuses: Vec<MemoryStatus>) -> Self {
        self.allowed_statuses = statuses;
        self.statuses_explicit = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedRetrievalRequest {
    /// The causal identity shared by every channel and the retrieval journal.
    pub request_id: RetrievalRunId,
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

        let historical_default =
            !req.statuses_explicit && req.allowed_statuses == vec![MemoryStatus::Active];
        let allowed_statuses = if matches!(req.time_perspective, TimePerspective::Current) {
            vec![MemoryStatus::Active]
        } else if historical_default {
            all_memory_statuses()
        } else {
            req.allowed_statuses
        };

        let channel_limit = req.limit.clamp(1, 100);

        Ok(Self {
            request_id: RetrievalRunId::new(),
            query,
            intent: req.intent,
            time_perspective: req.time_perspective,
            workspace_id: req.workspace_id,
            allowed_statuses,
            allowed_kinds,
            channel_limit,
            token_budget: req.token_budget,
            include_explanation: req.include_explanation,
        })
    }
}

fn all_memory_statuses() -> Vec<MemoryStatus> {
    vec![
        MemoryStatus::Candidate,
        MemoryStatus::Active,
        MemoryStatus::Superseded,
        MemoryStatus::Rejected,
        MemoryStatus::Expired,
        MemoryStatus::Deleted,
    ]
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

    #[test]
    fn normalize_current_forces_active_status_only() {
        let mut req = RetrievalRequest::new(WorkspaceId::new(), "test");
        req.allowed_statuses = vec![MemoryStatus::Superseded, MemoryStatus::Active];

        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();

        assert_eq!(normalized.allowed_statuses, vec![MemoryStatus::Active]);
    }

    #[test]
    fn normalize_all_history_expands_the_untouched_current_default() {
        let req = RetrievalRequest::new(WorkspaceId::new(), "test")
            .with_time_perspective(TimePerspective::AllHistory);

        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();

        assert_eq!(
            normalized.allowed_statuses,
            vec![
                MemoryStatus::Candidate,
                MemoryStatus::Active,
                MemoryStatus::Superseded,
                MemoryStatus::Rejected,
                MemoryStatus::Expired,
                MemoryStatus::Deleted,
            ]
        );
        assert_eq!(normalized.time_perspective, TimePerspective::AllHistory);
    }

    #[test]
    fn normalize_as_of_preserves_the_requested_timestamp() {
        let at = chrono::Utc::now();
        let req = RetrievalRequest::new(WorkspaceId::new(), "test")
            .with_time_perspective(TimePerspective::AsOf(at));

        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();

        assert_eq!(normalized.time_perspective, TimePerspective::AsOf(at));
    }

    #[test]
    fn normalize_preserves_an_explicit_historical_status_filter() {
        let req = RetrievalRequest::new(WorkspaceId::new(), "test")
            .with_time_perspective(TimePerspective::AllHistory)
            .with_allowed_statuses(vec![MemoryStatus::Active]);

        let normalized = NormalizedRetrievalRequest::normalize(req).unwrap();

        assert_eq!(normalized.allowed_statuses, vec![MemoryStatus::Active]);
    }
}
