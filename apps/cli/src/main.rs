use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, error::ErrorKind};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ONBOARDING_HELP: &str = "\
AI-native architecture analysis, safe refactoring, and structure visualization CLI

Usage: design_cli [COMMAND]

Core:
  analyze        Analyze project architecture and generate reports
  coding         Generate or safely apply code changes
  validate       Validate design and runtime constraints
  structure      Open structure viewer and edit sessions
  replay         Replay persisted IR sessions and export timelines

Workflow:
  repl           Interactive natural language and command workflow
  run            Execute controlled project workflows
  rules          Inspect, validate, and promote learned rules
  memory         Import and verify memory seeds

Advanced:
  simulate       Run runtime simulation
  phase-analyze  Internal phased analyzer
  phase1         Legacy phase1 execution
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Examples:
  design_cli analyze .
  design_cli coding . --check
  design_cli structure view .
  design_cli repl
";

#[derive(Parser, Debug)]
#[command(
    name = "design_cli",
    version = VERSION,
    about = "AI-native architecture analysis, safe refactoring, and structure visualization CLI",
    after_help = "Examples:\n  design_cli analyze .\n  design_cli coding . --check\n  design_cli structure view .\n  design_cli repl"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Analyze project architecture and generate reports")]
    Analyze(AnalyzeArgs),
    #[command(about = "Generate or safely apply code changes")]
    Coding(PassThroughArgs),
    #[command(about = "Validate design and runtime constraints")]
    Validate(PassThroughArgs),
    #[command(about = "Open structure viewer and edit sessions")]
    Structure(PassThroughArgs),
    #[command(about = "Replay persisted IR sessions and export timelines")]
    Replay(PassThroughArgs),
    #[command(about = "Interactive natural language and command workflow")]
    Repl(PassThroughArgs),
    #[command(about = "Execute controlled project workflows")]
    Run(PassThroughArgs),
    #[command(about = "Inspect, validate, and promote learned rules")]
    Rules(PassThroughArgs),
    #[command(about = "Internal phased analyzer")]
    PhaseAnalyze(PassThroughArgs),
    #[command(about = "Run runtime simulation")]
    Simulate(SimulateArgs),
    #[command(about = "Legacy phase1 execution")]
    Phase1(PassThroughArgs),
    #[command(about = "Import and verify memory seeds")]
    Memory(PassThroughArgs),
}

#[derive(Args, Debug)]
struct AnalyzeArgs {
    path: PathBuf,
    #[arg(long, default_value_t = false)]
    detailed: bool,
    #[arg(long, default_value_t = false)]
    report: bool,
    #[arg(long, default_value_t = false)]
    design: bool,
    #[arg(long, default_value = "ja")]
    lang: String,
    #[arg(long)]
    intent: Option<String>,
    #[arg(long, default_value_t = false)]
    json: bool,
    #[arg(long, default_value_t = false)]
    design_json: bool,
    #[arg(long, hide = true)]
    out: Option<PathBuf>,
    #[arg(long, hide = true)]
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
    let legacy_design = should_use_legacy_design(&args);
    if let Err(err) = dispatch(args) {
        let code = if err.contains("対象が指定されていません")
            || err.contains("入力を理解できませんでした")
        {
            2
        } else {
            1
        };
        if legacy_design {
            if err.trim_start().starts_with('{') {
                eprintln!("{err}");
            } else {
                eprintln!(
                    "{{\"error\":{{\"code\":\"PHASE1_ERROR\",\"details\":null,\"message\":{}}}}}",
                    serde_json::to_string(&err)
                        .unwrap_or_else(|_| "\"internal error\"".to_string())
                );
            }
        } else {
            eprintln!("{err}");
        }
        std::process::exit(code);
    }
}

fn dispatch(args: Vec<OsString>) -> Result<(), String> {
    if should_print_onboarding_help(&args) {
        print!("{ONBOARDING_HELP}");
        return Ok(());
    }

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
            args.detailed,
            args.report,
            args.design,
            args.lang,
            args.intent,
            args.json,
            args.design_json,
            args.out,
            args.report_md,
        ),
        Some(Commands::Coding(args)) => pass_through_app_command("coding", args),
        Some(Commands::Validate(args)) => pass_through_app_command("validate", args),
        Some(Commands::Structure(args)) => pass_through_app_command("structure", args),
        Some(Commands::Replay(args)) => pass_through_app_command("replay", args),
        Some(Commands::Repl(args)) => pass_through_app_command("repl", args),
        Some(Commands::Run(args)) => pass_through_app_command("run", args),
        Some(Commands::Rules(args)) => pass_through_app_command("rules", args),
        Some(Commands::PhaseAnalyze(args)) => design_cli::design_main::run_with_args(
            std::iter::once(OsString::from("design_cli"))
                .chain(std::iter::once(OsString::from("phase-analyze")))
                .chain(args.args)
                .collect::<Vec<_>>(),
        ),
        Some(Commands::Simulate(args)) => design_cli::cli::commands::simulate::run(args.args),
        Some(Commands::Phase1(args)) => design_cli::cli::commands::phase1::run(args.args),
        Some(Commands::Memory(args)) => design_cli::cli::commands::memory::run(args.args),
        None => {
            print!("{ONBOARDING_HELP}");
            Ok(())
        }
    }
}

fn pass_through_app_command(command: &str, args: PassThroughArgs) -> Result<(), String> {
    design_cli::app::run_with_args(
        std::iter::once(OsString::from("design_cli"))
            .chain(std::iter::once(OsString::from(command)))
            .chain(args.args)
            .collect::<Vec<_>>(),
    )
}

fn should_print_onboarding_help(args: &[OsString]) -> bool {
    match args {
        [_] => true,
        [_, arg] => matches!(arg.to_str(), Some("-h" | "--help" | "help")),
        _ => false,
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
            | "structure"
            | "replay"
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

    if matches!(
        first,
        "clear" | "adopt" | "reject" | "export" | "explain" | "phase9"
    ) {
        return true;
    }

    if first == "phase-analyze" {
        return true;
    }

    if first == "analyze" {
        return args
            .iter()
            .skip(2)
            .filter_map(|arg| arg.to_str())
            .any(|arg| {
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
    }

    !matches!(
        first,
        "-h" | "--help"
            | "-V"
            | "--version"
            | "analyze"
            | "phase-analyze"
            | "simulate"
            | "phase1"
            | "memory"
    ) && !should_use_legacy_app(args)
}
