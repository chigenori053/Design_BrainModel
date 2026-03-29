use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, error::ErrorKind};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "design_cli",
    version = VERSION,
    about = "Design Brain Model CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Analyze(AnalyzeArgs),
    Simulate(SimulateArgs),
    Phase1(PassThroughArgs),
    Memory(PassThroughArgs),
}

#[derive(Args, Debug)]
struct AnalyzeArgs {
    path: PathBuf,
    #[arg(long, default_value_t = false)]
    json: bool,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    report_md: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct SimulateArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

#[derive(Args, Debug)]
struct PassThroughArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

fn main() {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let args = std::env::args_os().collect::<Vec<_>>();
    if let Err(err) = dispatch(args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn dispatch(args: Vec<OsString>) -> Result<(), String> {
    if should_use_legacy_app(&args) {
        return design_cli::app::run_with_args(args);
    }
    if should_use_legacy_design(&args) {
        return design_cli::design_main::run_with_args(args);
    }

    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                print!("{err}");
                return Ok(());
            }
            _ => return Err(err.to_string()),
        },
    };
    match cli.command {
        Some(Commands::Analyze(args)) => design_cli::cli::commands::analyze::run(
            args.path,
            args.json,
            args.out,
            args.report_md,
        ),
        Some(Commands::Simulate(args)) => design_cli::cli::commands::simulate::run(args.args),
        Some(Commands::Phase1(args)) => design_cli::cli::commands::phase1::run(args.args),
        Some(Commands::Memory(args)) => design_cli::cli::commands::memory::run(args.args),
        None => {
            let mut cmd = Cli::command();
            cmd.print_long_help().map_err(|err| err.to_string())?;
            println!();
            Ok(())
        }
    }
}

fn should_use_legacy_app(args: &[OsString]) -> bool {
    let Some(first) = args.get(1).and_then(|arg| arg.to_str()) else {
        return false;
    };

    if first == "memory"
        && let Some(second) = args.get(2).and_then(|arg| arg.to_str())
    {
        return second == "import";
    }

    matches!(
        first,
        "generate"
            | "refactoring"
            | "design"
            | "validate"
            | "refactor"
            | "coding"
            | "diff"
            | "check"
            | "apply"
            | "exec"
            | "execute"
            | "run"
            | "wizard"
            | "repl"
            | "tui"
            | "rules"
    )
}

fn should_use_legacy_design(args: &[OsString]) -> bool {
    let Some(first) = args.get(1).and_then(|arg| arg.to_str()) else {
        return false;
    };

    if first.starts_with('/') {
        return true;
    }

    if matches!(first, "clear" | "adopt" | "reject" | "export" | "explain" | "phase9") {
        return true;
    }

    if first == "analyze" {
        let has_design_flag = args.iter().skip(2).filter_map(|arg| arg.to_str()).any(|arg| {
            matches!(
                arg,
                "--beam-width"
                    | "--max-steps"
                    | "--hv-guided"
                    | "--human-coherence"
                    | "--dump-analysis"
                    | "--target"
            )
        });
        let second_is_flag = args
            .get(2)
            .and_then(|arg| arg.to_str())
            .is_none_or(|arg| arg.starts_with('-'));
        return has_design_flag || second_is_flag;
    }

    !matches!(
        first,
        "-h" | "--help" | "-V" | "--version" | "analyze" | "simulate" | "phase1" | "memory"
    ) && !should_use_legacy_app(args)
}
