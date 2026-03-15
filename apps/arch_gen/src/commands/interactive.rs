use std::io::{self, BufRead, Write};
use std::path::Path;

use design_search_engine::RankedCandidate;

use crate::commands::chat::run_chat_session;
use crate::commands::generate::{PipelineResult, run_phase9_pipeline};
use crate::store::{DesignStore, format_store_list};
use crate::commands::knowledge_layer::{
    StoredKnowledgeHit, grounding_knowledge_path, knowledge_layer_metrics, prepare_inference_input,
    promote_hits_to_grounding, save_temporary_web_hits, temporary_knowledge_path,
};
use crate::input_bridge::{
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation, arch_state_to_architecture,
    load_design_file, save_design_file,
};
use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::narrative::verbalize_candidate;
use crate::output::text::CandidateDisplay;
use code_ir::{ArchitectureToCodeIR, DeterministicArchitectureToCodeIR};

/// インタラクティブセッションの状態
struct SessionState {
    requirement: String,
    candidates: Vec<RankedCandidate>,
    selected: Option<usize>,
    latest_web_hits: Vec<StoredKnowledgeHit>,
}

pub struct InteractiveArgs {
    pub from: Option<String>,
}

/// `interactive` コマンド: 対話型設計精緻化フロー
pub fn run(args: InteractiveArgs) -> Result<(), String> {
    let mut state: Option<SessionState> = None;
    let mut startup_messages: Vec<String> = Vec::new();

    // --from で既存設計をロード
    if let Some(ref path) = args.from {
        match load_design_file(Path::new(path)) {
            Ok(design) => {
                startup_messages.push(format!(
                    "Loaded: \"{}\" ({} candidates)",
                    design.input,
                    design.candidates.len()
                ));
                // 既存設計から requirement だけ復元してセッション開始
                // パイプラインを再実行してランク付き候補を取得
                let req = GenerateRequest::new(
                    design.input.clone(),
                    10,
                    5,
                    design.candidates.len().max(1),
                    true,
                    false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult { ranked, .. }) => {
                        let n = ranked.len();
                        state = Some(SessionState {
                            requirement: design.input,
                            candidates: ranked,
                            selected: None,
                            latest_web_hits: Vec::new(),
                        });
                        startup_messages.push(format!("Resumed session: {n} candidates available"));
                    }
                    Err(e) => {
                        startup_messages
                            .push(format!("Warning: could not restore candidates: {e}"));
                    }
                };
            }
            Err(e) => {
                startup_messages.push(format!("Warning: {e}; starting fresh session"));
            }
        };
    }

    run_session(state, &startup_messages)
}

pub fn run_seeded(requirement: String, candidates: Vec<RankedCandidate>) -> Result<(), String> {
    run_session(
        Some(SessionState {
            requirement,
            candidates,
            selected: None,
            latest_web_hits: Vec::new(),
        }),
        &[],
    )
}

