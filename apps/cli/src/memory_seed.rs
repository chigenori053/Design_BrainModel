/// Phase OP-1 — Structured seed knowledge loader.
///
/// Seeds are read from a JSON file (default: `seeds/knowledge.json`) and
/// stored into an `InMemoryEngine` instance at startup so that the recall
/// subsystem has baseline pattern knowledge from run 1.
///
/// Synonym expansion ensures that user intent terms like "cli", "service",
/// "api" resolve to their canonical library / pattern vocabulary.
use std::collections::HashMap;
use std::path::Path;

use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use serde::Deserialize;

// ── Seed format ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SeedEntry {
    pub id: String,
    pub intent: String,
    pub pattern: String,
    pub solution: String,
    pub tags: Vec<String>,
}

// ── Synonym table ─────────────────────────────────────────────────────────────

/// Returns the synonym expansion map.
/// Keys are canonical user-facing terms; values are additional tags injected
/// so that the recall scorer can match on either form.
fn synonym_map() -> HashMap<&'static str, Vec<&'static str>> {
    let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    m.insert("cli", vec!["command", "terminal", "tool", "repl", "bin"]);
    m.insert(
        "service",
        vec!["backend", "microservice", "daemon", "worker", "server"],
    );
    m.insert("api", vec!["rest", "http", "endpoint", "route", "grpc"]);
    m.insert(
        "web",
        vec!["http", "server", "browser", "html", "frontend", "backend"],
    );
    m.insert(
        "db",
        vec!["database", "storage", "sql", "nosql", "persistence"],
    );
    m.insert("rust", vec!["cargo", "crate", "tokio", "async"]);
    m
}

fn expand_tags(tags: &[String]) -> Vec<String> {
    let synonyms = synonym_map();
    let mut expanded: Vec<String> = tags.to_vec();
    for tag in tags {
        if let Some(extras) = synonyms.get(tag.as_str()) {
            for extra in extras {
                let s = extra.to_string();
                if !expanded.contains(&s) {
                    expanded.push(s);
                }
            }
        }
    }
    expanded
}

// ── Conversion ────────────────────────────────────────────────────────────────

fn seed_to_record(entry: SeedEntry) -> MemoryRecord {
    // Combine intent + pattern + solution into a rich text blob so the
    // term-overlap scorer finds query words in multiple fields.
    let text = format!("{} {} {}", entry.intent, entry.pattern, entry.solution);
    let mut tags = expand_tags(&entry.tags);
    // Mark as knowledge-seed so the intent refiner's apply_memory skips it.
    // Seeds are reference patterns, not user-session memory — they must not
    // force slot values (spec §3.2 "Memory must not force a solution").
    tags.push("knowledge_seed".to_string());
    MemoryRecord {
        id: entry.id,
        text,
        tags,
        embedding: None,
        architecture: None,
        relations: Vec::new(),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load seeds from `path` and store them into `engine`.
/// Silently skips entries that fail to parse.
/// Returns the number of records successfully loaded.
pub fn load_seeds_into(engine: &InMemoryEngine, path: &Path) -> usize {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let entries: Vec<SeedEntry> = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warn: seed parse error in {}: {e}", path.display());
            return 0;
        }
    };
    let count = entries.len();
    for entry in entries {
        engine.store(seed_to_record(entry));
    }
    count
}

/// Load seeds from the canonical project-relative path `seeds/knowledge.json`.
/// Searches upward from the current working directory (up to 3 levels) so the
/// binary works correctly whether invoked from the workspace root or a subdir.
pub fn load_default_seeds(engine: &InMemoryEngine) -> usize {
    let candidates = [
        Path::new("seeds/knowledge.json"),
        Path::new("../seeds/knowledge.json"),
        Path::new("../../seeds/knowledge.json"),
    ];
    for path in &candidates {
        if path.exists() {
            return load_seeds_into(engine, path);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryQuery};

    fn engine_with_defaults() -> InMemoryEngine {
        let engine = InMemoryEngine::default();
        let loaded = load_default_seeds(&engine);
        assert!(loaded >= 50, "expected ≥50 seed records, got {loaded}");
        engine
    }

    #[test]
    fn recall_rust_web_contains_axum() {
        let engine = engine_with_defaults();
        let results = engine.retrieve(MemoryQuery {
            text: "build rust web".to_string(),
            tags: vec!["web".to_string(), "rust".to_string()],
            limit: 5,
        });
        let texts: Vec<&str> = results.iter().map(|r| r.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("axum")),
            "expected 'axum' in recall results for 'build rust web', got: {texts:?}"
        );
    }

    #[test]
    fn recall_cli_matches_command_synonym() {
        let engine = engine_with_defaults();
        let results = engine.retrieve(MemoryQuery {
            text: "build cli tool".to_string(),
            tags: vec!["cli".to_string()],
            limit: 5,
        });
        assert!(
            !results.is_empty(),
            "expected ≥1 recall result for 'cli tool'"
        );
        let has_clap = results.iter().any(|r| r.text.contains("clap"));
        assert!(has_clap, "expected clap in cli recall results");
    }

    #[test]
    fn recall_service_matches_backend_synonym() {
        let engine = engine_with_defaults();
        let results = engine.retrieve(MemoryQuery {
            text: "async service backend".to_string(),
            tags: vec!["service".to_string(), "backend".to_string()],
            limit: 5,
        });
        assert!(
            !results.is_empty(),
            "expected ≥1 recall result for 'service backend'"
        );
    }

    #[test]
    fn recall_postgres_db() {
        let engine = engine_with_defaults();
        let results = engine.retrieve(MemoryQuery {
            text: "connect postgres database".to_string(),
            tags: vec!["db".to_string(), "postgres".to_string()],
            limit: 5,
        });
        assert!(
            results.iter().any(|r| r.text.contains("sqlx")),
            "expected sqlx in postgres recall results"
        );
    }

    #[test]
    fn expand_tags_adds_synonyms() {
        let tags = vec!["cli".to_string(), "rust".to_string()];
        let expanded = expand_tags(&tags);
        assert!(expanded.contains(&"command".to_string()));
        assert!(expanded.contains(&"terminal".to_string()));
        assert!(expanded.contains(&"cargo".to_string()));
    }
}
