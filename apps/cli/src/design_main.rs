use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use design_reasoning::{
    LanguageEngine, LanguageStateV2, TemplateId,
};
use hybrid_vm::{
    ArtifactFormat, ConceptUnitV2, FeedbackAction, FeedbackEntry, HybridVM, InfoCategory, L1Id,
    MeaningLayerSnapshotV2, SemanticError, SemanticUnitL1V2,
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
            arg if arg.starts_with('-') => cmd_args.push(arg.to_string()),
            arg => cmd_args.push(arg.to_string()),
        }
        i += 1;
    }

    let result = parse_command(&cmd_args).and_then(|cmd| {
        let command_name = cmd.name();
        dispatch_command(opts.clone(), cmd).map(|data| (command_name, data))
    });

    match result {
        Ok((name, (output, human))) => {
            if opts.quiet && !opts.json {
                return;
            }
            print_success(opts.json, name, output, &human);
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
    Adopt { draft_id: String },
    Reject { draft_id: String },
    Search { card_id: String, query: Option<String>, allow: bool },
    Refine { card_id: String, text: String },
    Clear,
    Snapshot,
    Diff,
    Rebuild,
    Simulate { target: u128, delta: f32, remove: bool },
    Export { path: PathBuf, format: Option<ArtifactFormat> },
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
            Command::Adopt { .. } => "adopt",
            Command::Reject { .. } => "reject",
            Command::Search { .. } => "search",
            Command::Refine { .. } => "refine",
            Command::Clear => "clear",
            Command::Snapshot => "snapshot",
            Command::Diff => "diff",
            Command::Rebuild => "rebuild",
            Command::Simulate { .. } => "simulate",
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
        "adopt" => {
            let mut draft_id: Option<String> = None;
            let mut i = 1usize;
            while i < args.len() {
                if args[i] == "--draft-id" && i + 1 < args.len() {
                    draft_id = Some(args[i + 1].clone());
                    i += 1;
                }
                i += 1;
            }
            let draft_id = draft_id.ok_or_else(|| CliError::invalid("adopt requires --draft-id <ID>"))?;
            Ok(Command::Adopt { draft_id })
        }
        "reject" => {
            let mut draft_id: Option<String> = None;
            let mut i = 1usize;
            while i < args.len() {
                if args[i] == "--draft-id" && i + 1 < args.len() {
                    draft_id = Some(args[i + 1].clone());
                    i += 1;
                }
                i += 1;
            }
            let draft_id = draft_id.ok_or_else(|| CliError::invalid("reject requires --draft-id <ID>"))?;
            Ok(Command::Reject { draft_id })
        }
        "search" => {
            let mut card_id: Option<String> = None;
            let mut query: Option<String> = None;
            let mut allow = false;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--card" if i + 1 < args.len() => {
                        card_id = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--query" if i + 1 < args.len() => {
                        query = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--allow" => allow = true,
                    _ => {}
                }
                i += 1;
            }
            let card_id = card_id.ok_or_else(|| CliError::invalid("search requires --card <CARD_ID>"))?;
            Ok(Command::Search { card_id, query, allow })
        }
        "refine" => {
            let mut card_id: Option<String> = None;
            let mut text: Option<String> = None;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--card" if i + 1 < args.len() => {
                        card_id = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--text" if i + 1 < args.len() => {
                        text = Some(args[i + 1].clone());
                        i += 1;
                    }
                    _ => {}
                }
                i += 1;
            }
            let card_id = card_id.ok_or_else(|| CliError::invalid("refine requires --card <CARD_ID>"))?;
            let text = text.ok_or_else(|| CliError::invalid("refine requires --text <DETAIL_TEXT>"))?;
            Ok(Command::Refine { card_id, text })
        }
        "clear" => Ok(Command::Clear),
        "snapshot" => Ok(Command::Snapshot),
        "diff" => Ok(Command::Diff),
        "rebuild" => Ok(Command::Rebuild),
        "simulate" => {
            let mut target: Option<u128> = None;
            let mut delta: f32 = 0.0;
            let mut remove = false;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--target" if i + 1 < args.len() => {
                        target = args[i + 1].parse::<u128>().ok();
                        i += 1;
                    }
                    "--delta" if i + 1 < args.len() => {
                        delta = args[i + 1]
                            .parse::<f32>()
                            .map_err(|_| CliError::invalid("simulate --delta requires float value"))?;
                        i += 1;
                    }
                    "--remove" => {
                        remove = true;
                    }
                    _ => {}
                }
                i += 1;
            }
            let target = target.ok_or_else(|| CliError::invalid("simulate requires --target <L1_ID>"))?;
            Ok(Command::Simulate { target, delta, remove })
        }
        "export" => {
            let mut format: Option<ArtifactFormat> = None;
            let mut out: Option<PathBuf> = None;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--format" if i + 1 < args.len() => {
                        format = Some(parse_artifact_format(&args[i + 1])?);
                        i += 1;
                    }
                    "--out" if i + 1 < args.len() => {
                        out = Some(PathBuf::from(&args[i + 1]));
                        i += 1;
                    }
                    s if !s.starts_with('-') => {
                        if out.is_none() {
                            out = Some(PathBuf::from(s));
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            let path = out.ok_or_else(|| {
                CliError::invalid("export requires output path or --out <dir>")
            })?;
            Ok(Command::Export { path, format })
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

fn dispatch_command(opts: GlobalOptions, cmd: Command) -> Result<(Value, String), CliError> {
    ensure_store_dirs(&opts).map_err(|e| CliError::io(cmd.name(), e))?;

    match cmd {
        Command::Analyze { input } => cmd_analyze(opts, input),
        Command::Explain => cmd_explain(opts),
        Command::Adopt { draft_id } => cmd_adopt(opts, draft_id),
        Command::Reject { draft_id } => cmd_reject(opts, draft_id),
        Command::Search { card_id, query, allow } => cmd_search(opts, card_id, query, allow),
        Command::Refine { card_id, text } => cmd_refine(opts, card_id, text),
        Command::Clear => cmd_clear(opts),
        Command::Snapshot => cmd_snapshot(opts),
        Command::Diff => cmd_diff(opts),
        Command::Rebuild => cmd_rebuild(opts),
        Command::Simulate { target, delta, remove } => cmd_simulate(opts, target, delta, remove),
        Command::Export { path, format } => cmd_export(opts, path, format),
        Command::Import { path } => cmd_import(opts, path),
        Command::SessionList => cmd_session_list(opts),
        Command::SessionSave => cmd_session_save(opts),
        Command::SessionLoad { id } => cmd_session_load(opts, id),
    }
}

fn cmd_analyze(opts: GlobalOptions, input: String) -> Result<(Value, String), CliError> {
    let text = resolve_input_text(&input).map_err(|e| CliError::io("analyze", e))?;
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("analyze", e))?;
    vm.analyze_text(&text).map_err(|e| CliError::semantic("analyze", e))?;
    vm.rebuild_l2_from_l1_v2().map_err(|e| CliError::semantic("analyze", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("analyze", e))?;
    store_snapshot_history(&opts, &snapshot).map_err(|e| CliError::io("analyze", e))?;

    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("analyze", e))?;
    let l2_units = vm.project_phase_a_v2().map_err(|e| CliError::semantic("analyze", e))?;
    let graph = build_graph_json(&vm, &l1_units, &l2_units);
    
    let stability = mean_stability(&l2_units);
    let ambiguity = mean_ambiguity(&l1_units);

    let data = json!({
        "l1_count": l1_units.len(),
        "l2_count": l2_units.len(),
        "stability_score": round6(stability),
        "ambiguity_score": round6(ambiguity),
        "graph": graph,
        "snapshot": {
            "l1_hash": snapshot.l1_hash.to_string(),
            "l2_hash": snapshot.l2_hash.to_string(),
            "version": snapshot.version
        }
    });

    let human = format!(
        "分析完了: L1ユニット {} 件 / L2コンセプト {} 件 を抽出しました。\n現在の設計安定度: {} (score: {:.2})\n曖昧性: {} (score: {:.2})",
        l1_units.len(),
        l2_units.len(),
        stability_label(stability),
        stability,
        ambiguity_label(ambiguity),
        ambiguity
    );

    Ok((data, human))
}

fn cmd_explain(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("explain", e))?;
    let l1_units = vm.all_l1_units_v2().map_err(|e| CliError::semantic("explain", e))?;
    let l2_units = vm.project_phase_a_v2().map_err(|e| CliError::semantic("explain", e))?;
    let remediations = detect_remediations(&l1_units, &l2_units);
    let drafts = vm.generate_drafts().map_err(|e| CliError::semantic("explain", e))?;
    let sorted_drafts = vm.pareto_optimize_drafts(drafts);
    let provocation = sorted_drafts
        .first()
        .map(|d| d.prompt.clone())
        .unwrap_or_else(|| "現時点で追加の設計誘発案はありません。".to_string());
    let missing_info = vm
        .extract_missing_information()
        .map_err(|e| CliError::semantic("explain", e))?
        .into_iter()
        .map(|m| MissingInfoItem {
            target_id: m.target_id.map(|id| format!("L1-{}", id.0)),
            category: info_category_name(&m.category).to_string(),
            prompt: m.prompt,
            importance: round6(m.importance),
        })
        .collect::<Vec<_>>();

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

    let mut vm_mut = init_vm(&opts).map_err(|e| CliError::io("explain", e))?;
    let cards = vm_mut.get_design_cards().unwrap_or_default();

    let data = json!({
        "schema_version": "1.6",
        "optimization": "pareto_frontier",
        "objective": objective,
        "requirement_count": requirement_count,
        "stability_label": stability_label(stability),
        "ambiguity_label": ambiguity_label(ambiguity),
        "template_id": template,
        "remediations": remediations,
        "missing_info": missing_info,
        "drafts": sorted_drafts.iter().map(|d| {
            json!({
                "draft_id": d.draft_id,
                "stability_impact": round6(d.stability_impact),
                "prompt": d.prompt
            })
        }).collect::<Vec<_>>(),
        "provocation": provocation,
        "cards": cards,
    });

    let remediation_text = if remediations.is_empty() {
        "推奨される改善アクション: なし".to_string()
    } else {
        let mut lines = vec!["推奨される改善アクション:".to_string()];
        for item in &remediations {
            lines.push(format!(
                "- [{}] {} ({})",
                item.target_id, item.message, item.action_type
            ));
        }
        lines.join("\n")
    };

    let mut human = format!(
        "【設計状態の診断】\n設計目標: {}\n{}\n\n{}\n{}\n\n(補足: 構造安定性={}, 曖昧性={}, 派生要件数={}件)",
        objective.as_deref().unwrap_or("未指定"),
        template.as_description(),
        remediation_text,
        format_missing_info_human(&missing_info),
        stability_label(stability),
        ambiguity_label(ambiguity),
        requirement_count
    );

    if !cards.is_empty() {
        human.push_str("\n\n【デザインカード】");
        for card in cards {
            human.push_str(&format!("\n ■ {}: {}\n    概要: {}\n    状態: {:?}", card.id, card.title, card.overview, card.status));
            for detail in card.details {
                human.push_str(&format!("\n      - {}", detail));
            }
        }
    }

    Ok((data, human))
}

