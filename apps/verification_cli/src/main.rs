mod commands;
mod error;
mod io;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::CliResult;

#[derive(Debug, Parser)]
#[command(name = "verification_cli", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Trace {
        #[arg(long, default_value = "rest-api")]
        scenario: String,
        #[arg(long, default_value = "trace.json")]
        output: PathBuf,
    },
    Replay {
        trace: PathBuf,
        #[arg(long, default_value = "trace_replay.json")]
        output: PathBuf,
    },
    Diff {
        trace1: PathBuf,
        trace2: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Audit {
        #[arg(long, default_value = "rest-api")]
        scenario: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> CliResult<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Trace { scenario, output } => commands::trace::run(&scenario, &output),
        Command::Replay { trace, output } => commands::replay::run(&trace, &output),
        Command::Diff {
            trace1,
            trace2,
            output,
        } => commands::diff::run(&trace1, &trace2, output.as_deref()),
        Command::Audit { scenario, output } => commands::audit::run(&scenario, output.as_deref()),
    }
}
