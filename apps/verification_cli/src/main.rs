/// verification_cli — DBM determinism audit engine (spec §5).
///
/// Commands:
///   trace   → run pipeline, write trace.json
///   replay  → reload trace, re-run, write trace_replay.json
///   diff    → compare two traces, print DiffReport
///   audit   → trace → replay → diff → print report  (spec §5.1 audit)
use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use design_search_engine::{BeamSearchController, SearchConfig};
use replay_engine::{capture, diff, replay, FullTrace};
use search_verification::{layered_state, microservice_state, rest_api_state, verification_config};

#[derive(Parser)]
#[command(
    name = "verification_cli",
    about = "DBM determinism audit engine — trace / replay / diff / audit"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the pipeline once and capture a full trace to JSON.
    Trace {
        /// Scenario to run: rest-api | layered | microservice
        #[arg(long, default_value = "rest-api")]
        scenario: String,

        /// Output path for trace.json
        #[arg(long, default_value = "trace.json")]
        output: PathBuf,

        /// Beam width
        #[arg(long = "beam-width", default_value_t = 8)]
        beam_width: usize,

        /// Max search depth
        #[arg(long = "max-depth", default_value_t = 4)]
        max_depth: usize,
    },

    /// Replay a captured trace (uses frozen inputs — no live WebSearch).
    Replay {
        /// Path to trace.json
        trace: PathBuf,

        /// Output path for trace_replay.json
        #[arg(long, default_value = "trace_replay.json")]
        output: PathBuf,
    },

    /// Diff two trace files and report layer-by-layer mismatches.
    Diff {
        /// Path to original trace (trace.json)
        original: PathBuf,

        /// Path to replayed trace (trace_replay.json)
        replayed: PathBuf,

        /// Write DiffReport JSON to this file (omit to print only)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Full audit: trace → replay → diff → print report.
    Audit {
        /// Scenario to run: rest-api | layered | microservice
        #[arg(long, default_value = "rest-api")]
        scenario: String,

        /// Write audit report JSON to this file (omit to print only)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Beam width
        #[arg(long = "beam-width", default_value_t = 8)]
        beam_width: usize,

        /// Max search depth
        #[arg(long = "max-depth", default_value_t = 4)]
        max_depth: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Trace {
            scenario,
            output,
            beam_width,
            max_depth,
        } => {
            let trace = run_trace(&scenario, beam_width, max_depth);
            write_json(&output, &trace);
            eprintln!(
                "[trace] wrote {} search states → {}",
                trace.search.len(),
                output.display()
            );
        }

        Commands::Replay { trace, output } => {
            let original = read_json::<FullTrace>(&trace);
            let controller = BeamSearchController::default();
            let replayed = replay(&original, &controller);
            write_json(&output, &replayed);
            eprintln!(
                "[replay] wrote {} search states → {}",
                replayed.search.len(),
                output.display()
            );
        }

        Commands::Diff {
            original,
            replayed,
            output,
        } => {
            let orig = read_json::<FullTrace>(&original);
            let rep = read_json::<FullTrace>(&replayed);
            let report = diff(&orig, &rep);
            print_report(&report);
            if let Some(path) = output {
                write_json(&path, &report);
            }
            if !report.deterministic {
                process::exit(1);
            }
        }

        Commands::Audit {
            scenario,
            output,
            beam_width,
            max_depth,
        } => {
            eprintln!("[audit] running trace…");
            let ctrl_orig = BeamSearchController::default();
            let orig = run_trace_with_controller(&scenario, beam_width, max_depth, &ctrl_orig);

            // Replay uses a fresh controller so memory starts from the same
            // bootstrap state as the original run (spec §11: same memory snapshot).
            eprintln!("[audit] running replay…");
            let ctrl_replay = BeamSearchController::default();
            let rep = replay(&orig, &ctrl_replay);

            eprintln!("[audit] computing diff…");
            let report = diff(&orig, &rep);

            print_report(&report);

            if let Some(path) = output {
                write_json(&path, &report);
            }

            if report.deterministic {
                eprintln!("[audit] PASS — pipeline is deterministic.");
            } else {
                eprintln!(
                    "[audit] FAIL — {} layer(s) are non-deterministic.",
                    report
                        .layer_diffs
                        .iter()
                        .filter(|d| d.match_status == replay_engine::MatchStatus::Mismatch)
                        .count()
                );
                process::exit(1);
            }
        }
    }
}

// ── Scenario helpers ──────────────────────────────────────────────────────────

fn run_trace(scenario: &str, beam_width: usize, max_depth: usize) -> FullTrace {
    let controller = BeamSearchController::default();
    run_trace_with_controller(scenario, beam_width, max_depth, &controller)
}

fn run_trace_with_controller(
    scenario: &str,
    beam_width: usize,
    max_depth: usize,
    controller: &BeamSearchController,
) -> FullTrace {
    let initial_state = select_scenario(scenario);
    let config = SearchConfig {
        beam_width,
        max_depth,
        ..verification_config(0.15)
    };
    capture(initial_state, config, &[], controller)
}

fn select_scenario(name: &str) -> world_model_core::WorldState {
    match name {
        "layered" => layered_state(),
        "microservice" => microservice_state(),
        _ => rest_api_state(),
    }
}

// ── Report output ─────────────────────────────────────────────────────────────

fn print_report(report: &replay_engine::DiffReport) {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║            DBM Determinism Audit Report              ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();
    println!(
        "  Status: {}",
        if report.deterministic {
            "✓ DETERMINISTIC"
        } else {
            "✗ NON-DETERMINISTIC"
        }
    );
    println!("  Summary: {}", report.summary);
    if let Some(ref class) = report.failure_class {
        println!("  Failure class: {:?}", class);
    }
    println!();
    println!("  Layer results:");
    for d in &report.layer_diffs {
        let icon = if d.match_status == replay_engine::MatchStatus::Match {
            "✓"
        } else {
            "✗"
        };
        println!("    {} {:10}", icon, d.layer);
        for detail in &d.details {
            println!("        {}", detail);
        }
    }
    println!();
}

// ── JSON I/O ──────────────────────────────────────────────────────────────────

fn write_json<T: serde::Serialize>(path: &PathBuf, value: &T) {
    let json = serde_json::to_string_pretty(value).expect("JSON serialization failed");
    fs::write(path, json).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", path.display(), e);
        process::exit(1);
    });
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> T {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {}", path.display(), e);
        process::exit(1);
    });
    serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("error parsing {}: {}", path.display(), e);
        process::exit(1);
    })
}
