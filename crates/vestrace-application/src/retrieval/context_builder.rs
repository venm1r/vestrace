use crate::ApplicationError;
use vestrace_domain::{
    ContextItem, ContextPack, ContextSection, EvidenceRef, MemoryId, MemoryKind, PrincipalId,
    RepresentationLevel, RetrievalCandidate, TimePerspective, WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
    now,
};

/// Conservative token counter: counts UTF-8 bytes as an upper bound.
/// Rounding up keeps every non-empty rendered value accounted for.
pub fn conservative_token_count(text: &str) -> u32 {
    if text.is_empty() {
        0
    } else {
        ((text.len() as u32).saturating_add(3)) / 4
    }
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
        actor: PrincipalId,
        retrieval_run_id: RetrievalRunId,
        candidates: &[RetrievalCandidate],
    ) -> Result<ContextPack, ApplicationError> {
        self.build_with_temporal_perspective(
            workspace_id,
            actor,
            retrieval_run_id,
            TimePerspective::Current,
            candidates,
        )
    }

    pub fn build_with_temporal_perspective(
        &self,
        workspace_id: WorkspaceId,
        actor: PrincipalId,
        retrieval_run_id: RetrievalRunId,
        temporal_perspective: TimePerspective,
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

                if section_for_kind(candidate.kind) != *label {
                    continue;
                }

                let Some((representation, rendered_text, token_count)) =
                    render_representation(candidate, remaining_budget)
                else {
                    continue;
                };

                items.push(ContextItem {
                    memory_id: candidate.memory_id,
                    revision_id: candidate.revision_id,
                    memory_status: candidate.memory_status,
                    revision_number: candidate.revision_number,
                    valid_from: candidate.valid_from,
                    valid_until: candidate.valid_until,
                    revision_created_at: candidate.revision_created_at,
                    source_generation: candidate.source_generation,
                    representation,
                    rendered_text,
                    accounted_tokens: token_count,
                    provenance_refs: vec![EvidenceRef::MemoryRevisionRef {
                        memory_id: candidate.memory_id,
                        revision_id: candidate.revision_id,
                    }],
                    inclusion_explanation: format!("included in {label} as {representation:?}"),
                    source_classification: Some(candidate.channel.clone()),
                });
                used_tokens += token_count;
                remaining_budget -= token_count;

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
            actor,
            temporal_perspective,
            self.token_budget,
            used_tokens,
            candidate_ids,
            sections,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "v1".to_string(),
            true,
            now(),
        )
        .map_err(Into::into)
    }
}

fn section_for_kind(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Constraint => "constraints",
        MemoryKind::Fact | MemoryKind::Preference => "current_facts",
        MemoryKind::Decision => "decisions",
        MemoryKind::Task => "tasks",
        MemoryKind::Procedure => "procedures",
        MemoryKind::Outcome | MemoryKind::Summary => "supporting",
        MemoryKind::Observation => "recent_events",
    }
}

fn render_representation(
    candidate: &RetrievalCandidate,
    remaining_budget: u32,
) -> Option<(RepresentationLevel, String, u32)> {
    let source = if candidate.content.is_empty() {
        candidate.explanation.as_str()
    } else {
        candidate.content.as_str()
    };

    let summary = first_sentence(source);
    let atomic = first_atomic(source);
    let reference = format!(
        "memory={} revision={}",
        candidate.memory_id, candidate.revision_id
    );

    for (representation, text) in [
        (RepresentationLevel::Full, source.to_owned()),
        (RepresentationLevel::Summary, summary),
        (RepresentationLevel::Atomic, atomic),
        (RepresentationLevel::Reference, reference.clone()),
    ] {
        if text.is_empty() || conservative_token_count(&text) > remaining_budget {
            continue;
        }

        return Some((
            representation,
            text.clone(),
            conservative_token_count(&text),
        ));
    }

    let truncated_reference = truncate_to_tokens(&reference, remaining_budget);
    if truncated_reference.is_empty() {
        None
    } else {
        Some((
            RepresentationLevel::Reference,
            truncated_reference.clone(),
            conservative_token_count(&truncated_reference),
        ))
    }
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    let end = trimmed.char_indices().find_map(|(index, character)| {
        (matches!(character, '.' | '!' | '?')).then_some(index + character.len_utf8())
    });

    end.map_or_else(
        || trimmed.to_owned(),
        |end| trimmed[..end].trim().to_owned(),
    )
}