fn cmd_adopt(opts: GlobalOptions, draft_id: String) -> Result<(Value, String), CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("adopt", e))?;
    vm.commit_draft(&draft_id)
        .map_err(|e| CliError::semantic("adopt", e))?;
    vm.record_feedback(&draft_id, FeedbackAction::Adopt);
    vm.adjust_weights();
    write_session_state(&opts, &vm).map_err(|e| CliError::io("adopt", e))?;
    let l1_count = vm
        .all_l1_units_v2()
        .map_err(|e| CliError::semantic("adopt", e))?
        .len();
    let l2_units = vm
        .project_phase_a_v2()
        .map_err(|e| CliError::semantic("adopt", e))?;
    let stability = mean_stability(&l2_units);
    let data = json!({
        "draft_id": draft_id,
        "adopted": true,
        "l1_count": l1_count,
        "stability_score": round6(stability)
    });
    let human = format!("Draft '{}' を採用しました。L1={} / stability={:.3}", draft_id, l1_count, stability);
    Ok((data, human))
}

fn cmd_reject(opts: GlobalOptions, draft_id: String) -> Result<(Value, String), CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("reject", e))?;
    let exists = vm
        .generate_drafts()
        .map_err(|e| CliError::semantic("reject", e))?
        .iter()
        .any(|d| d.draft_id == draft_id);
    if !exists {
        return Err(CliError::invalid(format!("draft not found: {draft_id}")));
    }
    vm.record_feedback(&draft_id, FeedbackAction::Reject);
    vm.adjust_weights();
    write_session_state(&opts, &vm).map_err(|e| CliError::io("reject", e))?;
    let data = json!({
        "draft_id": draft_id,
        "rejected": true
    });
    let human = format!("Draft '{}' を棄却しました。次回提案の優先度に反映します。", draft_id);
    Ok((data, human))
}

