use std::env;
use std::fs;
use std::path::PathBuf;

use agent_core::{
    generate_trace, generate_trace_baseline_off, generate_trace_baseline_off_balanced, generate_trace_baseline_off_soft,
    run_bench, run_bench_baseline_off, run_bench_baseline_off_balanced, run_bench_baseline_off_soft, run_phase1_matrix,
    BenchConfig, BenchResult, Phase1Config, Phase1RawRow, Phase1SummaryRow, TraceRow, TraceRunConfig,
};
use interface_ui::{UiEvent, UserInterface, VmBridge};

const MAX_DEPTH: usize = 1000;
const MAX_BEAM: usize = 100;
const MAX_BENCH_ITER: usize = 1000;

#[derive(Clone, Debug)]
pub struct TraceConfig {
    pub enabled: bool,
    pub output: Option<PathBuf>,
    pub depth: usize,
    pub beam: usize,
    pub baseline_off: bool,
    pub category_balanced: bool,
    pub category_m: usize,
    pub category_soft: bool,
    pub category_alpha: f64,
    pub temperature: f64,
    pub entropy_beta: f64,
    pub lambda_min: f64,
    pub lambda_target_entropy: f64,
    pub lambda_k: f64,
    pub lambda_ema: f64,
    pub log_per_depth: bool,
    pub field_profile: bool,
}

#[derive(Clone, Debug)]
pub struct BenchCliConfig {
    pub enabled: bool,
    pub depth: usize,
    pub beam: usize,
    pub iter: usize,
    pub warmup: usize,
    pub depth_set: bool,
    pub beam_set: bool,
    pub baseline_off: bool,
    pub category_balanced: bool,
    pub category_m: usize,
    pub category_soft: bool,
    pub category_alpha: f64,
    pub temperature: f64,
    pub entropy_beta: f64,
    pub lambda_min: f64,
    pub lambda_target_entropy: f64,
    pub lambda_k: f64,
    pub lambda_ema: f64,
    pub log_per_depth: bool,
    pub field_profile: bool,
}

struct CliUi {
    bridge: VmBridge,
}

impl CliUi {
    fn new() -> Self {
        Self {
            bridge: VmBridge::new(),
        }
    }
}

impl UserInterface for CliUi {
    fn render(&mut self) {
        println!("tick={}", self.bridge.current_tick());
    }

