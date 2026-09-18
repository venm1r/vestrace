use crate::{
    ModelCostProfile, ProviderLocality,
    id::{ModelId, ProviderId, RoutingDecisionId, WorkspaceId},
    security::Sensitivity,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    Fixed,
    BestQuality,
    LowestCost,
    LowestLatency,
    Balanced,
    LocalOnly,
    WithinBudget,
    QualityAboveThreshold,
    CustomWeighted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskRequirements {
    pub task_type: String,
    pub required_capabilities: Vec<String>,
    pub min_quality: f32,
    pub max_cost_per_mtoken: Option<f32>,
    pub context_tokens: u32,
    #[serde(default)]
    pub privacy_local_only: bool,
    #[serde(default = "default_sensitivity")]
    pub data_sensitivity: Sensitivity,
    #[serde(default)]
    pub remote_transfer_approved: bool,
}

fn default_sensitivity() -> Sensitivity {
    Sensitivity::Internal
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RoutingCandidate {
    pub model_id: ModelId,
    pub provider_id: ProviderId,
    pub model_name: String,
    pub locality: ProviderLocality,
    pub context_window: u32,
    pub cost: ModelCostProfile,
    pub expected_quality: f32,
    pub estimated_latency_ms: u32,
    pub reliability: f32,
    pub observation_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RejectedCandidate {
    pub model_id: ModelId,
    pub model_name: String,
    pub reason_code: String,
    pub reason_detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RoutingDecision {
    pub id: RoutingDecisionId,
    pub workspace_id: WorkspaceId,
    pub strategy: RoutingStrategy,
    pub selected: Option<RoutingCandidate>,
    pub fallbacks: Vec<RoutingCandidate>,
    pub rejected: Vec<RejectedCandidate>,
    pub fallback_used: bool,
    pub task_type: String,
    pub rationale: String,
    pub created_at: Timestamp,
}

pub struct ModelRouter;

impl ModelRouter {
    pub fn route(
        workspace_id: WorkspaceId,
        strategy: RoutingStrategy,
        task: &TaskRequirements,
        candidates: &[RoutingCandidate],
        at: Timestamp,
    ) -> RoutingDecision {
        let id = RoutingDecisionId::new();
        let mut eligible: Vec<&RoutingCandidate> = Vec::new();
        let mut rejected: Vec<RejectedCandidate> = Vec::new();

        for candidate in candidates {
            if let Some(reason) = hard_filter(candidate, task) {
                rejected.push(RejectedCandidate {
                    model_id: candidate.model_id,
                    model_name: candidate.model_name.clone(),
                    reason_code: reason.0.to_owned(),
                    reason_detail: reason.1.to_owned(),
                });
            } else {
                eligible.push(candidate);
            }
        }

        eligible.sort_by(|a, b| {
            score_candidate(b, &strategy, task)
                .partial_cmp(&score_candidate(a, &strategy, task))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.reliability
                        .partial_cmp(&a.reliability)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.estimated_latency_ms.cmp(&b.estimated_latency_ms))
                .then_with(|| match (a.locality, b.locality) {
                    (ProviderLocality::Local, ProviderLocality::Remote) => std::cmp::Ordering::Less,
                    (ProviderLocality::Remote, ProviderLocality::Local) => {
                        std::cmp::Ordering::Greater
                    }
                    _ => std::cmp::Ordering::Equal,
                })
                .then_with(|| a.model_id.as_uuid().cmp(&b.model_id.as_uuid()))
        });

        let selected = eligible.first().copied().cloned();
        let fallbacks: Vec<RoutingCandidate> =
            eligible.iter().skip(1).map(|c| (*c).clone()).collect();

        let rationale = if selected.is_some() {
            format!(
                "selected {:?} model matching task '{}' with quality {:.2}",
                strategy,
                task.task_type,
                selected.as_ref().map(|c| c.expected_quality).unwrap_or(0.0),
            )
        } else {
            format!(
                "no eligible model for task '{}' with strategy {:?}",
                task.task_type, strategy
            )
        };

        RoutingDecision {
            id,
            workspace_id,
            strategy,
            selected,
            fallbacks,
            rejected,
            fallback_used: false,
            task_type: task.task_type.clone(),
            rationale,
            created_at: at,
        }
    }
}

impl RoutingDecision {
    pub fn next_fallback(&mut self) -> Option<RoutingCandidate> {
        if self.fallbacks.is_empty() {
            self.fallback_used = true;
            self.selected = None;
            return None;
        }
        let next = self.fallbacks.remove(0);
        self.fallback_used = true;
        self.selected = Some(next.clone());
        Some(next)
    }
}

fn hard_filter(
    candidate: &RoutingCandidate,
    task: &TaskRequirements,
) -> Option<(&'static str, String)> {
    if candidate.context_window < task.context_tokens {
        return Some((
            "context_limit_exceeded",
            format!(
                "model context {} < required {}",
                candidate.context_window, task.context_tokens
            ),
        ));
    }

    if task.privacy_local_only && candidate.locality == ProviderLocality::Remote {
        return Some((
            "privacy_violation",
            "remote model rejected by local-only privacy constraint".to_owned(),
        ));
    }

    if candidate.locality == ProviderLocality::Remote
        && task.data_sensitivity >= Sensitivity::Restricted
        && !task.remote_transfer_approved
    {
        return Some((
            "restricted_data_no_transfer_approval",
            "remote model rejected: data sensitivity is Restricted and no transfer approval granted".to_owned(),
        ));
    }

    if candidate.expected_quality < task.min_quality {
        return Some((
            "quality_below_threshold",
            format!(
                "quality {:.2} < minimum {:.2}",
                candidate.expected_quality, task.min_quality
            ),
        ));
    }

    if let Some(max_cost) = task.max_cost_per_mtoken {
        let total_cost =
            candidate.cost.input_cost_per_mtoken + candidate.cost.output_cost_per_mtoken;
        if total_cost > max_cost {
            return Some((
                "budget_exceeded",
                format!("cost {:.4} > max {:.4}", total_cost, max_cost),
            ));
        }
    }

    None
}

fn score_candidate(
    candidate: &RoutingCandidate,
    strategy: &RoutingStrategy,
    _task: &TaskRequirements,
) -> f64 {
    let quality = candidate.expected_quality as f64;
    let cost =
        (candidate.cost.input_cost_per_mtoken + candidate.cost.output_cost_per_mtoken) as f64;
    let latency = candidate.estimated_latency_ms as f64;
    let reliability = candidate.reliability as f64;

    match strategy {
        RoutingStrategy::Fixed => 1.0,
        RoutingStrategy::BestQuality => quality,
        RoutingStrategy::LowestCost => -cost,
        RoutingStrategy::LowestLatency => -latency,
        RoutingStrategy::LocalOnly => {
            if candidate.locality == ProviderLocality::Local {
                quality
            } else {
                f64::MIN
            }
        }
        RoutingStrategy::WithinBudget => quality - cost * 0.01,
        RoutingStrategy::QualityAboveThreshold => quality,
        RoutingStrategy::Balanced => {
            quality * 0.5 + reliability * 0.3 - cost * 0.1 - latency * 0.0001
        }
        RoutingStrategy::CustomWeighted => quality * 0.5 + reliability * 0.3 - cost * 0.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        id::{ModelId, ProviderId},
        now,
    };

    fn candidate(
        name: &str,
        quality: f32,
        cost_in: f32,
        cost_out: f32,
        context: u32,
        locality: ProviderLocality,
    ) -> RoutingCandidate {
        RoutingCandidate {
            model_id: ModelId::new(),
            provider_id: ProviderId::new(),
            model_name: name.to_owned(),
            locality,
            context_window: context,
            cost: ModelCostProfile::new(cost_in, cost_out).unwrap(),
            expected_quality: quality,
            estimated_latency_ms: 100,
            reliability: 0.95,
            observation_count: 10,
        }
    }

    fn task(min_quality: f32, privacy_local: bool) -> TaskRequirements {
        TaskRequirements {
            task_type: "code_generation".to_owned(),
            required_capabilities: vec!["generation.code".to_owned()],
            min_quality,
            max_cost_per_mtoken: None,
            context_tokens: 4096,
            privacy_local_only: privacy_local,
            data_sensitivity: Sensitivity::Internal,
            remote_transfer_approved: false,
        }
    }

    #[test]
    fn balanced_selects_highest_quality_above_threshold() {
        let candidates = vec![
            candidate(
                "cheap-general",
                0.78,
                0.01,
                0.01,
                8192,
                ProviderLocality::Remote,
            ),
            candidate(
                "local-coder",
                0.86,
                0.03,
                0.03,
                8192,
                ProviderLocality::Local,
            ),
            candidate(
                "cloud-reasoner",
                0.94,
                0.20,
                0.20,
                32768,
                ProviderLocality::Remote,
            ),
        ];

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &task(0.85, false),
            &candidates,
            now(),
        );

        assert!(decision.selected.is_some());
        assert_eq!(
            decision.selected.as_ref().unwrap().model_name,
            "cloud-reasoner"
        );
        assert!(
            decision
                .fallbacks
                .iter()
                .any(|c| c.model_name == "local-coder")
        );
    }

    #[test]
    fn rejects_below_quality_threshold() {
        let candidates = vec![
            candidate(
                "cheap-general",
                0.78,
                0.01,
                0.01,
                8192,
                ProviderLocality::Remote,
            ),
            candidate(
                "local-coder",
                0.86,
                0.03,
                0.03,
                8192,
                ProviderLocality::Local,
            ),
        ];

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &task(0.85, false),
            &candidates,
            now(),
        );

        assert!(
            decision
                .rejected
                .iter()
                .any(|r| r.model_name == "cheap-general"
                    && r.reason_code == "quality_below_threshold")
        );
        assert_eq!(
            decision.selected.as_ref().unwrap().model_name,
            "local-coder"
        );
    }

    #[test]
    fn privacy_local_only_rejects_remote() {
        let candidates = vec![
            candidate(
                "cloud-model",
                0.95,
                0.10,
                0.10,
                32768,
                ProviderLocality::Remote,
            ),
            candidate(
                "local-model",
                0.80,
                0.01,
                0.01,
                8192,
                ProviderLocality::Local,
            ),
        ];

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::LocalOnly,
            &task(0.75, true),
            &candidates,
            now(),
        );

        assert!(
            decision
                .rejected
                .iter()
                .any(|r| r.reason_code == "privacy_violation")
        );
        assert_eq!(
            decision.selected.as_ref().unwrap().model_name,
            "local-model"
        );
    }

    #[test]
    fn no_eligible_candidates_produces_empty_selection() {
        let candidates = vec![candidate(
            "low-quality",
            0.50,
            0.01,
            0.01,
            8192,
            ProviderLocality::Local,
        )];

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &task(0.85, false),
            &candidates,
            now(),
        );

        assert!(decision.selected.is_none());
        assert!(!decision.rejected.is_empty());
    }

    #[test]
    fn context_limit_exceeded_rejects() {
        let candidates = vec![candidate(
            "small-context",
            0.90,
            0.01,
            0.01,
            2048,
            ProviderLocality::Local,
        )];

        let mut t = task(0.85, false);
        t.context_tokens = 4096;

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &t,
            &candidates,
            now(),
        );

        assert!(
            decision
                .rejected
                .iter()
                .any(|r| r.reason_code == "context_limit_exceeded")
        );
        assert!(decision.selected.is_none());
    }

    #[test]
    fn budget_exceeded_rejects() {
        let candidates = vec![candidate(
            "expensive",
            0.95,
            0.50,
            0.50,
            32768,
            ProviderLocality::Remote,
        )];

        let mut t = task(0.85, false);
        t.max_cost_per_mtoken = Some(0.10);

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::WithinBudget,
            &t,
            &candidates,
            now(),
        );

        assert!(
            decision
                .rejected
                .iter()
                .any(|r| r.reason_code == "budget_exceeded")
        );
        assert!(decision.selected.is_none());
    }

    #[test]
    fn restricted_data_routes_to_local_without_approval() {
        let candidates = vec![
            candidate(
                "remote-hq",
                0.95,
                0.10,
                0.10,
                32768,
                ProviderLocality::Remote,
            ),
            candidate("local-lq", 0.80, 0.01, 0.01, 8192, ProviderLocality::Local),
        ];

        let mut t = task(0.75, false);
        t.data_sensitivity = Sensitivity::Restricted;
        t.remote_transfer_approved = false;

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &t,
            &candidates,
            now(),
        );

        assert!(
            decision
                .rejected
                .iter()
                .any(|r| r.reason_code == "restricted_data_no_transfer_approval"
                    && r.model_name == "remote-hq")
        );
        assert_eq!(decision.selected.as_ref().unwrap().model_name, "local-lq");
    }

    #[test]
    fn restricted_data_allows_remote_with_approval() {
        let candidates = vec![
            candidate(
                "remote-hq",
                0.95,
                0.10,
                0.10,
                32768,
                ProviderLocality::Remote,
            ),
            candidate("local-lq", 0.80, 0.01, 0.01, 8192, ProviderLocality::Local),
        ];

        let mut t = task(0.75, false);
        t.data_sensitivity = Sensitivity::Restricted;
        t.remote_transfer_approved = true;

        let decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &t,
            &candidates,
            now(),
        );

        assert_eq!(decision.selected.as_ref().unwrap().model_name, "remote-hq");
    }

    #[test]
    fn fallback_used_when_selected_unavailable() {
        let candidates = vec![
            candidate("primary", 0.95, 0.10, 0.10, 32768, ProviderLocality::Remote),
            candidate(
                "secondary",
                0.88,
                0.05,
                0.05,
                16384,
                ProviderLocality::Remote,
            ),
            candidate("tertiary", 0.82, 0.01, 0.01, 8192, ProviderLocality::Local),
        ];

        let mut decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &task(0.80, false),
            &candidates,
            now(),
        );

        assert_eq!(decision.selected.as_ref().unwrap().model_name, "primary");
        assert!(!decision.fallback_used);

        let fallback = decision.next_fallback();
        assert!(fallback.is_some());
        assert_eq!(fallback.unwrap().model_name, "secondary");
        assert!(decision.fallback_used);
        assert_eq!(decision.selected.as_ref().unwrap().model_name, "secondary");
    }

    #[test]
    fn fallback_exhausted_returns_none() {
        let candidates = vec![candidate(
            "only",
            0.90,
            0.01,
            0.01,
            8192,
            ProviderLocality::Local,
        )];

        let mut decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &task(0.85, false),
            &candidates,
            now(),
        );

        assert!(decision.selected.is_some());
        let fallback = decision.next_fallback();
        assert!(fallback.is_none());
        assert!(decision.selected.is_none());
    }

    #[test]
    fn fallback_does_not_consider_rejected_candidates() {
        let candidates = vec![
            candidate(
                "low-q-remote",
                0.70,
                0.01,
                0.01,
                8192,
                ProviderLocality::Remote,
            ),
            candidate("local-ok", 0.86, 0.03, 0.03, 8192, ProviderLocality::Local),
        ];

        let mut t = task(0.85, true);
        t.data_sensitivity = Sensitivity::Confidential;

        let mut decision = ModelRouter::route(
            WorkspaceId::new(),
            RoutingStrategy::Balanced,
            &t,
            &candidates,
            now(),
        );

        assert_eq!(decision.selected.as_ref().unwrap().model_name, "local-ok");
        assert!(decision.fallbacks.is_empty());

        let fallback = decision.next_fallback();
        assert!(fallback.is_none());
        assert!(decision.selected.is_none());
    }
}