fn cmd_search(
    opts: GlobalOptions,
    card_id: String,
    query: Option<String>,
    allow: bool,
) -> Result<(Value, String), CliError> {
    if !allow {
        return Err(CliError::invalid("search requires explicit permission via --allow"));
    }
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("search", e))?;
    let l2_id = parse_card_id(&card_id)?;
    let has_gap = vm
        .card_has_knowledge_gap(l2_id)
        .map_err(|e| CliError::semantic("search", e))?;
    if !has_gap {
        let data = json!({
            "card_id": card_id,
            "grounded": false,
            "reason": "knowledge gap not detected"
        });
        return Ok((data, "知識補完は不要と判定されました。".to_string()));
    }
    let q = query.unwrap_or_else(|| format!("card {}", card_id));
    let grounded = vm
        .run_grounding_search(l2_id, &q)
        .map_err(|e| CliError::semantic("search", e))?;
    write_session_state(&opts, &vm).map_err(|e| CliError::io("search", e))?;
    let data = json!({
        "card_id": card_id,
        "grounded": true,
        "query": q,
        "results": grounded
    });
    let human = format!("カード {} の知識補完を実行しました。{} 件反映。", card_id, data["results"].as_array().map(|a| a.len()).unwrap_or(0));
    Ok((data, human))
}

