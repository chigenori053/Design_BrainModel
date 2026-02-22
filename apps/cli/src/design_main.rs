use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use design_reasoning::{
    LanguageEngine, LanguageStateV2, TemplateId,
};
use hybrid_vm::{
    ConceptUnitV2, HybridVM, MeaningLayerSnapshotV2, SemanticError, SemanticUnitL1V2,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CLI_VERSION: &str = "1.0";

const EXIT_OK: i32 = 0;
const EXIT_SEMANTIC: i32 = 1;
const EXIT_IO: i32 = 2;
const EXIT_INVALID_COMMAND: i32 = 3;
const EXIT_SESSION_NOT_FOUND: i32 = 4;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut opts = GlobalOptions::default();
    
    // Preliminary parse for global options to handle --json in errors
    let mut cmd_args = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => opts.json = true,
            "--verbose" => opts.verbose = true,
            "--quiet" => opts.quiet = true,
            "--no-color" => opts.no_color = true,
            "--session" if i + 1 < args.len() => {
                opts.session = args[i+1].clone();
                i += 1;
            },
            "--store" if i + 1 < args.len() => {
                opts.store = PathBuf::from(&args[i+1]);
                i += 1;
            },
            arg if arg.starts_with('-') => {}, // skip unknown global opts here
            arg => cmd_args.push(arg.to_string()),
        }
        i += 1;
    }

    let result = parse_command(&cmd_args).and_then(|cmd| {
        let command_name = cmd.name();
        dispatch_command(opts.clone(), cmd).map(|data| (command_name, data))
    });

    match result {
        Ok((name, output)) => {
            if opts.quiet && !opts.json {
                return;
            }
            print_success(opts.json, name, output);
            std::process::exit(EXIT_OK);
        }
        Err(err) => {
            print_error(opts.json, err);
        }
    }
}

#[derive(Clone, Debug)]
struct GlobalOptions {
    json: bool,
    verbose: bool,
    quiet: bool,
    session: String,
    no_color: bool,
    store: PathBuf,
}

impl Default for GlobalOptions {
    fn default() -> Self {
        let env_store = std::env::var("DESIGN_STORE_DIR").ok();
        Self {
            json: false,
            verbose: false,
            quiet: false,
            session: "default".to_string(),
            no_color: false,
            store: env_store
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".design_store")),
        }
    }
}

#[derive(Clone, Debug)]
enum Command {
    Analyze { input: String },
    Explain,
    Snapshot,
    Diff,
    Rebuild,
    Export { path: PathBuf },
    Import { path: PathBuf },
    SessionList,
    SessionSave,
    SessionLoad { id: String },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Analyze { .. } => "analyze",
            Command::Explain => "explain",
            Command::Snapshot => "snapshot",
            Command::Diff => "diff",
            Command::Rebuild => "rebuild",
            Command::Export { .. } => "export",
            Command::Import { .. } => "import",
            Command::SessionList => "session list",
            Command::SessionSave => "session save",
            Command::SessionLoad { .. } => "session load",
        }
    }
}

#[derive(Debug)]
struct CliError {
    code: i32,
    command: String,
    kind: String,
    message: String,
}

impl CliError {
    fn semantic(cmd: &str, err: SemanticError) -> Self {
        Self {
            code: EXIT_SEMANTIC,
            command: cmd.to_string(),
            kind: "SemanticError".to_string(),
            message: format!("{err:?}"),
        }
    }

