use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use agent_core::{Phase1Config, run_phase1_matrix};
use serde_json::json;

use crate::step0;

pub const RUNTIME_BINDING: &str = "ACTION_LAYER_V1";
pub const CLI_VERSION: &str = "phase1-batch-1.0";

pub fn run_from_args(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--version") {
        println!("{CLI_VERSION}");
        println!("PHASE1_RUNTIME_BINDING: {RUNTIME_BINDING}");
        return Ok(());
    }
    let cfg = BatchConfig::from_args(&args);
    run(cfg)
}

#[derive(Clone, Debug)]
struct BatchConfig {
    seeds: Vec<u64>,
    cases: usize,
    depth: usize,
    beam: usize,
    norm_alpha: f64,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
    lambda_min: f64,
    lambda_target_entropy: f64,
    lambda_k: f64,
    lambda_ema: f64,
    out_dir: PathBuf,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            seeds: vec![42, 43, 44, 45],
            cases: 100,
            depth: 25,
            beam: 5,
            norm_alpha: 0.1,
            alpha: 3.0,
            temperature: 0.1,
            entropy_beta: 0.03,
            lambda_min: 0.2,
            lambda_target_entropy: 1.2,
            lambda_k: 0.2,
            lambda_ema: 0.4,
            out_dir: PathBuf::from("report/phase1_v11"),
        }
    }
}

impl BatchConfig {
    fn from_args(args: &[String]) -> Self {
        let mut cfg = Self::default();
        let mut i = 0usize;
        while i < args.len() {
            match args[i].as_str() {
                "--seeds" if i + 1 < args.len() => {
                    cfg.seeds = args[i + 1]
                        .split(',')
                        .filter_map(|s| s.trim().parse::<u64>().ok())
                        .collect::<Vec<_>>();
                    i += 1;
                }
                "--cases" if i + 1 < args.len() => {
                    if let Ok(v) = args[i + 1].parse::<usize>() {
                        cfg.cases = v.max(1);
                    }
                    i += 1;
                }
                "--depth" if i + 1 < args.len() => {
                    if let Ok(v) = args[i + 1].parse::<usize>() {
                        cfg.depth = v.max(1);
                    }
                    i += 1;
                }
                "--beam" if i + 1 < args.len() => {
                    if let Ok(v) = args[i + 1].parse::<usize>() {
                        cfg.beam = v.max(1);
                    }
                    i += 1;
                }
                "--out-dir" if i + 1 < args.len() => {
                    cfg.out_dir = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
                _ => {}
            }
            i += 1;
        }
        if cfg.seeds.is_empty() {
            cfg.seeds = vec![42, 43, 44, 45];
        }
        cfg
    }
}

fn run(cfg: BatchConfig) -> Result<(), String> {
    println!("PHASE1_RUNTIME_BINDING: {RUNTIME_BINDING}");
    fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("failed to create out-dir: {e}"))?;
    let raw_path = cfg.out_dir.join("phase1_scs_v11_raw.jsonl");
    let summary_path = cfg.out_dir.join("phase1_scs_v11_summary.json");

    let mut raw_writer = BufWriter::new(
        File::create(&raw_path).map_err(|e| format!("failed to create raw log: {e}"))?,
    );

