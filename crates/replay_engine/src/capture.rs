/// Trace capture: run the full pipeline and freeze every layer to a FullTrace.
///
/// Spec §5.1 (trace command), §6 (trace format).
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use code_language_core::CodeLanguageCore;
use design_search_engine::{BeamSearchController, SearchConfig, SearchState, SearchTrace};
use knowledge_engine::KnowledgeDocument;
use memory_space_phase14::{MemorySpace, architecture_hash};
use world_model_core::WorldState;

use crate::trace::{
    FullTrace, IrSnapshot, KnowledgeEntry, KnowledgeSnapshot, MemoryLayerEntry, PatchEntry,
    SearchLayerEntry, SerializedArchitecture, SerializedClass, SerializedDependency,
    SerializedSearchConfig, SerializedStructure, SerializedUnit, TraceMetadata,
    InputSnapshot,
};

pub fn capture(
    initial_state: WorldState,
    config: SearchConfig,
    knowledge_docs: &[KnowledgeDocument],
    controller: &BeamSearchController,
) -> FullTrace {
    let input = snapshot_input(&initial_state, &config);
    let knowledge = snapshot_knowledge(knowledge_docs);
    let memory = snapshot_memory(controller, &initial_state);
    let search_trace = controller.search_trace(initial_state.clone(), None, &config);
    let search = snapshot_search_layers(&search_trace);

    let clc = CodeLanguageCore::default();
    let (ir, code) = match search_trace.final_beam.first() {
        Some(top) => (snapshot_ir(top, &clc), generate_code(top, &clc)),
        None => (IrSnapshot::empty(), String::new()),
    };

    FullTrace {
        input,
        knowledge,
        ir,
        memory,
        search,
        code,
        patch: Vec::<PatchEntry>::new(),
        metadata: TraceMetadata {
            timestamp_utc: utc_timestamp(),
            version: "1.0".into(),
            run_id: generate_run_id(&search_trace),
            explored_state_count: search_trace.explored_state_count,
            depth_best_scores: search_trace.depth_best_scores.clone(),
        },
    }
}

// ── Per-layer snapshot helpers ────────────────────────────────────────────────

fn snapshot_input(state: &WorldState, config: &SearchConfig) -> InputSnapshot {
    let arch_hash = architecture_hash(state);

    let classes = state
        .architecture
        .classes
        .iter()
        .map(|c| SerializedClass {
            id: c.id,
            name: c.name.clone(),
            structures: c
                .structures
                .iter()
                .map(|s| SerializedStructure {
                    id: s.id.0,
                    name: s.name.clone(),
                    units: s
                        .design_units
                        .iter()
                        .map(|u| SerializedUnit {
                            id: u.id.0,
                            name: u.name.clone(),
                            layer: u.layer.as_str().to_string(),
                            semantics: u.semantics.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();

    let deps = state
        .architecture
        .dependencies
        .iter()
        .map(|d| SerializedDependency {
            from: d.from.0,
            to: d.to.0,
            kind: format!("{:?}", d.kind),
        })
        .collect();

    InputSnapshot {
        state_id: state.state_id,
        initial_state_hash: format!("{:016x}", arch_hash),
        architecture: SerializedArchitecture { classes, deps },
        search_config: SerializedSearchConfig {
            max_depth: config.max_depth,
            max_candidates: config.max_candidates,
            beam_width: config.beam_width,
            experience_bias: config.experience_bias,
            policy_bias: config.policy_bias,
        },
        score: state.score,
        depth: state.depth,
    }
}

fn snapshot_knowledge(docs: &[KnowledgeDocument]) -> KnowledgeSnapshot {
    let entries: Vec<KnowledgeEntry> = docs
        .iter()
        .map(|d| KnowledgeEntry {
            source: format!("{:?}", d.source),
            content: d.content.clone(),
            title: d.metadata.title.clone(),
            source_uri: d.metadata.source_uri.clone(),
            reliability_hint: d.metadata.reliability_hint,
        })
        .collect();

    let content_hash = hash_str(
        &entries
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join("|"),
    );
    let web_search_used = docs.iter().any(|d| {
        matches!(d.source, knowledge_engine::KnowledgeSource::WebSearch)
    });

    KnowledgeSnapshot {
        source_count: entries.len(),
        web_search_used,
        content_hash,
        documents: entries,
    }
}

fn snapshot_memory(
    controller: &BeamSearchController,
    state: &WorldState,
) -> Vec<MemoryLayerEntry> {
    controller
        .memory
        .lock()
        .expect("memory space lock poisoned")
        .recall_patterns(state)
        .iter()
        .map(|p| MemoryLayerEntry {
            pattern_id: format!("{}", p.pattern_id.0),
            average_score: p.average_score,
            frequency: p.frequency,
            layer_sequence: p.layer_sequence.iter().map(|l| l.as_str().to_string()).collect(),
            dependency_edge_count: p.dependency_edges.len(),
        })
        .collect()
}

fn snapshot_search_layers(trace: &SearchTrace) -> Vec<SearchLayerEntry> {
    trace
        .final_beam
        .iter()
        .enumerate()
        .map(|(i, state)| SearchLayerEntry {
            step_index: i,
            branch_id: state.state_id,
            state_hash: hash_search_state(state),
            score: state.score,
            depth: state.depth,
            pareto_rank: state.pareto_rank,
            source_action: state.source_action.as_ref().map(|a| format!("{:?}", a)),
        })
        .collect()
}

fn snapshot_ir(state: &SearchState, clc: &CodeLanguageCore) -> IrSnapshot {
    let ir = clc.architecture_to_code_ir(&state.world_state.architecture);
    let module_names: Vec<String> = ir.modules.iter().map(|m| m.name.clone()).collect();
    let ir_hash = hash_str(&module_names.join(","));
    IrSnapshot {
        module_count: ir.modules.len(),
        dependency_count: ir.dependencies.len(),
        ir_hash,
        module_names,
    }
}

fn generate_code(state: &SearchState, clc: &CodeLanguageCore) -> String {
    clc.roundtrip_from_architecture(&state.world_state.architecture)
        .into_iter()
        .map(|(name, src)| format!("// --- {} ---\n{}", name, src))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Hashing utilities ─────────────────────────────────────────────────────────

pub(crate) fn hash_str(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn hash_search_state(state: &SearchState) -> String {
    hash_str(&format!(
        "{}|{:.9}|{}|{}",
        state.state_id, state.score, state.depth, state.pareto_rank
    ))
}

fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}Z", secs)
}

fn generate_run_id(trace: &SearchTrace) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let beam_hash = hash_str(&format!(
        "{}|{}",
        trace.explored_state_count,
        trace
            .depth_best_scores
            .iter()
            .map(|s| format!("{:.6}", s))
            .collect::<Vec<_>>()
            .join(",")
    ));
    format!("run-{:08x}-{}", nanos, &beam_hash[..8])
}
