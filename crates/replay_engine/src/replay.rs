/// Replay: reconstruct initial state from a frozen trace and re-run the pipeline.
///
/// Spec §5.2 (replay command), §11 (replay conditions).
/// WebSearch is satisfied from the snapshot — no live external calls (spec §12).
use design_domain::{
    Architecture, ArchitectureGraph, ClassUnit, Dependency, DependencyKind, DesignUnit,
    DesignUnitId, Layer, StructureUnit, StructureUnitId,
};
use design_search_engine::{BeamSearchController, SearchConfig};
use knowledge_engine::{KnowledgeDocument, KnowledgeMetadata, KnowledgeSource};
use world_model_core::WorldState;

use crate::capture::capture;
use crate::trace::{FullTrace, InputSnapshot, SerializedSearchConfig};

/// Re-runs the pipeline using the same inputs frozen in `trace`.
/// Returns a new FullTrace that can be compared via `diff`.
pub fn replay(trace: &FullTrace, controller: &BeamSearchController) -> FullTrace {
    let initial_state = reconstruct_world_state(&trace.input);
    let config = reconstruct_config(&trace.input.search_config);
    let knowledge_docs = reconstruct_knowledge_docs(trace);
    capture(initial_state, config, &knowledge_docs, controller)
}

// ── Reconstruction helpers ────────────────────────────────────────────────────

fn reconstruct_world_state(snapshot: &InputSnapshot) -> WorldState {
    let classes: Vec<ClassUnit> = snapshot
        .architecture
        .classes
        .iter()
        .map(|c| {
            let structures: Vec<StructureUnit> = c
                .structures
                .iter()
                .map(|s| {
                    let design_units: Vec<DesignUnit> = s
                        .units
                        .iter()
                        .map(|u| {
                            let mut unit =
                                DesignUnit::with_layer(u.id, u.name.clone(), parse_layer(&u.layer));
                            unit.semantics = u.semantics.clone();
                            unit
                        })
                        .collect();
                    StructureUnit {
                        id: StructureUnitId(s.id),
                        name: s.name.clone(),
                        design_units,
                    }
                })
                .collect();
            ClassUnit {
                id: c.id,
                name: c.name.clone(),
                structures,
            }
        })
        .collect();

    let dependencies: Vec<Dependency> = snapshot
        .architecture
        .deps
        .iter()
        .map(|d| Dependency {
            from: DesignUnitId(d.from),
            to: DesignUnitId(d.to),
            kind: parse_dep_kind(&d.kind),
        })
        .collect();

    let edges: Vec<(u64, u64)> = dependencies.iter().map(|d| (d.from.0, d.to.0)).collect();

    let architecture = Architecture {
        classes,
        dependencies,
        graph: ArchitectureGraph { edges },
    };

    WorldState::from_architecture(snapshot.state_id, architecture, Vec::new())
}

fn reconstruct_config(cfg: &SerializedSearchConfig) -> SearchConfig {
    SearchConfig {
        max_depth: cfg.max_depth,
        max_candidates: cfg.max_candidates,
        beam_width: cfg.beam_width,
        experience_bias: cfg.experience_bias,
        policy_bias: cfg.policy_bias,
    }
}

/// Reconstructs knowledge from the frozen snapshot so replay makes no live calls.
fn reconstruct_knowledge_docs(trace: &FullTrace) -> Vec<KnowledgeDocument> {
    trace
        .knowledge
        .documents
        .iter()
        .map(|e| KnowledgeDocument {
            source: parse_knowledge_source(&e.source),
            content: e.content.clone(),
            metadata: KnowledgeMetadata {
                title: e.title.clone(),
                source_uri: e.source_uri.clone(),
                reliability_hint: e.reliability_hint,
            },
        })
        .collect()
}

// ── Enum parsing ──────────────────────────────────────────────────────────────

fn parse_layer(s: &str) -> Layer {
    match s {
        "UI" => Layer::Ui,
        "Service" => Layer::Service,
        "Repository" => Layer::Repository,
        "Database" => Layer::Database,
        _ => Layer::Service,
    }
}

fn parse_dep_kind(s: &str) -> DependencyKind {
    match s {
        "Reads" => DependencyKind::Reads,
        "Writes" => DependencyKind::Writes,
        "Emits" => DependencyKind::Emits,
        _ => DependencyKind::Calls,
    }
}

fn parse_knowledge_source(s: &str) -> KnowledgeSource {
    match s {
        "WebSearch" => KnowledgeSource::WebSearch,
        "ExperienceDerived" => KnowledgeSource::ExperienceDerived,
        "Inferred" => KnowledgeSource::Inferred,
        _ => KnowledgeSource::LocalDocument,
    }
}
