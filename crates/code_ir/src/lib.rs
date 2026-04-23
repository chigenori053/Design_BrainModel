use design_domain::{Architecture, DependencyKind, DesignUnit, Layer};

// ── Module-level IR (Step5) ──────────────────────────────────────────────────

/// A single import declaration: `use module::{items}` / `from module import items`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrImport {
    pub module: String,
    pub items: Vec<String>,
}

impl IrImport {
    pub fn new(module: impl Into<String>, items: Vec<impl Into<String>>) -> Self {
        Self {
            module: module.into(),
            items: items.into_iter().map(|i| i.into()).collect(),
        }
    }
}

/// A named collection of functions with explicit imports — maps to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrModule {
    pub name: String,
    pub functions: Vec<IrFunction>,
    pub imports: Vec<IrImport>,
}

impl IrModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), functions: vec![], imports: vec![] }
    }

    pub fn with_functions(mut self, functions: Vec<IrFunction>) -> Self {
        self.functions = functions;
        self
    }

    pub fn with_imports(mut self, imports: Vec<IrImport>) -> Self {
        self.imports = imports;
        self
    }
}

// ── Function-level IR (Step4) ────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrType {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrParam {
    pub name: String,
    pub ty: Option<IrType>,
}

impl IrParam {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ty: None }
    }

    pub fn typed(name: impl Into<String>, ty: IrType) -> Self {
        Self { name: name.into(), ty: Some(ty) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: Option<IrType>,
    pub body: Vec<IrStep>,
}

impl IrFunction {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: vec![],
            return_type: None,
            body: vec![],
        }
    }

    pub fn with_params(mut self, params: Vec<IrParam>) -> Self {
        self.params = params;
        self
    }

    pub fn with_return_type(mut self, ty: IrType) -> Self {
        self.return_type = Some(ty);
        self
    }

    pub fn with_body(mut self, body: Vec<IrStep>) -> Self {
        self.body = body;
        self
    }
}

// ── Step-level IR ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrOp {
    Assign,
    Call,
    Return,
    Branch,
    Block,
    Loop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    pub name: String,
}

impl Value {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub text: String,
}

impl Expr {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrStep {
    pub op: IrOp,
    pub inputs: Vec<Value>,
    pub outputs: Vec<Value>,
    pub condition: Option<Expr>,
    pub body: Option<Vec<IrStep>>,
    pub else_body: Option<Vec<IrStep>>,
}

impl IrStep {
    pub fn assign(output: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            op: IrOp::Assign,
            inputs: vec![Value::new(input)],
            outputs: vec![Value::new(output)],
            condition: None,
            body: None,
            else_body: None,
        }
    }

    pub fn call(func: impl Into<String>, args: Vec<impl Into<String>>) -> Self {
        Self {
            op: IrOp::Call,
            inputs: args.into_iter().map(Value::new).collect(),
            outputs: vec![Value::new(func)],
            condition: None,
            body: None,
            else_body: None,
        }
    }

    pub fn return_val(val: impl Into<String>) -> Self {
        Self {
            op: IrOp::Return,
            inputs: vec![Value::new(val)],
            outputs: vec![],
            condition: None,
            body: None,
            else_body: None,
        }
    }

    pub fn branch(
        condition: impl Into<String>,
        then_body: Vec<IrStep>,
        else_body: Option<Vec<IrStep>>,
    ) -> Self {
        Self {
            op: IrOp::Branch,
            inputs: vec![],
            outputs: vec![],
            condition: Some(Expr::new(condition)),
            body: Some(then_body),
            else_body,
        }
    }

    pub fn block(body: Vec<IrStep>) -> Self {
        Self {
            op: IrOp::Block,
            inputs: vec![],
            outputs: vec![],
            condition: None,
            body: Some(body),
            else_body: None,
        }
    }

    pub fn loop_step(iter_expr: impl Into<String>, body: Vec<IrStep>) -> Self {
        Self {
            op: IrOp::Loop,
            inputs: vec![Value::new(iter_expr)],
            outputs: vec![],
            condition: None,
            body: Some(body),
            else_body: None,
        }
    }
}

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
    pub functions: Vec<IrFunction>,
    /// Step5: project-level module layout (each IrModule → one file).
    pub ir_modules: Vec<IrModule>,
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
            functions: vec![],
            ir_modules: vec![],
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
