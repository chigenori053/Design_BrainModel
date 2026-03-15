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