    fn handle_input(&mut self, input: UiEvent) {
        if let UiEvent::Tick = input {
            self.bridge.tick();
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let trace_cfg = parse_trace_config(&args);
    let bench_cfg = parse_bench_config(&args);
    validate_cli_configs(&trace_cfg, &bench_cfg);

    if bench_cfg.enabled {
        run_bench_mode(&bench_cfg);
        return;
    }

    if args.iter().any(|a| a == "--phase1") {
        run_phase1_mode();
        return;
    }

    if trace_cfg.enabled {
        let rows = if trace_cfg.baseline_off {
            if trace_cfg.category_soft {
                generate_trace_baseline_off_soft(
                    TraceRunConfig {
                        depth: trace_cfg.depth,
                        beam: trace_cfg.beam,
                        seed: 42,
                    },
                    trace_cfg.category_alpha,
                    trace_cfg.temperature,
                    trace_cfg.entropy_beta,
                    trace_cfg.lambda_min,
                    trace_cfg.lambda_target_entropy,
                    trace_cfg.lambda_k,
                    trace_cfg.lambda_ema,
                    trace_cfg.field_profile,
                )
            } else if trace_cfg.category_balanced {
                generate_trace_baseline_off_balanced(
                    TraceRunConfig {
                        depth: trace_cfg.depth,
                        beam: trace_cfg.beam,
                        seed: 42,
                    },
                    trace_cfg.category_m,
                )
            } else {
                generate_trace_baseline_off(TraceRunConfig {
                    depth: trace_cfg.depth,
                    beam: trace_cfg.beam,
                    seed: 42,
                })
            }
        } else {
            generate_trace(TraceRunConfig {
                depth: trace_cfg.depth,
                beam: trace_cfg.beam,
                seed: 42,
            })
        };
        let rows = if trace_cfg.log_per_depth {
            rows
        } else {
            rows.into_iter().last().into_iter().collect()
        };

        let csv = render_csv(&rows);
        if let Some(path) = trace_cfg.output {
            fs::write(&path, csv).expect("failed to write trace output");
            println!("trace written: {}", path.display());
        } else {
            print!("{csv}");
        }
        return;
    }

    let mut ui = CliUi::new();
    ui.render();
    ui.handle_input(UiEvent::Tick);
    ui.render();
}

fn run_phase1_mode() {
    let cfg = Phase1Config {
        depth: 100,
        beam: 5,
        seed: 42,
        alpha: 3.0,
        temperature: 0.8,
        entropy_beta: 0.0,
        lambda_min: 0.05,
        lambda_target_entropy: 0.8,
        lambda_k: 0.05,
        lambda_ema: 0.1,
    };
    let (raw, summary) = run_phase1_matrix(cfg);
    fs::create_dir_all("report").expect("failed to create report directory");
    fs::write("report/trace_phase1_raw.csv", render_phase1_raw_csv(&raw)).expect("failed to write trace_phase1_raw.csv");
    fs::write("report/trace_phase1_summary.csv", render_phase1_summary_csv(&summary))
        .expect("failed to write trace_phase1_summary.csv");
    println!("phase1 written: report/trace_phase1_raw.csv");
    println!("phase1 written: report/trace_phase1_summary.csv");
}

fn render_phase1_raw_csv(rows: &[Phase1RawRow]) -> String {
    let mut out = String::from("variant,depth,beam_index,rule_id,objective_vector_raw,objective_vector_norm\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},\"{}\",\"{}\"\n",
            r.variant, r.depth, r.beam_index, r.rule_id, r.objective_vector_raw, r.objective_vector_norm
        ));
    }
    out
}

fn render_phase1_summary_csv(rows: &[Phase1SummaryRow]) -> String {
    let mut out = String::from("variant,depth,corr_matrix_flat,mean_nn_dist,spacing,pareto_front_size,collapse_flag\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},\"{}\",{:.9},{:.9},{},{}\n",
            r.variant, r.depth, r.corr_matrix_flat, r.mean_nn_dist, r.spacing, r.pareto_front_size, r.collapse_flag
        ));
    }
    out
}

fn run_bench_mode(cfg: &BenchCliConfig) {
    if cfg!(debug_assertions) {
        eprintln!("warning: benchmark should be run with --release for valid numbers");
    }

    let presets = [(10usize, 5usize), (50, 5), (50, 10), (100, 5)];
    let runs: Vec<(usize, usize)> = if !cfg.depth_set && !cfg.beam_set {
        presets.to_vec()
    } else {
        vec![(cfg.depth, cfg.beam)]
    };

    for (depth, beam) in runs {
        let result = if cfg.baseline_off {
            if cfg.category_soft {
                run_bench_baseline_off_soft(
                    BenchConfig {
                        depth,
                        beam,
                        iterations: cfg.iter,
                        warmup: cfg.warmup,
                        seed: 42,
                    },
                    cfg.category_alpha,
                    cfg.temperature,
                    cfg.entropy_beta,
                    cfg.lambda_min,
                    cfg.lambda_target_entropy,
                    cfg.lambda_k,
                    cfg.lambda_ema,
                    cfg.field_profile,
                )
            } else if cfg.category_balanced {
                run_bench_baseline_off_balanced(
                    BenchConfig {
                        depth,
                        beam,
                        iterations: cfg.iter,
                        warmup: cfg.warmup,
                        seed: 42,
                    },
                    cfg.category_m,
                )
            } else {
                run_bench_baseline_off(BenchConfig {
                    depth,
                    beam,
                    iterations: cfg.iter,
                    warmup: cfg.warmup,
                    seed: 42,
                })
            }
        } else {
            run_bench(BenchConfig {
                depth,
                beam,
                iterations: cfg.iter,
                warmup: cfg.warmup,
                seed: 42,
            })
        };
        print_bench_result(&result);
    }
}

