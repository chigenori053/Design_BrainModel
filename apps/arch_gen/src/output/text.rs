use world_model_core::EvaluationVector;

/// generate コマンドの1候補分の表示データ。
#[derive(Clone)]
pub struct CandidateDisplay {
    pub score: f64,
    pub pareto_rank: usize,
    /// コンポーネント名一覧（"service_1" 等）
    pub component_names: Vec<String>,
    /// 依存関係 (from_name, to_name)
    pub dependency_pairs: Vec<(String, String)>,
    pub evaluation: EvaluationVector,
    /// 書き出したファイルパス一覧（--no-code 時は空）
    pub generated_files: Vec<String>,
}

pub struct GenerationSummary<'a> {
    pub input: &'a str,
    pub search_states: usize,
    pub frontier_size: usize,
    pub candidates: &'a [CandidateDisplay],
}

pub fn render_summary(summary: &GenerationSummary<'_>) -> String {
    let mut out = String::new();

    out.push_str("Architecture Generation Result\n");
    out.push_str(&"═".repeat(55));
    out.push('\n');
    out.push('\n');
    out.push_str(&format!("Input: \"{}\"\n", summary.input));
    out.push_str("Pipeline: Phase9-D (BeamSearch)\n");
    out.push_str(&format!("Search states evaluated: {}\n", summary.search_states));
    out.push_str(&format!("Pareto frontier size:    {}\n", summary.frontier_size));
    out.push('\n');

    for (i, c) in summary.candidates.iter().enumerate() {
        out.push_str(&format!(
            "─── Candidate {} (Score: {:.4}) ",
            i + 1,
            c.score
        ));
        out.push_str(&"─".repeat(28));
        out.push('\n');

        if !c.dependency_pairs.is_empty() {
            out.push_str("Components:\n");
            for (from, to) in &c.dependency_pairs {
                out.push_str(&format!("  {from} → {to}\n"));
            }
        } else if !c.component_names.is_empty() {
            out.push_str("Components:\n");
            for name in &c.component_names {
                out.push_str(&format!("  {name}\n"));
            }
        }
        out.push_str(&format!(
            "  ({} components, {} dependencies)\n",
            c.component_names.len(),
            c.dependency_pairs.len()
        ));
        out.push('\n');

        out.push_str(&render_evaluation(&c.evaluation));

        if !c.generated_files.is_empty() {
            out.push_str("Generated files:\n");
            for f in &c.generated_files {
                out.push_str(&format!("  {f}\n"));
            }
        }
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