fn cmd_refine(opts: GlobalOptions, card_id: String, text: String) -> Result<(Value, String), CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("refine", e))?;
    let l2_id = parse_card_id(&card_id)?;
    vm.refine_l2_detail(l2_id, &text)
        .map_err(|e| CliError::semantic("refine", e))?;
    let l2_units = vm
        .project_phase_a_v2()
        .map_err(|e| CliError::semantic("refine", e))?;
    let stability = mean_stability(&l2_units);
    write_session_state(&opts, &vm).map_err(|e| CliError::io("refine", e))?;
    let data = json!({
        "card_id": card_id,
        "refined": true,
        "stability_score": round6(stability)
    });
    let human = format!("カード {} を更新しました。stability={:.3}", card_id, stability);
    Ok((data, human))
}

fn cmd_snapshot(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("snapshot", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("snapshot", e))?;
    let data = json!({
        "l1_hash": snapshot.l1_hash.to_string(),
        "l2_hash": snapshot.l2_hash.to_string(),
        "version": snapshot.version,
        "timestamp_ms": snapshot.timestamp_ms
    });
    let human = format!("Snapshot: L1={}, L2={}, Version={}", snapshot.l1_hash, snapshot.l2_hash, snapshot.version);
    Ok((data, human))
}

fn cmd_clear(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("clear", e))?;
    vm.clear_context().map_err(|e| CliError::semantic("clear", e))?;
    let data = json!({
        "session": opts.session,
        "cleared": true
    });
    let human = format!("セッション '{}' の文脈を初期化しました。", opts.session);
    Ok((data, human))
}

