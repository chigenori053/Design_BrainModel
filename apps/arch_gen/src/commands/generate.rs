use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::Path;

use code_ir::{
    ArchitectureToCodeIR, CodeGenerator, DeterministicArchitectureToCodeIR,
    DeterministicCodeGenerator,
};
use design_search_engine::{
    BeamSearchController, RankedCandidate, SearchConfig as DesignSearchConfig,
    SearchController as _, rank_candidates,
};
use runtime_vm::{
    ExecutionMode as RuntimeExecutionMode, HybridVm as RuntimeHybridVm, Phase9RuntimeAdapter,
};
use world_model_core::{
    ConsistencyEvaluator, DeltaConsistencyEvaluator, DeterministicWorldModel, HypothesisGenerator,
    SimpleHypothesisGenerator, WorldModel,
};

use crate::commands::knowledge_layer::prepare_inference_input;
use crate::input_bridge::{
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation, arch_state_to_architecture,
    resolve_requirement, save_design_file,
};
use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::narrative::verbalize_candidate;
use crate::output::plantuml::build_plantuml;
use crate::output::source_writer::{OutputLayout, OutputStrategy, write_source_tree_with_options};
use crate::output::text::CandidateDisplay;

pub struct GenerateArgs {
    pub requirement: String,
    pub candidates: usize,
    pub output_dir: String,
    pub format: String,
    pub beam_width: usize,
    pub max_depth: usize,
    pub no_code: bool,
    pub write_files: bool,
    pub verbose: bool,
    pub output_strategy: String,
    pub output_layout: String,
}

pub fn run(args: GenerateArgs) -> Result<(), String> {
    let raw_text = resolve_requirement(&args.requirement)?;
    let knowledge_context = prepare_inference_input(&raw_text)?;
    let should_write_files = args.write_files && !args.no_code;
    let no_code = !should_write_files;

    let inference_req = GenerateRequest::new(
        knowledge_context.enriched_requirement,
        args.beam_width,
        args.max_depth,
        args.candidates,
        no_code,
        args.verbose,
    );
    let req = GenerateRequest::new(
        raw_text,
        args.beam_width,
        args.max_depth,
        args.candidates,
        no_code,
        args.verbose,
    );

    let PipelineResult {
        ranked,
        search_states_count,
    } = run_phase9_pipeline(&inference_req)?;
    let top: Vec<_> = ranked.into_iter().take(req.candidates).collect();
    let frontier_size = top.len();
    let output_dir = Path::new(&args.output_dir);

    let strategy = OutputStrategy::from_str(&args.output_strategy)?;
    let layout = OutputLayout::from_str(&args.output_layout)?;

    // 各候補を Architecture → CodeIR → SourceTree に変換
    let built: Vec<BuiltCandidate> = top
        .into_iter()
        .enumerate()
        .map(|(i, candidate): (usize, RankedCandidate)| {
            build_candidate(
                i + 1,
                candidate,
                output_dir,
                req.no_code,
                &strategy,
                &layout,
            )
        })
        .collect::<Result<_, _>>()?;

    render_output(
        &args.format,
        req.input_text(),
        &built,
        search_states_count,
        frontier_size,
    )?;
    save_design_json(&req, &built, search_states_count, output_dir)?;

    if should_enter_chat(&args.format) {
        let ranked = built.iter().map(|b| b.ranked.clone()).collect();
        crate::commands::interactive::run_seeded(req.input_text().to_string(), ranked)?;
    }

    Ok(())
}

// ─── 候補の変換・コード生成 ──────────────────────────────────────────────────

struct BuiltCandidate {
    ranked: RankedCandidate,
    display: CandidateDisplay,
}

