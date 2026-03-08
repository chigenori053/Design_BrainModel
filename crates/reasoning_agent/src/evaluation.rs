use memory_space_api::{MemoryEngine, MemoryQuery};
use memory_space_complex::ComplexField;
use memory_space_index::MemoryIndex;

pub fn evaluate<M: MemoryIndex>(
    state: &ComplexField,
    memory: &MemoryEngine<M>,
    top_k: usize,
) -> f64 {
    if state.data.is_empty() || top_k == 0 {
        return 0.0;
    }

    let recalled = memory.query(MemoryQuery {
        vector: state.clone(),
        context: None,
        k: top_k,
    });
    if recalled.is_empty() {
        return 0.0;
    }

    let mean = recalled
        .iter()
        .map(|candidate| candidate.resonance)
        .sum::<f64>()
        / recalled.len() as f64;
    mean.clamp(0.0, 1.0)
}