fn run_session(mut state: Option<SessionState>, startup_messages: &[String]) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(
        out,
        "arch_gen interactive mode (type 'q' to quit, 'help' for commands)"
    )
    .ok();
    writeln!(out, "{}", "─".repeat(55)).ok();
    for message in startup_messages {
        writeln!(out, "{message}").ok();
    }
    if let Some(ref sess) = state {
        writeln!(out, "Requirement: \"{}\"", sess.requirement).ok();
        writeln!(
            out,
            "{} candidate(s) ready for review.",
            sess.candidates.len()
        )
        .ok();
        writeln!(
            out,
            "Type 'list' to inspect candidates. You can also type a new requirement directly."
        )
        .ok();
        print_knowledge_status(&mut out);
        writeln!(out).ok();
    }

    // REPLループ
    loop {
        // プロンプト表示
        let prompt = if state.is_none() {
            "arch_gen> "
        } else if state
            .as_ref()
            .map(|s| s.selected.is_none())
            .unwrap_or(false)
        {
            "arch_gen [s]elect> "
        } else {
            "arch_gen [r/e/m/q]> "
        };
        write!(out, "{prompt}").ok();
        out.flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Err(_) => break,
            _ => {}
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "q" | "quit" | "exit" => {
                writeln!(out, "Goodbye.").ok();
                break;
            }
            "help" | "h" | "?" => {
                print_help(
                    &mut out,
                    state.is_some(),
                    state.as_ref().and_then(|s| s.selected),
                );
            }
            _ if state.is_none() => {
                // 要件入力モード
                let requirement = normalize_requirement_input(input);
                if requirement != input {
                    writeln!(
                        out,
                        "Interpreting input as requirement: \"{}\"",
                        requirement
                    )
                    .ok();
                }
                write!(out, "Generating... ").ok();
                out.flush().ok();

                let knowledge = match prepare_inference_input(&requirement) {
                    Ok(knowledge) => knowledge,
                    Err(e) => {
                        writeln!(out, "\nError: {e}").ok();
                        continue;
                    }
                };
                let req = GenerateRequest::new(
                    knowledge.enriched_requirement.clone(),
                    10,
                    5,
                    3,
                    true,
                    false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult {
                        ranked,
                        search_states_count,
                    }) => {
                        writeln!(out, "done ({search_states_count} states searched)").ok();
                        writeln!(out).ok();
                        print_candidates(&requirement, &ranked, &mut out);
                        print_knowledge_context(&mut out, &knowledge);
                        state = Some(SessionState {
                            requirement,
                            candidates: ranked,
                            selected: None,
                            latest_web_hits: Vec::new(),
                        });
                    }
                    Err(e) => {
                        writeln!(out, "\nError: {e}").ok();
                    }
                }
            }
            _ => {
                let Some(ref mut sess) = state else {
                    writeln!(out, "Enter a requirement to start, or 'q' to quit.").ok();
                    continue;
                };

                let parts: Vec<&str> = input.splitn(2, ' ').collect();
                match parts[0] {
                    "s" | "select" => {
                        let idx: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                        if idx == 0 || idx > sess.candidates.len() {
                            writeln!(out, "Invalid index. Choose 1–{}", sess.candidates.len()).ok();
                        } else {
                            sess.selected = Some(idx - 1);
                            writeln!(out, "案 {idx} を選択しました。").ok();
                            if let Some(candidate) = sess.candidates.get(idx - 1) {
                                print_candidate_detail(&sess.requirement, idx, candidate, &mut out);
                            }
                        }
                    }
                    "r" | "refine" => {
                        write!(out, "Additional requirement: ").ok();
                        out.flush().ok();
                        let mut extra = String::new();
                        stdin.lock().read_line(&mut extra).ok();
                        let extra = extra.trim().to_string();
                        refine_session(sess, &extra, &mut out);
                    }
                    "m" | "mermaid" => {
                        let idx = sess.selected.unwrap_or(0);
                        if let Some(c) = sess.candidates.get(idx) {
                            let (names, pairs) = candidate_to_display_parts(c);
                            writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                        } else {
                            writeln!(out, "No candidate selected. Use 's <N>' first.").ok();
                        }
                    }
                    "e" | "export" => {
                        let fmt = parts.get(1).copied().unwrap_or("text");
                        let idx = sess.selected.unwrap_or(0);
                        if let Some(c) = sess.candidates.get(idx) {
                            let (names, pairs) = candidate_to_display_parts(c);
                            match fmt {
                                "mermaid" => {
                                    writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                                }
                                "markdown" => {
                                    let ev = c.state.world_state.evaluation.clone();
                                    let display = CandidateDisplay {
                                        score: c.score,
                                        pareto_rank: c.pareto_rank,
                                        component_names: names,
                                        dependency_pairs: pairs,
                                        evaluation: ev,
                                        generated_files: vec![],
                                    };
                                    writeln!(
                                        out,
                                        "{}",
                                        build_markdown(&sess.requirement, 0, &[display])
                                    )
                                    .ok();
                                }
                                _ => {
                                    print_candidate_detail(&sess.requirement, idx + 1, c, &mut out);
                                }
                            }
                        } else {
                            writeln!(out, "No candidate selected.").ok();
                        }
                    }
                    "save" => {
                        let arg = parts.get(1).copied().unwrap_or("design_session.json");
                        let saved = build_saved_design(sess);

                        // 引数がファイルパスっぽければ従来の動作、それ以外はストアに保存
                        if arg.contains('/') || arg.ends_with(".json") {
                            if let Err(e) = save_design_file(&saved, Path::new(arg)) {
                                writeln!(out, "Error: {e}").ok();
                            } else {
                                writeln!(out, "Saved to {arg}").ok();
                            }
                        } else {
                            let store = DesignStore::new();
                            match store.save(arg, &saved) {
                                Ok(path) => writeln!(out, "Saved as '{}' → {}", arg, path.display()).ok(),
                                Err(e)   => writeln!(out, "Error: {e}").ok(),
                            };
                        }
                    }
                    "saves" | "store" => {
                        let store = DesignStore::new();
                        match store.list() {
                            Ok(entries) => writeln!(out, "{}", format_store_list(&entries)).ok(),
                            Err(e)      => writeln!(out, "Error: {e}").ok(),
                        };
                    }
                    "load" => {
                        let name = match parts.get(1) {
                            Some(n) => *n,
                            None => {
                                writeln!(out, "Usage: load <name>").ok();
                                continue;
                            }
                        };
                        let store = DesignStore::new();
                        match store.load(name) {
                            Ok(design) => {
                                // 読み込んだ設計でパイプラインを再実行
                                let req = GenerateRequest::new(
                                    design.input.clone(), 10, 5,
                                    design.candidates.len().max(1), true, false,
                                );
                                match run_phase9_pipeline(&req) {
                                    Ok(PipelineResult { ranked, .. }) => {
                                        writeln!(out, "Loaded '{}' — {} candidates.", name, ranked.len()).ok();
                                        print_candidates(&design.input, &ranked, &mut out);
                                        sess.requirement = design.input;
                                        sess.candidates = ranked;
                                        sess.selected = None;
                                    }
                                    Err(e) => { writeln!(out, "Error restoring pipeline: {e}").ok(); }
                                }
                            }
                            Err(e) => { writeln!(out, "Error: {e}").ok(); }
                        }
                    }
                    "chat" => {
                        run_chat_session(
                            &sess.requirement,
                            &sess.candidates,
                            sess.selected,
                            &stdin,
                            &mut out,
                        );
                    }
                    "w" | "web" => {
                        let query = parts.get(1).copied().unwrap_or(&sess.requirement);
                        match crate::commands::web_search::search(query, 5) {
                            Ok(hits) if hits.is_empty() => {
                                writeln!(out, "Web search returned no results.").ok();
                            }
                            Ok(hits) => {
                                match save_temporary_web_hits(query, &hits) {
                                    Ok(saved_hits) => {
                                        sess.latest_web_hits = saved_hits.clone();
                                    }
                                    Err(e) => {
                                        writeln!(out, "Failed to save temporary knowledge: {e}")
                                            .ok();
                                        sess.latest_web_hits.clear();
                                    }
                                }
                                writeln!(out, "Web knowledge for: {query}").ok();
                                for (i, hit) in sess.latest_web_hits.iter().enumerate() {
                                    writeln!(out, "  {}. {}", i + 1, hit.title).ok();
                                    writeln!(out, "     {}", hit.snippet).ok();
                                }
                                if !sess.latest_web_hits.is_empty() {
                                    writeln!(
                                        out,
                                        "Temporary knowledge saved to {}",
                                        temporary_knowledge_path().display()
                                    )
                                    .ok();
                                    writeln!(
                                        out,
                                        "`g <N>` で確認済みの結果を grounding knowledge に昇格できます。"
                                    )
                                    .ok();
                                }
                            }
                            Err(e) => {
                                writeln!(out, "Web search error: {e}").ok();
                            }
                        }
                    }
                    "g" | "ground" => {
                        let idx: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                        if idx == 0 || idx > sess.latest_web_hits.len() {
                            writeln!(
                                out,
                                "Invalid index. Choose 1–{}",
                                sess.latest_web_hits.len()
                            )
                            .ok();
                            continue;
                        }
                        let hit_id = sess.latest_web_hits[idx - 1].id;
                        match promote_hits_to_grounding(&[hit_id]) {
                            Ok(promoted) if promoted.is_empty() => {
                                writeln!(out, "Selected knowledge was not promoted.").ok();
                            }
                            Ok(promoted) => {
                                writeln!(
                                    out,
                                    "Promoted {} item(s) to grounding knowledge.",
                                    promoted.len()
                                )
                                .ok();
                                writeln!(
                                    out,
                                    "Grounding knowledge saved to {}",
                                    grounding_knowledge_path()
                                        .map(|path| path.display().to_string())
                                        .unwrap_or_else(
                                            |_| ".arch_gen/grounding_knowledge.json".to_string()
                                        )
                                )
                                .ok();
                                sess.latest_web_hits.retain(|hit| hit.id != hit_id);
                            }
                            Err(e) => {
                                writeln!(out, "Grounding promotion error: {e}").ok();
                            }
                        }
                    }
                    "k" | "knowledge" => {
                        print_knowledge_status(&mut out);
                    }
                    "list" | "ls" => {
                        print_candidates(&sess.requirement, &sess.candidates, &mut out);
                    }
                    _ => {
                        let normalized = normalize_requirement_input(input);
                        if normalized != input {
                            writeln!(
                                out,
                                "Interpreting input as additional requirement: \"{}\"",
                                normalized
                            )
                            .ok();
                        }
                        refine_session(sess, &normalized, &mut out);
                    }
                }
            }
        }
    }

    Ok(())
}

