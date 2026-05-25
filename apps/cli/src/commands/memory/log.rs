use crate::command::{CommandError, Output};
use crate::holographic_memory_observation::{
    DuplicateClass, HolographicMemoryLogStore, HolographicMemoryObservationLog, MemoryLogFilter,
    parse_duplicate_class,
};

pub fn handle_log(args: &[String]) -> Result<Output, CommandError> {
    let (filter, json, verbose, store_path) = parse_log_args(args)?;
    let store = HolographicMemoryLogStore::new(
        store_path.unwrap_or_else(|| ".dbm/logs/holographic_memory_observation.jsonl".into()),
    );
    let logs = store
        .query(filter)
        .map_err(|err| CommandError::ExecutionError(format!("memory log read error: {err}")))?;
    if json {
        let rendered = serde_json::to_string_pretty(&logs)
            .map_err(|err| CommandError::ExecutionError(err.to_string()))?;
        return Ok(Output::text(rendered));
    }
    Ok(Output::text(render_logs(&logs, verbose)))
}

fn parse_log_args(
    args: &[String],
) -> Result<(MemoryLogFilter, bool, bool, Option<std::path::PathBuf>), CommandError> {
    let mut filter = MemoryLogFilter::default();
    let mut json = false;
    let mut verbose = false;
    let mut store_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--recent" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CommandError::ExecutionError(
                        "--recent requires a value".to_string(),
                    ));
                };
                filter.recent = value.parse::<usize>().ok();
                index += 2;
            }
            "--duplicates" => {
                filter.duplicate_only = true;
                index += 1;
            }
            "--conflicts" => {
                filter.conflict_only = true;
                index += 1;
            }
            "--memory-id" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CommandError::ExecutionError(
                        "--memory-id requires a value".to_string(),
                    ));
                };
                filter.memory_id = Some(value.clone());
                index += 2;
            }
            "--class" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CommandError::ExecutionError(
                        "--class requires a value".to_string(),
                    ));
                };
                filter.duplicate_class = parse_duplicate_class(value);
                index += 2;
            }
            "--since" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CommandError::ExecutionError(
                        "--since requires a value".to_string(),
                    ));
                };
                filter.since = value.parse::<u64>().ok();
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--verbose" => {
                verbose = true;
                index += 1;
            }
            "--store" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CommandError::ExecutionError(
                        "--store requires a value".to_string(),
                    ));
                };
                store_path = Some(value.into());
                index += 2;
            }
            other => {
                if other.starts_with("mem_") || other.starts_with("mem:") {
                    filter.memory_id = Some(other.to_string());
                    index += 1;
                } else {
                    return Err(CommandError::UnknownSubcommand {
                        command: "memory log".to_string(),
                        subcommand: other.to_string(),
                    });
                }
            }
        }
    }
    Ok((filter, json, verbose, store_path))
}

fn render_logs(logs: &[HolographicMemoryObservationLog], verbose: bool) -> String {
    if logs.is_empty() {
        return "time | event | memory_id | class | score | candidates\n(no memory observation logs)".to_string();
    }
    if verbose {
        return logs
            .iter()
            .map(render_verbose)
            .collect::<Vec<_>>()
            .join("\n\n");
    }
    let mut lines = vec!["time | event | memory_id | class | score | candidates".to_string()];
    lines.extend(logs.iter().map(|log| {
        format!(
            "{} | {:?} | {} | {:?} | {:.2} | {}",
            log.created_at,
            log.event_type,
            log.memory_id,
            log.duplicate_class,
            log.resonance_score,
            log.duplicate_candidate_ids.join(",")
        )
    }));
    lines.join("\n")
}

fn render_verbose(log: &HolographicMemoryObservationLog) -> String {
    format!(
        "event_id: {}\nevent_type: {:?}\nmemory_id: {}\nsource_input_hash: {}\ncanonical_key: {}\nembedding_dim: {}\nholographic_dim: {}\nresonance_score: {:.3}\nambiguity_score: {:.3}\nrecall_count: {}\ncreated_at: {}\nlast_used_at: {:?}\nduplicate_candidate_ids: {:?}\nduplicate_class: {:?}\nselected_as_canonical: {}\nrejected_reason: {:?}",
        log.event_id,
        log.event_type,
        log.memory_id,
        log.source_input_hash,
        log.canonical_key,
        log.embedding_dim,
        log.holographic_dim,
        log.resonance_score,
        log.ambiguity_score,
        log.recall_count,
        log.created_at,
        log.last_used_at,
        log.duplicate_candidate_ids,
        log.duplicate_class,
        log.selected_as_canonical,
        log.rejected_reason
    )
}

#[allow(dead_code)]
fn _keep_duplicate_class_used(_: DuplicateClass) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holographic_memory_observation::{HolographicMemoryObservationEvent, now_secs};

    fn sample(memory_id: &str, class: DuplicateClass) -> HolographicMemoryObservationLog {
        HolographicMemoryObservationLog {
            event_id: format!("event-{memory_id}"),
            event_type: HolographicMemoryObservationEvent::DuplicateCandidateDetected,
            memory_id: memory_id.to_string(),
            source_input_hash: "hash".to_string(),
            canonical_key: "key".to_string(),
            embedding_dim: 8,
            holographic_dim: 8,
            resonance_score: 0.93,
            ambiguity_score: 0.2,
            recall_count: 1,
            created_at: now_secs(),
            last_used_at: None,
            duplicate_candidate_ids: vec!["mem_001".to_string()],
            duplicate_class: class,
            selected_as_canonical: false,
            rejected_reason: None,
        }
    }

    #[test]
    fn dbm_memory_log_reads_logs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.jsonl");
        let store = HolographicMemoryLogStore::new(path.clone());
        store
            .append(sample("mem_008", DuplicateClass::SemanticDuplicate))
            .expect("append");

        let out = handle_log(&["--store".to_string(), path.display().to_string()]).expect("log");

        assert!(out.message.contains("mem_008"));
        assert!(out.message.contains("SemanticDuplicate"));
    }

    #[test]
    fn memory_log_json_outputs_array() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.jsonl");
        HolographicMemoryLogStore::new(path.clone())
            .append(sample("mem_008", DuplicateClass::SemanticDuplicate))
            .expect("append");

        let out = handle_log(&[
            "--store".to_string(),
            path.display().to_string(),
            "--json".to_string(),
        ])
        .expect("log");
        let parsed: serde_json::Value = serde_json::from_str(&out.message).expect("json");

        assert!(parsed.is_array());
    }
}
