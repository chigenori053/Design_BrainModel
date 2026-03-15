use architecture_domain::{ArchitectureState, Component, ComponentRole};
use design_domain::{
    Architecture, ClassUnit, DesignUnit, DesignUnitId, Layer, StructureUnit,
};

/// `ArchitectureState`（Phase9パイプライン出力）を
/// `design_domain::Architecture`（CodeIRパイプライン入力）に変換する。
pub fn arch_state_to_architecture(state: &ArchitectureState) -> Architecture {
    let design_units: Vec<DesignUnit> = state
        .components
        .iter()
        .map(|comp| component_to_design_unit(comp))
        .collect();

    let mut structure = StructureUnit::new(1, "generated");
    structure.design_units = design_units;

    let mut class = ClassUnit::new(1, "GeneratedArchitecture");
    class.structures.push(structure);

    Architecture {
        classes: vec![class],
        dependencies: state.dependencies.clone(),
        graph: Default::default(),
    }
}

fn component_to_design_unit(comp: &Component) -> DesignUnit {
    let name = component_name(comp);
    let layer = component_layer(&comp.role);
    let mut unit = DesignUnit::with_layer(comp.id.0, &name, layer);
    unit.inputs = comp.inputs.iter().map(|i| i.name.clone()).collect();
    unit.outputs = comp.outputs.iter().map(|o| o.name.clone()).collect();
    unit
}

fn component_name(comp: &Component) -> String {
    match &comp.role {
        ComponentRole::Controller => format!("controller_{}", comp.id.0),
        ComponentRole::Service => format!("service_{}", comp.id.0),
        ComponentRole::Repository => format!("repository_{}", comp.id.0),
        ComponentRole::Database => format!("database_{}", comp.id.0),
        ComponentRole::Gateway => format!("gateway_{}", comp.id.0),
        ComponentRole::Unknown(s) if !s.is_empty() => s.clone(),
        ComponentRole::Unknown(_) => format!("component_{}", comp.id.0),
    }
}

fn component_layer(role: &ComponentRole) -> Layer {
    match role {
        ComponentRole::Controller | ComponentRole::Gateway => Layer::Ui,
        ComponentRole::Service | ComponentRole::Unknown(_) => Layer::Service,
        ComponentRole::Repository => Layer::Repository,
        ComponentRole::Database => Layer::Database,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use architecture_domain::{ComponentId, ComponentRole, Interface};
    use design_domain::DependencyKind;

    fn make_component(id: u64, role: ComponentRole) -> Component {
        Component {
            id: ComponentId(id),
            role,
            inputs: vec![Interface { name: format!("in_{id}") }],
            outputs: vec![Interface { name: format!("out_{id}") }],
        }
    }

    #[test]
    fn test_converts_components_to_design_units() {
        let state = ArchitectureState {
            components: vec![
                make_component(1, ComponentRole::Service),
                make_component(2, ComponentRole::Database),
            ],
            dependencies: vec![],
            deployment: Default::default(),
            constraints: vec![],
            metrics: Default::default(),
        };

        let arch = arch_state_to_architecture(&state);
        let units = arch.design_units_by_id();

        assert_eq!(units.len(), 2);
        assert_eq!(units[&1].name, "service_1");
        assert_eq!(units[&1].layer, Layer::Service);
        assert_eq!(units[&2].name, "database_2");
        assert_eq!(units[&2].layer, Layer::Database);
    }

    #[test]
    fn test_preserves_interfaces() {
        let state = ArchitectureState {
            components: vec![make_component(1, ComponentRole::Gateway)],
            dependencies: vec![],
            deployment: Default::default(),
            constraints: vec![],
            metrics: Default::default(),
        };

        let arch = arch_state_to_architecture(&state);
        let units = arch.design_units_by_id();
        assert_eq!(units[&1].inputs, vec!["in_1"]);
        assert_eq!(units[&1].outputs, vec!["out_1"]);
        assert_eq!(units[&1].layer, Layer::Ui);
    }
}
