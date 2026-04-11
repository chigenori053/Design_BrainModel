use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Write, stdin, stdout};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};
use code_language_core::stable_v03::dynamic_ir::{
    DefaultRuleValidator, bootstrap_rule_store, promote_validated_rule, prune_rules, rollback_rule,
    should_promote_validated, validate_all_candidates, validate_candidate_rule,
};
use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use integration_layer::{
    SystemOutput, diagnostic_analysis, structural_analysis, to_relations, to_system_output,
    trace_links, validate_mapping,
};
use memory_space_phase14::stable_v03::InMemoryEngine;
use runtime_core::{CoreRuntime, RuntimeExecutionResult};
use serde::Serialize;
use serde_json::json;

use crate::autonomous_execute::{GitIntegrationOptions, execute_autonomous_command_with_options};
use crate::coding::{
    CodingOptions, apply_bootstrap_safety_policy, collect_apply_target_resolutions,
    deterministic_repl_v2_wiring_patches, execute_code_change_set,
    load_patches_from_design_snapshot, load_patches_from_json, patches_to_change_set,
    resolve_apply_paths_for_patches, resolve_sandbox_module_file,
    resolve_transactional_candidate_for_patches,
};
use crate::commands::analyze::project::{self, AnalyzeMode};
use crate::execution_foundation::{ExecAction, ExecReport, ExecutionFoundation};
use crate::r#loop::run_loop;
use crate::refactor::{
    PatchScope, RefactorApplyReport, RefactorPreviewReport, RefactorRuntimeOptions,
    candidate_for_target, persist_refactor_candidates, preview_report, resolve_target,
};
use crate::renderer::{
    render_analysis_report_markdown, render_autonomous_execute_report, render_coding_report,
    render_design_report, render_exec_report, render_refactor_apply_report,
    render_refactor_preview_report, render_result, render_rules_report, render_run_report,
    render_validation_report,
};
use crate::repl::run_repl_stdio;
use crate::runner::{
    ExecutionConfig, ExecutionResult as RunnerExecutionResult, ExecutionTarget, OutputMode,
    SandboxPolicy, TimeoutConfig, build_command, create_sandbox, detect_target, fixed_env,
    resolve_command, run as run_command,
};
use crate::service::{
    CodingReport, RuleReport, RulesReport, RunReport, RunSandbox, RunTelemetry,
    ValidatedRuleReport, analysis_to_system_input, analyze_path, build_design_report,
    build_validation_report, design_graph_from_analysis, enrich_analysis_report,
    path_contains_parent_component,
};
use crate::viewer::{
    BenchmarkCommandReport, ReplayCommandReport, StructureViewReport, TimelineCommandReport,
    ViewMode, attach_session, benchmark_structure_replay, dispatch_gui_action, edit_session,
    export_demo_replay_assets, export_structure_view, export_structure_view_from_plan,
    gui_action_path, launch_native_viewer, redo_session, structure_ir_path, summarize_timeline,
    undo_session,
};

