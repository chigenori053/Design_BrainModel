use std::path::Path;

use code_ir::{ArchitectureToCodeIR, CodeGenerator, DeterministicArchitectureToCodeIR, DeterministicCodeGenerator};
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

use crate::input_bridge::{
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation,
    arch_state_to_architecture, resolve_requirement, save_design_file,
};
use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::plantuml::build_plantuml;
use crate::output::source_writer::{OutputLayout, OutputStrategy, write_source_tree_with_options};
use crate::output::text::{CandidateDisplay, GenerationSummary, render_summary};

pub struct GenerateArgs {
    pub requirement: String,
    pub candidates: usize,
    pub output_dir: String,
    pub format: String,
    pub beam_width: usize,
    pub max_depth: usize,
    pub no_code: bool,
    pub verbose: bool,
    pub output_strategy: String,
    pub output_layout: String,
}

pub fn run(args: GenerateArgs) -> Result<(), String> {
    let raw_text = resolve_requirement(&args.requirement)?;

    let req = GenerateRequest::new(
        raw_text,
        args.beam_width,
        args.max_depth,
        args.candidates,
        args.no_code,
        args.verbose,
    );

    let PipelineResult { ranked, search_states_count } = run_phase9_pipeline(&req)?;
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
            build_candidate(i + 1, candidate, output_dir, req.no_code, &strategy, &layout)
        })
        .collect::<Result<_, _>>()?;

    render_output(&args.format, req.input_text(), &built, search_states_count, frontier_size)?;
    save_design_json(&req, &built, search_states_count, output_dir)?;

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
        let written = write_source_tree_with_options(&source_tree, output_dir, id, strategy, layout)?;
        eprintln!(
            "[arch-gen] candidate {id} → {} file(s) written to {}/candidate_{id}/",
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

    Ok(BuiltCandidate { ranked: candidate, display })
}

// ─── Phase9 パイプライン ─────────────────────────────────────────────────────

pub struct PipelineResult {
    pub ranked: Vec<RankedCandidate>,
    pub search_states_count: usize,
}

pub fn run_phase9_pipeline(req: &GenerateRequest) -> Result<PipelineResult, String> {
    if req.verbose {
        eprintln!("[arch-gen] input: {}", req.input_text());
        eprintln!("[arch-gen] beam_width={}, max_depth={}", req.beam_width, req.max_depth);
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
        eprintln!("[arch-gen] Phase9 stage: {:?}", phase9_ctx.stage);
        eprintln!(
            "[arch-gen] recalled memories: {}",
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

    if req.verbose {
        eprintln!("[arch-gen] search states: {search_states_count}");
        eprintln!("[arch-gen] ranked candidates: {}", ranked.len());
    }

    Ok(PipelineResult { ranked, search_states_count })
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
                println!("{}", build_mermaid(&b.display.component_names, &b.display.dependency_pairs));
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
                println!("' --- Candidate {} (score: {:.4}) ---", i + 1, b.ranked.score);
                println!(
                    "{}",
                    build_plantuml(&b.display.component_names, &b.display.dependency_pairs)
                );
            }
        }
        _ => {
            let displays: Vec<CandidateDisplay> = built.iter().map(|b| b.display.clone()).collect();
            let summary = GenerationSummary {
                input,
                search_states,
                frontier_size,
                candidates: &displays,
            };
            print!("{}", render_summary(&summary));
        }
    }
    Ok(())
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
                    dependencies: b.display.dependency_pairs.iter()
                        .map(|(f, t)| [f.clone(), t.clone()])
                        .collect(),
                    code_metrics: Default::default(),
                }
            })
            .collect(),
    };

    let design_path = output_dir.join("design.json");
    save_design_file(&saved, &design_path)?;
    eprintln!("[arch-gen] design saved to {}", design_path.display());
    Ok(())
}