fn print_bench_result(r: &BenchResult) {
    let phase_sum_us =
        r.avg_field_us + r.avg_resonance_us + r.avg_chm_us + r.avg_dhm_us + r.avg_pareto_us + r.avg_lambda_us;
    let resonance_ratio = if phase_sum_us > 0.0 {
        r.avg_resonance_us / phase_sum_us
    } else {
        0.0
    };
    let chm_ratio = if phase_sum_us > 0.0 {
        r.avg_chm_us / phase_sum_us
    } else {
        0.0
    };
    let dhm_ratio = if phase_sum_us > 0.0 {
        r.avg_dhm_us / phase_sum_us
    } else {
        0.0
    };

    println!("=== Bench Result ===");
    println!("depth: {}", r.depth);
    println!("beam: {}", r.beam);
    println!("iterations: {}", r.iterations);
    println!("avg_total_ms: {:.3}", r.avg_total_ms);
    println!("avg_per_depth_ms: {:.3}", r.avg_per_depth_ms);
    println!("avg_field_us: {:.3}", r.avg_field_us);
    println!("avg_resonance_us: {:.3}", r.avg_resonance_us);
    println!("avg_chm_us: {:.3}", r.avg_chm_us);
    println!("avg_dhm_us: {:.3}", r.avg_dhm_us);
    println!("avg_pareto_us: {:.3}", r.avg_pareto_us);
    println!("avg_lambda_us: {:.3}", r.avg_lambda_us);
    println!("lambda_final: {:.6}", r.lambda_final);
    println!("resonance_ratio: {:.4}", resonance_ratio);
    println!("chm_ratio: {:.4}", chm_ratio);
    println!("dhm_ratio: {:.4}", dhm_ratio);
}

fn parse_trace_config(args: &[String]) -> TraceConfig {
    let mut enabled = false;
    let mut output = None;
    let mut depth = 50usize;
    let mut beam = 5usize;
    let mut baseline_off = false;
    let mut category_balanced = false;
    let mut category_m = 1usize;
    let mut category_soft = false;
    let mut category_alpha = 3.0f64;
    let mut temperature = 0.8f64;
    let mut entropy_beta = 0.02f64;
    let mut lambda_min = 0.05f64;
    let mut lambda_target_entropy = 0.8f64;
    let mut lambda_k = 0.05f64;
    let mut lambda_ema = 0.1f64;
    let mut log_per_depth = false;
    let mut field_profile = false;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--trace" => {
                enabled = true;
                i += 1;
            }
            "--trace-output" => {
                if i + 1 >= args.len() {
                    panic!("--trace-output requires a path");
                }
                output = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--trace-depth" => {
                if i + 1 >= args.len() {
                    panic!("--trace-depth requires a number");
                }
                depth = args[i + 1]
                    .parse::<usize>()
                    .expect("--trace-depth must be usize");
                i += 2;
            }
            "--trace-beam" => {
                if i + 1 >= args.len() {
                    panic!("--trace-beam requires a number");
                }
                beam = args[i + 1]
                    .parse::<usize>()
                    .expect("--trace-beam must be usize");
                i += 2;
            }
            "--baseline-off" => {
                baseline_off = true;
                i += 1;
            }
            "--category-balanced" => {
                category_balanced = true;
                i += 1;
            }
            "--category-m" => {
                if i + 1 >= args.len() {
                    panic!("--category-m requires a number");
                }
                category_m = args[i + 1].parse::<usize>().expect("--category-m must be usize");
                i += 2;
            }
            "--category-soft" => {
                category_soft = true;
                i += 1;
            }
            "--category-alpha" => {
                if i + 1 >= args.len() {
                    panic!("--category-alpha requires a number");
                }
                category_alpha = args[i + 1]
                    .parse::<f64>()
                    .expect("--category-alpha must be f64");
                i += 2;
            }
            "--temperature" => {
                if i + 1 >= args.len() {
                    panic!("--temperature requires a number");
                }
                temperature = args[i + 1].parse::<f64>().expect("--temperature must be f64");
                i += 2;
            }
            "--entropy-beta" => {
                if i + 1 >= args.len() {
                    panic!("--entropy-beta requires a number");
                }
                entropy_beta = args[i + 1].parse::<f64>().expect("--entropy-beta must be f64");
                i += 2;
            }
            "--lambda-min" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-min requires a number");
                }
                lambda_min = args[i + 1].parse::<f64>().expect("--lambda-min must be f64");
                i += 2;
            }
            "--lambda-target-entropy" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-target-entropy requires a number");
                }
                lambda_target_entropy = args[i + 1]
                    .parse::<f64>()
                    .expect("--lambda-target-entropy must be f64");
                i += 2;
            }
            "--lambda-k" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-k requires a number");
                }
                lambda_k = args[i + 1].parse::<f64>().expect("--lambda-k must be f64");
                i += 2;
            }
            "--lambda-ema" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-ema requires a number");
                }
                lambda_ema = args[i + 1].parse::<f64>().expect("--lambda-ema must be f64");
                i += 2;
            }
            "--log-per-depth" => {
                log_per_depth = true;
                i += 1;
            }
            "--field-profile" => {
                field_profile = true;
                i += 1;
            }
            _ => i += 1,
        }
    }

    TraceConfig {
        enabled,
        output,
        depth,
        beam,
        baseline_off,
        category_balanced,
        category_m,
        category_soft,
        category_alpha,
        temperature,
        entropy_beta,
        lambda_min,
        lambda_target_entropy,
        lambda_k,
        lambda_ema,
        log_per_depth,
        field_profile,
    }
}

