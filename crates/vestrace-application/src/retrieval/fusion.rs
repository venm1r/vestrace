use crate::ApplicationError;
use std::collections::HashMap;
use vestrace_domain::{CorpusGenerationId, RetrievalCandidate, id::MemoryId};

pub fn reciprocal_rank_fusion_pinned(
    channel_results: &[Vec<RetrievalCandidate>],
    k: f32,
    corpus_generation_id: CorpusGenerationId,
) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
    for candidate in channel_results.iter().flatten() {
        if candidate.corpus_generation_id != corpus_generation_id {
            return Err(ApplicationError::Policy(format!(
                "retrieval candidate generation {} does not match requested generation {}",
                candidate.corpus_generation_id, corpus_generation_id
            )));
        }
    }
    Ok(reciprocal_rank_fusion(channel_results, k))
}

/// Reciprocal Rank Fusion: combines ranked channel results into a single
/// ranked list. Each channel's rank starts at 1. Score is:
///
/// ```text
/// score += channel_weight / (k + rank)
/// ```
///
/// Candidates are deduplicated by `memory_id` before accumulating scores.
pub fn reciprocal_rank_fusion(
    channel_results: &[Vec<RetrievalCandidate>],
    k: f32,
) -> Vec<RetrievalCandidate> {
    let mut scores: HashMap<MemoryId, f32> = HashMap::new();
    let mut best: HashMap<MemoryId, RetrievalCandidate> = HashMap::new();

    for channel in channel_results {
        for (rank_idx, candidate) in channel.iter().enumerate() {
            let rank = (rank_idx + 1) as f32;
            let contribution = 1.0 / (k + rank);
            *scores.entry(candidate.memory_id).or_insert(0.0) += contribution;

            best.entry(candidate.memory_id)
                .and_modify(|existing| {
                    if candidate.score > existing.score {
                        *existing = candidate.clone();
                    }
                })
                .or_insert_with(|| candidate.clone());
        }
    }

    let mut fused: Vec<RetrievalCandidate> = best
        .into_iter()
        .map(|(memory_id, mut candidate)| {
            candidate.score = *scores.get(&memory_id).unwrap_or(&0.0);
            candidate.channel = "fused".to_owned();
            candidate
        })
        .collect();

    // Score descending, then memory id ascending.
    //
    // The tie-break is not cosmetic. `best` is a `HashMap`, whose iteration
    // order is randomised per process, and sorting on score alone leaves ties
    // in whatever order iteration produced. Two runs of the same server over
    // the same data could therefore rank tied candidates differently — and a
    // test inside one process would never show it, because the hasher seed is
    // fixed for that process's lifetime.
    //
    // Ties are not rare here: reciprocal rank fusion assigns identical
    // contributions to identical ranks, so any two candidates appearing at
    // mirrored positions across channels tie exactly.
    //
    // With this the order is a total function of the data, which is what
    // "deterministic and auditable" requires: the same inputs justify the same
    // ranking, and a ranking can be re-derived from a journal.
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.memory_id.as_uuid().cmp(&b.memory_id.as_uuid()))
    });

    for (idx, candidate) in fused.iter_mut().enumerate() {
        candidate.channel_rank = (idx + 1) as u32;
    }

    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(memory_id: MemoryId, score: f32, channel: &str, rank: u32) -> RetrievalCandidate {
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
            channel_rank: rank,
            channel: channel.to_owned(),
            explanation: String::new(),
            conflict_ids: Vec::new(),
        }
    }

    #[test]
    fn candidate_in_two_channels_outranks_single_channel_candidate() {
        let shared = MemoryId::new();
        let other_a = MemoryId::new();
        let other_b = MemoryId::new();

        let channel_a = vec![
            candidate(shared, 0.9, "text", 1),
            candidate(other_a, 0.8, "text", 2),
        ];
        let channel_b = vec![
            candidate(shared, 0.85, "vector", 1),
            candidate(other_b, 0.7, "vector", 2),
        ];

        let fused = reciprocal_rank_fusion(&[channel_a, channel_b], 60.0);

        assert_eq!(fused[0].memory_id, shared);
        assert_eq!(fused[0].channel_rank, 1);
    }

    #[test]
    fn single_channel_preserves_order() {
        let a = MemoryId::new();
        let b = MemoryId::new();
        let c = MemoryId::new();

        let channel = vec![
            candidate(a, 0.9, "text", 1),
            candidate(b, 0.8, "text", 2),
            candidate(c, 0.7, "text", 3),
        ];

        let fused = reciprocal_rank_fusion(&[channel], 60.0);

        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].memory_id, a);
        assert_eq!(fused[1].memory_id, b);
        assert_eq!(fused[2].memory_id, c);
    }

    #[test]
    fn empty_channels_produce_empty_result() {
        let fused = reciprocal_rank_fusion(&[], 60.0);
        assert!(fused.is_empty());
    }

    #[test]
    fn duplicate_candidates_within_a_channel_are_deduplicated() {
        let id = MemoryId::new();
        let first = candidate(id, 0.9, "text", 1);
        let mut duplicate = first.clone();
        duplicate.channel_rank = 2;
        let channel = vec![first, duplicate];

        let fused = reciprocal_rank_fusion(&[channel], 60.0);
        assert_eq!(fused.len(), 1);
    }
}
