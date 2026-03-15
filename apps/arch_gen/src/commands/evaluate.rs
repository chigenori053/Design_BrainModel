use std::path::Path;

use crate::input_bridge::load_design_file;
use crate::output::text::render_evaluation;

/// `evaluate` コマンド: 保存済み design.json を読み込んでスコアを表示する。
pub fn run(design_file: &str) -> Result<(), String> {
    let path = Path::new(design_file);
    let design = load_design_file(path)?;

    println!("Architecture Evaluation");
    println!("{}", "═".repeat(55));
    println!("File:          {}", path.display());
    println!("Input:         \"{}\"", design.input);
    println!("Generated at:  {}", design.generated_at);
    println!("Search states: {}", design.search_states);
    println!("Candidates:    {}", design.candidates.len());
    println!();

    for c in &design.candidates {
        println!(
            "─── Candidate {} (Score: {:.4}, Pareto rank: {}) {}",
            c.id,
            c.score,
            c.pareto_rank,
            "─".repeat(20)
        );

        // コンポーネント / 依存関係
        if !c.components.is_empty() {
            if !c.dependencies.is_empty() {
                println!("  Components ({}):", c.components.len());
                for dep in &c.dependencies {
                    println!("    {} → {}", dep[0], dep[1]);
                }
            } else {
                println!("  Components ({}):", c.components.len());
                for comp in &c.components {
                    println!("    {comp}");
                }
            }
        }

        // EvaluationVector を SavedEvaluation から再構築して表示
        let eval = world_model_core::EvaluationVector {
            structural_quality: c.evaluation.structural_quality,
            dependency_quality: c.evaluation.dependency_quality,
            constraint_satisfaction: c.evaluation.constraint_satisfaction,
            complexity: c.evaluation.complexity,
            simulation_quality: c.evaluation.simulation_quality,
        };
        print!("  {}", render_evaluation(&eval));
        println!();
    }

    println!("{}", "═".repeat(55));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{SavedCandidate, SavedDesign, SavedEvaluation, save_design_file};

    fn make_design() -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "1742000000".to_string(),
            input: "テスト要件".to_string(),
            search_states: 20,
            candidates: vec![SavedCandidate {
                id: 1,
                score: 0.75,
                pareto_rank: 0,
                evaluation: SavedEvaluation {
                    structural_quality: 0.9,
                    dependency_quality: 0.7,
                    constraint_satisfaction: 0.8,
                    complexity: 0.4,
                    simulation_quality: 0.8,
                    total: 0.72,
                },
                components: vec!["service_1".to_string()],
                dependencies: vec![],
                code_metrics: Default::default(),
            }],
        }
    }

    #[test]
    fn test_evaluate_missing_file_is_error() {
        let result = run("/nonexistent/design.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_valid_file_succeeds() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let design = make_design();
        save_design_file(&design, tmp.path()).unwrap();
        let result = run(tmp.path().to_str().unwrap());
        assert!(result.is_ok());
    }
}
