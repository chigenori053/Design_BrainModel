use memory_space_core::MemoryCandidate;

pub fn rank_candidates(mut candidates: Vec<MemoryCandidate>, k: usize) -> Vec<MemoryCandidate> {
    candidates.sort_by(|lhs, rhs| {
        rhs.resonance
            .total_cmp(&lhs.resonance)
            .then_with(|| lhs.memory_id.cmp(&rhs.memory_id))
    });

    candidates.truncate(k);
    candidates
}
