use std::fs;

use serde::{Deserialize, Serialize};

const EPS: f64 = 1e-12;
const DIM: usize = 4;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputRoot {
    Cases(Vec<InputCaseResult>),
    Wrapped { cases: Vec<InputCaseResult> },
}

#[derive(Debug, Clone, Deserialize)]
struct InputCaseResult {
    case_id: String,
    #[serde(alias = "raw_objective_vector", alias = "raw")]
    raw: Vec<f64>,
    #[serde(alias = "normalized_vector", alias = "normalized")]
    normalized: Vec<f64>,
    #[serde(alias = "clamped_vector", alias = "clamped")]
    clamped: Vec<f64>,
    domination_count: usize,
    pareto_rank: usize,
}

#[derive(Debug, Clone, Serialize)]
struct OutputCaseData {
    case_id: String,
    raw: [f64; DIM],
    normalized: [f64; DIM],
    clamped: [f64; DIM],
    domination_count: usize,
    pareto_rank: usize,
}

#[derive(Debug, Serialize)]
struct Phase9EvalReport {
    report_type: &'static str,
    objective_vector_spec_status: &'static str,
    normalized_range_status: &'static str,
    normalized_out_of_range_case_count: usize,
    normalized_out_of_range_max_abs: f64,
    warning_meta: Vec<String>,
    case_count: usize,
    frontier_size: usize,
    frontier_hypervolume: f64,
    objective_correlation_matrix: [[f64; DIM]; DIM],
    frontier_objective_mean: [f64; DIM],
    frontier_objective_variance: [f64; DIM],
    cases: Vec<OutputCaseData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InvalidReason {
    OutOfRange = 0,
    ParetoRank = 1,
    Dimension = 2,
    NonFinite = 3,
}

pub fn run_pareto_eval(
    input_path: &str,
    out_path: &str,
    allow_normalized_out_of_range: bool,
) -> Result<(), String> {
    let raw = match fs::read_to_string(input_path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let cwd = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown cwd>".to_string());
            let candidates = find_candidate_json_files().join(", ");
            return Err(format!(
                "failed to read input json {input_path}: {e}\ncurrent_dir={cwd}\njson_candidates={candidates}\nexample: cargo run -p design_cli -- pareto-eval /path/to/input.json -o {out_path}"
            ));
        }
        Err(e) => return Err(format!("failed to read input json {input_path}: {e}")),
    };
    let parsed: InputRoot =
        serde_json::from_str(&raw).map_err(|e| format!("input parse error: {e}"))?;
    let input_cases = match parsed {
        InputRoot::Cases(v) => v,
        InputRoot::Wrapped { cases } => cases,
    };
    if input_cases.is_empty() {
        return Err("input cases must not be empty".to_string());
    }

    let report = build_report(input_cases, allow_normalized_out_of_range)?;
    let rendered = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("failed to serialize report: {e}"))?;
    fs::write(out_path, rendered).map_err(|e| format!("failed to write {out_path}: {e}"))?;
    println!("Wrote {out_path}");
    Ok(())
}

fn find_candidate_json_files() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push(path.display().to_string());
            }
        }
    }
    out.sort();
    if out.len() > 8 {
        out.truncate(8);
    }
    if out.is_empty() {
        out.push("<none in cwd>".to_string());
    }
    out
}

