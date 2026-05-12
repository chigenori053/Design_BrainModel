use std::ffi::OsString;

use clap::{Args, CommandFactory, Parser, Subcommand};
use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};
use design_cli::runtime::bootstrap::start_runtime_tui;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "dbm",
    version = VERSION,
    about = "Explainable Governed Cognitive Runtime Workspace",
    disable_help_subcommand = true,
    arg_required_else_help = true,
)]
struct Cli {
    #[arg(long, global = true)]
    diagnostic_input: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Repl,
    Workspace,
    #[command(name = "legacy-repl")]
    LegacyRepl,
    Analyze(CoreArgs),
    Coding(CoreArgs),
    Validate(CoreArgs),
    Structure(CoreArgs),
    Replay(CoreArgs),
    Run(CoreArgs),
    #[command(name = "run-dsl")]
    RunDsl(CoreArgs),
    Rules(CoreArgs),
    Memory(CoreArgs),
    Simulate(CoreArgs),
    #[command(name = "phase-analyze")]
    PhaseAnalyze(CoreArgs),
    Phase1(CoreArgs),
    Clear(CoreArgs),
    Adopt(CoreArgs),
    Reject(CoreArgs),
    Export(CoreArgs),
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[derive(Args, Debug)]
struct CoreArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeIntent {
    Repl,
    Workspace,
    LegacyRepl,
}

fn main() {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let cli = Cli::parse();

    match runtime_intent(&cli.command) {
        Some(RuntimeIntent::Repl | RuntimeIntent::Workspace | RuntimeIntent::LegacyRepl) => {
            if let Err(err) = start_runtime_tui(cli.diagnostic_input) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        None => match core_input(&cli.command) {
            Ok(input) => run_core_command(input),
            Err(err) => err.exit(),
        },
    }
}

fn runtime_intent(command: &Commands) -> Option<RuntimeIntent> {
    match command {
        Commands::Repl => Some(RuntimeIntent::Repl),
        Commands::Workspace => Some(RuntimeIntent::Workspace),
        Commands::LegacyRepl => Some(RuntimeIntent::LegacyRepl),
        _ => None,
    }
}

fn run_core_command(input: String) {
    let core = RuntimeCoreBridge::with_defaults();
    let request = CoreRequest::new(input);
    let response = core.execute(request);

    for event in response.events {
        match event {
            design_cli::core::CoreEvent::Result { message } => println!("{}", message),
            design_cli::core::CoreEvent::Error { message } => eprintln!("[ERROR] {}", message),
            _ => {}
        }
    }

    if response.status == design_cli::core::ExecutionStatus::Failed {
        std::process::exit(1);
    }
}

fn core_input(command: &Commands) -> Result<String, clap::Error> {
    let input = match command {
        Commands::Analyze(args) => join_core("analyze", &args.args),
        Commands::Coding(args) => join_core("coding", &args.args),
        Commands::Validate(args) => join_core("validate", &args.args),
        Commands::Structure(args) => join_core("structure", &args.args),
        Commands::Replay(args) => join_core("replay", &args.args),
        Commands::Run(args) => join_core("run", &args.args),
        Commands::RunDsl(args) => join_core("run-dsl", &args.args),
        Commands::Rules(args) => join_core("rules", &args.args),
        Commands::Memory(args) => join_core("memory", &args.args),
        Commands::Simulate(args) => join_core("simulate", &args.args),
        Commands::PhaseAnalyze(args) => join_core("phase-analyze", &args.args),
        Commands::Phase1(args) => join_core("phase1", &args.args),
        Commands::Clear(args) => join_core("clear", &args.args),
        Commands::Adopt(args) => join_core("adopt", &args.args),
        Commands::Reject(args) => join_core("reject", &args.args),
        Commands::Export(args) => join_core("export", &args.args),
        Commands::External(args) => external_core_input(args)?,
        Commands::Repl | Commands::Workspace | Commands::LegacyRepl => String::new(),
    };
    Ok(input)
}

fn join_core(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn external_core_input(args: &[OsString]) -> Result<String, clap::Error> {
    let input = args
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");

    if is_freeform_core_input(&input) {
        Ok(input)
    } else {
        let command = input
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        Err(Cli::command().error(
            clap::error::ErrorKind::InvalidSubcommand,
            format!("unrecognized subcommand '{command}'"),
        ))
    }
}

fn is_freeform_core_input(input: &str) -> bool {
    input.starts_with('/') || input.chars().any(|ch| !ch.is_ascii())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_command_parses() {
        let cli = Cli::try_parse_from(["dbm", "repl"]).expect("parse repl");
        assert_eq!(runtime_intent(&cli.command), Some(RuntimeIntent::Repl));
    }

    #[test]
    fn test_workspace_command_parses() {
        let cli = Cli::try_parse_from(["dbm", "workspace"]).expect("parse workspace");
        assert_eq!(runtime_intent(&cli.command), Some(RuntimeIntent::Workspace));
    }

    #[test]
    fn test_legacy_repl_parses() {
        let cli = Cli::try_parse_from(["dbm", "legacy-repl"]).expect("parse legacy repl");
        assert_eq!(
            runtime_intent(&cli.command),
            Some(RuntimeIntent::LegacyRepl)
        );
    }

    #[test]
    fn test_diagnostic_flag_propagates_after_subcommand() {
        let cli =
            Cli::try_parse_from(["dbm", "repl", "--diagnostic-input"]).expect("parse diagnostic");
        assert!(cli.diagnostic_input);
        assert_eq!(runtime_intent(&cli.command), Some(RuntimeIntent::Repl));
    }

    #[test]
    fn test_invalid_flag_rejected_for_runtime_command() {
        let err = Cli::try_parse_from(["dbm", "repl", "--unknown-runtime-flag"])
            .expect_err("invalid runtime flag should be rejected");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_no_hidden_runtime_activation_for_core_command() {
        let cli = Cli::try_parse_from(["dbm", "analyze", "."]).expect("parse analyze");
        assert_eq!(runtime_intent(&cli.command), None);
        assert_eq!(core_input(&cli.command).expect("core input"), "analyze .");
    }

    #[test]
    fn test_unknown_command_rejected() {
        let cli = Cli::try_parse_from(["dbm", "invalid-cmd"]).expect("external parse");
        let err = core_input(&cli.command).expect_err("unknown command should reject");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn test_freeform_input_remains_core_routed() {
        let cli =
            Cli::try_parse_from(["dbm", "このプロジェクトを解析して"]).expect("external parse");
        assert_eq!(
            core_input(&cli.command).expect("freeform input"),
            "このプロジェクトを解析して"
        );
    }

    #[test]
    fn test_no_string_equality_routing() {
        let source = include_str!("main.rs");
        assert!(!source.contains(&format!("raw_input {}", "==")));
        assert!(!source.contains(&format!("match {}.as_str()", "raw_input")));
        assert!(!source.contains(&format!("{}.contains(\"--diagnostic-input\")", "raw_input")));
        assert!(!source.contains(&format!("replace(\"{}\"", "--diagnostic-input")));
    }
}
// DBM clarification execution guarantee