    fn io(cmd: &str, err: std::io::Error) -> Self {
        Self {
            code: EXIT_IO,
            command: cmd.to_string(),
            kind: "IoError".to_string(),
            message: err.to_string(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_INVALID_COMMAND,
            command: "unknown".to_string(),
            kind: "InvalidCommand".to_string(),
            message: message.into(),
        }
    }

    fn session_missing(cmd: &str, id: &str) -> Self {
        Self {
            code: EXIT_SESSION_NOT_FOUND,
            command: cmd.to_string(),
            kind: "SessionNotFound".to_string(),
            message: format!("session not found: {id}"),
        }
    }
}

fn parse_command(args: &[String]) -> Result<Command, CliError> {
    let cmd = args.first().ok_or_else(|| CliError::invalid(help_text()))?;
    
    match cmd.as_str() {
        "analyze" => {
            let input = args.get(1).ok_or_else(|| CliError::invalid("analyze requires text or file path"))?;
            Ok(Command::Analyze { input: input.clone() })
        }
        "explain" => Ok(Command::Explain),
        "snapshot" => Ok(Command::Snapshot),
        "diff" => Ok(Command::Diff),
        "rebuild" => Ok(Command::Rebuild),
        "export" => {
            let path = args.get(1).ok_or_else(|| CliError::invalid("export requires output path"))?;
            Ok(Command::Export { path: PathBuf::from(path) })
        }
        "import" => {
            let path = args.get(1).ok_or_else(|| CliError::invalid("import requires input path"))?;
            Ok(Command::Import { path: PathBuf::from(path) })
        }
        "session" => {
            match args.get(1).map(|s| s.as_str()) {
                Some("list") => Ok(Command::SessionList),
                Some("save") => Ok(Command::SessionSave),
                Some("load") => {
                    let id = args.get(2).ok_or_else(|| CliError::invalid("session load requires id"))?;
                    Ok(Command::SessionLoad { id: id.clone() })
                }
                _ => Err(CliError::invalid("session supports: list|save|load <id>")),
            }
        }
        _ => Err(CliError::invalid(format!("unknown command: {cmd}"))),
    }
}

fn dispatch_command(opts: GlobalOptions, cmd: Command) -> Result<Value, CliError> {
    ensure_store_dirs(&opts).map_err(|e| CliError::io(cmd.name(), e))?;

    match cmd {
        Command::Analyze { input } => cmd_analyze(opts, input),
        Command::Explain => cmd_explain(opts),
        Command::Snapshot => cmd_snapshot(opts),
        Command::Diff => cmd_diff(opts),
        Command::Rebuild => cmd_rebuild(opts),
        Command::Export { path } => cmd_export(opts, path),
        Command::Import { path } => cmd_import(opts, path),
        Command::SessionList => cmd_session_list(opts),
        Command::SessionSave => cmd_session_save(opts),
        Command::SessionLoad { id } => cmd_session_load(opts, id),
    }
}

fn cmd_analyze(opts: GlobalOptions, input: String) -> Result<Value, CliError> {
    let text = resolve_input_text(&input).map_err(|e| CliError::io("analyze", e))?;
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("analyze", e))?;
    vm.analyze_text(&text).map_err(|e| CliError::semantic("analyze", e))?;
    vm.rebuild_l2_from_l1_v2().map_err(|e| CliError::semantic("analyze", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("analyze", e))?;
    store_snapshot_history(&opts, &snapshot).map_err(|e| CliError::io("analyze", e))?;

    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("analyze", e))?;
    let l2_units = vm.project_phase_a_v2().map_err(|e| CliError::semantic("analyze", e))?;
    
    let stability = mean_stability(&l2_units);
    let ambiguity = mean_ambiguity(&l1_units);

    Ok(json!({
        "l1_count": l1_units.len(),
        "l2_count": l2_units.len(),
        "stability_score": round6(stability),
        "ambiguity_score": round6(ambiguity),
        "snapshot": {
            "l1_hash": snapshot.l1_hash.to_string(),
            "l2_hash": snapshot.l2_hash.to_string(),
            "version": snapshot.version
        }
    }))
}

fn cmd_explain(opts: GlobalOptions) -> Result<Value, CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("explain", e))?;
    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("explain", e))?;
    let l2_units = vm.project_phase_a_v2().map_err(|e| CliError::semantic("explain", e))?;

    let objective = l1_units
        .iter()
        .find_map(|u| u.objective.clone());
    let requirement_count = l2_units
        .iter()
        .map(|c| c.derived_requirements.len())
        .sum::<usize>();
    let stability = mean_stability(&l2_units);
    let ambiguity = mean_ambiguity(&l1_units);

    let state = LanguageStateV2 {
        selected_objective: objective.clone(),
        requirement_count,
        stability_score: stability,
        ambiguity_score: ambiguity,
    };
    let language = LanguageEngine::new();
    let h = language.build_h_state(&state);
    let template = language.select_template(&h).unwrap_or(TemplateId::Fallback);

    Ok(json!({
        "objective": objective,
        "requirement_count": requirement_count,
        "stability_label": stability_label(stability),
        "ambiguity_label": ambiguity_label(ambiguity),
        "template_id": template
    }))
}

fn cmd_snapshot(opts: GlobalOptions) -> Result<Value, CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("snapshot", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("snapshot", e))?;
    Ok(json!({
        "l1_hash": snapshot.l1_hash.to_string(),
        "l2_hash": snapshot.l2_hash.to_string(),
        "version": snapshot.version,
        "timestamp_ms": snapshot.timestamp_ms
    }))
}

fn cmd_diff(opts: GlobalOptions) -> Result<Value, CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("diff", e))?;
    let current = vm.snapshot_v2().map_err(|e| CliError::semantic("diff", e))?;
    let previous = load_previous_snapshot(&opts, &opts.session).ok_or_else(|| CliError::session_missing("diff", &opts.session))?;
    let diff = vm.compare_snapshots_v2(&previous, &current);

    Ok(json!({
        "l1_changed": diff.l1_changed,
        "l2_changed": diff.l2_changed,
        "version_changed": diff.version_changed
    }))
}

