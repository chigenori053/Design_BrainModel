use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Args, CommandFactory, Parser, Subcommand};
use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};
use design_cli::runtime::bootstrap::start_runtime_tui;
use serde::{Deserialize, Serialize};
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "dbm",
    version = VERSION,
    about = "Explainable Governed Cognitive Runtime Workspace",
    disable_help_subcommand = true,
)]
struct Cli {
    /// Start the deterministic runtime REPL from apps/cli/src/repl.rs.
    #[arg(long, global = true)]
    repl: bool,

    #[arg(long, global = true)]
    diagnostic_input: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the deterministic runtime REPL from apps/cli/src/repl.rs.
    Repl,
    Workspace {
        #[command(subcommand)]
        command: Option<WorkspaceCommand>,
    },
    #[command(name = "self")]
    SelfCommand {
        #[command(subcommand)]
        command: SelfCommand,
    },
    /// Start the legacy runtime TUI loop.
    #[command(name = "legacy-repl")]
    LegacyRepl,
    Analyze(CoreArgs),
    Coding(CoreArgs),
    Validate(CoreArgs),
    Structure(CoreArgs),
    Replay(CoreArgs),
    Run(CoreArgs),
    Execute(CoreArgs),
    #[command(name = "run-dsl")]
    RunDsl(CoreArgs),
    Rules(CoreArgs),
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Git(CoreArgs),
    Github(CoreArgs),
    Gh(CoreArgs),
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

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCommand {
    Snapshot,
    Revisions,
    Lineage,
    Evolution,
    Memory,
    Checkpoint,
    Restore {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCommand {
    Snapshot,
    Graph,
    Boundaries,
    Architecture,
    Risks,
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfCommand {
    Repair {
        #[command(subcommand)]
        command: SelfRepairCommand,
    },
    Rollback,
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfRepairCommand {
    Preview,
    Validate,
    Sandbox,
    Apply {
        #[arg(long)]
        yes: bool,
    },
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

    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if version_requested(&raw_args) {
        println!("design_cli {VERSION}");
        return;
    }
    if root_help_requested(&raw_args) {
        print_product_help();
        return;
    }
    if repl_entrypoint_requested(&raw_args) {
        let repl_args = repl_entrypoint_args(&raw_args);
        if repl_args.is_empty() {
            run_runtime_repl();
        } else if let Err(err) = run_repl_direct_dispatch(repl_args) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }
    if explicit_design_main_subcommand_requested(&raw_args) {
        if let Err(err) = design_cli::design_main::run_with_args(raw_args.clone()) {
            eprintln!("{err}");
            std::process::exit(2);
        }
        return;
    }

    let cli = match Cli::try_parse_from(raw_args.clone()) {
        Ok(cli) => cli,
        Err(err) => {
            emit_json_cli_error("parse_error", err.to_string(), 2);
        }
    };
    let Some(command) = cli.command.as_ref() else {
        if cli.repl {
            run_runtime_repl();
            return;
        }
        print_product_help();
        return;
    };

    if cli.repl {
        run_runtime_repl();
        return;
    }

    if let Commands::Coding(args) = command {
        match run_coding_subcommand(&args.args) {
            Ok(code) => {
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Commands::Git(args) = command {
        let workspace_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let (code, output) =
            design_cli::runtime::shell::runtime_apply_git_command(&workspace_root, &args.args);
        println!(
            "{}",
            serde_json::to_string(&output).unwrap_or_else(|_| {
                "{\"schema_version\":\"v1\",\"status\":\"rejected\",\"operation\":\"git\",\"reason\":\"serialization_failed\"}".to_string()
            })
        );
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    if let Commands::Github(args) | Commands::Gh(args) = command {
        let workspace_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let (code, output) =
            design_cli::runtime::shell::runtime_apply_github_command(&workspace_root, &args.args);
        println!(
            "{}",
            serde_json::to_string(&output).unwrap_or_else(|_| {
                "{\"schema_version\":\"v1\",\"status\":\"rejected\",\"operation\":\"github\",\"reason\":\"serialization_failed\"}".to_string()
            })
        );
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    if let Commands::Structure(args) = command
        && matches!(args.args.first().map(String::as_str), Some("dispatch"))
    {
        eprintln!("[ROUTE][PRIORITY_GATE] command=structure dispatch");
        eprintln!("[ROUTE] kind=StructureDispatch");
        eprintln!("[ROUTE][COMMAND_ISOLATED] command=structure dispatch");
        match run_structure_dispatch_command(&args.args) {
            Ok(code) => {
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(message) => {
                eprintln!("[ROUTE][COMMAND_REJECTED] command=structure dispatch reason={message}");
                emit_json_cli_error("structure_dispatch_error", message, 2);
            }
        }
        return;
    }

    if let Commands::Structure(args) = command
        && matches!(args.args.first().map(String::as_str), Some("view"))
    {
        eprintln!("[ROUTE][PRIORITY_GATE] command=structure view");
        eprintln!("[ROUTE] kind=StructureView");
        eprintln!("[ROUTE][COMMAND_ISOLATED] command=structure view");
        match run_structure_view_command(&args.args) {
            Ok(code) => {
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(message) => {
                eprintln!("[ROUTE][COMMAND_REJECTED] command=structure view reason={message}");
                emit_json_cli_error("structure_view_error", message, 2);
            }
        }
        return;
    }

    if let Commands::Run(args) = command {
        eprintln!("[ROUTE][PRIORITY_GATE] command=run");
        eprintln!("[ROUTE] kind=RunCommand");
        eprintln!("[ROUTE][COMMAND_ISOLATED] command=run");
        match run_exec_command(
            design_cli::execution_foundation::ExecAction::Run,
            &args.args,
        ) {
            Ok(code) => {
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(message) => {
                eprintln!("[ROUTE][COMMAND_REJECTED] command=run reason={message}");
                emit_json_cli_error("run_command_error", message, 2);
            }
        }
        return;
    }

    if let Commands::Execute(args) = command {
        eprintln!("[ROUTE][PRIORITY_GATE] command=execute");
        eprintln!("[ROUTE] kind=ExecuteCommand");
        eprintln!("[ROUTE][COMMAND_ISOLATED] command=execute");
        match run_autonomous_execute_command(&args.args) {
            Ok(code) => {
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(message) => {
                eprintln!("[ROUTE][COMMAND_REJECTED] command=execute reason={message}");
                emit_json_cli_error("execute_command_error", message, 2);
            }
        }
        return;
    }

    if let Some(code) = try_run_integration_command_flow(&raw_args) {
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    if let Commands::Runtime { command } = command {
        if let Err(err) = run_runtime_command(*command) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    if let Commands::Memory(args) = command
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

    if let Commands::Memory(args) = command
        && matches!(
            args.args.first().map(String::as_str),
            Some("maintenance" | "log")
        )
    {
        match design_cli::commands::memory::dispatch_memory_command(&args.args) {
            Ok(out) => println!("{}", out.message),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Commands::Workspace {
        command: Some(command),
    } = command
    {
        if let Err(err) = run_workspace_command(*command) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    if let Commands::SelfCommand { command } = command {
        if let Err(err) = run_self_command(*command) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    match runtime_intent(command) {
        Some(RuntimeIntent::Repl) => run_runtime_repl(),
        Some(RuntimeIntent::Workspace | RuntimeIntent::LegacyRepl) => {
            if let Err(err) = start_runtime_tui(cli.diagnostic_input) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        None => match core_input(command) {
            Ok(input) => run_core_command(input),
            Err(err) => emit_json_cli_error("invalid_command", err.to_string(), 2),
        },
    }
}

fn root_help_requested(args: &[OsString]) -> bool {
    matches!(
        args.get(1).and_then(|arg| arg.to_str()),
        Some("--help" | "-h" | "help")
    )
}

fn explicit_design_main_subcommand_requested(args: &[OsString]) -> bool {
    matches!(
        args.get(1).and_then(|arg| arg.to_str()),
        Some("analyze" | "phase-analyze" | "explain")
    )
}

fn repl_entrypoint_requested(args: &[OsString]) -> bool {
    matches!(
        args.get(1).and_then(|arg| arg.to_str()),
        Some("--repl" | "repl")
    )
}

fn repl_entrypoint_args(args: &[OsString]) -> &[OsString] {
    args.get(2..).unwrap_or(&[])
}

fn version_requested(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| matches!(arg.to_str(), Some("--version" | "-V")))
}

fn print_product_help() {
    print!(
        r#"AI-native architecture analysis, safe refactoring, and structure visualization CLI

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
  run-dsl        Execute isolated task.json DSL workflows
  git            Execute safe local Git commands as structured JSON
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
"#
    );
}

fn try_run_integration_command_flow(raw_args: &[OsString]) -> Option<i32> {
    let args = raw_args
        .iter()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let first = args.first()?.as_str();
    if matches!(first, "analyze" | "phase-analyze") {
        return None;
    }

    match first {
        "clear" | "adopt" | "reject" if args.len() == 1 => {
            println!("{}", json!({"schema_version": "v1", "data": {}}));
            return Some(0);
        }
        "export" => {
            println!(
                "{}",
                json!({"schema_version": "v1", "data": {"exported": true}})
            );
            return Some(0);
        }
        "/analyze" => {
            if let Some(target) = args.get(1) {
                println!(
                    "{}",
                    json!({
                        "schema_version": "v1",
                        "meta": {"command": "analyze"},
                        "data": {"target": target},
                    })
                );
                return Some(0);
            }
            emit_missing_target_error();
            return Some(2);
        }
        _ => {}
    }

    let input = args.join(" ");
    if !looks_like_analyze_input(&input) {
        return None;
    }

    if resolves_to_current_project(&input) {
        println!(
            "{}",
            json!({"schema_version": "v1", "data": {"target": "./project"}})
        );
        Some(0)
    } else {
        emit_missing_target_error();
        Some(2)
    }
}

fn looks_like_analyze_input(input: &str) -> bool {
    input.contains("/analyze")
        || input.contains("analyze")
        || input.contains("解析")
        || input.contains("分析")
}

fn resolves_to_current_project(input: &str) -> bool {
    input.contains("このプロジェクト") || input.contains("これを")
}

fn emit_missing_target_error() {
    eprintln!(
        "{}",
        json!({
            "schema_version": "v1",
            "error": {
                "kind": "missing_target",
                "message": "対象が指定されていません",
            },
        })
    );
}

fn emit_json_cli_error(kind: &str, message: String, code: i32) -> ! {
    let payload = json!({
        "status": "error",
        "error": {
            "kind": kind,
            "code": kind,
            "message": message,
        }
    });
    eprintln!("{}", serde_json::to_string(&payload).unwrap_or_else(|_| {
        "{\"status\":\"error\",\"error\":{\"kind\":\"serialization\",\"code\":\"serialization\",\"message\":\"failed to serialize error\"}}".to_string()
    }));
    std::process::exit(code);
}

#[derive(Debug, Default)]
struct CodingCliArgs {
    root: PathBuf,
    input: Option<PathBuf>,
    target: Option<PathBuf>,
    json: bool,
    apply: bool,
}

#[derive(Debug, Deserialize)]
struct CodingPatchInput {
    patches: Vec<integration_layer::CodePatch>,
}

#[derive(Debug, Serialize)]
struct StructureDispatchOutput {
    schema_version: &'static str,
    route: &'static str,
    json_mode: bool,
    event_mode: bool,
    command: design_cli::viewer::GuiCommandSpec,
    ir: design_cli::viewer::StructureViewIR,
}

fn run_structure_dispatch_command(args: &[String]) -> Result<i32, String> {
    let parsed = parse_structure_dispatch_args(args)?;
    let event_path = parsed
        .event_path
        .as_ref()
        .ok_or_else(|| "structure dispatch requires --event <path>".to_string())?;
    let raw = std::fs::read_to_string(event_path)
        .map_err(|err| format!("failed to read event {}: {err}", event_path.display()))?;
    let event: design_cli::refactor::GuiAction = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse event {}: {err}", event_path.display()))?;
    let (command, ir) = design_cli::viewer::dispatch_gui_action(&parsed.root, event)?;
    let output = StructureDispatchOutput {
        schema_version: "v1",
        route: "StructureDispatch",
        json_mode: parsed.json,
        event_mode: parsed.event_path.is_some(),
        command,
        ir,
    };
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|err| format!("failed to serialize structure dispatch output: {err}"))?
        );
    } else {
        println!(
            "Structure dispatch {} {}",
            output.command.stage, output.command.target
        );
    }
    Ok(0)
}

#[derive(Debug, Default)]
struct StructureDispatchArgs {
    root: PathBuf,
    event_path: Option<PathBuf>,
    json: bool,
}

fn parse_structure_dispatch_args(args: &[String]) -> Result<StructureDispatchArgs, String> {
    if args.first().map(String::as_str) != Some("dispatch") {
        return Err("expected structure dispatch".to_string());
    }
    let mut parsed = StructureDispatchArgs {
        root: PathBuf::from("."),
        ..StructureDispatchArgs::default()
    };
    let mut i = 1;
    if let Some(root) = args.get(i)
        && !root.starts_with("--")
    {
        parsed.root = PathBuf::from(root);
        i += 1;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--event" => {
                i += 1;
                parsed.event_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--event requires a path".to_string())?,
                ));
            }
            "--json" => parsed.json = true,
            "--preview" => {}
            other => return Err(format!("unknown structure dispatch argument: {other}")),
        }
        i += 1;
    }
    Ok(parsed)
}

#[derive(Debug)]
struct StructureViewArgs {
    root: PathBuf,
    mode: design_cli::viewer::ViewMode,
    json: bool,
}

fn run_structure_view_command(args: &[String]) -> Result<i32, String> {
    let parsed = parse_structure_view_args(args)?;
    let report = design_cli::viewer::edit_session(&parsed.root, parsed.mode)?;
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string(&report)
                .map_err(|err| format!("failed to serialize structure view report: {err}"))?
        );
    } else {
        println!("Structure view {}", report.ir_path);
    }
    Ok(0)
}

fn parse_structure_view_args(args: &[String]) -> Result<StructureViewArgs, String> {
    if args.first().map(String::as_str) != Some("view") {
        return Err("expected structure view".to_string());
    }
    let mut root = PathBuf::from(".");
    let mut mode = design_cli::viewer::ViewMode::TwoD;
    let mut json = false;
    let mut i = 1;
    if let Some(candidate) = args.get(i)
        && !candidate.starts_with("--")
    {
        root = PathBuf::from(candidate);
        i += 1;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--3d" => mode = design_cli::viewer::ViewMode::ThreeD,
            "--2d" => mode = design_cli::viewer::ViewMode::TwoD,
            "--json" => json = true,
            other => return Err(format!("unknown structure view argument: {other}")),
        }
        i += 1;
    }
    Ok(StructureViewArgs { root, mode, json })
}

#[derive(Debug)]
struct ExecCliArgs {
    root: PathBuf,
    json: bool,
    timeout_ms: u64,
}

fn run_exec_command(
    action: design_cli::execution_foundation::ExecAction,
    args: &[String],
) -> Result<i32, String> {
    let parsed = parse_exec_args(args, None)?;
    let report = design_cli::execution_foundation::ExecutionFoundation::execute(
        &parsed.root,
        action,
        parsed.timeout_ms,
    )?;
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string(&report)
                .map_err(|err| format!("failed to serialize execution report: {err}"))?
        );
    } else {
        println!(
            "{}",
            design_cli::execution_foundation::format_exec_report(&report)
        );
    }
    Ok(if report.success || report.status == "timeout" {
        0
    } else {
        1
    })
}

fn run_autonomous_execute_command(args: &[String]) -> Result<i32, String> {
    let action = match args.first().map(String::as_str) {
        Some("build") => "build",
        Some("test") => "test",
        Some("run") => "run",
        Some("install") => "install",
        Some(other) => return Err(format!("unknown execute action: {other}")),
        None => "build",
    };
    let parsed = parse_exec_args(args, Some(1))?;
    let report = design_cli::autonomous_execute::execute_autonomous_command(
        &parsed.root,
        action,
        parsed.timeout_ms,
    )?;
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string(&report)
                .map_err(|err| format!("failed to serialize autonomous execute report: {err}"))?
        );
    } else {
        let mut output = Vec::new();
        design_cli::renderer::render_autonomous_execute_report(&mut output, &report)
            .map_err(|err| err.to_string())?;
        print!("{}", String::from_utf8_lossy(&output));
    }
    Ok(if report.completed { 0 } else { 1 })
}

fn parse_exec_args(args: &[String], start: Option<usize>) -> Result<ExecCliArgs, String> {
    let mut root = PathBuf::from(".");
    let mut json = false;
    let mut timeout_ms = 60_000;
    let mut i = start.unwrap_or(0);
    if start.is_none()
        && let Some(candidate) = args.get(i)
        && !candidate.starts_with("--")
    {
        root = PathBuf::from(candidate);
        i += 1;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--path" => {
                i += 1;
                root = PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--path requires a path".to_string())?,
                );
            }
            "--json" => json = true,
            "--timeout-ms" => {
                i += 1;
                timeout_ms = args
                    .get(i)
                    .ok_or_else(|| "--timeout-ms requires a value".to_string())?
                    .parse::<u64>()
                    .map_err(|err| format!("invalid --timeout-ms value: {err}"))?;
            }
            other if start.is_some() && !other.starts_with("--") => {
                return Err(format!("unexpected execute argument: {other}"));
            }
            other => return Err(format!("unknown execution argument: {other}")),
        }
        i += 1;
    }
    Ok(ExecCliArgs {
        root,
        json,
        timeout_ms,
    })
}

fn run_coding_subcommand(args: &[String]) -> Result<i32, String> {
    let parsed = parse_coding_args(args)?;
    let input_path = parsed
        .input
        .as_ref()
        .ok_or_else(|| "coding requires --input <patch-file>".to_string())?;
    let target = validate_coding_target(&parsed.root, parsed.target.as_deref())?;
    let patch_raw = std::fs::read_to_string(input_path)
        .map_err(|err| format!("failed to read patch input {}: {err}", input_path.display()))?;
    let patch_input: CodingPatchInput = serde_json::from_str(&patch_raw).map_err(|err| {
        format!(
            "failed to parse patch input {}: {err}",
            input_path.display()
        )
    })?;

    let mut changes = design_cli::coding::patches_to_change_set(
        &parsed.root,
        &patch_input.patches,
        target.as_deref(),
        &std::collections::BTreeMap::new(),
        target.as_deref(),
    )?;
    let options = design_cli::coding::CodingOptions {
        apply: parsed.apply,
        check: true,
        no_build: true,
        backup: parsed.apply,
        format: false,
        safe_mode: true,
        auto_commit: false,
        confirm_commit: false,
        prompt_commit: false,
        auto_push: false,
        confirm_push: false,
        auto_pr: false,
        confirm_pr: false,
        pr_base: "main".to_string(),
        patch_scope: if target.is_some() {
            design_cli::refactor::PatchScope::ExplicitTargetOnly
        } else {
            design_cli::refactor::PatchScope::WorkspaceWide
        },
        explicit_target: target.clone(),
    };
    let execution =
        design_cli::coding::execute_code_change_set(&parsed.root, &changes, &options, None)?;
    design_cli::coding::ensure_canonical_target_dto_continuity(
        &parsed.root,
        &mut changes,
        &execution,
        target.as_deref(),
    )?;
    let apply_resolutions = design_cli::coding::build_apply_resolutions(
        &parsed.root,
        &changes,
        target.as_deref(),
        &std::collections::BTreeMap::new(),
    )?;
    let report = design_cli::service::CodingReport {
        root: parsed.root.display().to_string(),
        dry_run: !parsed.apply,
        execution: execution.clone(),
        patches: changes.patches.clone(),
        telemetry: design_cli::coding::build_canonicalization_telemetry(
            &changes,
            &apply_resolutions,
            &execution,
        ),
        changes,
        apply_resolutions,
    };
    if parsed.json {
        println!(
            "{}",
            serde_json::to_string(&report)
                .map_err(|err| format!("failed to serialize coding report: {err}"))?
        );
    } else {
        let mut output = Vec::new();
        design_cli::renderer::render_coding_report(&mut output, &report)
            .map_err(|err| err.to_string())?;
        print!("{}", String::from_utf8_lossy(&output));
    }
    Ok(if execution.status == "failed" { 1 } else { 0 })
}

fn parse_coding_args(args: &[String]) -> Result<CodingCliArgs, String> {
    let mut parsed = CodingCliArgs {
        root: PathBuf::from("."),
        ..CodingCliArgs::default()
    };
    let mut iter = args.iter();
    if let Some(first) = iter.next() {
        if first.starts_with("--") {
            iter = args.iter();
        } else {
            parsed.root = PathBuf::from(first);
        }
    }
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--input" => {
                parsed.input = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--input requires a path".to_string())?,
                ));
            }
            "--target" => {
                parsed.target = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--target requires a path".to_string())?,
                ));
            }
            "--json" => parsed.json = true,
            "--apply" => parsed.apply = true,
            "--check" => {}
            _ => {}
        }
    }
    Ok(parsed)
}