fn build_report(
    input_cases: Vec<InputCaseResult>,
    allow_normalized_out_of_range: bool,
) -> Result<Phase9EvalReport, String> {
    let mut highest_invalid: Option<(InvalidReason, String)> = None;
    let mut out_of_range_case_count = 0usize;
    let mut out_of_range_max_abs = 0.0_f64;
    let mut out_cases = Vec::<OutputCaseData>::with_capacity(input_cases.len());

    for (idx, c) in input_cases.into_iter().enumerate() {
        if let Some(err) = validate_non_finite(&c, idx) {
            update_highest_invalid(&mut highest_invalid, InvalidReason::NonFinite, err);
            continue;
        }
        if let Some(err) = validate_dimension(&c, idx) {
            update_highest_invalid(&mut highest_invalid, InvalidReason::Dimension, err);
            continue;
        }
        if c.pareto_rank == 0 {
            update_highest_invalid(
                &mut highest_invalid,
                InvalidReason::ParetoRank,
                format!("case[{idx}] pareto_rank must be >= 1"),
            );
            continue;
        }

        let raw = vec_to_array4(&c.raw);
        let mut normalized = vec_to_array4(&c.normalized);
        let clamped_in = vec_to_array4(&c.clamped);

        let mut case_out_of_range = false;
        for v in normalized {
            if v < 0.0 - EPS || v > 1.0 + EPS {
                case_out_of_range = true;
                out_of_range_max_abs = out_of_range_max_abs.max(out_of_range_abs(v));
            }
        }
        if case_out_of_range {
            out_of_range_case_count += 1;
        }

        if case_out_of_range && allow_normalized_out_of_range {
            for v in &mut normalized {
                *v = clamp01(*v);
            }
        } else if case_out_of_range {
            update_highest_invalid(
                &mut highest_invalid,
                InvalidReason::OutOfRange,
                format!("case[{idx}] normalized value is out of [0,1]"),
            );
            continue;
        }

        out_cases.push(OutputCaseData {
            case_id: c.case_id,
            raw,
            normalized,
            clamped: clamped_in,
            domination_count: c.domination_count,
            pareto_rank: c.pareto_rank,
        });
    }

    if let Some((reason, msg)) = highest_invalid {
        if reason == InvalidReason::OutOfRange && !allow_normalized_out_of_range {
            return Err(format!(
                "{msg}; objective_vector_spec_status=OBJECTIVE_VECTOR_V2_INVALID; normalized_range_status=OUT_OF_RANGE_ERROR"
            ));
        }
        return Err(format!(
            "{msg}; objective_vector_spec_status=OBJECTIVE_VECTOR_V2_INVALID"
        ));
    }

    let range_status = if out_of_range_case_count == 0 {
        "OK"
    } else {
        "CLAMPED"
    };
    let frontier_points = out_cases
        .iter()
        .filter(|c| c.pareto_rank == 1)
        .map(|c| c.normalized)
        .collect::<Vec<_>>();
    let frontier_size = frontier_points.len();
    let frontier_hypervolume = round4(agent_core::hv_4d_from_origin_normalized(&frontier_points));
    let objective_correlation_matrix = pearson_correlation_matrix(&out_cases);
    let frontier_objective_mean = frontier_mean(&out_cases);
    let frontier_objective_variance = frontier_variance(&out_cases, &frontier_objective_mean);
    let spec_status = if range_status == "OK" && frontier_size > 0 {
        "OBJECTIVE_VECTOR_V2_VALID"
    } else {
        "OBJECTIVE_VECTOR_V2_INVALID"
    };
    let warning_meta = if range_status == "CLAMPED" {
        vec![
            "normalized vector was clamped due to --allow-normalized-out-of-range".to_string(),
            "frontier_hypervolume was computed on clamped normalized values".to_string(),
        ]
    } else {
        Vec::new()
    };

    Ok(Phase9EvalReport {
        report_type: "phase9_eval_v1",
        objective_vector_spec_status: spec_status,
        normalized_range_status: range_status,
        normalized_out_of_range_case_count: out_of_range_case_count,
        normalized_out_of_range_max_abs: round4(out_of_range_max_abs),
        warning_meta,
        case_count: out_cases.len(),
        frontier_size,
        frontier_hypervolume,
        objective_correlation_matrix,
        frontier_objective_mean,
        frontier_objective_variance,
        cases: out_cases,
    })
}

fn update_highest_invalid(
    highest: &mut Option<(InvalidReason, String)>,
    kind: InvalidReason,
    msg: String,
) {
    match highest {
        Some((prev, _)) if *prev >= kind => {}
        _ => *highest = Some((kind, msg)),
    }
}

fn validate_non_finite(c: &InputCaseResult, idx: usize) -> Option<String> {
    for (name, vec) in [
        ("raw", &c.raw),
        ("normalized", &c.normalized),
        ("clamped", &c.clamped),
    ] {
        if let Some((j, v)) = vec.iter().enumerate().find(|(_, v)| !v.is_finite()) {
            return Some(format!("non-finite value in case[{idx}] {name}[{j}] = {v}"));
        }
    }
    None
}

fn validate_dimension(c: &InputCaseResult, idx: usize) -> Option<String> {
    let dims = [
        ("raw", c.raw.len()),
        ("normalized", c.normalized.len()),
        ("clamped", c.clamped.len()),
    ];
    if let Some((name, got)) = dims.into_iter().find(|(_, got)| *got != DIM) {
        return Some(format!(
            "dimension mismatch in case[{idx}] {name}: expected {DIM}, got {got}"
        ));
    }
    None
}

fn vec_to_array4(v: &[f64]) -> [f64; DIM] {
    let mut out = [0.0; DIM];
    out.copy_from_slice(&v[..DIM]);
    out
}

