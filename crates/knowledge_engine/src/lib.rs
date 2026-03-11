use std::sync::Arc;

use design_domain::Constraint;
use knowledge_store::KnowledgeStore;
use language_core::{
    Concept, ConceptId, RelationType as SemanticRelationType, SemanticGraph, SemanticRelation,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeQuery {
    pub text: String,
    pub semantic_hints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeSource {
    WebSearch,
    LocalDocument,
    ExperienceDerived,
    Inferred,
}

impl Default for KnowledgeSource {
    fn default() -> Self {
        Self::LocalDocument
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceReliability {
    pub source: KnowledgeSource,
    pub reliability_score: f64,
}

impl SourceReliability {
    pub fn from_source(source: &KnowledgeSource) -> Self {
        Self {
            source: source.clone(),
            reliability_score: default_reliability_for_source(source),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeMetadata {
    pub title: String,
    pub source_uri: String,
    pub reliability_hint: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeDocument {
    pub source: KnowledgeSource,
    pub content: String,
    pub metadata: KnowledgeMetadata,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct EntityId(pub u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeEntity {
    pub id: EntityId,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum RelationType {
    Supports,
    Requires,
    Constrains,
    Recommends,
}

impl Default for RelationType {
    fn default() -> Self {
        Self::Supports
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeProvenance {
    pub source: KnowledgeSource,
    pub timestamp: u64,
    pub usage_count: u64,
    pub last_used: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeConfidence {
    pub inference_confidence: f64,
    pub source_reliability: f64,
    pub effective_confidence: f64,
}

impl KnowledgeConfidence {
    pub fn new(inference_confidence: f64, source_reliability: f64) -> Self {
        let inference_confidence = inference_confidence.clamp(0.0, 1.0);
        let source_reliability = source_reliability.clamp(0.0, 1.0);
        Self {
            inference_confidence,
            source_reliability,
            effective_confidence: (inference_confidence * source_reliability)
                .sqrt()
                .clamp(0.0, 1.0),
        }
    }

    pub fn with_inference_confidence(&self, inference_confidence: f64) -> Self {
        Self::new(inference_confidence, self.source_reliability)
    }

    pub fn with_source_reliability(&self, source_reliability: f64) -> Self {
        Self::new(self.inference_confidence, source_reliability)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeRelation {
    pub source: EntityId,
    pub target: EntityId,
    pub relation_type: RelationType,
    pub confidence: KnowledgeConfidence,
    pub provenance: KnowledgeProvenance,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeGraph {
    pub entities: Vec<KnowledgeEntity>,
    pub relations: Vec<KnowledgeRelation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ValidationScore {
    pub consistency: f64,
    pub source_reliability: f64,
    pub confidence: f64,
}

pub trait KnowledgeRetriever: Send + Sync {
    fn retrieve(&self, query: KnowledgeQuery) -> Vec<KnowledgeDocument>;
}

#[derive(Clone, Default)]
pub struct LocalDocumentRetriever;

#[derive(Clone)]
pub struct WebSearchRetriever {
    store: Arc<KnowledgeStore>,
}

impl Default for Box<dyn KnowledgeRetriever> {
    fn default() -> Self {
        Box::new(WebSearchRetriever::default())
    }
}

#[derive(Clone, Debug, Default)]
pub struct KnowledgeParser;

#[derive(Clone, Debug, Default)]
pub struct KnowledgeValidator;

pub struct KnowledgeEngine {
    pub retriever: Box<dyn KnowledgeRetriever>,
    pub parser: KnowledgeParser,
    pub validator: KnowledgeValidator,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeIntegration {
    pub documents: Vec<KnowledgeDocument>,
    pub knowledge_graph: KnowledgeGraph,
    pub validation: ValidationScore,
}

impl Default for WebSearchRetriever {
    fn default() -> Self {
        let mut store = KnowledgeStore::new();
        store.preload_defaults();
        Self {
            store: Arc::new(store),
        }
    }
}

impl KnowledgeEngine {
    pub fn new(retriever: impl KnowledgeRetriever + 'static) -> Self {
        Self {
            retriever: Box::new(retriever),
            parser: KnowledgeParser,
            validator: KnowledgeValidator,
        }
    }

    pub fn process_query(&self, query: KnowledgeQuery) -> KnowledgeIntegration {
        let documents = self.retriever.retrieve(query);
        let knowledge_graph = self.parser.parse_documents(&documents);
        let validation = self.validator.validate(&knowledge_graph, &documents);
        KnowledgeIntegration {
            documents,
            knowledge_graph,
            validation,
        }
    }
}

impl Default for KnowledgeEngine {
    fn default() -> Self {
        Self::new(WebSearchRetriever::default())
    }
}

impl KnowledgeRetriever for LocalDocumentRetriever {
    fn retrieve(&self, query: KnowledgeQuery) -> Vec<KnowledgeDocument> {
        let lower = query.text.to_ascii_lowercase();
        let mut documents = Vec::new();
        if lower.contains("rest") || lower.contains("api") {
            documents.push(KnowledgeDocument {
                source: KnowledgeSource::LocalDocument,
                content:
                    "REST API should remain stateless. API gateway requires service discovery."
                        .to_string(),
                metadata: KnowledgeMetadata {
                    title: "REST API architecture notes".to_string(),
                    source_uri: "local://architecture/rest-api".to_string(),
                    reliability_hint: 0.9,
                },
            });
        }
        if lower.contains("cache") || lower.contains("scalable") {
            documents.push(KnowledgeDocument {
                source: KnowledgeSource::LocalDocument,
                content: "Scalable service recommends cache strategy and stateless controller."
                    .to_string(),
                metadata: KnowledgeMetadata {
                    title: "Scalability notes".to_string(),
                    source_uri: "local://architecture/scalability".to_string(),
                    reliability_hint: 0.85,
                },
            });
        }
        documents
    }
}

impl KnowledgeRetriever for WebSearchRetriever {
    fn retrieve(&self, query: KnowledgeQuery) -> Vec<KnowledgeDocument> {
        let lower = normalize_query(&query);
        let mut documents = self
            .store
            .labels()
            .iter()
            .filter(|label| lower.contains(&label.to_ascii_lowercase()))
            .filter_map(|label| {
                self.store
                    .get_prompt_by_label(label)
                    .map(|prompt| KnowledgeDocument {
                        source: KnowledgeSource::WebSearch,
                        content: format!("{label} requires stateless API gateway. {prompt}"),
                        metadata: KnowledgeMetadata {
                            title: label.clone(),
                            source_uri: format!("web://knowledge/{label}"),
                            reliability_hint: 0.7,
                        },
                    })
            })
            .collect::<Vec<_>>();

        if lower.contains("rest") || lower.contains("api") {
            documents.push(KnowledgeDocument {
                source: KnowledgeSource::WebSearch,
                content:
                    "REST API should remain stateless. API gateway requires service discovery."
                        .to_string(),
                metadata: KnowledgeMetadata {
                    title: "REST API web result".to_string(),
                    source_uri: "web://knowledge/rest-api".to_string(),
                    reliability_hint: 0.72,
                },
            });
        }
        if lower.contains("scalable") || lower.contains("cache") {
            documents.push(KnowledgeDocument {
                source: KnowledgeSource::WebSearch,
                content: "Scalable service recommends cache strategy and layered architecture."
                    .to_string(),
                metadata: KnowledgeMetadata {
                    title: "Scalability web result".to_string(),
                    source_uri: "web://knowledge/scalability".to_string(),
                    reliability_hint: 0.7,
                },
            });
        }

        if documents.is_empty() {
            let fallback_labels = self
                .store
                .labels()
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>();
            for label in fallback_labels {
                if let Some(prompt) = self.store.get_prompt_by_label(&label) {
                    documents.push(KnowledgeDocument {
                        source: KnowledgeSource::WebSearch,
                        content: format!("{label} recommends layered architecture. {prompt}"),
                        metadata: KnowledgeMetadata {
                            title: label.clone(),
                            source_uri: format!("web://knowledge/{label}"),
                            reliability_hint: 0.65,
                        },
                    });
                }
            }
        }

        documents.sort_by(|lhs, rhs| lhs.metadata.title.cmp(&rhs.metadata.title));
        documents.dedup_by(|lhs, rhs| lhs.metadata.source_uri == rhs.metadata.source_uri);
        documents
    }
}

impl KnowledgeParser {
    pub fn parse(&self, doc: KnowledgeDocument) -> KnowledgeGraph {
        self.parse_documents(&[doc])
    }

    pub fn parse_documents(&self, docs: &[KnowledgeDocument]) -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::default();
        for (idx, doc) in docs.iter().enumerate() {
            let lower = doc.content.to_ascii_lowercase();
            for label in infer_labels(&lower) {
                ensure_entity(&mut graph, label);
            }
            let timestamp = idx as u64 + 1;
            let source_reliability =
                default_reliability_for_source(&doc.source) * doc.metadata.reliability_hint.clamp(0.0, 1.0);
            let inference_confidence = doc.metadata.reliability_hint.clamp(0.0, 1.0);
            infer_relation(
                &mut graph,
                "rest",
                "stateless",
                RelationType::Constrains,
                doc.source.clone(),
                inference_confidence,
                timestamp,
                source_reliability,
            );
            infer_relation(
                &mut graph,
                "api_gateway",
                "service_discovery",
                RelationType::Requires,
                doc.source.clone(),
                inference_confidence,
                timestamp,
                source_reliability,
            );
            infer_relation(
                &mut graph,
                "scalable",
                "cache_strategy",
                RelationType::Recommends,
                doc.source.clone(),
                inference_confidence,
                timestamp,
                source_reliability,
            );
            infer_relation(
                &mut graph,
                "layered_architecture",
                "service",
                RelationType::Supports,
                doc.source.clone(),
                inference_confidence,
                timestamp,
                source_reliability,
            );
        }
        graph.entities.sort_by_key(|entity| entity.id);
        graph.relations.sort_by(|lhs, rhs| {
            (lhs.source, lhs.target, lhs.relation_type)
                .cmp(&(rhs.source, rhs.target, rhs.relation_type))
        });
        graph.relations.dedup_by(|lhs, rhs| {
            let same_edge = lhs.source == rhs.source
                && lhs.target == rhs.target
                && lhs.relation_type == rhs.relation_type;
            if same_edge {
                if lhs.confidence.effective_confidence < rhs.confidence.effective_confidence {
                    lhs.confidence = rhs.confidence.clone();
                }
                lhs.provenance.usage_count = lhs
                    .provenance
                    .usage_count
                    .saturating_add(rhs.provenance.usage_count);
                lhs.provenance.last_used = lhs.provenance.last_used.max(rhs.provenance.last_used);
            }
            same_edge
        });
        graph
    }
}

impl KnowledgeValidator {
    pub fn validate(
        &self,
        graph: &KnowledgeGraph,
        documents: &[KnowledgeDocument],
    ) -> ValidationScore {
        let entity_count = graph.entities.len() as f64;
        let relation_count = graph.relations.len() as f64;
        let consistency = if entity_count == 0.0 {
            0.0
        } else {
            (relation_count / entity_count.max(1.0)).clamp(0.0, 1.0)
        };
        let source_reliability = if documents.is_empty() {
            0.0
        } else {
            documents
                .iter()
                .map(|doc| {
                    default_reliability_for_source(&doc.source)
                        * doc.metadata.reliability_hint.clamp(0.0, 1.0)
                })
                .sum::<f64>()
                / documents.len() as f64
        };
        let confidence = ((consistency + source_reliability) / 2.0).clamp(0.0, 1.0);
        ValidationScore {
            consistency,
            source_reliability,
            confidence,
        }
    }
}

pub fn knowledge_query_from_semantic_graph(
    graph: &SemanticGraph,
    source_text: &str,
) -> KnowledgeQuery {
    let mut semantic_hints = graph
        .concepts
        .values()
        .map(|concept| concept.label.clone())
        .collect::<Vec<_>>();
    semantic_hints.sort();
    semantic_hints.dedup();
    KnowledgeQuery {
        text: if source_text.trim().is_empty() {
            semantic_hints.join(" ")
        } else {
            source_text.to_string()
        },
        semantic_hints,
    }
}

pub fn integrate_knowledge_into_semantic_graph(
    semantic_graph: &mut SemanticGraph,
    knowledge_graph: &KnowledgeGraph,
) {
    for entity in &knowledge_graph.entities {
        let concept_id = ConceptId(10_000 + entity.id.0);
        if !semantic_graph
            .concepts
            .values()
            .any(|concept| concept.label == entity.label)
        {
            semantic_graph.add_concept(Concept {
                concept_id,
                label: entity.label.clone(),
                attributes: Vec::new(),
            });
        }
    }

    for relation in &knowledge_graph.relations {
        let Some(source) = find_concept_id(semantic_graph, knowledge_graph, relation.source) else {
            continue;
        };
        let Some(target) = find_concept_id(semantic_graph, knowledge_graph, relation.target) else {
            continue;
        };
        semantic_graph.add_relation(SemanticRelation {
            source,
            target,
            relation: match relation.relation_type {
                RelationType::Supports => SemanticRelationType::Clarifies,
                RelationType::Requires => SemanticRelationType::Requires,
                RelationType::Constrains => SemanticRelationType::Constrains,
                RelationType::Recommends => SemanticRelationType::Pattern,
            },
        });
    }
}

pub fn knowledge_graph_to_constraints(graph: &KnowledgeGraph) -> Vec<Constraint> {
    let labels = graph
        .entities
        .iter()
        .map(|entity| entity.label.as_str())
        .collect::<Vec<_>>();
    let mut constraints = Vec::new();
    if labels.contains(&"stateless") {
        constraints.push(Constraint {
            name: "knowledge_stateless".to_string(),
            max_design_units: Some(20),
            max_dependencies: Some(20),
        });
    }
    if labels.contains(&"api_gateway") {
        constraints.push(Constraint {
            name: "knowledge_api_gateway".to_string(),
            max_design_units: Some(28),
            max_dependencies: Some(36),
        });
    }
    if labels.contains(&"layered_architecture") {
        constraints.push(Constraint {
            name: "knowledge_layered_architecture".to_string(),
            max_design_units: Some(24),
            max_dependencies: Some(28),
        });
    }
    constraints
}

fn normalize_query(query: &KnowledgeQuery) -> String {
    let mut text = query.text.to_ascii_lowercase();
    for hint in &query.semantic_hints {
        text.push(' ');
        text.push_str(&hint.to_ascii_lowercase());
    }
    text
}

fn infer_labels(lower: &str) -> Vec<&'static str> {
    let mut labels = Vec::new();
    for (needle, label) in [
        ("rest", "rest"),
        ("api gateway", "api_gateway"),
        ("api_gateway", "api_gateway"),
        ("service discovery", "service_discovery"),
        ("service_discovery", "service_discovery"),
        ("stateless", "stateless"),
        ("cache", "cache_strategy"),
        ("scalable", "scalable"),
        ("layered", "layered_architecture"),
        ("service", "service"),
    ] {
        if lower.contains(needle) {
            labels.push(label);
        }
    }
    labels.sort();
    labels.dedup();
    labels
}

fn ensure_entity(graph: &mut KnowledgeGraph, label: &str) -> EntityId {
    if let Some(entity) = graph.entities.iter().find(|entity| entity.label == label) {
        return entity.id;
    }
    let id = EntityId(graph.entities.len() as u64 + 1);
    graph.entities.push(KnowledgeEntity {
        id,
        label: label.to_string(),
    });
    id
}

fn infer_relation(
    graph: &mut KnowledgeGraph,
    source_label: &str,
    target_label: &str,
    relation_type: RelationType,
    knowledge_source: KnowledgeSource,
    inference_confidence: f64,
    timestamp: u64,
    source_reliability: f64,
) {
    let Some(source) = graph
        .entities
        .iter()
        .find(|entity| entity.label == source_label)
        .map(|entity| entity.id)
    else {
        return;
    };
    let Some(target) = graph
        .entities
        .iter()
        .find(|entity| entity.label == target_label)
        .map(|entity| entity.id)
    else {
        return;
    };
    graph.relations.push(KnowledgeRelation {
        source,
        target,
        relation_type,
        confidence: KnowledgeConfidence::new(inference_confidence, source_reliability),
        provenance: KnowledgeProvenance {
            source: knowledge_source,
            timestamp,
            usage_count: 1,
            last_used: timestamp,
        },
    });
}

pub fn default_reliability_for_source(source: &KnowledgeSource) -> f64 {
    match source {
        KnowledgeSource::LocalDocument => 0.9,
        KnowledgeSource::ExperienceDerived => 0.85,
        KnowledgeSource::Inferred => 0.75,
        KnowledgeSource::WebSearch => 0.6,
    }
}

fn find_concept_id(
    semantic_graph: &SemanticGraph,
    knowledge_graph: &KnowledgeGraph,
    entity_id: EntityId,
) -> Option<ConceptId> {
    let label = knowledge_graph
        .entities
        .iter()
        .find(|entity| entity.id == entity_id)?
        .label
        .as_str();
    semantic_graph
        .concepts
        .values()
        .find(|concept| concept.label == label)
        .map(|concept| concept.concept_id)
}
