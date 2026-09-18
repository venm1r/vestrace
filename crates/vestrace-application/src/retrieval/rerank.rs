use vestrace_domain::{RetrievalCandidate, ScoreComponents};

/// Deterministic, provider-free reranker. Applies domain policy components
/// to fused candidates and produces a final ranked order.
///
/// Components:
/// - scope_match: 1.0 (scope filtering is done at channel level)
/// - kind_match: 1.0 if no kind filter, else 1.0 if matched
/// - importance: from candidate score (0.0–1.0 mapped)
/// - confidence: from candidate score
/// - recency: 0.5 (no timestamp available in candidate; neutral)
/// - provenance_quality: 0.5 (no provenance in candidate; neutral)
/// - redundancy_penalty: near-duplicate memory IDs get penalized
pub fn rerank(candidates: Vec<RetrievalCandidate>) -> Vec<RankedCandidate> {
    let mut seen_kinds: Vec<String> = Vec::new();
    let mut ranked: Vec<RankedCandidate> = Vec::new();

    let mut sorted = candidates;
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (idx, candidate) in sorted.iter().enumerate() {
        let fused_rank = candidate.score;
        let importance = candidate.score.clamp(0.0, 1.0);
        let confidence = candidate.score.clamp(0.0, 1.0);

        let redundancy_penalty = if seen_kinds.iter().any(|k| k == &candidate.channel) {
            0.1
        } else {
            0.0
        };

        let components = ScoreComponents {
            fused_rank,
            scope_match: 1.0,
            kind_match: 1.0,
            importance,
            confidence,
            recency: 0.5,
            provenance_quality: 0.5,
            redundancy_penalty,
        };

        let final_score =
            components.fused_rank + components.importance * 0.15 + components.confidence * 0.10
                - components.redundancy_penalty;

        seen_kinds.push(candidate.channel.clone());

        ranked.push(RankedCandidate {
            candidate: RetrievalCandidate {
                score: final_score,
                channel_rank: (idx + 1) as u32,
                ..candidate.clone()
            },
            components,
        });
    }

    ranked.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (idx, ranked) in ranked.iter_mut().enumerate() {
        ranked.candidate.channel_rank = (idx + 1) as u32;
    }

    ranked
}

#[derive(Clone, Debug, PartialEq)]
pub struct RankedCandidate {
    pub candidate: RetrievalCandidate,
    pub components: ScoreComponents,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::MemoryId;

    fn candidate(memory_id: MemoryId, score: f32, channel: &str) -> RetrievalCandidate {
        RetrievalCandidate {
            memory_id,
            revision_id: vestrace_domain::id::MemoryRevisionId::new(),
            kind: vestrace_domain::MemoryKind::Fact,
            memory_status: vestrace_domain::MemoryStatus::Active,
            revision_number: 1,
            content: String::new(),
            classification: None,
            valid_from: None,
            valid_until: None,
            revision_created_at: vestrace_domain::now(),
            source_generation: 1,
            corpus_generation_id: vestrace_domain::CorpusGenerationId::new(),
            score,
            channel_rank: 0,
            channel: channel.to_owned(),
            explanation: String::new(),
            conflict_ids: Vec::new(),
        }
    }

    #[test]
    fn higher_score_ranks_first() {
        let a = MemoryId::new();
        let b = MemoryId::new();

        let ranked = rerank(vec![candidate(a, 0.9, "text"), candidate(b, 0.5, "text")]);

        assert_eq!(ranked[0].candidate.memory_id, a);
        assert_eq!(ranked[1].candidate.memory_id, b);
    }

    #[test]
    fn empty_input_produces_empty_output() {
        let ranked = rerank(Vec::new());
        assert!(ranked.is_empty());
    }

    #[test]
    fn ranked_candidates_have_sequential_ranks() {
        let ids: Vec<MemoryId> = (0..3).map(|_| MemoryId::new()).collect();
        let candidates = ids.iter().map(|id| candidate(*id, 0.5, "text")).collect();

        let ranked = rerank(candidates);
        assert_eq!(ranked[0].candidate.channel_rank, 1);
        assert_eq!(ranked[1].candidate.channel_rank, 2);
        assert_eq!(ranked[2].candidate.channel_rank, 3);
    }
}
