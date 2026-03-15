use std::path::Path;

use crate::input_bridge::{SavedCandidate, load_design_file};
use crate::output::narrative::verbalize_candidate;

/// `explain` コマンド: 保存済み design.json を読み込み設計の説明テキストを生成する。
pub fn run(design_file: &str) -> Result<(), String> {
    let path = Path::new(design_file);
    let design = load_design_file(path)?;

    println!("Architecture Explanation");
    println!("{}", "═".repeat(55));
    println!("File:  {}", path.display());
    println!("Input: \"{}\"", design.input);
    println!();

    for c in &design.candidates {
        println!(
            "─── Candidate {} (Score: {:.4}) {}",
            c.id,
            c.score,
            "─".repeat(30)
        );
        println!();

        // パターン推定
        let pattern = detect_pattern(c);
        println!("Design Pattern: {pattern}");
        println!();

        // コンポーネント構成
        println!("Component Overview ({} components):", c.components.len());
        for comp in &c.components {
            println!("  - {comp}");
        }
        if !c.dependencies.is_empty() {
            println!();
            println!("Key Dependencies:");
            for dep in &c.dependencies {
                println!("  {} → {}", dep[0], dep[1]);
            }
        }
        println!();

        // 品質分析
        println!("Quality Analysis:");
        println!(
            "  Structural Quality      {:.4}  {}",
            c.evaluation.structural_quality,
            quality_label(c.evaluation.structural_quality)
        );
        println!(
            "  Dependency Quality      {:.4}  {}",
            c.evaluation.dependency_quality,
            quality_label(c.evaluation.dependency_quality)
        );
        println!(
            "  Constraint Satisfaction {:.4}  {}",
            c.evaluation.constraint_satisfaction,
            quality_label(c.evaluation.constraint_satisfaction)
        );
        println!(
            "  Complexity              {:.4}  {}",
            c.evaluation.complexity,
            complexity_label(c.evaluation.complexity)
        );
        println!(
            "  Simulation Quality      {:.4}  {}",
            c.evaluation.simulation_quality,
            quality_label(c.evaluation.simulation_quality)
        );
        println!("  ──────────────────────────────");
        println!("  Total Score             {:.4}", c.evaluation.total);
        println!();

        // Code metrics (available when generated with code)
        if c.code_metrics.module_count > 0 {
            println!("Code Metrics:");
            println!("  Modules:           {}", c.code_metrics.module_count);
            println!("  Dependency depth:  {}", c.code_metrics.dependency_depth);
            println!("  Coupling score:    {:.4}", c.code_metrics.coupling_score);
            println!("  Dependency cycles: {}", c.code_metrics.dependency_cycles);
            println!();
        }

        // Language Engine による自然言語解説
        println!("Narrative Analysis (Language Engine):");
        let narrative = verbalize_candidate(&design.input, c);
        for line in narrative.lines() {
            println!("  {line}");
        }
        println!();

        println!("{}", "─".repeat(55));
        println!();
    }

    Ok(())
}

// ─── ヘルパー ────────────────────────────────────────────────────────────────

/// コンポーネント名・依存構造から設計パターンを推定する（ヒューリスティック）。
fn detect_pattern(c: &SavedCandidate) -> &'static str {
    let names: Vec<&str> = c.components.iter().map(|s| s.as_str()).collect();
    let has_gateway = names
        .iter()
        .any(|n| n.contains("gateway") || n.contains("api"));
    let has_event = names
        .iter()
        .any(|n| n.contains("event") || n.contains("bus") || n.contains("queue"));
    let has_database = names
        .iter()
        .any(|n| n.contains("database") || n.contains("db") || n.contains("repository"));
    let service_count = names.iter().filter(|n| n.contains("service")).count();

    if has_event && service_count >= 2 {
        "Event-Driven Microservices"
    } else if has_gateway && service_count >= 2 {
        "API Gateway + Microservices"
    } else if has_gateway && has_database {
        "Layered Architecture (Gateway → Service → Repository)"
    } else if service_count >= 3 {
        "Microservices"
    } else if has_database && service_count >= 1 {
        "Layered Architecture (Service → Repository)"
    } else {
        "Modular Monolith"
    }
}

fn quality_label(score: f64) -> &'static str {
    if score >= 0.9 {
        "優秀"
    } else if score >= 0.7 {
        "良好"
    } else if score >= 0.5 {
        "普通"
    } else {
        "要改善"
    }
}

fn complexity_label(score: f64) -> &'static str {
    if score <= 0.3 {
        "低複雑性"
    } else if score <= 0.6 {
        "中複雑性"
    } else {
        "高複雑性"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{
        SavedCandidate, SavedCodeMetrics, SavedDesign, SavedEvaluation, save_design_file,
    };

    fn make_design_with(components: Vec<&str>, deps: Vec<[&str; 2]>) -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "1742000000".to_string(),
            input: "テスト".to_string(),
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
                components: components.iter().map(|s| s.to_string()).collect(),
                dependencies: deps
                    .iter()
                    .map(|d| [d[0].to_string(), d[1].to_string()])
                    .collect(),
                code_metrics: SavedCodeMetrics::default(),
            }],
        }
    }

    #[test]
    fn test_explain_missing_file_is_error() {
        assert!(run("/nonexistent/design.json").is_err());
    }

    #[test]
    fn test_explain_valid_file_succeeds() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let design = make_design_with(
            vec!["api_gateway", "order_service", "database_1"],
            vec![
                ["api_gateway", "order_service"],
                ["order_service", "database_1"],
            ],
        );
        save_design_file(&design, tmp.path()).unwrap();
        assert!(run(tmp.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn test_detect_pattern_api_gateway_microservices() {
        let design = make_design_with(
            vec!["api_gateway", "service_1", "service_2", "service_3"],
            vec![],
        );
        let pattern = detect_pattern(&design.candidates[0]);
        assert!(pattern.contains("Microservices") || pattern.contains("Gateway"));
    }

    #[test]
    fn test_detect_pattern_event_driven() {
        let design = make_design_with(vec!["service_1", "event_bus", "service_2"], vec![]);
        let pattern = detect_pattern(&design.candidates[0]);
        assert_eq!(pattern, "Event-Driven Microservices");
    }

    #[test]
    fn test_quality_labels() {
        assert_eq!(quality_label(0.95), "優秀");
        assert_eq!(quality_label(0.75), "良好");
        assert_eq!(quality_label(0.55), "普通");
        assert_eq!(quality_label(0.3), "要改善");
    }

    #[test]
    fn test_complexity_labels() {
        assert_eq!(complexity_label(0.2), "低複雑性");
        assert_eq!(complexity_label(0.5), "中複雑性");
        assert_eq!(complexity_label(0.8), "高複雑性");
    }
}
