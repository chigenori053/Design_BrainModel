use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Write, stdin, stdout};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};
use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_space_phase14::stable_v03::InMemoryEngine;
use runtime_core::{CoreRuntime, RuntimeExecutionResult};
use serde::Serialize;
use serde_json::json;

use crate::r#loop::run_loop;
use crate::renderer::{
    render_analysis_report, render_design_report, render_run_report, render_validation_report,
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
    Analyze(PathArgs),
    Design(PathArgs),
    Validate(PathArgs),
    Run(PathArgs),
    Wizard(WizardArgs),
    Repl(ReplArgs),
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
}

#[derive(clap::Args, Debug, Clone)]
pub struct PathArgs {
    pub path: PathBuf,
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

#[derive(Debug, Serialize)]
pub struct AnalysisReport {
    pub root: String,
    pub total_files: usize,
    pub source_files: usize,
    pub manifests: Vec<String>,
    pub languages: BTreeMap<String, usize>,
    pub top_level_entries: Vec<String>,
    pub architecture_hints: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DesignReport {
    pub root: String,
    pub inferred_style: String,
    pub components: Vec<String>,
    pub design_units: Vec<String>,
    pub recommended_next_steps: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub root: String,
    pub valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RunReport {
    pub root: String,
    pub mode: &'static str,
    pub selected_command: Option<String>,
    pub reason: String,
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
    CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    )
}

fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Some(Commands::Generate(args)) => execute_generate(&build_runtime(), args, "command"),
        Some(Commands::Analyze(args)) => execute_analyze(args),
        Some(Commands::Design(args)) => execute_design(args),
        Some(Commands::Validate(args)) => execute_validate(args),
        Some(Commands::Run(args)) => execute_run(args),
        Some(Commands::Wizard(args)) => wizard_mode(args),
        Some(Commands::Repl(args)) => repl_mode(args),
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
    let prompt = build_generate_prompt(&args);
    let context = runtime_core::ChatContext::default();
    let result = runtime
        .execute_from_text(&prompt, &context)
        .map_err(|err| format!("generate failed: {err:?}"))?;
    match result {
        RuntimeExecutionResult::Executed(result) => {
            let report = GenerateReport {
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
            };
            if report_json(args.json, &report)? {
                return Ok(());
            }
            println!("Generated project");
            println!("Command: {}", report.command);
            println!("Root: {}", report.project_root);
            println!("Manifest: {}", report.manifest_path);
            println!("Files:");
            for file in &report.files {
                println!(" - {file}");
            }
            Ok(())
        }
        RuntimeExecutionResult::Clarification(clarification) => Err(format!(
            "generate requires more input: {}",
            clarification.message
        )),
    }
}

fn execute_analyze(args: PathArgs) -> Result<(), String> {
    let report = analyze_path(&args.path)?;
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_analysis_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_design(args: PathArgs) -> Result<(), String> {
    let analysis = analyze_path(&args.path)?;
    let report = design_from_analysis(&analysis);
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_design_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_validate(args: PathArgs) -> Result<(), String> {
    let analysis = analyze_path(&args.path)?;
    let report = validate_from_analysis(&analysis);
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_validation_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
}

fn execute_run(args: PathArgs) -> Result<(), String> {
    let analysis = analyze_path(&args.path)?;
    let report = simulate_run(&analysis);
    if report_json(args.json, &report)? {
        return Ok(());
    }
    render_run_report(&mut io::stdout().lock(), &report).map_err(|err| err.to_string())
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
    };
    writeln!(writer, "Executing: {}", generate_command_string(&generate))
        .map_err(|err| err.to_string())?;
    execute_generate(&runtime, generate, "wizard")
}

fn repl_mode(args: ReplArgs) -> Result<(), String> {
    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    writeln!(writer, "REPL Mode").map_err(|err| err.to_string())?;
    writer.flush().map_err(|err| err.to_string())?;

    if args.json {
        run_repl_commands(&mut reader, &mut writer, true)
    } else {
        run_repl_commands(&mut reader, &mut writer, false)
    }
}

fn run_repl_commands<R, W>(reader: &mut R, writer: &mut W, json: bool) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    loop {
        write!(writer, "> ").map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;

        let mut line = String::new();
        let bytes = reader.read_line(&mut line).map_err(|err| err.to_string())?;
        if bytes == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, "exit" | "quit" | "/exit" | "/quit") {
            break;
        }
        if trimmed == "wizard" {
            return Err("wizard is not available from repl; use `cli wizard`".to_string());
        }

        let mut argv = vec!["cli".to_string()];
        argv.extend(trimmed.split_whitespace().map(str::to_string));
        if json && !argv.iter().any(|arg| arg == "--json") {
            argv.push("--json".to_string());
        }
        let cli = Cli::try_parse_from(argv).map_err(|err| err.to_string())?;
        match cli.command {
            Some(Commands::Repl(_)) | Some(Commands::Wizard(_)) | None => {
                return Err("unsupported repl command".to_string());
            }
            Some(command) => dispatch(Cli {
                command: Some(command),
            })?,
        }
    }
    Ok(())
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

fn analyze_path(path: &Path) -> Result<AnalysisReport, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("path is not a directory: {}", path.display()));
    }

    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    let mut manifests = Vec::new();
    let mut languages = BTreeMap::new();
    for file in &files {
        if let Some(name) = file.file_name().and_then(|name| name.to_str()) {
            if matches!(name, "Cargo.toml" | "pyproject.toml" | "package.json") {
                manifests.push(relativize(path, file));
            }
        }
        if let Some(language) = language_for_path(file) {
            *languages.entry(language.to_string()).or_insert(0) += 1;
        }
    }

    let top_level_entries = fs::read_dir(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();

    let mut architecture_hints = BTreeSet::new();
    if manifests.iter().any(|path| path.ends_with("Cargo.toml")) {
        architecture_hints.insert("rust-project".to_string());
    }
    if files
        .iter()
        .any(|path| path.to_string_lossy().contains("/src/"))
    {
        architecture_hints.insert("layered-source-layout".to_string());
    }
    if files
        .iter()
        .any(|path| path.to_string_lossy().contains("/tests/"))
    {
        architecture_hints.insert("has-tests".to_string());
    }
    if top_level_entries.iter().any(|name| name == "crates") {
        architecture_hints.insert("workspace-layout".to_string());
    }

    Ok(AnalysisReport {
        root: path.display().to_string(),
        total_files: files.len(),
        source_files: languages.values().sum(),
        manifests,
        languages,
        top_level_entries,
        architecture_hints: architecture_hints.into_iter().collect(),
    })
}