fn cmd_rebuild(opts: GlobalOptions) -> Result<Value, CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("rebuild", e))?;
    let concepts = vm.rebuild_l2_from_l1_v2().map_err(|e| CliError::semantic("rebuild", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("rebuild", e))?;
    store_snapshot_history(&opts, &snapshot).map_err(|e| CliError::io("rebuild", e))?;

    Ok(json!({
        "l2_count": concepts.len(),
        "snapshot": {
            "l1_hash": snapshot.l1_hash.to_string(),
            "l2_hash": snapshot.l2_hash.to_string(),
            "version": snapshot.version
        }
    }))
}

fn cmd_export(opts: GlobalOptions, out_path: PathBuf) -> Result<Value, CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("export", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("export", e))?;
    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("export", e))?;
    let l2_units = vm.project_phase_a_v2().map_err( |e| CliError::semantic("export", e))?;
    
    let now = now_ms();
    let export = SessionJsonV1 {
        schema_version: "1.0".to_string(),
        snapshot_version: snapshot.version,
        id: opts.session.clone(),
        created_at: now,
        last_modified: now,
        l1_units,
        l2_units,
        snapshot: SnapshotBrief {
            l1_hash: snapshot.l1_hash.to_string(),
            l2_hash: snapshot.l2_hash.to_string(),
            version: snapshot.version,
        },
    };
    
    let raw = serde_json::to_string_pretty(&export).map_err(|e| CliError::invalid(format!("serialize failed: {e}")))?;
    fs::write(&out_path, raw).map_err(|e| CliError::io("export", e))?;

    Ok(json!({ "path": out_path.to_string_lossy() }))
}

fn cmd_import(opts: GlobalOptions, in_path: PathBuf) -> Result<Value, CliError> {
    let raw = fs::read_to_string(&in_path).map_err(|e| CliError::io("import", e))?;
    let imported: SessionJsonV1 = serde_json::from_str(&raw).map_err(|e| CliError::invalid(format!("invalid session json: {e}")))?;

    let target = session_file_path(&opts, &opts.session);
    fs::write(&target, raw).map_err(|e| CliError::io("import", e))?;
    write_active_session(&opts, &opts.session).map_err(|e| CliError::io("import", e))?;

    Ok(json!({
        "session_id": opts.session,
        "path": target.to_string_lossy(),
        "snapshot": imported.snapshot
    }))
}

fn cmd_session_list(opts: GlobalOptions) -> Result<Value, CliError> {
    let mut sessions = Vec::new();
    if let Ok(entries) = fs::read_dir(&opts.store) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                if name.starts_with("session_") && name.ends_with(".json") {
                    let id = name.trim_start_matches("session_").trim_end_matches(".json").to_string();
                    let metadata = fs::metadata(&path).ok();
                    let last_modified = metadata.and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    
                    sessions.push(json!({
                        "id": id,
                        "created_at": last_modified, // fallback
                        "last_modified": last_modified
                    }));
                }
            }
        }
    }
    Ok(json!({ "sessions": sessions }))
}

fn cmd_session_save(opts: GlobalOptions) -> Result<Value, CliError> {
    let out = session_file_path(&opts, &opts.session);
    let vm = init_vm(&opts).map_err(|e| CliError::io("session save", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("session save", e))?;
    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("session save", e))?;
    let l2_units = vm.project_phase_a_v2().map_err(|e| CliError::semantic("session save", e))?;

    let now = now_ms();
    let export = SessionJsonV1 {
        schema_version: "1.0".to_string(),
        snapshot_version: snapshot.version,
        id: opts.session.clone(),
        created_at: now,
        last_modified: now,
        l1_units,
        l2_units,
        snapshot: SnapshotBrief {
            l1_hash: snapshot.l1_hash.to_string(),
            l2_hash: snapshot.l2_hash.to_string(),
            version: snapshot.version,
        },
    };

    let raw = serde_json::to_string_pretty(&export).map_err(|e| CliError::invalid(format!("serialize failed: {e}")))?;
    fs::write(&out, raw).map_err(|e| CliError::io("session save", e))?;
    write_active_session(&opts, &opts.session).map_err(|e| CliError::io("session save", e))?;

    Ok(json!({
        "session_id": opts.session,
        "path": out.to_string_lossy(),
        "snapshot": export.snapshot
    }))
}

