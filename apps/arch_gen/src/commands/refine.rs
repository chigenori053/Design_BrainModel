use std::path::Path;

use crate::commands::generate::{PipelineResult, run_phase9_pipeline};
use crate::input_bridge::{
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation,
    arch_state_to_architecture, load_design_file, save_design_file,
};
use crate::output::text::render_evaluation;
use code_ir::{ArchitectureToCodeIR, DeterministicArchitectureToCodeIR};

/// `refine` コマンド: 既存設計に追加要件を合成してパイプラインを再実行し、
/// 改良版設計ファイルを `design_refined.json` として保存する。
pub fn run(design_file: &str, additional_requirement: &str) -> Result<(), String> {
    let path = Path::new(design_file);
    let design = load_design_file(path)?;

    // 元の要件 + 追加要件を合成
    let combined_input = format!("{}\n{}", design.input.trim(), additional_requirement.trim());
    eprintln!("[arch-gen] refine: combined input:");
    eprintln!("  original:    \"{}\"", design.input.trim());
    eprintln!("  additional:  \"{additional_requirement}\"");

    let req = GenerateRequest::new(
        combined_input,
        /* beam_width */ 10,
        /* max_depth  */ 5,
        /* candidates */ design.candidates.len().max(1),
        /* no_code    */ true,
        /* verbose    */ false,
    );

    let PipelineResult { ranked, search_states_count } = run_phase9_pipeline(&req)?;
    let top: Vec<_> = ranked.into_iter().take(req.candidates).collect();

    eprintln!(
        "[arch-gen] refine: {} search states → {} candidates",
        search_states_count,
        top.len()
    );

    // 結果をサマリー表示
    println!("Refine Result");
    println!("{}", "═".repeat(55));
    println!("Original input:   \"{}\"", design.input);
    println!("Additional:       \"{additional_requirement}\"");
    println!("Search states:    {search_states_count}");
    println!("New candidates:   {}", top.len());
    println!();

    // SavedDesign として保存
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let saved_candidates: Vec<SavedCandidate> = top
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let eval = &c.state.world_state.evaluation;
            let architecture = arch_state_to_architecture(&c.state.architecture_state);
            let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);
            let component_names: Vec<String> =
                code_ir.modules.iter().map(|m| m.name.clone()).collect();
            let units_by_id = architecture.design_units_by_id();
            let dependencies: Vec<[String; 2]> = architecture
                .dependencies
                .iter()
                .filter_map(|dep| {
                    let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
                    let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
                    Some([from, to])
                })
                .collect();

            // 表示
            let ev = world_model_core::EvaluationVector {
                structural_quality: eval.structural_quality,
                dependency_quality: eval.dependency_quality,
                constraint_satisfaction: eval.constraint_satisfaction,
                complexity: eval.complexity,
                simulation_quality: eval.simulation_quality,
            };
            println!("─── Candidate {} (Score: {:.4}) {}", i + 1, c.score, "─".repeat(28));
            for comp in &component_names {
                println!("  {comp}");
            }
            print!("{}", render_evaluation(&ev));
            println!();

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
                components: component_names,
                dependencies,
                code_metrics: Default::default(),
            }
        })
        .collect();

    let refined = SavedDesign {
        version: "1.0".to_string(),
        generated_at: format!("{now}"),
        input: req.input_text().to_string(),
        search_states: search_states_count,
        candidates: saved_candidates,
    };

    // 元ファイルと同じディレクトリに design_refined.json を保存
    let out_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("design_refined.json");
    save_design_file(&refined, &out_path)?;
    println!("{}", "═".repeat(55));
    eprintln!("[arch-gen] refined design saved to {}", out_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{
        SavedCandidate, SavedCodeMetrics, SavedDesign, SavedEvaluation,
    };

    fn make_minimal_design() -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "1742000000".to_string(),
            input: "ECサイトを設計する".to_string(),
            search_states: 10,
            candidates: vec![SavedCandidate {
                id: 1,
                score: 0.80,
                pareto_rank: 0,
                evaluation: SavedEvaluation {
                    structural_quality: 0.9,
                    dependency_quality: 0.8,
                    constraint_satisfaction: 0.9,
                    complexity: 0.3,
                    simulation_quality: 0.9,
                    total: 0.76,
                },
                components: vec!["service_1".to_string()],
                dependencies: vec![],
                code_metrics: SavedCodeMetrics::default(),
            }],
        }
    }

    #[test]
    fn test_refine_missing_file_is_error() {
        let result = run("/nonexistent/design.json", "追加要件");
        assert!(result.is_err());
    }

    #[test]
    fn test_refine_produces_refined_json() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let design_path = tmp_dir.path().join("design.json");
        let design = make_minimal_design();
        save_design_file(&design, &design_path).unwrap();

        let result = run(design_path.to_str().unwrap(), "認証機能を追加する");
        assert!(result.is_ok(), "{:?}", result);

        // design_refined.json が同じディレクトリに生成されているか確認
        let refined_path = tmp_dir.path().join("design_refined.json");
        assert!(refined_path.exists(), "design_refined.json should exist");

        let refined = load_design_file(&refined_path).unwrap();
        assert!(refined.input.contains("ECサイトを設計する"));
        assert!(refined.input.contains("認証機能を追加する"));
    }
}