// ─── ヘルパー ────────────────────────────────────────────────────────────────

fn print_help(out: &mut impl Write, has_session: bool, selected: Option<usize>) {
    writeln!(out, "Commands:").ok();
    if !has_session {
        writeln!(out, "  <requirement>   Start a design conversation").ok();
    }
    writeln!(out, "  s <N>           Select candidate N").ok();
    writeln!(
        out,
        "  r               Add another requirement and re-search"
    )
    .ok();
    writeln!(out, "  list            Show all candidate summaries").ok();
    writeln!(
        out,
        "  w [query]       Fetch supplementary knowledge from web search"
    )
    .ok();
    writeln!(
        out,
        "  g <N>           Promote latest web result N to grounding knowledge"
    )
    .ok();
    writeln!(
        out,
        "  k               Show temporary/grounding knowledge status"
    )
    .ok();
    writeln!(
        out,
        "  m               Show Mermaid diagram for selected candidate"
    )
    .ok();
    writeln!(
        out,
        "  e [fmt]         Export selected candidate (text|mermaid|markdown)"
    )
    .ok();
    writeln!(out, "  save [path]     Save session to design JSON file").ok();
    writeln!(out, "  q / quit        Exit").ok();
    writeln!(out, "  Any plain text  Treat as an additional requirement").ok();
    writeln!(out, "  CLI 形式の入力は自動で要件文に正規化されます").ok();
    if let Some(idx) = selected {
        writeln!(out, "  (currently selected: candidate {})", idx + 1).ok();
    }
}