fn design_from_analysis(analysis: &AnalysisReport) -> DesignReport {
    let inferred_style = if analysis
        .architecture_hints
        .iter()
        .any(|hint| hint == "workspace-layout")
    {
        "workspace"
    } else if analysis.languages.contains_key("rust") {
        "service"
    } else if analysis.languages.contains_key("python") {
        "application"
    } else {
        "generic"
    };

    let mut components = Vec::new();
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "src")
    {
        components.push("src".to_string());
    }
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "tests")
    {
        components.push("tests".to_string());
    }
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "crates")
    {
        components.push("crates".to_string());
    }
    if components.is_empty() {
        components.push("root".to_string());
    }

    let mut design_units = analysis
        .manifests
        .iter()
        .map(|manifest| format!("manifest:{manifest}"))
        .collect::<Vec<_>>();
    if design_units.is_empty() {
        design_units.push("source-scan".to_string());
    }

    DesignReport {
        root: analysis.root.clone(),
        inferred_style: inferred_style.to_string(),
        components,
        design_units,
        recommended_next_steps: vec![
            "cli analyze <path>".to_string(),
            "cli validate <path> --json".to_string(),
        ],
    }
}

fn validate_from_analysis(analysis: &AnalysisReport) -> ValidationReport {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    if analysis.source_files == 0 {
        issues.push("no source files detected".to_string());
    }
    if analysis.manifests.is_empty() {
        warnings.push("no manifest file detected".to_string());
    }
    if !analysis
        .architecture_hints
        .iter()
        .any(|hint| hint == "has-tests")
    {
        warnings.push("test directory not detected".to_string());
    }

    ValidationReport {
        root: analysis.root.clone(),
        valid: issues.is_empty(),
        issues,
        warnings,
    }
}

fn simulate_run(analysis: &AnalysisReport) -> RunReport {
    if analysis
        .manifests
        .iter()
        .any(|manifest| manifest.ends_with("Cargo.toml"))
    {
        return RunReport {
            root: analysis.root.clone(),
            mode: "simulation",
            selected_command: Some("cargo run".to_string()),
            reason: "Rust manifest detected".to_string(),
        };
    }
    if analysis
        .manifests
        .iter()
        .any(|manifest| manifest.ends_with("pyproject.toml"))
    {
        return RunReport {
            root: analysis.root.clone(),
            mode: "simulation",
            selected_command: Some("python -m app".to_string()),
            reason: "Python manifest detected".to_string(),
        };
    }
    if analysis
        .manifests
        .iter()
        .any(|manifest| manifest.ends_with("package.json"))
    {
        return RunReport {
            root: analysis.root.clone(),
            mode: "simulation",
            selected_command: Some("npm run start".to_string()),
            reason: "Node manifest detected".to_string(),
        };
    }

    RunReport {
        root: analysis.root.clone(),
        mode: "simulation",
        selected_command: None,
        reason: "No known runtime manifest detected".to_string(),
    }
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read dir entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), ".git" | "target" | "node_modules") {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some("rust"),
        Some("py") => Some("python"),
        Some("ts" | "tsx" | "js" | "jsx") => Some("typescript"),
        Some("go") => Some("go"),
        _ => None,
    }
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
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
