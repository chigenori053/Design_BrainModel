use causal_domain::{CausalGraph, CausalRelationKind};

use crate::{ArchitectureGraph, ClassUnit, Dependency, DependencyKind, DesignUnit, StructureUnit};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Architecture {
    pub classes: Vec<ClassUnit>,
    pub dependencies: Vec<Dependency>,
    pub graph: ArchitectureGraph,
}

impl Architecture {
    pub fn seeded() -> Self {
        let mut architecture = Self::default();
        architecture
            .classes
            .push(ClassUnit::new(1, "ApplicationService"));
        architecture.classes[0]
            .structures
            .push(StructureUnit::new(1, "handle_request"));
        architecture
    }

    pub fn design_unit_count(&self) -> usize {
        self.classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .map(|structure| structure.design_units.len())
            .sum()
    }

    pub fn structure_count(&self) -> usize {
        self.classes
            .iter()
            .map(|class_unit| class_unit.structures.len())
            .sum()
    }

    pub fn ensure_seeded(&mut self) {
        if self.classes.is_empty() {
            *self = Self::seeded();
        } else if self.classes[0].structures.is_empty() {
            self.classes[0]
                .structures
                .push(StructureUnit::new(1, "handle_request"));
        }
    }

    pub fn add_design_unit(&mut self, unit: DesignUnit) {
        self.ensure_seeded();
        self.classes[0].structures[0].design_units.push(unit);
    }

    pub fn remove_design_unit(&mut self) -> Option<DesignUnit> {
        self.ensure_seeded();
        self.classes[0].structures[0].design_units.pop()
    }

    pub fn all_design_unit_ids(&self) -> Vec<u64> {
        self.classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .flat_map(|structure| structure.design_units.iter())
            .map(|unit| unit.id.0)
            .collect()
    }

    pub fn design_units_by_id(&self) -> BTreeMap<u64, &DesignUnit> {
        self.classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .flat_map(|structure| structure.design_units.iter())
            .map(|unit| (unit.id.0, unit))
            .collect()
    }

    pub fn causal_graph(&self) -> CausalGraph {
        let mut graph = CausalGraph::new();
        let units = self.design_units_by_id();

        for unit_id in units.keys() {
            graph.add_node(*unit_id);
        }

        for dependency in &self.dependencies {
            let kind = match dependency.kind {
                DependencyKind::Calls | DependencyKind::Reads | DependencyKind::Writes => {
                    CausalRelationKind::Requires
                }
                DependencyKind::Emits => CausalRelationKind::Emits,
            };
            graph.add_edge(dependency.from.0, dependency.to.0, kind);
        }

        for unit in units.values() {
            for relation in &unit.causal_relations {
                graph.add_edge(unit.id.0, relation.target, relation.kind);
            }
        }

        graph
    }
}
