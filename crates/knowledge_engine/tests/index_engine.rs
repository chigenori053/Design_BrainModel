use knowledge_engine::{
    KnowledgeEngine, KnowledgeIndexConfig, KnowledgeIndexEngine, KnowledgeParser, KnowledgeQuery,
    KnowledgeRetriever, LocalDocumentRetriever, QueryCache, QueryNormalizer, RelationType,
    WebSearchRetriever,
};
use std::sync::{Arc, Mutex};

#[test]
fn knowledge_index_engine_filters_and_ranks_results() {
    let retriever = WebSearchRetriever::default();
    let parser = KnowledgeParser;
    let query = KnowledgeQuery {
        text: "Build scalable REST API with cache".to_string(),
        semantic_hints: vec!["REST API".to_string(), "Cache".to_string()],
        semantic_vector: vec![0.3, 0.1, 0.5, 0.2],
        keywords: vec!["rest".to_string(), "cache".to_string()],
        relation_types: vec![RelationType::Recommends],
        max_results: 8,
        confidence_threshold: 0.1,
    };
    let documents = retriever.retrieve(query.clone());
    let index = KnowledgeIndexEngine::build(
        &documents,
        &parser,
        KnowledgeIndexConfig::default(),
        Arc::new(Mutex::new(QueryCache::default())),
    );
    let (result_set, selected_documents, telemetry) = index.execute(query, &documents, &parser);

    assert!(!result_set.results.is_empty());
    assert!(!selected_documents.is_empty());
    assert!(
        result_set
            .results
            .windows(2)
            .all(|pair| { pair[0].ranking_score >= pair[1].ranking_score })
    );
    assert!(
        telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeQueryIssued")
    );
    assert!(
        telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeIndexHit")
    );
    assert!(
        telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeResultRanked")
    );
}

#[test]
fn knowledge_engine_process_query_uses_index_results() {
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let out = engine.process_query(KnowledgeQuery {
        text: "Build REST API".to_string(),
        semantic_hints: vec!["REST API".to_string()],
        semantic_vector: vec![0.2, 0.4, 0.1],
        keywords: vec!["rest".to_string(), "api".to_string()],
        relation_types: vec![RelationType::Requires],
        max_results: 4,
        confidence_threshold: 0.1,
    });

    assert!(!out.documents.is_empty());
    assert!(!out.result_set.results.is_empty());
    assert!(
        out.index_telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeQueryResult")
    );
}

#[test]
fn query_normalization_normalizes_vector_and_terms() {
    let normalized = QueryNormalizer::normalize(KnowledgeQuery {
        text: "  Build REST API  ".to_string(),
        semantic_hints: vec!["REST API".to_string(), "rest api".to_string()],
        semantic_vector: vec![3.0, 4.0],
        keywords: vec![" Cache ".to_string(), "cache".to_string()],
        relation_types: vec![RelationType::Requires],
        max_results: 0,
        confidence_threshold: 1.2,
    });

    assert_eq!(normalized.text, "Build REST API");
    assert_eq!(normalized.semantic_hints, vec!["rest api".to_string()]);
    assert_eq!(normalized.keywords, vec!["cache".to_string()]);
    assert_eq!(normalized.max_results, 1);
    assert_eq!(normalized.confidence_threshold, 1.0);
    assert!((normalized.semantic_vector[0] - 0.6).abs() < 1e-5);
    assert!((normalized.semantic_vector[1] - 0.8).abs() < 1e-5);
}

#[test]
fn query_cache_hits_on_repeated_query() {
    let retriever = LocalDocumentRetriever;
    let parser = KnowledgeParser;
    let query = KnowledgeQuery {
        text: "Build REST API with cache".to_string(),
        semantic_hints: vec!["REST API".to_string()],
        semantic_vector: vec![0.3, 0.2, 0.1],
        keywords: vec!["rest".to_string(), "cache".to_string()],
        relation_types: vec![RelationType::Requires],
        max_results: 4,
        confidence_threshold: 0.1,
    };
    let documents = retriever.retrieve(query.clone());
    let cache = Arc::new(Mutex::new(QueryCache::new(8)));
    let index = KnowledgeIndexEngine::build(
        &documents,
        &parser,
        KnowledgeIndexConfig::default(),
        Arc::clone(&cache),
    );

    let (_, _, first_telemetry) = index.execute(query.clone(), &documents, &parser);
    let (_, _, second_telemetry) = index.execute(query, &documents, &parser);

    assert!(
        first_telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeQueryCacheMiss")
    );
    assert!(
        second_telemetry
            .events
            .iter()
            .any(|event| event.name == "KnowledgeQueryCacheHit")
    );
}

#[test]
fn graph_expansion_limit_is_enforced() {
    let retriever = WebSearchRetriever::default();
    let parser = KnowledgeParser;
    let query = KnowledgeQuery {
        text: "Build scalable REST API with service discovery and cache".to_string(),
        semantic_hints: vec!["REST API".to_string(), "service discovery".to_string()],
        semantic_vector: vec![0.4, 0.2, 0.3, 0.1],
        keywords: vec!["service".to_string()],
        relation_types: vec![RelationType::Requires, RelationType::Recommends],
        max_results: 8,
        confidence_threshold: 0.0,
    };
    let documents = retriever.retrieve(query.clone());
    let index = KnowledgeIndexEngine::build(
        &documents,
        &parser,
        KnowledgeIndexConfig {
            max_graph_expansion: 1,
            max_graph_depth: 1,
            ..KnowledgeIndexConfig::default()
        },
        Arc::new(Mutex::new(QueryCache::default())),
    );

    let (_, _, telemetry) = index.execute(query, &documents, &parser);
    let expansion = telemetry
        .events
        .iter()
        .find(|event| event.name == "KnowledgeGraphExpansion")
        .expect("graph expansion telemetry");
    assert!(expansion.detail.contains("expanded_docs=1"));
    assert!(expansion.detail.contains("max_depth=1"));
}

#[test]
fn ranking_stability_uses_weighted_score_order() {
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let out = engine.process_query(KnowledgeQuery {
        text: "Build scalable REST API with cache".to_string(),
        semantic_hints: vec!["REST API".to_string(), "cache".to_string()],
        semantic_vector: vec![0.3, 0.1, 0.5, 0.2],
        keywords: vec!["rest".to_string(), "cache".to_string()],
        relation_types: vec![RelationType::Recommends],
        max_results: 8,
        confidence_threshold: 0.0,
    });

    assert!(!out.result_set.results.is_empty());
    assert!(
        out.result_set
            .results
            .windows(2)
            .all(|pair| { pair[0].ranking_score >= pair[1].ranking_score })
    );
    assert!(out.result_set.results.iter().all(|result| {
        let expected = (0.6 * result.similarity)
            + (0.3 * result.confidence)
            + (0.1 * result.source_reliability);
        (result.ranking_score - expected).abs() < 1e-9
    }));
}