fn normalize_requirement_input(input: &str) -> String {
    let trimmed = input.trim();
    let stripped = trimmed.strip_prefix("arch_gen ").unwrap_or(trimmed).trim();

    for prefix in ["/generate", "generate"] {
        if let Some(rest) = stripped.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() {
                return trimmed.to_string();
            }
            return trim_matching_quotes(rest);
        }
    }

    trim_matching_quotes(stripped)
}

fn trim_matching_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap_or_default();
        let last = trimmed.chars().last().unwrap_or_default();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn print_candidates(requirement: &str, candidates: &[RankedCandidate], out: &mut impl Write) {
    for (i, c) in candidates.iter().enumerate() {
        let (names, _) = candidate_to_display_parts(c);
        let title = candidate_title(&names);
        let summary = candidate_summary(requirement, &names);
        writeln!(out, "  {}. {}", i + 1, title).ok();
        writeln!(out, "     構成: {}", summary).ok();
        writeln!(out, "     役割: {}", describe_components(&names)).ok();
    }
    writeln!(out).ok();
    writeln!(
        out,
        "番号を選ぶには `s <N>`、追加要求を試すならそのまま文章を入力してください。"
    )
    .ok();
}

fn print_candidate_detail(
    requirement: &str,
    index: usize,
    candidate: &RankedCandidate,
    out: &mut impl Write,
) {
    let saved = saved_candidate_from_ranked(index, candidate);
    let (names, pairs) = candidate_to_display_parts(candidate);
    writeln!(out, "案 {} の概要", index).ok();
    writeln!(out, "{}", verbalize_candidate(requirement, &saved)).ok();
    writeln!(
        out,
        "構成の要約: {}",
        candidate_summary(requirement, &names)
    )
    .ok();
    writeln!(out, "各要素の役割: {}", describe_components(&names)).ok();
    if pairs.is_empty() {
        writeln!(out, "まだ依存関係が見えていません。編集バッファ、保存、描画更新の分離を追加で指定すると精度が上がります。").ok();
    } else {
        writeln!(out, "主な関係:").ok();
        for (from, to) in pairs {
            writeln!(out, "  - {from} -> {to}").ok();
        }
    }
}