fn out_of_range_abs(v: f64) -> f64 {
    if v < 0.0 {
        -v
    } else if v > 1.0 {
        v - 1.0
    } else {
        0.0
    }
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn pearson_correlation_matrix(cases: &[OutputCaseData]) -> [[f64; DIM]; DIM] {
    let n = cases.len() as f64;
    let mut cols = vec![Vec::<f64>::with_capacity(cases.len()); DIM];
    for case in cases {
        for (i, col) in cols.iter_mut().enumerate() {
            col.push(case.normalized[i]);
        }
    }
    let mut out = [[0.0; DIM]; DIM];
    for i in 0..DIM {
        for j in 0..DIM {
            if i == j {
                out[i][j] = 1.0;
                continue;
            }
            let mean_i = cols[i].iter().sum::<f64>() / n;
            let mean_j = cols[j].iter().sum::<f64>() / n;
            let mut cov = 0.0;
            let mut var_i = 0.0;
            let mut var_j = 0.0;
            for k in 0..cases.len() {
                let di = cols[i][k] - mean_i;
                let dj = cols[j][k] - mean_j;
                cov += di * dj;
                var_i += di * di;
                var_j += dj * dj;
            }
            let denom = (var_i.sqrt() * var_j.sqrt()).max(EPS);
            out[i][j] = round4((cov / denom).clamp(-1.0, 1.0));
        }
    }
    out
}

fn frontier_mean(cases: &[OutputCaseData]) -> [f64; DIM] {
    let frontier = cases
        .iter()
        .filter(|c| c.pareto_rank == 1)
        .collect::<Vec<_>>();
    if frontier.is_empty() {
        return [0.0; DIM];
    }
    let mut mean = [0.0; DIM];
    for c in &frontier {
        for (i, m) in mean.iter_mut().enumerate() {
            *m += c.normalized[i];
        }
    }
    for m in &mut mean {
        *m = round4(*m / frontier.len() as f64);
    }
    mean
}

fn frontier_variance(cases: &[OutputCaseData], mean: &[f64; DIM]) -> [f64; DIM] {
    let frontier = cases
        .iter()
        .filter(|c| c.pareto_rank == 1)
        .collect::<Vec<_>>();
    if frontier.is_empty() {
        return [0.0; DIM];
    }
    let mut var = [0.0; DIM];
    for c in &frontier {
        for i in 0..DIM {
            let d = c.normalized[i] - mean[i];
            var[i] += d * d;
        }
    }
    for v in &mut var {
        *v = round4(*v / frontier.len() as f64);
    }
    var
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_case(id: &str, normalized: [f64; DIM], rank: usize) -> InputCaseResult {
        InputCaseResult {
            case_id: id.to_string(),
            raw: normalized.to_vec(),
            normalized: normalized.to_vec(),
            clamped: normalized.to_vec(),
            domination_count: 0,
            pareto_rank: rank,
        }
    }

    #[test]
    fn hypervolume_monotonicity_case() {
        let f1 = vec![[0.2, 0.2, 0.2, 0.2]];
        let f2 = vec![[0.2, 0.2, 0.2, 0.2], [0.8, 0.2, 0.2, 0.2]];
        let hv1 = agent_core::hv_4d_from_origin_normalized(&f1);
        let hv2 = agent_core::hv_4d_from_origin_normalized(&f2);
        assert!(hv2 + EPS >= hv1);
    }

    #[test]
    fn strict_out_of_range_is_error() {
        let cases = vec![mk_case("C-001", [1.2, 0.3, 0.4, 0.5], 1)];
        let err = build_report(cases, false).unwrap_err();
        assert!(err.contains("OBJECTIVE_VECTOR_V2_INVALID"));
        assert!(err.contains("OUT_OF_RANGE_ERROR"));
    }

    #[test]
    fn allow_out_of_range_clamps_and_marks_invalid() {
        let cases = vec![mk_case("C-001", [1.2, -0.3, 0.4, 0.5], 1)];
        let report = build_report(cases, true).expect("allow mode should produce report");
        assert_eq!(report.normalized_range_status, "CLAMPED");
        assert_eq!(
            report.objective_vector_spec_status,
            "OBJECTIVE_VECTOR_V2_INVALID"
        );
        assert_eq!(report.normalized_out_of_range_case_count, 1);
        assert_eq!(report.cases[0].normalized, [1.0, 0.0, 0.4, 0.5]);
    }

    #[test]
    fn invalid_nonfinite_is_rejected() {
        let cases = vec![mk_case("C-001", [0.2, 0.3, 0.4, 0.5], 1)];
        let mut broken = cases;
        broken[0].normalized[2] = f64::NAN;
        let err = build_report(broken, true).unwrap_err();
        assert!(err.contains("non-finite"));
    }
}