fn cmd_diff(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("diff", e))?;
    let current = vm.snapshot_v2().map_err(|e| CliError::semantic("diff", e))?;
    let previous = load_previous_snapshot(&opts, &opts.session).ok_or_else(|| CliError::session_missing("diff", &opts.session))?;
    let diff = vm.compare_snapshots_v2(&previous, &current);

    let data = json!({
        "l1_changed": diff.l1_changed,
        "l2_changed": diff.l2_changed,
        "version_changed": diff.version_changed
    });
    let human = format!(
        "差分分析結果:\n  L1変更: {}\n  L2変更: {}\n  バージョン変更: {}",
        diff.l1_changed, diff.l2_changed, diff.version_changed
    );
    Ok((data, human))
}

fn cmd_rebuild(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let mut vm = init_vm(&opts).map_err(|e| CliError::io("rebuild", e))?;
    let concepts = vm.rebuild_l2_from_l1_v2().map_err(|e| CliError::semantic("rebuild", e))?;
    let snapshot = vm.snapshot_v2().map_err(|e| CliError::semantic("rebuild", e))?;
    store_snapshot_history(&opts, &snapshot).map_err(|e| CliError::io("rebuild", e))?;

    let data = json!({
        "l2_count": concepts.len(),
        "snapshot": {
            "l1_hash": snapshot.l1_hash.to_string(),
            "l2_hash": snapshot.l2_hash.to_string(),
            "version": snapshot.version
        }
    });
    let human = format!("再構築完了: L2コンセプトを {} 件生成しました。", concepts.len());
    Ok((data, human))
}

fn cmd_simulate(opts: GlobalOptions, target: u128, delta: f32, remove: bool) -> Result<(Value, String), CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("simulate", e))?;
    let target_id = L1Id(target);
    let report = if remove {
        vm.simulate_removal(target_id)
    } else {
        vm.simulate_perturbation(target_id, delta)
    }
    .map_err(|e| CliError::semantic("simulate", e))?;
    let blast = vm.evaluate_blast_radius(&report);

    let impact_summary = json!({
        "stability_delta": round6(report.simulated_objectives.f_struct - report.original_objectives.f_struct),
        "risk_delta": round6(report.simulated_objectives.f_risk - report.original_objectives.f_risk),
        "field_delta": round6(report.simulated_objectives.f_field - report.original_objectives.f_field),
        "shape_delta": round6(report.simulated_objectives.f_shape - report.original_objectives.f_shape)
    });

    let affected_concepts = report
        .affected_concepts
        .iter()
        .map(|c| {
            json!({
                "id": format!("L2-{}", c.concept_id.0),
                "original_stability": round6(c.original_stability),
                "simulated_stability": round6(c.simulated_stability),
                "stability_change": round6(c.simulated_stability - c.original_stability)
            })
        })
        .collect::<Vec<_>>();

    let data = json!({
        "schema_version": "1.3",
        "target": format!("L1-{}", target),
        "impact_summary": impact_summary,
        "affected_concepts": affected_concepts,
        "blast_radius": {
            "coverage": round6(blast.coverage),
            "intensity": round6(blast.intensity),
            "structural_risk": round6(blast.structural_risk),
            "total_score": round6(blast.total_score)
        }
    });

    let human = format!(
        "Simulation complete: target=L1-{target}, affected={} concepts, blast_radius={:.3}",
        report.affected_concepts.len(),
        blast.total_score
    );
    Ok((data, human))
}

fn cmd_export(
    opts: GlobalOptions,
    out_path: PathBuf,
    format: Option<ArtifactFormat>,
) -> Result<(Value, String), CliError> {
    let vm = init_vm(&opts).map_err(|e| CliError::io("export", e))?;
    if let Some(fmt) = format {
        fs::create_dir_all(&out_path).map_err(|e| CliError::io("export", e))?;
        let artifacts = vm
            .generate_artifacts(fmt)
            .map_err(|e| CliError::semantic("export", e))?;
        let mut files = Vec::new();
        for artifact in artifacts {
            let full_path = out_path.join(&artifact.file_name);
            fs::write(&full_path, artifact.content).map_err(|e| CliError::io("export", e))?;
            files.push(full_path.to_string_lossy().to_string());
        }
        let format_name = artifact_format_name(fmt);
        let data = json!({
            "format": format_name,
            "output_dir": out_path.to_string_lossy(),
            "files": files
        });
        let human = format!(
            "成果物エクスポート完了: format={}, files={} ({})",
            format_name,
            data["files"].as_array().map(|a| a.len()).unwrap_or(0),
            out_path.display()
        );
        return Ok((data, human));
    }

    let export = build_session_json(&opts, &vm).map_err(|e| CliError::semantic("export", e))?;
    let raw = serde_json::to_string_pretty(&export)
        .map_err(|e| CliError::invalid(format!("serialize failed: {e}")))?;
    fs::write(&out_path, raw).map_err(|e| CliError::io("export", e))?;

    let data = json!({ "path": out_path.to_string_lossy() });
    let human = format!("エクスポート完了: {}", out_path.display());
    Ok((data, human))
}