fn build_candidate(
    id: usize,
    candidate: RankedCandidate,
    output_dir: &Path,
    no_code: bool,
    strategy: &OutputStrategy,
    layout: &OutputLayout,
) -> Result<BuiltCandidate, String> {
    let architecture = arch_state_to_architecture(&candidate.state.architecture_state);
    let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);

    // コンポーネント名・依存関係を CodeIR から取得
    let component_names: Vec<String> = code_ir.modules.iter().map(|m| m.name.clone()).collect();
    let units_by_id = architecture.design_units_by_id();
    let dependency_pairs: Vec<(String, String)> = architecture
        .dependencies
        .iter()
        .filter_map(|dep| {
            let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
            let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
            Some((from, to))
        })
        .collect();

    let generated_files = if no_code {
        vec![]
    } else {
        let source_tree = DeterministicCodeGenerator::generate(&code_ir);
        let written =
            write_source_tree_with_options(&source_tree, output_dir, id, strategy, layout)?;
        eprintln!(
            "[arch_gen] candidate {id} → {} file(s) written to {}/candidate_{id}/",
            written.len(),
            output_dir.display()
        );
        written.iter().map(|p| p.display().to_string()).collect()
    };

    let display = CandidateDisplay {
        score: candidate.score,
        pareto_rank: candidate.pareto_rank,
        component_names,
        dependency_pairs,
        evaluation: candidate.state.world_state.evaluation.clone(),
        generated_files,
    };

    Ok(BuiltCandidate {
        ranked: candidate,
        display,
    })
}

// ─── Phase9 パイプライン ─────────────────────────────────────────────────────

pub struct PipelineResult {
    pub ranked: Vec<RankedCandidate>,
    pub search_states_count: usize,
}

pub fn run_phase9_pipeline(req: &GenerateRequest) -> Result<PipelineResult, String> {
    if req.verbose {
        eprintln!("[arch_gen] input: {}", req.input_text());
        eprintln!(
            "[arch_gen] beam_width={}, max_depth={}",
            req.beam_width, req.max_depth
        );
    }

    let mut vm = RuntimeHybridVm::new(RuntimeExecutionMode::Reasoning);
    vm.set_input_text(req.input_text().to_string());
    vm.execute();

    let phase9_ctx = Phase9RuntimeAdapter::from_legacy(vm.context());
    let current_state = phase9_ctx
        .world_state
        .clone()
        .unwrap_or_else(|| world_model_core::WorldState::new(1, vec![1.0, 1.0, 1.0]));

    if req.verbose {
        eprintln!("[arch_gen] Phase9 stage: {:?}", phase9_ctx.stage);
        eprintln!(
            "[arch_gen] recalled memories: {}",
            Phase9RuntimeAdapter::snapshot(vm.context()).recalled_memories
        );
    }

    let generator = SimpleHypothesisGenerator;
    let generated = generator
        .generate(&current_state, phase9_ctx.recall_result.as_ref())
        .map_err(|e| format!("hypothesis generation failed: {e}"))?;
    let selected = generated
        .first()
        .cloned()
        .ok_or_else(|| "no hypotheses generated".to_string())?;

    let model = DeterministicWorldModel;
    let prediction = model
        .transition(&current_state, &selected)
        .map_err(|e| format!("world model transition failed: {e}"))?;
    let _consistency = DeltaConsistencyEvaluator
        .evaluate(&current_state, &prediction)
        .map_err(|e| format!("consistency evaluation failed: {e}"))?;

    let search_controller = BeamSearchController::default();
    let mut search_config = DesignSearchConfig::default();
    search_config.beam_width = req.beam_width;
    search_config.max_depth = req.max_depth;

    let search_states = search_controller.search(
        current_state,
        phase9_ctx.recall_result.as_ref(),
        &search_config,
    );
    let search_states_count = search_states.len();
    let ranked = rank_candidates(search_states);
    let ranked_count = ranked.len();
    let ranked = dedup_ranked_candidates(ranked);

    if req.verbose {
        eprintln!("[arch_gen] search states: {search_states_count}");
        eprintln!("[arch_gen] ranked candidates: {ranked_count}");
        eprintln!("[arch_gen] unique candidates: {}", ranked.len());
    }

    Ok(PipelineResult {
        ranked,
        search_states_count,
    })
}

// ─── 出力 ────────────────────────────────────────────────────────────────────

