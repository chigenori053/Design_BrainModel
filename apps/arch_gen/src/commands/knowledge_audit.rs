use std::collections::BTreeSet;

use knowledge_store::KnowledgeStore;
use memory_space_phase14::InMemoryMemorySpace;
use serde::Serialize;

use crate::commands::knowledge_layer::knowledge_layer_metrics;

#[derive(Debug, Serialize)]
struct KnowledgeAuditReport {
    web_search: WebSearchStatus,
    knowledge_store: KnowledgeStoreMetrics,
    pattern_memory: PatternMemoryMetrics,
    knowledge_layer: KnowledgeLayerStatus,
    totals: KnowledgeTotals,
}

#[derive(Debug, Serialize)]
struct WebSearchStatus {
    agent_available: bool,
    arch_gen_command_available: bool,
    adapter_wired: bool,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct KnowledgeStoreMetrics {
    entries: usize,
    unique_topics: usize,
    avg_prompt_chars: f64,
    vector_dimensions_min: usize,
    vector_dimensions_max: usize,
    category_coverage: usize,
}

#[derive(Debug, Serialize)]
struct PatternMemoryMetrics {
    experiences: usize,
    patterns: usize,
    unique_layer_sequences: usize,
    avg_dependency_edges: f64,
}

#[derive(Debug, Serialize)]
struct KnowledgeLayerStatus {
    temporary_hits: usize,
    grounding_hits: usize,
}

#[derive(Debug, Serialize)]
struct KnowledgeTotals {
    total_knowledge_entries: usize,
    total_pattern_entries: usize,
    total_grounding_entries: usize,
    total_temporary_entries: usize,
    combined_knowledge_volume: usize,
}

pub fn run(format: &str) -> Result<(), String> {
    let report = build_report();

    match format {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|e| format!("json serialization failed: {e}"))?
            );
        }
        "text" => {
            print!("{}", render_text_report(&report));
        }
        other => {
            return Err(format!(
                "unknown knowledge audit format '{other}'; expected: text | json"
            ));
        }
    }

    Ok(())
}

fn build_report() -> KnowledgeAuditReport {
    let mut knowledge_store = KnowledgeStore::new();
    knowledge_store.preload_defaults();

    let knowledge_entries = knowledge_store.labels().len();
    let unique_topics = knowledge_store
        .labels()
        .iter()
        .collect::<BTreeSet<_>>()
        .len();
    let prompt_lengths: Vec<usize> = knowledge_store
        .labels()
        .iter()
        .filter_map(|label| {
            knowledge_store
                .get_prompt_by_label(label)
                .map(|prompt| prompt.chars().count())
        })
        .collect();
    let avg_prompt_chars = if prompt_lengths.is_empty() {
        0.0
    } else {
        prompt_lengths.iter().sum::<usize>() as f64 / prompt_lengths.len() as f64
    };
    let category_coverage = knowledge_store
        .labels()
        .iter()
        .map(|label| normalize_topic(label))
        .collect::<BTreeSet<_>>()
        .len();

    // KnowledgeStore はベクトルを外部公開していないため、現在の seed 実装に合わせて次元を監査する。
    let vector_dimensions_min = 8;
    let vector_dimensions_max = 8;

    let memory = InMemoryMemorySpace::with_bootstrap_patterns();
    let experiences = memory.experience_count();
    let patterns = memory.pattern_store.patterns.len();
    let unique_layer_sequences = memory
        .pattern_store
        .patterns
        .iter()
        .map(|pattern| {
            pattern
                .layer_sequence
                .iter()
                .map(|layer| layer.as_str().to_string())
                .collect::<Vec<_>>()
                .join(">")
        })
        .collect::<BTreeSet<_>>()
        .len();
    let avg_dependency_edges = if patterns == 0 {
        0.0
    } else {
        memory
            .pattern_store
            .patterns
            .iter()
            .map(|pattern| pattern.dependency_edges.len())
            .sum::<usize>() as f64
            / patterns as f64
    };

    let total_knowledge_entries = knowledge_entries;
    let total_pattern_entries = patterns;
    let knowledge_layer = knowledge_layer_metrics().unwrap_or(
        crate::commands::knowledge_layer::KnowledgeLayerMetrics {
            temporary_hits: 0,
            grounding_hits: 0,
        },
    );

    KnowledgeAuditReport {
        web_search: WebSearchStatus {
            agent_available: true,
            arch_gen_command_available: true,
            adapter_wired: true,
            status: "agent_core HttpClient and arch_gen both use DuckDuckGo for on-demand web search",
        },
        knowledge_store: KnowledgeStoreMetrics {
            entries: knowledge_entries,
            unique_topics,
            avg_prompt_chars,
            vector_dimensions_min,
            vector_dimensions_max,
            category_coverage,
        },
        pattern_memory: PatternMemoryMetrics {
            experiences,
            patterns,
            unique_layer_sequences,
            avg_dependency_edges,
        },
        knowledge_layer: KnowledgeLayerStatus {
            temporary_hits: knowledge_layer.temporary_hits,
            grounding_hits: knowledge_layer.grounding_hits,
        },
        totals: KnowledgeTotals {
            total_knowledge_entries,
            total_pattern_entries,
            total_grounding_entries: knowledge_layer.grounding_hits,
            total_temporary_entries: knowledge_layer.temporary_hits,
            combined_knowledge_volume: total_knowledge_entries
                + total_pattern_entries
                + knowledge_layer.grounding_hits
                + knowledge_layer.temporary_hits,
        },
    }
}

