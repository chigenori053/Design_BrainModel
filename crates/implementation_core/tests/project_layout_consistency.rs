use code_language_core::stable_v03::{
    default_generation_context, default_language_profile, DependencyPolicy, DependencySpec,
    GeneratedFile, TemplatePolicy, TestPolicy, TestStyle, FrameworkProfile, InterfaceConvention,
    ProjectLayoutPolicy, TargetLanguage, TestConvention,
};
use implementation_core::stable_v03::{DefaultProjectGenerator, ProjectGenerator};

fn context_with_framework(language: TargetLanguage, framework: &str, layout: ProjectLayoutPolicy, dependency: &str) -> code_language_core::stable_v03::GenerationContext {
    code_language_core::stable_v03::GenerationContext {
        language_profile: default_language_profile(language),
        framework_profile: Some(FrameworkProfile {
            name: framework.to_string(),
            project_layout: layout,
            dependency_overrides: vec![DependencySpec {
                name: dependency.to_string(),
                version: "1".to_string(),
            }],
            interface_conventions: InterfaceConvention {
                trait_prefix: "Api".to_string(),
                method_prefix: "handle".to_string(),
            },
            test_conventions: TestConvention {
                file_suffix: "_test".to_string(),
                command: match language {
                    TargetLanguage::Rust => "cargo test",
                    TargetLanguage::Python => "pytest",
                    TargetLanguage::TypeScript => "npm test",
                }
                .to_string(),
            },
        }),
        dependency_policy: DependencyPolicy {
            defaults: vec![DependencySpec {
                name: "core".to_string(),
                version: "1".to_string(),
            }],
            optional: vec![],
            framework_bound: vec![DependencySpec {
                name: dependency.to_string(),
                version: "1".to_string(),
            }],
        },
        template_policy: TemplatePolicy {
            entrypoint_template: match language {
                TargetLanguage::Rust => "main",
                TargetLanguage::Python => "app",
                TargetLanguage::TypeScript => "index",
            }
            .to_string(),
            module_template_family: "phase4".to_string(),
            test_template_family: "phase4-tests".to_string(),
            project_layout_policy: layout,
        },
        test_policy: TestPolicy {
            enabled: true,
            style: match language {
                TargetLanguage::Rust => TestStyle::Native,
                TargetLanguage::Python => TestStyle::Pytest,
                TargetLanguage::TypeScript => TestStyle::Jest,
            },
            conventions: vec![],
        },
    }
}

#[test]
fn framework_switch_changes_project_layout() {
    let files = vec![GeneratedFile {
        path: "src/api.rs".to_string(),
        content: "pub fn run() {}\n".to_string(),
    }];
    let (axum_layout, axum_plan) = DefaultProjectGenerator.generate(
        "demo_axum",
        files.clone(),
        vec![context_with_framework(
            TargetLanguage::Rust,
            "axum",
            ProjectLayoutPolicy::CargoBinaryLib,
            "axum",
        )],
    );
    let (fastapi_layout, fastapi_plan) = DefaultProjectGenerator.generate(
        "demo_fastapi",
        vec![GeneratedFile {
            path: "app/api.py".to_string(),
            content: "def run():\n    pass\n".to_string(),
        }],
        vec![context_with_framework(
            TargetLanguage::Python,
            "fastapi",
            ProjectLayoutPolicy::PythonPackage,
            "fastapi",
        )],
    );

    assert_ne!(axum_layout, fastapi_layout);
    assert_ne!(axum_plan, fastapi_plan);
    assert!(axum_layout.files.iter().any(|file| file.path.ends_with("Cargo.toml")));
    assert!(fastapi_layout.files.iter().any(|file| file.path.ends_with("pyproject.toml")));
}

#[test]
fn policy_consistency_matches_language_manifest_rules() {
    let (python_layout, _) = DefaultProjectGenerator.generate(
        "demo_python",
        vec![GeneratedFile {
            path: "app/service.py".to_string(),
            content: "def run():\n    pass\n".to_string(),
        }],
        vec![default_generation_context(TargetLanguage::Python, None)],
    );
    let (rust_layout, _) = DefaultProjectGenerator.generate(
        "demo_rust",
        vec![GeneratedFile {
            path: "src/service.rs".to_string(),
            content: "pub fn run() {}\n".to_string(),
        }],
        vec![default_generation_context(TargetLanguage::Rust, None)],
    );

    assert!(python_layout.is_valid());
    assert!(rust_layout.is_valid());
    assert!(python_layout.files.iter().any(|file| file.path.ends_with("pyproject.toml")));
    assert!(!python_layout.files.iter().any(|file| file.path.ends_with("Cargo.toml")));
    assert!(rust_layout.files.iter().any(|file| file.path.ends_with("Cargo.toml")));
    assert!(!rust_layout.files.iter().any(|file| file.path.ends_with("requirements.txt")));
}