fn render_output(
    format: &str,
    input: &str,
    built: &[BuiltCandidate],
    search_states: usize,
    frontier_size: usize,
) -> Result<(), String> {
    match format {
        "mermaid" => {
            for (i, b) in built.iter().enumerate() {
                println!("--- Candidate {} (score: {:.4}) ---", i + 1, b.ranked.score);
                println!(
                    "{}",
                    build_mermaid(&b.display.component_names, &b.display.dependency_pairs)
                );
            }
        }
        "json" => {
            let data: Vec<serde_json::Value> = built
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let eval = &b.ranked.state.world_state.evaluation;
                    serde_json::json!({
                        "candidate": i + 1,
                        "score": b.ranked.score,
                        "pareto_rank": b.ranked.pareto_rank,
                        "components": b.display.component_names,
                        "evaluation": {
                            "structural_quality": eval.structural_quality,
                            "dependency_quality": eval.dependency_quality,
                            "constraint_satisfaction": eval.constraint_satisfaction,
                            "complexity": eval.complexity,
                            "simulation_quality": eval.simulation_quality,
                            "total": eval.total(),
                        }
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "input": input,
                    "search_states": search_states,
                    "candidates": data,
                }))
                .map_err(|e| format!("json serialization failed: {e}"))?
            );
        }
        "markdown" => {
            let displays: Vec<CandidateDisplay> = built.iter().map(|b| b.display.clone()).collect();
            print!("{}", build_markdown(input, search_states, &displays));
        }
        "plantuml" => {
            for (i, b) in built.iter().enumerate() {
                println!(
                    "' --- Candidate {} (score: {:.4}) ---",
                    i + 1,
                    b.ranked.score
                );
                println!(
                    "{}",
                    build_plantuml(&b.display.component_names, &b.display.dependency_pairs)
                );
            }
        }
        _ => {
            let _ = (search_states, frontier_size);
            print!("{}", render_design_review(input, built));
        }
    }
    Ok(())
}

fn render_design_review(input: &str, built: &[BuiltCandidate]) -> String {
    let mut out = String::new();
    out.push_str("Design Conversation\n");
    out.push_str(&"─".repeat(55));
    out.push('\n');
    out.push_str("まずは実装に進まず、方向性の違う設計案を会話しながら絞り込みます。\n");
    out.push_str("数値や探索過程は隠し、設計上の違いだけを要約して提示します。\n\n");

    for (i, built) in built.iter().enumerate() {
        let saved = to_saved_candidate(i + 1, built);
        out.push_str(&format!(
            "案 {}: {}\n",
            i + 1,
            candidate_title(&built.display)
        ));
        out.push_str(&verbalize_candidate(input, &saved));
        out.push('\n');
        if !built.display.component_names.is_empty() {
            out.push_str("含まれる要素: ");
            out.push_str(&built.display.component_names.join(", "));
            out.push('\n');
        }
        if built.display.dependency_pairs.is_empty() {
            out.push_str("まだ依存関係が見えていないため、画面更新・編集バッファ・保存・プラグイン境界の整理が必要です。\n");
        } else {
            out.push_str("見えている関係:\n");
            for (from, to) in &built.display.dependency_pairs {
                out.push_str(&format!("  - {from} -> {to}\n"));
            }
        }
        out.push('\n');
    }

    out.push_str("次の進め方\n");
    out.push_str(&"─".repeat(55));
    out.push('\n');
    out.push_str("- どの案をベースにしたいか決めたら `s 1` のように選択してください。\n");
    out.push_str("- 追加したい要求をそのまま文章で入力すると、次の対話で再探索します。\n");
    out.push_str(
        "- 設計が固まってからファイル生成したい場合だけ `--write-files` を使ってください。\n",
    );
    out
}

