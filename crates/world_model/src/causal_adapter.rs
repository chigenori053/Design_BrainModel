use std::collections::BTreeMap;

use design_domain::{CausalRelationKind, DependencyKind};
use world_model_core::{Action, WorldState as StructureWorldState};

use crate::semantic_causal_runtime::{
    CausalPropagationEdge, CausalRuntimeState, CausalState, EntityState,
};

pub struct WorldStateToCausalAdapter;

impl WorldStateToCausalAdapter {
    pub fn is_available() -> bool {
        true
    }

    pub fn convert(state: &StructureWorldState) -> CausalRuntimeState {
        let mut entities = Vec::new();

        for class in &state.architecture.classes {
            for structure in &class.structures {
                for unit in &structure.design_units {
                    let entity_id = entity_id(unit.id.0);
                    let mut metadata = BTreeMap::new();
                    metadata.insert("name".to_string(), unit.name.clone());
                    metadata.insert("class_id".to_string(), class.id.to_string());
                    metadata.insert("class_name".to_string(), class.name.clone());
                    metadata.insert("structure_id".to_string(), structure.id.0.to_string());
                    metadata.insert("structure_name".to_string(), structure.name.clone());
                    metadata.insert("inputs".to_string(), stable_strings(&unit.inputs));
                    metadata.insert("outputs".to_string(), stable_strings(&unit.outputs));
                    metadata.insert(
                        "dependencies".to_string(),
                        unit.dependencies
                            .iter()
                            .map(|dependency| dependency.0.to_string())
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    metadata.insert("semantics".to_string(), stable_strings(&unit.semantics));

                    for (index, feature) in state.features.iter().enumerate() {
                        metadata.insert(format!("feature_{index}"), stable_float(*feature));
                    }

                    entities.push(EntityState {
                        current_state: entity_id.clone(),
                        entity_id,
                        semantic_role: unit.layer.as_str().to_string(),
                        metadata,
                        canonical_ref: None,
                    });
                }
            }
        }

        let mut edges = state
            .architecture
            .dependencies
            .iter()
            .map(|dependency| CausalPropagationEdge {
                source_state: entity_id(dependency.from.0),
                target_state: entity_id(dependency.to.0),
                causal_weight: dependency_weight(dependency.kind),
            })
            .collect::<Vec<_>>();

        for unit in state.architecture.design_units_by_id().values() {
            edges.extend(
                unit.causal_relations
                    .iter()
                    .map(|relation| CausalPropagationEdge {
                        source_state: entity_id(unit.id.0),
                        target_state: entity_id(relation.target),
                        causal_weight: relation_weight(relation.kind),
                    }),
            );
        }

        CausalRuntimeState::new(
            entities,
            state.constraints.clone(),
            CausalState {
                edges,
                seed_history: state.history.iter().map(stable_action).collect(),
            },
        )
    }
}

fn entity_id(id: u64) -> String {
    format!("design_unit:{id}")
}

fn stable_strings(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{}:{value}", value.len()))
        .collect::<Vec<_>>()
        .join("|")
}

fn stable_float(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn dependency_weight(kind: DependencyKind) -> f64 {
    match kind {
        DependencyKind::Calls => 1.0,
        DependencyKind::Reads => 0.9,
        DependencyKind::Writes => 0.95,
        DependencyKind::Emits => 0.85,
    }
}

fn relation_weight(kind: CausalRelationKind) -> f64 {
    match kind {
        CausalRelationKind::Enables => 1.0,
        CausalRelationKind::Inhibits => -1.0,
        CausalRelationKind::Requires => 0.95,
        CausalRelationKind::Emits => 0.85,
    }
}

fn stable_action(action: &Action) -> String {
    match action {
        Action::AddDesignUnit { name, layer } => {
            format!("add_design_unit:{}:{}:{name}", layer.as_str(), name.len())
        }
        Action::RemoveDesignUnit => "remove_design_unit".to_string(),
        Action::ConnectDependency { from, to } => format!("connect_dependency:{from}:{to}"),
        Action::SplitStructure => "split_structure".to_string(),
        Action::MergeStructure => "merge_structure".to_string(),
    }
}
