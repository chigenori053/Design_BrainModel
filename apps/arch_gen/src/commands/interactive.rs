use std::io::{self, BufRead, Write};
use std::path::Path;

use design_search_engine::RankedCandidate;

use crate::commands::generate::{PipelineResult, run_phase9_pipeline};
use crate::input_bridge::{
    GenerateRequest, SavedDesign, arch_state_to_architecture, load_design_file, save_design_file,
    SavedCandidate, SavedEvaluation,
};
use crate::output::mermaid::build_mermaid;
use crate::output::markdown::build_markdown;
use crate::output::text::{CandidateDisplay, render_evaluation};
use code_ir::{ArchitectureToCodeIR, DeterministicArchitectureToCodeIR};

/// インタラクティブセッションの状態
struct SessionState {
    requirement: String,
    candidates: Vec<RankedCandidate>,
    selected: Option<usize>,
}

pub struct InteractiveArgs {
    pub from: Option<String>,
}

/// `interactive` コマンド: 対話型設計精緻化フロー
pub fn run(args: InteractiveArgs) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(out, "arch-gen interactive mode (type 'q' to quit, 'help' for commands)").ok();
    writeln!(out, "{}", "─".repeat(55)).ok();

    let mut state: Option<SessionState> = None;

    // --from で既存設計をロード
    if let Some(ref path) = args.from {
        match load_design_file(Path::new(path)) {
            Ok(design) => {
                writeln!(out, "Loaded: \"{}\" ({} candidates)", design.input, design.candidates.len()).ok();
                // 既存設計から requirement だけ復元してセッション開始
                // パイプラインを再実行してランク付き候補を取得
                let req = GenerateRequest::new(
                    design.input.clone(), 10, 5, design.candidates.len().max(1), true, false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult { ranked, .. }) => {
                        let n = ranked.len();
                        state = Some(SessionState {
                            requirement: design.input,
                            candidates: ranked,
                            selected: None,
                        });
                        let _ = writeln!(out, "Resumed session: {n} candidates available");
                    }
                    Err(e) => {
                        let _ = writeln!(out, "Warning: could not restore candidates: {e}");
                    }
                };
            }
            Err(e) => {
                let _ = writeln!(out, "Warning: {e}; starting fresh session");
            }
        };
    }

    // REPLループ
    loop {
        // プロンプト表示
        let prompt = if state.is_none() {
            "arch-gen> "
        } else if state.as_ref().map(|s| s.selected.is_none()).unwrap_or(false) {
            "arch-gen [s]elect> "
        } else {
            "arch-gen [r/e/m/q]> "
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
                print_help(&mut out, state.is_some(), state.as_ref().and_then(|s| s.selected));
            }
            _ if state.is_none() => {
                // 要件入力モード
                let requirement = input.to_string();
                write!(out, "Generating... ").ok();
                out.flush().ok();

                let req = GenerateRequest::new(requirement.clone(), 10, 5, 3, true, false);
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult { ranked, search_states_count }) => {
                        writeln!(out, "done ({search_states_count} states searched)").ok();
                        writeln!(out).ok();
                        print_candidates(&ranked, &mut out);
                        state = Some(SessionState {
                            requirement,
                            candidates: ranked,
                            selected: None,
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
                            writeln!(out, "Selected candidate {idx}.").ok();
                        }
                    }
                    "r" | "refine" => {
                        write!(out, "Additional requirement: ").ok();
                        out.flush().ok();
                        let mut extra = String::new();
                        stdin.lock().read_line(&mut extra).ok();
                        let extra = extra.trim().to_string();
                        let combined = format!("{}\n{extra}", sess.requirement);
                        write!(out, "Refining... ").ok();
                        out.flush().ok();
                        let req = GenerateRequest::new(combined.clone(), 10, 5, 3, true, false);
                        match run_phase9_pipeline(&req) {
                            Ok(PipelineResult { ranked, search_states_count }) => {
                                writeln!(out, "done ({search_states_count} states)").ok();
                                print_candidates(&ranked, &mut out);
                                sess.requirement = combined;
                                sess.candidates = ranked;
                                sess.selected = None;
                            }
                            Err(e) => {
                                writeln!(out, "\nError: {e}").ok();
                            }
                        }
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
                                    writeln!(out, "{}", build_markdown(&sess.requirement, 0, &[display])).ok();
                                }
                                _ => {
                                    // テキスト表示
                                    let ev = &c.state.world_state.evaluation;
                                    writeln!(out, "Score: {:.4}", c.score).ok();
                                    write!(out, "{}", render_evaluation(ev)).ok();
                                }
                            }
                        } else {
                            writeln!(out, "No candidate selected.").ok();
                        }
                    }
                    "save" => {
                        let out_path = parts.get(1).copied().unwrap_or("design_session.json");
                        let saved = build_saved_design(sess);
                        if let Err(e) = save_design_file(&saved, Path::new(out_path)) {
                            writeln!(out, "Error: {e}").ok();
                        } else {
                            writeln!(out, "Saved to {out_path}").ok();
                        }
                    }
                    "list" | "ls" => {
                        print_candidates(&sess.candidates, &mut out);
                    }
                    _ => {
                        writeln!(out, "Unknown command. Type 'help' for available commands.").ok();
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
        writeln!(out, "  <requirement>   Generate architecture candidates").ok();
    }
    writeln!(out, "  s <N>           Select candidate N").ok();
    writeln!(out, "  r               Refine with additional requirement").ok();
    writeln!(out, "  list            Show all candidates").ok();
    writeln!(out, "  m               Show Mermaid diagram for selected candidate").ok();
    writeln!(out, "  e [fmt]         Export selected candidate (text|mermaid|markdown)").ok();
    writeln!(out, "  save [path]     Save session to design JSON file").ok();
    writeln!(out, "  q / quit        Exit").ok();
    if let Some(idx) = selected {
        writeln!(out, "  (currently selected: candidate {})", idx + 1).ok();
    }
}

fn print_candidates(
    candidates: &[RankedCandidate],
    out: &mut impl Write,
) {
    for (i, c) in candidates.iter().enumerate() {
        let (names, _) = candidate_to_display_parts(c);
        writeln!(
            out,
            "  Candidate {} (score: {:.4}, pareto: {}) — {} components",
            i + 1,
            c.score,
            c.pareto_rank,
            names.len()
        ).ok();
    }
    writeln!(out).ok();
    writeln!(out, "Use 's <N>' to select a candidate.").ok();
}

fn candidate_to_display_parts(
    c: &RankedCandidate,
) -> (Vec<String>, Vec<(String, String)>) {
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

    let candidates = sess.candidates.iter().enumerate().map(|(i, c)| {
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
    }).collect();

    SavedDesign {
        version: "1.0".to_string(),
        generated_at: format!("{now}"),
        input: sess.requirement.clone(),
        search_states: 0,
        candidates,
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
}