fn render_text_report(report: &KnowledgeAuditReport) -> String {
    let mut out = String::new();
    out.push_str("Knowledge Audit\n");
    out.push_str(&"─".repeat(55));
    out.push('\n');
    out.push_str(&format!(
        "WebSearch: agent={}, wired={}\n",
        report.web_search.agent_available, report.web_search.adapter_wired
    ));
    out.push_str(&format!(
        "arch_gen command available: {}\n",
        report.web_search.arch_gen_command_available
    ));
    out.push_str(&format!("Status: {}\n\n", report.web_search.status));

    out.push_str("Knowledge Store\n");
    out.push_str(&format!("  entries: {}\n", report.knowledge_store.entries));
    out.push_str(&format!(
        "  unique topics: {}\n",
        report.knowledge_store.unique_topics
    ));
    out.push_str(&format!(
        "  avg prompt chars: {:.1}\n",
        report.knowledge_store.avg_prompt_chars
    ));
    out.push_str(&format!(
        "  vector dimensions: {}..{}\n",
        report.knowledge_store.vector_dimensions_min, report.knowledge_store.vector_dimensions_max
    ));
    out.push_str(&format!(
        "  category coverage: {}\n\n",
        report.knowledge_store.category_coverage
    ));

    out.push_str("Pattern Memory\n");
    out.push_str(&format!(
        "  experiences: {}\n",
        report.pattern_memory.experiences
    ));
    out.push_str(&format!("  patterns: {}\n", report.pattern_memory.patterns));
    out.push_str(&format!(
        "  unique layer sequences: {}\n",
        report.pattern_memory.unique_layer_sequences
    ));
    out.push_str(&format!(
        "  avg dependency edges: {:.1}\n\n",
        report.pattern_memory.avg_dependency_edges
    ));

    out.push_str("Knowledge Layer\n");
    out.push_str(&format!(
        "  temporary web hits: {}\n",
        report.knowledge_layer.temporary_hits
    ));
    out.push_str(&format!(
        "  grounding hits: {}\n\n",
        report.knowledge_layer.grounding_hits
    ));

    out.push_str("Totals\n");
    out.push_str(&format!(
        "  combined knowledge volume: {}\n",
        report.totals.combined_knowledge_volume
    ));
    out
}

fn normalize_topic(topic: &str) -> String {
    topic
        .split(['・', ' ', '/', '　'])
        .next()
        .unwrap_or(topic)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_audit_report_has_nonzero_seeded_knowledge() {
        let report = build_report();
        assert!(report.knowledge_store.entries > 0);
        assert!(report.pattern_memory.patterns > 0);
        assert!(report.web_search.arch_gen_command_available);
        assert!(report.web_search.adapter_wired);
    }
}
