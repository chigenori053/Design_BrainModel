use crate::output::text::CandidateDisplay;
use world_model_core::EvaluationVector;

/// `CandidateDisplay` のスライスから Markdown レポートを生成する。
pub fn build_markdown(input: &str, search_states: usize, candidates: &[CandidateDisplay]) -> String {
    let mut out = String::new();

    out.push_str("# Architecture Generation Report\n\n");
    out.push_str(&format!("**Input:** {input}\n\n"));
    out.push_str(&format!("**Search states evaluated:** {search_states}\n\n"));
    out.push_str("---\n\n");

    for (i, c) in candidates.iter().enumerate() {
        out.push_str(&format!(
            "## Candidate {} — Score: {:.4} (Pareto rank: {})\n\n",
            i + 1,
            c.score,
            c.pareto_rank
        ));

        // Components / dependencies
        if !c.dependency_pairs.is_empty() {
            out.push_str("### Component Dependencies\n\n");
            out.push_str("| From | To |\n|------|----|\n");
            for (from, to) in &c.dependency_pairs {
                out.push_str(&format!("| {from} | {to} |\n"));
            }
            out.push('\n');
        } else if !c.component_names.is_empty() {
            out.push_str("### Components\n\n");
            for name in &c.component_names {
                out.push_str(&format!("- {name}\n"));
            }
            out.push('\n');
        }

        // Evaluation scores
        out.push_str("### Evaluation\n\n");
        out.push_str(&render_evaluation_md(&c.evaluation));

        // Mermaid diagram
        out.push_str("### Diagram\n\n");
        out.push_str("```mermaid\n");
        out.push_str(&crate::output::mermaid::build_mermaid(
            &c.component_names,
            &c.dependency_pairs,
        ));
        out.push_str("```\n\n");

        // Generated files
        if !c.generated_files.is_empty() {
            out.push_str("### Generated Files\n\n");
            for f in &c.generated_files {
                out.push_str(&format!("- `{f}`\n"));
            }
            out.push('\n');
        }

        out.push_str("---\n\n");
    }

    out
}

fn render_evaluation_md(eval: &EvaluationVector) -> String {
    format!(
        "| Metric | Score |\n|--------|-------|\n\
         | Structural quality | {:.4} |\n\
         | Dependency quality | {:.4} |\n\
         | Constraint satisfaction | {:.4} |\n\
         | Complexity | {:.4} |\n\
         | Simulation quality | {:.4} |\n\
         | **Total** | **{:.4}** |\n\n",
        eval.structural_quality,
        eval.dependency_quality,
        eval.constraint_satisfaction,
        eval.complexity,
        eval.simulation_quality,
        eval.total(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_model_core::EvaluationVector;

    fn sample_candidate() -> CandidateDisplay {
        CandidateDisplay {
            score: 0.85,
            pareto_rank: 0,
            component_names: vec!["service_1".to_string(), "database_2".to_string()],
            dependency_pairs: vec![("service_1".to_string(), "database_2".to_string())],
            evaluation: EvaluationVector {
                structural_quality: 1.0,
                dependency_quality: 0.8,
                constraint_satisfaction: 0.9,
                complexity: 0.3,
                simulation_quality: 0.9,
            },
            generated_files: vec![],
        }
    }

    #[test]
    fn test_build_markdown_contains_header() {
        let md = build_markdown("ECサイト", 50, &[sample_candidate()]);
        assert!(md.contains("# Architecture Generation Report"));
        assert!(md.contains("ECサイト"));
        assert!(md.contains("50"));
    }

    #[test]
    fn test_build_markdown_candidate_section() {
        let md = build_markdown("test", 10, &[sample_candidate()]);
        assert!(md.contains("Candidate 1"));
        assert!(md.contains("0.8500"));
        assert!(md.contains("service_1"));
        assert!(md.contains("database_2"));
    }

    #[test]
    fn test_build_markdown_dependency_table() {
        let md = build_markdown("test", 10, &[sample_candidate()]);
        assert!(md.contains("| From | To |"));
        assert!(md.contains("| service_1 | database_2 |"));
    }

    #[test]
    fn test_build_markdown_mermaid_block() {
        let md = build_markdown("test", 10, &[sample_candidate()]);
        assert!(md.contains("```mermaid"));
        assert!(md.contains("graph TD"));
        assert!(md.contains("```"));
    }

    #[test]
    fn test_build_markdown_empty_candidates() {
        let md = build_markdown("test", 0, &[]);
        assert!(md.contains("# Architecture Generation Report"));
        assert!(!md.contains("Candidate"));
    }

    #[test]
    fn test_build_markdown_no_dependencies() {
        let mut c = sample_candidate();
        c.dependency_pairs.clear();
        let md = build_markdown("test", 5, &[c]);
        assert!(md.contains("### Components"));
        assert!(!md.contains("| From | To |"));
    }
}
