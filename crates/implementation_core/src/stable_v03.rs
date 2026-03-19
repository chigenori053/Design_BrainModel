use std::path::PathBuf;

use code_language_core::stable_v03::{
    DependencySpec, GeneratedFile, GenerationContext, ProjectLayoutPolicy,
    TargetLanguage as CodeTargetLanguage,
};
pub use execution_core::engine::execution_plan::ExecutionPlan;
use execution_core::engine::execution_plan::{
    BuildPlan, DependencyPlan, DependencySpec as ExecutionDependencySpec, RunPlan, TargetLanguage,
    TestPlan,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSpec {
    pub root_dir: String,
    pub language: TargetLanguage,
    pub framework: Option<String>,
    pub dependencies: Vec<DependencySpec>,
    pub files: Vec<GeneratedFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLayout {
    pub root_dir: String,
    pub files: Vec<GeneratedFile>,
    pub manifest_path: String,
}

impl ProjectLayout {
    pub fn is_valid(&self) -> bool {
        !self.root_dir.is_empty()
            && self
                .files
                .iter()
                .any(|file| file.path.ends_with(&self.manifest_path))
            && self
                .files
                .iter()
                .all(|file| file.path.starts_with(&self.root_dir))
    }
}

pub trait ProjectGenerator: Send + Sync {
    fn generate(
        &self,
        project_name: &str,
        files: Vec<GeneratedFile>,
        contexts: Vec<GenerationContext>,
        test_files: Vec<GeneratedFile>,
    ) -> (ProjectLayout, ExecutionPlan);
}

#[derive(Clone, Debug, Default)]
pub struct DefaultProjectGenerator;

impl ProjectGenerator for DefaultProjectGenerator {
    fn generate(
        &self,
        project_name: &str,
        files: Vec<GeneratedFile>,
        contexts: Vec<GenerationContext>,
        test_files: Vec<GeneratedFile>,
    ) -> (ProjectLayout, ExecutionPlan) {
        let primary = contexts
            .first()
            .cloned()
            .unwrap_or_else(default_rust_context);
        let root_dir = project_name.to_string();
        let source_prefix = match primary.template_policy.project_layout_policy {
            ProjectLayoutPolicy::CargoBinaryLib => "src",
            ProjectLayoutPolicy::PythonPackage => "app",
            ProjectLayoutPolicy::TypeScriptService => "src",
        };
        let manifest_path = match primary.language_profile.language {
            CodeTargetLanguage::Rust => "Cargo.toml",
            CodeTargetLanguage::Python => "pyproject.toml",
            CodeTargetLanguage::TypeScript => "package.json",
        };
        let mut dependencies = contexts
            .iter()
            .flat_map(|ctx| {
                let mut items = ctx.dependency_policy.defaults.clone();
                items.extend(ctx.dependency_policy.framework_bound.clone());
                items
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|lhs, rhs| {
            lhs.name
                .cmp(&rhs.name)
                .then_with(|| lhs.version.cmp(&rhs.version))
        });
        dependencies.dedup_by(|lhs, rhs| lhs.name == rhs.name);

        let mut layout_files = files
            .into_iter()
            .map(|file| GeneratedFile {
                path: format!(
                    "{root_dir}/{source_prefix}/{}",
                    trim_known_prefix(&file.path)
                ),
                content: file.content,
            })
            .collect::<Vec<_>>();
        layout_files.push(GeneratedFile {
            path: format!("{root_dir}/{manifest_path}"),
            content: render_manifest(primary.language_profile.language, &dependencies),
        });
        let materialized_tests = materialize_test_files(
            &root_dir,
            primary.language_profile.language,
            primary.test_policy.enabled,
            test_files,
        );
        let test_paths = materialized_tests
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        layout_files.extend(materialized_tests);

        let execution_plan = ExecutionPlan {
            language: execution_language(primary.language_profile.language),
            framework: primary
                .framework_profile
                .as_ref()
                .map(|framework| framework.name.clone()),
            project_root: PathBuf::from(root_dir.clone()),
            dependency_plan: DependencyPlan {
                manifest_file: manifest_path.to_string(),
                dependencies: dependencies
                    .into_iter()
                    .map(|dependency| ExecutionDependencySpec {
                        name: dependency.name,
                        version: Some(dependency.version),
                    })
                    .collect(),
                install_commands: dependency_commands(primary.language_profile.language),
            },
            build_plan: BuildPlan {
                build_commands: build_commands(primary.language_profile.language),
            },
            run_plan: RunPlan {
                run_commands: run_commands(
                    primary.language_profile.language,
                    source_prefix,
                    &primary.template_policy.entrypoint_template,
                ),
            },
            test_plan: TestPlan {
                test_files: test_paths,
                test_commands: test_commands(primary.language_profile.language, &primary),
            },
        };
        let project_layout = ProjectLayout {
            root_dir,
            files: layout_files,
            manifest_path: manifest_path.to_string(),
        };
        (project_layout, execution_plan)
    }
}

fn render_manifest(language: CodeTargetLanguage, dependencies: &[DependencySpec]) -> String {
    match language {
        CodeTargetLanguage::Rust => {
            let deps = dependencies
                .iter()
                .map(|dep| format!("{} = \"{}\"", dep.name, dep.version))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "[package]\nname = \"generated-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{}",
                deps
            )
        }
        CodeTargetLanguage::Python => {
            let deps = dependencies
                .iter()
                .map(|dep| format!("    \"{}=={}\",", dep.name, dep.version))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "[project]\nname = \"generated-project\"\nversion = \"0.1.0\"\ndependencies = [\n{}\n]",
                deps
            )
        }
        CodeTargetLanguage::TypeScript => {
            let deps = dependencies
                .iter()
                .map(|dep| format!("    \"{}\": \"{}\"", dep.name, dep.version))
                .collect::<Vec<_>>()
                .join(",\n");
            format!(
                "{{\n  \"name\": \"generated-project\",\n  \"version\": \"0.1.0\",\n  \"dependencies\": {{\n{}\n  }}\n}}",
                deps
            )
        }
    }
}

fn default_test_file(language: CodeTargetLanguage) -> &'static str {
    match language {
        CodeTargetLanguage::Rust => "smoke_test.rs",
        CodeTargetLanguage::Python => "test_smoke.py",
        CodeTargetLanguage::TypeScript => "smoke.spec.ts",
    }
}

