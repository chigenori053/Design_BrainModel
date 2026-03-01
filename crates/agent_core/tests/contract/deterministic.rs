use agent_core::{HvPolicy, Phase1Config, run_phase1_matrix};

const EPS: f64 = 1e-12;

#[derive(Clone)]
struct Case {
    id: String,
    v: [f64; 4],
}

#[test]
fn deterministic_engine_fast_10_seeds_3_runs_hash_and_hv_equal() {
    for seed in 0..10u64 {
        let a = run_once(seed, 1);
        let b = run_once(seed, 2);
        let c = run_once(seed, 3);
        assert_eq!(a.0, b.0);
        assert_eq!(b.0, c.0);
        assert!((a.1 - b.1).abs() <= EPS);
        assert!((b.1 - c.1).abs() <= EPS);
    }
}

#[cfg(feature = "ci-heavy")]
#[test]
fn deterministic_engine_100_seeds_3_runs_hash_and_hv_equal() {
    for seed in 0..100u64 {
        let a = run_once(seed, 1);
        let b = run_once(seed, 2);
        let c = run_once(seed, 3);
        assert_eq!(a.0, b.0);
        assert_eq!(b.0, c.0);
        assert!((a.1 - b.1).abs() <= EPS);
        assert!((b.1 - c.1).abs() <= EPS);
    }
}

fn run_once(seed: u64, salt: u64) -> (String, f64) {
    let cfg = Phase1Config {
        beam_width: 1,
        max_steps: 1,
        hv_policy: HvPolicy::Legacy,
        seed: seed ^ salt.wrapping_mul(0x9e3779b97f4a7c15),
        norm_alpha: 0.1,
        alpha: 3.0,
        temperature: 0.1,
        entropy_beta: 0.03,
        lambda_min: 0.2,
        lambda_target_entropy: 1.2,
        lambda_k: 0.2,
        lambda_ema: 0.4,
    };
    let (rows, _) = run_phase1_matrix(cfg);
    let max_depth = rows
        .iter()
        .filter(|r| r.variant == "Base")
        .map(|r| r.depth)
        .max()
        .unwrap_or(0);
    let mut cases = rows
        .into_iter()
        .filter(|r| r.variant == "Base" && r.depth == max_depth)
        .filter_map(|r| {
            parse_pipe_vec4(&r.objective_vector_norm).map(|v| Case {
                id: format!("{}-{}", r.rule_id, r.beam_index),
                v,
            })
        })
        .collect::<Vec<_>>();
    cases.sort_by(|l, r| l.id.cmp(&r.id));
    let front = pareto_front(cases);
    let hash = frontier_hash(&front);
    let points = front.iter().map(|c| c.v).collect::<Vec<_>>();
    let hv = agent_core::hv_4d_from_origin_normalized(&points);
    (hash, hv)
}

fn parse_pipe_vec4(s: &str) -> Option<[f64; 4]> {
    let mut out = [0.0; 4];
    let parts = s.split('|').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    for (i, p) in parts.into_iter().enumerate() {
        out[i] = p.parse::<f64>().ok()?;
    }
    Some(out)
}

fn pareto_front(cases: Vec<Case>) -> Vec<Case> {
    let mut out = Vec::new();
    for i in 0..cases.len() {
        let mut dominated = false;
        for j in 0..cases.len() {
            if i == j {
                continue;
            }
            if dominates(cases[j].v, cases[i].v) {
                dominated = true;
                break;
            }
        }
        if !dominated {
            out.push(cases[i].clone());
        }
    }
    out.sort_by(|l, r| l.id.cmp(&r.id));
    out
}

fn dominates(a: [f64; 4], b: [f64; 4]) -> bool {
    let all_ge = (0..4).all(|i| a[i] + EPS >= b[i]);
    let one_gt = (0..4).any(|i| a[i] > b[i] + EPS);
    all_ge && one_gt
}

fn frontier_hash(front: &[Case]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for c in front {
        fnv1a(&mut h, c.id.as_bytes());
        fnv1a(&mut h, b"|");
        for v in c.v {
            fnv1a(&mut h, &v.to_bits().to_le_bytes());
        }
        fnv1a(&mut h, b"\n");
    }
    format!("{h:016x}")
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= *b as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}
