use std::collections::BTreeSet;

use design_domain::{Architecture, DependencyKind, DesignUnit, Layer};

pub type Timestamp = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ModuleId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct InterfaceId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TypeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct FunctionId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleType {
    Service,
    Library,
    Worker,
    API,
    Adapter,
    DatabaseAdapter,
    QueueAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceType {
    HTTP,
    RPC,
    Event,
    Queue,
    FunctionCall,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BehaviorSpec {
    CRUD,
    HTTPHandler,
    QueueConsumer,
    QueueProducer,
    EventHandler,
    Computation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyType {
    Call,
    Publish,
    Subscribe,
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    Primitive(String),
    Data(TypeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub field_type: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataType {
    pub id: TypeId,
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub behavior: BehaviorSpec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeModule {
    pub id: ModuleId,
    pub name: String,
    pub module_type: ModuleType,
    pub interfaces: Vec<InterfaceId>,
    pub functions: Vec<FunctionId>,
    pub dependencies: Vec<ModuleId>,
    pub layer: Layer,
    pub responsibilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub id: InterfaceId,
    pub module_id: ModuleId,
    pub name: String,
    pub interface_type: InterfaceType,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub direction: InterfaceDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub source: ModuleId,
    pub target: ModuleId,
    pub dependency_type: DependencyType,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeMetadata {
    pub version: String,
    pub generated_at: Timestamp,
    pub source_architecture: u64,
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
pub struct CodeIR {
    pub modules: Vec<CodeModule>,
    pub interfaces: Vec<Interface>,
    pub datatypes: Vec<DataType>,
    pub functions: Vec<Function>,
    pub dependencies: Vec<Dependency>,
    pub metadata: CodeMetadata,
    pub control_flow: Vec<ControlFlowEdge>,
    pub data_flow: Vec<DataFlowEdge>,
}

pub type CodeIr = CodeIR;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceTree {
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CodeMetrics {
    pub dependency_depth: usize,
    pub coupling_score: f64,
    pub module_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeValidation {
    pub dependency_cycle_count: usize,
    pub missing_interface_count: usize,
    pub unused_module_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeState {
    pub code_ir: CodeIR,
    pub metrics: CodeMetrics,
}

pub trait ArchitectureToCodeIR {
    fn transform(architecture: &Architecture) -> CodeIR;
}

pub trait CodeGenerator {
    fn generate(code_ir: &CodeIR) -> SourceTree;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicArchitectureToCodeIR;

impl ArchitectureToCodeIR for DeterministicArchitectureToCodeIR {
    fn transform(architecture: &Architecture) -> CodeIR {
        CodeIR::from_architecture(architecture)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicCodeGenerator;

impl CodeGenerator for DeterministicCodeGenerator {
    fn generate(code_ir: &CodeIR) -> SourceTree {
        let mut files = code_ir
            .modules
            .iter()
            .map(|module| {
                let imports = code_ir
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.source == module.id)
                    .filter_map(|dependency| {
                        code_ir
                            .modules
                            .iter()
                            .find(|candidate| candidate.id == dependency.target)
                    })
                    .map(|target| format!("use crate::{}::*;", to_snake_case(&target.name)))
                    .collect::<Vec<_>>();
                let functions = module
                    .functions
                    .iter()
                    .filter_map(|function_id| {
                        code_ir.functions.iter().find(|function| function.id == *function_id)
                    })
                    .map(|function| {
                        format!(
                            "pub fn {}() {{ /* {:?} */ }}",
                            to_snake_case(&function.name),
                            function.behavior
                        )
                    })
                    .collect::<Vec<_>>();
                let body = if functions.is_empty() {
                    "pub fn run() {}".to_string()
                } else {
                    functions.join("\n")
                };
                let prefix = if imports.is_empty() {
                    String::new()
                } else {
                    format!("{}\n\n", imports.join("\n"))
                };

                SourceFile {
                    path: format!("{}.rs", to_snake_case(&module.name)),
                    content: format!(
                        "{}pub mod {} {{\n{}\n}}\n",
                        prefix,
                        to_snake_case(&module.name),
                        indent_block(&body)
                    ),
                }
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        SourceTree { files }
    }
}

impl CodeMetadata {
    pub fn for_architecture(architecture: &Architecture) -> Self {
        Self {
            version: "1.0.0".into(),
            generated_at: 0,
            source_architecture: architecture_signature(architecture),
        }
    }
}

impl Default for CodeMetadata {
    fn default() -> Self {
        Self {
            version: "1.0.0".into(),
            generated_at: 0,
            source_architecture: 0,
        }
    }
}

impl CodeIR {
    pub fn from_design_units(units: &[DesignUnit]) -> Self {
        let mut interfaces = Vec::new();
        let mut datatypes = Vec::new();
        let mut functions = Vec::new();
        let mut dependencies = Vec::new();
        let mut next_interface_id = 1_u64;
        let mut next_type_id = 1_u64;
        let mut next_function_id = 1_u64;

        let modules = units
            .iter()
            .map(|unit| {
                let mut module_interface_ids = Vec::new();
                for name in &unit.inputs {
                    let type_ref = TypeRef::Data(TypeId(next_type_id));
                    datatypes.push(DataType {
                        id: TypeId(next_type_id),
                        name: name.clone(),
                        fields: Vec::new(),
                    });
                    interfaces.push(Interface {
                        id: InterfaceId(next_interface_id),
                        module_id: ModuleId(unit.id.0),
                        name: name.clone(),
                        interface_type: infer_interface_type(name),
                        inputs: vec![type_ref.clone()],
                        outputs: Vec::new(),
                        direction: InterfaceDirection::Input,
                    });
                    module_interface_ids.push(InterfaceId(next_interface_id));
                    next_interface_id += 1;
                    next_type_id += 1;
                }
                for name in &unit.outputs {
                    let type_ref = TypeRef::Data(TypeId(next_type_id));
                    datatypes.push(DataType {
                        id: TypeId(next_type_id),
                        name: name.clone(),
                        fields: Vec::new(),
                    });
                    interfaces.push(Interface {
                        id: InterfaceId(next_interface_id),
                        module_id: ModuleId(unit.id.0),
                        name: name.clone(),
                        interface_type: infer_interface_type(name),
                        inputs: Vec::new(),
                        outputs: vec![type_ref.clone()],
                        direction: InterfaceDirection::Output,
                    });
                    module_interface_ids.push(InterfaceId(next_interface_id));
                    next_interface_id += 1;
                    next_type_id += 1;
                }

                let function_name = default_function_name(unit);
                let function_id = FunctionId(next_function_id);
                next_function_id += 1;
                functions.push(Function {
                    id: function_id,
                    name: function_name,
                    inputs: unit
                        .inputs
                        .iter()
                        .map(|name| TypeRef::Primitive(name.clone()))
                        .collect(),
                    outputs: unit
                        .outputs
                        .iter()
                        .map(|name| TypeRef::Primitive(name.clone()))
                        .collect(),
                    behavior: infer_behavior(unit),
                });

                CodeModule {
                    id: ModuleId(unit.id.0),
                    name: unit.name.clone(),
                    module_type: infer_module_type(unit),
                    interfaces: module_interface_ids,
                    functions: vec![function_id],
                    dependencies: unit.dependencies.iter().map(|dependency| ModuleId(dependency.0)).collect(),
                    layer: unit.layer,
                    responsibilities: inferred_responsibilities(unit),
                }
            })
            .collect::<Vec<_>>();

        for unit in units {
            dependencies.extend(unit.dependencies.iter().map(|dependency| Dependency {
                source: ModuleId(unit.id.0),
                target: ModuleId(dependency.0),
                dependency_type: DependencyType::Call,
                kind: DependencyKind::Calls,
            }));
        }

        let control_flow = dependencies
            .iter()
            .map(|dependency| ControlFlowEdge {
                from: dependency.source.0,
                to: dependency.target.0,
                label: format!("{:?}", dependency.dependency_type).to_ascii_lowercase(),
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
            datatypes,
            functions,
            dependencies,
            metadata: CodeMetadata::default(),
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
            .map(|dependency| Dependency {
                source: ModuleId(dependency.from.0),
                target: ModuleId(dependency.to.0),
                dependency_type: map_dependency_type(dependency.kind),
                kind: dependency.kind,
            })
            .collect();
        ir.control_flow = ir
            .dependencies
            .iter()
            .map(|dependency| ControlFlowEdge {
                from: dependency.source.0,
                to: dependency.target.0,
                label: format!("{:?}", dependency.kind).to_ascii_lowercase(),
            })
            .collect();
        ir.metadata = CodeMetadata::for_architecture(architecture);
        ir
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn validate(&self) -> CodeValidation {
        let declared = self
            .modules
            .iter()
            .map(|module| module.id)
            .collect::<BTreeSet<_>>();
        let referenced_interfaces = self
            .modules
            .iter()
            .flat_map(|module| module.interfaces.iter().copied())
            .collect::<BTreeSet<_>>();
        let missing_interface_count = referenced_interfaces
            .iter()
            .filter(|interface_id| !self.interfaces.iter().any(|interface| interface.id == **interface_id))
            .count();
        let used_modules = self
            .dependencies
            .iter()
            .flat_map(|dependency| [dependency.source, dependency.target])
            .collect::<BTreeSet<_>>();
        let unused_module_count = declared.difference(&used_modules).count();

        CodeValidation {
            dependency_cycle_count: dependency_cycle_count(self),
            missing_interface_count,
            unused_module_count,
        }
    }

    pub fn metrics(&self) -> CodeMetrics {
        let dependency_depth = dependency_depth(self);
        let possible_edges = self.modules.len().saturating_mul(self.modules.len().saturating_sub(1));
        let coupling_score = if possible_edges == 0 {
            0.0
        } else {
            self.dependencies.len() as f64 / possible_edges as f64
        };

        CodeMetrics {
            dependency_depth,
            coupling_score: coupling_score.clamp(0.0, 1.0),
            module_count: self.modules.len(),
        }
    }

    pub fn code_state(&self) -> CodeState {
        CodeState {
            code_ir: self.clone(),
            metrics: self.metrics(),
        }
    }
}

fn default_function_name(unit: &DesignUnit) -> String {
    let lower = unit.name.to_ascii_lowercase();
    if lower.contains("handler") || lower.contains("controller") {
        "handle".into()
    } else if lower.contains("consumer") {
        "consume".into()
    } else if lower.contains("producer") {
        "produce".into()
    } else {
        "execute".into()
    }
}

fn infer_interface_type(name: &str) -> InterfaceType {
    let lower = name.to_ascii_lowercase();
    if lower.contains("http") || lower.contains("router") || lower.contains("json") {
        InterfaceType::HTTP
    } else if lower.contains("event") {
        InterfaceType::Event
    } else if lower.contains("queue") {
        InterfaceType::Queue
    } else if lower.contains("rpc") {
        InterfaceType::RPC
    } else {
        InterfaceType::FunctionCall
    }
}

fn infer_behavior(unit: &DesignUnit) -> BehaviorSpec {
    let lower = unit.name.to_ascii_lowercase();
    if lower.contains("handler") || lower.contains("controller") {
        BehaviorSpec::HTTPHandler
    } else if lower.contains("consumer") {
        BehaviorSpec::QueueConsumer
    } else if lower.contains("producer") {
        BehaviorSpec::QueueProducer
    } else if lower.contains("repository") || lower.contains("store") {
        BehaviorSpec::CRUD
    } else if lower.contains("event") {
        BehaviorSpec::EventHandler
    } else {
        BehaviorSpec::Computation
    }
}

fn infer_module_type(unit: &DesignUnit) -> ModuleType {
    match unit.layer {
        Layer::Ui => ModuleType::API,
        Layer::Service => {
            if unit.name.to_ascii_lowercase().contains("worker") {
                ModuleType::Worker
            } else {
                ModuleType::Service
            }
        }
        Layer::Repository => ModuleType::Adapter,
        Layer::Database => ModuleType::DatabaseAdapter,
    }
}

fn map_dependency_type(kind: DependencyKind) -> DependencyType {
    match kind {
        DependencyKind::Calls => DependencyType::Call,
        DependencyKind::Reads => DependencyType::Read,
        DependencyKind::Writes => DependencyType::Write,
        DependencyKind::Emits => DependencyType::Publish,
    }
}

fn inferred_responsibilities(unit: &DesignUnit) -> Vec<String> {
    if unit.semantics.is_empty() {
        vec![format!("{} handling", unit.layer.as_str().to_ascii_lowercase())]
    } else {
        unit.semantics.clone()
    }
}

fn architecture_signature(architecture: &Architecture) -> u64 {
    let mut signature = 0_u64;
    for unit_id in architecture.all_design_unit_ids() {
        signature = signature.wrapping_mul(31).wrapping_add(unit_id);
    }
    for dependency in &architecture.dependencies {
        signature = signature
            .wrapping_mul(131)
            .wrapping_add(dependency.from.0)
            .wrapping_add(dependency.to.0);
    }
    signature
}

fn dependency_depth(ir: &CodeIR) -> usize {
    let mut best = 0;
    for module in &ir.modules {
        best = best.max(depth_from(module.id, ir, &mut Vec::new()));
    }
    best
}

fn depth_from(module: ModuleId, ir: &CodeIR, stack: &mut Vec<ModuleId>) -> usize {
    if stack.contains(&module) {
        return 0;
    }
    stack.push(module);
    let best = ir
        .dependencies
        .iter()
        .filter(|dependency| dependency.source == module)
        .map(|dependency| 1 + depth_from(dependency.target, ir, stack))
        .max()
        .unwrap_or(1);
    stack.pop();
    best
}

fn dependency_cycle_count(ir: &CodeIR) -> usize {
    let mut visited = Vec::new();
    let mut stack = Vec::new();
    let mut cycles = 0;
    let mut ids = ir.modules.iter().map(|module| module.id).collect::<Vec<_>>();
    ids.sort();
    for module_id in ids {
        if dfs_cycle_count(module_id, ir, &mut visited, &mut stack, &mut cycles) {
            stack.clear();
        }
    }
    cycles
}

fn dfs_cycle_count(
    module: ModuleId,
    ir: &CodeIR,
    visited: &mut Vec<ModuleId>,
    stack: &mut Vec<ModuleId>,
    cycles: &mut usize,
) -> bool {
    if stack.contains(&module) {
        *cycles += 1;
        return true;
    }
    if visited.contains(&module) {
        return false;
    }
    visited.push(module);
    stack.push(module);
    let mut found = false;
    for dependency in ir
        .dependencies
        .iter()
        .filter(|dependency| dependency.source == module)
    {
        found |= dfs_cycle_count(dependency.target, ir, visited, stack, cycles);
    }
    stack.pop();
    found
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn indent_block(block: &str) -> String {
    block.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_domain::DesignUnitId;

    #[test]
    fn builds_code_ir_from_design_units() {
        let mut api = DesignUnit::new(1, "ApiController");
        api.inputs.push("HttpRequest".into());
        api.outputs.push("UserDto".into());
        api.dependencies.push(DesignUnitId(2));

        let mut service = DesignUnit::new(2, "UserService");
        service.inputs.push("UserDto".into());
        service.outputs.push("User".into());

        let ir = CodeIR::from_design_units(&[api, service]);

        assert_eq!(ir.module_count(), 2);
        assert_eq!(ir.dependencies.len(), 1);
        assert_eq!(ir.data_flow[0].payload, "UserDto");
        assert_eq!(ir.functions.len(), 2);
        assert!(!ir.datatypes.is_empty());
    }

    #[test]
    fn architecture_transform_is_deterministic() {
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::new(1, "ApiController"));
        architecture.add_design_unit(DesignUnit::new(2, "UserService"));
        architecture.dependencies.push(design_domain::Dependency {
            from: DesignUnitId(1),
            to: DesignUnitId(2),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((1, 2));

        let left = DeterministicArchitectureToCodeIR::transform(&architecture);
        let right = DeterministicArchitectureToCodeIR::transform(&architecture);

        assert_eq!(left, right);
        assert_eq!(left.metadata.version, "1.0.0");
    }

    #[test]
    fn code_ir_validation_detects_cycles_and_unused_modules() {
        let mut module_a = DesignUnit::new(1, "ModuleA");
        module_a.dependencies.push(DesignUnitId(2));
        let mut module_b = DesignUnit::new(2, "ModuleB");
        module_b.dependencies.push(DesignUnitId(1));
        let module_c = DesignUnit::new(3, "ModuleC");

        let ir = CodeIR::from_design_units(&[module_a, module_b, module_c]);
        let validation = ir.validate();
        let metrics = ir.metrics();

        assert_eq!(validation.dependency_cycle_count, 1);
        assert_eq!(validation.unused_module_count, 1);
        assert_eq!(metrics.module_count, 3);
        assert!(metrics.dependency_depth >= 2);
    }
}
