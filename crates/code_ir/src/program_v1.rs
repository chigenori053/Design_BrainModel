use std::collections::BTreeMap;

use design_domain::Architecture;
use serde::{Deserialize, Serialize};
use unified_design_ir::{FieldSpec, ImplementationUnit, InterfaceSpec, MethodSpec, StructSpec};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    pub metadata: Metadata,
    pub modules: Vec<Module>,
    pub dependencies: Vec<Dependency>,
    pub generation_strategy: GenerationStrategy,
    pub build_validation: BuildValidation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub target_domains: Vec<TargetDomain>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TargetDomain {
    Web,
    Cli,
    Backend,
    Embedded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub visibility: Visibility,
    pub imports: Vec<String>,
    pub types: Vec<Type>,
    pub functions: Vec<Function>,
    pub state: Vec<State>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Type {
    pub kind: TypeKind,
    pub name: String,
    pub generics: Vec<Generic>,
    pub fields: Vec<TypeField>,
    pub variants: Vec<TypeVariant>,
    pub methods: Vec<FunctionSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TypeKind {
    Struct,
    Enum,
    Interface,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeField {
    pub name: String,
    pub r#type: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeVariant {
    pub name: String,
    pub fields: Vec<TypeField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRef {
    pub name: String,
    pub generics: Vec<TypeRef>,
    pub nullable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generic {
    pub name: String,
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub visibility: Visibility,
    pub inputs: Vec<FunctionInput>,
    pub outputs: FunctionOutput,
    pub effects: Vec<Effect>,
    pub can_fail: bool,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSignature {
    pub name: String,
    pub inputs: Vec<FunctionInput>,
    pub outputs: FunctionOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionInput {
    pub name: String,
    pub r#type: TypeRef,
    pub borrow: Option<BorrowInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionOutput {
    pub r#type: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Effect {
    Pure,
    IO,
    Mutation,
    Async,
    Error,
    Blocking,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorrowInfo {
    pub is_mut: bool,
    pub lifetime: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    pub r#type: TypeRef,
    pub kind: StateKind,
    pub scope: StateScope,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StateKind {
    Immutable,
    Mutable,
    Shared,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StateScope {
    Global,
    Module,
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Statement {
    Assign { target: String, value: Expression },
    If(IfStatement),
    Loop(LoopStatement),
    Return { value: Option<Expression> },
    Expression(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_block: Block,
    pub else_block: Block,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopStatement {
    pub kind: LoopKind,
    pub iterator: Expression,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LoopKind {
    For,
    While,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expression {
    Literal(Literal),
    Variable(String),
    Call(Call),
    Await(Box<Expression>),
    BinaryOp(BinaryOp),
    UnaryOp(UnaryOp),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Float(String),
    Bool(bool),
    String(String),
    Void,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub function: String,
    pub args: Vec<Expression>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryOp {
    pub op: BinaryOperator,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnaryOp {
    pub op: UnaryOperator,
    pub expr: Box<Expression>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnaryOperator {
    Neg,
    Not,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DependencyKind {
    Runtime,
    Dev,
    Build,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationStrategy {
    pub mode: GenerationMode,
    pub safety: SafetyPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GenerationMode {
    New,
    Merge,
    Overwrite,
    DryRun,
    SafeMerge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub backup: bool,
    pub check: bool,
    pub rollback_on_fail: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildValidation {
    pub enabled: bool,
    pub command: String,
    pub sandbox: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BackendLanguage {
    Rust,
    Python,
    TypeScript,
}

impl Program {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: Metadata {
                name: name.into(),
                version: "1.0.0".to_string(),
                target_domains: vec![TargetDomain::Backend],
            },
            modules: Vec::new(),
            dependencies: Vec::new(),
            generation_strategy: GenerationStrategy::default(),
            build_validation: BuildValidation::for_backend(BackendLanguage::Rust),
        }
    }

    pub fn from_implementation_units(
        name: impl Into<String>,
        version: impl Into<String>,
        target_domains: Vec<TargetDomain>,
        units: &[ImplementationUnit],
    ) -> Self {
        let mut modules = units.iter().map(module_from_unit).collect::<Vec<_>>();
        modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

        let mut dependency_names = units
            .iter()
            .flat_map(|unit| unit.dependencies.iter().cloned())
            .collect::<Vec<_>>();
        dependency_names.sort();
        dependency_names.dedup();
        let dependencies = dependency_names
            .into_iter()
            .map(|name| Dependency {
                name,
                version: "*".to_string(),
                kind: DependencyKind::Runtime,
            })
            .collect::<Vec<_>>();

        Self {
            metadata: Metadata {
                name: name.into(),
                version: version.into(),
                target_domains,
            },
            modules,
            dependencies,
            generation_strategy: GenerationStrategy::default(),
            build_validation: BuildValidation::for_backend(BackendLanguage::Rust),
        }
    }

    pub fn from_architecture(
        name: impl Into<String>,
        version: impl Into<String>,
        target_domains: Vec<TargetDomain>,
        architecture: &Architecture,
    ) -> Self {
        let units = architecture
            .classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .flat_map(|structure| structure.design_units.iter())
            .collect::<Vec<_>>();

        let mut modules = units
            .iter()
            .map(|unit| Module {
                name: unit.name.clone(),
                visibility: Visibility::Public,
                imports: unit
                    .dependencies
                    .iter()
                    .map(|dependency| format!("crate::unit_{}", dependency.0))
                    .collect(),
                types: vec![Type {
                    kind: TypeKind::Struct,
                    name: format!("{}State", pascal_case(&unit.name)),
                    generics: Vec::new(),
                    fields: unit
                        .inputs
                        .iter()
                        .map(|input| TypeField {
                            name: snake_case(input),
                            r#type: primitive_or_named(input),
                        })
                        .collect(),
                    variants: Vec::new(),
                    methods: Vec::new(),
                }],
                functions: vec![Function {
                    name: format!("handle_{}", snake_case(&unit.name)),
                    visibility: Visibility::Public,
                    inputs: unit
                        .inputs
                        .iter()
                        .map(|input| FunctionInput {
                            name: snake_case(input),
                            r#type: primitive_or_named(input),
                            borrow: None,
                        })
                        .collect(),
                    outputs: FunctionOutput {
                        r#type: if let Some(output) = unit.outputs.first() {
                            primitive_or_named(output)
                        } else {
                            TypeRef::void()
                        },
                    },
                    effects: effects_from_unit(unit),
                    can_fail: false,
                    body: default_body(unit.outputs.first()),
                }],
                state: unit
                    .outputs
                    .iter()
                    .map(|output| State {
                        name: snake_case(output),
                        r#type: primitive_or_named(output),
                        kind: StateKind::Mutable,
                        scope: StateScope::Module,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

        let mut dependencies = architecture
            .dependencies
            .iter()
            .map(|dependency| Dependency {
                name: format!("unit_{}_{}", dependency.from.0, dependency.to.0),
                version: "*".to_string(),
                kind: DependencyKind::Runtime,
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
        dependencies.dedup_by(|lhs, rhs| lhs.name == rhs.name);

        Self {
            metadata: Metadata {
                name: name.into(),
                version: version.into(),
                target_domains,
            },
            modules,
            dependencies,
            generation_strategy: GenerationStrategy::default(),
            build_validation: BuildValidation::for_backend(BackendLanguage::Rust),
        }
    }

    pub fn render_stub_source_tree(&self, backend: BackendLanguage) -> Vec<(String, String)> {
        let mut files = self
            .modules
            .iter()
            .map(|module| {
                let file_name = match backend {
                    BackendLanguage::Rust => format!("{}.rs", snake_case(&module.name)),
                    BackendLanguage::Python => format!("{}.py", snake_case(&module.name)),
                    BackendLanguage::TypeScript => format!("{}.ts", snake_case(&module.name)),
                };
                (file_name, render_module_stub(module, backend.clone()))
            })
            .collect::<Vec<_>>();
        files.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        files
    }
}

impl Default for GenerationStrategy {
    fn default() -> Self {
        Self {
            mode: GenerationMode::DryRun,
            safety: SafetyPolicy {
                backup: true,
                check: true,
                rollback_on_fail: true,
            },
        }
    }
}

impl BuildValidation {
    pub fn for_backend(backend: BackendLanguage) -> Self {
        let command = match backend {
            BackendLanguage::Rust => "cargo check",
            BackendLanguage::Python => "python -m py_compile",
            BackendLanguage::TypeScript => "npm build",
        };
        Self {
            enabled: true,
            command: command.to_string(),
            sandbox: true,
        }
    }
}

impl TypeRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            generics: Vec::new(),
            nullable: false,
        }
    }

    pub fn primitive(name: impl Into<String>) -> Self {
        Self::named(name)
    }

    pub fn void() -> Self {
        Self::named("Void")
    }
}

fn module_from_unit(unit: &ImplementationUnit) -> Module {
    let mut types = unit
        .public_interfaces
        .iter()
        .map(interface_from_spec)
        .chain(unit.internal_structs.iter().map(struct_from_spec))
        .collect::<Vec<_>>();
    types.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    let mut imports = unit
        .dependencies
        .iter()
        .map(|dependency| format!("crate::{}", snake_case(dependency)))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();

    let mut functions = unit
        .public_interfaces
        .iter()
        .flat_map(|interface| interface.methods.iter())
        .map(function_from_method)
        .collect::<Vec<_>>();
    functions.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    let mut state = unit
        .internal_structs
        .iter()
        .flat_map(|structure| structure.fields.iter())
        .map(state_from_field)
        .collect::<Vec<_>>();
    state.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    Module {
        name: unit.module_name.clone(),
        visibility: Visibility::Public,
        imports,
        types,
        functions,
        state,
    }
}

fn interface_from_spec(interface: &InterfaceSpec) -> Type {
    let mut methods = interface
        .methods
        .iter()
        .map(signature_from_method)
        .collect::<Vec<_>>();
    methods.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    Type {
        kind: TypeKind::Interface,
        name: interface.name.clone(),
        generics: Vec::new(),
        fields: Vec::new(),
        variants: Vec::new(),
        methods,
    }
}

fn struct_from_spec(structure: &StructSpec) -> Type {
    let mut fields = structure
        .fields
        .iter()
        .map(field_from_spec)
        .collect::<Vec<_>>();
    fields.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    Type {
        kind: TypeKind::Struct,
        name: structure.name.clone(),
        generics: Vec::new(),
        fields,
        variants: Vec::new(),
        methods: Vec::new(),
    }
}

fn function_from_method(method: &MethodSpec) -> Function {
    Function {
        name: method.name.clone(),
        visibility: Visibility::Public,
        inputs: method
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| FunctionInput {
                name: format!("arg_{index}"),
                r#type: map_unified_type_ref(input),
                borrow: None,
            })
            .collect(),
        outputs: FunctionOutput {
            r#type: method
                .output
                .as_ref()
                .map(map_unified_type_ref)
                .unwrap_or_else(TypeRef::void),
        },
        effects: vec![Effect::Pure],
        can_fail: false,
        body: Block {
            statements: vec![Statement::Return {
                value: default_expression_for_output(method.output.as_ref()),
            }],
        },
    }
}

fn signature_from_method(method: &MethodSpec) -> FunctionSignature {
    FunctionSignature {
        name: method.name.clone(),
        inputs: method
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| FunctionInput {
                name: format!("arg_{index}"),
                r#type: map_unified_type_ref(input),
                borrow: None,
            })
            .collect(),
        outputs: FunctionOutput {
            r#type: method
                .output
                .as_ref()
                .map(map_unified_type_ref)
                .unwrap_or_else(TypeRef::void),
        },
    }
}

fn field_from_spec(field: &FieldSpec) -> TypeField {
    TypeField {
        name: field.name.clone(),
        r#type: map_unified_type_ref(&field.ty),
    }
}

fn state_from_field(field: &FieldSpec) -> State {
    State {
        name: field.name.clone(),
        r#type: map_unified_type_ref(&field.ty),
        kind: StateKind::Mutable,
        scope: StateScope::Module,
    }
}

fn map_unified_type_ref(ty: &unified_design_ir::TypeRef) -> TypeRef {
    match ty {
        unified_design_ir::TypeRef::Primitive(name) | unified_design_ir::TypeRef::Custom(name) => {
            TypeRef::named(name.clone())
        }
        unified_design_ir::TypeRef::List(inner) => TypeRef {
            name: "List".to_string(),
            generics: vec![map_unified_type_ref(inner)],
            nullable: false,
        },
        unified_design_ir::TypeRef::Optional(inner) => {
            let mut mapped = map_unified_type_ref(inner);
            mapped.nullable = true;
            mapped
        }
    }
}

fn primitive_or_named(name: &str) -> TypeRef {
    match name {
        "int" | "Int" => TypeRef::primitive("Int"),
        "float" | "Float" => TypeRef::primitive("Float"),
        "bool" | "Bool" => TypeRef::primitive("Bool"),
        "string" | "String" => TypeRef::primitive("String"),
        "void" | "Void" => TypeRef::void(),
        other => TypeRef::named(pascal_case(other)),
    }
}

fn effects_from_unit(unit: &design_domain::DesignUnit) -> Vec<Effect> {
    let mut counts = BTreeMap::<Effect, usize>::new();
    counts.insert(Effect::Pure, 1);
    if !unit.dependencies.is_empty() {
        counts.insert(Effect::IO, 1);
    }
    if !unit.outputs.is_empty() {
        counts.insert(Effect::Mutation, 1);
    }
    counts.into_keys().collect()
}

fn default_body(output: Option<&String>) -> Block {
    Block {
        statements: vec![Statement::Return {
            value: output.map(|name| Expression::Variable(snake_case(name))),
        }],
    }
}

fn default_expression_for_output(
    output: Option<&unified_design_ir::TypeRef>,
) -> Option<Expression> {
    output.map(|ty| match ty {
        unified_design_ir::TypeRef::Primitive(name) if name == "String" => {
            Expression::Literal(Literal::String(String::new()))
        }
        unified_design_ir::TypeRef::Primitive(name) if name == "Bool" => {
            Expression::Literal(Literal::Bool(false))
        }
        unified_design_ir::TypeRef::Primitive(name) if name == "Int" => {
            Expression::Literal(Literal::Int(0))
        }
        _ => Expression::Literal(Literal::Void),
    })
}

fn render_module_stub(module: &Module, backend: BackendLanguage) -> String {
    match backend {
        BackendLanguage::Rust => render_rust_module(module),
        BackendLanguage::Python => render_python_module(module),
        BackendLanguage::TypeScript => render_typescript_module(module),
    }
}

fn render_rust_module(module: &Module) -> String {
    let mut lines = module
        .imports
        .iter()
        .map(|import| format!("use {import};"))
        .collect::<Vec<_>>();
    if !lines.is_empty() {
        lines.push(String::new());
    }
    for ty in &module.types {
        match ty.kind {
            TypeKind::Struct => {
                lines.push(format!("pub struct {} {{", ty.name));
                for field in &ty.fields {
                    lines.push(format!(
                        "    pub {}: {},",
                        field.name,
                        render_rust_type(&field.r#type)
                    ));
                }
                lines.push("}".to_string());
            }
            TypeKind::Interface => {
                lines.push(format!("pub trait {} {{", ty.name));
                for method in &ty.methods {
                    lines.push(format!(
                        "    fn {}({}) -> {};",
                        method.name,
                        render_rust_inputs(&method.inputs),
                        render_rust_type(&method.outputs.r#type)
                    ));
                }
                lines.push("}".to_string());
            }
            TypeKind::Enum | TypeKind::Alias => {}
        }
        lines.push(String::new());
    }
    for function in &module.functions {
        let async_prefix = if function.effects.contains(&Effect::Async) {
            "async "
        } else {
            ""
        };
        lines.push(format!(
            "pub {}fn {}({}) -> {} {{",
            async_prefix,
            function.name,
            render_rust_inputs(&function.inputs),
            render_rust_type(&function.outputs.r#type)
        ));
        lines.push("    todo!()".to_string());
        lines.push("}".to_string());
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n"))
}

fn render_python_module(module: &Module) -> String {
    let mut lines = Vec::new();
    for ty in &module.types {
        if ty.kind == TypeKind::Struct {
            lines.push(format!("class {}:", ty.name));
            lines.push("    pass".to_string());
            lines.push(String::new());
        }
    }
    for function in &module.functions {
        let async_prefix = if function.effects.contains(&Effect::Async) {
            "async "
        } else {
            ""
        };
        lines.push(format!(
            "{}def {}({}):",
            async_prefix,
            function.name,
            function
                .inputs
                .iter()
                .map(|input| input.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        lines.push("    pass".to_string());
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n"))
}

fn render_typescript_module(module: &Module) -> String {
    let mut lines = Vec::new();
    for ty in &module.types {
        match ty.kind {
            TypeKind::Struct => {
                lines.push(format!("export interface {} {{", ty.name));
                for field in &ty.fields {
                    lines.push(format!(
                        "  {}: {};",
                        field.name,
                        render_typescript_type(&field.r#type)
                    ));
                }
                lines.push("}".to_string());
                lines.push(String::new());
            }
            TypeKind::Interface => {
                lines.push(format!("export interface {} {{", ty.name));
                for method in &ty.methods {
                    lines.push(format!(
                        "  {}({}): {};",
                        method.name,
                        render_ts_inputs(&method.inputs),
                        render_typescript_type(&method.outputs.r#type)
                    ));
                }
                lines.push("}".to_string());
                lines.push(String::new());
            }
            TypeKind::Enum | TypeKind::Alias => {}
        }
    }
    for function in &module.functions {
        let output = if function.effects.contains(&Effect::Async) {
            format!(
                "Promise<{}>",
                render_typescript_type(&function.outputs.r#type)
            )
        } else {
            render_typescript_type(&function.outputs.r#type)
        };
        lines.push(format!(
            "export function {}({}): {} {{",
            function.name,
            render_ts_inputs(&function.inputs),
            output
        ));
        lines.push("  throw new Error(\"TODO\");".to_string());
        lines.push("}".to_string());
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n"))
}

fn render_rust_inputs(inputs: &[FunctionInput]) -> String {
    inputs
        .iter()
        .map(|input| format!("{}: {}", input.name, render_rust_type(&input.r#type)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_ts_inputs(inputs: &[FunctionInput]) -> String {
    inputs
        .iter()
        .map(|input| format!("{}: {}", input.name, render_typescript_type(&input.r#type)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_rust_type(ty: &TypeRef) -> String {
    if ty.generics.is_empty() {
        if ty.nullable {
            format!("Option<{}>", ty.name)
        } else {
            ty.name.clone()
        }
    } else {
        let inner = ty
            .generics
            .iter()
            .map(render_rust_type)
            .collect::<Vec<_>>()
            .join(", ");
        if ty.nullable {
            format!("Option<{}<{}>>", ty.name, inner)
        } else {
            format!("{}<{}>", ty.name, inner)
        }
    }
}

fn render_typescript_type(ty: &TypeRef) -> String {
    let base = match ty.name.as_str() {
        "Int" | "Float" => "number".to_string(),
        "Bool" => "boolean".to_string(),
        "String" => "string".to_string(),
        "Void" => "void".to_string(),
        other => other.to_string(),
    };
    if ty.nullable {
        format!("{base} | null")
    } else {
        base
    }
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn pascal_case(value: &str) -> String {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect()
}
