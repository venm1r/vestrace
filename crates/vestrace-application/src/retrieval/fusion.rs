use std::collections::HashMap;
use vestrace_domain::{RetrievalCandidate, id::MemoryId};

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

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
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
            revision_id: None,
            score,
            channel_rank: rank,
            channel: channel.to_owned(),
            explanation: String::new(),
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
        let channel = vec![candidate(id, 0.9, "text", 1), candidate(id, 0.9, "text", 2)];

        let fused = reciprocal_rank_fusion(&[channel], 60.0);
        assert_eq!(fused.len(), 1);
    }
}