    let mut all = Vec::<SeedRow>::new();
    for &seed in &cfg.seeds {
        // Step0 Gate (generator side)
        let generated_cases = step0::generate_cases(seed);
        let integrity = step0::validate_cases(&generated_cases, seed).map_err(|errs| {
            format!(
                "Step0 generator gate failed (seed={seed}): {}",
                errs.join("; ")
            )
        })?;
        step0::check_diversity_sanity(&generated_cases)
            .map_err(|errs| format!("Step0 diversity failed (seed={seed}): {}", errs.join("; ")))?;
        step0::verify_seed_reproducibility(seed)
            .map_err(|e| format!("Step0 reproducibility failed (seed={seed}): {e}"))?;
        step0::write_audit_logs(&cfg.out_dir, seed, &generated_cases, &integrity)?;

        // Step0 Gate (runner side, double-check right before run)
        let loaded_cases = generated_cases.clone();
        step0::validate_cases(&loaded_cases, seed).map_err(|errs| {
            format!(
                "Step0 runner gate failed before execution (seed={seed}): {}",
                errs.join("; ")
            )
        })?;

        let phase1_cfg = Phase1Config {
            depth: cfg.depth,
            beam: cfg.beam,
            seed,
            hv_guided: false,
            norm_alpha: cfg.norm_alpha,
            alpha: cfg.alpha,
            temperature: cfg.temperature,
            entropy_beta: cfg.entropy_beta,
            lambda_min: cfg.lambda_min,
            lambda_target_entropy: cfg.lambda_target_entropy,
            lambda_k: cfg.lambda_k,
            lambda_ema: cfg.lambda_ema,
        };
        let (raw_rows, _) = run_phase1_matrix(phase1_cfg);
        let sampled = raw_rows
            .into_iter()
            .take(cfg.cases.min(loaded_cases.len()))
            .collect::<Vec<_>>();
        for (idx, row) in sampled.into_iter().enumerate() {
            let tc = &loaded_cases[idx];
            let question_count =
                ((row.cls * 2.0 + row.orphan_rate * 2.0).clamp(0.0, 4.0)).round() as usize;
            let revision_score = round6(row.inconsistency);
            let revision_count = if row.inconsistency > 0.40 { 1 } else { 0 };
            let scs = round6(row.scs_v1_1);
            let entry = json!({
                "seed": seed,
                "case_id": tc.case_id,
                "category": tc.category,
                "variant": row.variant,
                "depth": row.depth,
                "beam_index": row.beam_index,
                "rule_id": row.rule_id,
                "dependency_consistency": round6(row.dependency_consistency),
                "connectivity": round6(row.connectivity),
                "cyclicity": round6(row.cyclicity),
                "orphan_rate": round6(row.orphan_rate),
                "scs_v1": round6(row.scs_v1),
                "scs_v1_1": round6(row.scs_v1_1),
                "scs": scs,
                "cls": round6(row.cls),
                "inconsistency": round6(row.inconsistency),
                "question_count": question_count,
                "revision_score": revision_score,
                "revision_count": revision_count,
                "phase2_triggered": row.phase2_triggered,
                "sanity": {
                    "empty_id_fixes": row.sanity_empty_id_fixes,
                    "duplicate_id_fixes": row.sanity_duplicate_id_fixes,
                    "unknown_dependency_drops": row.sanity_unknown_dependency_drops
                },
                "step0": {
                    "input_len": tc.input_text.chars().count(),
                    "input_hash_prefix": step0::input_hash_prefix(&tc.input_text)
                }
            });
            serde_json::to_writer(&mut raw_writer, &entry)
                .map_err(|e| format!("failed to write raw jsonl: {e}"))?;
            raw_writer
                .write_all(b"\n")
                .map_err(|e| format!("failed to write raw newline: {e}"))?;

            all.push(SeedRow {
                seed,
                category: tc.category.clone(),
                scs_v1_1: row.scs_v1_1,
                cls: row.cls,
                inconsistency: row.inconsistency,
                phase2_triggered: row.phase2_triggered,
                phase2_false_trigger_proxy: row.phase2_false_trigger_proxy,
                dependency_consistency: row.dependency_consistency,
                cyclicity: row.cyclicity,
                orphan_rate: row.orphan_rate,
                had_sanity_fix: row.sanity_empty_id_fixes > 0
                    || row.sanity_duplicate_id_fixes > 0
                    || row.sanity_unknown_dependency_drops > 0,
            });
        }
    }
    raw_writer
        .flush()
        .map_err(|e| format!("failed to flush raw log: {e}"))?;

    let summary = build_summary(&all);
    let summary_json = serde_json::to_string_pretty(&summary)
        .map_err(|e| format!("summary serialize failed: {e}"))?;
    fs::write(&summary_path, summary_json).map_err(|e| format!("failed to write summary: {e}"))?;

    println!("Wrote {}", raw_path.display());
    println!("Wrote {}", summary_path.display());
    Ok(())
}

