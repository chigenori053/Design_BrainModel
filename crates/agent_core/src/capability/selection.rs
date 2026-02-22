use std::collections::BTreeMap;

use core_types::ObjectiveVector;
use memory_space::{DesignState, StateId};

const SELECTION_W1_QUALITY: f64 = 0.60;
const SELECTION_W2_PRESSURE: f64 = 0.25;
const SELECTION_W3_STABILITY: f64 = 0.15;
const SELECTION_PRESSURE_LAMBDA: f64 = 1.0;
const SELECTION_STABILITY_EPS: f64 = 0.05;

pub fn soft_front_rank(
    candidates: Vec<(DesignState, ObjectiveVector)>,
    temperature: f64,
) -> Vec<(DesignState, ObjectiveVector)> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut dedup: BTreeMap<StateId, (DesignState, ObjectiveVector)> = BTreeMap::new();
    for (state, obj) in candidates {
        dedup.entry(state.id).or_insert((state, obj));
    }
    let entries: Vec<(DesignState, ObjectiveVector)> = dedup.into_values().collect();
    let objs: Vec<ObjectiveVector> = entries.iter().map(|(_, o)| o.clone()).collect();
    let n = objs.len();
    let scores = crate::engine::normalization::soft_dominance_scores(&objs, temperature);
    let selection_scores = selection_scores_for_objs(&objs);

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&li, &ri| {
        scores[ri]
            .partial_cmp(&scores[li])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                selection_scores[ri]
                    .partial_cmp(&selection_scores[li])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| entries[li].0.id.cmp(&entries[ri].0.id))
    });

    order
        .into_iter()
        .map(|idx| entries[idx].clone())
        .collect::<Vec<_>>()
}

fn selection_score(quality: f64, pressure: f64, stability: f64) -> f64 {
    SELECTION_W1_QUALITY * quality
        + SELECTION_W2_PRESSURE * pressure
        + SELECTION_W3_STABILITY * stability
}

fn selection_scores_for_objs(objs: &[ObjectiveVector]) -> Vec<f64> {
    if objs.is_empty() {
        return Vec::new();
    }
    let n = objs.len();
    let centroid = ObjectiveVector {
        f_struct: objs.iter().map(|o| o.f_struct).sum::<f64>() / n as f64,
        f_field: objs.iter().map(|o| o.f_field).sum::<f64>() / n as f64,
        f_risk: objs.iter().map(|o| o.f_risk).sum::<f64>() / n as f64,
        f_shape: objs.iter().map(|o| o.f_shape).sum::<f64>() / n as f64,
    };

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let local_distance = if n <= 1 {
            0.0
        } else {
            let mut min_d = f64::INFINITY;
            for j in 0..n {
                if i == j {
                    continue;
                }
                min_d = min_d.min(crate::engine::normalization::objective_distance(&objs[i], &objs[j]));
            }
            if min_d.is_finite() { min_d } else { 0.0 }
        };
        let global_distance = crate::engine::normalization::objective_distance(&objs[i], &centroid);
        let integrated_distance = 0.5 * local_distance + 0.5 * global_distance;
        let pressure = (-SELECTION_PRESSURE_LAMBDA * integrated_distance)
            .exp()
            .clamp(0.0, 1.0);
        let stability =
            crate::stability::stable_flag(local_distance, global_distance, SELECTION_STABILITY_EPS);
        let quality = crate::scalar_score(&objs[i]).clamp(0.0, 1.0);
        out.push(selection_score(quality, pressure, stability));
    }
    out
}
