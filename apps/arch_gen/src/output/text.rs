use design_search_engine::RankedCandidate;
use world_model_core::EvaluationVector;

pub struct GenerationSummary<'a> {
    pub input: &'a str,
    pub search_states: usize,
    pub frontier_size: usize,
    pub candidates: &'a [RankedCandidate],
}

pub fn render_summary(summary: &GenerationSummary<'_>) -> String {
    let mut out = String::new();

    out.push_str("Architecture Generation Result\n");
    out.push_str(&"═".repeat(55));
    out.push('\n');
    out.push('\n');
    out.push_str(&format!("Input: \"{}\"\n", summary.input));
    out.push_str(&format!(
        "Pipeline: Phase9-D (BeamSearch)\n"
    ));
    out.push_str(&format!(
        "Search states evaluated: {}\n",
        summary.search_states
    ));
    out.push_str(&format!(
        "Pareto frontier size:    {}\n",
        summary.frontier_size
    ));
    out.push('\n');

    for (i, candidate) in summary.candidates.iter().enumerate() {
        out.push_str(&format!(
            "─── Candidate {} (Score: {:.4}) ",
            i + 1,
            candidate.score
        ));
        out.push_str(&"─".repeat(30));
        out.push('\n');

        let arch = &candidate.state.architecture_state;
        if !arch.components.is_empty() {
            out.push_str("Components:\n");
            for dep in &arch.dependencies {
                out.push_str(&format!(
                    "  {:?} → {:?}\n",
                    dep.from, dep.to
                ));
            }
            out.push_str(&format!(
                "  ({} components, {} dependencies)\n",
                arch.components.len(),
                arch.dependencies.len()
            ));
        }
        out.push('\n');

        let eval = &candidate.state.world_state.evaluation;
        out.push_str(&render_evaluation(eval));
        out.push('\n');
    }

    out.push_str(&"═".repeat(55));
    out.push('\n');
    out
}

pub fn render_evaluation(eval: &EvaluationVector) -> String {
    format!(
        "EvaluationVector:\n  structural:  {:.4} | dependency: {:.4} | constraint: {:.4}\n  complexity:  {:.4} | simulation: {:.4} | total: {:.4}\n",
        eval.structural_quality,
        eval.dependency_quality,
        eval.constraint_satisfaction,
        eval.complexity,
        eval.simulation_quality,
        eval.total(),
    )
}
