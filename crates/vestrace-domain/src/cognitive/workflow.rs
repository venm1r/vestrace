use crate::{
    id::{
        AgentId, SkillId, WorkflowId, WorkflowNodeId, WorkflowRevisionId, WorkflowTransitionId,
        WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Agent,
    Skill,
    Tool,
    Decision,
    Parallel,
    Join,
    HumanApproval,
    SubWorkflow,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowNodeRef {
    pub node_id: WorkflowNodeId,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowNode {
    pub id: WorkflowNodeId,
    pub kind: WorkflowNodeKind,
    pub label: String,
    pub agent_ref: Option<AgentId>,
    pub skill_ref: Option<SkillId>,
    pub sub_workflow_ref: Option<(WorkflowId, u32)>,
    pub is_start: bool,
    pub is_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTransition {
    pub id: WorkflowTransitionId,
    pub from_node: WorkflowNodeId,
    pub to_node: WorkflowNodeId,
    pub condition: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoopPolicy {
    pub max_iterations: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowDefinition {
    pub id: WorkflowId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub current_revision: u32,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowRevision {
    pub revision_id: WorkflowRevisionId,
    pub workflow_id: WorkflowId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub nodes: Vec<WorkflowNode>,
    pub transitions: Vec<WorkflowTransition>,
    pub loop_policy: Option<LoopPolicy>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowValidationReport {
    pub errors: Vec<WorkflowValidationError>,
}

impl WorkflowValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowValidationError {
    pub code: String,
    pub node_id: Option<WorkflowNodeId>,
    pub transition_id: Option<WorkflowTransitionId>,
    pub message: String,
}

impl WorkflowRevision {
    pub fn validate(&self) -> WorkflowValidationReport {
        let mut errors = Vec::new();

        let node_ids: std::collections::HashSet<WorkflowNodeId> =
            self.nodes.iter().map(|n| n.id).collect();

        let start_count = self.nodes.iter().filter(|n| n.is_start).count();
        if start_count == 0 {
            errors.push(WorkflowValidationError {
                code: "missing_start_node".to_owned(),
                node_id: None,
                transition_id: None,
                message: "workflow has no start node".to_owned(),
            });
        }
        if start_count > 1 {
            errors.push(WorkflowValidationError {
                code: "duplicate_start_node".to_owned(),
                node_id: None,
                transition_id: None,
                message: format!("workflow has {start_count} start nodes, expected 1"),
            });
        }

        let mut seen_ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if !seen_ids.insert(node.id) {
                errors.push(WorkflowValidationError {
                    code: "duplicate_node_id".to_owned(),
                    node_id: Some(node.id),
                    transition_id: None,
                    message: format!("duplicate node id {}", node.id),
                });
            }
        }

        for transition in &self.transitions {
            if !node_ids.contains(&transition.from_node) {
                errors.push(WorkflowValidationError {
                    code: "transition_from_missing_node".to_owned(),
                    node_id: None,
                    transition_id: Some(transition.id),
                    message: format!(
                        "transition {} references missing from_node {}",
                        transition.id, transition.from_node
                    ),
                });
            }
            if !node_ids.contains(&transition.to_node) {
                errors.push(WorkflowValidationError {
                    code: "transition_to_missing_node".to_owned(),
                    node_id: None,
                    transition_id: Some(transition.id),
                    message: format!(
                        "transition {} references missing to_node {}",
                        transition.id, transition.to_node
                    ),
                });
            }
        }

        for node in &self.nodes {
            if node.is_required && !self.is_reachable_from_start(&node.id) {
                errors.push(WorkflowValidationError {
                    code: "unreachable_required_node".to_owned(),
                    node_id: Some(node.id),
                    transition_id: None,
                    message: format!(
                        "required node '{}' ({}) is unreachable from start",
                        node.label, node.id
                    ),
                });
            }
        }

        let parallel_nodes: Vec<&WorkflowNode> = self
            .nodes
            .iter()
            .filter(|n| n.kind == WorkflowNodeKind::Parallel)
            .collect();
        for parallel in parallel_nodes {
            let has_join = self.transitions.iter().any(|t| {
                t.from_node == parallel.id
                    && self
                        .nodes
                        .iter()
                        .any(|n| n.id == t.to_node && n.kind == WorkflowNodeKind::Join)
            });
            if !has_join {
                errors.push(WorkflowValidationError {
                    code: "parallel_without_join".to_owned(),
                    node_id: Some(parallel.id),
                    transition_id: None,
                    message: format!(
                        "parallel node '{}' ({}) has no join successor",
                        parallel.label, parallel.id
                    ),
                });
            }
        }

        for node in &self.nodes {
            if node.kind == WorkflowNodeKind::SubWorkflow {
                if let Some((sub_wf_id, _)) = &node.sub_workflow_ref {
                    if sub_wf_id != &self.workflow_id {
                        errors.push(WorkflowValidationError {
                            code: "subworkflow_cross_workspace".to_owned(),
                            node_id: Some(node.id),
                            transition_id: None,
                            message: format!(
                                "sub-workflow reference in node '{}' ({}) is external to this workflow",
                                node.label, node.id
                            ),
                        });
                    }
                }
            }
        }

        if self.loop_policy.is_none() && self.has_cycle() {
            errors.push(WorkflowValidationError {
                code: "cycle_without_loop_policy".to_owned(),
                node_id: None,
                transition_id: None,
                message: "workflow contains a cycle but no loop policy is defined".to_owned(),
            });
        }

        if let Some(policy) = &self.loop_policy {
            if policy.max_iterations == 0 {
                errors.push(WorkflowValidationError {
                    code: "loop_policy_zero_iterations".to_owned(),
                    node_id: None,
                    transition_id: None,
                    message: "loop policy max_iterations must be > 0".to_owned(),
                });
            }
        }

        WorkflowValidationReport { errors }
    }

    fn is_reachable_from_start(&self, target: &WorkflowNodeId) -> bool {
        let start = self.nodes.iter().find(|n| n.is_start);
        let Some(start_node) = start else {
            return false;
        };
        if start_node.id == *target {
            return true;
        }

        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start_node.id];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            for transition in &self.transitions {
                if transition.from_node == current && transition.to_node == *target {
                    return true;
                }
                if transition.from_node == current && !visited.contains(&transition.to_node) {
                    stack.push(transition.to_node);
                }
            }
        }
        false
    }

    fn has_cycle(&self) -> bool {
        let node_ids: Vec<WorkflowNodeId> = self.nodes.iter().map(|n| n.id).collect();
        let mut visited = std::collections::HashSet::new();
        let mut rec_stack = std::collections::HashSet::new();

        for &node in &node_ids {
            if !visited.contains(&node) && self.dfs_cycle(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn dfs_cycle(
        &self,
        node: WorkflowNodeId,
        visited: &mut std::collections::HashSet<WorkflowNodeId>,
        rec_stack: &mut std::collections::HashSet<WorkflowNodeId>,
    ) -> bool {
        visited.insert(node);
        rec_stack.insert(node);

        for transition in &self.transitions {
            if transition.from_node == node {
                if !visited.contains(&transition.to_node) {
                    if self.dfs_cycle(transition.to_node, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(&transition.to_node) {
                    return true;
                }
            }
        }

        rec_stack.remove(&node);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: u32, kind: WorkflowNodeKind, label: &str, is_start: bool) -> WorkflowNode {
        WorkflowNode {
            id: WorkflowNodeId::from_uuid(uuid::Uuid::from_u128(id as u128)),
            kind,
            label: label.to_owned(),
            agent_ref: None,
            skill_ref: None,
            sub_workflow_ref: None,
            is_start,
            is_required: false,
        }
    }

    fn make_transition(from: u32, to: u32) -> WorkflowTransition {
        WorkflowTransition {
            id: WorkflowTransitionId::new(),
            from_node: WorkflowNodeId::from_uuid(uuid::Uuid::from_u128(from as u128)),
            to_node: WorkflowNodeId::from_uuid(uuid::Uuid::from_u128(to as u128)),
            condition: None,
        }
    }

    fn make_revision(
        nodes: Vec<WorkflowNode>,
        transitions: Vec<WorkflowTransition>,
        loop_policy: Option<LoopPolicy>,
    ) -> WorkflowRevision {
        WorkflowRevision {
            revision_id: WorkflowRevisionId::new(),
            workflow_id: WorkflowId::new(),
            workspace_id: WorkspaceId::new(),
            revision_number: 1,
            nodes,
            transitions,
            loop_policy,
            created_at: crate::time::now(),
        }
    }

    #[test]
    fn rejects_missing_start_node() {
        let rev = make_revision(
            vec![make_node(2, WorkflowNodeKind::End, "end", false)],
            vec![],
            None,
        );
        let report = rev.validate();
        assert!(report.errors.iter().any(|e| e.code == "missing_start_node"));
    }

    #[test]
    fn rejects_duplicate_node_id() {
        let node = make_node(1, WorkflowNodeKind::Agent, "a", true);
        let rev = make_revision(vec![node.clone(), node], vec![], None);
        let report = rev.validate();
        assert!(report.errors.iter().any(|e| e.code == "duplicate_node_id"));
    }

    #[test]
    fn rejects_transition_to_missing_node() {
        let rev = make_revision(
            vec![make_node(1, WorkflowNodeKind::Agent, "a", true)],
            vec![make_transition(1, 99)],
            None,
        );
        let report = rev.validate();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == "transition_to_missing_node")
        );
    }

    #[test]
    fn rejects_unreachable_required_node() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "start", true),
                make_node(2, WorkflowNodeKind::Skill, "unreachable", false),
                make_node(3, WorkflowNodeKind::End, "end", false),
            ],
            vec![make_transition(1, 3)],
            None,
        );
        let mut rev = rev;
        rev.nodes[1].is_required = true;
        let report = rev.validate();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == "unreachable_required_node")
        );
    }

    #[test]
    fn rejects_parallel_without_join() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "start", true),
                make_node(2, WorkflowNodeKind::Parallel, "parallel", false),
                make_node(3, WorkflowNodeKind::End, "end", false),
            ],
            vec![make_transition(1, 2), make_transition(2, 3)],
            None,
        );
        let report = rev.validate();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == "parallel_without_join")
        );
    }

    #[test]
    fn accepts_parallel_with_join() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "start", true),
                make_node(2, WorkflowNodeKind::Parallel, "parallel", false),
                make_node(3, WorkflowNodeKind::Join, "join", false),
                make_node(4, WorkflowNodeKind::End, "end", false),
            ],
            vec![
                make_transition(1, 2),
                make_transition(2, 3),
                make_transition(3, 4),
            ],
            None,
        );
        let report = rev.validate();
        assert!(report.is_valid(), "{:?}", report.errors);
    }

    #[test]
    fn rejects_cycle_without_loop_policy() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "a", true),
                make_node(2, WorkflowNodeKind::Skill, "b", false),
            ],
            vec![make_transition(1, 2), make_transition(2, 1)],
            None,
        );
        let report = rev.validate();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == "cycle_without_loop_policy")
        );
    }

    #[test]
    fn accepts_cycle_with_loop_policy() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "a", true),
                make_node(2, WorkflowNodeKind::Skill, "b", false),
            ],
            vec![make_transition(1, 2), make_transition(2, 1)],
            Some(LoopPolicy { max_iterations: 5 }),
        );
        let report = rev.validate();
        assert!(report.is_valid(), "{:?}", report.errors);
    }

    #[test]
    fn rejects_zero_loop_iterations() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "a", true),
                make_node(2, WorkflowNodeKind::End, "end", false),
            ],
            vec![make_transition(1, 2)],
            Some(LoopPolicy { max_iterations: 0 }),
        );
        let report = rev.validate();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == "loop_policy_zero_iterations")
        );
    }

    #[test]
    fn accepts_valid_workflow() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Agent, "start", true),
                make_node(2, WorkflowNodeKind::Skill, "step", false),
                make_node(3, WorkflowNodeKind::End, "end", false),
            ],
            vec![make_transition(1, 2), make_transition(2, 3)],
            None,
        );
        let report = rev.validate();
        assert!(report.is_valid(), "{:?}", report.errors);
    }

    #[test]
    fn returns_all_errors_not_just_first() {
        let rev = make_revision(
            vec![
                make_node(1, WorkflowNodeKind::Parallel, "p", true),
                make_node(1, WorkflowNodeKind::End, "dup", false),
            ],
            vec![make_transition(1, 99)],
            None,
        );
        let report = rev.validate();
        assert!(report.errors.len() >= 3);
    }
}
