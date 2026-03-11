use architecture_metrics::{ArchitectureMetrics, MetricsCalculator};
use architecture_reasoner::ArchitectureGraph;
use architecture_rules::{ArchitectureRule, RuleValidator};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchitecturePatternKind {
    Layered,
    Hexagonal,
    Microservice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitecturePattern {
    pub kind: ArchitecturePatternKind,
    pub name: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AntiPattern {
    pub name: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureKnowledge {
    pub patterns: Vec<ArchitecturePattern>,
    pub anti_patterns: Vec<AntiPattern>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternDetection {
    pub matched_patterns: Vec<ArchitecturePattern>,
    pub detected_anti_patterns: Vec<AntiPattern>,
    pub knowledge_score: f64,
    pub metrics: ArchitectureMetrics,
}

#[derive(Clone, Debug, Default)]
pub struct KnowledgeAnalyzer {
    metrics: MetricsCalculator,
    validator: RuleValidator,
}

impl KnowledgeAnalyzer {
    pub fn detect(&self, graph: &ArchitectureGraph) -> PatternDetection {
        let metrics = self.metrics.compute(graph);
        let violations = self.validator.validate(graph);
        let mut matched_patterns = Vec::new();
        let mut anti_patterns = Vec::new();

        if metrics.layering_score >= 0.8 && metrics.modularity >= 0.3 {
            matched_patterns.push(ArchitecturePattern {
                kind: ArchitecturePatternKind::Layered,
                name: "Layered architecture".to_string(),
                evidence: vec![
                    format!("layering_score={:.2}", metrics.layering_score),
                    format!("modularity={:.2}", metrics.modularity),
                ],
            });
        }
        if graph
            .nodes
            .iter()
            .any(|node| node.name.to_ascii_lowercase().contains("gateway"))
            && graph
                .nodes
                .iter()
                .any(|node| node.name.to_ascii_lowercase().contains("service"))
        {
            matched_patterns.push(ArchitecturePattern {
                kind: ArchitecturePatternKind::Hexagonal,
                name: "Hexagonal architecture".to_string(),
                evidence: vec!["gateway-service boundary".to_string()],
            });
        }
        let service_like = graph
            .nodes
            .iter()
            .filter(|node| {
                let lower = node.name.to_ascii_lowercase();
                lower.contains("service") || lower.contains("gateway")
            })
            .count();
        if service_like >= 2 && metrics.coupling <= 0.8 {
            matched_patterns.push(ArchitecturePattern {
                kind: ArchitecturePatternKind::Microservice,
                name: "Microservice architecture".to_string(),
                evidence: vec![format!("service_like={service_like}")],
            });
        }

        for violation in violations {
            anti_patterns.push(AntiPattern {
                name: match violation.rule {
                    ArchitectureRule::NoDependencyCycle => "Dependency cycle",
                    ArchitectureRule::LayerViolation => "Layer violation",
                    ArchitectureRule::BoundedContextViolation => "Bounded context violation",
                    ArchitectureRule::ForbiddenDependency => "Forbidden dependency",
                }
                .to_string(),
                evidence: vec![violation.message],
            });
        }

        let knowledge_score = ((matched_patterns.len() as f64 * 0.3)
            + (1.0 - anti_patterns.len() as f64 * 0.2)
            + metrics.layering_score)
            .clamp(0.0, 1.0);

        PatternDetection {
            matched_patterns,
            detected_anti_patterns: anti_patterns,
            knowledge_score,
            metrics,
        }
    }
}