fn render_test_stub(language: CodeTargetLanguage) -> String {
    match language {
        CodeTargetLanguage::Rust => "#[test]\nfn smoke() { assert!(true); }\n".to_string(),
        CodeTargetLanguage::Python => "def test_smoke() -> None:\n    assert True\n".to_string(),
        CodeTargetLanguage::TypeScript => {
            "test('smoke', () => expect(true).toBe(true));\n".to_string()
        }
    }
}

fn trim_known_prefix(path: &str) -> String {
    path.trim_start_matches("src/")
        .trim_start_matches("app/")
        .to_string()
}

fn default_rust_context() -> GenerationContext {
    GenerationContext {
        language_profile: code_language_core::stable_v03::default_language_profile(
            CodeTargetLanguage::Rust,
        ),
        framework_profile: None,
        dependency_policy: code_language_core::stable_v03::DependencyPolicy {
            defaults: vec![DependencySpec {
                name: "anyhow".to_string(),
                version: "1".to_string(),
            }],
            optional: vec![],
            framework_bound: vec![],
        },
        template_policy: code_language_core::stable_v03::TemplatePolicy {
            entrypoint_template: "main".to_string(),
            module_template_family: "rust-module".to_string(),
            test_template_family: "rust-tests".to_string(),
            project_layout_policy: ProjectLayoutPolicy::CargoBinaryLib,
        },
        test_policy: code_language_core::stable_v03::TestPolicy {
            enabled: true,
            style: code_language_core::stable_v03::TestStyle::Native,
            conventions: vec!["cargo test".to_string()],
        },
    }
}

fn execution_language(language: CodeTargetLanguage) -> TargetLanguage {
    match language {
        CodeTargetLanguage::Rust => TargetLanguage::Rust,
        CodeTargetLanguage::Python => TargetLanguage::Python,
        CodeTargetLanguage::TypeScript => TargetLanguage::TypeScript,
    }
}

fn dependency_commands(language: CodeTargetLanguage) -> Vec<String> {
    match language {
        CodeTargetLanguage::Rust => vec!["rustc --version".to_string()],
        CodeTargetLanguage::Python => vec!["python3 --version".to_string()],
        CodeTargetLanguage::TypeScript => vec!["node --version".to_string()],
    }
}

fn build_commands(language: CodeTargetLanguage) -> Vec<String> {
    match language {
        CodeTargetLanguage::Rust => vec!["cargo build".to_string()],
        CodeTargetLanguage::Python => Vec::new(),
        CodeTargetLanguage::TypeScript => vec!["npm run build".to_string()],
    }
}

fn run_commands(
    language: CodeTargetLanguage,
    source_prefix: &str,
    entrypoint_template: &str,
) -> Vec<String> {
    match language {
        CodeTargetLanguage::Rust => vec!["cargo run".to_string()],
        CodeTargetLanguage::Python => {
            vec![format!("python3 -m {source_prefix}.{entrypoint_template}")]
        }
        CodeTargetLanguage::TypeScript => vec!["node dist/index.js".to_string()],
    }
}

fn test_commands(language: CodeTargetLanguage, context: &GenerationContext) -> Vec<String> {
    if !context.test_policy.conventions.is_empty() {
        return context.test_policy.conventions.clone();
    }
    match language {
        CodeTargetLanguage::Rust => vec!["cargo test".to_string()],
        CodeTargetLanguage::Python => vec!["pytest".to_string()],
        CodeTargetLanguage::TypeScript => vec!["npm test".to_string()],
    }
}

fn materialize_test_files(
    root_dir: &str,
    language: CodeTargetLanguage,
    tests_enabled: bool,
    test_files: Vec<GeneratedFile>,
) -> Vec<GeneratedFile> {
    if !test_files.is_empty() {
        return test_files
            .into_iter()
            .map(|file| GeneratedFile {
                path: format!(
                    "{root_dir}/tests/{}",
                    file.path.trim_start_matches("tests/")
                ),
                content: file.content,
            })
            .collect();
    }
    if !tests_enabled {
        return Vec::new();
    }
    vec![GeneratedFile {
        path: format!("{root_dir}/tests/{}", default_test_file(language)),
        content: render_test_stub(language),
    }]
}