fn parse_bench_config(args: &[String]) -> BenchCliConfig {
    let mut enabled = false;
    let mut depth = 50usize;
    let mut beam = 5usize;
    let mut iter = 3usize;
    let mut warmup = 1usize;
    let mut depth_set = false;
    let mut beam_set = false;
    let mut baseline_off = false;
    let mut category_balanced = false;
    let mut category_m = 1usize;
    let mut category_soft = false;
    let mut category_alpha = 3.0f64;
    let mut temperature = 0.8f64;
    let mut entropy_beta = 0.02f64;
    let mut lambda_min = 0.05f64;
    let mut lambda_target_entropy = 0.8f64;
    let mut lambda_k = 0.05f64;
    let mut lambda_ema = 0.1f64;
    let mut log_per_depth = false;
    let mut field_profile = false;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--bench" => {
                enabled = true;
                i += 1;
            }
            "--bench-depth" => {
                if i + 1 >= args.len() {
                    panic!("--bench-depth requires a number");
                }
                depth = args[i + 1]
                    .parse::<usize>()
                    .expect("--bench-depth must be usize");
                depth_set = true;
                i += 2;
            }
            "--bench-beam" => {
                if i + 1 >= args.len() {
                    panic!("--bench-beam requires a number");
                }
                beam = args[i + 1]
                    .parse::<usize>()
                    .expect("--bench-beam must be usize");
                beam_set = true;
                i += 2;
            }
            "--bench-iter" => {
                if i + 1 >= args.len() {
                    panic!("--bench-iter requires a number");
                }
                iter = args[i + 1]
                    .parse::<usize>()
                    .expect("--bench-iter must be usize");
                i += 2;
            }
            "--bench-warmup" => {
                if i + 1 >= args.len() {
                    panic!("--bench-warmup requires a number");
                }
                warmup = args[i + 1]
                    .parse::<usize>()
                    .expect("--bench-warmup must be usize");
                i += 2;
            }
            "--baseline-off" => {
                baseline_off = true;
                i += 1;
            }
            "--category-balanced" => {
                category_balanced = true;
                i += 1;
            }
            "--category-m" => {
                if i + 1 >= args.len() {
                    panic!("--category-m requires a number");
                }
                category_m = args[i + 1].parse::<usize>().expect("--category-m must be usize");
                i += 2;
            }
            "--category-soft" => {
                category_soft = true;
                i += 1;
            }
            "--category-alpha" => {
                if i + 1 >= args.len() {
                    panic!("--category-alpha requires a number");
                }
                category_alpha = args[i + 1]
                    .parse::<f64>()
                    .expect("--category-alpha must be f64");
                i += 2;
            }
            "--temperature" => {
                if i + 1 >= args.len() {
                    panic!("--temperature requires a number");
                }
                temperature = args[i + 1].parse::<f64>().expect("--temperature must be f64");
                i += 2;
            }
            "--entropy-beta" => {
                if i + 1 >= args.len() {
                    panic!("--entropy-beta requires a number");
                }
                entropy_beta = args[i + 1].parse::<f64>().expect("--entropy-beta must be f64");
                i += 2;
            }
            "--lambda-min" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-min requires a number");
                }
                lambda_min = args[i + 1].parse::<f64>().expect("--lambda-min must be f64");
                i += 2;
            }
            "--lambda-target-entropy" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-target-entropy requires a number");
                }
                lambda_target_entropy = args[i + 1]
                    .parse::<f64>()
                    .expect("--lambda-target-entropy must be f64");
                i += 2;
            }
            "--lambda-k" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-k requires a number");
                }
                lambda_k = args[i + 1].parse::<f64>().expect("--lambda-k must be f64");
                i += 2;
            }
            "--lambda-ema" => {
                if i + 1 >= args.len() {
                    panic!("--lambda-ema requires a number");
                }
                lambda_ema = args[i + 1].parse::<f64>().expect("--lambda-ema must be f64");
                i += 2;
            }
            "--log-per-depth" => {
                log_per_depth = true;
                i += 1;
            }
            "--field-profile" => {
                field_profile = true;
                i += 1;
            }
            _ => i += 1,
        }
    }

    BenchCliConfig {
        enabled,
        depth,
        beam,
        iter,
        warmup,
        depth_set,
        beam_set,
        baseline_off,
        category_balanced,
        category_m,
        category_soft,
        category_alpha,
        temperature,
        entropy_beta,
        lambda_min,
        lambda_target_entropy,
        lambda_k,
        lambda_ema,
        log_per_depth,
        field_profile,
    }
}

