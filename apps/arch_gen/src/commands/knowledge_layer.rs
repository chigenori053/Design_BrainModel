use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::commands::web_search::WebSearchHit;

const MAX_TEMPORARY_HITS: usize = 24;
const MAX_PROMPT_HITS: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredKnowledgeHit {
    pub id: u64,
    pub query: String,
    pub title: String,
    pub snippet: String,
    pub url: String,
    pub saved_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KnowledgeLayerFile {
    hits: Vec<StoredKnowledgeHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeLayerMetrics {
    pub temporary_hits: usize,
    pub grounding_hits: usize,
}

#[derive(Debug, Clone)]
pub struct InferenceKnowledgeContext {
    pub enriched_requirement: String,
    pub temporary_hits: Vec<StoredKnowledgeHit>,
    pub grounding_hits: Vec<StoredKnowledgeHit>,
}

pub fn save_temporary_web_hits(
    query: &str,
    hits: &[WebSearchHit],
) -> Result<Vec<StoredKnowledgeHit>, String> {
    let path = temporary_knowledge_path();
    let mut file = read_knowledge_file(&path)?;
    let now = now_epoch_seconds();
    let mut saved = Vec::new();

    for hit in hits {
        let candidate = StoredKnowledgeHit {
            id: stable_hit_id(query, hit, now),
            query: query.trim().to_string(),
            title: hit.title.trim().to_string(),
            snippet: hit.snippet.trim().to_string(),
            url: hit.url.trim().to_string(),
            saved_at: now,
        };

        if candidate.title.is_empty() && candidate.snippet.is_empty() && candidate.url.is_empty() {
            continue;
        }
        if file
            .hits
            .iter()
            .any(|existing| same_hit(existing, &candidate))
        {
            continue;
        }

        saved.push(candidate.clone());
        file.hits.insert(0, candidate);
    }

    file.hits.truncate(MAX_TEMPORARY_HITS);
    write_knowledge_file(&path, &file)?;
    Ok(saved)
}

pub fn load_temporary_hits() -> Result<Vec<StoredKnowledgeHit>, String> {
    read_knowledge_file(&temporary_knowledge_path()).map(|file| file.hits)
}

pub fn load_grounding_hits() -> Result<Vec<StoredKnowledgeHit>, String> {
    read_knowledge_file(&grounding_knowledge_path()?).map(|file| file.hits)
}

pub fn promote_hits_to_grounding(hit_ids: &[u64]) -> Result<Vec<StoredKnowledgeHit>, String> {
    let wanted = hit_ids.iter().copied().collect::<BTreeSet<_>>();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let temp_path = temporary_knowledge_path();
    let grounding_path = grounding_knowledge_path()?;
    let mut temp_file = read_knowledge_file(&temp_path)?;
    let mut grounding_file = read_knowledge_file(&grounding_path)?;
    let mut promoted = Vec::new();

    for hit in &temp_file.hits {
        if wanted.contains(&hit.id)
            && !grounding_file
                .hits
                .iter()
                .any(|existing| same_hit(existing, hit))
        {
            grounding_file.hits.insert(0, hit.clone());
            promoted.push(hit.clone());
        }
    }

    temp_file.hits.retain(|hit| !wanted.contains(&hit.id));
    write_knowledge_file(&temp_path, &temp_file)?;
    write_knowledge_file(&grounding_path, &grounding_file)?;
    Ok(promoted)
}

pub fn prepare_inference_input(requirement: &str) -> Result<InferenceKnowledgeContext, String> {
    let temporary_hits = load_temporary_hits()?;
    let grounding_hits = load_grounding_hits()?;
    let mut sections = Vec::new();

    if !grounding_hits.is_empty() {
        sections.push(format!(
            "[Confirmed grounding knowledge]\n{}",
            format_hits_for_prompt(&grounding_hits)
        ));
    }
    if !temporary_hits.is_empty() {
        sections.push(format!(
            "[Temporary web knowledge for design evaluation]\n{}",
            format_hits_for_prompt(&temporary_hits)
        ));
    }

    let enriched_requirement = if sections.is_empty() {
        requirement.trim().to_string()
    } else {
        format!("{}\n\n{}", requirement.trim(), sections.join("\n\n"))
    };

    Ok(InferenceKnowledgeContext {
        enriched_requirement,
        temporary_hits,
        grounding_hits,
    })
}

pub fn knowledge_layer_metrics() -> Result<KnowledgeLayerMetrics, String> {
    Ok(KnowledgeLayerMetrics {
        temporary_hits: load_temporary_hits()?.len(),
        grounding_hits: load_grounding_hits()?.len(),
    })
}

pub fn temporary_knowledge_path() -> PathBuf {
    std::env::temp_dir()
        .join("arch_gen")
        .join("design_inference_knowledge.json")
}

pub fn grounding_knowledge_path() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("failed to resolve cwd: {e}"))?;
    Ok(cwd.join(".arch_gen").join("grounding_knowledge.json"))
}

