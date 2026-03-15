use std::path::Path;

use design_search_engine::{
    BeamSearchController, SearchConfig as DesignSearchConfig, SearchController as _,
    rank_candidates,
};
use runtime_vm::{ExecutionMode as RuntimeExecutionMode, HybridVm as RuntimeHybridVm, Phase9RuntimeAdapter};
use world_model_core::{
    ConsistencyEvaluator, DeltaConsistencyEvaluator, DeterministicWorldModel, HypothesisGenerator,
    SimpleHypothesisGenerator, WorldModel,
};

use crate::input_bridge::resolve_requirement;
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
    let text = resolve_requirement(&args.requirement)?;

    if args.verbose {
        eprintln!("[arch-gen] input: {text}");
        eprintln!("[arch-gen] beam_width={}, max_depth={}", args.beam_width, args.max_depth);
    }

    // Phase9 パイプライン
    let mut vm = RuntimeHybridVm::new(RuntimeExecutionMode::Reasoning);
    vm.set_input_text(text.clone());
    vm.execute();

    let phase9_ctx = Phase9RuntimeAdapter::from_legacy(vm.context());
    let current_state = phase9_ctx
        .world_state
        .clone()
        .unwrap_or_else(|| world_model_core::WorldState::new(1, vec![1.0, 1.0, 1.0]));

    if args.verbose {
        eprintln!("[arch-gen] Phase9 stage: {:?}", phase9_ctx.stage);
        eprintln!("[arch-gen] recalled memories: {}", Phase9RuntimeAdapter::snapshot(vm.context()).recalled_memories);
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

    // BeamSearch
    let search_controller = BeamSearchController::default();
    let mut search_config = DesignSearchConfig::default();
    search_config.beam_width = args.beam_width;
    search_config.max_depth = args.max_depth;

    let search_states = search_controller.search(
        current_state.clone(),
        phase9_ctx.recall_result.as_ref(),
        &search_config,
    );
    let ranked = rank_candidates(search_states.clone());

    if args.verbose {
        eprintln!("[arch-gen] search states: {}", search_states.len());
        eprintln!("[arch-gen] ranked candidates: {}", ranked.len());
    }

    let top: Vec<_> = ranked.into_iter().take(args.candidates).collect();
    let frontier_size = top.len();

    // 出力
    let output_dir = Path::new(&args.output_dir);

    match args.format.as_str() {
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
                    "input": text,
                    "search_states": search_states.len(),
                    "candidates": data,
                }))
                .map_err(|e| format!("json serialization failed: {e}"))?
            );
        }
        _ => {
            // "text" (デフォルト)
            let summary = GenerationSummary {
                input: &text,
                search_states: search_states.len(),
                frontier_size,
                candidates: &top,
            };
            print!("{}", render_summary(&summary));
        }
    }

    // コード生成（--no-code でない場合）
    if !args.no_code {
        for (i, candidate) in top.iter().enumerate() {
            let source_tree = generate_stub_source_tree(candidate);
            let written = write_source_tree(&source_tree, output_dir, i + 1)?;
            if !written.is_empty() {
                eprintln!(
                    "[arch-gen] candidate {} → {} file(s) written to {}/candidate_{}/",
                    i + 1,
                    written.len(),
                    args.output_dir,
                    i + 1
                );
            }
        }
    }

    Ok(())
}

/// `ArchitectureState` から最低限のRustソーススタブを生成する。
/// Phase2以降でDeterministicCodeGeneratorに差し替える。
fn generate_stub_source_tree(
    candidate: &design_search_engine::RankedCandidate,
) -> SourceTree {
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
