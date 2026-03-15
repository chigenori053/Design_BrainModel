use std::path::Path;

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
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation, resolve_requirement,
    save_design_file,
};
use crate::output::mermaid::candidate_to_mermaid;
use crate::output::source_writer::{GeneratedFile, SourceTree, write_source_tree};
use crate::output::text::{GenerationSummary, render_summary};

pub struct GenerateArgs {
    pub requirement: String,
    pub candidates: usize,
    pub output_dir: String,
    pub format: String,
    pub beam_width: usize,
    pub max_depth: usize,
    pub no_code: bool,
    pub verbose: bool,
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

    render_output(&args.format, req.input_text(), &top, search_states_count)?;

    if !req.no_code {
        write_candidates(&top, output_dir, &args.output_dir)?;
    }

    save_design_json(&req, &top, search_states_count, output_dir)?;

    Ok(())
}

// ─── パイプライン実行 ──────────────────────────────────────────────────────

struct PipelineResult {
    ranked: Vec<RankedCandidate>,
    search_states_count: usize,
}

fn run_phase9_pipeline(req: &GenerateRequest) -> Result<PipelineResult, String> {
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
    let evaluator = DeltaConsistencyEvaluator;
    let _consistency = evaluator
        .evaluate(&current_state, &prediction)
        .map_err(|e| format!("consistency evaluation failed: {e}"))?;

    let search_controller = BeamSearchController::default();
    let mut search_config = DesignSearchConfig::default();
    search_config.beam_width = req.beam_width;
    search_config.max_depth = req.max_depth;

    let search_states = search_controller.search(
        current_state.clone(),
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
    top: &[RankedCandidate],
    search_states: usize,
) -> Result<(), String> {
    match format {
        "mermaid" => {
            for (i, candidate) in top.iter().enumerate() {
                println!("--- Candidate {} (score: {:.4}) ---", i + 1, candidate.score);
                println!("{}", candidate_to_mermaid(candidate));
            }
        }
        "json" => {
            let data: Vec<serde_json::Value> = top
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    serde_json::json!({
                        "candidate": i + 1,
                        "score": c.score,
                        "pareto_rank": c.pareto_rank,
                        "evaluation": {
                            "structural_quality": c.state.world_state.evaluation.structural_quality,
                            "dependency_quality": c.state.world_state.evaluation.dependency_quality,
                            "constraint_satisfaction": c.state.world_state.evaluation.constraint_satisfaction,
                            "complexity": c.state.world_state.evaluation.complexity,
                            "simulation_quality": c.state.world_state.evaluation.simulation_quality,
                            "total": c.state.world_state.evaluation.total(),
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
        _ => {
            let summary = GenerationSummary {
                input,
                search_states,
                frontier_size: top.len(),
                candidates: top,
            };
            print!("{}", render_summary(&summary));
        }
    }
    Ok(())
}

fn write_candidates(
    top: &[RankedCandidate],
    output_dir: &Path,
    output_dir_str: &str,
) -> Result<(), String> {
    for (i, candidate) in top.iter().enumerate() {
        let source_tree = generate_stub_source_tree(candidate);
        let written = write_source_tree(&source_tree, output_dir, i + 1)?;
        if !written.is_empty() {
            eprintln!(
                "[arch-gen] candidate {} → {} file(s) written to {}/candidate_{}/",
                i + 1,
                written.len(),
                output_dir_str,
                i + 1
            );
        }
    }
    Ok(())
}

fn save_design_json(
    req: &GenerateRequest,
    top: &[RankedCandidate],
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
        candidates: top
            .iter()
            .enumerate()
            .map(|(i, c)| SavedCandidate {
                id: i + 1,
                score: c.score,
                pareto_rank: c.pareto_rank,
                evaluation: SavedEvaluation {
                    structural_quality: c.state.world_state.evaluation.structural_quality,
                    dependency_quality: c.state.world_state.evaluation.dependency_quality,
                    constraint_satisfaction: c.state.world_state.evaluation.constraint_satisfaction,
                    complexity: c.state.world_state.evaluation.complexity,
                    simulation_quality: c.state.world_state.evaluation.simulation_quality,
                    total: c.state.world_state.evaluation.total(),
                },
            })
            .collect(),
    };

    let design_path = output_dir.join("design.json");
    save_design_file(&saved, &design_path)?;
    eprintln!("[arch-gen] design saved to {}", design_path.display());
    Ok(())
}

// ─── コード生成（スタブ） ────────────────────────────────────────────────────

fn generate_stub_source_tree(candidate: &RankedCandidate) -> SourceTree {
    let arch = &candidate.state.architecture_state;
    let mut files = Vec::new();

    for comp in &arch.components {
        let name = format!("{:?}", comp.id)
            .to_ascii_lowercase()
            .replace(['(', ')'], "");
        files.push(GeneratedFile {
            path: format!("src/{name}.rs"),
            contents: format!(
                "// Component: {name}\n// Role: {:?}\n\npub struct {};\n\nimpl {} {{\n    pub fn new() -> Self {{ Self }}\n}}\n",
                comp.role,
                pascal_case(&name),
                pascal_case(&name),
            ),
        });
    }

    if files.is_empty() {
        files.push(GeneratedFile {
            path: "src/lib.rs".to_string(),
            contents: "// Generated by arch-gen\n".to_string(),
        });
    }

    SourceTree { files }
}

fn pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}