fn cmd_session_load(mut opts: GlobalOptions, id: String) -> Result<Value, CliError> {
    let path = session_file_path(&opts, &id);
    if !path.exists() {
        return Err(CliError::session_missing("session load", &id));
    }
    let raw = fs::read_to_string(&path).map_err(|e| CliError::io("session load", e))?;
    let imported: SessionJsonV1 = serde_json::from_str(&raw).map_err(|e| CliError::invalid(format!("invalid session: {e}")))?;
    
    opts.session = id.clone();
    write_active_session(&opts, &id).map_err(|e| CliError::io("session load", e))?;

    Ok(json!({
        "session_id": id,
        "snapshot": imported.snapshot
    }))
}

// Helpers

fn resolve_input_text(input: &str) -> std::io::Result<String> {
    let path = Path::new(input);
    if path.exists() && path.is_file() {
        fs::read_to_string(path)
    } else {
        Ok(input.to_string())
    }
}

fn init_vm(opts: &GlobalOptions) -> std::io::Result<HybridVM> {
    let vm_store = opts.store.join(format!("session_{}", opts.session)).join("vm");
    fs::create_dir_all(&vm_store)?;
    HybridVM::for_cli_storage(vm_store)
}

fn ensure_store_dirs(opts: &GlobalOptions) -> std::io::Result<()> {
    fs::create_dir_all(&opts.store)?;
    let vm_store = opts.store.join(format!("session_{}", opts.session)).join("vm");
    fs::create_dir_all(vm_store)
}

fn session_file_path(opts: &GlobalOptions, session: &str) -> PathBuf {
    opts.store.join(format!("session_{session}.json"))
}

fn write_active_session(opts: &GlobalOptions, session: &str) -> std::io::Result<()> {
    fs::write(opts.store.join("active_session"), session)
}

fn store_snapshot_history(opts: &GlobalOptions, snapshot: &MeaningLayerSnapshotV2) -> std::io::Result<()> {
    let current_path = opts.store.join(format!("snapshot_current_{}.json", opts.session));
    let prev_path = opts.store.join(format!("snapshot_prev_{}.json", opts.session));
    if current_path.exists() {
        let _ = fs::copy(&current_path, &prev_path);
    }
    let raw = serde_json::to_string_pretty(snapshot).unwrap();
    fs::write(current_path, raw)
}

fn load_previous_snapshot(opts: &GlobalOptions, session: &str) -> Option<MeaningLayerSnapshotV2> {
    let path = opts.store.join(format!("snapshot_prev_{session}.json"));
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn print_success(json_mode: bool, command: &str, data: Value) {
    if json_mode {
        let response = json!({
            "status": "ok",
            "version": CLI_VERSION,
            "command": command,
            "data": data,
            "error": null
        });
        println!("{}", serde_json::to_string_pretty(&response).unwrap());
    } else {
        // Human readable output can be custom per command if needed
        println!("{data}"); 
    }
}

fn print_error(json_mode: bool, err: CliError) {
    if json_mode {
        let response = json!({
            "status": "error",
            "version": CLI_VERSION,
            "command": err.command,
            "data": null,
            "error": {
                "code": err.code,
                "type": err.kind,
                "message": err.message
            }
        });
        eprintln!("{}", serde_json::to_string_pretty(&response).unwrap());
    } else {
        eprintln!("Error: {}", err.message);
    }
    std::process::exit(err.code);
}

fn mean_stability(l2_units: &[ConceptUnitV2]) -> f64 {
    if l2_units.is_empty() { 0.0 }
    else { l2_units.iter().map(|u| u.stability_score).sum::<f64>() / l2_units.len() as f64 }
}

fn mean_ambiguity(l1_units: &[SemanticUnitL1V2]) -> f64 {
    if l1_units.is_empty() { 1.0 }
    else { l1_units.iter().map(|u| u.ambiguity_score).sum::<f64>() / l1_units.len() as f64 }
}

fn stability_label(score: f64) -> &'static str {
    if score > 0.85 { "安定" } else if score >= 0.6 { "概ね安定" } else { "不安定" }
}

fn ambiguity_label(score: f64) -> &'static str {
    if score > 0.7 { "不明確" } else if score >= 0.4 { "部分的に不明確" } else { "明確" }
}

fn round6(v: f64) -> f64 { (v * 1_000_000.0).round() / 1_000_000.0 }

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn help_text() -> String {
    "design [OPTIONS] <COMMAND>\nCommands: analyze, explain, snapshot, diff, rebuild, export, import, session list/save/load".to_string()
}

#[derive(Serialize, Deserialize)]
struct SessionJsonV1 {
    schema_version: String,
    snapshot_version: u16,
    id: String,
    created_at: u64,
    last_modified: u64,
    l1_units: Vec<SemanticUnitL1V2>,
    l2_units: Vec<ConceptUnitV2>,
    snapshot: SnapshotBrief,
}

#[derive(Serialize, Deserialize)]
struct SnapshotBrief {
    l1_hash: String,
    l2_hash: String,
    version: u16,
}
