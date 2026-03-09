use concept_engine::ConceptId;

use crate::constraint::ConstraintEngine;
use crate::design_state::DesignState;
use crate::evaluator::Evaluator;
use crate::hypothesis_graph::{DesignOperation, DesignTransition, HypothesisGraph};
use crate::search_config::SearchConfig;
use crate::search_strategy::SearchStrategy;

pub struct DesignSearchEngine {
    pub strategy: Box<dyn SearchStrategy>,
    pub evaluator: Evaluator,
    pub constraint_engine: ConstraintEngine,
    pub config: SearchConfig,
}

impl DesignSearchEngine {
    pub fn search(&self, initial: DesignState, concepts: &[ConceptId]) -> HypothesisGraph {
        let mut graph = HypothesisGraph::default();
        let mut seed = initial.clone();
        seed.evaluation = Some(self.evaluator.evaluate(&seed, concepts));
        graph.insert_state(seed.clone());

        let mut beam = vec![seed];
        for _ in 0..self.config.max_depth {
            let mut candidates = Vec::new();
            for parent in &beam {
                for mut child in self.strategy.expand(parent) {
                    if !self.constraint_engine.is_valid(&child) {
                        continue;
                    }
                    child.evaluation = Some(self.evaluator.evaluate(&child, concepts));
                    graph.add_transition(DesignTransition {
                        from: parent.id,
                        to: child.id,
                        operation: infer_operation(parent, &child),
                    });
                    graph.insert_state(child.clone());
                    candidates.push(child);
                }
            }

            if candidates.is_empty() {
                break;
            }

            candidates.sort_by(|lhs, rhs| {
                let ls = lhs.evaluation.as_ref().map(|e| e.total()).unwrap_or(0.0);
                let rs = rhs.evaluation.as_ref().map(|e| e.total()).unwrap_or(0.0);
                rs.total_cmp(&ls).then_with(|| lhs.id.cmp(&rhs.id))
            });
            candidates.truncate(self.config.beam_width.max(1));
            beam = candidates;
        }

        graph
    }
}

fn infer_operation(from: &DesignState, to: &DesignState) -> DesignOperation {
    match to.design_units.len().cmp(&from.design_units.len()) {
        std::cmp::Ordering::Greater => DesignOperation::AddUnit,
        std::cmp::Ordering::Less => DesignOperation::RemoveUnit,
        std::cmp::Ordering::Equal => {
            if from.design_units == to.design_units {
                DesignOperation::RefactorStructure
            } else {
                DesignOperation::ModifyDependency
            }
        }
    }
}