fn refine_session(sess: &mut SessionState, extra: &str, out: &mut impl Write) {
    let extra = extra.trim();
    if extra.is_empty() {
        writeln!(out, "追加したい要件を入力してください。").ok();
        return;
    }
    let combined = format!("{}\n{}", sess.requirement, extra);
    writeln!(out, "追加要件を反映して設計案を更新します。").ok();
    let knowledge = match prepare_inference_input(&combined) {
        Ok(knowledge) => knowledge,
        Err(e) => {
            writeln!(out, "\nError: {e}").ok();
            return;
        }
    };
    let req = GenerateRequest::new(
        knowledge.enriched_requirement.clone(),
        10,
        5,
        3,
        true,
        false,
    );
    match run_phase9_pipeline(&req) {
        Ok(PipelineResult { ranked, .. }) => {
            writeln!(out).ok();
            print_candidates(&sess.requirement, &ranked, out);
            print_knowledge_context(out, &knowledge);
            sess.requirement = combined;
            sess.candidates = ranked;
            sess.selected = None;
        }
        Err(e) => {
            writeln!(out, "\nError: {e}").ok();
        }
    }
}

fn candidate_to_display_parts(c: &RankedCandidate) -> (Vec<String>, Vec<(String, String)>) {
    let architecture = arch_state_to_architecture(&c.state.architecture_state);
    let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);
    let names: Vec<String> = code_ir.modules.iter().map(|m| m.name.clone()).collect();
    let units_by_id = architecture.design_units_by_id();
    let pairs: Vec<(String, String)> = architecture
        .dependencies
        .iter()
        .filter_map(|dep| {
            let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
            let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
            Some((from, to))
        })
        .collect();
    (names, pairs)
}

