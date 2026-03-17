use architecture_ir::stable_v03::ArchitectureGraph;

pub trait ArchitectureEvaluator: Send + Sync {
    fn evaluate(&self, graph: &ArchitectureGraph) -> EvaluationResult;
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationResult {
    pub score: f64,
    pub metrics: EvaluationMetrics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationMetrics {
    pub modularity: f64,
    pub coupling: f64,
    pub cohesion: f64,
    pub complexity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeightedArchitectureEvaluator {
    pub modularity_weight: f64,
    pub coupling_weight: f64,
    pub cohesion_weight: f64,
    pub complexity_weight: f64,
}

impl Default for WeightedArchitectureEvaluator {
    fn default() -> Self {
        Self {
            modularity_weight: 0.35,
            coupling_weight: 0.2,
            cohesion_weight: 0.3,
            complexity_weight: 0.15,
        }
    }
}

impl ArchitectureEvaluator for WeightedArchitectureEvaluator {
    fn evaluate(&self, graph: &ArchitectureGraph) -> EvaluationResult {
        let node_count = graph.nodes().len() as f64;
        let edge_count = graph.edges().len() as f64;
        let adjacency_density = if node_count <= 1.0 {
            0.0
        } else {
            (edge_count / (node_count * (node_count - 1.0))).clamp(0.0, 1.0)
        };
        let coupling = if node_count == 0.0 {
            0.0
        } else {
            (edge_count / node_count).clamp(0.0, 1.0)
        };
        let isolated_nodes = graph
            .nodes()
            .iter()
            .filter(|node| {
                graph.outgoing(node.id.clone()).is_empty()
                    && graph.incoming(node.id.clone()).is_empty()
            })
            .count() as f64;
        let modularity = if node_count == 0.0 {
            1.0
        } else {
            (1.0 - adjacency_density - isolated_nodes / node_count).clamp(0.0, 1.0)
        };
        let bidirectional_edges = graph
            .edges()
            .iter()
            .filter(|edge| {
                graph
                    .outgoing(edge.target.clone())
                    .iter()
                    .any(|candidate| candidate.target == edge.source)
            })
            .count() as f64;
        let cohesion = if edge_count == 0.0 {
            1.0
        } else {
            (1.0 - bidirectional_edges / edge_count).clamp(0.0, 1.0)
        };
        let complexity = ((node_count + edge_count) / 20.0).clamp(0.0, 1.0);
        let metrics = EvaluationMetrics {
            modularity,
            coupling,
            cohesion,
            complexity,
        };
        let score = (self.modularity_weight * metrics.modularity
            + self.cohesion_weight * metrics.cohesion
            - self.coupling_weight * metrics.coupling
            - self.complexity_weight * metrics.complexity)
            .clamp(-1.0, 1.0);

        EvaluationResult { score, metrics }
    }
}
