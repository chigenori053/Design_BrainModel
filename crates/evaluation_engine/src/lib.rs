use architecture_domain::ArchitectureState;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DependencyModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerformanceModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComplexityModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationEngine {
    pub dependency_model: DependencyModel,
    pub performance_model: PerformanceModel,
    pub cost_model: CostModel,
    pub complexity_model: ComplexityModel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EvaluationResult {
    pub dependency_score: f64,
    pub performance_score: f64,
    pub cost_score: f64,
    pub complexity_score: f64,
    pub total_score: f64,
}

impl EvaluationEngine {
    pub fn evaluate(&self, architecture: &ArchitectureState) -> EvaluationResult {
        let dependency_score = architecture.metrics.layering_score.clamp(0.0, 1.0);
        let performance_score = (1.0
            - architecture.metrics.dependency_count as f64
                / (architecture.metrics.component_count.max(1) * 4) as f64)
            .clamp(0.0, 1.0);
        let cost_score = (1.0 - architecture.deployment.replicas as f64 / 16.0).clamp(0.0, 1.0);
        let complexity_score = (architecture.metrics.dependency_count as f64
            / (architecture.metrics.component_count.max(1) * 3) as f64)
            .clamp(0.0, 1.0);
        let total_score =
            (dependency_score + performance_score + cost_score + (1.0 - complexity_score)) / 4.0;
        EvaluationResult {
            dependency_score,
            performance_score,
            cost_score,
            complexity_score,
            total_score: total_score.clamp(0.0, 1.0),
        }
    }
}
