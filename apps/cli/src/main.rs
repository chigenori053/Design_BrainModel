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
    Runtime(CoreArgs),
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

    if let Commands::Runtime(args) = &cli.command {
        if let Err(err) = run_runtime_command(&args.args) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    if let Commands::Memory(args) = &cli.command
        && matches!(
            args.args.first().map(String::as_str),
            Some("rewrite" | "rollback" | "topology" | "drift" | "attractors")
        )
    {
        let forwarded = std::iter::once(OsString::from("design_cli"))
            .chain(args.args.iter().map(OsString::from));
        if let Err(err) = design_cli::memory_admin_main::run_with_args(forwarded) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

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
        Commands::Runtime(args) => join_core("runtime", &args.args),
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

fn run_runtime_command(args: &[String]) -> Result<(), String> {
    use design_cli::runtime::persistence::{
        cognitive_session_state, persistent_revision_lineage, persistent_runtime_memory,
        render_checkpoint, render_cognitive_session_state, render_evolution_events, render_lineage,
        render_persistent_runtime_memory, restore_runtime_checkpoint, runtime_memory_checkpoint,
        runtime_memory_evolution_events,
    };
    use design_cli::runtime::unified_apply::{
        execution_apply_transaction, unified_mutation_transaction, unified_runtime_apply,
        unified_runtime_rollback,
    };
    use design_cli::runtime::unified_projection::{
        Runtime, render_revision, render_unified_snapshot, unified_runtime_snapshot,
    };
    use memory_space_core::{SemanticIdentityGraph, semantic_rewrite_transaction};

    let mut runtime = Runtime::default();
    let snapshot = unified_runtime_snapshot(&runtime);
    match args.first().map(String::as_str) {
        Some("snapshot") => {
            println!("{}", render_unified_snapshot(&snapshot));
            Ok(())
        }
        Some("revisions") => {
            println!("{}", render_revision(&snapshot));
            Ok(())
        }
        Some("memory") => {
            let memory = persistent_runtime_memory(&runtime);
            let session = cognitive_session_state(&runtime);
            println!("{}", render_persistent_runtime_memory(&memory));
            println!();
            println!("{}", render_cognitive_session_state(&session));
            Ok(())
        }
        Some("checkpoint") => {
            let checkpoint = runtime_memory_checkpoint(&runtime);
            println!("{}", render_checkpoint(&checkpoint));
            Ok(())
        }
        Some("restore") => {
            if !args.iter().any(|arg| arg == "--yes") {
                return Err("runtime restore requires explicit --yes confirmation".to_string());
            }
            let checkpoint = runtime_memory_checkpoint(&runtime);
            let result = restore_runtime_checkpoint(checkpoint);
            println!(
                "runtime restore: restored={} revision={} replay_invariant={} topology_invariant={} revision_invariant={} errors={}",
                result.restored,
                result.runtime.runtime_revision,
                result.replay_invariant,
                result.topology_invariant,
                result.revision_invariant,
                result.errors.join(", ")
            );
            Ok(())
        }
        Some("lineage") => {
            let lineage = persistent_revision_lineage(&runtime);
            println!("{}", render_lineage(&lineage));
            Ok(())
        }
        Some("evolution") => {
            let memory = persistent_runtime_memory(&runtime);
            let events = runtime_memory_evolution_events(&memory);
            println!("{}", render_evolution_events(&events));
            Ok(())
        }
        Some("apply") => {
            let execution = execution_apply_transaction(&runtime, "runtime/unified-apply", 1);
            let semantic = semantic_rewrite_transaction(&SemanticIdentityGraph::default());
            let transaction =
                unified_mutation_transaction(&runtime, Some(execution), Some(semantic));
            if args.iter().any(|arg| arg == "--preview") {
                println!(
                    "unified preview: execution_diff={} topology_diff={} continuity_delta={:.6} semantic_mass_delta={:.6} revision_delta={}->{}",
                    transaction.unified_preview.execution_diff.is_some(),
                    transaction.unified_preview.topology_diff.is_some(),
                    transaction.unified_preview.continuity_delta,
                    transaction.unified_preview.semantic_mass_delta,
                    transaction
                        .unified_preview
                        .runtime_state_delta
                        .revision_before,
                    transaction
                        .unified_preview
                        .runtime_state_delta
                        .revision_after
                );
                Ok(())
            } else if args.iter().any(|arg| arg == "--validate") {
                println!(
                    "unified validation: execution_safe={} semantic_safe={} rollback_safe={} replay_invariant={} topology_invariant={} revision_consistent={} errors={}",
                    transaction.unified_validation.execution_safe,
                    transaction.unified_validation.semantic_safe,
                    transaction.unified_validation.rollback_safe,
                    transaction.unified_validation.replay_invariant,
                    transaction.unified_validation.topology_invariant,
                    transaction.unified_validation.revision_consistent,
                    transaction.unified_validation.validation_errors.join(", ")
                );
                Ok(())
            } else if args.iter().any(|arg| arg == "--yes") {
                let result = unified_runtime_apply(&mut runtime, transaction);
                println!(
                    "unified apply: applied={} rolled_back={} revision={} projection_synchronized={} checksum={} errors={}",
                    result.applied,
                    result.rolled_back,
                    result.runtime_revision,
                    result.projection_synchronized,
                    result.checksum,
                    result.errors.join(", ")
                );
                Ok(())
            } else {
                Err("runtime apply requires --preview, --validate, or --yes".to_string())
            }
        }
        Some("rollback") => {
            let transaction = unified_mutation_transaction(&runtime, None, None);
            let result = unified_runtime_rollback(&mut runtime, transaction.rollback_chain);
            println!(
                "unified rollback: rolled_back={} revision={} projection_synchronized={} replay_invariant={} topology_invariant={} errors={}",
                result.rolled_back,
                result.runtime_revision,
                result.projection_synchronized,
                result.replay_invariant,
                result.topology_invariant,
                result.errors.join(", ")
            );
            Ok(())
        }
        Some(other) => Err(format!(
            "unrecognized runtime command `{other}`. Available: snapshot, revisions, memory, checkpoint, restore, lineage, evolution, apply, rollback"
        )),
        None => Err("runtime command required. Available: snapshot, revisions, memory, checkpoint, restore, lineage, evolution, apply, rollback".to_string()),
    }
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
    fn test_runtime_command_parses_without_tui_activation() {
        let cli = Cli::try_parse_from(["dbm", "runtime", "snapshot"]).expect("parse runtime");
        assert_eq!(runtime_intent(&cli.command), None);
        assert_eq!(
            core_input(&cli.command).expect("core input"),
            "runtime snapshot"
        );
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
