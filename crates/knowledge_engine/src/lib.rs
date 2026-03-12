use std::collections::{BTreeMap, BTreeSet, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::Mutex;

use design_domain::Constraint;
use knowledge_store::KnowledgeStore;
use language_core::{
    Concept, ConceptId, RelationType as SemanticRelationType, SemanticGraph, SemanticRelation,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeQuery {
    pub text: String,
    pub semantic_hints: Vec<String>,
    pub semantic_vector: Vec<f32>,
    pub keywords: Vec<String>,
    pub relation_types: Vec<RelationType>,
    pub max_results: usize,
    pub confidence_threshold: f64,
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

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeResult {
    pub knowledge_id: String,
    pub similarity: f64,
    pub confidence: f64,
    pub source_reliability: f64,
    pub ranking_score: f64,
    pub source: KnowledgeSource,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeResultSet {
    pub results: Vec<KnowledgeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeIndexTelemetryEvent {
    pub name: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeIndexTelemetry {
    pub events: Vec<KnowledgeIndexTelemetryEvent>,
}

impl KnowledgeIndexTelemetry {
    fn push(&mut self, name: &'static str, detail: impl Into<String>) {
        self.events.push(KnowledgeIndexTelemetryEvent {
            name,
            detail: detail.into(),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnowledgeIndexConfig {
    pub max_semantic_results: usize,
    pub max_graph_expansion: usize,
    pub max_cluster_candidates: usize,
    pub max_cache_size: usize,
    pub max_graph_depth: usize,
}

impl Default for KnowledgeIndexConfig {
    fn default() -> Self {
        Self {
            max_semantic_results: 64,
            max_graph_expansion: 32,
            max_cluster_candidates: 16,
            max_cache_size: 1024,
            max_graph_depth: 3,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticIndex {
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphIndex {
    pub adjacency: BTreeMap<EntityId, Vec<EntityId>>,
    pub relation_docs: BTreeMap<RelationType, Vec<usize>>,
    pub entity_docs: BTreeMap<EntityId, Vec<usize>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClusterIndex {
    pub representatives: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProvenanceIndex {
    pub reliability_by_doc: BTreeMap<usize, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeQueryEngine;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryCacheEntry {
    pub query_hash: u64,
    pub result_set: KnowledgeResultSet,
    pub timestamp: u64,
}

#[derive(Debug)]
pub struct QueryCache {
    pub max_cache_size: usize,
    entries: BTreeMap<u64, QueryCacheEntry>,
    clock: u64,
}

impl QueryCache {
    pub fn new(max_cache_size: usize) -> Self {
        Self {
            max_cache_size: max_cache_size.max(1),
            entries: BTreeMap::new(),
            clock: 0,
        }
    }

    pub fn lookup(&mut self, query_hash: u64) -> Option<KnowledgeResultSet> {
        if let Some(entry) = self.entries.get_mut(&query_hash) {
            self.clock = self.clock.saturating_add(1);
            entry.timestamp = self.clock;
            return Some(entry.result_set.clone());
        }
        None
    }

    pub fn store(&mut self, query_hash: u64, result_set: KnowledgeResultSet) {
        self.clock = self.clock.saturating_add(1);
        self.entries.insert(
            query_hash,
            QueryCacheEntry {
                query_hash,
                result_set,
                timestamp: self.clock,
            },
        );

        while self.entries.len() > self.max_cache_size {
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.timestamp)
                .map(|(key, _)| *key)
            {
                self.entries.remove(&oldest_key);
            } else {
                break;
            }
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(KnowledgeIndexConfig::default().max_cache_size)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryNormalizer;

#[derive(Debug)]
pub struct KnowledgeIndexEngine {
    pub semantic_index: SemanticIndex,
    pub graph_index: GraphIndex,
    pub cluster_index: ClusterIndex,
    pub provenance_index: ProvenanceIndex,
    pub query_engine: KnowledgeQueryEngine,
    pub config: KnowledgeIndexConfig,
    pub query_cache: Arc<Mutex<QueryCache>>,
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
    pub query_cache: Arc<Mutex<QueryCache>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeIntegration {
    pub documents: Vec<KnowledgeDocument>,
    pub knowledge_graph: KnowledgeGraph,
    pub validation: ValidationScore,
    pub result_set: KnowledgeResultSet,
    pub index_telemetry: KnowledgeIndexTelemetry,
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
            query_cache: Arc::new(Mutex::new(QueryCache::default())),
        }
    }

    pub fn process_query(&self, query: KnowledgeQuery) -> KnowledgeIntegration {
        let normalized_query = QueryNormalizer::normalize(query);
        let documents = self.retriever.retrieve(normalized_query.clone());
        let index = KnowledgeIndexEngine::build(
            &documents,
            &self.parser,
            KnowledgeIndexConfig::default(),
            Arc::clone(&self.query_cache),
        );
        let (result_set, selected_documents, index_telemetry) =
            index.execute(normalized_query, &documents, &self.parser);
        let knowledge_graph = self.parser.parse_documents(&selected_documents);
        let validation = self
            .validator
            .validate(&knowledge_graph, &selected_documents);
        KnowledgeIntegration {
            documents: selected_documents,
            knowledge_graph,
            validation,
            result_set,
            index_telemetry,
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
        let lower = normalize_query_text(&query);
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

impl KnowledgeIndexEngine {
    pub fn build(
        documents: &[KnowledgeDocument],
        parser: &KnowledgeParser,
        config: KnowledgeIndexConfig,
        query_cache: Arc<Mutex<QueryCache>>,
    ) -> Self {
        let semantic_index = SemanticIndex {
            vectors: documents
                .iter()
                .map(|document| {
                    embed_text(&(document.content.clone() + " " + &document.metadata.title))
                })
                .collect(),
        };

        let mut relation_docs = BTreeMap::<RelationType, Vec<usize>>::new();
        let mut adjacency = BTreeMap::<EntityId, Vec<EntityId>>::new();
        let mut entity_docs = BTreeMap::<EntityId, Vec<usize>>::new();
        for (idx, document) in documents.iter().cloned().enumerate() {
            let graph = parser.parse(document);
            for entity in &graph.entities {
                entity_docs.entry(entity.id).or_default().push(idx);
            }
            for relation in &graph.relations {
                relation_docs
                    .entry(relation.relation_type)
                    .or_default()
                    .push(idx);
                adjacency
                    .entry(relation.source)
                    .or_default()
                    .push(relation.target);
            }
        }
        for docs in relation_docs.values_mut() {
            docs.sort_unstable();
            docs.dedup();
        }
        for neighbors in adjacency.values_mut() {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        for docs in entity_docs.values_mut() {
            docs.sort_unstable();
            docs.dedup();
        }

        let cluster_index = ClusterIndex {
            representatives: documents.iter().enumerate().fold(
                BTreeMap::new(),
                |mut clusters, (idx, document)| {
                    clusters.entry(cluster_key(document)).or_insert(idx);
                    clusters
                },
            ),
        };

        let provenance_index = ProvenanceIndex {
            reliability_by_doc: documents
                .iter()
                .enumerate()
                .map(|(idx, document)| {
                    (
                        idx,
                        (default_reliability_for_source(&document.source)
                            * document.metadata.reliability_hint.clamp(0.0, 1.0))
                        .clamp(0.0, 1.0),
                    )
                })
                .collect(),
        };

        Self {
            semantic_index,
            graph_index: GraphIndex {
                adjacency,
                relation_docs,
                entity_docs,
            },
            cluster_index,
            provenance_index,
            query_engine: KnowledgeQueryEngine,
            config,
            query_cache,
        }
    }

    pub fn execute(
        &self,
        query: KnowledgeQuery,
        documents: &[KnowledgeDocument],
        parser: &KnowledgeParser,
    ) -> (
        KnowledgeResultSet,
        Vec<KnowledgeDocument>,
        KnowledgeIndexTelemetry,
    ) {
        self.query_engine.execute(query, documents, self, parser)
    }
}

impl KnowledgeQueryEngine {
    pub fn execute(
        &self,
        query: KnowledgeQuery,
        documents: &[KnowledgeDocument],
        index: &KnowledgeIndexEngine,
        parser: &KnowledgeParser,
    ) -> (
        KnowledgeResultSet,
        Vec<KnowledgeDocument>,
        KnowledgeIndexTelemetry,
    ) {
        let mut telemetry = KnowledgeIndexTelemetry::default();
        let normalized_query = QueryNormalizer::normalize(query);
        let query_hash = hash_query(&normalized_query);
        telemetry.push(
            "KnowledgeQueryIssued",
            format!(
                "keywords={}, relation_types={}, max_results={}",
                normalized_query.keywords.len(),
                normalized_query.relation_types.len(),
                normalized_query.max_results
            ),
        );

        if let Some(cached) = index
            .query_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.lookup(query_hash))
        {
            telemetry.push("KnowledgeQueryCacheHit", format!("query_hash={query_hash}"));
            let selected_documents = cached
                .results
                .iter()
                .filter_map(|result| {
                    documents
                        .iter()
                        .find(|document| document.metadata.source_uri == result.knowledge_id)
                        .cloned()
                })
                .collect::<Vec<_>>();
            telemetry.push(
                "KnowledgeQueryResult",
                format!("selected_documents={}", selected_documents.len()),
            );
            return (cached, selected_documents, telemetry);
        }
        telemetry.push(
            "KnowledgeQueryCacheMiss",
            format!("query_hash={query_hash}"),
        );

        let semantic_query = if normalized_query.semantic_vector.is_empty() {
            normalize_vector(embed_text(&normalize_query_text(&normalized_query)))
        } else {
            normalized_query.semantic_vector.clone()
        };

        let mut semantic_hits = index
            .semantic_index
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, vector)| (idx, cosine_similarity(&semantic_query, vector)))
            .collect::<Vec<_>>();
        semantic_hits.sort_by(|lhs, rhs| rhs.1.total_cmp(&lhs.1).then_with(|| lhs.0.cmp(&rhs.0)));
        semantic_hits.truncate(
            index
                .config
                .max_semantic_results
                .min(documents.len())
                .max(1),
        );

        let mut candidate_ids = semantic_hits
            .iter()
            .map(|(idx, _)| *idx)
            .collect::<Vec<_>>();

        let cluster_representatives = index
            .cluster_index
            .representatives
            .values()
            .copied()
            .take(index.config.max_cluster_candidates)
            .collect::<BTreeSet<_>>();
        candidate_ids.retain(|idx| cluster_representatives.contains(idx));

        if !normalized_query.relation_types.is_empty() {
            let relation_expansion =
                expand_graph_candidates(index, &normalized_query.relation_types, &mut telemetry);
            candidate_ids.retain(|idx| relation_expansion.contains(idx));
        }

        let query_lower = normalize_query_text(&normalized_query);
        candidate_ids.retain(|idx| {
            let document = &documents[*idx];
            normalized_query.keywords.iter().all(|keyword| {
                document
                    .content
                    .to_ascii_lowercase()
                    .contains(&keyword.to_ascii_lowercase())
                    || document
                        .metadata
                        .title
                        .to_ascii_lowercase()
                        .contains(&keyword.to_ascii_lowercase())
                    || query_lower.contains(&keyword.to_ascii_lowercase())
            })
        });

        let mut results = candidate_ids
            .into_iter()
            .map(|idx| {
                let similarity = semantic_hits
                    .iter()
                    .find(|(hit_idx, _)| *hit_idx == idx)
                    .map(|(_, score)| *score)
                    .unwrap_or(0.0);
                let confidence = index
                    .provenance_index
                    .reliability_by_doc
                    .get(&idx)
                    .copied()
                    .unwrap_or_default();
                let source_reliability = confidence;
                let ranking_score =
                    (0.6 * similarity) + (0.3 * confidence) + (0.1 * source_reliability);
                (
                    idx,
                    similarity,
                    confidence,
                    source_reliability,
                    ranking_score,
                )
            })
            .filter(|(_, _, confidence, _, _)| *confidence >= normalized_query.confidence_threshold)
            .collect::<Vec<_>>();

        results.sort_by(|lhs, rhs| {
            rhs.4
                .total_cmp(&lhs.4)
                .then_with(|| rhs.1.total_cmp(&lhs.1))
                .then_with(|| rhs.2.total_cmp(&lhs.2))
                .then_with(|| lhs.0.cmp(&rhs.0))
        });
        results.truncate(normalized_query.max_results.max(1));

        let selected_documents = results
            .iter()
            .map(|(idx, _, _, _, _)| documents[*idx].clone())
            .collect::<Vec<_>>();
        let graph = parser.parse_documents(&selected_documents);

        telemetry.push(
            "KnowledgeResultRanked",
            format!("ranked_results={}", results.len()),
        );

        let result_set = KnowledgeResultSet {
            results: results
                .iter()
                .map(
                    |(idx, similarity, confidence, source_reliability, ranking_score)| {
                        KnowledgeResult {
                            knowledge_id: documents[*idx].metadata.source_uri.clone(),
                            similarity: *similarity,
                            confidence: *confidence,
                            source_reliability: *source_reliability,
                            ranking_score: *ranking_score,
                            source: documents[*idx].source.clone(),
                        }
                    },
                )
                .collect(),
        };

        if let Ok(mut cache) = index.query_cache.lock() {
            cache.store(query_hash, result_set.clone());
        }

        if result_set.results.is_empty() {
            telemetry.push("KnowledgeIndexMiss", "no indexed results matched");
        } else {
            telemetry.push(
                "KnowledgeIndexHit",
                format!(
                    "hits={}, entities={}",
                    result_set.results.len(),
                    graph.entities.len()
                ),
            );
        }
        telemetry.push(
            "KnowledgeQueryResult",
            format!("selected_documents={}", selected_documents.len()),
        );

        (result_set, selected_documents, telemetry)
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
            let source_reliability = default_reliability_for_source(&doc.source)
                * doc.metadata.reliability_hint.clamp(0.0, 1.0);
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
            (lhs.source, lhs.target, lhs.relation_type).cmp(&(
                rhs.source,
                rhs.target,
                rhs.relation_type,
            ))
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
    let semantic_vector = embed_text(&semantic_hints.join(" "));
    KnowledgeQuery {
        text: if source_text.trim().is_empty() {
            semantic_hints.join(" ")
        } else {
            source_text.to_string()
        },
        keywords: semantic_hints.clone(),
        semantic_hints,
        semantic_vector,
        relation_types: Vec::new(),
        max_results: KnowledgeIndexConfig::default().max_semantic_results,
        confidence_threshold: 0.0,
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

fn normalize_query_text(query: &KnowledgeQuery) -> String {
    let mut text = query.text.to_ascii_lowercase();
    for hint in &query.semantic_hints {
        text.push(' ');
        text.push_str(&hint.to_ascii_lowercase());
    }
    for keyword in &query.keywords {
        text.push(' ');
        text.push_str(&keyword.to_ascii_lowercase());
    }
    text
}

fn embed_text(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; 16];
    for (idx, byte) in text.bytes().enumerate() {
        vector[idx % 16] += f32::from(byte) / 255.0;
    }
    normalize_vector(vector)
}

fn cosine_similarity(lhs: &[f32], rhs: &[f32]) -> f64 {
    let dot = lhs
        .iter()
        .zip(rhs.iter())
        .map(|(l, r)| f64::from(*l) * f64::from(*r))
        .sum::<f64>();
    let lhs_norm = lhs
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    let rhs_norm = rhs
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.0
    } else {
        (dot / (lhs_norm * rhs_norm)).clamp(0.0, 1.0)
    }
}

fn cluster_key(document: &KnowledgeDocument) -> String {
    let lower = format!("{} {}", document.metadata.title, document.content).to_ascii_lowercase();
    let labels = infer_labels(&lower);
    if labels.is_empty() {
        document.metadata.title.to_ascii_lowercase()
    } else {
        labels.join("|")
    }
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

impl QueryNormalizer {
    pub fn normalize(mut query: KnowledgeQuery) -> KnowledgeQuery {
        query.text = query.text.trim().to_string();
        query.semantic_hints = normalize_strings(query.semantic_hints);
        query.keywords = normalize_strings(query.keywords);
        query.semantic_vector = if query.semantic_vector.is_empty() {
            normalize_vector(embed_text(&normalize_query_text(&query)))
        } else {
            normalize_vector(query.semantic_vector)
        };
        query.max_results = query.max_results.max(1);
        query.confidence_threshold = query.confidence_threshold.clamp(0.0, 1.0);
        query
    }
}

fn normalize_strings(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn normalize_vector(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn hash_query(query: &KnowledgeQuery) -> u64 {
    let mut hasher = DefaultHasher::new();
    query.text.hash(&mut hasher);
    query.semantic_hints.hash(&mut hasher);
    query.keywords.hash(&mut hasher);
    query.relation_types.hash(&mut hasher);
    query.max_results.hash(&mut hasher);
    query.confidence_threshold.to_bits().hash(&mut hasher);
    for value in &query.semantic_vector {
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn expand_graph_candidates(
    index: &KnowledgeIndexEngine,
    relation_types: &[RelationType],
    telemetry: &mut KnowledgeIndexTelemetry,
) -> BTreeSet<usize> {
    let mut seed_docs = BTreeSet::new();
    let mut frontier = VecDeque::new();
    let mut visited_entities = BTreeSet::new();

    for relation_type in relation_types {
        if let Some(doc_ids) = index.graph_index.relation_docs.get(relation_type) {
            for doc_id in doc_ids.iter().take(index.config.max_graph_expansion) {
                seed_docs.insert(*doc_id);
                if seed_docs.len() >= index.config.max_graph_expansion {
                    break;
                }
            }
        }
        if seed_docs.len() >= index.config.max_graph_expansion {
            break;
        }
    }

    for (&entity_id, doc_ids) in &index.graph_index.entity_docs {
        if doc_ids.iter().any(|doc_id| seed_docs.contains(doc_id))
            && visited_entities.insert(entity_id)
        {
            frontier.push_back((entity_id, 0usize));
        }
    }

    let mut expanded_docs = seed_docs.clone();
    while let Some((entity_id, depth)) = frontier.pop_front() {
        if expanded_docs.len() >= index.config.max_graph_expansion {
            break;
        }

        if let Some(doc_ids) = index.graph_index.entity_docs.get(&entity_id) {
            for doc_id in doc_ids {
                expanded_docs.insert(*doc_id);
                if expanded_docs.len() >= index.config.max_graph_expansion {
                    break;
                }
            }
        }

        if depth >= index.config.max_graph_depth {
            continue;
        }

        if let Some(neighbors) = index.graph_index.adjacency.get(&entity_id) {
            for neighbor in neighbors {
                if visited_entities.insert(*neighbor) {
                    frontier.push_back((*neighbor, depth + 1));
                }
            }
        }
    }

    telemetry.push(
        "KnowledgeGraphExpansion",
        format!(
            "seed_docs={}, expanded_docs={}, max_depth={}",
            seed_docs.len(),
            expanded_docs.len(),
            index.config.max_graph_depth
        ),
    );

    expanded_docs
}
