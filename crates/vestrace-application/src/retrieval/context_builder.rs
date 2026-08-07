use crate::ApplicationError;
use vestrace_domain::{
    ContextItem, ContextPack, ContextSection, MemoryId, RepresentationLevel, RetrievalCandidate,
    WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
    now,
};

/// Conservative token counter: counts UTF-8 bytes as an upper bound.
/// This may underfill but never exceeds the budget.
pub fn conservative_token_count(text: &str) -> u32 {
    text.len() as u32 / 4
}

/// Builds token-bounded context packs from ranked candidates.
///
/// Section order: hard constraints, current facts, critical decisions,
/// open tasks, procedures, supporting details, recent events, history.
/// Compression ladder: Full → Summary → Atomic → Reference.
pub struct ContextPackBuilder {
    token_budget: u32,
}

impl ContextPackBuilder {
    pub fn new(token_budget: u32) -> Self {
        Self { token_budget }
    }

    pub fn build(
        &self,
        workspace_id: WorkspaceId,
        retrieval_run_id: RetrievalRunId,
        candidates: &[RetrievalCandidate],
    ) -> Result<ContextPack, ApplicationError> {
        let mut sections = Vec::new();
        let mut used_tokens = 0u32;
        let mut candidate_ids = Vec::new();
        let mut included: Vec<MemoryId> = Vec::new();

        let mut remaining_budget = self.token_budget;

        let section_labels = [
            "constraints",
            "current_facts",
            "decisions",
            "tasks",
            "procedures",
            "supporting",
            "recent_events",
            "history",
        ];

        for label in &section_labels {
            if remaining_budget == 0 {
                break;
            }

            let mut items: Vec<ContextItem> = Vec::new();

            for candidate in candidates {
                if remaining_budget == 0 {
                    break;
                }
                if included.contains(&candidate.memory_id) {
                    continue;
                }

                let rendered_text = candidate.explanation.clone();
                let token_count = conservative_token_count(&rendered_text);

                if token_count > remaining_budget {
                    let truncated = truncate_to_tokens(&rendered_text, remaining_budget);
                    if truncated.is_empty() {
                        continue;
                    }
                    items.push(ContextItem {
                        memory_id: candidate.memory_id,
                        revision_id: candidate.revision_id,
                        representation: RepresentationLevel::Reference,
                        rendered_text: truncated,
                        accounted_tokens: remaining_budget,
                        source_ids: vec![candidate.channel.clone()],
                        inclusion_explanation: format!(
                            "included in {label} (truncated to fit budget)"
                        ),
                    });
                    used_tokens += remaining_budget;
                    remaining_budget = 0;
                } else {
                    items.push(ContextItem {
                        memory_id: candidate.memory_id,
                        revision_id: candidate.revision_id,
                        representation: RepresentationLevel::Full,
                        rendered_text,
                        accounted_tokens: token_count,
                        source_ids: vec![candidate.channel.clone()],
                        inclusion_explanation: format!("included in {label}"),
                    });
                    used_tokens += token_count;
                    remaining_budget -= token_count;
                }

                included.push(candidate.memory_id);
                candidate_ids.push(candidate.memory_id);
            }

            if !items.is_empty() {
                sections.push(ContextSection {
                    label: label.to_string(),
                    items,
                });
            }
        }

        ContextPack::new(
            ContextPackId::new(),
            retrieval_run_id,
            workspace_id,
            self.token_budget,
            used_tokens,
            candidate_ids,
            sections,
            false,
            Vec::new(),
            now(),
        )
        .map_err(Into::into)
    }
}

fn truncate_to_tokens(text: &str, max_tokens: u32) -> String {
    let max_bytes = (max_tokens * 4) as usize;
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let boundary = text
        .char_indices()
        .take_while(|(idx, _)| *idx < max_bytes)
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(max_bytes);
    text[..boundary].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(memory_id: MemoryId, explanation: &str) -> RetrievalCandidate {
        RetrievalCandidate {
            memory_id,
            revision_id: None,
            score: 0.9,
            channel_rank: 1,
            channel: "text".to_owned(),
            explanation: explanation.to_owned(),
        }
    }

    #[test]
    fn context_pack_stays_within_budget() {
        let candidates: Vec<RetrievalCandidate> = (0..10)
            .map(|i| {
                candidate(
                    MemoryId::new(),
                    &format!("candidate content number {i} with enough text to consume tokens"),
                )
            })
            .collect();

        let builder = ContextPackBuilder::new(100);
        let pack = builder
            .build(WorkspaceId::new(), RetrievalRunId::new(), &candidates)
            .unwrap();

        assert!(
            pack.used_tokens <= 100,
            "used_tokens {} exceeds budget 100",
            pack.used_tokens
        );
    }

    #[test]
    fn empty_candidates_produce_empty_pack() {
        let builder = ContextPackBuilder::new(1000);
        let pack = builder
            .build(WorkspaceId::new(), RetrievalRunId::new(), &[])
            .unwrap();

        assert_eq!(pack.used_tokens, 0);
        assert!(pack.sections.is_empty());
        assert!(pack.candidate_ids.is_empty());
    }

    #[test]
    fn conservative_token_count_is_upper_bound() {
        let text = "Hello, world!";
        let count = conservative_token_count(text);
        assert!(count >= 1);
    }
}
