use std::env;
use std::fs;
use std::path::PathBuf;

use agent_core::{generate_trace, TraceRow, TraceRunConfig};
use interface_ui::{UiEvent, UserInterface, VmBridge};

#[derive(Clone, Debug)]
pub struct TraceConfig {
    pub enabled: bool,
    pub output: Option<PathBuf>,
    pub depth: usize,
    pub beam: usize,
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
    let cfg = parse_trace_config(env::args().skip(1).collect());

    if cfg.enabled {
        let rows = generate_trace(TraceRunConfig {
            depth: cfg.depth,
            beam: cfg.beam,
            seed: 42,
        });

        let csv = render_csv(&rows);
        if let Some(path) = cfg.output {
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

fn parse_trace_config(args: Vec<String>) -> TraceConfig {
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
            _ => {
                i += 1;
            }
        }
    }

    TraceConfig {
        enabled,
        output,
        depth,
        beam,
    }
}

fn render_csv(rows: &[TraceRow]) -> String {
    let mut out = String::from(
        "depth,lambda,delta_lambda,tau_prime,conf_chm,density,k,h_profile,pareto_size,diversity,resonance_avg\n",
    );

    for row in rows {
        out.push_str(&format!(
            "{},{:.9},{:.9},{:.9},{:.9},{:.9},{},{:.9},{},{:.9},{:.9}\n",
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
        ));
    }

    out
}
