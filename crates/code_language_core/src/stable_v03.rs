use std::sync::Arc;

use memory_space_phase14::stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord};
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
                .map(|output| map_typescript_type(output))
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

fn default_body_for(language: TargetLanguage) -> Vec<String> {
    match language {
        TargetLanguage::Rust => vec!["unimplemented!()".to_string()],
        TargetLanguage::Python => vec!["raise NotImplementedError()".to_string()],
        TargetLanguage::TypeScript => vec!["throw new Error('not implemented');".to_string()],
    }
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
