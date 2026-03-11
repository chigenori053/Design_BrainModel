use design_domain::{Architecture, DependencyKind, DesignUnit, Layer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleIr {
    pub id: u64,
    pub name: String,
    pub layer: Layer,
    pub responsibilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceIr {
    pub module_id: u64,
    pub name: String,
    pub direction: InterfaceDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyIr {
    pub from: u64,
    pub to: u64,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlFlowEdge {
    pub from: u64,
    pub to: u64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFlowEdge {
    pub from: u64,
    pub to: u64,
    pub payload: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeIr {
    pub modules: Vec<ModuleIr>,
    pub interfaces: Vec<InterfaceIr>,
    pub dependencies: Vec<DependencyIr>,
    pub control_flow: Vec<ControlFlowEdge>,
    pub data_flow: Vec<DataFlowEdge>,
}

impl CodeIr {
    pub fn from_design_units(units: &[DesignUnit]) -> Self {
        let modules = units
            .iter()
            .map(|unit| ModuleIr {
                id: unit.id.0,
                name: unit.name.clone(),
                layer: unit.layer,
                responsibilities: inferred_responsibilities(unit),
            })
            .collect::<Vec<_>>();
        let interfaces = units
            .iter()
            .flat_map(|unit| {
                unit.inputs
                    .iter()
                    .map(|name| InterfaceIr {
                        module_id: unit.id.0,
                        name: name.clone(),
                        direction: InterfaceDirection::Input,
                    })
                    .chain(unit.outputs.iter().map(|name| InterfaceIr {
                        module_id: unit.id.0,
                        name: name.clone(),
                        direction: InterfaceDirection::Output,
                    }))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let dependencies = units
            .iter()
            .flat_map(|unit| {
                unit.dependencies.iter().map(|dependency| DependencyIr {
                    from: unit.id.0,
                    to: dependency.0,
                    kind: DependencyKind::Calls,
                })
            })
            .collect::<Vec<_>>();
        let control_flow = dependencies
            .iter()
            .map(|dependency| ControlFlowEdge {
                from: dependency.from,
                to: dependency.to,
                label: "calls".to_string(),
            })
            .collect::<Vec<_>>();
        let data_flow = units
            .iter()
            .flat_map(|unit| {
                unit.outputs.iter().flat_map(|output| {
                    unit.dependencies.iter().map(|dependency| DataFlowEdge {
                        from: unit.id.0,
                        to: dependency.0,
                        payload: output.clone(),
                    })
                })
            })
            .collect::<Vec<_>>();

        Self {
            modules,
            interfaces,
            dependencies,
            control_flow,
            data_flow,
        }
    }

    pub fn from_architecture(architecture: &Architecture) -> Self {
        let units = architecture
            .classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .flat_map(|structure| structure.design_units.iter().cloned())
            .collect::<Vec<_>>();
        let mut ir = Self::from_design_units(&units);
        ir.dependencies = architecture
            .dependencies
            .iter()
            .map(|dependency| DependencyIr {
                from: dependency.from.0,
                to: dependency.to.0,
                kind: dependency.kind,
            })
            .collect();
        ir.control_flow = ir
            .dependencies
            .iter()
            .map(|dependency| ControlFlowEdge {
                from: dependency.from,
                to: dependency.to,
                label: format!("{:?}", dependency.kind).to_ascii_lowercase(),
            })
            .collect();
        ir
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
}

fn inferred_responsibilities(unit: &DesignUnit) -> Vec<String> {
    if unit.semantics.is_empty() {
        vec![format!(
            "{} handling",
            unit.layer.as_str().to_ascii_lowercase()
        )]
    } else {
        unit.semantics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_domain::DesignUnit;

    #[test]
    fn builds_code_ir_from_design_units() {
        let mut api = DesignUnit::new(1, "ApiController");
        api.inputs.push("HttpRequest".into());
        api.outputs.push("UserDto".into());
        api.dependencies.push(design_domain::DesignUnitId(2));

        let mut service = DesignUnit::new(2, "UserService");
        service.inputs.push("UserDto".into());
        service.outputs.push("User".into());

        let ir = CodeIr::from_design_units(&[api, service]);

        assert_eq!(ir.module_count(), 2);
        assert_eq!(ir.dependencies.len(), 1);
        assert_eq!(ir.data_flow[0].payload, "UserDto");
    }
}
