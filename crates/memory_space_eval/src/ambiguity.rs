pub fn estimate_ambiguity(resonance: f64, memory_density: f64) -> f64 {
    let signal = resonance.clamp(0.0, 1.0);
    let density_penalty = (memory_density / (1.0 + memory_density)).clamp(0.0, 1.0);
    (1.0 - signal * (1.0 - 0.5 * density_penalty)).clamp(0.0, 1.0)
}