fn candidate_title(display: &CandidateDisplay) -> String {
    let has_database = display
        .component_names
        .iter()
        .any(|name| name.contains("database"));
    let has_repository = display
        .component_names
        .iter()
        .any(|name| name.contains("repository"));
    let has_controller = display
        .component_names
        .iter()
        .any(|name| name.contains("controller"));
    let has_service = display
        .component_names
        .iter()
        .any(|name| name.contains("service"));

    match (has_service, has_controller, has_repository, has_database) {
        (true, true, true, true) => "アプリ層を分けた標準的な構成".to_string(),
        (true, true, false, _) => "UI とアプリロジックを近くに置いた軽量構成".to_string(),
        (true, false, true, true) => "データ境界を重視した構成".to_string(),
        _ => "最小要素でまとめた構成".to_string(),
    }
}

fn save_design_json(
    req: &GenerateRequest,
    built: &[BuiltCandidate],
    search_states_count: usize,
    output_dir: &Path,
) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let saved = SavedDesign {
        version: "1.0".to_string(),
        generated_at: format!("{now}"),
        input: req.input_text().to_string(),
        search_states: search_states_count,
        candidates: built
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let eval = &b.ranked.state.world_state.evaluation;
                SavedCandidate {
                    id: i + 1,
                    score: b.ranked.score,
                    pareto_rank: b.ranked.pareto_rank,
                    evaluation: SavedEvaluation {
                        structural_quality: eval.structural_quality,
                        dependency_quality: eval.dependency_quality,
                        constraint_satisfaction: eval.constraint_satisfaction,
                        complexity: eval.complexity,
                        simulation_quality: eval.simulation_quality,
                        total: eval.total(),
                    },
                    components: b.display.component_names.clone(),
                    dependencies: b
                        .display
                        .dependency_pairs
                        .iter()
                        .map(|(f, t)| [f.clone(), t.clone()])
                        .collect(),
                    code_metrics: Default::default(),
                }
            })
            .collect(),
    };

    let design_path = output_dir.join("design.json");
    save_design_file(&saved, &design_path)?;
    eprintln!("[arch_gen] design saved to {}", design_path.display());
    Ok(())
}

fn to_saved_candidate(id: usize, built: &BuiltCandidate) -> SavedCandidate {
    let eval = &built.ranked.state.world_state.evaluation;
    SavedCandidate {
        id,
        score: built.ranked.score,
        pareto_rank: built.ranked.pareto_rank,
        evaluation: SavedEvaluation {
            structural_quality: eval.structural_quality,
            dependency_quality: eval.dependency_quality,
            constraint_satisfaction: eval.constraint_satisfaction,
            complexity: eval.complexity,
            simulation_quality: eval.simulation_quality,
            total: eval.total(),
        },
        components: built.display.component_names.clone(),
        dependencies: built
            .display
            .dependency_pairs
            .iter()
            .map(|(from, to)| [from.clone(), to.clone()])
            .collect(),
        code_metrics: Default::default(),
    }
}

fn should_enter_chat(format: &str) -> bool {
    format == "text" && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn dedup_ranked_candidates(ranked: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();

    for candidate in ranked {
        let signature = candidate_signature(&candidate);
        if seen.insert(signature) {
            unique.push(candidate);
        }
    }

    unique
}

fn candidate_signature(candidate: &RankedCandidate) -> String {
    let architecture = arch_state_to_architecture(&candidate.state.architecture_state);
    let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);

    let mut names: Vec<String> = code_ir.modules.iter().map(|m| m.name.clone()).collect();
    names.sort();

    let units_by_id = architecture.design_units_by_id();
    let mut deps: Vec<String> = architecture
        .dependencies
        .iter()
        .filter_map(|dep| {
            let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
            let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
            Some(format!("{from}->{to}"))
        })
        .collect();
    deps.sort();

    format!("{}||{}", names.join("|"), deps.join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_phase9_pipeline_returns_unique_candidates() {
        let req = GenerateRequest::new(
            "NeoVim風のEditorを作成したい".to_string(),
            10,
            5,
            10,
            true,
            false,
        );

        let result = run_phase9_pipeline(&req).expect("pipeline should succeed");
        let signatures: BTreeSet<String> = result.ranked.iter().map(candidate_signature).collect();

        assert_eq!(
            signatures.len(),
            result.ranked.len(),
            "pipeline output should already be deduplicated"
        );
    }
}