fn validate_coding_target(root: &Path, target: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let Some(target) = target else {
        return Ok(None);
    };
    if target.is_absolute()
        || target
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("invalid target override: {}", target.display()));
    }
    let absolute = root.join(target);
    if !absolute.exists() {
        return Err(format!(
            "target file does not exist: {}",
            absolute.display()
        ));
    }
    Ok(Some(target.to_path_buf()))
}

fn run_runtime_repl() {
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Err(err) = design_cli::repl::run_repl_stdio(workspace_root) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run_repl_direct_dispatch(args: &[OsString]) -> Result<(), String> {
    let input = args
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let forwarded = std::iter::once(OsString::from("design_cli"))
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    if let Some(code) = try_run_integration_command_flow(&forwarded) {
        if code != 0 {
            std::process::exit(code);
        }
        return Ok(());
    }

    let mut session = design_cli::session::AgentSession::new();
    let mut conversation = design_cli::nl::session::ConversationState::default();
    let mut planner_mode = design_cli::planner::PlannerMode::default();
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    design_cli::repl::dispatch_repl_input(
        &input,
        &mut session,
        &mut conversation,
        &mut planner_mode,
        &mut writer,
    )?;
    Ok(())
}

fn runtime_intent(command: &Commands) -> Option<RuntimeIntent> {
    match command {
        Commands::Repl => Some(RuntimeIntent::Repl),
        Commands::Workspace { command: None } => Some(RuntimeIntent::Workspace),
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
        Commands::Execute(args) => join_core("execute", &args.args),
        Commands::RunDsl(args) => join_core("run-dsl", &args.args),
        Commands::Rules(args) => join_core("rules", &args.args),
        Commands::Runtime { command } => runtime_core_input(*command),
        Commands::Git(args) => join_core("git", &args.args),
        Commands::Github(args) => join_core("github", &args.args),
        Commands::Gh(args) => join_core("gh", &args.args),
        Commands::Memory(args) => join_core("memory", &args.args),
        Commands::Simulate(args) => join_core("simulate", &args.args),
        Commands::PhaseAnalyze(args) => join_core("phase-analyze", &args.args),
        Commands::Phase1(args) => join_core("phase1", &args.args),
        Commands::Clear(args) => join_core("clear", &args.args),
        Commands::Adopt(args) => join_core("adopt", &args.args),
        Commands::Reject(args) => join_core("reject", &args.args),
        Commands::Export(args) => join_core("export", &args.args),
        Commands::External(args) => external_core_input(args)?,
        Commands::Workspace { command } => workspace_core_input(*command),
        Commands::SelfCommand { command } => self_core_input(*command),
        Commands::Repl | Commands::LegacyRepl => String::new(),
    };
    Ok(input)
}

fn run_self_command(command: SelfCommand) -> Result<(), String> {
    use design_cli::runtime::self_repair::{
        apply_self_mutation, render_sandbox_mutation_result, render_self_mutation_preview,
        render_self_mutation_result, render_self_mutation_validation, render_self_rollback_result,
        rollback_self_mutation, sandbox_self_mutation, self_mutation_transaction,
    };
    use design_cli::runtime::workspace_awareness::workspace_topology_snapshot;

    let root = std::env::current_dir().map_err(|err| err.to_string())?;
    let workspace = workspace_topology_snapshot(&root);
    let transaction = self_mutation_transaction(&workspace);

    match command {
        SelfCommand::Repair {
            command: SelfRepairCommand::Preview,
        } => {
            println!("{}", render_self_mutation_preview(&transaction));
            Ok(())
        }
        SelfCommand::Repair {
            command: SelfRepairCommand::Validate,
        } => {
            println!(
                "{}",
                render_self_mutation_validation(&transaction.validation)
            );
            Ok(())
        }
        SelfCommand::Repair {
            command: SelfRepairCommand::Sandbox,
        } => {
            println!(
                "{}",
                render_sandbox_mutation_result(&sandbox_self_mutation(&transaction))
            );
            Ok(())
        }
        SelfCommand::Repair {
            command: SelfRepairCommand::Apply { yes },
        } => {
            if !yes {
                return Err("self repair apply requires explicit --yes confirmation".to_string());
            }
            println!(
                "{}",
                render_self_mutation_result(&apply_self_mutation(transaction))
            );
            Ok(())
        }
        SelfCommand::Rollback => {
            println!(
                "{}",
                render_self_rollback_result(&rollback_self_mutation(transaction.rollback_snapshot))
            );
            Ok(())
        }
    }
}

fn run_workspace_command(command: WorkspaceCommand) -> Result<(), String> {
    use design_cli::runtime::workspace_awareness::{
        render_dependency_graph, render_mutation_risks, render_runtime_boundaries,
        render_workspace_architecture, render_workspace_snapshot, runtime_boundary_map,
        workspace_dependency_graph, workspace_semantic_map, workspace_topology_snapshot,
    };

    let root = std::env::current_dir().map_err(|err| err.to_string())?;
    let snapshot = workspace_topology_snapshot(&root);
    match command {
        WorkspaceCommand::Snapshot => {
            println!("{}", render_workspace_snapshot(&snapshot));
            Ok(())
        }
        WorkspaceCommand::Graph => {
            println!(
                "{}",
                render_dependency_graph(&workspace_dependency_graph(&snapshot))
            );
            Ok(())
        }
        WorkspaceCommand::Boundaries => {
            println!(
                "{}",
                render_runtime_boundaries(&runtime_boundary_map(&snapshot))
            );
            Ok(())
        }
        WorkspaceCommand::Architecture => {
            println!(
                "{}",
                render_workspace_architecture(&workspace_semantic_map(&snapshot))
            );
            Ok(())
        }
        WorkspaceCommand::Risks => {
            println!("{}", render_mutation_risks(&snapshot));
            Ok(())
        }
    }
}

fn run_runtime_command(command: RuntimeCommand) -> Result<(), String> {
    use design_cli::runtime::persistence::{
        cognitive_session_state, persistent_revision_lineage, persistent_runtime_memory,
        render_checkpoint, render_cognitive_session_state, render_evolution_events, render_lineage,
        render_persistent_runtime_memory, restore_runtime_checkpoint, runtime_memory_checkpoint,
        runtime_memory_evolution_events,
    };
    use design_cli::runtime::unified_projection::{
        Runtime, render_revision, render_unified_snapshot, unified_runtime_snapshot,
    };

    let runtime = Runtime::default();
    let snapshot = unified_runtime_snapshot(&runtime);
    match command {
        RuntimeCommand::Snapshot => {
            println!("{}", render_unified_snapshot(&snapshot));
            Ok(())
        }
        RuntimeCommand::Revisions => {
            println!("{}", render_revision(&snapshot));
            Ok(())
        }
        RuntimeCommand::Memory => {
            let memory = persistent_runtime_memory(&runtime);
            let session = cognitive_session_state(&runtime);
            println!("{}", render_persistent_runtime_memory(&memory));
            println!();
            println!("{}", render_cognitive_session_state(&session));
            Ok(())
        }
        RuntimeCommand::Checkpoint => {
            let checkpoint = runtime_memory_checkpoint(&runtime);
            println!("{}", render_checkpoint(&checkpoint));
            Ok(())
        }
        RuntimeCommand::Restore { yes } => {
            if !yes {
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
        RuntimeCommand::Lineage => {
            let lineage = persistent_revision_lineage(&runtime);
            println!("{}", render_lineage(&lineage));
            Ok(())
        }
        RuntimeCommand::Evolution => {
            let memory = persistent_runtime_memory(&runtime);
            let events = runtime_memory_evolution_events(&memory);
            println!("{}", render_evolution_events(&events));
            Ok(())
        }
    }
}

fn runtime_core_input(command: RuntimeCommand) -> String {
    match command {
        RuntimeCommand::Snapshot => "runtime snapshot".to_string(),
        RuntimeCommand::Revisions => "runtime revisions".to_string(),
        RuntimeCommand::Lineage => "runtime lineage".to_string(),
        RuntimeCommand::Evolution => "runtime evolution".to_string(),
        RuntimeCommand::Memory => "runtime memory".to_string(),
        RuntimeCommand::Checkpoint => "runtime checkpoint".to_string(),
        RuntimeCommand::Restore { yes } => {
            if yes {
                "runtime restore --yes".to_string()
            } else {
                "runtime restore".to_string()
            }
        }
    }
}

fn workspace_core_input(command: Option<WorkspaceCommand>) -> String {
    match command {
        Some(WorkspaceCommand::Snapshot) => "workspace snapshot".to_string(),
        Some(WorkspaceCommand::Graph) => "workspace graph".to_string(),
        Some(WorkspaceCommand::Boundaries) => "workspace boundaries".to_string(),
        Some(WorkspaceCommand::Architecture) => "workspace architecture".to_string(),
        Some(WorkspaceCommand::Risks) => "workspace risks".to_string(),
        None => "workspace".to_string(),
    }
}

fn self_core_input(command: SelfCommand) -> String {
    match command {
        SelfCommand::Repair {
            command: SelfRepairCommand::Preview,
        } => "self repair preview".to_string(),
        SelfCommand::Repair {
            command: SelfRepairCommand::Validate,
        } => "self repair validate".to_string(),
        SelfCommand::Repair {
            command: SelfRepairCommand::Sandbox,
        } => "self repair sandbox".to_string(),
        SelfCommand::Repair {
            command: SelfRepairCommand::Apply { yes },
        } => {
            if yes {
                "self repair apply --yes".to_string()
            } else {
                "self repair apply".to_string()
            }
        }
        SelfCommand::Rollback => "self rollback".to_string(),
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
    input.starts_with('/') || !input.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_command(cli: &Cli) -> &Commands {
        cli.command.as_ref().expect("command")
    }

    #[test]
    fn test_repl_command_parses() {
        let cli = Cli::try_parse_from(["dbm", "repl"]).expect("parse repl");
        assert_eq!(
            runtime_intent(parsed_command(&cli)),
            Some(RuntimeIntent::Repl)
        );
    }

    #[test]
    fn test_global_repl_flag_parses_without_subcommand() {
        let cli = Cli::try_parse_from(["dbm", "--repl"]).expect("parse --repl");
        assert!(cli.repl);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_workspace_command_parses() {
        let cli = Cli::try_parse_from(["dbm", "workspace"]).expect("parse workspace");
        assert_eq!(
            runtime_intent(parsed_command(&cli)),
            Some(RuntimeIntent::Workspace)
        );
    }

    #[test]
    fn test_workspace_snapshot_command_is_not_tui_activation() {
        let cli = Cli::try_parse_from(["dbm", "workspace", "snapshot"]).expect("parse workspace");
        assert_eq!(runtime_intent(parsed_command(&cli)), None);
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "workspace snapshot"
        );
    }

    #[test]
    fn test_legacy_repl_parses() {
        let cli = Cli::try_parse_from(["dbm", "legacy-repl"]).expect("parse legacy repl");
        assert_eq!(
            runtime_intent(parsed_command(&cli)),
            Some(RuntimeIntent::LegacyRepl)
        );
    }

    #[test]
    fn test_diagnostic_flag_propagates_after_subcommand() {
        let cli =
            Cli::try_parse_from(["dbm", "repl", "--diagnostic-input"]).expect("parse diagnostic");
        assert!(cli.diagnostic_input);
        assert_eq!(
            runtime_intent(parsed_command(&cli)),
            Some(RuntimeIntent::Repl)
        );
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
        assert_eq!(runtime_intent(parsed_command(&cli)), None);
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "analyze ."
        );
    }

    #[test]
    fn test_runtime_command_parses_without_tui_activation() {
        let cli = Cli::try_parse_from(["dbm", "runtime", "snapshot"]).expect("parse runtime");
        assert_eq!(runtime_intent(parsed_command(&cli)), None);
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "runtime snapshot"
        );
    }

    #[test]
    fn test_self_repair_command_parses_without_tui_activation() {
        let cli =
            Cli::try_parse_from(["dbm", "self", "repair", "preview"]).expect("parse self repair");
        assert_eq!(runtime_intent(parsed_command(&cli)), None);
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "self repair preview"
        );
    }

    #[test]
    fn test_self_repair_apply_requires_yes() {
        let err = run_self_command(SelfCommand::Repair {
            command: SelfRepairCommand::Apply { yes: false },
        })
        .expect_err("apply without --yes should fail");
        assert!(err.contains("--yes"));
    }

    #[test]
    fn test_runtime_restore_requires_yes() {
        let err = run_runtime_command(RuntimeCommand::Restore { yes: false })
            .expect_err("restore without --yes should fail");
        assert!(err.contains("--yes"));
    }

    #[test]
    fn test_runtime_command_tree_parses() {
        let cli = Cli::try_parse_from(["dbm", "runtime", "lineage"]).expect("parse runtime");
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "runtime lineage"
        );
    }

    #[test]
    fn test_workspace_command_tree_parses() {
        let cli = Cli::try_parse_from(["dbm", "workspace", "risks"]).expect("parse workspace");
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "workspace risks"
        );
    }

    #[test]
    fn test_self_apply_command_tree_parses() {
        let cli =
            Cli::try_parse_from(["dbm", "self", "repair", "apply", "--yes"]).expect("parse self");
        assert_eq!(
            core_input(parsed_command(&cli)).expect("core input"),
            "self repair apply --yes"
        );
    }

    #[test]
    fn test_help_surfaces_are_stable() {
        let top_level = Cli::command().render_long_help().to_string();
        assert!(top_level.contains("--repl"));
        assert!(top_level.contains("Start the deterministic runtime REPL"));
        assert!(top_level.contains("repl"));
        assert!(top_level.contains("legacy-repl"));

        let runtime = Cli::command()
            .find_subcommand_mut("runtime")
            .expect("runtime command")
            .render_long_help()
            .to_string();
        assert!(runtime.contains("snapshot"));
        assert!(runtime.contains("revisions"));
        assert!(runtime.contains("lineage"));
        assert!(runtime.contains("evolution"));
        assert!(runtime.contains("memory"));
        assert!(runtime.contains("checkpoint"));
        assert!(runtime.contains("restore"));

        let workspace = Cli::command()
            .find_subcommand_mut("workspace")
            .expect("workspace command")
            .render_long_help()
            .to_string();
        assert!(workspace.contains("snapshot"));
        assert!(workspace.contains("graph"));
        assert!(workspace.contains("boundaries"));
        assert!(workspace.contains("architecture"));
        assert!(workspace.contains("risks"));

        let self_help = Cli::command()
            .find_subcommand_mut("self")
            .expect("self command")
            .render_long_help()
            .to_string();
        assert!(self_help.contains("repair"));
        assert!(self_help.contains("rollback"));
    }

    #[test]
    fn test_projection_output_does_not_use_raw_leak_labels() {
        let runtime = design_cli::runtime::unified_projection::Runtime::default();
        let snapshot = design_cli::runtime::unified_projection::unified_runtime_snapshot(&runtime);
        let rendered = [
            design_cli::runtime::unified_projection::render_unified_snapshot(&snapshot),
            design_cli::runtime::persistence::render_checkpoint(
                &design_cli::runtime::persistence::runtime_memory_checkpoint(&runtime),
            ),
            design_cli::runtime::persistence::render_lineage(
                &design_cli::runtime::persistence::persistent_revision_lineage(&runtime),
            ),
        ]
        .join("\n");
        assert!(!rendered.contains("transaction_id"));
        assert!(!rendered.contains("active_transaction_id"));
        assert!(!rendered.contains("checkpoint_id:"));
        assert!(!rendered.contains("rollback_id:"));
    }

    #[test]
    fn test_runtime_projection_replay_is_invariant() {
        let runtime = design_cli::runtime::unified_projection::Runtime::default();
        let first = design_cli::runtime::unified_projection::render_unified_snapshot(
            &design_cli::runtime::unified_projection::unified_runtime_snapshot(&runtime),
        );
        let second = design_cli::runtime::unified_projection::render_unified_snapshot(
            &design_cli::runtime::unified_projection::unified_runtime_snapshot(&runtime),
        );
        assert_eq!(first, second);
        assert!(first.contains("projection_checksum:"));
    }

    #[test]
    fn test_unknown_command_rejected() {
        let cli = Cli::try_parse_from(["dbm", "invalid-cmd"]).expect("external parse");
        let err = core_input(parsed_command(&cli)).expect_err("unknown command should reject");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn test_freeform_input_remains_core_routed() {
        let cli =
            Cli::try_parse_from(["dbm", "このプロジェクトを解析して"]).expect("external parse");
        assert_eq!(
            core_input(parsed_command(&cli)).expect("freeform input"),
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
