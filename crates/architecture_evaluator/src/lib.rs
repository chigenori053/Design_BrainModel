use architecture_state_v2::{ArchitectureEvaluation, ArchitectureState};
use geometry_engine::GeometryEngine;

pub trait ArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultArchitectureEvaluator;

impl ArchitectureEvaluator for DefaultArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation {
        let geometry = GeometryEngine.evaluate(&state.architecture_graph);
        let knowledge_alignment = if let Some(knowledge) = &state.knowledge {
            knowledge.validation.confidence
        } else {
            0.5
        };
        let overall = ((geometry.structural_coherence
            + geometry.modularity
            + geometry.architecture_symmetry
            + knowledge_alignment)
            / 4.0)
            .clamp(0.0, 1.0);

        ArchitectureEvaluation {
            geometry,
            knowledge_alignment,
            overall,
        }
    }
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