#[derive(Parser, Debug)]
#[command(name = "cli", about = "Design Brain Model CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Generate(GenerateArgs),
    Analyze(AnalyzeArgs),
    Refactoring(RefactoringArgs),
    Design(PathArgs),
    Validate(PathArgs),
    Refactor(RefactorArgs),
    Coding(CodingArgs),
    Diff(CodingArgs),
    Check(CodingArgs),
    Apply(CodingArgs),
    Exec(ExecArgs),
    Execute(ExecuteArgs),
    Run(RunArgs),
    Wizard(WizardArgs),
    Repl(ReplArgs),
    /// Launch the interactive TUI for a saved UI payload JSON.
    Tui(TuiArgs),
    Structure(StructureArgs),
    Rules(RulesArgs),
    /// Memory management commands.
    Memory(MemoryArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct RulesArgs {
    #[command(subcommand)]
    pub command: RulesCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RulesCommands {
    List(RulesListArgs),
    Inspect(RulesRuleArgs),
    Validate(RulesRuleArgs),
    Promote(RulesPromoteArgs),
    Rollback(RulesRuleArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct RulesListArgs {
    #[arg(long, default_value = "rust")]
    pub lang: String,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RulesRuleArgs {
    pub rule_id: String,
    #[arg(long, default_value = "rust")]
    pub lang: String,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RulesPromoteArgs {
    pub rule_id: Option<String>,
    #[arg(long, default_value_t = false)]
    pub validated: bool,
    #[arg(long, default_value = "rust")]
    pub lang: String,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum MemoryCommands {
    /// Import a seed knowledge JSON file into memory and verify recall stats.
    Import(MemoryImportArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct MemoryImportArgs {
    /// Path to the seed JSON file (e.g. seeds/knowledge.json).
    pub path: PathBuf,
    /// Print per-record details after import.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct GenerateArgs {
    #[arg(long = "type")]
    pub interface_type: String,
    #[arg(long = "lang")]
    pub language: String,
    #[arg(long)]
    pub framework: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Export UI payload JSON to this path and open the TUI.
    #[arg(long)]
    pub tui: Option<PathBuf>,
    /// Save a structured operational log (JSON) to this path.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct AnalyzeArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub detailed: bool,
    #[arg(long, default_value_t = false)]
    pub report: bool,
    #[arg(long, default_value_t = false)]
    pub design: bool,
    #[arg(long, default_value = "ja")]
    pub lang: String,
    #[arg(long)]
    pub intent: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub design_json: bool,
    /// Save a structured operational log (JSON) to this path.
    #[arg(long, hide = true)]
    pub out: Option<PathBuf>,
    /// Write the diagnostic report as Markdown to this path.
    #[arg(long, hide = true)]
    pub report_md: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct PathArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RefactoringArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub node: Option<String>,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub no_build: bool,
    #[arg(long, default_value_t = false)]
    pub backup: bool,
    #[arg(long, default_value_t = false)]
    pub format: bool,
    #[arg(long, default_value_t = false)]
    pub safe: bool,
    #[arg(long, default_value_t = false)]
    pub auto_commit: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RefactorArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub node: Option<String>,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct CodingArgs {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub input: Option<PathBuf>,
    #[arg(long)]
    pub from_design_snapshot: Option<PathBuf>,
    #[arg(long)]
    pub target: Option<PathBuf>,
    #[arg(long)]
    pub candidate: Option<String>,
    #[arg(long, default_value_t = false)]
    pub apply: bool,
    #[arg(long, default_value_t = false)]
    pub check: bool,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub no_build: bool,
    #[arg(long, default_value_t = false)]
    pub backup: bool,
    #[arg(long, default_value_t = false)]
    pub format: bool,
    #[arg(long, default_value_t = false)]
    pub safe: bool,
    #[arg(long, default_value_t = false)]
    pub auto_commit: bool,
    #[arg(long, default_value_t = false)]
    pub confirm_commit: bool,
    #[arg(long, default_value_t = false)]
    pub auto_push: bool,
    #[arg(long, default_value_t = false)]
    pub confirm_push: bool,
    #[arg(long, default_value_t = false)]
    pub auto_pr: bool,
    #[arg(long, default_value_t = false)]
    pub confirm_pr: bool,
    #[arg(long, default_value = "main")]
    pub pr_base: String,
    #[arg(long, hide = true)]
    pub request: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct RunArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = 5000)]
    pub timeout_ms: u64,
    #[arg(long, default_value_t = false)]
    pub allow_network: bool,
    #[arg(long, default_value_t = true)]
    pub allow_fs_write: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ExecArgs {
    pub action: ExecAction,
    pub path: Option<PathBuf>,
    #[arg(long, default_value_t = 60_000)]
    pub timeout_ms: u64,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
#[command(visible_alias = "/execute")]
pub struct ExecuteArgs {
    pub input: String,
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[arg(long, default_value_t = 60_000)]
    pub timeout_ms: u64,
    #[arg(long, default_value_t = false)]
    pub auto_commit: bool,
    #[arg(long, default_value_t = false)]
    pub no_commit: bool,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub rollback_on_failure: bool,
    #[arg(long, default_value_t = false)]
    pub auto_remote: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct WizardArgs {
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ReplArgs {
	#[arg(long)]
	pub root: Option<std::path::PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct TuiArgs {
    /// Path to a UI payload JSON file. If omitted, runs a demo with synthetic data.
    pub file: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureArgs {
    #[command(subcommand)]
    pub command: StructureCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum StructureCommands {
    /// Export IR and launch the Native GUI Viewer.
    View(StructureViewArgs),
    /// Attach or resume a GUI Viewer edit session.
    Edit(StructureEditArgs),
    Session(StructureSessionArgs),
    /// Dispatch a GUI action event into the CLI runtime.
    Dispatch(StructureDispatchArgs),
    Undo(PathArgs),
    Redo(PathArgs),
    Replay(StructureReplayArgs),
    Timeline(StructureTimelineArgs),
    Benchmark(StructureBenchmarkArgs),
    ExportDemo(StructureExportDemoArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureViewArgs {
    pub path: PathBuf,
    #[arg(long = "2d", default_value_t = false)]
    pub two_d: bool,
    #[arg(long = "3d", default_value_t = false)]
    pub three_d: bool,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub node: Option<String>,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureDispatchArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub event: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureEditArgs {
    pub path: PathBuf,
    #[arg(long = "3d", default_value_t = false)]
    pub three_d: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureSessionArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureReplayArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub tick: Option<usize>,
    #[arg(long, default_value_t = false)]
    pub reverse: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureTimelineArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureBenchmarkArgs {
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub benchmark_json: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct StructureExportDemoArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub export_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct GenerateReport {
    pub mode: &'static str,
    pub command: String,
    pub interface_type: String,
    pub language: String,
    pub framework: Option<String>,
    pub project_root: String,
    pub manifest_path: String,
    pub files: Vec<String>,
}

pub fn run() -> Result<(), String> {
    run_with_args(std::env::args_os())
}

pub fn run_with_args<I, T>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(|err| err.to_string())?;
    dispatch(cli)
}

pub fn build_runtime() -> CoreRuntime {
    let memory = InMemoryEngine::default();
    let loaded = crate::memory_seed::load_default_seeds(&memory);
    if loaded > 0 {
        eprintln!("info: loaded {loaded} seed records into memory engine");
    }
    CoreRuntime::new_with_defaults(
        Arc::new(memory),
        Arc::new(DeterministicBeamSearchEngine::default()),
    )
}

fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Some(Commands::Generate(args)) => execute_generate(&build_runtime(), args, "command"),
        Some(Commands::Analyze(args)) => execute_analyze_with_log(args),
        Some(Commands::Refactoring(args)) => execute_refactoring(args),
        Some(Commands::Design(args)) => execute_design(args),
        Some(Commands::Validate(args)) => execute_validate(args),
        Some(Commands::Refactor(args)) => execute_refactor(args),
        Some(Commands::Coding(args)) => execute_coding(args, CodingMode::Coding),
        Some(Commands::Diff(args)) => execute_coding(args, CodingMode::Diff),
        Some(Commands::Check(args)) => execute_coding(args, CodingMode::Check),
        Some(Commands::Apply(args)) => execute_coding(args, CodingMode::Apply),
        Some(Commands::Exec(args)) => execute_exec(args),
        Some(Commands::Execute(args)) => execute_autonomous(args),
        Some(Commands::Run(args)) => execute_run(args),
        Some(Commands::Wizard(args)) => wizard_mode(args),
        Some(Commands::Repl(args)) => repl_mode(args),
        Some(Commands::Tui(args)) => execute_tui(args),
        Some(Commands::Structure(args)) => execute_structure(args),
        Some(Commands::Rules(args)) => execute_rules(args),
        Some(Commands::Memory(args)) => execute_memory(args),
        None => {
            let mut cmd = Cli::command();
            cmd.print_long_help().map_err(|err| err.to_string())?;
            println!();
            Ok(())
        }
    }
}

fn execute_generate(
    runtime: &CoreRuntime,
    args: GenerateArgs,
    mode: &'static str,
) -> Result<(), String> {
    use std::time::Instant;

    let prompt = build_generate_prompt(&args);
    let context = runtime_core::ChatContext::default();
    let started = Instant::now();
    let exec_result = runtime.execute_from_text(&prompt, &context);
    let latency_ms = started.elapsed().as_millis();

    // Determine request_id (best-effort before match).
    let request_id_hint = uuid::Uuid::new_v4().to_string();

    let out_path = args.out.clone();
    let tui_path = args.tui.clone();

    match exec_result {
        Ok(RuntimeExecutionResult::Executed(result)) => {
            let _trace_links = trace_links(&result.output_relations);
            let request_id = result
                .reasoning_trace
                .as_ref()
                .map(|t| t.request_id.0.clone())
                .unwrap_or(request_id_hint);

            // Write operational log if --out was given.
            if let Some(ref path) = out_path {
                let log = crate::ops::RunLogBuilder {
                    request_id: request_id.clone(),
                    input: prompt.clone(),
                    latency_ms,
                }
                .success(&result);
                if let Err(e) = crate::ops::write_log(&log, path) {
                    eprintln!("warn: could not write log: {e}");
                }
            }

            let report = match &result.system_output {
                SystemOutput::Design(_) | SystemOutput::Actions(_) | SystemOutput::Ui(_) => {
                    GenerateReport {
                        mode,
                        command: generate_command_string(&args),
                        interface_type: args.interface_type,
                        language: args.language,
                        framework: args.framework,
                        project_root: result.project_layout.root_dir.clone(),
                        manifest_path: result.project_layout.manifest_path.clone(),
                        files: result
                            .project_layout
                            .files
                            .iter()
                            .map(|file| file.path.clone())
                            .collect(),
                    }
                }
            };
            if report_json(args.json, &report)? {
                return Ok(());
            }
            if let Some(tui_path) = tui_path {
                let payload = ui_payload_from_result(&result);
                let json = serde_json::to_string_pretty(&payload)
                    .map_err(|e| format!("ui payload serialization failed: {e}"))?;
                fs::write(&tui_path, &json)
                    .map_err(|e| format!("cannot write {}: {e}", tui_path.display()))?;
                return crate::tui::run_tui(payload);
            }
            render_result(&mut io::stdout().lock(), &result).map_err(|err| err.to_string())
        }
        Ok(RuntimeExecutionResult::Clarification(clarification)) => {
            if let Some(ref path) = out_path {
                let log = crate::ops::RunLogBuilder {
                    request_id: request_id_hint,
                    input: prompt.clone(),
                    latency_ms,
                }
                .failure(
                    crate::ops::FailureType::SearchFailure,
                    format!("clarification required: {}", clarification.message),
                );
                let _ = crate::ops::write_log(&log, path);
            }
            Err(format!(
                "generate requires more input: {}",
                clarification.message
            ))
        }
        Err(err) => {
            if let Some(ref path) = out_path {
                let log = crate::ops::RunLogBuilder {
                    request_id: request_id_hint,
                    input: prompt.clone(),
                    latency_ms,
                }
                .failure(crate::ops::FailureType::SystemError, format!("{err:?}"));
                let _ = crate::ops::write_log(&log, path);
            }
            Err(format!("generate failed: {err:?}"))
        }
    }
}

fn execute_analyze_with_log(args: AnalyzeArgs) -> Result<(), String> {
    use std::time::Instant;

    let started = Instant::now();
    let mut forwarded = vec![args.path.display().to_string()];
    if args.detailed {
        forwarded.push("--detailed".to_string());
    }
    if args.report {
        forwarded.push("--report".to_string());
    }
    if args.design {
        forwarded.push("--design".to_string());
    }
    forwarded.push("--lang".to_string());
    forwarded.push(args.lang.clone());
    if let Some(intent) = &args.intent {
        forwarded.push("--intent".to_string());
        forwarded.push(intent.clone());
    }
    if args.json {
        forwarded.push("--json".to_string());
    }
    if args.design_json {
        forwarded.push("--design-json".to_string());
    }

    let output_result = (|| {
        let options = project::parse_options(&forwarded)?;
        let mode = if args.detailed {
            AnalyzeMode::Detailed
        } else {
            AnalyzeMode::Summary
        };
        let options = project::AnalyzeOptions {
            path: args.path.display().to_string(),
            mode,
            report: options.report,
            design: options.design,
            language: options.language,
            intent: options.intent,
            json: options.json,
            design_json: options.design_json,
        };
        project::execute(&options.path.clone(), options)
    })();
    let latency_ms = started.elapsed().as_millis();

    if let Some(ref out) = args.out {
        let (success, actual) = match &output_result {
            Ok(_) => (true, None),
            Err(err) => (false, Some(err.clone())),
        };
        let log = crate::ops::AnalyzeLog {
            path: args.path.display().to_string(),
            latency_ms,
            success,
            actual,
        };
        if let Err(err) = crate::ops::write_analyze_log(&log, out) {
            eprintln!("warn: could not write log: {err}");
        }
    }

    if let Some(path) = &args.report_md {
        let report = analyze_path(&args.path)?;
        let canonical_input = analysis_to_system_input(&report);
        let relations = to_relations(canonical_input.clone());
        let _system_output = to_system_output(relations.clone());
        let validation = validate_mapping(&canonical_input, &relations);
        if !validation.is_valid {
            return Err("integration mapping failed for analysis report".to_string());
        }
        let design_graph = design_graph_from_analysis(&report);
        let report = enrich_analysis_report(report, diagnostic_analysis(&design_graph));
        fs::write(path, render_analysis_report_markdown(&report))
            .map_err(|err| format!("failed to write markdown report {}: {err}", path.display()))?;
    }

    let output = output_result?;
    println!("{output}");
    Ok(())
}

fn execute_design(args: PathArgs) -> Result<(), String> {
    let report = build_design_report(&args.path)?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_design_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_validate(args: PathArgs) -> Result<(), String> {
    let report = build_validation_report(&args.path)?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_validation_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_refactor(args: RefactorArgs) -> Result<(), String> {
    let analysis = analyze_path(&args.path)?;
    let target = resolve_target(
        &analysis,
        args.target.as_deref(),
        args.node.as_deref(),
        args.file.as_deref(),
    );
    let candidate = candidate_for_target(&analysis, &target);
    persist_refactor_candidates(&args.path, &[candidate])?;
    let report: RefactorPreviewReport = preview_report(&args.path, Some(target))?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_refactor_preview_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_refactoring(args: RefactoringArgs) -> Result<(), String> {
    let analysis = analyze_path(&args.path)?;
    let target = resolve_target(
        &analysis,
        args.target.as_deref(),
        args.node.as_deref(),
        args.file.as_deref(),
    );
    let preview = preview_report(&args.path, Some(target))?;
    let report: RefactorApplyReport = if args.dry_run {
        RefactorApplyReport {
            root: preview.root.clone(),
            plan: preview.plan.clone(),
            preview: preview.preview.clone(),
            validation: preview.validation.clone(),
            apply: crate::refactor::ApplyResult {
                applied: false,
                build_ok: false,
                rolled_back: false,
                changed_files: Vec::new(),
                commit_id: None,
            },
        }
    } else {
        crate::refactor::build_apply_report(
            &preview.plan,
            &RefactorRuntimeOptions {
                auto_commit: args.auto_commit,
                no_build: args.no_build,
                backup: args.backup,
                format: args.format,
            },
        )?
    };
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_refactor_apply_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_structure(args: StructureArgs) -> Result<(), String> {
    match args.command {
        StructureCommands::View(args) => execute_structure_view(args),
        StructureCommands::Edit(args) => execute_structure_edit(args),
        StructureCommands::Session(args) => execute_structure_session(args),
        StructureCommands::Dispatch(args) => execute_structure_dispatch(args),
        StructureCommands::Undo(args) => execute_structure_undo(args),
        StructureCommands::Redo(args) => execute_structure_redo(args),
        StructureCommands::Replay(args) => execute_structure_replay(args),
        StructureCommands::Timeline(args) => execute_structure_timeline(args),
        StructureCommands::Benchmark(args) => execute_structure_benchmark(args),
        StructureCommands::ExportDemo(args) => execute_structure_export_demo(args),
    }
}

fn execute_structure_view(args: StructureViewArgs) -> Result<(), String> {
    let mode = if args.three_d {
        ViewMode::ThreeD
    } else {
        let _ = args.two_d;
        ViewMode::TwoD
    };
    let ir = if args.target.is_some() || args.node.is_some() || args.file.is_some() {
        let analysis = analyze_path(&args.path)?;
        let target = resolve_target(
            &analysis,
            args.target.as_deref(),
            args.node.as_deref(),
            args.file.as_deref(),
        );
        let preview = preview_report(&args.path, Some(target))?;
        export_structure_view_from_plan(&args.path, &preview.plan)?
    } else {
        export_structure_view(&args.path)?
    };
    let ir_path = structure_ir_path(&args.path);
    let launch = launch_native_viewer(mode, &args.path, &ir_path)?;
    let report = StructureViewReport {
        root: args.path.display().to_string(),
        mode,
        ir_path: ir_path.display().to_string(),
        launch_url: launch.url,
        launched: launch.launched,
        viewer_loop: launch.telemetry,
        ir,
    };
    if report_json(args.json, &report)? {
        return Ok(());
    }
    writeln!(io::stdout().lock(), "GUI Viewer").map_err(|err| err.to_string())?;
    writeln!(io::stdout().lock(), "Root: {}", report.root).map_err(|err| err.to_string())?;
    writeln!(io::stdout().lock(), "Mode: {:?}", report.mode).map_err(|err| err.to_string())?;
    writeln!(io::stdout().lock(), "IR: {}", report.ir_path).map_err(|err| err.to_string())?;
    writeln!(
        io::stdout().lock(),
        "GUI Viewer: {}",
        if report.launched {
            "launched"
        } else {
            "not launched"
        }
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn execute_structure_dispatch(args: StructureDispatchArgs) -> Result<(), String> {
    let event_path = args.event.unwrap_or_else(|| gui_action_path(&args.path));
    let raw = fs::read_to_string(&event_path)
        .map_err(|err| format!("failed to read {}: {err}", event_path.display()))?;
    let event =
        serde_json::from_str(&raw).map_err(|err| format!("invalid GUI event JSON: {err}"))?;
    let (command, ir) = dispatch_gui_action(&args.path, event)?;
    let report = serde_json::json!({
        "root": args.path.display().to_string(),
        "event_path": event_path.display().to_string(),
        "command": command,
        "viewer_loop": crate::viewer::ViewerLoopTelemetry::default(),
        "ir": ir,
    });
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_edit(args: StructureEditArgs) -> Result<(), String> {
    let mode = if args.three_d {
        ViewMode::ThreeD
    } else {
        ViewMode::TwoD
    };
    let report = edit_session(&args.path, mode)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_session(args: StructureSessionArgs) -> Result<(), String> {
    let session = attach_session(&args.path)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&session).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&session).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_undo(args: PathArgs) -> Result<(), String> {
    let report = undo_session(&args.path)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_redo(args: PathArgs) -> Result<(), String> {
    let report = redo_session(&args.path)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_replay(args: StructureReplayArgs) -> Result<(), String> {
    let report: ReplayCommandReport =
        crate::viewer::replay::replay_structure(&args.path, args.tick, args.reverse)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_timeline(args: StructureTimelineArgs) -> Result<(), String> {
    let report: TimelineCommandReport = summarize_timeline(&args.path)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_benchmark(args: StructureBenchmarkArgs) -> Result<(), String> {
    let report: BenchmarkCommandReport = benchmark_structure_replay(&args.path)?;
    if args.benchmark_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn execute_structure_export_demo(args: StructureExportDemoArgs) -> Result<(), String> {
    let report = export_demo_replay_assets(&args.path, args.export_dir.as_deref())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum CodingMode {
    Coding,
    Diff,
    Check,
    Apply,
}

fn execute_coding(mut args: CodingArgs, mode: CodingMode) -> Result<(), String> {
    match mode {
        CodingMode::Coding => {}
        CodingMode::Diff => {
            args.apply = false;
            args.check = false;
        }
        CodingMode::Check => {
            args.apply = false;
            args.check = true;
        }
        CodingMode::Apply => {
            args.apply = true;
            args.check = true;
        }
    }
    if args.dry_run {
        args.apply = false;
    }
    let (root, patches, mutation_resolution) = if let Some(input) = &args.input {
        let root = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
        (root, load_patches_from_json(input)?, None)
    } else if let Some(snapshot) = &args.from_design_snapshot {
        let Some(path) = args.path.clone() else {
            return Err("coding requires <path> with --from-design-snapshot".to_string());
        };
        let (plan, patches, resolution) = load_patches_from_design_snapshot(&path, snapshot)?;
        if args.target.is_none()
            && plan.constraints.target_scope_locked
            && !matches!(
                plan.operation,
                crate::service::MutationOperation::BreakCycle
            )
        {
            args.target = resolution.canonical_target_path.clone();
        }
        (path, patches, Some(resolution))
    } else {
        let Some(path) = args.path.clone() else {
            return Err("coding requires either <path> or --input".to_string());
        };
        if let Some(target) = args.target.as_deref()
            && let Some(patches) =
                deterministic_repl_v2_wiring_patches(&path, target, args.request.as_deref())
        {
            (path, patches, None)
        } else {
            let analysis = analyze_path(&path)?;
            let design_graph = design_graph_from_analysis(&analysis);
            let structural = structural_analysis(&design_graph);
            (path, structural.code_patches, None)
        }
    };
    let patches = apply_bootstrap_safety_policy(&patches, args.target.as_deref());
    let resolved_paths = if args.apply {
        resolve_apply_paths_for_patches(&root, &patches, args.candidate.as_deref())?
    } else {
        std::collections::BTreeMap::new()
    };
    let transactional_candidate = if args.apply {
        resolve_transactional_candidate_for_patches(&root, &patches, args.candidate.as_deref())?
    } else {
        None
    };
    let changes = patches_to_change_set(
        &root,
        &patches,
        args.target.as_deref(),
        &resolved_paths,
        mutation_resolution
            .as_ref()
            .and_then(|resolution| resolution.canonical_target_path.as_deref()),
    )?;
    let mut execution = execute_code_change_set(
        &root,
        &changes,
        &CodingOptions {
            apply: args.apply,
            check: args.check,
            no_build: args.no_build,
            backup: args.backup,
            format: args.format,
            safe_mode: true,
            auto_commit: args.auto_commit,
            confirm_commit: args.confirm_commit,
            prompt_commit: args.auto_commit && !args.json,
            auto_push: args.auto_push,
            confirm_push: args.confirm_push,
            auto_pr: args.auto_pr,
            confirm_pr: args.confirm_pr,
            pr_base: args.pr_base.clone(),
            patch_scope: if args.target.is_some() {
                PatchScope::ExplicitTargetOnly
            } else {
                PatchScope::WorkspaceWide
            },
            explicit_target: args.target.clone(),
        },
        transactional_candidate.as_ref(),
    )?;
    if let Some(resolution) = mutation_resolution.as_ref() {
        execution.canonical_target_path = resolution
            .canonical_target_path
            .as_ref()
            .map(|path| path.display().to_string());
        execution.legacy_pipeline_hits = resolution.legacy_pipeline_hits;
        execution.fallback_resolution_hits = resolution.fallback_resolution_hits;
        execution.stale_artifact_detected = resolution.stale_artifact_detected;
        if resolution.stale_artifact_detected {
            execution.reason = Some(match execution.reason.take() {
                Some(reason) => {
                    format!("stale snapshot artifact detected: resolver_version mismatch\n{reason}")
                }
                None => "stale snapshot artifact detected: resolver_version mismatch".to_string(),
            });
        }
    }
    let mut apply_resolutions =
        collect_apply_target_resolutions(&root, &patches, args.target.as_deref(), &resolved_paths)?;
    if let Some(transactional) = execution.transactional_apply.as_ref() {
        for resolution in &mut apply_resolutions {
            resolution.sandbox_path =
                resolve_sandbox_module_file(&resolution.module, &root, &transactional.sandbox_path)
                    .ok()
                    .or_else(|| {
                        Some(
                            transactional
                                .sandbox_path
                                .join(&resolution.resolved_relative_path),
                        )
                    });
        }
    }
    let coding_report = CodingReport {
        root: root.display().to_string(),
        dry_run: !args.apply,
        execution,
        // Use the canonical narrowed stream stored in CodeChangeSet (R3: candidate
        // log count must equal the executable diff count).
        patches: changes.patches.clone(),
        changes,
        apply_resolutions,
    };
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&coding_report).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    render_coding_report(&mut io::stdout().lock(), &coding_report).map_err(|err| err.to_string())
}

fn execute_run(args: RunArgs) -> Result<(), String> {
    let report = execute_run_command(&args)?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_run_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_exec(args: ExecArgs) -> Result<(), String> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    let report = execute_exec_command(&path, args.action, args.timeout_ms)?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_exec_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_autonomous(args: ExecuteArgs) -> Result<(), String> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    let report = execute_autonomous_command_with_options(
        &path,
        &args.input,
        args.timeout_ms,
        GitIntegrationOptions {
            auto_commit: args.auto_commit,
            require_confirmation: !args.auto_commit && !args.json,
            no_commit: args.no_commit,
            dry_run: args.dry_run,
            rollback_on_failure: args.rollback_on_failure,
            auto_remote: args.auto_remote,
            enable_remote: !args.json,
        },
    )?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_autonomous_execute_report(&mut io::stdout().lock(), &report)
        .map_err(|err| err.to_string())
}

fn execute_rules(args: RulesArgs) -> Result<(), String> {
    let validator = DefaultRuleValidator;
    match args.command {
        RulesCommands::List(args) => {
            let mut store = bootstrap_rule_store(&args.lang);
            prune_rules(&mut store, 100);
            let report = rules_report_from_store(&args.lang, "list", &store, None);
            if report_json(args.json, &report)? {
                return Ok(());
            }
            render_rules_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
        }
        RulesCommands::Inspect(args) => {
            let store = bootstrap_rule_store(&args.lang);
            let message = inspect_rule_message(&store, &args.rule_id);
            let report = rules_report_from_store(&args.lang, "inspect", &store, message);
            if report_json(args.json, &report)? {
                return Ok(());
            }
            render_rules_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
        }
        RulesCommands::Validate(args) => {
            let mut store = bootstrap_rule_store(&args.lang);
            if let Some(validated) = validate_candidate_rule(&store, &args.rule_id, &validator) {
                store.validated_rules.push(validated);
            }
            let message = if store.validated_rules.is_empty() {
                Some(format!("rule not found or not candidate: {}", args.rule_id))
            } else {
                Some(format!("validated {}", args.rule_id))
            };
            let report = rules_report_from_store(&args.lang, "validate", &store, message);
            if report_json(args.json, &report)? {
                return Ok(());
            }
            render_rules_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
        }
        RulesCommands::Promote(args) => {
            let mut store = bootstrap_rule_store(&args.lang);
            store.validated_rules = validate_all_candidates(&store, &validator);
            let promoted = if args.validated {
                let ids = store
                    .validated_rules
                    .iter()
                    .filter(|record| {
                        should_promote_validated(
                            &record.rule,
                            &code_language_core::stable_v03::dynamic_ir::ValidationResult {
                                passed: record.passed_checks.len() == 5,
                                score: record.validation_score,
                                checks: record.passed_checks.clone(),
                            },
                        )
                    })
                    .map(|record| record.rule.id.clone())
                    .collect::<Vec<_>>();
                let mut count = 0;
                for rule_id in ids {
                    if promote_validated_rule(&mut store, &rule_id) {
                        count += 1;
                    }
                }
                count
            } else if let Some(rule_id) = args.rule_id.as_deref() {
                let allow = store
                    .validated_rules
                    .iter()
                    .find(|record| record.rule.id == rule_id)
                    .map(|record| {
                        should_promote_validated(
                            &record.rule,
                            &code_language_core::stable_v03::dynamic_ir::ValidationResult {
                                passed: record.passed_checks.len() == 5,
                                score: record.validation_score,
                                checks: record.passed_checks.clone(),
                            },
                        )
                    })
                    .unwrap_or(false);
                usize::from(allow && promote_validated_rule(&mut store, rule_id))
            } else {
                0
            };
            let message = Some(format!("promoted {} rule(s)", promoted));
            let report = rules_report_from_store(&args.lang, "promote", &store, message);
            if report_json(args.json, &report)? {
                return Ok(());
            }
            render_rules_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
        }
        RulesCommands::Rollback(args) => {
            let mut store = bootstrap_rule_store(&args.lang);
            let rolled_back = rollback_rule(&mut store, &args.rule_id);
            let message = Some(if rolled_back {
                format!("rolled back {}", args.rule_id)
            } else {
                format!("active rule not found: {}", args.rule_id)
            });
            let report = rules_report_from_store(&args.lang, "rollback", &store, message);
            if report_json(args.json, &report)? {
                return Ok(());
            }
            render_rules_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
        }
    }
}

fn wizard_mode(args: WizardArgs) -> Result<(), String> {
    let runtime = build_runtime();
    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    writeln!(writer, "Wizard Mode").map_err(|err| err.to_string())?;
    let interface_type = prompt_value(&mut reader, &mut writer, "interface")?;
    let language = prompt_value(&mut reader, &mut writer, "language")?;
    let framework = prompt_optional_value(&mut reader, &mut writer, "framework")?;
    let generate = GenerateArgs {
        interface_type,
        language,
        framework,
        json: args.json,
        tui: None,
        out: None,
    };
    writeln!(writer, "Executing: {}", generate_command_string(&generate))
        .map_err(|err| err.to_string())?;
    execute_generate(&runtime, generate, "wizard")
}

fn repl_mode(args: ReplArgs) -> Result<(), String> {
    let workspace_root = args
    	.root
    	.clone()
    	.unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?);
    run_repl_stdio(workspace_root)
}

pub fn run_chat_loop() -> Result<(), String> {
    let runtime = build_runtime();
    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    run_loop(&runtime, &mut reader, &mut writer)
}

fn build_generate_prompt(args: &GenerateArgs) -> String {
    match &args.framework {
        Some(framework) => format!(
            "build {} {} {}",
            args.language, args.interface_type, framework
        ),
        None => format!("build {} {}", args.language, args.interface_type),
    }
}

fn generate_command_string(args: &GenerateArgs) -> String {
    let mut command = format!(
        "cli generate --type {} --lang {}",
        args.interface_type, args.language
    );
    if let Some(framework) = &args.framework {
        command.push_str(&format!(" --framework {framework}"));
    }
    command
}

fn prompt_value<R, W>(reader: &mut R, writer: &mut W, name: &str) -> Result<String, String>
where
    R: BufRead,
    W: Write,
{
    loop {
        write!(writer, "{name}? ").map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).map_err(|err| err.to_string())?;
        if bytes == 0 {
            return Err("wizard terminated by EOF".to_string());
        }
        let value = line.trim();
        if matches!(value, "exit" | "quit") {
            return Err("wizard terminated by user".to_string());
        }
        if !value.is_empty() {
            return Ok(value.to_string());
        }
    }
}

fn prompt_optional_value<R, W>(
    reader: &mut R,
    writer: &mut W,
    name: &str,
) -> Result<Option<String>, String>
where
    R: BufRead,
    W: Write,
{
    write!(writer, "{name}? ").map_err(|err| err.to_string())?;
    writer.flush().map_err(|err| err.to_string())?;
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).map_err(|err| err.to_string())?;
    if bytes == 0 {
        return Err("wizard terminated by EOF".to_string());
    }
    let value = line.trim();
    if matches!(value, "exit" | "quit") {
        return Err("wizard terminated by user".to_string());
    }
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

fn execute_run_command(args: &RunArgs) -> Result<RunReport, String> {
    if path_contains_parent_component(&args.path) {
        return Err(
            "ValidationError: working directory contains forbidden parent traversal".to_string(),
        );
    }
    let canonical_path = args
        .path
        .canonicalize()
        .map_err(|err| format!("failed to resolve run path {}: {err}", args.path.display()))?;
    let sandbox = create_sandbox(&canonical_path).map_err(|err| err.to_string())?;
    let sandbox_path = sandbox.guard.path();
    let target = detect_run_target(sandbox_path)?;
    let (command_name, command_args) = build_command(&target);
    let resolved_command = resolve_command(&command_name).map_err(|err| err.to_string())?;
    let config = ExecutionConfig {
        command: resolved_command,
        args: command_args.clone(),
        working_dir: sandbox_path.display().to_string(),
        timeout_ms: args.timeout_ms,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    };
    let policy = SandboxPolicy {
        allow_network: args.allow_network,
        allow_fs_write: args.allow_fs_write,
        allowed_paths: vec![sandbox_path.display().to_string()],
    };
    let timeout = TimeoutConfig {
        timeout_ms: args.timeout_ms,
        kill_signal: "kill".to_string(),
    };
    let sandbox_mode = sandbox.mode;
    let result = run_command(&config, &timeout, &policy, sandbox_path, sandbox_mode)
        .map_err(|err| err.to_string())?;
    Ok(to_run_report(
        sandbox_path,
        &config,
        &timeout,
        &policy,
        result,
    ))
}

pub fn execute_exec_command(
    path: &Path,
    action: ExecAction,
    timeout_ms: u64,
) -> Result<ExecReport, String> {
    ExecutionFoundation::execute(path, action, timeout_ms)
}

fn detect_run_target(path: &Path) -> Result<ExecutionTarget, String> {
    detect_target(path).map_err(|err| err.to_string())
}

fn to_run_report(
    path: &Path,
    config: &ExecutionConfig,
    timeout: &TimeoutConfig,
    policy: &SandboxPolicy,
    result: RunnerExecutionResult,
) -> RunReport {
    RunReport {
        root: path.display().to_string(),
        status: result.status.clone(),
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        stdout: result.stdout,
        stderr: result.stderr,
        command: config.command.clone(),
        args: config.args.clone(),
        telemetry: RunTelemetry {
            duration_ms: result.telemetry.duration_ms,
            exit_code: result.telemetry.exit_code,
            stdout_size: result.telemetry.stdout_size,
            stderr_size: result.telemetry.stderr_size,
            memory_usage_kb: result.telemetry.memory_usage_kb,
            cpu_release: result.telemetry.cpu_release,
        },
        sandbox: RunSandbox {
            max_execution_time_ms: timeout.timeout_ms,
            allow_network: policy.allow_network,
            allow_fs_write: policy.allow_fs_write,
            allowed_paths: policy.allowed_paths.clone(),
            working_dir: config.working_dir.clone(),
            timed_out: result.status == "timeout",
        },
        output_meta: result.output_meta,
        stderr_meta: result.stderr_meta,
        sandbox_mode: result.sandbox_mode,
        deterministic: true,
    }
}

fn report_json<T: Serialize>(enabled: bool, report: &T) -> Result<bool, String> {
    if !enabled {
        return Ok(false);
    }
    let payload = json!(report);
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?
    );
    Ok(true)
}

fn rules_report_from_store(
    lang: &str,
    action: &str,
    store: &code_language_core::stable_v03::dynamic_ir::RuleStore,
    message: Option<String>,
) -> RulesReport {
    RulesReport {
        language: lang.to_string(),
        action: action.to_string(),
        active: store
            .active_rules
            .iter()
            .map(|record| rule_report(&record.rule, "active"))
            .collect(),
        candidate: store
            .candidate_rules
            .iter()
            .map(|record| rule_report(&record.rule, "candidate"))
            .collect(),
        validated: store
            .validated_rules
            .iter()
            .map(|record| ValidatedRuleReport {
                id: record.rule.id.clone(),
                validation_score: record.validation_score,
                passed_checks: record
                    .passed_checks
                    .iter()
                    .map(validation_check_label)
                    .map(str::to_string)
                    .collect(),
                source: rule_source_label(&record.rule.source).to_string(),
            })
            .collect(),
        deprecated: store
            .deprecated_rules
            .iter()
            .map(|record| rule_report(&record.rule, "deprecated"))
            .collect(),
        message,
    }
}

fn rule_report(
    rule: &code_language_core::stable_v03::dynamic_ir::MappingRule,
    bucket: &str,
) -> RuleReport {
    RuleReport {
        id: rule.id.clone(),
        priority: rule.priority,
        confidence: rule.confidence,
        usage_count: rule.usage_count,
        source: rule_source_label(&rule.source).to_string(),
        bucket: bucket.to_string(),
    }
}

fn inspect_rule_message(
    store: &code_language_core::stable_v03::dynamic_ir::RuleStore,
    rule_id: &str,
) -> Option<String> {
    if let Some(record) = store
        .active_rules
        .iter()
        .find(|record| record.rule.id == rule_id)
    {
        return Some(format!(
            "active rule {} (confidence {:.2}, usage {})",
            record.rule.id, record.rule.confidence, record.rule.usage_count
        ));
    }
    if let Some(record) = store
        .candidate_rules
        .iter()
        .find(|record| record.rule.id == rule_id)
    {
        return Some(format!(
            "candidate rule {} (confidence {:.2}, usage {})",
            record.rule.id, record.rule.confidence, record.rule.usage_count
        ));
    }
    if let Some(record) = store
        .validated_rules
        .iter()
        .find(|record| record.rule.id == rule_id)
    {
        return Some(format!(
            "validated rule {} (validation {:.2})",
            record.rule.id, record.validation_score
        ));
    }
    None
}

fn rule_source_label(
    source: &code_language_core::stable_v03::dynamic_ir::RuleSource,
) -> &'static str {
    match source {
        code_language_core::stable_v03::dynamic_ir::RuleSource::Static => "Static",
        code_language_core::stable_v03::dynamic_ir::RuleSource::Learned => "Learned",
        code_language_core::stable_v03::dynamic_ir::RuleSource::User => "User",
    }
}

fn validation_check_label(
    check: &code_language_core::stable_v03::dynamic_ir::ValidationCheck,
) -> &'static str {
    match check {
        code_language_core::stable_v03::dynamic_ir::ValidationCheck::RegressionPass => {
            "RegressionPass"
        }
        code_language_core::stable_v03::dynamic_ir::ValidationCheck::Deterministic => {
            "Deterministic"
        }
        code_language_core::stable_v03::dynamic_ir::ValidationCheck::NoConflict => "NoConflict",
        code_language_core::stable_v03::dynamic_ir::ValidationCheck::DiffSafe => "DiffSafe",
        code_language_core::stable_v03::dynamic_ir::ValidationCheck::CrossLanguageConsistent => {
            "CrossLanguageConsistent"
        }
    }
}

// ── Memory command ────────────────────────────────────────────────────────────

fn execute_memory(args: MemoryArgs) -> Result<(), String> {
    match args.command {
        MemoryCommands::Import(import_args) => execute_memory_import(import_args),
    }
}

fn execute_memory_import(args: MemoryImportArgs) -> Result<(), String> {
    use memory_space_phase14::stable_v03::{MemoryEngine, MemoryQuery};

    let engine = InMemoryEngine::default();
    let count = crate::memory_seed::load_seeds_into(&engine, &args.path);
    if count == 0 {
        return Err(format!("no records loaded from {}", args.path.display()));
    }

    println!("Loaded {count} seed records from {}", args.path.display());

    // Spot-check recall for common patterns.
    let probes = [
        ("web rust", vec!["web", "rust"]),
        ("cli tool", vec!["cli"]),
        ("service backend", vec!["service"]),
        ("database postgres", vec!["db"]),
    ];
    println!("\nRecall spot-check:");
    for (text, tags) in &probes {
        let results = engine.retrieve(MemoryQuery {
            text: text.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            limit: 3,
        });
        println!("  {:20} → {} result(s)", text, results.len());
        if args.verbose {
            for r in &results {
                println!("    [{}] {:.60}", r.id, r.text);
            }
        }
    }

    Ok(())
}

// ── TUI command ──────────────────────────────────────────────────────────────

fn execute_tui(args: TuiArgs) -> Result<(), String> {
    use crate::tui::model::UiPayload;
    use crate::tui::run_tui;

    let payload = if let Some(path) = args.file {
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        serde_json::from_str::<UiPayload>(&raw)
            .map_err(|e| format!("invalid UI payload JSON: {e}"))?
    } else {
        demo_payload()
    };

    run_tui(payload)
}

/// Build a UiPayload from a completed RuntimeResult.
/// Exported so other CLI commands can call it (e.g. `generate --tui`).
pub fn ui_payload_from_result(
    result: &runtime_core::stable_v03::RuntimeResult,
) -> crate::tui::model::UiPayload {
    use crate::tui::model::{
        HypothesisViewModel, MemoryCandidateViewModel, ScorePartsViewModel, TraceStatsViewModel,
        TraceStepViewModel, TraceViewModel, UiPayload,
    };

    let (request_id, steps, stats) = result
        .reasoning_trace
        .as_ref()
        .map(|t| {
            let steps = t
                .steps
                .iter()
                .map(|s| TraceStepViewModel {
                    depth: s.depth,
                    beam_width: s.beam_width,
                    candidates: s.candidates,
                    pruned: s.pruned,
                    recall_hits: s.recall_hits,
                })
                .collect::<Vec<_>>();
            let stats = TraceStatsViewModel {
                total_nodes: t.stats.total_nodes,
                max_depth: t.stats.max_depth,
                recall_hit_rate: t.stats.recall_hit_rate,
                avg_branching: t.stats.avg_branching,
            };
            (t.request_id.0.clone(), steps, stats)
        })
        .unwrap_or_else(|| {
            (
                "unknown".to_string(),
                vec![],
                TraceStatsViewModel {
                    total_nodes: 0,
                    max_depth: 0,
                    recall_hit_rate: 0.0,
                    avg_branching: 0.0,
                },
            )
        });

    let hypotheses: Vec<HypothesisViewModel> = result
        .scored_candidates
        .iter()
        .enumerate()
        .map(|(idx, sc)| {
            let m = &sc.evaluation.metrics;
            HypothesisViewModel {
                id: idx,
                parent: if sc.candidate.depth > 0 {
                    Some(idx.saturating_sub(1))
                } else {
                    None
                },
                depth: sc.candidate.depth,
                score: sc.evaluation.score as f32,
                score_parts: ScorePartsViewModel {
                    relevance: m.modularity as f32,
                    goal: m.cohesion as f32,
                    constraint: (1.0 - m.coupling) as f32,
                    memory: (1.0 - m.complexity) as f32,
                },
                relations: vec![],
            }
        })
        .collect();

    let memory: Vec<MemoryCandidateViewModel> = result
        .recall_records
        .iter()
        .enumerate()
        .map(|(rank, r)| {
            let score = r.score as f32;
            MemoryCandidateViewModel {
                id: r.record.id.clone(),
                score,
                source: MemoryCandidateViewModel::source_from_score(score).to_string(),
                rank,
                tags: r.record.tags.iter().take(3).cloned().collect(),
            }
        })
        .collect();

    let selected = hypotheses.first().map(|h| h.id);

    UiPayload {
        trace: TraceViewModel {
            request_id,
            steps,
            stats,
        },
        hypotheses,
        memory,
        selected,
    }
}

/// Demo payload used when no JSON file is provided.
fn demo_payload() -> crate::tui::model::UiPayload {
    use crate::tui::model::{
        HypothesisRelationViewModel, HypothesisViewModel, MemoryCandidateViewModel,
        ScorePartsViewModel, TraceStatsViewModel, TraceStepViewModel, TraceViewModel, UiPayload,
    };

    let steps = vec![
        TraceStepViewModel {
            depth: 0,
            beam_width: 5,
            candidates: 12,
            pruned: 7,
            recall_hits: 3,
        },
        TraceStepViewModel {
            depth: 1,
            beam_width: 4,
            candidates: 9,
            pruned: 5,
            recall_hits: 2,
        },
        TraceStepViewModel {
            depth: 2,
            beam_width: 3,
            candidates: 6,
            pruned: 3,
            recall_hits: 1,
        },
    ];

    let sp = |r: f32, g: f32, c: f32, m: f32| ScorePartsViewModel {
        relevance: r,
        goal: g,
        constraint: c,
        memory: m,
    };

    let hypotheses = vec![
        HypothesisViewModel {
            id: 0,
            parent: None,
            depth: 0,
            score: 0.92,
            score_parts: sp(0.90, 0.88, 0.95, 0.85),
            relations: vec![],
        },
        HypothesisViewModel {
            id: 1,
            parent: Some(0),
            depth: 1,
            score: 0.88,
            score_parts: sp(0.85, 0.82, 0.90, 0.80),
            relations: vec![],
        },
        HypothesisViewModel {
            id: 2,
            parent: Some(0),
            depth: 1,
            score: 0.85,
            score_parts: sp(0.82, 0.80, 0.88, 0.78),
            relations: vec![HypothesisRelationViewModel {
                to_id: 4,
                relation_type: "similar".to_string(),
            }],
        },
        HypothesisViewModel {
            id: 3,
            parent: Some(1),
            depth: 2,
            score: 0.81,
            score_parts: sp(0.78, 0.75, 0.85, 0.72),
            relations: vec![],
        },
        HypothesisViewModel {
            id: 4,
            parent: Some(1),
            depth: 2,
            score: 0.79,
            score_parts: sp(0.76, 0.73, 0.83, 0.70),
            relations: vec![],
        },
        HypothesisViewModel {
            id: 5,
            parent: Some(2),
            depth: 2,
            score: 0.80,
            score_parts: sp(0.77, 0.74, 0.84, 0.71),
            relations: vec![],
        },
    ];

    let memory = vec![
        MemoryCandidateViewModel {
            id: "mem-a1b2".to_string(),
            score: 0.91,
            source: "exact".to_string(),
            rank: 0,
            tags: vec!["web".to_string(), "rust".to_string()],
        },
        MemoryCandidateViewModel {
            id: "mem-c3d4".to_string(),
            score: 0.84,
            source: "cache".to_string(),
            rank: 1,
            tags: vec!["api".to_string()],
        },
        MemoryCandidateViewModel {
            id: "mem-e5f6".to_string(),
            score: 0.80,
            source: "cache".to_string(),
            rank: 2,
            tags: vec!["service".to_string(), "grpc".to_string()],
        },
        MemoryCandidateViewModel {
            id: "mem-g7h8".to_string(),
            score: 0.72,
            source: "index".to_string(),
            rank: 3,
            tags: vec![],
        },
    ];

    UiPayload {
        trace: TraceViewModel {
            request_id: "demo".to_string(),
            steps,
            stats: TraceStatsViewModel {
                total_nodes: 15,
                max_depth: 2,
                recall_hit_rate: 0.43,
                avg_branching: 2.5,
            },
        },
        hypotheses,
        memory,
        selected: Some(0),
    }
}
