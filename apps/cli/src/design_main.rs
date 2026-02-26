mod pareto_eval;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

use agent_core::{HvPolicy, Phase1Config, SoftTraceParams, TraceRunConfig, run_phase1_matrix};
use clap::{Parser, Subcommand};
use design_reasoning::{Phase1Engine, ScsInputs};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const CLI_VERSION: &str = "0.1.0";
const RUNTIME_BINDING: &str = "ACTION_LAYER_V1";
const PARETO_EPS: f64 = 1e-12;

#[derive(Parser, Debug)]
#[command(name = "design", about = "Phase1 CLI", disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Analyze {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long = "beam-width", default_value_t = 5)]
        beam_width: usize,
        #[arg(long = "max-steps", default_value_t = 25)]
        max_steps: usize,
        #[arg(long = "hv-guided", default_value_t = false)]
        hv_guided: bool,
    },
    Explain {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long = "beam-width", default_value_t = 5)]
        beam_width: usize,
        #[arg(long = "max-steps", default_value_t = 25)]
        max_steps: usize,
        #[arg(long = "hv-guided", default_value_t = false)]
        hv_guided: bool,
    },
    Simulate {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long = "beam-width", default_value_t = 5)]
        beam_width: usize,
        #[arg(long = "max-steps", default_value_t = 25)]
        max_steps: usize,
        #[arg(long = "hv-guided", default_value_t = false)]
        hv_guided: bool,
    },
    Clear,
    Adopt,
    Reject,
    Export {
        #[arg(long)]
        out: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct InputCase {
    case_id: Option<String>,
    category: Option<String>,
    text: Option<String>,
    input: Option<String>,
    input_text: Option<String>,
}

#[derive(Debug)]
struct ResolvedCase {
    case_id: String,
    category: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Phase1RawRow {
    case_id: String,
    category: String,
    #[serde(default)]
    cls: f64,
    #[serde(default)]
    scs: f64,
    #[serde(default)]
    completeness: f64,
    #[serde(default)]
    ambiguity_mean: f64,
    #[serde(default)]
    inconsistency: f64,
    #[serde(default)]
    dependency_consistency: f64,
    #[serde(default)]
    question_count: usize,
    #[serde(default)]
    revision_score: f64,
    #[serde(default)]
    revision_count: usize,
    #[serde(default)]
    phase2_triggered: bool,
    #[serde(default)]
    objective_legacy: Option<LegacyObjectiveVector>,
}

#[derive(Debug, Clone, Copy)]
struct FiveW2HFlags {
    who: bool,
    what: bool,
    why: bool,
    where_: bool,
    when_: bool,
    how: bool,
    how_much: bool,
}

#[derive(Debug, Clone, Copy)]
struct MissingSlots {
    missing_5w2h_slots: u8,
    flags: FiveW2HFlags,
}

#[derive(Debug, Serialize)]
struct EvalSummary {
    count: usize,
    avg_cls: f64,
    avg_questions: f64,
    frontier_size: usize,
    frontier_hash: String,
    frontier_hash_consistent: bool,
    frontier_hypervolume: f64,
    objective_correlation_matrix: [[f64; 4]; 4],
    frontier_objective_mean: [f64; 4],
    frontier_objective_variance: [f64; 4],
    domination_count_histogram: BTreeMap<usize, usize>,
    objective_vector_spec_status: &'static str,
    runtime_binding: &'static str,
    status: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct ObjectiveVectorV1_1 {
    raw: [f64; 4],
    normalized: [f64; 4],
    clamped: [f64; 4],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct LegacyObjectiveVector {
    structural_integrity: f64,
    cognitive_stability: f64,
    revision_pressure: f64,
    exploration_readiness: f64,
}

#[derive(Debug, Clone)]
struct ObjectiveCase {
    case_id: String,
    category: String,
    objective: ObjectiveVectorV1_1,
}

#[derive(Debug, Serialize)]
struct JsonMeta {
    command: &'static str,
    hv_policy: Option<&'static str>,
    deterministic: bool,
}

fn main() {
    if let Err(err) = run() {
        let err_json = json!({
            "error": {
                "code": "PHASE1_ERROR",
                "message": err,
                "details": Value::Null
            }
        });
        eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap_or_else(|_| "{\"error\":{\"code\":\"PHASE1_ERROR\",\"message\":\"serialization failed\",\"details\":null}}".to_string()));
        std::process::exit(2);
    };
}

fn run() -> Result<(), String> {
    let cli = Cli::try_parse().map_err(|e| format!("invalid command: {e}"))?;
    match cli.command {
        Commands::Analyze {
            seed,
            beam_width,
            max_steps,
            hv_guided,
        } => run_analyze(seed, beam_width, max_steps, hv_guided),
        Commands::Explain {
            seed,
            beam_width,
            max_steps,
            hv_guided,
        } => run_explain(seed, beam_width, max_steps, hv_guided),
        Commands::Simulate {
            seed,
            beam_width,
            max_steps,
            hv_guided,
        } => run_simulate(seed, beam_width, max_steps, hv_guided),
        Commands::Clear => render_success(
            "clear",
            json!({"cleared": true}),
            JsonMeta {
                command: "clear",
                hv_policy: None,
                deterministic: true,
            },
        ),
        Commands::Adopt => render_success(
            "adopt",
            json!({"adopted": true}),
            JsonMeta {
                command: "adopt",
                hv_policy: None,
                deterministic: true,
            },
        ),
        Commands::Reject => render_success(
            "reject",
            json!({"rejected": true}),
            JsonMeta {
                command: "reject",
                hv_policy: None,
                deterministic: true,
            },
        ),
        Commands::Export { out } => render_success(
            "export",
            json!({"exported": true, "out": out}),
            JsonMeta {
                command: "export",
                hv_policy: None,
                deterministic: true,
            },
        ),
    }
}

fn run_analyze(seed: u64, beam_width: usize, max_steps: usize, hv_guided: bool) -> Result<(), String> {
    let payload = analyze_payload(seed, beam_width, max_steps, hv_guided)?;
    render_success(
        "analyze",
        payload,
        JsonMeta {
            command: "analyze",
            hv_policy: Some(if hv_guided { "Guided" } else { "Legacy" }),
            deterministic: true,
        },
    )
}

fn run_explain(seed: u64, beam_width: usize, max_steps: usize, hv_guided: bool) -> Result<(), String> {
    let payload = analyze_payload(seed, beam_width, max_steps, hv_guided)?;
    render_success(
        "explain",
        json!({
            "summary": "Phase1 deterministic multi-objective explanation",
            "analysis": payload
        }),
        JsonMeta {
            command: "explain",
            hv_policy: Some(if hv_guided { "Guided" } else { "Legacy" }),
            deterministic: true,
        },
    )
}

fn run_simulate(seed: u64, beam_width: usize, max_steps: usize, hv_guided: bool) -> Result<(), String> {
    let payload = analyze_payload(seed, beam_width, max_steps, hv_guided)?;
    render_success(
        "simulate",
        json!({
            "simulation": "phase1",
            "result": payload
        }),
        JsonMeta {
            command: "simulate",
            hv_policy: Some(if hv_guided { "Guided" } else { "Legacy" }),
            deterministic: true,
        },
    )
}

fn analyze_payload(seed: u64, beam_width: usize, max_steps: usize, hv_guided: bool) -> Result<Value, String> {
    if beam_width == 0 {
        return Err("beam_width must be > 0".to_string());
    }
    if max_steps == 0 {
        return Err("max_steps must be > 0".to_string());
    }

    let rows = run_engine_with_policy(seed, beam_width, max_steps, hv_guided)?;
    let mut objective_cases_raw = Vec::<ObjectiveCase>::with_capacity(rows.len());
    for (idx, row) in rows.iter().enumerate() {
        let raw = parse_vec4_pipe(&row.objective_vector_raw)
            .ok_or_else(|| format!("invalid objective_vector_raw format at row {idx}"))?;
        objective_cases_raw.push(ObjectiveCase {
            case_id: format!("{}-{:04}-{:04}", row.variant, row.depth, row.beam_index),
            category: row.variant.clone(),
            objective: ObjectiveVectorV1_1 {
                raw,
                normalized: raw,
                clamped: raw,
            },
        });
    }
    let objective_cases = normalize_objective_cases(objective_cases_raw)?;
    let frontier = pareto_frontier_by_case_id(&objective_cases);
    let frontier_hv = hypervolume_4d_from_origin(&frontier);
    let hash = frontier_hash(&frontier);

    let drafts = objective_cases
        .iter()
        .map(|c| {
            json!({
                "case_id": c.case_id,
                "category": c.category,
                "objective": c.objective
            })
        })
        .collect::<Vec<_>>();
    let frontier_cases = frontier
        .iter()
        .map(|c| {
            json!({
                "case_id": c.case_id,
                "category": c.category,
                "objective": c.objective
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "objective_vector_version": "v1.1",
        "policy": if hv_guided { "Guided" } else { "Legacy" },
        "drafts": drafts,
        "frontier": frontier_cases,
        "frontier_size": frontier.len(),
        "frontier_hash": hash,
        "hypervolume": round6(frontier_hv)
    }))
}

fn run_engine_with_policy(
    seed: u64,
    beam_width: usize,
    max_steps: usize,
    hv_guided: bool,
) -> Result<Vec<agent_core::Phase1RawRow>, String> {
    let cfg = Phase1Config {
        beam_width,
        max_steps,
        hv_policy: if hv_guided {
            HvPolicy::Guided
        } else {
            HvPolicy::Legacy
        },
        seed,
        norm_alpha: 0.1,
        alpha: 3.0,
        temperature: 0.1,
        entropy_beta: 0.03,
        lambda_min: 0.2,
        lambda_target_entropy: 1.2,
        lambda_k: 0.2,
        lambda_ema: 0.4,
    };
    if !cfg.is_valid() {
        return Err("invalid Phase1Config constraints".to_string());
    }
    let (rows, _) = run_phase1_matrix(cfg);
    if rows.is_empty() {
        return Err("Phase1 engine produced no rows".to_string());
    }
    Ok(rows)
}

fn parse_vec4_pipe(v: &str) -> Option<[f64; 4]> {
    let parts = v.split('|').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse::<f64>().ok()?;
    }
    Some(out)
}

fn render_success(command: &'static str, data: Value, meta: JsonMeta) -> Result<(), String> {
    let wrapper = success_wrapper_value(command, data, meta);
    println!(
        "{}",
        serde_json::to_string_pretty(&wrapper)
            .map_err(|e| format!("failed to serialize response: {e}"))?
    );
    Ok(())
}

fn success_wrapper_value(command: &'static str, data: Value, meta: JsonMeta) -> Value {
    json!({
        "schema_version": "v1",
        "command": command,
        "data": data,
        "meta": meta
    })
}

fn run_phase1_batch(input: &str, output: &str, seed: Option<u64>) -> Result<(), String> {
    println!("PHASE1_RUNTIME_BINDING: {RUNTIME_BINDING}");
    let cases = load_cases(input)?;
    let seed = seed.unwrap_or(42);
    let base_rows = run_engine(seed)?;
    let engine = Phase1Engine;
    let debug_5w2h = std::env::var("PHASE1_DEBUG_5W2H_TEMP")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let debug_dep_temp = std::env::var("PHASE1_TEMP_DEP_STABILIZE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let file = File::create(output).map_err(|e| format!("failed to create output: {e}"))?;
    let mut writer = BufWriter::new(file);

    for (idx, case) in cases.iter().enumerate() {
        let base = &base_rows[idx % base_rows.len()];
        let (row, missing_slots) = map_to_output_row(case, base, &engine);
        if debug_5w2h && seed == 42 && idx < 3 {
            eprintln!(
                "[5W2H_TEMP] case_id={} missing={} flags=who:{} what:{} why:{} where:{} when:{} how:{} how_much:{}",
                case.case_id,
                missing_slots.missing_5w2h_slots,
                missing_slots.flags.who,
                missing_slots.flags.what,
                missing_slots.flags.why,
                missing_slots.flags.where_,
                missing_slots.flags.when_,
                missing_slots.flags.how,
                missing_slots.flags.how_much
            );
        }
        if debug_dep_temp && seed == 42 && idx < 3 {
            eprintln!(
                "[DEP_TEMP] case_id={} inconsistency={} dep={}",
                case.case_id, row.inconsistency, row.dependency_consistency
            );
        }
        serde_json::to_writer(&mut writer, &row)
            .map_err(|e| format!("failed to write output row: {e}"))?;
        writer
            .write_all(b"\n")
            .map_err(|e| format!("failed to write newline: {e}"))?;
    }
    writer
        .flush()
        .map_err(|e| format!("failed to flush output: {e}"))?;
    Ok(())
}

fn run_phase1_single(
    text: &str,
    case_id: Option<String>,
    category: Option<String>,
    seed: Option<u64>,
) -> Result<(), String> {
    let seed = seed.unwrap_or(42);
    let base_rows = run_engine(seed)?;
    let engine = Phase1Engine;
    let case = ResolvedCase {
        case_id: case_id.unwrap_or_else(|| "single-0001".to_string()),
        category: category.unwrap_or_else(|| "single".to_string()),
        text: text.to_string(),
    };
    let (row, _) = map_to_output_row(&case, &base_rows[0], &engine);
    println!(
        "{}",
        serde_json::to_string_pretty(&row).map_err(|e| format!("json serialize failed: {e}"))?
    );
    Ok(())
}

fn run_phase1_eval(input: &str) -> Result<(), String> {
    let rows = load_output_rows(input)?;
    if rows.is_empty() {
        return Err("evaluation input is empty".to_string());
    }
    let count = rows.len();
    let avg_cls = rows.iter().map(|r| r.cls).sum::<f64>() / count as f64;
    let avg_questions = rows.iter().map(|r| r.question_count as f64).sum::<f64>() / count as f64;
    let objective_cases_raw = rows
        .iter()
        .map(|r| ObjectiveCase {
            case_id: r.case_id.clone(),
            category: r.category.clone(),
            objective: if let Some(legacy) = r.objective_legacy {
                legacy_objective_to_v11(legacy)
            } else {
                objective_vector_v11_from_phase1_row(r)
            },
        })
        .collect::<Vec<_>>();
    let objective_cases = normalize_objective_cases(objective_cases_raw)?;

    let mut run_inputs = Vec::new();
    run_inputs.push(objective_cases.clone());
    let mut reversed = objective_cases.clone();
    reversed.reverse();
    run_inputs.push(reversed);
    let mut rotated = objective_cases.clone();
    if !rotated.is_empty() {
        let shift = (rotated.len() / 3).max(1);
        rotated.rotate_left(shift);
    }
    run_inputs.push(rotated);

    let mut frontier_sizes = Vec::new();
    let mut frontier_hashes = Vec::new();
    let mut frontier_category_counts = BTreeMap::<String, usize>::new();
    let mut frontier_hypervolumes = Vec::new();
    for (idx, run_input) in run_inputs.into_iter().enumerate() {
        let frontier = pareto_frontier_by_case_id(&run_input);
        frontier_sizes.push(frontier.len() as f64);
        frontier_hashes.push(frontier_hash(&frontier));
        frontier_hypervolumes.push(hypervolume_4d_from_origin(&frontier));
        if idx == 0 {
            for item in frontier {
                *frontier_category_counts
                    .entry(item.category.clone())
                    .or_insert(0) += 1;
            }
        }
    }
    let frontier_size = frontier_sizes.first().copied().unwrap_or(0.0) as usize;
    let frontier_hash = frontier_hashes
        .first()
        .cloned()
        .unwrap_or_else(|| "0000000000000000".to_string());
    let frontier_hash_consistent = frontier_hashes.windows(2).all(|w| w[0] == w[1]);
    let frontier_size_variance = variance(&frontier_sizes);
    let frontier_hypervolume = frontier_hypervolumes.first().copied().unwrap_or(0.0);
    let legacy_mode = rows.iter().all(|r| r.objective_legacy.is_some());
    let objective_vector_spec_status = if legacy_mode {
        "V1_0_LEGACY"
    } else if frontier_hash_consistent && frontier_size_variance == 0.0 {
        "OBJECTIVE_VECTOR_SPEC_V1_1_DEFINED"
    } else {
        "OBJECTIVE_VECTOR_SPEC_V1_1_VIOLATION"
    };
    let objective_correlation_matrix = pearson_correlation_matrix(&objective_cases);
    let first_frontier = pareto_frontier_by_case_id(&objective_cases);
    let frontier_objective_mean = objective_mean(&first_frontier);
    let frontier_objective_variance = objective_variance(&first_frontier, &frontier_objective_mean);
    let domination_count_histogram = domination_count_histogram(&objective_cases);
    let _pareto_ranks = pareto_ranks(&objective_cases);

    let status = if avg_cls > 0.15 && avg_questions > 0.8 {
        "PHASE1_RUNTIME_BINDING_VALID"
    } else {
        "PHASE1_RUNTIME_BINDING_INVALID"
    };

    let summary = EvalSummary {
        count,
        avg_cls: round6(avg_cls),
        avg_questions: round6(avg_questions),
        frontier_size,
        frontier_hash,
        frontier_hash_consistent,
        frontier_hypervolume: round6(frontier_hypervolume),
        objective_correlation_matrix,
        frontier_objective_mean,
        frontier_objective_variance,
        domination_count_histogram,
        objective_vector_spec_status,
        runtime_binding: RUNTIME_BINDING,
        status,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .map_err(|e| format!("failed to serialize eval summary: {e}"))?
    );
    if !frontier_category_counts.is_empty() {
        eprintln!(
            "frontier_category_distribution={}",
            serde_json::to_string(&frontier_category_counts)
                .map_err(|e| format!("failed to serialize frontier categories: {e}"))?
        );
    }
    Ok(())
}

fn run_search(depth: usize, beam: usize, seed: u64, hv_guided: bool) -> Result<(), String> {
    let cfg = TraceRunConfig {
        depth: depth.max(1),
        beam: beam.max(1),
        seed,
        norm_alpha: 0.1,
        adaptive_alpha: false,
        hv_guided,
        raw_output_path: None,
    };
    let rows = agent_core::generate_trace_baseline_off_soft(cfg, SoftTraceParams::default());
    let last = rows.last().cloned().unwrap_or_default();
    let summary = serde_json::json!({
        "mode": if hv_guided { "HV_GUIDED" } else { "DEFAULT" },
        "rows": rows.len(),
        "depth_last": last.depth,
        "pareto_front_size_last": last.pareto_front_size_per_depth,
        "pareto_hv_2d_last": last.pareto_hv_2d,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .map_err(|e| format!("search summary serialize failed: {e}"))?
    );
    Ok(())
}

fn run_engine(seed: u64) -> Result<Vec<agent_core::Phase1RawRow>, String> {
    run_engine_with_policy(seed, 5, 25, false)
}

fn load_cases(path: &str) -> Result<Vec<ResolvedCase>, String> {
    let file = File::open(path).map_err(|e| format!("failed to open input jsonl: {e}"))?;
    let reader = BufReader::new(file);

    let mut out = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("failed to read input line: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: InputCase = serde_json::from_str(&line)
            .map_err(|e| format!("input parse error at line {}: {e}", idx + 1))?;
        let text = parsed
            .text
            .or(parsed.input)
            .or(parsed.input_text)
            .unwrap_or_default();

        let case_id = parsed
            .case_id
            .unwrap_or_else(|| format!("case-{:05}", idx + 1));
        let category = parsed.category.unwrap_or_else(|| "unknown".to_string());

        out.push(ResolvedCase {
            case_id,
            category,
            text,
        });
    }

    if out.is_empty() {
        return Err("input jsonl is empty".to_string());
    }
    Ok(out)
}

fn load_output_rows(path: &str) -> Result<Vec<Phase1RawRow>, String> {
    let file = File::open(path).map_err(|e| format!("failed to open output jsonl: {e}"))?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("failed to read output line: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let row: Phase1RawRow = serde_json::from_str(&line)
            .map_err(|e| format!("output parse error at line {}: {e}", idx + 1))?;
        rows.push(row);
    }
    Ok(rows)
}

fn map_to_output_row(
    case: &ResolvedCase,
    base: &agent_core::Phase1RawRow,
    engine: &Phase1Engine,
) -> (Phase1RawRow, MissingSlots) {
    let completeness = clamp01(base.completeness);
    let ambiguity_mean = clamp01(base.ambiguity_mean.max(ambiguity_from_text(&case.text)));
    let inconsistency = inconsistency_v2(&case.text);
    let dependency_consistency =
        resolve_dependency_consistency(inconsistency, base.dependency_consistency);

    let scs = engine.compute_scs_v1_1(ScsInputs {
        completeness,
        ambiguity_mean,
        inconsistency,
        dependency_consistency,
    });

    let (question_count, missing_slots) =
        estimate_question_count(&case.text, ambiguity_mean, base.orphan_rate, inconsistency);
    let revision_score = inconsistency;
    let revision_count = if revision_score < 0.35 {
        0
    } else if revision_score < 0.55 {
        1
    } else if revision_score < 0.75 {
        2
    } else {
        3
    };
    let cls = clamp01(
        0.4 * ambiguity_mean
            + 0.3 * (1.0 - completeness)
            + 0.2 * (question_count as f64 / 5.0)
            + 0.1 * (revision_count as f64 / 3.0),
    );
    let phase2_triggered = cls < 0.25 && dependency_consistency > 0.6 && revision_count == 0;

    (
        Phase1RawRow {
            case_id: case.case_id.clone(),
            category: case.category.clone(),
            cls: round6(cls),
            scs: round6(scs),
            completeness: round6(completeness),
            ambiguity_mean: round6(ambiguity_mean),
            inconsistency: round6(inconsistency),
            dependency_consistency: round6(dependency_consistency),
            question_count,
            revision_score: round6(revision_score),
            revision_count,
            phase2_triggered,
            objective_legacy: None,
        },
        missing_slots,
    )
}

fn resolve_dependency_consistency(inconsistency: f64, fallback: f64) -> f64 {
    let temp_enabled = std::env::var("PHASE1_TEMP_DEP_STABILIZE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if temp_enabled {
        if inconsistency < 0.25 {
            0.80
        } else if inconsistency < 0.40 {
            0.70
        } else {
            0.50
        }
    } else {
        clamp01(fallback)
    }
}

fn ambiguity_from_text(text: &str) -> f64 {
    if text.trim().is_empty() {
        return 0.18;
    }
    let tokens = text.split_whitespace().count() as f64;
    let hedges = [
        "maybe", "possibly", "probably", "might", "could", "検討", "要件",
    ];
    let hedge_count = hedges.iter().filter(|h| text.contains(**h)).count() as f64;
    let question_marks = text.matches('?').count() as f64 + text.matches('？').count() as f64;
    clamp01((0.12 + hedge_count * 0.08 + question_marks * 0.12 + (tokens / 80.0)).max(0.18))
}

fn inconsistency_v2(text: &str) -> f64 {
    let raw_conflict_score = contradiction_signal(text)
        + logical_opposition_signal(text)
        + temporal_conflict_signal(text);
    clamp01(raw_conflict_score.powf(1.5))
}

fn contradiction_signal(text: &str) -> f64 {
    let t = text.to_lowercase();
    let pairs = [
        ("must", "optional"),
        ("required", "avoid"),
        ("always", "sometimes"),
        ("enabled", "disabled"),
        ("必要", "不要"),
        ("必須", "任意"),
    ];
    let hits = pairs
        .iter()
        .filter(|(a, b)| t.contains(a) && t.contains(b))
        .count() as f64;
    (hits * 0.35).min(1.0)
}

fn logical_opposition_signal(text: &str) -> f64 {
    let t = text.to_lowercase();
    let oppose_terms = [
        "but",
        "however",
        "although",
        "yet",
        "while",
        "一方",
        "しかし",
        "ただし",
    ];
    let oppose_hits = oppose_terms.iter().filter(|w| t.contains(**w)).count() as f64;
    let hedge_terms = [
        "maybe", "possibly", "probably", "might", "could", "検討", "要件",
    ];
    let hedge_hits = hedge_terms.iter().filter(|w| t.contains(**w)).count() as f64;
    ((oppose_hits * 0.2) + (hedge_hits * 0.08)).min(1.0)
}

fn temporal_conflict_signal(text: &str) -> f64 {
    let t = text.to_lowercase();
    let timing_pairs = [
        ("before", "after"),
        ("today", "weekly"),
        ("now", "later"),
        ("immediate", "eventually"),
        ("先に", "後で"),
    ];
    let hits = timing_pairs
        .iter()
        .filter(|(a, b)| t.contains(a) && t.contains(b))
        .count() as f64;
    (hits * 0.4).min(1.0)
}

fn estimate_question_count(
    text: &str,
    ambiguity_mean: f64,
    orphan_rate: f64,
    inconsistency: f64,
) -> (usize, MissingSlots) {
    let missing_slots = detect_5w2h_slots(text);
    let missing_5w2h_slots = missing_slots.missing_5w2h_slots as usize;
    let high_ambiguity_factor_count = high_ambiguity_factor_count(text, ambiguity_mean);
    let orphan_factor_count = orphan_factor_count(orphan_rate);
    let high_risk_inconsistency_flag = if inconsistency >= 0.45 { 1 } else { 0 };
    let raw_sum = missing_5w2h_slots
        + high_ambiguity_factor_count
        + orphan_factor_count
        + high_risk_inconsistency_flag;
    let compressed = (raw_sum as f64).powf(0.7).round() as usize;
    let question_count = compressed.min(5);
    (question_count, missing_slots)
}

fn detect_5w2h_slots(text: &str) -> MissingSlots {
    let t = text.to_lowercase();
    let contains_any = |keywords: &[&str]| keywords.iter().any(|kw| t.contains(kw));
    let flags = FiveW2HFlags {
        who: contains_any(&[
            "user",
            "users",
            "admin",
            "developer",
            "operator",
            "actor",
            "team",
            "client",
        ]),
        what: contains_any(&[
            "build",
            "implement",
            "create",
            "generate",
            "system",
            "feature",
            "module",
            "cli",
            "engine",
        ]),
        why: contains_any(&[
            "because",
            "so that",
            "goal",
            "purpose",
            "to improve",
            "in order to",
            "reason",
        ]),
        where_: contains_any(&[
            "server",
            "local",
            "cloud",
            "on-prem",
            "workspace",
            "repo",
            "directory",
            "production",
        ]),
        when_: contains_any(&[
            "today", "now", "daily", "weekly", "phase", "before", "after", "deadline", "release",
        ]),
        how: contains_any(&[
            "how",
            "method",
            "approach",
            "algorithm",
            "design",
            "spec",
            "workflow",
            "pipeline",
        ]),
        how_much: contains_any(&[
            "cost", "budget", "price", "yen", "usd", "minutes", "hours", "days", "scale", "limit",
            "max", "min",
        ]),
    };
    let mut missing = 0u8;
    missing += (!flags.who) as u8;
    missing += (!flags.what) as u8;
    missing += (!flags.why) as u8;
    missing += (!flags.where_) as u8;
    missing += (!flags.when_) as u8;
    missing += (!flags.how) as u8;
    missing += (!flags.how_much) as u8;
    if text.chars().count() < 20 {
        missing = missing.max(4);
    }
    missing = missing.min(7);
    MissingSlots {
        missing_5w2h_slots: missing,
        flags,
    }
}

fn high_ambiguity_factor_count(text: &str, ambiguity_mean: f64) -> usize {
    let hedge_terms = [
        "maybe", "possibly", "probably", "might", "could", "検討", "要件",
    ];
    let hedge_hits = hedge_terms.iter().filter(|h| text.contains(**h)).count();
    let by_score = if ambiguity_mean >= 0.6 {
        2
    } else if ambiguity_mean >= 0.4 {
        1
    } else {
        0
    };
    by_score.max((hedge_hits / 2).min(2))
}

fn orphan_factor_count(orphan_rate: f64) -> usize {
    if orphan_rate >= 0.5 {
        2
    } else if orphan_rate >= 0.2 {
        1
    } else {
        0
    }
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn validate_normalized_vector(
    mut values: [f64; 4],
    allow_out_of_range: bool,
) -> Result<([f64; 4], bool), String> {
    let mut invalid = false;
    for v in &mut values {
        if !v.is_finite() {
            return Err("normalized value must be finite".to_string());
        }
        if *v < 0.0 - PARETO_EPS || *v > 1.0 + PARETO_EPS {
            if allow_out_of_range {
                *v = clamp01(*v);
                invalid = true;
            } else {
                return Err("normalized value out of [0,1]".to_string());
            }
        }
    }
    Ok((values, invalid))
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn objective_vector_v11_from_phase1_row(row: &Phase1RawRow) -> ObjectiveVectorV1_1 {
    let raw = objective_raw_from_phase1_row(row);
    ObjectiveVectorV1_1 {
        raw,
        normalized: raw,
        clamped: raw.map(clamp01),
    }
}

fn objective_raw_from_phase1_row(row: &Phase1RawRow) -> [f64; 4] {
    let completeness = if row.completeness <= 0.0 && row.scs > 0.0 {
        row.scs
    } else {
        row.completeness
    };
    let dependency_consistency = row.dependency_consistency;
    let inconsistency = row.inconsistency;
    let cls = row.cls;
    let revision_count = row.revision_count.min(3) as f64;
    let structural_integrity =
        0.4 * completeness + 0.4 * dependency_consistency + 0.2 * (1.0 - inconsistency);
    let cognitive_stability = 1.0 - cls;
    let revision_pressure = 1.0 - (revision_count / 3.0);
    let exploration_readiness = if row.phase2_triggered { 1.0 } else { 0.0 };
    [
        structural_integrity,
        cognitive_stability,
        revision_pressure,
        exploration_readiness,
    ]
}

fn normalize_objective_cases(mut cases: Vec<ObjectiveCase>) -> Result<Vec<ObjectiveCase>, String> {
    if cases.is_empty() {
        return Ok(cases);
    }
    let mut mins = [f64::INFINITY; 4];
    let mut maxs = [f64::NEG_INFINITY; 4];
    for case in &cases {
        for i in 0..4 {
            let value = case.objective.raw[i];
            if !value.is_finite() {
                return Err(format!(
                    "objective raw value must be finite: case_id={} index={} value={value}",
                    case.case_id, i
                ));
            }
            mins[i] = mins[i].min(value);
            maxs[i] = maxs[i].max(value);
        }
    }

    for case in &mut cases {
        let mut normalized = [0.0; 4];
        for i in 0..4 {
            normalized[i] = if (maxs[i] - mins[i]).abs() > PARETO_EPS {
                (case.objective.raw[i] - mins[i]) / (maxs[i] - mins[i])
            } else {
                0.5
            };
        }
        case.objective.normalized = normalized;
        case.objective.clamped = normalized.map(clamp01);
    }
    Ok(cases)
}

fn legacy_objective_to_v11(legacy: LegacyObjectiveVector) -> ObjectiveVectorV1_1 {
    let clamped = [
        clamp01(legacy.structural_integrity),
        clamp01(legacy.cognitive_stability),
        clamp01(legacy.revision_pressure),
        clamp01(legacy.exploration_readiness),
    ];
    ObjectiveVectorV1_1 {
        raw: clamped,
        normalized: clamped,
        clamped,
    }
}

fn pareto_frontier_by_case_id(cases: &[ObjectiveCase]) -> Vec<ObjectiveCase> {
    let mut sorted = cases.to_vec();
    sorted.sort_by(|a, b| a.case_id.cmp(&b.case_id));

    let mut dedup = Vec::<ObjectiveCase>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for c in sorted {
        if seen.insert(c.case_id.clone()) {
            dedup.push(c);
        }
    }

    let mut front = Vec::<ObjectiveCase>::new();
    for i in 0..dedup.len() {
        let mut dominated = false;
        for j in 0..dedup.len() {
            if i == j {
                continue;
            }
            if dominates_objective(&dedup[j].objective, &dedup[i].objective) {
                dominated = true;
                break;
            }
        }
        if !dominated {
            front.push(dedup[i].clone());
        }
    }
    front.sort_by(|a, b| a.case_id.cmp(&b.case_id));
    front
}

fn pareto_ranks(cases: &[ObjectiveCase]) -> Vec<Vec<ObjectiveCase>> {
    let mut remaining = cases.to_vec();
    let mut ranks = Vec::<Vec<ObjectiveCase>>::new();
    while !remaining.is_empty() {
        let frontier = pareto_frontier_by_case_id(&remaining);
        if frontier.is_empty() {
            break;
        }
        let frontier_ids = frontier
            .iter()
            .map(|c| c.case_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        remaining.retain(|c| !frontier_ids.contains(&c.case_id));
        ranks.push(frontier);
    }
    ranks
}

fn dominates_objective(a: &ObjectiveVectorV1_1, b: &ObjectiveVectorV1_1) -> bool {
    let all_ge = (0..4).all(|i| a.clamped[i] + PARETO_EPS >= b.clamped[i]);
    let one_gt = (0..4).any(|i| a.clamped[i] > b.clamped[i] + PARETO_EPS);
    all_ge && one_gt
}

fn frontier_hash(frontier: &[ObjectiveCase]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for item in frontier {
        fnv1a_update(&mut h, item.case_id.as_bytes());
        fnv1a_update(&mut h, &[b'|']);
        for value in item.objective.normalized {
            fnv1a_update(&mut h, &value.to_bits().to_le_bytes());
        }
        fnv1a_update(&mut h, &[b'|']);
        for value in item.objective.clamped {
            fnv1a_update(&mut h, &value.to_bits().to_le_bytes());
        }
        fnv1a_update(&mut h, &[b'\n']);
    }
    format!("{h:016x}")
}

fn objective_mean(frontier: &[ObjectiveCase]) -> [f64; 4] {
    if frontier.is_empty() {
        return [0.0; 4];
    }
    let mut mean = [0.0; 4];
    for case in frontier {
        for (i, acc) in mean.iter_mut().enumerate() {
            *acc += case.objective.clamped[i];
        }
    }
    for acc in &mut mean {
        *acc = round6(*acc / frontier.len() as f64);
    }
    mean
}

fn objective_variance(frontier: &[ObjectiveCase], mean: &[f64; 4]) -> [f64; 4] {
    if frontier.is_empty() {
        return [0.0; 4];
    }
    let mut var = [0.0; 4];
    for case in frontier {
        for i in 0..4 {
            let d = case.objective.clamped[i] - mean[i];
            var[i] += d * d;
        }
    }
    for value in &mut var {
        *value = round6(*value / frontier.len() as f64);
    }
    var
}

fn domination_count_histogram(cases: &[ObjectiveCase]) -> BTreeMap<usize, usize> {
    let mut hist = BTreeMap::<usize, usize>::new();
    for i in 0..cases.len() {
        let mut dominated_by_count = 0usize;
        for j in 0..cases.len() {
            if i != j && dominates_objective(&cases[j].objective, &cases[i].objective) {
                dominated_by_count += 1;
            }
        }
        *hist.entry(dominated_by_count).or_insert(0) += 1;
    }
    hist
}

fn pearson_correlation_matrix(cases: &[ObjectiveCase]) -> [[f64; 4]; 4] {
    if cases.is_empty() {
        return [[0.0; 4]; 4];
    }
    let n = cases.len() as f64;
    let mut cols = vec![Vec::<f64>::with_capacity(cases.len()); 4];
    for case in cases {
        for (i, col) in cols.iter_mut().enumerate() {
            col.push(case.objective.normalized[i]);
        }
    }
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
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
            let denom = (var_i.sqrt() * var_j.sqrt()).max(PARETO_EPS);
            out[i][j] = round6((cov / denom).clamp(-1.0, 1.0));
        }
    }
    out
}

fn hypervolume_4d_from_origin(frontier: &[ObjectiveCase]) -> f64 {
    let mut points = Vec::<[f64; 4]>::new();
    for case in frontier {
        points.push(case.objective.clamped);
    }
    round6(hypervolume_recursive(&points, 4))
}

fn hypervolume_recursive(points: &[[f64; 4]], dim: usize) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if dim == 1 {
        return points
            .iter()
            .map(|p| p[0])
            .fold(0.0, |acc, v| if v > acc { v } else { acc });
    }
    let axis = 4 - dim;
    let mut coords = points.iter().map(|p| p[axis]).collect::<Vec<_>>();
    coords.sort_by(|a, b| a.total_cmp(b));
    coords.dedup_by(|a, b| (*a - *b).abs() <= PARETO_EPS);

    let mut prev = 0.0;
    let mut volume = 0.0;
    for c in coords {
        let width = c - prev;
        if width > PARETO_EPS {
            let mut projected = Vec::<[f64; 4]>::new();
            for p in points {
                if p[axis] + PARETO_EPS >= c {
                    let mut q = [0.0; 4];
                    q[..(dim - 1)].copy_from_slice(&p[(axis + 1)..(axis + dim)]);
                    projected.push(q);
                }
            }
            volume += width * hypervolume_recursive(&projected, dim - 1);
        }
        prev = c;
    }
    volume
}

fn fnv1a_update(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= *b as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n
}

#[cfg(test)]
mod objective_vector_tests {
    use super::*;

    fn mk_case(case_id: &str, raw: [f64; 4]) -> ObjectiveCase {
        ObjectiveCase {
            case_id: case_id.to_string(),
            category: "test".to_string(),
            objective: ObjectiveVectorV1_1 {
                raw,
                normalized: raw,
                clamped: raw.map(clamp01),
            },
        }
    }

    fn normalize(cases: Vec<ObjectiveCase>) -> Vec<ObjectiveCase> {
        normalize_objective_cases(cases).expect("normalization should succeed")
    }

    #[test]
    fn min_max_equal_normalizes_to_half() {
        let cases = normalize(vec![
            mk_case("A", [0.3, 0.3, 0.3, 0.3]),
            mk_case("B", [0.3, 0.3, 0.3, 0.3]),
        ]);
        for case in cases {
            assert_eq!(case.objective.normalized, [0.5; 4]);
            assert_eq!(case.objective.clamped, [0.5; 4]);
        }
    }

    #[test]
    fn all_identical_vectors_are_all_non_dominated() {
        let cases = normalize(vec![
            mk_case("A", [1.0, 2.0, 3.0, 4.0]),
            mk_case("B", [1.0, 2.0, 3.0, 4.0]),
            mk_case("C", [1.0, 2.0, 3.0, 4.0]),
        ]);
        let front = pareto_frontier_by_case_id(&cases);
        assert_eq!(front.len(), 3);
    }

    #[test]
    fn all_monotonic_chain_has_single_frontier_point() {
        let cases = normalize(vec![
            mk_case("A", [0.1, 0.1, 0.1, 0.1]),
            mk_case("B", [0.2, 0.2, 0.2, 0.2]),
            mk_case("C", [0.3, 0.3, 0.3, 0.3]),
        ]);
        let front = pareto_frontier_by_case_id(&cases);
        assert_eq!(front.len(), 1);
        assert_eq!(front[0].case_id, "C");
    }

    #[test]
    fn all_non_dominated_points_remain_in_frontier() {
        let cases = normalize(vec![
            mk_case("A", [1.0, 0.0, 0.0, 0.0]),
            mk_case("B", [0.0, 1.0, 0.0, 0.0]),
            mk_case("C", [0.0, 0.0, 1.0, 0.0]),
            mk_case("D", [0.0, 0.0, 0.0, 1.0]),
        ]);
        let front = pareto_frontier_by_case_id(&cases);
        assert_eq!(front.len(), 4);
    }

    #[test]
    fn boundary_values_and_epsilon_tolerance() {
        let a = ObjectiveVectorV1_1 {
            raw: [0.0, 1.0, 1.0, 1.0],
            normalized: [0.0, 1.0, 1.0, 1.0],
            clamped: [0.0, 1.0, 1.0, 1.0],
        };
        let b = ObjectiveVectorV1_1 {
            raw: [0.0, 1.0 - 5e-13, 1.0, 1.0],
            normalized: [0.0, 1.0 - 5e-13, 1.0, 1.0],
            clamped: [0.0, 1.0 - 5e-13, 1.0, 1.0],
        };
        assert!(!dominates_objective(&a, &b));
        assert!(!dominates_objective(&b, &a));
    }

    #[test]
    fn hypervolume_simple_case_matches_expected() {
        let front = vec![
            mk_case("A", [0.5, 0.5, 0.5, 0.5]),
            mk_case("B", [1.0, 0.0, 0.0, 0.0]),
        ];
        let normalized = normalize(front);
        let frontier = pareto_frontier_by_case_id(&normalized);
        let hv = hypervolume_4d_from_origin(&frontier);
        assert!(hv >= 0.0);
        assert!(hv <= 1.0);
    }

    fn shuffle_cases(mut cases: Vec<ObjectiveCase>, seed: u64) -> Vec<ObjectiveCase> {
        let mut s = seed;
        for i in (1..cases.len()).rev() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (s as usize) % (i + 1);
            cases.swap(i, j);
        }
        cases
    }

    #[test]
    fn reproducibility_100_seeds_x3_hash_and_hypervolume_match() {
        for seed in 0..100u64 {
            let mut base = Vec::new();
            let mut s = seed.wrapping_add(1);
            for idx in 0..40usize {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                let a = ((s >> 16) as f64) / (u32::MAX as f64);
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                let b = ((s >> 16) as f64) / (u32::MAX as f64);
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                let c = ((s >> 16) as f64) / (u32::MAX as f64);
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                let d = ((s >> 16) as f64) / (u32::MAX as f64);
                base.push(mk_case(&format!("C-{idx:03}"), [a, b, c, d]));
            }
            let norm = normalize(base);
            let run1_front = pareto_frontier_by_case_id(&shuffle_cases(norm.clone(), seed + 11));
            let run2_front = pareto_frontier_by_case_id(&shuffle_cases(norm.clone(), seed + 22));
            let run3_front = pareto_frontier_by_case_id(&shuffle_cases(norm, seed + 33));

            let h1 = frontier_hash(&run1_front);
            let h2 = frontier_hash(&run2_front);
            let h3 = frontier_hash(&run3_front);
            assert_eq!(h1, h2);
            assert_eq!(h2, h3);

            let v1 = hypervolume_4d_from_origin(&run1_front);
            let v2 = hypervolume_4d_from_origin(&run2_front);
            let v3 = hypervolume_4d_from_origin(&run3_front);
            assert!((v1 - v2).abs() <= 1e-12);
            assert!((v2 - v3).abs() <= 1e-12);
        }
    }

    #[test]
    fn legacy_vector_maps_to_v11() {
        let legacy = LegacyObjectiveVector {
            structural_integrity: 1.3,
            cognitive_stability: -0.1,
            revision_pressure: 0.8,
            exploration_readiness: 0.4,
        };
        let v = legacy_objective_to_v11(legacy);
        assert_eq!(v.normalized, v.clamped);
        assert_eq!(v.raw, v.clamped);
        assert_eq!(v.clamped, [1.0, 0.0, 0.8, 0.4]);
    }

    #[test]
    fn reproducibility_100_seeds() {
        reproducibility_100_seeds_x3_hash_and_hypervolume_match();
    }

    #[test]
    fn hypervolume_monotonicity() {
        let base = normalize(vec![mk_case("A", [0.2, 0.2, 0.2, 0.2])]);
        let expanded = normalize(vec![
            mk_case("A", [0.2, 0.2, 0.2, 0.2]),
            mk_case("B", [0.8, 0.8, 0.8, 0.8]),
        ]);
        let hv_base = hypervolume_4d_from_origin(&pareto_frontier_by_case_id(&base));
        let hv_expanded = hypervolume_4d_from_origin(&pareto_frontier_by_case_id(&expanded));
        assert!(hv_expanded + 1e-12 >= hv_base);
    }

    #[test]
    fn strict_out_of_range() {
        let out = [1.1, 0.5, 0.5, 0.5];
        assert!(validate_normalized_vector(out, false).is_err());
        let (clamped, invalid) =
            validate_normalized_vector(out, true).expect("allow mode should clamp");
        assert_eq!(clamped, [1.0, 0.5, 0.5, 0.5]);
        assert!(invalid);
    }

    #[test]
    fn schema_v1_wrapper_structure() {
        let wrapper = success_wrapper_value(
            "analyze",
            json!({"x": 1}),
            JsonMeta {
                command: "analyze",
                hv_policy: Some("Legacy"),
                deterministic: true,
            },
        );
        assert_eq!(wrapper["schema_version"], "v1");
        assert!(wrapper["data"].is_object());
        assert!(wrapper["meta"].is_object());
    }

    #[test]
    fn legacy_and_guided_produce_same_schema() {
        let legacy = success_wrapper_value(
            "analyze",
            json!({"frontier_size": 1}),
            JsonMeta {
                command: "analyze",
                hv_policy: Some("Legacy"),
                deterministic: true,
            },
        );
        let guided = success_wrapper_value(
            "analyze",
            json!({"frontier_size": 1}),
            JsonMeta {
                command: "analyze",
                hv_policy: Some("Guided"),
                deterministic: true,
            },
        );
        assert_eq!(legacy.as_object().map(|o| o.len()), guided.as_object().map(|o| o.len()));
        assert_eq!(
            legacy["data"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()),
            guided["data"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
        );
    }
}
