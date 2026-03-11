use architecture_knowledge::{ArchitecturePattern, KnowledgeAnalyzer};
use architecture_metrics::{ArchitectureMetrics, MetricsCalculator};
use architecture_reasoner::ArchitectureGraph;

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureEmbedding {
    pub vector: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureMemory {
    pub patterns: Vec<ArchitecturePattern>,
    pub embeddings: Vec<ArchitectureEmbedding>,
}

impl ArchitectureMemory {
    pub fn from_graph(graph: &ArchitectureGraph) -> Self {
        let analyzer = KnowledgeAnalyzer::default();
        let metrics = MetricsCalculator.compute(graph);
        let detection = analyzer.detect(graph);
        Self {
            patterns: detection.matched_patterns,
            embeddings: vec![ArchitectureEmbedding {
                vector: metrics_to_vector(&metrics),
            }],
        }
    }

    pub fn with_seed_patterns(patterns: Vec<ArchitecturePattern>) -> Self {
        Self {
            patterns,
            embeddings: Vec::new(),
        }
    }
}

pub fn recall_similar_architecture(
    graph: &ArchitectureGraph,
    memory: &ArchitectureMemory,
) -> Vec<ArchitecturePattern> {
    let metrics = MetricsCalculator.compute(graph);
    let query = metrics_to_vector(&metrics);
    let similarity = memory
        .embeddings
        .iter()
        .map(|embedding| cosine_similarity(&query, &embedding.vector))
        .fold(0.0_f64, f64::max);
    if similarity >= 0.6 || memory.embeddings.is_empty() {
        memory.patterns.clone()
    } else {
        Vec::new()
    }
}

fn metrics_to_vector(metrics: &ArchitectureMetrics) -> Vec<f64> {
    vec![
        metrics.modularity,
        metrics.coupling,
        metrics.cohesion,
        metrics.layering_score,
        metrics.dependency_entropy,
    ]
}

fn cosine_similarity(lhs: &[f64], rhs: &[f64]) -> f64 {
    let dot = lhs.iter().zip(rhs.iter()).map(|(a, b)| a * b).sum::<f64>();
    let lhs_norm = lhs.iter().map(|value| value * value).sum::<f64>().sqrt();
    let rhs_norm = rhs.iter().map(|value| value * value).sum::<f64>().sqrt();
    if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.0
    } else {
        (dot / (lhs_norm * rhs_norm)).clamp(0.0, 1.0)
    }
}