fn cmd_import(opts: GlobalOptions, in_path: PathBuf) -> Result<(Value, String), CliError> {
    let raw = fs::read_to_string(&in_path).map_err(|e| CliError::io("import", e))?;
    let imported: SessionJsonV1 = serde_json::from_str(&raw).map_err(|e| CliError::invalid(format!("invalid session json: {e}")))?;

    let target = session_file_path(&opts, &opts.session);
    fs::write(&target, raw).map_err(|e| CliError::io("import", e))?;
    write_active_session(&opts, &opts.session).map_err(|e| CliError::io("import", e))?;

    let data = json!({
        "session_id": opts.session,
        "path": target.to_string_lossy(),
        "snapshot": imported.snapshot
    });
    let human = format!("インポート完了: セッション '{}' を読み込みました。", opts.session);
    Ok((data, human))
}

fn cmd_session_list(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let mut sessions = Vec::new();
    let mut human = String::from("利用可能なセッション一覧:\n");
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
                        "created_at": last_modified,
                        "last_modified": last_modified
                    }));
                    human.push_str(&format!("  - {}\n", id));
                }
            }
        }
    }
    Ok((json!({ "sessions": sessions }), human))
}

fn cmd_session_save(opts: GlobalOptions) -> Result<(Value, String), CliError> {
    let out = session_file_path(&opts, &opts.session);
    let vm = init_vm(&opts).map_err(|e| CliError::io("session save", e))?;
    let export = build_session_json(&opts, &vm).map_err(|e| CliError::semantic("session save", e))?;

    let raw = serde_json::to_string_pretty(&export).map_err(|e| CliError::invalid(format!("serialize failed: {e}")))?;
    fs::write(&out, raw).map_err(|e| CliError::io("session save", e))?;
    write_active_session(&opts, &opts.session).map_err(|e| CliError::io("session save", e))?;

    let data = json!({
        "session_id": opts.session,
        "path": out.to_string_lossy(),
        "snapshot": export.snapshot
    });
    let human = format!("セッション '{}' を保存しました。", opts.session);
    Ok((data, human))
}

