use std::io;

use core_types::ObjectiveVector;

use crate::holographic_store::{HolographicVectorStore, MemoryEntry};

const TAU_MEM_MIN: f64 = 1e-9;
const DELTA_EPS: f64 = 1e-12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterferenceMode {
    Disabled,
    Contractive,
    Repulsive,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryInterferenceTelemetry {
    pub avg_tau_mem: f64,
    pub avg_delta_norm: f64,
    pub memory_hit_rate: f64,
    pub samples: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct InterferenceStepStats {
    tau_mem: f64,
    delta_norm: f64,
    hit_rate: f64,
}

#[derive(Debug)]
pub struct MemorySpace {
    store: HolographicVectorStore,
    decay: f64,
    lambda: f64,
    mode: InterferenceMode,
    window: usize,
    next_id: u64,
    entries_cache: Vec<MemoryEntry>,
    stats_sum_tau: f64,
    stats_sum_delta: f64,
    stats_sum_hit_rate: f64,
    stats_count: usize,
}

impl MemorySpace {
    pub fn new(
        store: HolographicVectorStore,
        decay: f64,
        lambda: f64,
        mode: InterferenceMode,
        window: usize,
    ) -> io::Result<Self> {
        let next_id = store.entry_count()?;
        let entries_cache = store.entries().unwrap_or_default();
        Ok(Self {
            store,
            decay: decay.clamp(1e-12, 0.999_999),
            lambda: lambda.max(0.0),
            mode,
            window: window.max(1),
            next_id,
            entries_cache,
            stats_sum_tau: 0.0,
            stats_sum_delta: 0.0,
            stats_sum_hit_rate: 0.0,
            stats_count: 0,
        })
    }

    pub fn mode(&self) -> InterferenceMode {
        self.mode
    }

    pub fn apply_interference(&mut self, base: &ObjectiveVector) -> ObjectiveVector {
        let (adjusted, step) = self.apply_interference_with_stats(base);
        self.stats_sum_tau += step.tau_mem;
        self.stats_sum_delta += step.delta_norm;
        self.stats_sum_hit_rate += step.hit_rate;
        self.stats_count = self.stats_count.saturating_add(1);
        adjusted
    }

    pub fn take_telemetry(&mut self) -> MemoryInterferenceTelemetry {
        if self.stats_count == 0 {
            return MemoryInterferenceTelemetry::default();
        }
        let denom = self.stats_count as f64;
        let out = MemoryInterferenceTelemetry {
            avg_tau_mem: self.stats_sum_tau / denom,
            avg_delta_norm: self.stats_sum_delta / denom,
            memory_hit_rate: self.stats_sum_hit_rate / denom,
            samples: self.stats_count,
        };
        self.stats_sum_tau = 0.0;
        self.stats_sum_delta = 0.0;
        self.stats_sum_hit_rate = 0.0;
        self.stats_count = 0;
        out
    }

    pub fn store(&mut self, objective: &ObjectiveVector, depth: usize) -> io::Result<()> {
        if self.mode == InterferenceMode::Disabled {
            return Ok(());
        }
        let entry = MemoryEntry {
            id: self.next_id,
            depth,
            // Use a logical timestamp for deterministic decay behavior.
            timestamp: self.next_id,
            vector: vec![
                objective.f_struct,
                objective.f_field,
                objective.f_risk,
                objective.f_shape,
            ],
        };
        self.store.append(&entry)?;
        self.entries_cache.push(entry);
        self.next_id = self.next_id.saturating_add(1);
        Ok(())
    }

    fn apply_interference_with_stats(
        &self,
        base: &ObjectiveVector,
    ) -> (ObjectiveVector, InterferenceStepStats) {
        if self.mode == InterferenceMode::Disabled {
            return (base.clone(), InterferenceStepStats::default());
        }
        let x = [base.f_struct, base.f_field, base.f_risk, base.f_shape];
        let memory = self.recent_entries();
        if memory.is_empty() {
            return (base.clone(), InterferenceStepStats::default());
        }

        let mut sq_distances = Vec::with_capacity(memory.len());
        for entry in &memory {
            sq_distances.push(squared_l2(&x, &entry.vector));
        }
        let tau = median(sq_distances).max(TAU_MEM_MIN);
        let now = self.next_id;

        let mut weights = Vec::with_capacity(memory.len());
        let mut hit = 0usize;
        for entry in &memory {
            let d2 = squared_l2(&x, &entry.vector);
            let sim = (-(d2 / tau)).exp();
            let age = now.saturating_sub(entry.timestamp) as f64;
            let w = sim * self.decay.powf(age);
            if sim > 0.5 {
                hit += 1;
            }
            weights.push(w);
        }
        let hit_rate = hit as f64 / memory.len() as f64;

        match self.mode {
            InterferenceMode::Contractive => {
                let i_scalar = weights.iter().sum::<f64>();
                let norm = l2_norm(&x);
                if norm <= DELTA_EPS {
                    return (
                        base.clone(),
                        InterferenceStepStats {
                            tau_mem: tau,
                            delta_norm: 0.0,
                            hit_rate,
                        },
                    );
                }
                let direction = [x[0] / norm, x[1] / norm, x[2] / norm, x[3] / norm];
                let scale = self.lambda * i_scalar;
                let adjusted = ObjectiveVector {
                    f_struct: (x[0] - scale * direction[0]).clamp(0.0, 1.0),
                    f_field: (x[1] - scale * direction[1]).clamp(0.0, 1.0),
                    f_risk: (x[2] - scale * direction[2]).clamp(0.0, 1.0),
                    f_shape: (x[3] - scale * direction[3]).clamp(0.0, 1.0),
                };
                (
                    adjusted,
                    InterferenceStepStats {
                        tau_mem: tau,
                        delta_norm: scale.abs(),
                        hit_rate,
                    },
                )
            }
            InterferenceMode::Repulsive => {
                let mut delta = [0.0; 4];
                for (entry, w) in memory.iter().zip(weights.iter().copied()) {
                    let m = vector4(&entry.vector);
                    delta[0] += w * (x[0] - m[0]);
                    delta[1] += w * (x[1] - m[1]);
                    delta[2] += w * (x[2] - m[2]);
                    delta[3] += w * (x[3] - m[3]);
                }
                let delta_norm = l2_norm(&delta);
                if delta_norm <= DELTA_EPS {
                    return (
                        base.clone(),
                        InterferenceStepStats {
                            tau_mem: tau,
                            delta_norm: 0.0,
                            hit_rate,
                        },
                    );
                }
                let unit = [
                    delta[0] / delta_norm,
                    delta[1] / delta_norm,
                    delta[2] / delta_norm,
                    delta[3] / delta_norm,
                ];
                let scale = self.lambda;
                let adjusted = ObjectiveVector {
                    f_struct: (x[0] + scale * unit[0]).clamp(0.0, 1.0),
                    f_field: (x[1] + scale * unit[1]).clamp(0.0, 1.0),
                    f_risk: (x[2] + scale * unit[2]).clamp(0.0, 1.0),
                    f_shape: (x[3] + scale * unit[3]).clamp(0.0, 1.0),
                };
                (
                    adjusted,
                    InterferenceStepStats {
                        tau_mem: tau,
                        delta_norm: delta_norm * scale,
                        hit_rate,
                    },
                )
            }
            InterferenceMode::Disabled => (base.clone(), InterferenceStepStats::default()),
        }
    }

    fn recent_entries(&self) -> Vec<&MemoryEntry> {
        let len = self.entries_cache.len();
        if len == 0 {
            return Vec::new();
        }
        let start = len.saturating_sub(self.window);
        self.entries_cache[start..].iter().collect()
    }
}

fn vector4(v: &[f64]) -> [f64; 4] {
    [
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
        v.get(2).copied().unwrap_or(0.0),
        v.get(3).copied().unwrap_or(0.0),
    ]
}

fn l2_norm(v: &[f64; 4]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>()).sqrt()
}

fn squared_l2(x: &[f64; 4], y: &[f64]) -> f64 {
    let mut sum = 0.0;
    for i in 0..4 {
        let rhs = if i < y.len() { y[i] } else { 0.0 };
        let d = x[i] - rhs;
        sum += d * d;
    }
    sum
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

#[cfg(test)]
mod tests {
    use core_types::ObjectiveVector;

    use crate::{HolographicVectorStore, InterferenceMode, MemorySpace};

    #[test]
    fn memory_space_stores_and_adjusts() {
        let path = std::env::temp_dir().join("memory_space_test_store.bin");
        let store = HolographicVectorStore::open(&path, 4).expect("open");
        let mut memory =
            MemorySpace::new(store, 0.95, 0.02, InterferenceMode::Repulsive, 256).expect("new");
        let base = ObjectiveVector {
            f_struct: 0.8,
            f_field: 0.7,
            f_risk: 0.6,
            f_shape: 0.5,
        };
        memory.store(&base, 1).expect("store");
        let adjusted = memory.apply_interference(&base);
        assert!((0.0..=1.0).contains(&adjusted.f_struct));
        assert!((0.0..=1.0).contains(&adjusted.f_field));
        assert!((0.0..=1.0).contains(&adjusted.f_risk));
        assert!((0.0..=1.0).contains(&adjusted.f_shape));
        let telemetry = memory.take_telemetry();
        assert!(telemetry.samples > 0);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(std::env::temp_dir().join("memory_space_test_store.lock"));
    }
}