fn build_saved_design(sess: &SessionState) -> SavedDesign {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let candidates = sess
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let eval = &c.state.world_state.evaluation;
            let (names, pairs) = candidate_to_display_parts(c);
            SavedCandidate {
                id: i + 1,
                score: c.score,
                pareto_rank: c.pareto_rank,
                evaluation: SavedEvaluation {
                    structural_quality: eval.structural_quality,
                    dependency_quality: eval.dependency_quality,
                    constraint_satisfaction: eval.constraint_satisfaction,
                    complexity: eval.complexity,
                    simulation_quality: eval.simulation_quality,
                    total: eval.total(),
                },
                components: names,
                dependencies: pairs.into_iter().map(|(f, t)| [f, t]).collect(),
                code_metrics: Default::default(),
            }
        })
        .collect();

    SavedDesign {
        version: "1.0".to_string(),
        generated_at: format!("{now}"),
        input: sess.requirement.clone(),
        search_states: 0,
        candidates,
    }
}

fn saved_candidate_from_ranked(id: usize, c: &RankedCandidate) -> SavedCandidate {
    let eval = &c.state.world_state.evaluation;
    let (names, pairs) = candidate_to_display_parts(c);
    SavedCandidate {
        id,
        score: c.score,
        pareto_rank: c.pareto_rank,
        evaluation: SavedEvaluation {
            structural_quality: eval.structural_quality,
            dependency_quality: eval.dependency_quality,
            constraint_satisfaction: eval.constraint_satisfaction,
            complexity: eval.complexity,
            simulation_quality: eval.simulation_quality,
            total: eval.total(),
        },
        components: names,
        dependencies: pairs.into_iter().map(|(f, t)| [f, t]).collect(),
        code_metrics: Default::default(),
    }
}

fn candidate_title(names: &[String]) -> &'static str {
    let service_count = names.iter().filter(|name| name.contains("service")).count();
    let controller_count = names
        .iter()
        .filter(|name| name.contains("controller"))
        .count();
    let repository_count = names
        .iter()
        .filter(|name| name.contains("repository"))
        .count();
    let database_count = names
        .iter()
        .filter(|name| name.contains("database"))
        .count();

    match (
        service_count,
        controller_count,
        repository_count,
        database_count,
    ) {
        (1, 1, 1, 1) => "単一編集コアでまとめた標準構成",
        (2.., 1, 1, 1) => "編集機能を分割した標準構成",
        (1, 2.., 1, 1) => "入力系を分割した標準構成",
        (2.., 2.., 1, 1) => "入力系と編集系を分割した拡張構成",
        (1.., 1.., 0, 0) => "永続化を持たない軽量構成",
        _ => "最小要素でまとめた構成",
    }
}

fn candidate_summary(requirement: &str, names: &[String]) -> String {
    let service_count = names.iter().filter(|name| name.contains("service")).count();
    let controller_count = names
        .iter()
        .filter(|name| name.contains("controller"))
        .count();
    let terminal = if requirement.contains("ターミナル")
        || requirement.contains("NeoVim")
        || requirement.contains("Vim")
    {
        "ターミナル操作を前提に"
    } else {
        "要求を満たすために"
    };

    let focus = if requirement.contains("学生") {
        "学習コストを抑えた編集体験"
    } else if requirement.contains("設定") || requirement.contains("カスタム") {
        "設定可能な編集体験"
    } else {
        "基本的な編集体験"
    };
    let split = match (service_count, controller_count) {
        (2.., 2..) => "入力処理と編集機能の両方を分割しながら",
        (2.., _) => "編集機能を複数サービスへ分割しながら",
        (_, 2..) => "入力処理を複数コントローラへ分割しながら",
        _ => "主要な責務を素直に分離しながら",
    };

    format!(
        "{terminal}{focus}を目指して、{split} {} 要素で組む構成",
        names.len()
    )
}

