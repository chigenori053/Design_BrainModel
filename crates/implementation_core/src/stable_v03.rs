use code_language_core::stable_v03::{
    DependencySpec, GeneratedFile, GenerationContext, ProjectLayoutPolicy, TargetLanguage,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub steps: Vec<String>,
    pub entrypoints: Vec<String>,
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

impl ExecutionPlan {
    pub fn is_valid(&self) -> bool {
        !self.steps.is_empty() && !self.entrypoints.is_empty()
    }
}

pub trait ProjectGenerator: Send + Sync {
    fn generate(
        &self,
        project_name: &str,
        files: Vec<GeneratedFile>,
        contexts: Vec<GenerationContext>,
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
            TargetLanguage::Rust => "Cargo.toml",
            TargetLanguage::Python => "pyproject.toml",
            TargetLanguage::TypeScript => "package.json",
        };
        let mut dependencies = contexts
            .iter()
            .flat_map(|ctx| {
                let mut items = ctx.dependency_policy.defaults.clone();
                items.extend(ctx.dependency_policy.framework_bound.clone());
                items
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name).then_with(|| lhs.version.cmp(&rhs.version)));
        dependencies.dedup_by(|lhs, rhs| lhs.name == rhs.name);

        let mut layout_files = files
            .into_iter()
            .map(|file| GeneratedFile {
                path: format!("{root_dir}/{source_prefix}/{}", trim_known_prefix(&file.path)),
                content: file.content,
            })
            .collect::<Vec<_>>();
        layout_files.push(GeneratedFile {
            path: format!("{root_dir}/{manifest_path}"),
            content: render_manifest(primary.language_profile.language, &dependencies),
        });
        if primary.test_policy.enabled {
            layout_files.push(GeneratedFile {
                path: format!(
                    "{root_dir}/tests/{}",
                    default_test_file(primary.language_profile.language)
                ),
                content: render_test_stub(primary.language_profile.language),
            });
        }

        let execution_plan = ExecutionPlan {
            steps: execution_steps(primary.language_profile.language),
            entrypoints: vec![format!(
                "{root_dir}/{source_prefix}/{}.{}",
                primary.template_policy.entrypoint_template,
                source_extension(primary.language_profile.language)
            )],
        };
        let project_layout = ProjectLayout {
            root_dir,
            files: layout_files,
            manifest_path: manifest_path.to_string(),
        };
        (project_layout, execution_plan)
    }
}

fn render_manifest(language: TargetLanguage, dependencies: &[DependencySpec]) -> String {
    match language {
        TargetLanguage::Rust => {
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
        TargetLanguage::Python => {
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
        TargetLanguage::TypeScript => {
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

fn execution_steps(language: TargetLanguage) -> Vec<String> {
    match language {
        TargetLanguage::Rust => vec!["cargo test".to_string(), "cargo run".to_string()],
        TargetLanguage::Python => vec!["pytest".to_string(), "python -m app.main".to_string()],
        TargetLanguage::TypeScript => vec!["npm test".to_string(), "npm run build".to_string()],
    }
}

fn source_extension(language: TargetLanguage) -> &'static str {
    match language {
        TargetLanguage::Rust => "rs",
        TargetLanguage::Python => "py",
        TargetLanguage::TypeScript => "ts",
    }
}

fn default_test_file(language: TargetLanguage) -> &'static str {
    match language {
        TargetLanguage::Rust => "smoke_test.rs",
        TargetLanguage::Python => "test_smoke.py",
        TargetLanguage::TypeScript => "smoke.spec.ts",
    }
}

fn render_test_stub(language: TargetLanguage) -> String {
    match language {
        TargetLanguage::Rust => "#[test]\nfn smoke() { assert!(true); }\n".to_string(),
        TargetLanguage::Python => "def test_smoke() -> None:\n    assert True\n".to_string(),
        TargetLanguage::TypeScript => "test('smoke', () => expect(true).toBe(true));\n".to_string(),
    }
}

fn trim_known_prefix(path: &str) -> String {
    path.trim_start_matches("src/")
        .trim_start_matches("app/")
        .to_string()
}

fn default_rust_context() -> GenerationContext {
    GenerationContext {
        language_profile: code_language_core::stable_v03::default_language_profile(TargetLanguage::Rust),
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
