use std::env;
use std::fs;
use std::path::PathBuf;

use agent_core::{run_bench, generate_trace, BenchConfig, BenchResult, TraceRow, TraceRunConfig};
use interface_ui::{UiEvent, UserInterface, VmBridge};

#[derive(Clone, Debug)]
pub struct TraceConfig {
    pub enabled: bool,
    pub output: Option<PathBuf>,
    pub depth: usize,
    pub beam: usize,
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

    if bench_cfg.enabled {
        run_bench_mode(&bench_cfg);
        return;
    }

    if trace_cfg.enabled {
        let rows = generate_trace(TraceRunConfig {
            depth: trace_cfg.depth,
            beam: trace_cfg.beam,
            seed: 42,
        });

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
        let result = run_bench(BenchConfig {
            depth,
            beam,
            iterations: cfg.iter,
            warmup: cfg.warmup,
            seed: 42,
        });
        print_bench_result(&result);
    }
}

fn print_bench_result(r: &BenchResult) {
    let phase_sum_us = r.avg_field_us + r.avg_resonance_us + r.avg_chm_us + r.avg_pareto_us + r.avg_lambda_us;
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

    println!("=== Bench Result ===");
    println!("depth: {}", r.depth);
    println!("beam: {}", r.beam);
    println!("iterations: {}", r.iterations);
    println!("avg_total_ms: {:.3}", r.avg_total_ms);
    println!("avg_per_depth_ms: {:.3}", r.avg_per_depth_ms);
    println!("avg_field_us: {:.3}", r.avg_field_us);
    println!("avg_resonance_us: {:.3}", r.avg_resonance_us);
    println!("avg_chm_us: {:.3}", r.avg_chm_us);
    println!("avg_pareto_us: {:.3}", r.avg_pareto_us);
    println!("avg_lambda_us: {:.3}", r.avg_lambda_us);
    println!("lambda_final: {:.6}", r.lambda_final);
    println!("resonance_ratio: {:.4}", resonance_ratio);
    println!("chm_ratio: {:.4}", chm_ratio);
}

fn parse_trace_config(args: &[String]) -> TraceConfig {
    let mut enabled = false;
    let mut output = None;
    let mut depth = 50usize;
    let mut beam = 5usize;

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
            _ => i += 1,
        }
    }

    TraceConfig {
        enabled,
        output,
        depth,
        beam,
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
    }
}

fn render_csv(rows: &[TraceRow]) -> String {
    let mut out = String::from(
        "depth,lambda,delta_lambda,tau_prime,conf_chm,density,k,h_profile,pareto_size,diversity,resonance_avg,pressure,epsilon_effect,target_local_weight,target_global_weight,local_global_distance,field_min_distance,field_rejected_count\n",
    );

    for row in rows {
        out.push_str(&format!(
            "{},{:.9},{:.9},{:.9},{:.9},{:.9},{},{:.9},{},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{}\n",
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
        ));
    }

    out
}