fn cmd_session_load(mut opts: GlobalOptions, id: String) -> Result<(Value, String), CliError> {
    let path = session_file_path(&opts, &id);
    if !path.exists() {
        return Err(CliError::session_missing("session load", &id));
    }
    let raw = fs::read_to_string(&path).map_err(|e| CliError::io("session load", e))?;
    let imported: SessionJsonV1 = serde_json::from_str(&raw).map_err(|e| CliError::invalid(format!("invalid session: {e}")))?;
    
    opts.session = id.clone();
    write_active_session(&opts, &id).map_err(|e| CliError::io("session load", e))?;

    let data = json!({
        "session_id": id,
        "snapshot": imported.snapshot
    });
    let human = format!("セッション '{}' を読み込みました。", id);
    Ok((data, human))
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
    let mut vm = HybridVM::for_cli_storage(vm_store)?;
    let session_path = session_file_path(opts, &opts.session);
    if session_path.exists() {
        if let Ok(raw) = fs::read_to_string(&session_path) {
            if let Ok(saved) = serde_json::from_str::<SessionJsonV1>(&raw) {
                vm.load_feedback_entries(saved.feedback_entries);
                vm.load_l2_grounding(saved.l2_grounding);
                vm.load_l2_refinements(saved.l2_refinements);
            }
        }
    }
    Ok(vm)
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

fn print_success(json_mode: bool, command: &str, data: Value, human: &str) {
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
        println!("{human}");
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

#[derive(Clone, Debug, Serialize)]
struct RemediationItem {
    id: String,
    target_id: String,
    priority: String,
    issue: String,
    message: String,
    action_type: String,
}

#[derive(Clone, Debug, Serialize)]
struct MissingInfoItem {
    target_id: Option<String>,
    category: String,
    prompt: String,
    importance: f64,
}

fn info_category_name(category: &InfoCategory) -> &'static str {
    match category {
        InfoCategory::Constraint => "constraint",
        InfoCategory::Boundary => "boundary",
        InfoCategory::Metric => "metric",
        InfoCategory::Objective => "objective",
    }
}

fn format_missing_info_human(items: &[MissingInfoItem]) -> String {
    if items.is_empty() {
        return "不足情報の問いかけ: なし".to_string();
    }
    let mut lines = vec!["不足情報の問いかけ:".to_string()];
    for item in items {
        let target = item.target_id.clone().unwrap_or_else(|| "global".to_string());
        lines.push(format!(
            "- [{}] {} (importance={:.2}, category={})",
            target, item.prompt, item.importance, item.category
        ));
    }
    lines.join("\n")
}

fn detect_remediations(l1_units: &[SemanticUnitL1V2], l2_units: &[ConceptUnitV2]) -> Vec<RemediationItem> {
    let mut out = Vec::new();
    let mut seq = 1usize;

    for l1 in l1_units {
        if l1.ambiguity_score > 0.7 {
            out.push(RemediationItem {
                id: format!("REM-{seq:03}"),
                target_id: format!("L1-{}", l1.id.0),
                priority: "high".to_string(),
                issue: "high_ambiguity".to_string(),
                message: "要件が抽象的です。数値化・境界条件の明示で具体化してください。".to_string(),
                action_type: "refine_text".to_string(),
            });
            seq += 1;
        }
    }

    for l2 in l2_units {
        let has_pos = l2.derived_requirements.iter().any(|r| r.strength > 0.0);
        let has_neg = l2.derived_requirements.iter().any(|r| r.strength < 0.0);
        if has_pos && has_neg {
            out.push(RemediationItem {
                id: format!("REM-{seq:03}"),
                target_id: format!("L2-{}", l2.id.0),
                priority: "high".to_string(),
                issue: "l2_conflict".to_string(),
                message: "相反する要件が混在しています。優先順位の明確化またはモジュール分離を検討してください。".to_string(),
                action_type: "resolve_tradeoff".to_string(),
            });
            seq += 1;
        }
    }

    let avg_links = if l2_units.is_empty() {
        0.0
    } else {
        l2_units.iter().map(|u| u.causal_links.len() as f64).sum::<f64>() / l2_units.len() as f64
    };
    for l2 in l2_units {
        if avg_links > 0.0 && (l2.causal_links.len() as f64) >= avg_links * 2.0 {
            out.push(RemediationItem {
                id: format!("REM-{seq:03}"),
                target_id: format!("L2-{}", l2.id.0),
                priority: "medium".to_string(),
                issue: "coupling_hub".to_string(),
                message: "因果リンクが集中しています。ハブ機能の分割を検討してください。".to_string(),
                action_type: "split_hub".to_string(),
            });
            seq += 1;
        }
    }

    out
}

fn build_graph_json(vm: &HybridVM, l1_units: &[SemanticUnitL1V2], l2_units: &[ConceptUnitV2]) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for l1 in l1_units {
        nodes.push(json!({
            "id": format!("L1-{}", l1.id.0),
            "type": "L1",
            "label": format!("L1-{}", l1.id.0),
            "score": round6(l1.ambiguity_score)
        }));
    }

    let mut concept_l1_refs = std::collections::BTreeMap::<u64, Vec<u128>>::new();
    for l2 in l2_units {
        nodes.push(json!({
            "id": format!("L2-{}", l2.id.0),
            "type": "L2",
            "label": format!("L2-{}", l2.id.0),
            "score": round6(l2.stability_score)
        }));
        if let Some(concept) = vm.get_concept(l2.id) {
            let refs = concept.l1_refs.iter().map(|id| id.0).collect::<Vec<_>>();
            concept_l1_refs.insert(l2.id.0, refs.clone());
            for l1_id in refs {
                edges.push(json!({
                    "from": format!("L1-{l1_id}"),
                    "to": format!("L2-{}", l2.id.0),
                    "type": "mapping"
                }));
            }
        }
    }

    for src in l2_units {
        let Some(src_refs) = concept_l1_refs.get(&src.id.0) else {
            continue;
        };
        for link in &src.causal_links {
            for dst in l2_units {
                if src.id == dst.id {
                    continue;
                }
                let Some(dst_refs) = concept_l1_refs.get(&dst.id.0) else {
                    continue;
                };
                if src_refs.contains(&link.from.0) && dst_refs.contains(&link.to.0) {
                    edges.push(json!({
                        "from": format!("L2-{}", src.id.0),
                        "to": format!("L2-{}", dst.id.0),
                        "type": "causal",
                        "weight": round6(link.weight)
                    }));
                }
            }
        }
    }

    json!({
        "nodes": nodes,
        "edges": edges
    })
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn help_text() -> String {
    "design [OPTIONS] <COMMAND>\nCommands: analyze, explain, adopt, reject, search --card <L2-ID> [--query <text>] --allow, refine --card <L2-ID> --text <detail>, clear, snapshot, diff, rebuild, simulate, export [--format rust|sql|mermaid --out <dir>], import, session list/save/load".to_string()
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
    #[serde(default)]
    feedback_entries: Vec<FeedbackEntry>,
    #[serde(default)]
    l2_grounding: Vec<(u64, Vec<String>)>,
    #[serde(default)]
    l2_refinements: Vec<(u64, Vec<String>)>,
}

#[derive(Serialize, Deserialize)]
struct SnapshotBrief {
    l1_hash: String,
    l2_hash: String,
    version: u16,
}

fn build_session_json(opts: &GlobalOptions, vm: &HybridVM) -> Result<SessionJsonV1, SemanticError> {
    let snapshot = vm.snapshot_v2()?;
    let l1_units = vm.all_l1_units_v2()?;
    let l2_units = vm.project_phase_a_v2()?;
    let now = now_ms();
    Ok(SessionJsonV1 {
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
        feedback_entries: vm.feedback_entries(),
        l2_grounding: vm.export_l2_grounding(),
        l2_refinements: vm.export_l2_refinements(),
    })
}

fn write_session_state(opts: &GlobalOptions, vm: &HybridVM) -> std::io::Result<()> {
    let out = session_file_path(opts, &opts.session);
    let export = build_session_json(opts, vm)
        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let raw = serde_json::to_string_pretty(&export)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    fs::write(&out, raw)?;
    write_active_session(opts, &opts.session)?;
    Ok(())
}

fn parse_artifact_format(raw: &str) -> Result<ArtifactFormat, CliError> {
    match raw {
        "rust" => Ok(ArtifactFormat::Rust),
        "sql" => Ok(ArtifactFormat::Sql),
        "mermaid" => Ok(ArtifactFormat::Mermaid),
        _ => Err(CliError::invalid(
            "invalid format. expected one of: rust|sql|mermaid",
        )),
    }
}

fn artifact_format_name(format: ArtifactFormat) -> &'static str {
    match format {
        ArtifactFormat::Rust => "rust",
        ArtifactFormat::Sql => "sql",
        ArtifactFormat::Mermaid => "mermaid",
    }
}

fn parse_card_id(card_id: &str) -> Result<hybrid_vm::ConceptId, CliError> {
    let raw = card_id.strip_prefix("L2-").unwrap_or(card_id);
    let parsed = raw
        .parse::<u64>()
        .map_err(|_| CliError::invalid("card id must be L2-<number>"))?;
    Ok(hybrid_vm::ConceptId(parsed))
}