fn first_atomic(text: &str) -> String {
    let trimmed = text.trim();
    let first_clause = trimmed
        .split_once(';')
        .or_else(|| trimmed.split_once('\n'))
        .or_else(|| trimmed.split_once(','))
        .map(|(first, _)| first.trim())
        .filter(|first| !first.is_empty());

    first_clause.map_or_else(|| first_sentence(trimmed), ToOwned::to_owned)
}

fn truncate_to_tokens(text: &str, max_tokens: u32) -> String {
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut boundary = 0;
    for (index, character) in text.char_indices() {
        let next_boundary = index + character.len_utf8();
        if next_boundary > max_bytes {
            break;
        }
        boundary = next_boundary;
    }
    text[..boundary].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::{EvidenceRef, MemoryKind, MemoryStatus};

    fn candidate(memory_id: MemoryId, explanation: &str) -> RetrievalCandidate {
        RetrievalCandidate {
            memory_id,
            revision_id: vestrace_domain::id::MemoryRevisionId::new(),
            kind: MemoryKind::Fact,
            memory_status: MemoryStatus::Active,
            revision_number: 1,
            content: explanation.to_owned(),
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
            score: 0.9,
            channel_rank: 1,
            channel: "text".to_owned(),
            explanation: explanation.to_owned(),
            conflict_ids: Vec::new(),
        }
    }

    fn candidate_with_kind(
        memory_id: MemoryId,
        revision_id: vestrace_domain::id::MemoryRevisionId,
        kind: MemoryKind,
        content: &str,
    ) -> RetrievalCandidate {
        RetrievalCandidate {
            memory_id,
            revision_id,
            kind,
            memory_status: MemoryStatus::Active,
            revision_number: 1,
            content: content.to_owned(),
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
            score: 0.9,
            channel_rank: 1,
            channel: "text".to_owned(),
            explanation: "selected because it matched".to_owned(),
            conflict_ids: Vec::new(),
        }
    }

    #[test]
    fn routes_candidates_and_preserves_revision_provenance() {
        let task_id = MemoryId::new();
        let task_revision = vestrace_domain::id::MemoryRevisionId::new();
        let constraint_id = MemoryId::new();
        let constraint_revision = vestrace_domain::id::MemoryRevisionId::new();
        let decision_id = MemoryId::new();
        let decision_revision = vestrace_domain::id::MemoryRevisionId::new();

        let pack = ContextPackBuilder::new(1000)
            .build(
                WorkspaceId::new(),
                PrincipalId::new(),
                RetrievalRunId::new(),
                &[
                    candidate_with_kind(task_id, task_revision, MemoryKind::Task, "task"),
                    candidate_with_kind(
                        constraint_id,
                        constraint_revision,
                        MemoryKind::Constraint,
                        "constraint",
                    ),
                    candidate_with_kind(
                        decision_id,
                        decision_revision,
                        MemoryKind::Decision,
                        "decision",
                    ),
                ],
            )
            .unwrap();

        let labels: Vec<&str> = pack
            .sections
            .iter()
            .map(|section| section.label.as_str())
            .collect();
        assert_eq!(labels, vec!["constraints", "decisions", "tasks"]);

        let item = &pack.sections[0].items[0];
        assert_eq!(
            item.provenance_refs,
            vec![EvidenceRef::MemoryRevisionRef {
                memory_id: constraint_id,
                revision_id: constraint_revision,
            }]
        );
        assert!(item.inclusion_explanation.contains("constraints"));
        assert!(item.inclusion_explanation.contains("Full"));
    }

    #[test]
    fn uses_deterministic_representation_ladder() {
        let build_one = |content: &str, budget: u32| {
            let candidate = candidate_with_kind(
                MemoryId::new(),
                vestrace_domain::id::MemoryRevisionId::new(),
                MemoryKind::Fact,
                content,
            );
            ContextPackBuilder::new(budget)
                .build(
                    WorkspaceId::new(),
                    PrincipalId::new(),
                    RetrievalRunId::new(),
                    &[candidate],
                )
                .unwrap()
                .sections
                .into_iter()
                .next()
                .unwrap()
                .items
                .into_iter()
                .next()
                .unwrap()
        };

        assert_eq!(
            build_one("short content", 100).representation,
            RepresentationLevel::Full
        );
        assert_eq!(
            build_one("summary sentence. additional detail follows here", 5).representation,
            RepresentationLevel::Summary
        );
        assert_eq!(
            build_one("atomic clause; additional detail follows here", 4).representation,
            RepresentationLevel::Atomic
        );
        assert_eq!(
            build_one(
                "a very long content that cannot fit any textual representation",
                1
            )
            .representation,
            RepresentationLevel::Reference
        );
    }

    #[test]
    fn context_item_preserves_exact_revision_content() {
        let memory_id = MemoryId::new();
        let revision_id = vestrace_domain::id::MemoryRevisionId::new();
        let actor = PrincipalId::new();
        let candidate = RetrievalCandidate {
            memory_id,
            revision_id,
            kind: MemoryKind::Fact,
            memory_status: MemoryStatus::Active,
            revision_number: 1,
            content: "hydrated revision content".to_owned(),
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
            score: 0.9,
            channel_rank: 1,
            channel: "text".to_owned(),
            explanation: "diagnostic explanation".to_owned(),
            conflict_ids: Vec::new(),
        };

        let pack = ContextPackBuilder::new(100)
            .build(
                WorkspaceId::new(),
                actor,
                RetrievalRunId::new(),
                &[candidate],
            )
            .unwrap();
        let item = &pack.sections[0].items[0];

        assert_eq!(item.revision_id, revision_id);
        assert_eq!(item.rendered_text, "hydrated revision content");
        assert!(
            item.inclusion_explanation
                .contains("included in current_facts")
        );
        assert_eq!(pack.actor, actor);
    }

    #[test]
    fn perspective_aware_build_preserves_temporal_metadata() {
        let at = vestrace_domain::now();
        let candidate = RetrievalCandidate {
            memory_id: MemoryId::new(),
            revision_id: vestrace_domain::id::MemoryRevisionId::new(),
            kind: MemoryKind::Fact,
            memory_status: MemoryStatus::Superseded,
            revision_number: 3,
            content: "historical fact".to_owned(),
            valid_from: Some(at),
            valid_until: None,
            revision_created_at: at,
            source_generation: 7,
            score: 0.9,
            channel_rank: 1,
            channel: "text".to_owned(),
            explanation: "historical match".to_owned(),
            conflict_ids: Vec::new(),
        };

        let pack = ContextPackBuilder::new(100)
            .build_with_temporal_perspective(
                WorkspaceId::new(),
                PrincipalId::new(),
                RetrievalRunId::new(),
                TimePerspective::AllHistory,
                &[candidate],
            )
            .unwrap();
        let item = &pack.sections[0].items[0];

        assert_eq!(pack.temporal_perspective, TimePerspective::AllHistory);
        assert_eq!(item.memory_status, MemoryStatus::Superseded);
        assert_eq!(item.revision_number, 3);
        assert_eq!(item.valid_from, Some(at));
        assert_eq!(item.source_generation, 7);
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
            .build(
                WorkspaceId::new(),
                PrincipalId::new(),
                RetrievalRunId::new(),
                &candidates,
            )
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
            .build(
                WorkspaceId::new(),
                PrincipalId::new(),
                RetrievalRunId::new(),
                &[],
            )
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
