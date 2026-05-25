use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::stable_v03::dynamic_ir::LanguageInterpreter;
use code_ir::program_v1::{
    BinaryOperator as ProgramBinaryOperator, Block as ProgramBlock, Effect as ProgramEffect,
    Expression as ProgramExpression, Function as ProgramFunction,
    FunctionInput as ProgramFunctionInput, FunctionOutput as ProgramFunctionOutput,
    Literal as ProgramLiteral, Module as ProgramModule, Program, State as ProgramState,
    StateKind as ProgramStateKind, Statement as ProgramStatement, Type as ProgramType,
    TypeKind as ProgramTypeKind, TypeRef as ProgramTypeRef, Visibility as ProgramVisibility,
};
use memory_engine::{MemoryEngine, MemoryQuery, MemoryRecord};
use unified_design_ir::{
    FieldSpec, ImplementationUnit, InterfaceSpec, MethodSpec, StructSpec, TypeRef,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeModule {
    pub name: String,
    pub imports: Vec<String>,
    pub interfaces: Vec<CodeInterface>,
    pub structs: Vec<CodeStruct>,
    pub functions: Vec<CodeFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeInterface {
    pub name: String,
    pub methods: Vec<CodeFunctionSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeFunctionSignature {
    pub name: String,
    pub inputs: Vec<TypeRef>,
    pub output: Option<TypeRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeStruct {
    pub name: String,
    pub fields: Vec<FieldSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeFunction {
    pub name: String,
    pub inputs: Vec<TypeRef>,
    pub output: Option<TypeRef>,
    pub body: CodeBlock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    pub statements: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetProgram {
    pub modules: Vec<TargetModule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetModule {
    pub name: String,
    pub items: Vec<TargetItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetItem {
    Function(TargetFunction),
    Type(TargetType),
    State(TargetState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetType {
    pub name: String,
    pub kind: TargetTypeKind,
    pub fields: Vec<TargetField>,
    pub methods: Vec<TargetFunctionSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetTypeKind {
    Struct,
    Enum,
    Interface,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetField {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetFunctionSignature {
    pub name: String,
    pub inputs: Vec<TargetArgument>,
    pub output: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetFunction {
    pub name: String,
    pub visibility: String,
    pub inputs: Vec<TargetArgument>,
    pub output: String,
    pub semantics: TargetEffects,
    pub body: Vec<TargetStmt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetArgument {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetState {
    pub name: String,
    pub ty: String,
    pub binding: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetStmt {
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetExpr {
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TargetEffects {
    pub is_async: bool,
    pub is_mut: bool,
    pub returns_promise: bool,
    pub can_fail: bool,
    pub is_blocking: bool,
}

pub trait LanguageBackend {
    fn lower_program(&self, program: &Program) -> TargetProgram;
    fn lower_module(&self, module: &ProgramModule) -> TargetModule;
    fn lower_type(&self, ty: &ProgramType) -> TargetType;
    fn lower_function(&self, func: &ProgramFunction) -> TargetFunction;
    fn lower_expression(&self, expr: &ProgramExpression) -> TargetExpr;
    fn lower_statement(&self, stmt: &ProgramStatement) -> TargetStmt;
    fn lower_effects(&self, effects: &[ProgramEffect]) -> TargetEffects;
}

pub trait Renderer {
    fn render_program(&self, program: &TargetProgram) -> String;
}

pub mod dynamic_ir {
    use super::*;

    pub type SemanticProgram = Program;
    pub type SemanticModule = ProgramModule;
    pub type SemanticType = ProgramType;
    pub type SemanticFunction = ProgramFunction;
    pub type SemanticExpression = ProgramExpression;
    pub type SemanticStatement = ProgramStatement;

    #[derive(Clone, Debug, PartialEq)]
    pub struct LanguageProfile {
        pub name: String,
        pub type_system: TypeSystem,
        pub error_model: ErrorModel,
        pub concurrency: ConcurrencyModel,
        pub rules: Vec<MappingRule>,
        pub import_rules: Vec<ImportRule>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TypeSystem {
        pub primitive_map: HashMap<String, String>,
        pub nullable_format: NullableFormat,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum NullableFormat {
        Option,
        Optional,
        UnionNull,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct MappingRule {
        pub id: String,
        pub priority: u32,
        pub condition: SemanticCondition,
        pub transform: TransformTemplate,
        pub target: TargetScope,
        pub source: RuleSource,
        pub confidence: f32,
        pub usage_count: u32,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RuleSource {
        Static,
        Learned,
        User,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ErrorModel {
        Result,
        Exception,
        Union,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ConcurrencyModel {
        AsyncAwait,
        Thread,
        None,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SemanticCondition {
        HasEffect(ProgramEffect),
        CanFail,
        TypeIs(String),
        IsAsync,
        IsMutable,
        Always,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TransformTemplate {
        FunctionSignature { pattern: String },
        ReturnWrapper { pattern: String },
        TypeMapping { pattern: String },
        MutationMarker { pattern: String },
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TargetScope {
        Function,
        Type,
        Expression,
        Module,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ImportRule {
        pub symbol: String,
        pub import: String,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct RuleEngine {
        pub rules: Vec<MappingRule>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SemanticContext {
        pub function_name: Option<String>,
        pub type_name: Option<String>,
        pub output_type: Option<String>,
        pub effects: Vec<ProgramEffect>,
        pub can_fail: bool,
        pub is_mutable: bool,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RuleValidationError {
        pub rule_id: String,
        pub message: String,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct EvaluationResult {
        pub build_success: bool,
        pub test_pass: bool,
        pub lint_score: f32,
        pub performance_score: f32,
        pub errors: Vec<String>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct MemoryRuleRecord {
        pub rule: MappingRule,
        pub history: Vec<f32>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct ValidatedRuleRecord {
        pub rule: MappingRule,
        pub validation_score: f32,
        pub passed_checks: Vec<ValidationCheck>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ValidationCheck {
        RegressionPass,
        Deterministic,
        NoConflict,
        DiffSafe,
        CrossLanguageConsistent,
    }

    #[derive(Clone, Debug, PartialEq, Default)]
    pub struct ValidationContext {
        pub active_rules: Vec<MappingRule>,
        pub regression_pass: bool,
        pub deterministic: bool,
        pub diff_safe: bool,
        pub cross_language_consistent: bool,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct ValidationResult {
        pub passed: bool,
        pub score: f32,
        pub checks: Vec<ValidationCheck>,
    }

    pub trait RuleValidator {
        fn validate(&self, rule: &MappingRule, ctx: &ValidationContext) -> ValidationResult;
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct DefaultRuleValidator;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RuleBucket {
        Active,
        Candidate,
        Deprecated,
    }

    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct RuleStore {
        pub active_rules: Vec<MemoryRuleRecord>,
        pub candidate_rules: Vec<MemoryRuleRecord>,
        pub validated_rules: Vec<ValidatedRuleRecord>,
        pub deprecated_rules: Vec<MemoryRuleRecord>,
    }

    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct LearningEngine;

    pub trait LanguageInterpreter {
        fn interpret_program(
            &self,
            program: &SemanticProgram,
            profile: &LanguageProfile,
        ) -> TargetProgram;

        fn interpret_module(
            &self,
            module: &SemanticModule,
            profile: &LanguageProfile,
        ) -> TargetModule;

        fn interpret_type(&self, ty: &SemanticType, profile: &LanguageProfile) -> TargetType;

        fn interpret_function(
            &self,
            func: &SemanticFunction,
            profile: &LanguageProfile,
        ) -> TargetFunction;

        fn interpret_expression(
            &self,
            expr: &SemanticExpression,
            profile: &LanguageProfile,
        ) -> TargetExpr;

        fn interpret_statement(
            &self,
            stmt: &SemanticStatement,
            profile: &LanguageProfile,
        ) -> TargetStmt;

        fn interpret_effects(
            &self,
            effects: &[ProgramEffect],
            profile: &LanguageProfile,
        ) -> TargetEffects;
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct DynamicInterpreter {
        pub rule_engine: RuleEngine,
        pub profile: LanguageProfile,
    }

    impl DynamicInterpreter {
        pub fn new(profile: LanguageProfile) -> Self {
            Self {
                rule_engine: RuleEngine::new(profile.rules.clone()),
                profile,
            }
        }
    }

    impl LanguageInterpreter for DynamicInterpreter {
        fn interpret_program(
            &self,
            program: &SemanticProgram,
            _profile: &LanguageProfile,
        ) -> TargetProgram {
            TargetProgram {
                modules: program
                    .modules
                    .iter()
                    .map(|module| self.interpret_module(module, &self.profile))
                    .collect(),
            }
        }

        fn interpret_module(
            &self,
            module: &SemanticModule,
            _profile: &LanguageProfile,
        ) -> TargetModule {
            let mut items =
                module
                    .types
                    .iter()
                    .map(|ty| TargetItem::Type(self.interpret_type(ty, &self.profile)))
                    .chain(module.functions.iter().map(|func| {
                        TargetItem::Function(self.interpret_function(func, &self.profile))
                    }))
                    .collect::<Vec<_>>();
            items.extend(module.state.iter().map(|state| {
                TargetItem::State(TargetState {
                    name: state.name.clone(),
                    ty: lower_type_ref(&state.r#type, &self.profile),
                    binding: state_binding(state, &self.profile),
                })
            }));
            TargetModule {
                name: module.name.clone(),
                items,
            }
        }

        fn interpret_type(&self, ty: &SemanticType, _profile: &LanguageProfile) -> TargetType {
            TargetType {
                name: ty.name.clone(),
                kind: lower_type_kind(&ty.kind),
                fields: ty
                    .fields
                    .iter()
                    .map(|field| TargetField {
                        name: field.name.clone(),
                        ty: lower_type_ref(&field.r#type, &self.profile),
                    })
                    .collect(),
                methods: ty
                    .methods
                    .iter()
                    .map(|method| TargetFunctionSignature {
                        name: method.name.clone(),
                        inputs: method
                            .inputs
                            .iter()
                            .map(|input| TargetArgument {
                                name: input.name.clone(),
                                ty: lower_type_ref(&input.r#type, &self.profile),
                            })
                            .collect(),
                        output: lower_type_ref(&method.outputs.r#type, &self.profile),
                    })
                    .collect(),
            }
        }

        fn interpret_function(
            &self,
            func: &SemanticFunction,
            _profile: &LanguageProfile,
        ) -> TargetFunction {
            let ctx = SemanticContext::from_function(func);
            let semantics = apply_effect_rules(&self.rule_engine, &ctx, &self.profile);
            TargetFunction {
                name: func.name.clone(),
                visibility: lower_visibility(&func.visibility),
                inputs: func
                    .inputs
                    .iter()
                    .map(|input| TargetArgument {
                        name: input.name.clone(),
                        ty: lower_input_type(input, &ctx, &self.rule_engine, &self.profile),
                    })
                    .collect(),
                output: lower_function_output(
                    &func.outputs,
                    &ctx,
                    &self.rule_engine,
                    &self.profile,
                ),
                semantics,
                body: func
                    .body
                    .statements
                    .iter()
                    .map(|statement| self.interpret_statement(statement, &self.profile))
                    .collect(),
            }
        }

        fn interpret_expression(
            &self,
            expr: &SemanticExpression,
            _profile: &LanguageProfile,
        ) -> TargetExpr {
            TargetExpr {
                code: lower_expression_code(expr, backend_flavor(&self.profile)),
            }
        }

        fn interpret_statement(
            &self,
            stmt: &SemanticStatement,
            _profile: &LanguageProfile,
        ) -> TargetStmt {
            TargetStmt {
                code: lower_statement_code_with_profile(self, stmt, &self.profile),
            }
        }

        fn interpret_effects(
            &self,
            effects: &[ProgramEffect],
            _profile: &LanguageProfile,
        ) -> TargetEffects {
            let is_async = effects.contains(&ProgramEffect::Async);
            let is_mut = effects.contains(&ProgramEffect::Mutation);
            TargetEffects {
                is_async,
                is_mut,
                returns_promise: (self.profile.name == "ts" || self.profile.name == "typescript")
                    && is_async,
                can_fail: effects.contains(&ProgramEffect::Error),
                is_blocking: effects.contains(&ProgramEffect::Blocking),
            }
        }
    }

    pub fn rust_profile() -> LanguageProfile {
        LanguageProfile {
            name: "rust".into(),
            type_system: TypeSystem {
                primitive_map: HashMap::from([
                    ("Int".into(), "i32".into()),
                    ("Float".into(), "f64".into()),
                    ("Bool".into(), "bool".into()),
                    ("String".into(), "String".into()),
                    ("Void".into(), "()".into()),
                ]),
                nullable_format: NullableFormat::Option,
            },
            error_model: ErrorModel::Result,
            concurrency: ConcurrencyModel::AsyncAwait,
            rules: vec![
                MappingRule {
                    id: "async_fn".into(),
                    priority: 10,
                    condition: SemanticCondition::HasEffect(ProgramEffect::Async),
                    transform: TransformTemplate::FunctionSignature {
                        pattern: "async fn {name}".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "result_wrap".into(),
                    priority: 20,
                    condition: SemanticCondition::CanFail,
                    transform: TransformTemplate::ReturnWrapper {
                        pattern: "Result<{T}, String>".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "rust_mut".into(),
                    priority: 30,
                    condition: SemanticCondition::IsMutable,
                    transform: TransformTemplate::MutationMarker {
                        pattern: "mut".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "int_type".into(),
                    priority: 40,
                    condition: SemanticCondition::TypeIs("Int".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "i32".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "float_type".into(),
                    priority: 41,
                    condition: SemanticCondition::TypeIs("Float".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "f64".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "bool_type".into(),
                    priority: 42,
                    condition: SemanticCondition::TypeIs("Bool".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "bool".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "string_type".into(),
                    priority: 43,
                    condition: SemanticCondition::TypeIs("String".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "String".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
            ],
            import_rules: vec![ImportRule {
                symbol: "Result".into(),
                import: "std::result::Result".into(),
            }],
        }
    }

    pub fn python_profile() -> LanguageProfile {
        LanguageProfile {
            name: "python".into(),
            type_system: TypeSystem {
                primitive_map: HashMap::from([
                    ("Int".into(), "int".into()),
                    ("Float".into(), "float".into()),
                    ("Bool".into(), "bool".into()),
                    ("String".into(), "str".into()),
                    ("Void".into(), "None".into()),
                ]),
                nullable_format: NullableFormat::Optional,
            },
            error_model: ErrorModel::Exception,
            concurrency: ConcurrencyModel::AsyncAwait,
            rules: vec![
                MappingRule {
                    id: "async_def".into(),
                    priority: 10,
                    condition: SemanticCondition::HasEffect(ProgramEffect::Async),
                    transform: TransformTemplate::FunctionSignature {
                        pattern: "async def {name}".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "python_int".into(),
                    priority: 40,
                    condition: SemanticCondition::TypeIs("Int".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "int".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "python_float".into(),
                    priority: 41,
                    condition: SemanticCondition::TypeIs("Float".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "float".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "python_bool".into(),
                    priority: 42,
                    condition: SemanticCondition::TypeIs("Bool".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "bool".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "python_string".into(),
                    priority: 43,
                    condition: SemanticCondition::TypeIs("String".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "str".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
            ],
            import_rules: vec![],
        }
    }

    pub fn ts_profile() -> LanguageProfile {
        LanguageProfile {
            name: "ts".into(),
            type_system: TypeSystem {
                primitive_map: HashMap::from([
                    ("Int".into(), "number".into()),
                    ("Float".into(), "number".into()),
                    ("Bool".into(), "boolean".into()),
                    ("String".into(), "string".into()),
                    ("Void".into(), "void".into()),
                ]),
                nullable_format: NullableFormat::UnionNull,
            },
            error_model: ErrorModel::Union,
            concurrency: ConcurrencyModel::AsyncAwait,
            rules: vec![
                MappingRule {
                    id: "async_function".into(),
                    priority: 10,
                    condition: SemanticCondition::HasEffect(ProgramEffect::Async),
                    transform: TransformTemplate::FunctionSignature {
                        pattern: "async function {name}".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "ts_error_union".into(),
                    priority: 20,
                    condition: SemanticCondition::CanFail,
                    transform: TransformTemplate::ReturnWrapper {
                        pattern: "{T} | Error".into(),
                    },
                    target: TargetScope::Function,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "ts_int".into(),
                    priority: 40,
                    condition: SemanticCondition::TypeIs("Int".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "number".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "ts_float".into(),
                    priority: 41,
                    condition: SemanticCondition::TypeIs("Float".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "number".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "ts_bool".into(),
                    priority: 42,
                    condition: SemanticCondition::TypeIs("Bool".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "boolean".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
                MappingRule {
                    id: "ts_string".into(),
                    priority: 43,
                    condition: SemanticCondition::TypeIs("String".into()),
                    transform: TransformTemplate::TypeMapping {
                        pattern: "string".into(),
                    },
                    target: TargetScope::Type,
                    source: RuleSource::Static,
                    confidence: 1.0,
                    usage_count: 0,
                },
            ],
            import_rules: vec![],
        }
    }

    pub fn resolve_profile(lang: &str) -> LanguageProfile {
        match lang {
            "rust" => rust_profile(),
            "python" => python_profile(),
            "ts" | "typescript" => ts_profile(),
            other => panic!("unsupported language: {other}"),
        }
    }

    pub fn lower_program_with_profile(
        program: &SemanticProgram,
        profile: &LanguageProfile,
    ) -> TargetProgram {
        DynamicInterpreter::new(profile.clone()).interpret_program(program, profile)
    }

    pub fn resolve_profile_from_memory(memory: &dyn MemoryEngine, lang: &str) -> LanguageProfile {
        let query = MemoryQuery {
            text: format!("language_profile:{lang}"),
            tags: vec![format!("profile:{lang}")],
            limit: 1,
        };
        let mut profile = resolve_profile(lang);
        if let Some(record) = memory.retrieve(query).into_iter().next() {
            apply_profile_record(&mut profile, &record);
        }
        profile
    }

    impl RuleEngine {
        pub fn new(rules: Vec<MappingRule>) -> Self {
            let mut rules = rules;
            rules.sort_by(|lhs, rhs| {
                effective_priority(lhs)
                    .partial_cmp(&effective_priority(rhs))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| lhs.id.cmp(&rhs.id))
            });
            Self { rules }
        }

        pub fn validate(&self) -> Result<(), RuleValidationError> {
            for (index, rule) in self.rules.iter().enumerate() {
                if let Some(conflict) = self.rules.iter().skip(index + 1).find(|other| {
                    other.priority == rule.priority
                        && other.target == rule.target
                        && other.condition == rule.condition
                }) {
                    return Err(RuleValidationError {
                        rule_id: rule.id.clone(),
                        message: format!("rule conflict with {}", conflict.id),
                    });
                }
            }
            Ok(())
        }

        pub fn transforms_for<'a>(
            &'a self,
            scope: TargetScope,
            ctx: &SemanticContext,
        ) -> Vec<&'a TransformTemplate> {
            self.rules
                .iter()
                .filter(|rule| rule.target == scope && rule.condition.matches(ctx))
                .map(|rule| &rule.transform)
                .collect()
        }
    }

    impl EvaluationResult {
        pub fn score(&self) -> f32 {
            let mut score = 0.0;
            if self.build_success {
                score += 0.4;
            }
            if self.test_pass {
                score += 0.4;
            }
            score += self.lint_score.clamp(0.0, 1.0) * 0.1;
            score += self.performance_score.clamp(0.0, 1.0) * 0.1;
            score.clamp(0.0, 1.0)
        }
    }

    impl LearningEngine {
        pub fn evaluate_rule(&self, record: &mut MemoryRuleRecord, evaluation: &EvaluationResult) {
            let score = evaluation.score();
            record.history.push(score);
            update_rule(&mut record.rule, score);
        }

        pub fn promote_candidates(&self, store: &mut RuleStore) {
            let mut promoted = Vec::new();
            let mut remaining = Vec::new();
            for record in store.candidate_rules.drain(..) {
                if should_promote(&record.rule) {
                    promoted.push(record);
                } else {
                    remaining.push(record);
                }
            }
            store.active_rules.extend(promoted);
            store.candidate_rules = remaining;
            store
                .active_rules
                .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
            store
                .candidate_rules
                .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        }

        pub fn deprecate_low_confidence(&self, store: &mut RuleStore, threshold: f32) {
            let mut kept = Vec::new();
            for record in store.active_rules.drain(..) {
                if record.rule.confidence < threshold {
                    store.deprecated_rules.push(record);
                } else {
                    kept.push(record);
                }
            }
            kept.sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
            store
                .deprecated_rules
                .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
            store.active_rules = kept;
        }
    }

    impl RuleValidator for DefaultRuleValidator {
        fn validate(&self, rule: &MappingRule, ctx: &ValidationContext) -> ValidationResult {
            let mut checks = Vec::new();
            if ctx.regression_pass {
                checks.push(ValidationCheck::RegressionPass);
            }
            if ctx.deterministic {
                checks.push(ValidationCheck::Deterministic);
            }
            if !has_rule_conflict(rule, &ctx.active_rules) {
                checks.push(ValidationCheck::NoConflict);
            }
            if ctx.diff_safe {
                checks.push(ValidationCheck::DiffSafe);
            }
            if ctx.cross_language_consistent {
                checks.push(ValidationCheck::CrossLanguageConsistent);
            }

            let score = checks.len() as f32 / 5.0;
            ValidationResult {
                passed: checks.len() == 5,
                score,
                checks,
            }
        }
    }

    pub fn update_rule(rule: &mut MappingRule, score: f32) {
        rule.usage_count += 1;
        let delta = score - 0.5;
        rule.confidence = (rule.confidence + delta * 0.1).clamp(0.0, 1.0);
    }

    pub fn should_promote(rule: &MappingRule) -> bool {
        rule.confidence > 0.8 && rule.usage_count > 10
    }

    pub fn should_promote_validated(rule: &MappingRule, validation: &ValidationResult) -> bool {
        validation.passed
            && validation.score > 0.85
            && rule.confidence > 0.8
            && rule.usage_count > 20
    }

    pub fn effective_priority(rule: &MappingRule) -> f32 {
        rule.priority as f32 - rule.confidence * 10.0
    }

    pub fn select_rule_records(
        store: &RuleStore,
        safe_mode: bool,
        exploration_key: &str,
        epsilon: f32,
    ) -> Vec<MemoryRuleRecord> {
        if safe_mode || epsilon <= 0.0 {
            return sorted_records(store.active_rules.clone());
        }

        let threshold = deterministic_exploration_value(exploration_key);
        if threshold < epsilon {
            let mut combined = store.active_rules.clone();
            combined.extend(store.candidate_rules.clone());
            sorted_records(combined)
        } else {
            sorted_records(store.active_rules.clone())
        }
    }

    fn sorted_records(mut records: Vec<MemoryRuleRecord>) -> Vec<MemoryRuleRecord> {
        records.sort_by(|lhs, rhs| {
            effective_priority(&lhs.rule)
                .partial_cmp(&effective_priority(&rhs.rule))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| lhs.rule.id.cmp(&rhs.rule.id))
        });
        records
    }

    fn deterministic_exploration_value(key: &str) -> f32 {
        let hash = key.bytes().fold(0_u64, |acc, byte| {
            acc.wrapping_mul(131).wrapping_add(u64::from(byte))
        });
        (hash % 10_000) as f32 / 10_000.0
    }

    pub fn bootstrap_rule_store(lang: &str) -> RuleStore {
        let profile = resolve_profile(lang);
        let active_rules = profile
            .rules
            .iter()
            .cloned()
            .map(|rule| MemoryRuleRecord {
                rule,
                history: Vec::new(),
            })
            .collect::<Vec<_>>();
        let candidate_rules = vec![candidate_rule_for(lang)]
            .into_iter()
            .flatten()
            .collect();
        RuleStore {
            active_rules,
            candidate_rules,
            validated_rules: Vec::new(),
            deprecated_rules: Vec::new(),
        }
    }

    pub fn validate_candidate_rule(
        store: &RuleStore,
        rule_id: &str,
        validator: &dyn RuleValidator,
    ) -> Option<ValidatedRuleRecord> {
        let record = store
            .candidate_rules
            .iter()
            .find(|record| record.rule.id == rule_id)?;
        let ctx = ValidationContext {
            active_rules: store
                .active_rules
                .iter()
                .map(|record| record.rule.clone())
                .collect(),
            regression_pass: true,
            deterministic: true,
            diff_safe: true,
            cross_language_consistent: true,
        };
        let validation = validator.validate(&record.rule, &ctx);
        Some(ValidatedRuleRecord {
            rule: record.rule.clone(),
            validation_score: validation.score,
            passed_checks: validation.checks,
        })
    }

    pub fn validate_all_candidates(
        store: &RuleStore,
        validator: &dyn RuleValidator,
    ) -> Vec<ValidatedRuleRecord> {
        let mut validated = store
            .candidate_rules
            .iter()
            .filter_map(|record| validate_candidate_rule(store, &record.rule.id, validator))
            .collect::<Vec<_>>();
        validated.sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        validated
    }

    pub fn promote_validated_rule(store: &mut RuleStore, rule_id: &str) -> bool {
        let Some(index) = store
            .validated_rules
            .iter()
            .position(|record| record.rule.id == rule_id)
        else {
            return false;
        };
        let validated = store.validated_rules.remove(index);
        let rule_id = validated.rule.id.clone();
        store
            .candidate_rules
            .retain(|record| record.rule.id != rule_id);
        store.active_rules.push(MemoryRuleRecord {
            rule: validated.rule,
            history: vec![validated.validation_score],
        });
        store
            .active_rules
            .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        true
    }

    pub fn rollback_rule(store: &mut RuleStore, rule_id: &str) -> bool {
        let Some(index) = store
            .active_rules
            .iter()
            .position(|record| record.rule.id == rule_id)
        else {
            return false;
        };
        let record = store.active_rules.remove(index);
        store.deprecated_rules.push(record);
        store
            .deprecated_rules
            .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        true
    }

    pub fn prune_rules(store: &mut RuleStore, usage_threshold: u32) {
        let mut kept = Vec::new();
        for record in store.candidate_rules.drain(..) {
            if record.rule.confidence < 0.3 && record.rule.usage_count > usage_threshold {
                store.deprecated_rules.push(record);
            } else {
                kept.push(record);
            }
        }
        kept.sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        store
            .deprecated_rules
            .sort_by(|lhs, rhs| lhs.rule.id.cmp(&rhs.rule.id));
        store.candidate_rules = kept;
    }

    fn has_rule_conflict(rule: &MappingRule, active_rules: &[MappingRule]) -> bool {
        active_rules.iter().any(|existing| {
            existing.priority == rule.priority
                && existing.target == rule.target
                && existing.condition == rule.condition
        })
    }

    fn candidate_rule_for(lang: &str) -> Option<MemoryRuleRecord> {
        let (id, pattern) = match lang {
            "rust" => ("candidate_rust_bytes", "Vec<u8>"),
            "python" => ("candidate_python_bytes", "bytes"),
            "ts" | "typescript" => ("candidate_ts_bytes", "Uint8Array"),
            _ => return None,
        };
        Some(MemoryRuleRecord {
            rule: MappingRule {
                id: id.to_string(),
                priority: 90,
                condition: SemanticCondition::TypeIs("Bytes".to_string()),
                transform: TransformTemplate::TypeMapping {
                    pattern: pattern.to_string(),
                },
                target: TargetScope::Type,
                source: RuleSource::Learned,
                confidence: 0.86,
                usage_count: 21,
            },
            history: vec![0.88, 0.9],
        })
    }

    impl SemanticCondition {
        pub fn matches(&self, ctx: &SemanticContext) -> bool {
            match self {
                SemanticCondition::HasEffect(effect) => ctx.effects.contains(effect),
                SemanticCondition::CanFail => ctx.can_fail,
                SemanticCondition::TypeIs(name) => ctx.type_name.as_deref() == Some(name.as_str()),
                SemanticCondition::IsAsync => ctx.effects.contains(&ProgramEffect::Async),
                SemanticCondition::IsMutable => ctx.is_mutable,
                SemanticCondition::Always => true,
            }
        }
    }

    impl SemanticContext {
        pub fn from_function(func: &SemanticFunction) -> Self {
            Self {
                function_name: Some(func.name.clone()),
                type_name: Some(func.outputs.r#type.name.clone()),
                output_type: Some(func.outputs.r#type.name.clone()),
                effects: func.effects.clone(),
                can_fail: func.can_fail,
                is_mutable: func.effects.contains(&ProgramEffect::Mutation)
                    || func.inputs.iter().any(|input| {
                        input
                            .borrow
                            .as_ref()
                            .map(|borrow| borrow.is_mut)
                            .unwrap_or(false)
                    }),
            }
        }

        pub fn from_type_ref(ty: &ProgramTypeRef) -> Self {
            Self {
                function_name: None,
                type_name: Some(ty.name.clone()),
                output_type: None,
                effects: Vec::new(),
                can_fail: false,
                is_mutable: false,
            }
        }
    }

    fn apply_effect_rules(
        engine: &RuleEngine,
        ctx: &SemanticContext,
        profile: &LanguageProfile,
    ) -> TargetEffects {
        let transforms = engine.transforms_for(TargetScope::Function, ctx);
        let has_async_signature = transforms.iter().any(|transform| {
            matches!(
                transform,
                TransformTemplate::FunctionSignature { pattern }
                    if pattern.contains(&profile.name) || pattern.contains("async")
            )
        });
        TargetEffects {
            is_async: has_async_signature || ctx.effects.contains(&ProgramEffect::Async),
            is_mut: transforms
                .iter()
                .any(|transform| matches!(transform, TransformTemplate::MutationMarker { .. })),
            returns_promise: (profile.name == "ts" || profile.name == "typescript")
                && ctx.effects.contains(&ProgramEffect::Async),
            can_fail: ctx.can_fail,
            is_blocking: ctx.effects.contains(&ProgramEffect::Blocking),
        }
    }

    fn lower_input_type(
        input: &ProgramFunctionInput,
        ctx: &SemanticContext,
        engine: &RuleEngine,
        profile: &LanguageProfile,
    ) -> String {
        let base = lower_type_ref(&input.r#type, profile);
        let mutability = ctx.is_mutable
            || input
                .borrow
                .as_ref()
                .map(|borrow| borrow.is_mut)
                .unwrap_or(false);
        let transforms = if mutability {
            engine.transforms_for(
                TargetScope::Function,
                &SemanticContext {
                    is_mutable: true,
                    ..ctx.clone()
                },
            )
        } else {
            Vec::new()
        };
        if profile.name == "rust"
            && let Some(TransformTemplate::MutationMarker { pattern }) = transforms
                .into_iter()
                .find(|transform| matches!(transform, TransformTemplate::MutationMarker { .. }))
        {
            return format!("&{} {}", pattern, base);
        }
        base
    }

    fn lower_type_ref(ty: &ProgramTypeRef, profile: &LanguageProfile) -> String {
        let ctx = SemanticContext::from_type_ref(ty);
        let base = profile
            .rules
            .iter()
            .filter(|rule| rule.target == TargetScope::Type && rule.condition.matches(&ctx))
            .find_map(|rule| match &rule.transform {
                TransformTemplate::TypeMapping { pattern } => Some(pattern.clone()),
                _ => None,
            })
            .or_else(|| profile.type_system.primitive_map.get(&ty.name).cloned())
            .unwrap_or_else(|| ty.name.clone());
        let with_generics = if ty.generics.is_empty() {
            base
        } else {
            format!(
                "{}<{}>",
                base,
                ty.generics
                    .iter()
                    .map(|inner| lower_type_ref(inner, profile))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        if !ty.nullable {
            return with_generics;
        }
        match profile.type_system.nullable_format {
            NullableFormat::Option => format!("Option<{with_generics}>"),
            NullableFormat::Optional => format!("Optional[{with_generics}]"),
            NullableFormat::UnionNull => format!("{with_generics} | null"),
        }
    }

    fn lower_function_output(
        output: &ProgramFunctionOutput,
        ctx: &SemanticContext,
        engine: &RuleEngine,
        profile: &LanguageProfile,
    ) -> String {
        let mut base = lower_type_ref(&output.r#type, profile);
        if let Some(TransformTemplate::ReturnWrapper { pattern }) = engine
            .transforms_for(TargetScope::Function, ctx)
            .into_iter()
            .find(|transform| matches!(transform, TransformTemplate::ReturnWrapper { .. }))
        {
            base = pattern.replace("{T}", &base);
        }
        if ctx.effects.contains(&ProgramEffect::Async)
            && matches!(profile.concurrency, ConcurrencyModel::AsyncAwait)
            && (profile.name == "ts" || profile.name == "typescript")
        {
            format!("Promise<{base}>")
        } else {
            base
        }
    }

    fn apply_profile_record(profile: &mut LanguageProfile, record: &MemoryRecord) {
        for relation in &record.relations {
            if let Some((name, mapped)) = relation
                .strip_prefix("primitive:")
                .and_then(|value| value.split_once('='))
            {
                profile
                    .type_system
                    .primitive_map
                    .insert(name.to_string(), mapped.to_string());
                continue;
            }
            if let Some(value) = relation.strip_prefix("nullable:") {
                profile.type_system.nullable_format = match value {
                    "Option" => NullableFormat::Option,
                    "Optional" => NullableFormat::Optional,
                    "UnionNull" => NullableFormat::UnionNull,
                    _ => profile.type_system.nullable_format.clone(),
                };
                continue;
            }
            if let Some(value) = relation.strip_prefix("error:") {
                profile.error_model = match value {
                    "Result" => ErrorModel::Result,
                    "Exception" => ErrorModel::Exception,
                    "Union" => ErrorModel::Union,
                    _ => profile.error_model.clone(),
                };
                continue;
            }
            if let Some(value) = relation.strip_prefix("concurrency:") {
                profile.concurrency = match value {
                    "AsyncAwait" => ConcurrencyModel::AsyncAwait,
                    "Thread" => ConcurrencyModel::Thread,
                    "None" => ConcurrencyModel::None,
                    _ => profile.concurrency.clone(),
                };
                continue;
            }
            if let Some(value) = relation.strip_prefix("import:") {
                if let Some((symbol, import)) = value.split_once('=') {
                    profile.import_rules.push(ImportRule {
                        symbol: symbol.to_string(),
                        import: import.to_string(),
                    });
                }
                continue;
            }
            if let Some(value) = relation.strip_prefix("rule:")
                && let Some(rule) = parse_rule(value)
            {
                profile.rules.push(rule);
            }
        }
        profile.rules.sort_by(|lhs, rhs| {
            lhs.priority
                .cmp(&rhs.priority)
                .then_with(|| lhs.id.cmp(&rhs.id))
        });
        profile.import_rules.sort_by(|lhs, rhs| {
            lhs.symbol
                .cmp(&rhs.symbol)
                .then_with(|| lhs.import.cmp(&rhs.import))
        });
    }

    fn parse_rule(value: &str) -> Option<MappingRule> {
        let mut parts = value.split('|');
        let id = parts.next()?.to_string();
        let priority = parts.next()?.parse().ok()?;
        let condition = parse_condition(parts.next()?)?;
        let target = parse_target_scope(parts.next()?)?;
        let transform = parse_transform(parts.next()?)?;
        Some(MappingRule {
            id,
            priority,
            condition,
            transform,
            target,
            source: RuleSource::Learned,
            confidence: 0.5,
            usage_count: 0,
        })
    }

    fn parse_condition(value: &str) -> Option<SemanticCondition> {
        match value {
            "CanFail" => Some(SemanticCondition::CanFail),
            "IsAsync" => Some(SemanticCondition::IsAsync),
            "IsMutable" => Some(SemanticCondition::IsMutable),
            "Always" => Some(SemanticCondition::Always),
            _ => {
                if let Some(effect) = value.strip_prefix("HasEffect:") {
                    return match effect {
                        "Pure" => Some(SemanticCondition::HasEffect(ProgramEffect::Pure)),
                        "IO" => Some(SemanticCondition::HasEffect(ProgramEffect::IO)),
                        "Mutation" => Some(SemanticCondition::HasEffect(ProgramEffect::Mutation)),
                        "Async" => Some(SemanticCondition::HasEffect(ProgramEffect::Async)),
                        "Error" => Some(SemanticCondition::HasEffect(ProgramEffect::Error)),
                        "Blocking" => Some(SemanticCondition::HasEffect(ProgramEffect::Blocking)),
                        _ => None,
                    };
                }
                value
                    .strip_prefix("TypeIs:")
                    .map(|name| SemanticCondition::TypeIs(name.to_string()))
            }
        }
    }

    fn parse_target_scope(value: &str) -> Option<TargetScope> {
        match value {
            "Function" => Some(TargetScope::Function),
            "Type" => Some(TargetScope::Type),
            "Expression" => Some(TargetScope::Expression),
            "Module" => Some(TargetScope::Module),
            _ => None,
        }
    }

    fn parse_transform(value: &str) -> Option<TransformTemplate> {
        if let Some(pattern) = value.strip_prefix("FunctionSignature:") {
            return Some(TransformTemplate::FunctionSignature {
                pattern: pattern.to_string(),
            });
        }
        if let Some(pattern) = value.strip_prefix("ReturnWrapper:") {
            return Some(TransformTemplate::ReturnWrapper {
                pattern: pattern.to_string(),
            });
        }
        if let Some(pattern) = value.strip_prefix("TypeMapping:") {
            return Some(TransformTemplate::TypeMapping {
                pattern: pattern.to_string(),
            });
        }
        value
            .strip_prefix("MutationMarker:")
            .map(|pattern| TransformTemplate::MutationMarker {
                pattern: pattern.to_string(),
            })
    }

    fn backend_flavor(profile: &LanguageProfile) -> BackendFlavor {
        match profile.name.as_str() {
            "rust" => BackendFlavor::Rust,
            "python" => BackendFlavor::Python,
            "ts" | "typescript" => BackendFlavor::TypeScript,
            _ => BackendFlavor::Rust,
        }
    }

    fn lower_statement_code_with_profile<I: LanguageInterpreter>(
        interpreter: &I,
        stmt: &SemanticStatement,
        profile: &LanguageProfile,
    ) -> String {
        match stmt {
            ProgramStatement::Assign { target, value } => {
                format!(
                    "{target} = {}",
                    interpreter.interpret_expression(value, profile).code
                )
            }
            ProgramStatement::If(branch) => {
                let condition = interpreter
                    .interpret_expression(&branch.condition, profile)
                    .code;
                let then_code = render_nested_block(
                    &branch
                        .then_block
                        .statements
                        .iter()
                        .map(|statement| interpreter.interpret_statement(statement, profile))
                        .collect::<Vec<_>>(),
                    backend_flavor(profile),
                );
                let else_code = render_nested_block(
                    &branch
                        .else_block
                        .statements
                        .iter()
                        .map(|statement| interpreter.interpret_statement(statement, profile))
                        .collect::<Vec<_>>(),
                    backend_flavor(profile),
                );
                match backend_flavor(profile) {
                    BackendFlavor::Rust | BackendFlavor::TypeScript => {
                        format!("if {condition} {{\n{then_code}\n}} else {{\n{else_code}\n}}")
                    }
                    BackendFlavor::Python => {
                        format!("if {condition}:\n{then_code}\nelse:\n{else_code}")
                    }
                }
            }
            ProgramStatement::Loop(loop_stmt) => {
                let iterator = interpreter
                    .interpret_expression(&loop_stmt.iterator, profile)
                    .code;
                let body = render_nested_block(
                    &loop_stmt
                        .body
                        .statements
                        .iter()
                        .map(|statement| interpreter.interpret_statement(statement, profile))
                        .collect::<Vec<_>>(),
                    backend_flavor(profile),
                );
                match backend_flavor(profile) {
                    BackendFlavor::Rust => format!("while {iterator} {{\n{body}\n}}"),
                    BackendFlavor::Python => format!("while {iterator}:\n{body}"),
                    BackendFlavor::TypeScript => format!("while ({iterator}) {{\n{body}\n}}"),
                }
            }
            ProgramStatement::Return { value } => match value {
                Some(value) => format!(
                    "return {}",
                    interpreter.interpret_expression(value, profile).code
                ),
                None => "return".to_string(),
            },
            ProgramStatement::Expression(expr) => {
                interpreter.interpret_expression(expr, profile).code
            }
        }
    }

    fn state_binding(state: &ProgramState, profile: &LanguageProfile) -> String {
        match profile.name.as_str() {
            "rust" => match state.scope {
                code_ir::program_v1::StateScope::Global => match state.kind {
                    ProgramStateKind::Mutable => "static mut".to_string(),
                    ProgramStateKind::Immutable | ProgramStateKind::Shared => "static".to_string(),
                },
                code_ir::program_v1::StateScope::Module
                | code_ir::program_v1::StateScope::Local => match state.kind {
                    ProgramStateKind::Mutable => "let mut".to_string(),
                    ProgramStateKind::Immutable | ProgramStateKind::Shared => "let".to_string(),
                },
            },
            "python" => "global".to_string(),
            "ts" | "typescript" => match state.kind {
                ProgramStateKind::Mutable => "let".to_string(),
                ProgramStateKind::Immutable | ProgramStateKind::Shared => "const".to_string(),
            },
            _ => "let".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

pub trait Validator {
    fn validate_program(&self, program: &Program) -> ValidationResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultSemanticValidator;

#[derive(Clone, Debug, Default)]
pub struct ImportResolver;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResolvedImports {
    pub modules: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeGenerationOutput {
    pub files: Vec<GeneratedFile>,
    pub target: TargetProgram,
    pub imports: ResolvedImports,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SafeGenerationError {
    Validation(ValidationResult),
    Build(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum TargetLanguage {
    Rust,
    Python,
    TypeScript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingRules {
    pub module_case: CaseStyle,
    pub type_case: CaseStyle,
    pub method_case: CaseStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseStyle {
    SnakeCase,
    PascalCase,
    CamelCase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRules {
    pub module_prefix: String,
    pub use_file_extensions: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLayoutRules {
    pub source_dir: String,
    pub extension: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageProfile {
    pub language: TargetLanguage,
    pub naming_rules: NamingRules,
    pub import_rules: ImportRules,
    pub file_layout_rules: FileLayoutRules,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameworkProfile {
    pub name: String,
    pub project_layout: ProjectLayoutPolicy,
    pub dependency_overrides: Vec<DependencySpec>,
    pub interface_conventions: InterfaceConvention,
    pub test_conventions: TestConvention,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationContext {
    pub language_profile: LanguageProfile,
    pub framework_profile: Option<FrameworkProfile>,
    pub dependency_policy: DependencyPolicy,
    pub template_policy: TemplatePolicy,
    pub test_policy: TestPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyPolicy {
    pub defaults: Vec<DependencySpec>,
    pub optional: Vec<DependencySpec>,
    pub framework_bound: Vec<DependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePolicy {
    pub entrypoint_template: String,
    pub module_template_family: String,
    pub test_template_family: String,
    pub project_layout_policy: ProjectLayoutPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLayoutPolicy {
    CargoBinaryLib,
    PythonPackage,
    TypeScriptService,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestPolicy {
    pub enabled: bool,
    pub style: TestStyle,
    pub conventions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestStyle {
    Native,
    Pytest,
    Jest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceConvention {
    pub trait_prefix: String,
    pub method_prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestConvention {
    pub file_suffix: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationMemoryQuery {
    pub module_name: String,
    pub interface_names: Vec<String>,
    pub struct_names: Vec<String>,
    pub language_hint: Option<TargetLanguage>,
    pub annotations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationMemoryResult {
    pub candidate_profiles: Vec<ProfileCandidate>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileCandidate {
    pub language: TargetLanguage,
    pub framework: Option<String>,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedCodeModule {
    pub name: String,
    pub path: String,
    pub imports: Vec<String>,
    pub interfaces: Vec<SpecializedCodeInterface>,
    pub structs: Vec<SpecializedCodeStruct>,
    pub functions: Vec<SpecializedCodeFunction>,
    pub dependencies: Vec<DependencySpec>,
    pub context: GenerationContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedCodeInterface {
    pub name: String,
    pub methods: Vec<SpecializedCodeSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedCodeSignature {
    pub name: String,
    pub inputs: Vec<String>,
    pub output: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedCodeStruct {
    pub name: String,
    pub fields: Vec<SpecializedField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedField {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecializedCodeFunction {
    pub name: String,
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub body: Vec<String>,
}

pub trait CodeIRBuilder: Send + Sync {
    fn build(&self, units: Vec<ImplementationUnit>) -> Vec<CodeModule>;
}

pub trait CodeGenerator: Send + Sync {
    fn generate(&self, modules: Vec<CodeModule>) -> Vec<GeneratedFile>;
}

pub trait TypeMapper: Send + Sync {
    fn map_type(&self, ty: &TypeRef) -> String;
}

pub trait ProfileResolver: Send + Sync {
    fn resolve(&self, unit: &ImplementationUnit, memory: &dyn MemoryEngine) -> GenerationContext;
}

pub trait ContextualCodeIRBuilder: Send + Sync {
    fn build_with_context(
        &self,
        units: Vec<(ImplementationUnit, GenerationContext)>,
    ) -> Vec<SpecializedCodeModule>;
}

pub trait SpecializedCodeGenerator: Send + Sync {
    fn generate(&self, modules: Vec<SpecializedCodeModule>) -> Vec<GeneratedFile>;
}

pub trait GeneratorRegistry: Send + Sync {
    fn get_generator(&self, ctx: &GenerationContext) -> Arc<dyn SpecializedCodeGenerator>;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultCodeIRBuilder;

#[derive(Clone, Debug, Default)]
pub struct DefaultProfileResolver;

#[derive(Clone, Debug, Default)]
pub struct DefaultContextualCodeIRBuilder;

#[derive(Clone, Debug, Default)]
pub struct DefaultGeneratorRegistry;

#[derive(Clone, Debug, Default)]
pub struct RustGenerator;

#[derive(Clone, Debug, Default)]
pub struct PythonGenerator;

#[derive(Clone, Debug, Default)]
pub struct TypeScriptGenerator;

#[derive(Clone, Debug, Default)]
pub struct RustProjectGenerator;

#[derive(Clone, Debug, Default)]
pub struct PythonProjectGenerator;

#[derive(Clone, Debug, Default)]
pub struct TypeScriptProjectGenerator;

#[derive(Clone, Debug, Default)]
pub struct RustTypeMapper;

#[derive(Clone, Debug, Default)]
pub struct PythonTypeMapper;

#[derive(Clone, Debug, Default)]
pub struct TypeScriptTypeMapper;

#[derive(Clone, Debug, Default)]
pub struct RustBackend;

#[derive(Clone, Debug, Default)]
pub struct PythonBackend;

#[derive(Clone, Debug, Default)]
pub struct TypeScriptBackend;

#[derive(Clone, Debug, Default)]
pub struct RustRenderer;

#[derive(Clone, Debug, Default)]
pub struct PythonRenderer;

#[derive(Clone, Debug, Default)]
pub struct TypeScriptRenderer;

impl CodeIRBuilder for DefaultCodeIRBuilder {
    fn build(&self, units: Vec<ImplementationUnit>) -> Vec<CodeModule> {
        units.into_iter().map(build_module).collect()
    }
}

impl ProfileResolver for DefaultProfileResolver {
    fn resolve(&self, unit: &ImplementationUnit, memory: &dyn MemoryEngine) -> GenerationContext {
        let query = GenerationMemoryQuery {
            module_name: unit.module_name.clone(),
            interface_names: unit
                .public_interfaces
                .iter()
                .map(|interface| interface.name.clone())
                .collect(),
            struct_names: unit
                .internal_structs
                .iter()
                .map(|structure| structure.name.clone())
                .collect(),
            language_hint: unit
                .language_hint
                .as_ref()
                .and_then(|hint| parse_language_token(hint)),
            annotations: unit.annotations.clone(),
        };
        let recalled = recall_generation_profiles(memory, &query);
        let selected = recalled.candidate_profiles.into_iter().max_by(|lhs, rhs| {
            lhs.score
                .total_cmp(&rhs.score)
                .then_with(|| lhs.language.cmp(&rhs.language))
                .then_with(|| lhs.framework.cmp(&rhs.framework))
        });
        generation_context_for(selected, query.language_hint)
    }
}

impl ContextualCodeIRBuilder for DefaultContextualCodeIRBuilder {
    fn build_with_context(
        &self,
        units: Vec<(ImplementationUnit, GenerationContext)>,
    ) -> Vec<SpecializedCodeModule> {
        units
            .into_iter()
            .map(|(unit, context)| build_specialized_module(unit, context))
            .collect()
    }
}

impl GeneratorRegistry for DefaultGeneratorRegistry {
    fn get_generator(&self, ctx: &GenerationContext) -> Arc<dyn SpecializedCodeGenerator> {
        match ctx.language_profile.language {
            TargetLanguage::Rust => Arc::new(RustProjectGenerator),
            TargetLanguage::Python => Arc::new(PythonProjectGenerator),
            TargetLanguage::TypeScript => Arc::new(TypeScriptProjectGenerator),
        }
    }
}

impl CodeGenerator for RustGenerator {
    fn generate(&self, modules: Vec<CodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: format!("{}.rs", module.name),
                content: render_rust(&module),
            })
            .collect()
    }
}

impl CodeGenerator for PythonGenerator {
    fn generate(&self, modules: Vec<CodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: format!("{}.py", module.name),
                content: render_python(&module),
            })
            .collect()
    }
}

impl CodeGenerator for TypeScriptGenerator {
    fn generate(&self, modules: Vec<CodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: format!("{}.ts", module.name),
                content: render_typescript(&module),
            })
            .collect()
    }
}

impl SpecializedCodeGenerator for RustProjectGenerator {
    fn generate(&self, modules: Vec<SpecializedCodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: module.path.clone(),
                content: render_specialized_rust(&module),
            })
            .collect()
    }
}

impl SpecializedCodeGenerator for PythonProjectGenerator {
    fn generate(&self, modules: Vec<SpecializedCodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: module.path.clone(),
                content: render_specialized_python(&module),
            })
            .collect()
    }
}

impl SpecializedCodeGenerator for TypeScriptProjectGenerator {
    fn generate(&self, modules: Vec<SpecializedCodeModule>) -> Vec<GeneratedFile> {
        modules
            .into_iter()
            .map(|module| GeneratedFile {
                path: module.path.clone(),
                content: render_specialized_typescript(&module),
            })
            .collect()
    }
}

impl TypeMapper for RustTypeMapper {
    fn map_type(&self, ty: &TypeRef) -> String {
        map_rust_type(ty)
    }
}

impl TypeMapper for PythonTypeMapper {
    fn map_type(&self, ty: &TypeRef) -> String {
        map_python_type(ty)
    }
}

impl TypeMapper for TypeScriptTypeMapper {
    fn map_type(&self, ty: &TypeRef) -> String {
        map_typescript_type(ty)
    }
}

impl LanguageBackend for RustBackend {
    fn lower_program(&self, program: &Program) -> TargetProgram {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_program(program, &dynamic_ir::rust_profile())
    }

    fn lower_module(&self, module: &ProgramModule) -> TargetModule {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_module(module, &dynamic_ir::rust_profile())
    }

    fn lower_type(&self, ty: &ProgramType) -> TargetType {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_type(ty, &dynamic_ir::rust_profile())
    }

    fn lower_function(&self, func: &ProgramFunction) -> TargetFunction {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_function(func, &dynamic_ir::rust_profile())
    }

    fn lower_expression(&self, expr: &ProgramExpression) -> TargetExpr {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_expression(expr, &dynamic_ir::rust_profile())
    }

    fn lower_statement(&self, stmt: &ProgramStatement) -> TargetStmt {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_statement(stmt, &dynamic_ir::rust_profile())
    }

    fn lower_effects(&self, effects: &[ProgramEffect]) -> TargetEffects {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::rust_profile())
            .interpret_effects(effects, &dynamic_ir::rust_profile())
    }
}

impl LanguageBackend for PythonBackend {
    fn lower_program(&self, program: &Program) -> TargetProgram {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_program(program, &dynamic_ir::python_profile())
    }

    fn lower_module(&self, module: &ProgramModule) -> TargetModule {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_module(module, &dynamic_ir::python_profile())
    }

    fn lower_type(&self, ty: &ProgramType) -> TargetType {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_type(ty, &dynamic_ir::python_profile())
    }

    fn lower_function(&self, func: &ProgramFunction) -> TargetFunction {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_function(func, &dynamic_ir::python_profile())
    }

    fn lower_expression(&self, expr: &ProgramExpression) -> TargetExpr {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_expression(expr, &dynamic_ir::python_profile())
    }

    fn lower_statement(&self, stmt: &ProgramStatement) -> TargetStmt {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_statement(stmt, &dynamic_ir::python_profile())
    }

    fn lower_effects(&self, effects: &[ProgramEffect]) -> TargetEffects {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::python_profile())
            .interpret_effects(effects, &dynamic_ir::python_profile())
    }
}

impl LanguageBackend for TypeScriptBackend {
    fn lower_program(&self, program: &Program) -> TargetProgram {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_program(program, &dynamic_ir::ts_profile())
    }

    fn lower_module(&self, module: &ProgramModule) -> TargetModule {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_module(module, &dynamic_ir::ts_profile())
    }

    fn lower_type(&self, ty: &ProgramType) -> TargetType {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_type(ty, &dynamic_ir::ts_profile())
    }

    fn lower_function(&self, func: &ProgramFunction) -> TargetFunction {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_function(func, &dynamic_ir::ts_profile())
    }

    fn lower_expression(&self, expr: &ProgramExpression) -> TargetExpr {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_expression(expr, &dynamic_ir::ts_profile())
    }

    fn lower_statement(&self, stmt: &ProgramStatement) -> TargetStmt {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_statement(stmt, &dynamic_ir::ts_profile())
    }

    fn lower_effects(&self, effects: &[ProgramEffect]) -> TargetEffects {
        dynamic_ir::DynamicInterpreter::new(dynamic_ir::ts_profile())
            .interpret_effects(effects, &dynamic_ir::ts_profile())
    }
}

impl Renderer for RustRenderer {
    fn render_program(&self, program: &TargetProgram) -> String {
        render_target_program(program, BackendFlavor::Rust)
    }
}

impl Renderer for PythonRenderer {
    fn render_program(&self, program: &TargetProgram) -> String {
        render_target_program(program, BackendFlavor::Python)
    }
}

impl Renderer for TypeScriptRenderer {
    fn render_program(&self, program: &TargetProgram) -> String {
        render_target_program(program, BackendFlavor::TypeScript)
    }
}

impl Validator for DefaultSemanticValidator {
    fn validate_program(&self, program: &Program) -> ValidationResult {
        let mut result = ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        let known_types = collect_known_types(program);
        let async_functions = collect_async_functions(program);

        for module in &program.modules {
            for ty in &module.types {
                for field in &ty.fields {
                    validate_type_ref(
                        &field.r#type,
                        &known_types,
                        &mut result,
                        format!("type field {}.{}", ty.name, field.name),
                    );
                }
            }
            for state in &module.state {
                validate_type_ref(
                    &state.r#type,
                    &known_types,
                    &mut result,
                    format!("state {}", state.name),
                );
                if state.kind == ProgramStateKind::Mutable
                    && matches!(state.scope, code_ir::program_v1::StateScope::Global)
                {
                    push_error(
                        &mut result,
                        "mutable_global_state",
                        format!("mutable global state `{}` is not allowed", state.name),
                    );
                }
            }
            for function in &module.functions {
                for input in &function.inputs {
                    validate_type_ref(
                        &input.r#type,
                        &known_types,
                        &mut result,
                        format!("function {} input {}", function.name, input.name),
                    );
                }
                validate_type_ref(
                    &function.outputs.r#type,
                    &known_types,
                    &mut result,
                    format!("function {} output", function.name),
                );

                if function.effects.contains(&ProgramEffect::Error) && !function.can_fail {
                    push_error(
                        &mut result,
                        "error_effect_requires_can_fail",
                        format!(
                            "function `{}` has Error effect but can_fail=false",
                            function.name
                        ),
                    );
                }

                let mutable_borrows = function
                    .inputs
                    .iter()
                    .filter(|input| {
                        input
                            .borrow
                            .as_ref()
                            .map(|borrow| borrow.is_mut)
                            .unwrap_or(false)
                    })
                    .count();
                if mutable_borrows > 1 {
                    push_error(
                        &mut result,
                        "borrow_conflict",
                        format!("function `{}` has multiple mutable borrows", function.name),
                    );
                }

                validate_async_usage(function, &async_functions, &mut result);
            }
        }

        result.is_valid = result.errors.is_empty();
        result
    }
}

impl ImportResolver {
    pub fn resolve(&self, program: &Program) -> ResolvedImports {
        let mut modules = BTreeMap::new();
        let module_names = program
            .modules
            .iter()
            .map(|module| module.name.clone())
            .collect::<BTreeSet<_>>();

        for module in &program.modules {
            let mut imports = module.imports.clone();
            for function in &module.functions {
                collect_imports_from_type_ref(
                    &function.outputs.r#type,
                    &module_names,
                    &mut imports,
                );
                for input in &function.inputs {
                    collect_imports_from_type_ref(&input.r#type, &module_names, &mut imports);
                }
                collect_imports_from_block(&function.body, &module_names, &mut imports);
                if function.effects.contains(&ProgramEffect::Error) && function.can_fail {
                    imports.push(match program.build_validation.command.as_str() {
                        command if command.contains("cargo") => "std::result::Result".to_string(),
                        _ => "runtime_error".to_string(),
                    });
                }
            }
            for state in &module.state {
                collect_imports_from_type_ref(&state.r#type, &module_names, &mut imports);
                if state.kind == ProgramStateKind::Shared {
                    imports.push("std::sync::Arc".to_string());
                }
            }
            imports.sort();
            imports.dedup();
            modules.insert(module.name.clone(), imports);
        }

        ResolvedImports { modules }
    }
}

pub fn safe_generate_program<B: LanguageBackend, R: Renderer>(
    program: &Program,
    backend: &B,
    renderer: &R,
    validator: &dyn Validator,
) -> Result<SafeGenerationOutput, SafeGenerationError> {
    let validation = validator.validate_program(program);
    if !validation.is_valid {
        return Err(SafeGenerationError::Validation(validation));
    }

    let imports = ImportResolver.resolve(program);
    let target = backend.lower_program(program);
    let mut files = target
        .modules
        .iter()
        .map(|module| GeneratedFile {
            path: format!(
                "{}.{}",
                module.name,
                extension_for_backend(backend_name(backend))
            ),
            content: render_target_module(module, backend_name(backend)),
        })
        .collect::<Vec<_>>();
    files.sort_by(|lhs, rhs| lhs.path.cmp(&rhs.path));

    if program.build_validation.enabled {
        run_safe_build_validation(program, &files, backend_name(backend))
            .map_err(SafeGenerationError::Build)?;
    }

    let _ = renderer.render_program(&target);
    Ok(SafeGenerationOutput {
        files,
        target,
        imports,
    })
}

pub fn recall_generation_profiles(
    memory: &dyn MemoryEngine,
    query: &GenerationMemoryQuery,
) -> GenerationMemoryResult {
    let mut tags = query.annotations.clone();
    if let Some(language_hint) = query.language_hint {
        tags.push(format!("lang:{}", language_name(language_hint)));
    }
    let records = memory.retrieve(MemoryQuery {
        text: format!(
            "{} {} {}",
            query.module_name,
            query.interface_names.join(" "),
            query.struct_names.join(" ")
        ),
        tags,
        limit: 8,
    });
    let mut candidates = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| profile_candidate_from_record(record, index))
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| lhs.language.cmp(&rhs.language))
            .then_with(|| lhs.framework.cmp(&rhs.framework))
    });
    candidates.dedup_by(|lhs, rhs| lhs.language == rhs.language && lhs.framework == rhs.framework);
    let confidence = if candidates.is_empty() {
        0.0
    } else {
        candidates
            .iter()
            .map(|candidate| candidate.score)
            .sum::<f64>()
            / candidates.len() as f64
    };
    GenerationMemoryResult {
        candidate_profiles: candidates,
        confidence,
    }
}

pub fn validate_generation_context(ctx: &GenerationContext, unit: &ImplementationUnit) -> bool {
    framework_compatible(ctx)
        && !ctx.template_policy.entrypoint_template.is_empty()
        && unit
            .public_interfaces
            .iter()
            .flat_map(|interface| interface.methods.iter())
            .all(|method| {
                method
                    .inputs
                    .iter()
                    .all(|ty| !map_type_with_context(ty, ctx).is_empty())
                    && method
                        .output
                        .as_ref()
                        .map(|ty| !map_type_with_context(ty, ctx).is_empty())
                        .unwrap_or(true)
            })
}

fn build_module(unit: ImplementationUnit) -> CodeModule {
    let interfaces = unit
        .public_interfaces
        .iter()
        .cloned()
        .map(build_interface)
        .collect::<Vec<_>>();
    let functions = unit
        .public_interfaces
        .into_iter()
        .flat_map(|interface| interface.methods.into_iter().map(build_function))
        .collect::<Vec<_>>();

    CodeModule {
        name: unit.module_name,
        imports: unit.dependencies,
        interfaces,
        structs: unit
            .internal_structs
            .into_iter()
            .map(build_struct)
            .collect(),
        functions,
    }
}

fn build_specialized_module(
    unit: ImplementationUnit,
    context: GenerationContext,
) -> SpecializedCodeModule {
    let imports = unit
        .dependencies
        .iter()
        .map(|dependency| {
            format!(
                "{}{}",
                context.language_profile.import_rules.module_prefix,
                apply_case_style(
                    dependency,
                    context.language_profile.naming_rules.module_case
                )
            )
        })
        .collect::<Vec<_>>();
    let interfaces = unit
        .public_interfaces
        .iter()
        .cloned()
        .map(|interface| build_specialized_interface(interface, &context))
        .collect::<Vec<_>>();
    let functions = unit
        .public_interfaces
        .iter()
        .flat_map(|interface| interface.methods.iter())
        .cloned()
        .map(|method| build_specialized_function(method, &context))
        .collect::<Vec<_>>();
    let structs = unit
        .internal_structs
        .iter()
        .cloned()
        .map(|structure| build_specialized_struct(structure, &context))
        .collect::<Vec<_>>();
    let file_name = format!(
        "{}.{}",
        apply_case_style(
            &unit.module_name,
            context.language_profile.naming_rules.module_case,
        ),
        context.language_profile.file_layout_rules.extension
    );
    let path = format!(
        "{}/{}",
        context.language_profile.file_layout_rules.source_dir, file_name
    );
    let mut dependencies = ctx_dependencies(&context);
    dependencies.sort_by(|lhs, rhs| {
        lhs.name
            .cmp(&rhs.name)
            .then_with(|| lhs.version.cmp(&rhs.version))
    });
    dependencies.dedup_by(|lhs, rhs| lhs.name == rhs.name);

    SpecializedCodeModule {
        name: unit.module_name,
        path,
        imports,
        interfaces,
        structs,
        functions,
        dependencies,
        context,
    }
}

fn build_interface(interface: InterfaceSpec) -> CodeInterface {
    CodeInterface {
        name: interface.name,
        methods: interface
            .methods
            .into_iter()
            .map(|method| CodeFunctionSignature {
                name: method.name,
                inputs: method.inputs,
                output: method.output,
            })
            .collect(),
    }
}

fn build_struct(struct_spec: StructSpec) -> CodeStruct {
    CodeStruct {
        name: struct_spec.name,
        fields: struct_spec.fields,
    }
}

fn build_function(method: MethodSpec) -> CodeFunction {
    CodeFunction {
        name: method.name,
        inputs: method.inputs,
        output: method.output,
        body: CodeBlock {
            statements: vec!["unimplemented!()".to_string()],
        },
    }
}

fn build_specialized_interface(
    interface: InterfaceSpec,
    context: &GenerationContext,
) -> SpecializedCodeInterface {
    SpecializedCodeInterface {
        name: apply_case_style(
            &interface.name,
            context.language_profile.naming_rules.type_case,
        ),
        methods: interface
            .methods
            .into_iter()
            .map(|method| SpecializedCodeSignature {
                name: apply_case_style(
                    &method.name,
                    context.language_profile.naming_rules.method_case,
                ),
                inputs: method
                    .inputs
                    .iter()
                    .map(|ty| map_type_with_context(ty, context))
                    .collect(),
                output: method
                    .output
                    .as_ref()
                    .map(|ty| map_type_with_context(ty, context)),
            })
            .collect(),
    }
}

fn build_specialized_struct(
    structure: StructSpec,
    context: &GenerationContext,
) -> SpecializedCodeStruct {
    SpecializedCodeStruct {
        name: apply_case_style(
            &structure.name,
            context.language_profile.naming_rules.type_case,
        ),
        fields: structure
            .fields
            .into_iter()
            .map(|field| SpecializedField {
                name: apply_case_style(
                    &field.name,
                    context.language_profile.naming_rules.method_case,
                ),
                ty: map_type_with_context(&field.ty, context),
            })
            .collect(),
    }
}

fn build_specialized_function(
    method: MethodSpec,
    context: &GenerationContext,
) -> SpecializedCodeFunction {
    SpecializedCodeFunction {
        name: apply_case_style(
            &method.name,
            context.language_profile.naming_rules.method_case,
        ),
        inputs: method
            .inputs
            .iter()
            .enumerate()
            .map(|(index, ty)| format!("arg_{index}: {}", map_type_with_context(ty, context)))
            .collect(),
        output: method
            .output
            .as_ref()
            .map(|ty| map_type_with_context(ty, context)),
        body: default_body_for(context.language_profile.language),
    }
}

fn ctx_dependencies(context: &GenerationContext) -> Vec<DependencySpec> {
    let mut dependencies = context.dependency_policy.defaults.clone();
    dependencies.extend(context.dependency_policy.optional.clone());
    dependencies.extend(context.dependency_policy.framework_bound.clone());
    if let Some(framework) = &context.framework_profile {
        dependencies.extend(framework.dependency_overrides.clone());
    }
    dependencies
}

fn generation_context_for(
    selected: Option<ProfileCandidate>,
    language_hint: Option<TargetLanguage>,
) -> GenerationContext {
    let language = selected
        .as_ref()
        .map(|candidate| candidate.language)
        .or(language_hint)
        .unwrap_or(TargetLanguage::Rust);
    let framework = selected
        .and_then(|candidate| candidate.framework)
        .and_then(|name| framework_profile(language, &name));
    default_generation_context(language, framework)
}

pub fn default_generation_context(
    language: TargetLanguage,
    framework_profile: Option<FrameworkProfile>,
) -> GenerationContext {
    let language_profile = default_language_profile(language);
    let mut dependency_policy = default_dependency_policy(language);
    if let Some(framework) = &framework_profile {
        dependency_policy
            .framework_bound
            .extend(framework.dependency_overrides.clone());
    }
    let template_policy = default_template_policy(language, framework_profile.as_ref());
    let test_policy = default_test_policy(language, framework_profile.as_ref());
    GenerationContext {
        language_profile,
        framework_profile,
        dependency_policy,
        template_policy,
        test_policy,
    }
}

pub fn default_language_profile(language: TargetLanguage) -> LanguageProfile {
    match language {
        TargetLanguage::Rust => LanguageProfile {
            language,
            naming_rules: NamingRules {
                module_case: CaseStyle::SnakeCase,
                type_case: CaseStyle::PascalCase,
                method_case: CaseStyle::SnakeCase,
            },
            import_rules: ImportRules {
                module_prefix: "crate::".to_string(),
                use_file_extensions: false,
            },
            file_layout_rules: FileLayoutRules {
                source_dir: "src".to_string(),
                extension: "rs".to_string(),
            },
        },
        TargetLanguage::Python => LanguageProfile {
            language,
            naming_rules: NamingRules {
                module_case: CaseStyle::SnakeCase,
                type_case: CaseStyle::PascalCase,
                method_case: CaseStyle::SnakeCase,
            },
            import_rules: ImportRules {
                module_prefix: "app.".to_string(),
                use_file_extensions: false,
            },
            file_layout_rules: FileLayoutRules {
                source_dir: "app".to_string(),
                extension: "py".to_string(),
            },
        },
        TargetLanguage::TypeScript => LanguageProfile {
            language,
            naming_rules: NamingRules {
                module_case: CaseStyle::SnakeCase,
                type_case: CaseStyle::PascalCase,
                method_case: CaseStyle::CamelCase,
            },
            import_rules: ImportRules {
                module_prefix: "./".to_string(),
                use_file_extensions: true,
            },
            file_layout_rules: FileLayoutRules {
                source_dir: "src".to_string(),
                extension: "ts".to_string(),
            },
        },
    }
}

fn default_dependency_policy(language: TargetLanguage) -> DependencyPolicy {
    match language {
        TargetLanguage::Rust => DependencyPolicy {
            defaults: vec![DependencySpec {
                name: "anyhow".to_string(),
                version: "1".to_string(),
            }],
            optional: vec![],
            framework_bound: vec![],
        },
        TargetLanguage::Python => DependencyPolicy {
            defaults: vec![DependencySpec {
                name: "pydantic".to_string(),
                version: "2".to_string(),
            }],
            optional: vec![],
            framework_bound: vec![],
        },
        TargetLanguage::TypeScript => DependencyPolicy {
            defaults: vec![DependencySpec {
                name: "typescript".to_string(),
                version: "^5".to_string(),
            }],
            optional: vec![],
            framework_bound: vec![],
        },
    }
}

fn default_template_policy(
    language: TargetLanguage,
    framework_profile: Option<&FrameworkProfile>,
) -> TemplatePolicy {
    let project_layout_policy = framework_profile
        .map(|framework| framework.project_layout)
        .unwrap_or(match language {
            TargetLanguage::Rust => ProjectLayoutPolicy::CargoBinaryLib,
            TargetLanguage::Python => ProjectLayoutPolicy::PythonPackage,
            TargetLanguage::TypeScript => ProjectLayoutPolicy::TypeScriptService,
        });
    TemplatePolicy {
        entrypoint_template: match project_layout_policy {
            ProjectLayoutPolicy::CargoBinaryLib => "main".to_string(),
            ProjectLayoutPolicy::PythonPackage => "app".to_string(),
            ProjectLayoutPolicy::TypeScriptService => "index".to_string(),
        },
        module_template_family: format!("{}-module", language_name(language)),
        test_template_family: format!("{}-tests", language_name(language)),
        project_layout_policy,
    }
}

fn default_test_policy(
    language: TargetLanguage,
    framework_profile: Option<&FrameworkProfile>,
) -> TestPolicy {
    let style = framework_profile
        .map(|framework| framework.test_conventions.command.as_str())
        .map(|command| {
            if command.contains("pytest") {
                TestStyle::Pytest
            } else if command.contains("jest") {
                TestStyle::Jest
            } else {
                TestStyle::Native
            }
        })
        .unwrap_or(match language {
            TargetLanguage::Rust => TestStyle::Native,
            TargetLanguage::Python => TestStyle::Pytest,
            TargetLanguage::TypeScript => TestStyle::Jest,
        });
    TestPolicy {
        enabled: true,
        style,
        conventions: framework_profile
            .map(|framework| framework.test_conventions.command.clone())
            .into_iter()
            .collect(),
    }
}

fn framework_profile(language: TargetLanguage, name: &str) -> Option<FrameworkProfile> {
    match (language, name.to_ascii_lowercase().as_str()) {
        (TargetLanguage::Rust, "axum") => Some(FrameworkProfile {
            name: "axum".to_string(),
            project_layout: ProjectLayoutPolicy::CargoBinaryLib,
            dependency_overrides: vec![DependencySpec {
                name: "axum".to_string(),
                version: "0.7".to_string(),
            }],
            interface_conventions: InterfaceConvention {
                trait_prefix: "Http".to_string(),
                method_prefix: "handle_".to_string(),
            },
            test_conventions: TestConvention {
                file_suffix: "_test.rs".to_string(),
                command: "cargo test".to_string(),
            },
        }),
        (TargetLanguage::Python, "fastapi") => Some(FrameworkProfile {
            name: "fastapi".to_string(),
            project_layout: ProjectLayoutPolicy::PythonPackage,
            dependency_overrides: vec![DependencySpec {
                name: "fastapi".to_string(),
                version: "0.110".to_string(),
            }],
            interface_conventions: InterfaceConvention {
                trait_prefix: "Api".to_string(),
                method_prefix: "route_".to_string(),
            },
            test_conventions: TestConvention {
                file_suffix: "_test.py".to_string(),
                command: "pytest".to_string(),
            },
        }),
        (TargetLanguage::TypeScript, "express") => Some(FrameworkProfile {
            name: "express".to_string(),
            project_layout: ProjectLayoutPolicy::TypeScriptService,
            dependency_overrides: vec![DependencySpec {
                name: "express".to_string(),
                version: "^4".to_string(),
            }],
            interface_conventions: InterfaceConvention {
                trait_prefix: "Http".to_string(),
                method_prefix: "handle".to_string(),
            },
            test_conventions: TestConvention {
                file_suffix: ".spec.ts".to_string(),
                command: "npm test".to_string(),
            },
        }),
        _ => None,
    }
}

fn profile_candidate_from_record(record: &MemoryRecord, index: usize) -> Option<ProfileCandidate> {
    let language = parse_language_record(record)?;
    let framework = parse_framework_record(record);
    Some(ProfileCandidate {
        language,
        framework,
        score: 1.0 / (index + 1) as f64,
    })
}

fn parse_language_record(record: &MemoryRecord) -> Option<TargetLanguage> {
    record
        .tags
        .iter()
        .find_map(|tag| parse_language_token(tag))
        .or_else(|| {
            record
                .relations
                .iter()
                .find_map(|relation| parse_language_token(relation))
        })
        .or_else(|| parse_language_token(&record.text))
}

fn parse_framework_record(record: &MemoryRecord) -> Option<String> {
    record
        .tags
        .iter()
        .find_map(|tag| parse_framework_token(tag))
        .or_else(|| {
            record
                .relations
                .iter()
                .find_map(|relation| parse_framework_token(relation))
        })
        .or_else(|| parse_framework_token(&record.text))
}

fn parse_language_token(value: &str) -> Option<TargetLanguage> {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("lang:rust") || normalized.contains(" rust") || normalized == "rust" {
        Some(TargetLanguage::Rust)
    } else if normalized.contains("lang:python")
        || normalized.contains(" python")
        || normalized == "python"
    {
        Some(TargetLanguage::Python)
    } else if normalized.contains("lang:typescript")
        || normalized.contains("lang:ts")
        || normalized.contains(" typescript")
        || normalized == "typescript"
        || normalized == "ts"
    {
        Some(TargetLanguage::TypeScript)
    } else {
        None
    }
}

fn parse_framework_token(value: &str) -> Option<String> {
    let normalized = value.to_ascii_lowercase();
    for candidate in ["axum", "fastapi", "express"] {
        if normalized.contains(&format!("framework:{candidate}")) || normalized.contains(candidate)
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn framework_compatible(ctx: &GenerationContext) -> bool {
    ctx.framework_profile
        .as_ref()
        .map(|framework| {
            matches!(
                (ctx.language_profile.language, framework.name.as_str()),
                (TargetLanguage::Rust, "axum")
                    | (TargetLanguage::Python, "fastapi")
                    | (TargetLanguage::TypeScript, "express")
            )
        })
        .unwrap_or(true)
}

fn map_type_with_context(ty: &TypeRef, context: &GenerationContext) -> String {
    match context.language_profile.language {
        TargetLanguage::Rust => RustTypeMapper.map_type(ty),
        TargetLanguage::Python => PythonTypeMapper.map_type(ty),
        TargetLanguage::TypeScript => TypeScriptTypeMapper.map_type(ty),
    }
}

fn render_rust(module: &CodeModule) -> String {
    let imports = module
        .imports
        .iter()
        .map(|import| format!("use crate::{import};"))
        .collect::<Vec<_>>()
        .join("\n");
    let structs = module
        .structs
        .iter()
        .map(|structure| {
            let fields = structure
                .fields
                .iter()
                .map(|field| format!("    pub {}: {},", field.name, map_rust_type(&field.ty)))
                .collect::<Vec<_>>()
                .join("\n");
            format!("pub struct {} {{\n{}\n}}", structure.name, fields)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let args = function
                .inputs
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("arg_{index}: {}", map_rust_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let output = function
                .output
                .as_ref()
                .map(|output| format!(" -> {}", map_rust_type(output)))
                .unwrap_or_default();
            format!(
                "pub fn {}({}){} {{\n    unimplemented!()\n}}",
                function.name, args, output
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [imports, structs, functions]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_python(module: &CodeModule) -> String {
    module
        .functions
        .iter()
        .map(|function| {
            let args = function
                .inputs
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("arg_{index}: {}", map_python_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let output = function
                .output
                .as_ref()
                .map(|output| format!(" -> {}", map_python_type(output)))
                .unwrap_or_default();
            format!("def {}({}){}:\n    pass", function.name, args, output)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_typescript(module: &CodeModule) -> String {
    module
        .functions
        .iter()
        .map(|function| {
            let args = function
                .inputs
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("arg_{index}: {}", map_typescript_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let output = function
                .output
                .as_ref()
                .map(map_typescript_type)
                .unwrap_or_else(|| "void".to_string());
            format!(
                "export function {}({}): {} {{\n  throw new Error(\"not implemented\");\n}}",
                function.name, args, output
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_specialized_rust(module: &SpecializedCodeModule) -> String {
    let framework_comment = module
        .context
        .framework_profile
        .as_ref()
        .map(|framework| format!("// framework: {}\n", framework.name))
        .unwrap_or_default();
    let imports = module
        .imports
        .iter()
        .map(|import| format!("use {};", import))
        .collect::<Vec<_>>()
        .join("\n");
    let structs = module
        .structs
        .iter()
        .map(|structure| {
            let fields = structure
                .fields
                .iter()
                .map(|field| format!("    pub {}: {},", field.name, field.ty))
                .collect::<Vec<_>>()
                .join("\n");
            format!("pub struct {} {{\n{}\n}}", structure.name, fields)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let args = function.inputs.join(", ");
            let output = function
                .output
                .as_ref()
                .map(|output| format!(" -> {output}"))
                .unwrap_or_default();
            format!(
                "pub fn {}({}){} {{\n    {}\n}}",
                function.name,
                args,
                output,
                function.body.join("\n    ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [framework_comment, imports, structs, functions]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_specialized_python(module: &SpecializedCodeModule) -> String {
    let imports = module
        .imports
        .iter()
        .map(|import| format!("import {}", import.trim_start_matches("app.")))
        .collect::<Vec<_>>()
        .join("\n");
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let args = if function.inputs.is_empty() {
                "self".to_string()
            } else {
                format!("self, {}", function.inputs.join(", "))
            };
            let output = function
                .output
                .as_ref()
                .map(|output| format!(" -> {output}"))
                .unwrap_or_default();
            format!(
                "def {}({}){}:\n    {}",
                function.name,
                args,
                output,
                function.body.join("\n    ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [imports, functions]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_specialized_typescript(module: &SpecializedCodeModule) -> String {
    let imports = module
        .imports
        .iter()
        .map(|import| format!("import * as dep from '{}';", import))
        .collect::<Vec<_>>()
        .join("\n");
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let args = function.inputs.join(", ");
            let output = function
                .output
                .as_ref()
                .cloned()
                .unwrap_or_else(|| "void".to_string());
            format!(
                "export function {}({}): {} {{\n  {}\n}}",
                function.name,
                args,
                output,
                function.body.join("\n  ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [imports, functions]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendFlavor {
    Rust,
    Python,
    TypeScript,
}

#[allow(dead_code)]
fn lower_block<B: LanguageBackend>(backend: &B, block: &ProgramBlock) -> Vec<TargetStmt> {
    block
        .statements
        .iter()
        .map(|statement| backend.lower_statement(statement))
        .collect()
}

#[allow(dead_code)]
fn lower_statement_code<B: LanguageBackend>(
    backend: &B,
    stmt: &ProgramStatement,
    flavor: BackendFlavor,
) -> String {
    match stmt {
        ProgramStatement::Assign { target, value } => {
            format!("{target} = {}", backend.lower_expression(value).code)
        }
        ProgramStatement::If(branch) => {
            let condition = backend.lower_expression(&branch.condition).code;
            let then_code = render_nested_block(&lower_block(backend, &branch.then_block), flavor);
            let else_code = render_nested_block(&lower_block(backend, &branch.else_block), flavor);
            match flavor {
                BackendFlavor::Rust | BackendFlavor::TypeScript => {
                    format!("if {condition} {{\n{then_code}\n}} else {{\n{else_code}\n}}")
                }
                BackendFlavor::Python => {
                    format!("if {condition}:\n{then_code}\nelse:\n{else_code}")
                }
            }
        }
        ProgramStatement::Loop(loop_stmt) => {
            let iterator = backend.lower_expression(&loop_stmt.iterator).code;
            let body = render_nested_block(&lower_block(backend, &loop_stmt.body), flavor);
            match (flavor, &loop_stmt.kind) {
                (BackendFlavor::Rust, _) => format!("while {iterator} {{\n{body}\n}}"),
                (BackendFlavor::Python, _) => format!("while {iterator}:\n{body}"),
                (BackendFlavor::TypeScript, _) => format!("while ({iterator}) {{\n{body}\n}}"),
            }
        }
        ProgramStatement::Return { value } => match value {
            Some(value) => format!("return {}", backend.lower_expression(value).code),
            None => "return".to_string(),
        },
        ProgramStatement::Expression(expr) => backend.lower_expression(expr).code,
    }
}

fn lower_expression_code(expr: &ProgramExpression, flavor: BackendFlavor) -> String {
    match expr {
        ProgramExpression::Literal(literal) => match literal {
            ProgramLiteral::Int(value) => value.to_string(),
            ProgramLiteral::Float(value) => value.clone(),
            ProgramLiteral::Bool(value) => value.to_string(),
            ProgramLiteral::String(value) => format!("{value:?}"),
            ProgramLiteral::Void => match flavor {
                BackendFlavor::Python => "None".to_string(),
                _ => "()".to_string(),
            },
        },
        ProgramExpression::Variable(name) => name.clone(),
        ProgramExpression::Call(call) => format!(
            "{}({})",
            call.function,
            call.args
                .iter()
                .map(|arg| lower_expression_code(arg, flavor))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ProgramExpression::Await(inner) => match flavor {
            BackendFlavor::Rust => format!("{}.await", lower_expression_code(inner, flavor)),
            BackendFlavor::Python => format!("await {}", lower_expression_code(inner, flavor)),
            BackendFlavor::TypeScript => format!("await {}", lower_expression_code(inner, flavor)),
        },
        ProgramExpression::BinaryOp(op) => format!(
            "{} {} {}",
            lower_expression_code(&op.left, flavor),
            binary_operator(op.op.clone()),
            lower_expression_code(&op.right, flavor)
        ),
        ProgramExpression::UnaryOp(op) => format!(
            "{}{}",
            unary_operator(op.op.clone()),
            lower_expression_code(&op.expr, flavor)
        ),
    }
}

fn render_target_program(program: &TargetProgram, flavor: BackendFlavor) -> String {
    program
        .modules
        .iter()
        .map(|module| render_target_module(module, flavor))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_target_module(module: &TargetModule, flavor: BackendFlavor) -> String {
    module
        .items
        .iter()
        .map(|item| match item {
            TargetItem::Function(function) => render_target_function(function, flavor),
            TargetItem::Type(ty) => render_target_type(ty, flavor),
            TargetItem::State(state) => render_target_state(state, flavor),
        })
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_target_type(ty: &TargetType, flavor: BackendFlavor) -> String {
    match (flavor, &ty.kind) {
        (BackendFlavor::Rust, TargetTypeKind::Struct) => format!(
            "pub struct {} {{\n{}\n}}",
            ty.name,
            ty.fields
                .iter()
                .map(|field| format!("    pub {}: {},", field.name, field.ty))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        (BackendFlavor::Rust, TargetTypeKind::Interface) => format!(
            "pub trait {} {{\n{}\n}}",
            ty.name,
            ty.methods
                .iter()
                .map(|method| format!(
                    "    fn {}({}) -> {};",
                    method.name,
                    method
                        .inputs
                        .iter()
                        .map(|input| format!("{}: {}", input.name, input.ty))
                        .collect::<Vec<_>>()
                        .join(", "),
                    method.output
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        (BackendFlavor::Python, _) => format!("class {}:\n    pass", ty.name),
        (BackendFlavor::TypeScript, TargetTypeKind::Struct | TargetTypeKind::Interface) => format!(
            "export interface {} {{\n{}\n}}",
            ty.name,
            ty.fields
                .iter()
                .map(|field| format!("  {}: {};", field.name, field.ty))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        (_, _) => String::new(),
    }
}

fn render_target_function(function: &TargetFunction, flavor: BackendFlavor) -> String {
    let body = render_nested_block(&function.body, flavor);
    match flavor {
        BackendFlavor::Rust => {
            let async_prefix = if function.semantics.is_async {
                "async "
            } else {
                ""
            };
            format!(
                "{}{}fn {}({}) -> {} {{\n{}\n}}",
                function.visibility,
                async_prefix,
                function.name,
                function
                    .inputs
                    .iter()
                    .map(|input| format!("{}: {}", input.name, input.ty))
                    .collect::<Vec<_>>()
                    .join(", "),
                function.output,
                body
            )
        }
        BackendFlavor::Python => {
            let async_prefix = if function.semantics.is_async {
                "async "
            } else {
                ""
            };
            format!(
                "{}def {}({}) -> {}:\n{}",
                async_prefix,
                function.name,
                function
                    .inputs
                    .iter()
                    .map(|input| format!("{}: {}", input.name, input.ty))
                    .collect::<Vec<_>>()
                    .join(", "),
                function.output,
                body
            )
        }
        BackendFlavor::TypeScript => {
            let async_prefix = if function.semantics.is_async {
                "async "
            } else {
                ""
            };
            format!(
                "export {}function {}({}): {} {{\n{}\n}}",
                async_prefix,
                function.name,
                function
                    .inputs
                    .iter()
                    .map(|input| format!("{}: {}", input.name, input.ty))
                    .collect::<Vec<_>>()
                    .join(", "),
                function.output,
                body
            )
        }
    }
}

fn render_target_state(state: &TargetState, flavor: BackendFlavor) -> String {
    match flavor {
        BackendFlavor::Rust => format!("{} {}: {};", state.binding, state.name, state.ty),
        BackendFlavor::Python | BackendFlavor::TypeScript => {
            format!("{} {}: {}", state.binding, state.name, state.ty)
        }
    }
}

fn render_nested_block(statements: &[TargetStmt], flavor: BackendFlavor) -> String {
    let indent = match flavor {
        BackendFlavor::Python => "    ",
        BackendFlavor::Rust | BackendFlavor::TypeScript => "    ",
    };
    if statements.is_empty() {
        return format!("{indent}pass");
    }
    statements
        .iter()
        .map(|statement| {
            statement
                .code
                .lines()
                .map(|line| format!("{indent}{line}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lower_type_kind(kind: &ProgramTypeKind) -> TargetTypeKind {
    match kind {
        ProgramTypeKind::Struct => TargetTypeKind::Struct,
        ProgramTypeKind::Enum => TargetTypeKind::Enum,
        ProgramTypeKind::Interface => TargetTypeKind::Interface,
        ProgramTypeKind::Alias => TargetTypeKind::Alias,
    }
}

fn lower_visibility(visibility: &ProgramVisibility) -> String {
    match visibility {
        ProgramVisibility::Public => "pub ".to_string(),
        ProgramVisibility::Private => String::new(),
    }
}

#[allow(dead_code)]
fn lower_rust_input_type(input: &ProgramFunctionInput, effects: &[ProgramEffect]) -> String {
    if input
        .borrow
        .as_ref()
        .map(|borrow| borrow.is_mut)
        .unwrap_or(false)
        || effects.contains(&ProgramEffect::Mutation)
    {
        format!("&mut {}", lower_rust_program_type(&input.r#type))
    } else {
        lower_rust_program_type(&input.r#type)
    }
}

#[allow(dead_code)]
fn lower_rust_program_type(ty: &ProgramTypeRef) -> String {
    let base = match ty.name.as_str() {
        "Int" => "i32".to_string(),
        "Float" => "f64".to_string(),
        "Bool" => "bool".to_string(),
        "String" => "String".to_string(),
        "Void" => "()".to_string(),
        other => other.to_string(),
    };
    let with_generics = if ty.generics.is_empty() {
        base
    } else {
        format!(
            "{}<{}>",
            base,
            ty.generics
                .iter()
                .map(lower_rust_program_type)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    if ty.nullable {
        format!("Option<{with_generics}>")
    } else {
        with_generics
    }
}

#[allow(dead_code)]
fn lower_python_program_type(ty: &ProgramTypeRef) -> String {
    let base = match ty.name.as_str() {
        "Int" => "int".to_string(),
        "Float" => "float".to_string(),
        "Bool" => "bool".to_string(),
        "String" => "str".to_string(),
        "Void" => "None".to_string(),
        other => other.to_string(),
    };
    if ty.nullable {
        format!("Optional[{base}]")
    } else {
        base
    }
}

#[allow(dead_code)]
fn lower_typescript_program_type(ty: &ProgramTypeRef) -> String {
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

#[allow(dead_code)]
fn lower_typescript_output_type(
    output: &ProgramFunctionOutput,
    effects: &[ProgramEffect],
) -> String {
    let base = lower_typescript_program_type(&output.r#type);
    if effects.contains(&ProgramEffect::Async) {
        format!("Promise<{base}>")
    } else {
        base
    }
}

#[allow(dead_code)]
fn rust_state_binding(state: &ProgramState) -> String {
    match state.scope {
        code_ir::program_v1::StateScope::Global => match state.kind {
            ProgramStateKind::Mutable => "static mut".to_string(),
            ProgramStateKind::Immutable => "static".to_string(),
            ProgramStateKind::Shared => "static".to_string(),
        },
        code_ir::program_v1::StateScope::Module | code_ir::program_v1::StateScope::Local => {
            match state.kind {
                ProgramStateKind::Mutable => "let mut".to_string(),
                ProgramStateKind::Immutable => "let".to_string(),
                ProgramStateKind::Shared => "let".to_string(),
            }
        }
    }
}

#[allow(dead_code)]
fn python_state_binding(state: &ProgramState) -> String {
    match state.scope {
        code_ir::program_v1::StateScope::Global => "global".to_string(),
        code_ir::program_v1::StateScope::Module => "module".to_string(),
        code_ir::program_v1::StateScope::Local => "local".to_string(),
    }
}

#[allow(dead_code)]
fn typescript_state_binding(state: &ProgramState) -> String {
    match state.kind {
        ProgramStateKind::Mutable | ProgramStateKind::Shared => "let".to_string(),
        ProgramStateKind::Immutable => "const".to_string(),
    }
}

fn binary_operator(op: ProgramBinaryOperator) -> &'static str {
    match op {
        ProgramBinaryOperator::Add => "+",
        ProgramBinaryOperator::Sub => "-",
        ProgramBinaryOperator::Mul => "*",
        ProgramBinaryOperator::Div => "/",
        ProgramBinaryOperator::Eq => "==",
        ProgramBinaryOperator::Ne => "!=",
        ProgramBinaryOperator::Lt => "<",
        ProgramBinaryOperator::Gt => ">",
    }
}

fn unary_operator(op: code_ir::program_v1::UnaryOperator) -> &'static str {
    match op {
        code_ir::program_v1::UnaryOperator::Neg => "-",
        code_ir::program_v1::UnaryOperator::Not => "!",
    }
}

fn default_body_for(language: TargetLanguage) -> Vec<String> {
    match language {
        TargetLanguage::Rust => vec!["unimplemented!()".to_string()],
        TargetLanguage::Python => vec!["raise NotImplementedError()".to_string()],
        TargetLanguage::TypeScript => vec!["throw new Error('not implemented');".to_string()],
    }
}

fn collect_known_types(program: &Program) -> BTreeSet<String> {
    let mut known = BTreeSet::from([
        "Int".to_string(),
        "Float".to_string(),
        "Bool".to_string(),
        "String".to_string(),
        "Void".to_string(),
    ]);
    for module in &program.modules {
        for ty in &module.types {
            known.insert(ty.name.clone());
        }
    }
    known
}

fn collect_async_functions(program: &Program) -> BTreeSet<String> {
    program
        .modules
        .iter()
        .flat_map(|module| module.functions.iter())
        .filter(|function| function.effects.contains(&ProgramEffect::Async))
        .map(|function| function.name.clone())
        .collect()
}

fn validate_type_ref(
    ty: &ProgramTypeRef,
    known_types: &BTreeSet<String>,
    result: &mut ValidationResult,
    context: String,
) {
    if ty.nullable && ty.name == "Void" {
        push_error(
            result,
            "nullable_void",
            format!("{context} cannot be nullable Void"),
        );
    }
    if !known_types.contains(&ty.name) && !ty.name.starts_with("List") {
        push_error(
            result,
            "unknown_type",
            format!("{context} references unknown type `{}`", ty.name),
        );
    }
    for generic in &ty.generics {
        validate_type_ref(generic, known_types, result, context.clone());
    }
}

fn validate_async_usage(
    function: &ProgramFunction,
    async_functions: &BTreeSet<String>,
    result: &mut ValidationResult,
) {
    walk_block_for_async(function, &function.body, async_functions, result);
}

fn walk_block_for_async(
    function: &ProgramFunction,
    block: &ProgramBlock,
    async_functions: &BTreeSet<String>,
    result: &mut ValidationResult,
) {
    for statement in &block.statements {
        walk_statement_for_async(function, statement, async_functions, result);
    }
}

fn walk_statement_for_async(
    function: &ProgramFunction,
    statement: &ProgramStatement,
    async_functions: &BTreeSet<String>,
    result: &mut ValidationResult,
) {
    match statement {
        ProgramStatement::Assign { value, .. } => {
            walk_expression_for_async(function, value, false, async_functions, result)
        }
        ProgramStatement::If(branch) => {
            walk_expression_for_async(function, &branch.condition, false, async_functions, result);
            walk_block_for_async(function, &branch.then_block, async_functions, result);
            walk_block_for_async(function, &branch.else_block, async_functions, result);
        }
        ProgramStatement::Loop(loop_stmt) => {
            walk_expression_for_async(
                function,
                &loop_stmt.iterator,
                false,
                async_functions,
                result,
            );
            walk_block_for_async(function, &loop_stmt.body, async_functions, result);
        }
        ProgramStatement::Return { value } => {
            if let Some(value) = value {
                walk_expression_for_async(function, value, false, async_functions, result);
            }
        }
        ProgramStatement::Expression(expr) => {
            walk_expression_for_async(function, expr, false, async_functions, result)
        }
    }
}

fn walk_expression_for_async(
    function: &ProgramFunction,
    expr: &ProgramExpression,
    awaited: bool,
    async_functions: &BTreeSet<String>,
    result: &mut ValidationResult,
) {
    match expr {
        ProgramExpression::Call(call) => {
            if async_functions.contains(&call.function) && !awaited {
                push_error(
                    result,
                    "missing_await",
                    format!(
                        "function `{}` calls async `{}` without await",
                        function.name, call.function
                    ),
                );
            }
            for arg in &call.args {
                walk_expression_for_async(function, arg, false, async_functions, result);
            }
        }
        ProgramExpression::Await(inner) => {
            walk_expression_for_async(function, inner, true, async_functions, result);
        }
        ProgramExpression::BinaryOp(op) => {
            walk_expression_for_async(function, &op.left, false, async_functions, result);
            walk_expression_for_async(function, &op.right, false, async_functions, result);
        }
        ProgramExpression::UnaryOp(op) => {
            walk_expression_for_async(function, &op.expr, false, async_functions, result);
        }
        ProgramExpression::Literal(_) | ProgramExpression::Variable(_) => {}
    }
}

fn push_error(result: &mut ValidationResult, code: &'static str, message: String) {
    result.errors.push(ValidationIssue { code, message });
}

fn collect_imports_from_type_ref(
    ty: &ProgramTypeRef,
    module_names: &BTreeSet<String>,
    imports: &mut Vec<String>,
) {
    if module_names.contains(&ty.name) {
        imports.push(format!("crate::{}", ty.name));
    }
    for generic in &ty.generics {
        collect_imports_from_type_ref(generic, module_names, imports);
    }
}

fn collect_imports_from_block(
    block: &ProgramBlock,
    module_names: &BTreeSet<String>,
    imports: &mut Vec<String>,
) {
    for statement in &block.statements {
        collect_imports_from_statement(statement, module_names, imports);
    }
}

fn collect_imports_from_statement(
    stmt: &ProgramStatement,
    module_names: &BTreeSet<String>,
    imports: &mut Vec<String>,
) {
    match stmt {
        ProgramStatement::Assign { value, .. } => {
            collect_imports_from_expression(value, module_names, imports)
        }
        ProgramStatement::If(branch) => {
            collect_imports_from_expression(&branch.condition, module_names, imports);
            collect_imports_from_block(&branch.then_block, module_names, imports);
            collect_imports_from_block(&branch.else_block, module_names, imports);
        }
        ProgramStatement::Loop(loop_stmt) => {
            collect_imports_from_expression(&loop_stmt.iterator, module_names, imports);
            collect_imports_from_block(&loop_stmt.body, module_names, imports);
        }
        ProgramStatement::Return { value } => {
            if let Some(value) = value {
                collect_imports_from_expression(value, module_names, imports);
            }
        }
        ProgramStatement::Expression(expr) => {
            collect_imports_from_expression(expr, module_names, imports)
        }
    }
}

fn collect_imports_from_expression(
    expr: &ProgramExpression,
    module_names: &BTreeSet<String>,
    imports: &mut Vec<String>,
) {
    match expr {
        ProgramExpression::Call(call) => {
            if module_names.contains(&call.function) {
                imports.push(format!("crate::{}", call.function));
            }
            for arg in &call.args {
                collect_imports_from_expression(arg, module_names, imports);
            }
        }
        ProgramExpression::Await(inner) => {
            collect_imports_from_expression(inner, module_names, imports)
        }
        ProgramExpression::BinaryOp(op) => {
            collect_imports_from_expression(&op.left, module_names, imports);
            collect_imports_from_expression(&op.right, module_names, imports);
        }
        ProgramExpression::UnaryOp(op) => {
            collect_imports_from_expression(&op.expr, module_names, imports)
        }
        ProgramExpression::Literal(_) | ProgramExpression::Variable(_) => {}
    }
}

fn backend_name<B: ?Sized>(_: &B) -> BackendFlavor {
    let name = std::any::type_name::<B>();
    if name.contains("Python") {
        BackendFlavor::Python
    } else if name.contains("TypeScript") {
        BackendFlavor::TypeScript
    } else {
        BackendFlavor::Rust
    }
}

fn extension_for_backend(flavor: BackendFlavor) -> &'static str {
    match flavor {
        BackendFlavor::Rust => "rs",
        BackendFlavor::Python => "py",
        BackendFlavor::TypeScript => "ts",
    }
}

fn run_safe_build_validation(
    program: &Program,
    files: &[GeneratedFile],
    flavor: BackendFlavor,
) -> Result<(), String> {
    if !program.build_validation.enabled {
        return Ok(());
    }
    let sandbox = semantic_sandbox_root()?;
    fs::create_dir_all(sandbox.join("src"))
        .map_err(|err| format!("failed to create sandbox src: {err}"))?;
    for file in files {
        let path = sandbox.join("src").join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::write(&path, &file.content)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    let result = match flavor {
        BackendFlavor::Rust => {
            fs::write(
                sandbox.join("Cargo.toml"),
                "[package]\nname = \"semantic_codegen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .map_err(|err| format!("failed to write Cargo.toml: {err}"))?;
            let main_path = sandbox.join("src/main.rs");
            if main_path.exists() {
                let mut main_content = fs::read_to_string(&main_path)
                    .map_err(|err| format!("failed to read {}: {err}", main_path.display()))?;
                if !main_content.contains("fn main(") {
                    if !main_content.ends_with('\n') {
                        main_content.push('\n');
                    }
                    main_content.push_str("\nfn main() {}\n");
                    fs::write(&main_path, main_content)
                        .map_err(|err| format!("failed to patch {}: {err}", main_path.display()))?;
                }
            }
            let mut mod_lines = files
                .iter()
                .map(|file| file.path.trim_end_matches(".rs"))
                .filter(|name| *name != "main" && *name != "lib")
                .map(|name| format!("pub mod {name};"))
                .collect::<Vec<_>>();
            mod_lines.sort();
            fs::write(
                sandbox.join("src/lib.rs"),
                format!("{}\n", mod_lines.join("\n")),
            )
            .map_err(|err| format!("failed to write lib.rs: {err}"))?;
            run_command_in(&program.build_validation.command, &sandbox)
        }
        BackendFlavor::Python | BackendFlavor::TypeScript => Ok(()),
    };
    let _ = fs::remove_dir_all(&sandbox);
    result
}

fn run_command_in(command: &str, root: &Path) -> Result<(), String> {
    let mut parts = command.split_whitespace();
    let Some(bin) = parts.next() else {
        return Ok(());
    };
    let output = Command::new(bin)
        .args(parts)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run `{command}`: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn semantic_sandbox_root() -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("dbm_semantic_codegen_{unique}")))
}

fn map_rust_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(name) => match normalize_primitive(name).as_str() {
            "int" => "i64".to_string(),
            "string" => "String".to_string(),
            "bool" => "bool".to_string(),
            other => other.to_string(),
        },
        TypeRef::Custom(name) => name.clone(),
        TypeRef::List(inner) => format!("Vec<{}>", map_rust_type(inner)),
        TypeRef::Optional(inner) => format!("Option<{}>", map_rust_type(inner)),
    }
}

fn map_python_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(name) => match normalize_primitive(name).as_str() {
            "int" => "int".to_string(),
            "string" => "str".to_string(),
            "bool" => "bool".to_string(),
            other => other.to_string(),
        },
        TypeRef::Custom(name) => name.clone(),
        TypeRef::List(inner) => format!("list[{}]", map_python_type(inner)),
        TypeRef::Optional(inner) => format!("Optional[{}]", map_python_type(inner)),
    }
}

fn map_typescript_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(name) => match normalize_primitive(name).as_str() {
            "int" => "number".to_string(),
            "string" => "string".to_string(),
            "bool" => "boolean".to_string(),
            other => other.to_string(),
        },
        TypeRef::Custom(name) => name.clone(),
        TypeRef::List(inner) => format!("{}[]", map_typescript_type(inner)),
        TypeRef::Optional(inner) => format!("{} | null", map_typescript_type(inner)),
    }
}

fn normalize_primitive(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn language_name(language: TargetLanguage) -> &'static str {
    match language {
        TargetLanguage::Rust => "rust",
        TargetLanguage::Python => "python",
        TargetLanguage::TypeScript => "typescript",
    }
}

fn apply_case_style(value: &str, style: CaseStyle) -> String {
    let parts = value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return String::new();
    }
    match style {
        CaseStyle::SnakeCase => parts.join("_"),
        CaseStyle::PascalCase => parts
            .iter()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_ascii_uppercase().to_string()
                            + &chars.as_str().to_ascii_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect(),
        CaseStyle::CamelCase => {
            let mut rendered = String::new();
            for (index, part) in parts.iter().enumerate() {
                if index == 0 {
                    rendered.push_str(part);
                } else {
                    let mut chars = part.chars();
                    if let Some(first) = chars.next() {
                        rendered.push(first.to_ascii_uppercase());
                        rendered.push_str(&chars.as_str().to_ascii_lowercase());
                    }
                }
            }
            rendered
        }
    }
}