fn validate_cli_configs(trace: &TraceConfig, bench: &BenchCliConfig) {
    if trace.depth == 0 || trace.depth > MAX_DEPTH {
        panic!("trace depth must be in 1..={MAX_DEPTH}");
    }
    if trace.beam == 0 || trace.beam > MAX_BEAM {
        panic!("trace beam must be in 1..={MAX_BEAM}");
    }
    if bench.depth == 0 || bench.depth > MAX_DEPTH {
        panic!("bench depth must be in 1..={MAX_DEPTH}");
    }
    if bench.beam == 0 || bench.beam > MAX_BEAM {
        panic!("bench beam must be in 1..={MAX_BEAM}");
    }
    if bench.iter == 0 || bench.iter > MAX_BENCH_ITER {
        panic!("bench iter must be in 1..={MAX_BENCH_ITER}");
    }
    for (name, value) in [
        ("trace.category_alpha", trace.category_alpha),
        ("trace.temperature", trace.temperature),
        ("trace.entropy_beta", trace.entropy_beta),
        ("trace.lambda_min", trace.lambda_min),
        ("trace.lambda_target_entropy", trace.lambda_target_entropy),
        ("trace.lambda_k", trace.lambda_k),
        ("trace.lambda_ema", trace.lambda_ema),
        ("bench.category_alpha", bench.category_alpha),
        ("bench.temperature", bench.temperature),
        ("bench.entropy_beta", bench.entropy_beta),
        ("bench.lambda_min", bench.lambda_min),
        ("bench.lambda_target_entropy", bench.lambda_target_entropy),
        ("bench.lambda_k", bench.lambda_k),
        ("bench.lambda_ema", bench.lambda_ema),
    ] {
        if !value.is_finite() {
            panic!("{name} must be finite");
        }
    }
    if !(0.0..=20.0).contains(&trace.category_alpha) || !(0.0..=20.0).contains(&bench.category_alpha) {
        panic!("category-alpha must be in [0,20]");
    }
    if !(0.0..=1.0).contains(&trace.entropy_beta) || !(0.0..=1.0).contains(&bench.entropy_beta) {
        panic!("entropy-beta must be in [0,1]");
    }
    if !(0.0..=1.0).contains(&trace.lambda_min) || !(0.0..=1.0).contains(&bench.lambda_min) {
        panic!("lambda-min must be in [0,1]");
    }
    if !(0.0..=1.0).contains(&trace.lambda_ema) || !(0.0..=1.0).contains(&bench.lambda_ema) {
        panic!("lambda-ema must be in [0,1]");
    }
    if !(0.0..=1.0).contains(&trace.lambda_k) || !(0.0..=1.0).contains(&bench.lambda_k) {
        panic!("lambda-k must be in [0,1]");
    }
    if trace.temperature <= 0.0 || bench.temperature <= 0.0 {
        panic!("temperature must be > 0");
    }
}

