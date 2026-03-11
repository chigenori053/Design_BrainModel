use architecture_knowledge::{KnowledgeAnalyzer, PatternDetection};
use architecture_memory::{recall_similar_architecture, ArchitectureMemory};
use architecture_metrics::{ArchitectureMetrics, MetricsCalculator};
use architecture_rules::{RuleValidator, RuleViolation};
use architecture_state_v2::{ArchitectureEvaluation, ArchitectureState};
use geometry_engine::GeometryEngine;

pub trait ArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation;

    fn evaluate_score(&self, state: &ArchitectureState) -> ArchitectureScore;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureScore {
    pub structural: f64,
    pub rule_score: f64,
    pub knowledge_score: f64,
    pub intent_alignment: f64,
}

impl ArchitectureScore {
    pub fn total(&self) -> f64 {
        ((self.structural + self.rule_score + self.knowledge_score + self.intent_alignment) / 4.0)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationDetails {
    pub score: ArchitectureScore,
    pub metrics: ArchitectureMetrics,
    pub violations: Vec<RuleViolation>,
    pub pattern_detection: PatternDetection,
    pub recalled_patterns: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DefaultArchitectureEvaluator;

impl ArchitectureEvaluator for DefaultArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation {
        let geometry = GeometryEngine.evaluate(&state.architecture_graph);
        let details = self.evaluate_details(state, None);

        ArchitectureEvaluation {
            geometry,
            knowledge_alignment: details.score.knowledge_score,
            overall: details.score.total(),
        }
    }

    fn evaluate_score(&self, state: &ArchitectureState) -> ArchitectureScore {
        self.evaluate_details(state, None).score
    }
}

impl DefaultArchitectureEvaluator {
    pub fn evaluate_details(
        &self,
        state: &ArchitectureState,
        memory: Option<&ArchitectureMemory>,
    ) -> EvaluationDetails {
        let metrics = MetricsCalculator.compute(&state.architecture_graph);
        let violations = RuleValidator.validate(&state.architecture_graph);
        let detection = KnowledgeAnalyzer::default().detect(&state.architecture_graph);
        let recalled_patterns = memory
            .map(|memory| recall_similar_architecture(&state.architecture_graph, memory))
            .unwrap_or_default()
            .into_iter()
            .map(|pattern| pattern.name)
            .collect::<Vec<_>>();
        let rule_score = (1.0 - violations.len() as f64 * 0.2).clamp(0.0, 1.0);
        let structural = ((metrics.modularity
            + (1.0 - metrics.coupling)
            + metrics.cohesion
            + metrics.layering_score
            + (1.0 - metrics.dependency_entropy))
            / 5.0)
            .clamp(0.0, 1.0);
        let knowledge_alignment = if let Some(knowledge) = &state.knowledge {
            knowledge.validation.confidence
        } else {
            0.5
        };
        let memory_bonus = if recalled_patterns.is_empty() {
            0.0
        } else {
            0.1
        };
        let intent_alignment = intent_alignment_score(state, &detection, memory_bonus);
        let knowledge_score =
            (detection.knowledge_score + knowledge_alignment + memory_bonus).min(1.0);
        let score = ArchitectureScore {
            structural,
            rule_score,
            knowledge_score,
            intent_alignment,
        };

        EvaluationDetails {
            score,
            metrics,
            violations,
            pattern_detection: detection,
            recalled_patterns,
        }
    }
}

fn intent_alignment_score(
    state: &ArchitectureState,
    detection: &PatternDetection,
    memory_bonus: f64,
) -> f64 {
    let problem = state.problem.to_ascii_lowercase();
    let layered_match = problem.contains("layer") || problem.contains("api");
    let service_match = problem.contains("service") || problem.contains("microservice");
    let pattern_bonus = detection
        .matched_patterns
        .iter()
        .filter(|pattern| {
            let name = pattern.name.to_ascii_lowercase();
            (layered_match && name.contains("layered"))
                || (service_match && name.contains("service"))
        })
        .count() as f64
        * 0.2;
    (0.4 + pattern_bonus + memory_bonus).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use architecture_reasoner::ReverseArchitectureReasoner;
    use code_ir::CodeIr;
    use design_domain::DesignUnit;

    use super::*;

    #[test]
    fn computes_non_zero_score_for_consistent_architecture() {
        let mut controller = DesignUnit::new(1, "ApiController");
        controller.dependencies.push(design_domain::DesignUnitId(2));
        let service = DesignUnit::new(2, "UserService");
        let code_ir = CodeIr::from_design_units(&[controller, service]);
        let architecture_graph = ReverseArchitectureReasoner.infer_from_code_ir(&code_ir);
        let state = ArchitectureState {
            problem: "serve users".into(),
            code_ir,
            architecture_graph,
            ..ArchitectureState::default()
        };

        let evaluation = DefaultArchitectureEvaluator.evaluate(&state);

        assert!(evaluation.overall > 0.0);
    }
}