fn format_hits_for_prompt(hits: &[StoredKnowledgeHit]) -> String {
    hits.iter()
        .take(MAX_PROMPT_HITS)
        .map(|hit| {
            format!(
                "- {}: {} [source: {}]",
                sanitize_prompt_text(&hit.title),
                sanitize_prompt_text(&hit.snippet),
                sanitize_prompt_text(&hit.url)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_prompt_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn read_knowledge_file(path: &Path) -> Result<KnowledgeLayerFile, String> {
    if !path.exists() {
        return Ok(KnowledgeLayerFile::default());
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("failed to parse '{}': {e}", path.display()))
}

fn write_knowledge_file(path: &Path, file: &KnowledgeLayerFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(file)
        .map_err(|e| format!("json serialization failed: {e}"))?;
    fs::write(path, raw).map_err(|e| format!("failed to write '{}': {e}", path.display()))
}

fn stable_hit_id(query: &str, hit: &WebSearchHit, now: u64) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    query.trim().hash(&mut hasher);
    hit.title.trim().hash(&mut hasher);
    hit.snippet.trim().hash(&mut hasher);
    hit.url.trim().hash(&mut hasher);
    now.hash(&mut hasher);
    hasher.finish()
}

fn same_hit(left: &StoredKnowledgeHit, right: &StoredKnowledgeHit) -> bool {
    left.title == right.title && left.snippet == right.snippet && left.url == right.url
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_inference_input_appends_grounding_and_temporary_hits() {
        let dir = tempfile::tempdir().unwrap();
        let temp_path = dir.path().join("temp.json");
        let grounding_path = dir.path().join("grounding.json");

        write_knowledge_file(
            &temp_path,
            &KnowledgeLayerFile {
                hits: vec![StoredKnowledgeHit {
                    id: 1,
                    query: "q".to_string(),
                    title: "Terminal UI".to_string(),
                    snippet: "Use event loop".to_string(),
                    url: "https://example.com/temp".to_string(),
                    saved_at: 1,
                }],
            },
        )
        .unwrap();
        write_knowledge_file(
            &grounding_path,
            &KnowledgeLayerFile {
                hits: vec![StoredKnowledgeHit {
                    id: 2,
                    query: "q".to_string(),
                    title: "Plugin model".to_string(),
                    snippet: "Expose stable extension points".to_string(),
                    url: "https://example.com/ground".to_string(),
                    saved_at: 2,
                }],
            },
        )
        .unwrap();

        let temp_hits = read_knowledge_file(&temp_path).unwrap().hits;
        let grounding_hits = read_knowledge_file(&grounding_path).unwrap().hits;
        let context = InferenceKnowledgeContext {
            enriched_requirement: format!(
                "editor\n\n[Confirmed grounding knowledge]\n{}\n\n[Temporary web knowledge for design evaluation]\n{}",
                format_hits_for_prompt(&grounding_hits),
                format_hits_for_prompt(&temp_hits)
            ),
            temporary_hits: temp_hits,
            grounding_hits,
        };

        assert!(
            context
                .enriched_requirement
                .contains("Confirmed grounding knowledge")
        );
        assert!(
            context
                .enriched_requirement
                .contains("Temporary web knowledge")
        );
    }

    #[test]
    fn save_temporary_hits_deduplicates_equivalent_entries() {
        let mut file = KnowledgeLayerFile::default();
        let hit = StoredKnowledgeHit {
            id: 1,
            query: "q".to_string(),
            title: "NeoVim".to_string(),
            snippet: "Editor".to_string(),
            url: "https://example.com".to_string(),
            saved_at: 1,
        };
        file.hits.push(hit.clone());

        assert!(same_hit(&file.hits[0], &hit));
    }
}