fn describe_components(names: &[String]) -> String {
    let mut roles = Vec::new();

    let service_count = names.iter().filter(|name| name.contains("service")).count();
    let controller_count = names
        .iter()
        .filter(|name| name.contains("controller"))
        .count();
    let repository_count = names
        .iter()
        .filter(|name| name.contains("repository"))
        .count();
    let database_count = names
        .iter()
        .filter(|name| name.contains("database"))
        .count();

    if controller_count > 0 {
        roles.push(if controller_count > 1 {
            "入力処理や画面制御を分けて扱う層".to_string()
        } else {
            "入力処理や画面制御を受け持つ層".to_string()
        });
    }
    if service_count > 0 {
        roles.push(if service_count > 1 {
            "編集機能や設定処理を複数のサービスに分割する層".to_string()
        } else {
            "編集機能や設定処理をまとめる層".to_string()
        });
    }
    if repository_count > 0 {
        roles.push("設定・履歴・永続化へのアクセスを仲介する層".to_string());
    }
    if database_count > 0 {
        roles.push("設定や状態を保存するストレージ層".to_string());
    }

    if roles.is_empty() {
        "役割分担がまだ明確ではない最小構成".to_string()
    } else {
        roles.join(" / ")
    }
}

fn print_knowledge_context(
    out: &mut impl Write,
    knowledge: &crate::commands::knowledge_layer::InferenceKnowledgeContext,
) {
    if knowledge.temporary_hits.is_empty() && knowledge.grounding_hits.is_empty() {
        return;
    }

    writeln!(out, "Knowledge Context").ok();
    if !knowledge.temporary_hits.is_empty() {
        writeln!(
            out,
            "  temporary web knowledge: {} item(s)",
            knowledge.temporary_hits.len()
        )
        .ok();
    }
    if !knowledge.grounding_hits.is_empty() {
        writeln!(
            out,
            "  grounding knowledge: {} item(s)",
            knowledge.grounding_hits.len()
        )
        .ok();
    }
    writeln!(out).ok();
}

fn print_knowledge_status(out: &mut impl Write) {
    match knowledge_layer_metrics() {
        Ok(metrics) => {
            writeln!(out, "Knowledge layer status").ok();
            writeln!(
                out,
                "  temporary: {} item(s) [{}]",
                metrics.temporary_hits,
                temporary_knowledge_path().display()
            )
            .ok();
            match grounding_knowledge_path() {
                Ok(path) => {
                    writeln!(
                        out,
                        "  grounding: {} item(s) [{}]",
                        metrics.grounding_hits,
                        path.display()
                    )
                    .ok();
                }
                Err(e) => {
                    writeln!(out, "  grounding: {} item(s) [{e}]", metrics.grounding_hits).ok();
                }
            }
        }
        Err(e) => {
            writeln!(out, "Knowledge layer status unavailable: {e}").ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// パイプで入力を渡してインタラクティブモードをテスト
    #[test]
    fn test_interactive_quit_immediately() {
        // stdin に "q\n" を渡す → 即座に終了
        // run() は stdin を直接使うため、ここでは引数のバリデーションのみテスト
        let args = InteractiveArgs { from: None };
        // from=None の場合は常に Ok で返る（ループ起動まで）
        // 実際の REPL は stdin に依存するため統合テストで検証
        drop(args); // コンパイル確認のみ
    }

    #[test]
    fn test_interactive_invalid_from_file_is_warned_not_errored() {
        // --from に存在しないファイルを渡しても Warning のみで続行できる設計
        // (run() 内で Err をログするだけ)
        // ここでは load_design_file のエラーパスが適切に処理されることを確認
        let result = crate::input_bridge::load_design_file(Path::new("/nonexistent.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_requirement_input_strips_cli_prefix() {
        assert_eq!(
            normalize_requirement_input(r#"arch_gen /generate "NeoVim風のEditorを作成したい""#),
            "NeoVim風のEditorを作成したい"
        );
        assert_eq!(
            normalize_requirement_input("/generate NeoVim風のEditorを作成したい"),
            "NeoVim風のEditorを作成したい"
        );
        assert_eq!(
            normalize_requirement_input("generate 'NeoVim風のEditorを作成したい'"),
            "NeoVim風のEditorを作成したい"
        );
    }

    #[test]
    fn test_normalize_requirement_input_keeps_plain_text() {
        assert_eq!(
            normalize_requirement_input("ターミナルUIとモード切替を持つこと"),
            "ターミナルUIとモード切替を持つこと"
        );
    }
}