fn render_csv(rows: &[TraceRow]) -> String {
    let mut out = String::from(
        "depth,lambda,delta_lambda,tau_prime,conf_chm,density,k,h_profile,pareto_size,diversity,resonance_avg,pressure,epsilon_effect,target_local_weight,target_global_weight,local_global_distance,field_min_distance,field_rejected_count,mu,dhm_k,dhm_norm,dhm_resonance_mean,dhm_score_ratio,dhm_build_us,expanded_categories_count,selected_rules_count,per_category_selected,entropy_per_depth,unique_category_count_per_depth,pareto_front_size_per_depth,mean_nn_dist,pareto_spacing,pareto_hv_2d,field_extract_us,field_score_us,field_aggregate_us,field_total_us,norm_median_0,norm_median_1,norm_median_2,norm_median_3,norm_mad_0,norm_mad_1,norm_mad_2,norm_mad_3,median_nn_dist_all_depth,collapse_flag,normalization_mode,unique_norm_vec_count,norm_dim_mad_zero_count,mean_nn_dist_raw,mean_nn_dist_norm,pareto_spacing_raw,pareto_spacing_norm,distance_calls,nn_distance_calls\n",
    );

    for row in rows {
        out.push_str(&format!(
            "{},{:.9},{:.9},{:.9},{:.9},{:.9},{},{:.9},{},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{},{:.9},{},{:.9},{:.9},{:.9},{:.9},{},{},\"{}\",{:.9},{},{},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{},\"{}\",{},{},{:.9},{:.9},{:.9},{:.9},{},{}\n",
            row.depth,
            row.lambda,
            row.delta_lambda,
            row.tau_prime,
            row.conf_chm,
            row.density,
            row.k,
            row.h_profile,
            row.pareto_size,
            row.diversity,
            row.resonance_avg,
            row.pressure,
            row.epsilon_effect,
            row.target_local_weight,
            row.target_global_weight,
            row.local_global_distance,
            row.field_min_distance,
            row.field_rejected_count,
            row.mu,
            row.dhm_k,
            row.dhm_norm,
            row.dhm_resonance_mean,
            row.dhm_score_ratio,
            row.dhm_build_us,
            row.expanded_categories_count,
            row.selected_rules_count,
            row.per_category_selected,
            row.entropy_per_depth,
            row.unique_category_count_per_depth,
            row.pareto_front_size_per_depth,
            row.pareto_mean_nn_dist,
            row.pareto_spacing,
            row.pareto_hv_2d,
            row.field_extract_us,
            row.field_score_us,
            row.field_aggregate_us,
            row.field_total_us,
            row.norm_median_0,
            row.norm_median_1,
            row.norm_median_2,
            row.norm_median_3,
            row.norm_mad_0,
            row.norm_mad_1,
            row.norm_mad_2,
            row.norm_mad_3,
            row.median_nn_dist_all_depth,
            row.collapse_flag,
            row.normalization_mode,
            row.unique_norm_vec_count,
            row.norm_dim_mad_zero_count,
            row.mean_nn_dist_raw,
            row.mean_nn_dist_norm,
            row.pareto_spacing_raw,
            row.pareto_spacing_norm,
            row.distance_calls,
            row.nn_distance_calls,
        ));
    }

    out
}
