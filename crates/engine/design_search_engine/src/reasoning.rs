use concept_engine::ConceptId;
use knowledge_engine::{
    KnowledgeEngine, KnowledgeIntegration, KnowledgeQuery, KnowledgeRelation, RelationType,
};
use memory_space_complex::ComplexField;

use crate::design_state::{DesignState, DesignStateId, DesignUnit, DesignUnitId, DesignUnitType};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IntentGraph {
    pub intents: Vec<String>,
    pub edges: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureHypothesis {
    pub hypothesis_id: usize,
    pub name: String,
    pub required_concepts: Vec<String>,
    pub framework_candidates: Vec<String>,
    pub seed_state: DesignState,
    pub valid: bool,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HypothesisValidation {
    pub hypothesis_id: usize,
    pub knowledge_consistency: bool,
    pub constraint_compatibility: bool,
    pub design_validity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReasoningTelemetryEvent {
    pub name: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReasoningTelemetry {
    pub events: Vec<ReasoningTelemetryEvent>,
}

impl ReasoningTelemetry {
    fn push(&mut self, name: &'static str, detail: impl Into<String>) {
        self.events.push(ReasoningTelemetryEvent {
            name,
            detail: detail.into(),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReasoningConfig {
    pub max_hypotheses: usize,
    pub max_reasoning_depth: usize,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        Self {
            max_hypotheses: 32,
            max_reasoning_depth: 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningResult {
    pub intent_graph: IntentGraph,
    pub inferred_knowledge: Vec<KnowledgeRelation>,
    pub architecture_hypotheses: Vec<ArchitectureHypothesis>,
    pub validations: Vec<HypothesisValidation>,
    pub reasoning_confidence: f64,
    pub telemetry: ReasoningTelemetry,
}

impl ReasoningResult {
    pub fn best_hypothesis(&self) -> Option<&ArchitectureHypothesis> {
        self.architecture_hypotheses.first()
    }

    pub fn best_seed_state(&self) -> Option<DesignState> {
        self.best_hypothesis()
            .map(|hypothesis| hypothesis.seed_state.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntentParser;

impl IntentParser {
    pub fn parse(&self, request: &str) -> IntentGraph {
        let lower = request.to_ascii_lowercase();
        let mut intents = Vec::new();
        for (keywords, label) in [
            (&["rust"][..], "Rust"),
            (&["web", "http", "server"][..], "WebServer"),
            (&["rest", "api", "json"][..], "REST API"),
            (&["database", "postgres", "mysql", "sql"][..], "Database"),
            (
                &["auth", "authentication", "jwt", "oauth"][..],
                "Authentication",
            ),
            (&["cache", "redis"][..], "Cache"),
            (&["queue", "event", "stream", "messaging"][..], "Messaging"),
        ] {
            if keywords.iter().any(|keyword| lower.contains(keyword)) {
                intents.push(label.to_string());
            }
        }

        intents.sort();
        intents.dedup();
        if intents.is_empty() {
            intents.push("GeneralApplication".to_string());
        }

        let edges = intents
            .windows(2)
            .map(|window| (window[0].clone(), window[1].clone()))
            .collect();

        IntentGraph { intents, edges }
    }
}

pub struct KnowledgeRetriever {
    engine: KnowledgeEngine,
}

impl Default for KnowledgeRetriever {
    fn default() -> Self {
        Self {
            engine: KnowledgeEngine::default(),
        }
    }
}

impl KnowledgeRetriever {
    pub fn retrieve(&self, intent_graph: &IntentGraph) -> KnowledgeIntegration {
        self.engine.process_query(KnowledgeQuery {
            text: intent_graph.intents.join(" "),
            semantic_hints: intent_graph.intents.clone(),
            semantic_vector: embed_intents(intent_graph),
            keywords: intent_graph
                .intents
                .iter()
                .map(|intent| intent.to_ascii_lowercase())
                .collect(),
            relation_types: infer_relation_requirements(intent_graph),
            max_results: 32,
            confidence_threshold: 0.2,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HypothesisGenerator;

impl HypothesisGenerator {
    pub fn generate(
        &self,
        intent_graph: &IntentGraph,
        knowledge: &KnowledgeIntegration,
        state_vector: &ComplexField,
        config: ReasoningConfig,
    ) -> Vec<ArchitectureHypothesis> {
        let mut hypotheses = Vec::new();
        let base_frameworks = frameworks_for_intents(intent_graph);
        let base_units = seed_unit_count(intent_graph, config);

        hypotheses.push(ArchitectureHypothesis {
            hypothesis_id: 0,
            name: "LayeredArchitecture".to_string(),
            required_concepts: intent_graph.intents.clone(),
            framework_candidates: base_frameworks.clone(),
            seed_state: seed_state_for_hypothesis(1, base_units, state_vector),
            valid: true,
            confidence: base_confidence(intent_graph, knowledge, 0.0),
        });

        for (offset, framework) in base_frameworks.iter().enumerate() {
            hypotheses.push(ArchitectureHypothesis {
                hypothesis_id: offset + 1,
                name: format!("{framework}ServiceStack"),
                required_concepts: intent_graph.intents.clone(),
                framework_candidates: vec![framework.clone()],
                seed_state: seed_state_for_hypothesis(
                    (offset + 2) as u64,
                    base_units
                        + usize::from(
                            intent_graph
                                .intents
                                .iter()
                                .any(|intent| intent == "Authentication"),
                        ),
                    state_vector,
                ),
                valid: true,
                confidence: base_confidence(intent_graph, knowledge, 0.05),
            });
        }

        if intent_graph
            .intents
            .iter()
            .any(|intent| intent == "Authentication")
        {
            hypotheses.push(ArchitectureHypothesis {
                hypothesis_id: hypotheses.len(),
                name: "AuthGatewayArchitecture".to_string(),
                required_concepts: intent_graph.intents.clone(),
                framework_candidates: base_frameworks.clone(),
                seed_state: seed_state_for_hypothesis(50, base_units + 1, state_vector),
                valid: true,
                confidence: base_confidence(intent_graph, knowledge, 0.08),
            });
        }

        if intent_graph.intents.iter().any(|intent| intent == "Cache") {
            hypotheses.push(ArchitectureHypothesis {
                hypothesis_id: hypotheses.len(),
                name: "CachedReadOptimizedArchitecture".to_string(),
                required_concepts: intent_graph.intents.clone(),
                framework_candidates: base_frameworks,
                seed_state: seed_state_for_hypothesis(60, base_units + 1, state_vector),
                valid: true,
                confidence: base_confidence(intent_graph, knowledge, 0.07),
            });
        }

        hypotheses.sort_by(|lhs, rhs| {
            rhs.confidence
                .total_cmp(&lhs.confidence)
                .then_with(|| lhs.hypothesis_id.cmp(&rhs.hypothesis_id))
        });
        hypotheses.truncate(config.max_hypotheses.max(1));
        hypotheses
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReasoningValidator;

impl ReasoningValidator {
    pub fn validate(
        &self,
        hypotheses: Vec<ArchitectureHypothesis>,
        knowledge: &KnowledgeIntegration,
        config: ReasoningConfig,
    ) -> (Vec<ArchitectureHypothesis>, Vec<HypothesisValidation>) {
        let mut validated = Vec::new();
        let mut validations = Vec::new();

        for mut hypothesis in hypotheses {
            let knowledge_consistency =
                !knowledge.documents.is_empty() || !knowledge.knowledge_graph.relations.is_empty();
            let constraint_compatibility =
                hypothesis.seed_state.design_units.len() <= config.max_reasoning_depth * 2;
            let design_validity = !hypothesis.seed_state.design_units.is_empty();
            let valid = knowledge_consistency && constraint_compatibility && design_validity;
            hypothesis.valid = valid;
            if valid {
                validated.push(hypothesis.clone());
            }
            validations.push(HypothesisValidation {
                hypothesis_id: hypothesis.hypothesis_id,
                knowledge_consistency,
                constraint_compatibility,
                design_validity,
            });
        }

        validated.sort_by(|lhs, rhs| {
            rhs.confidence
                .total_cmp(&lhs.confidence)
                .then_with(|| lhs.hypothesis_id.cmp(&rhs.hypothesis_id))
        });

        (validated, validations)
    }
}

pub struct ReasoningEngine {
    pub parser: IntentParser,
    pub retriever: KnowledgeRetriever,
    pub hypothesis_generator: HypothesisGenerator,
    pub validator: ReasoningValidator,
    pub config: ReasoningConfig,
}

impl Default for ReasoningEngine {
    fn default() -> Self {
        Self {
            parser: IntentParser,
            retriever: KnowledgeRetriever::default(),
            hypothesis_generator: HypothesisGenerator,
            validator: ReasoningValidator,
            config: ReasoningConfig::default(),
        }
    }
}

impl ReasoningEngine {
    pub fn reason(&self, request: &str, state_vector: ComplexField) -> ReasoningResult {
        let mut telemetry = ReasoningTelemetry::default();
        telemetry.push("ReasoningStarted", request);

        let intent_graph = self.parser.parse(request);
        let knowledge = self.retriever.retrieve(&intent_graph);
        telemetry.push(
            "KnowledgeRetrieved",
            format!(
                "documents={}, relations={}",
                knowledge.documents.len(),
                knowledge.knowledge_graph.relations.len()
            ),
        );

        let hypotheses = self.hypothesis_generator.generate(
            &intent_graph,
            &knowledge,
            &state_vector,
            self.config,
        );
        telemetry.push("HypothesisGenerated", format!("count={}", hypotheses.len()));

        let (validated_hypotheses, validations) =
            self.validator.validate(hypotheses, &knowledge, self.config);
        telemetry.push(
            "HypothesisValidated",
            format!("valid_count={}", validated_hypotheses.len()),
        );

        let reasoning_confidence = if validated_hypotheses.is_empty() {
            knowledge.validation.confidence * 0.5
        } else {
            let hypothesis_confidence = validated_hypotheses
                .iter()
                .map(|hypothesis| hypothesis.confidence)
                .sum::<f64>()
                / validated_hypotheses.len() as f64;
            ((knowledge.validation.confidence + hypothesis_confidence) / 2.0).clamp(0.0, 1.0)
        };

        ReasoningResult {
            intent_graph,
            inferred_knowledge: knowledge.knowledge_graph.relations,
            architecture_hypotheses: validated_hypotheses,
            validations,
            reasoning_confidence,
            telemetry,
        }
    }
}

fn frameworks_for_intents(intent_graph: &IntentGraph) -> Vec<String> {
    let has_rust = intent_graph.intents.iter().any(|intent| intent == "Rust");
    let has_rest_api = intent_graph
        .intents
        .iter()
        .any(|intent| intent == "REST API");
    let has_database = intent_graph
        .intents
        .iter()
        .any(|intent| intent == "Database");

    let mut frameworks = if has_rust && has_rest_api {
        vec!["Axum".to_string(), "Actix".to_string(), "Warp".to_string()]
    } else if has_rest_api {
        vec!["HTTP Service".to_string(), "Layered Service".to_string()]
    } else {
        vec!["Modular Core".to_string()]
    };

    if has_database {
        frameworks.push("Repository Pattern".to_string());
    }
    frameworks
}

fn embed_intents(intent_graph: &IntentGraph) -> Vec<f32> {
    let mut vector = vec![0.0f32; 16];
    for (idx, byte) in intent_graph.intents.join(" ").bytes().enumerate() {
        vector[idx % 16] += f32::from(byte) / 255.0;
    }
    vector
}

fn infer_relation_requirements(intent_graph: &IntentGraph) -> Vec<RelationType> {
    let mut relation_types = Vec::new();
    if intent_graph
        .intents
        .iter()
        .any(|intent| intent == "REST API")
    {
        relation_types.push(RelationType::Requires);
    }
    if intent_graph
        .intents
        .iter()
        .any(|intent| intent == "Database")
    {
        relation_types.push(RelationType::Constrains);
    }
    if intent_graph.intents.iter().any(|intent| intent == "Cache") {
        relation_types.push(RelationType::Recommends);
    }
    relation_types
}

fn seed_unit_count(intent_graph: &IntentGraph, config: ReasoningConfig) -> usize {
    intent_graph
        .intents
        .len()
        .max(2)
        .min(config.max_reasoning_depth.saturating_mul(2).max(2))
}

fn base_confidence(
    intent_graph: &IntentGraph,
    knowledge: &KnowledgeIntegration,
    bonus: f64,
) -> f64 {
    let intent_score = (intent_graph.intents.len() as f64 / 6.0).clamp(0.0, 1.0);
    let knowledge_score = knowledge.validation.confidence.clamp(0.0, 1.0);
    ((intent_score * 0.4) + (knowledge_score * 0.6) + bonus).clamp(0.0, 1.0)
}

fn seed_state_for_hypothesis(
    base_id: u64,
    unit_count: usize,
    state_vector: &ComplexField,
) -> DesignState {
    let design_units = (0..unit_count.max(1))
        .map(|idx| DesignUnit {
            id: DesignUnitId(base_id + idx as u64),
            unit_type: DesignUnitType::DesignUnit,
            dependencies: if idx == 0 {
                Vec::new()
            } else {
                vec![DesignUnitId(base_id + idx as u64 - 1)]
            },
            causal_relations: Vec::new(),
        })
        .collect();

    DesignState {
        id: DesignStateId(base_id),
        design_units,
        evaluation: None,
        state_vector: state_vector.clone(),
    }
}

pub fn runtime_hypotheses_from_reasoning(
    result: &ReasoningResult,
    concepts: &[ConceptId],
) -> Vec<(ConceptId, ConceptId)> {
    let available = concepts.iter().copied().collect::<Vec<_>>();
    let mut pairs = Vec::new();
    for hypothesis in &result.architecture_hypotheses {
        let mut labels = hypothesis
            .required_concepts
            .iter()
            .map(|label| ConceptId::from_name(label))
            .collect::<Vec<_>>();
        labels.extend(available.iter().copied());
        labels.sort_by_key(|id| id.0);
        labels.dedup();
        for window in labels.windows(2) {
            pairs.push((window[0], window[1]));
        }
    }
    pairs.sort_by_key(|pair| (pair.0.0, pair.1.0));
    pairs.dedup();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_core::Complex64;

    fn vector() -> ComplexField {
        ComplexField::new(vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(0.5, 0.0),
            Complex64::new(0.25, 0.0),
        ])
    }

    #[test]
    fn intent_parser_extracts_expected_intents() {
        let parser = IntentParser;
        let graph = parser.parse("Build Rust Web API with database and authentication");

        assert!(graph.intents.iter().any(|intent| intent == "Rust"));
        assert!(graph.intents.iter().any(|intent| intent == "WebServer"));
        assert!(graph.intents.iter().any(|intent| intent == "REST API"));
        assert!(graph.intents.iter().any(|intent| intent == "Database"));
        assert!(
            graph
                .intents
                .iter()
                .any(|intent| intent == "Authentication")
        );
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn reasoning_engine_generates_valid_hypotheses_and_telemetry() {
        let result =
            ReasoningEngine::default().reason("Build Rust Web API with database cache", vector());

        assert!(!result.inferred_knowledge.is_empty());
        assert!(!result.architecture_hypotheses.is_empty());
        assert!(result.reasoning_confidence > 0.0);
        assert!(
            result
                .telemetry
                .events
                .iter()
                .any(|event| event.name == "ReasoningStarted")
        );
        assert!(
            result
                .telemetry
                .events
                .iter()
                .any(|event| event.name == "KnowledgeRetrieved")
        );
        assert!(
            result
                .telemetry
                .events
                .iter()
                .any(|event| event.name == "HypothesisGenerated")
        );
        assert!(
            result
                .telemetry
                .events
                .iter()
                .any(|event| event.name == "HypothesisValidated")
        );
    }

    #[test]
    fn runtime_hypotheses_are_bounded_and_unique() {
        let result =
            ReasoningEngine::default().reason("Build Rust Web API with database cache", vector());
        let pairs = runtime_hypotheses_from_reasoning(
            &result,
            &[
                ConceptId::from_name("DATABASE"),
                ConceptId::from_name("CACHE"),
                ConceptId::from_name("AUTH"),
            ],
        );

        assert!(!pairs.is_empty());
        assert!(pairs.len() <= 32 * 8);
    }
}