#[derive(Clone, Debug)]
struct SeedRow {
    seed: u64,
    category: String,
    scs_v1_1: f64,
    cls: f64,
    inconsistency: f64,
    phase2_triggered: bool,
    phase2_false_trigger_proxy: bool,
    dependency_consistency: f64,
    cyclicity: f64,
    orphan_rate: f64,
    had_sanity_fix: bool,
}

fn build_summary(rows: &[SeedRow]) -> serde_json::Value {
    let n = rows.len().max(1) as f64;
    let avg_cls = rows.iter().map(|r| r.cls).sum::<f64>() / n;
    let avg_scs = rows.iter().map(|r| r.scs_v1_1).sum::<f64>() / n;
    let revision_rate = rows.iter().filter(|r| r.inconsistency > 0.40).count() as f64 / n;
    let avg_questions = rows
        .iter()
        .map(|r| (r.cls * 2.0 + r.orphan_rate * 2.0).clamp(0.0, 4.0))
        .sum::<f64>()
        / n;
    let phase2_false_trigger_rate =
        rows.iter().filter(|r| r.phase2_false_trigger_proxy).count() as f64 / n;
    let abnormal_rate = rows.iter().filter(|r| r.had_sanity_fix).count() as f64 / n;
    let scs_1_0_rate = rows.iter().filter(|r| r.scs_v1_1 >= 0.999_999).count() as f64 / n;
    let phase2_trigger_rate = rows.iter().filter(|r| r.phase2_triggered).count() as f64 / n;
    let avg_dependency_consistency = rows.iter().map(|r| r.dependency_consistency).sum::<f64>() / n;
    let avg_cyclicity = rows.iter().map(|r| r.cyclicity).sum::<f64>() / n;

    let mut per_seed = BTreeMap::<u64, Vec<f64>>::new();
    for r in rows {
        per_seed.entry(r.seed).or_default().push(r.scs_v1_1);
    }
    let objectives = per_seed
        .iter()
        .map(|(seed, values)| {
            let mean = values.iter().sum::<f64>() / values.len().max(1) as f64;
            (*seed, mean)
        })
        .collect::<BTreeMap<_, _>>();
    let objective = objectives.values().sum::<f64>() / objectives.len().max(1) as f64;
    let objective_var = {
        let m = objective;
        objectives.values().map(|v| (v - m).powi(2)).sum::<f64>() / objectives.len().max(1) as f64
    };

    let mut cat_sum = BTreeMap::<String, (f64, usize)>::new();
    for r in rows {
        let entry = cat_sum.entry(r.category.clone()).or_insert((0.0, 0));
        entry.0 += r.scs_v1_1;
        entry.1 += 1;
    }
    let mut category_scs_mean = BTreeMap::<String, f64>::new();
    for (k, (sum, cnt)) in cat_sum {
        if cnt > 0 {
            category_scs_mean.insert(k, sum / cnt as f64);
        }
    }

    json!({
        "avg_cls": round6(avg_cls),
        "avg_scs_v1_1": round6(avg_scs),
        "revision_rate": round6(revision_rate),
        "avg_questions": round6(avg_questions),
        "phase2_false_trigger_rate": round6(phase2_false_trigger_rate),
        "abnormal_rate": round6(abnormal_rate),
        "scs_1.0_rate": round6(scs_1_0_rate),
        "phase2_trigger_rate": round6(phase2_trigger_rate),
        "avg_dependency_consistency": round6(avg_dependency_consistency),
        "avg_cyclicity": round6(avg_cyclicity),
        "category_scs_mean": category_scs_mean,
        "category_counts": rows.iter().fold(BTreeMap::<String, usize>::new(), |mut acc, r| { *acc.entry(r.category.clone()).or_insert(0) += 1; acc }),
        "objective": round6(objective),
        "objective_var": round6(objective_var),
        "objective_by_seed": objectives.iter().map(|(k, v)| (k.to_string(), round6(*v))).collect::<BTreeMap<_, _>>(),
        "notes": [
            "cls is proxied by ambiguity_mean",
            "revision_rate counts inconsistency > 0.40",
            "avg_questions is a proxy derived from cls and orphan_rate",
            "phase2_false_trigger_rate is a proxy: triggered AND dependency_consistency < 0.50",
            "category_scs_mean uses Step0 assigned fixed categories"
        ]
    })
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}
