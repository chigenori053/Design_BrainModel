use code_language_core::stable_v03::{GeneratedFile, TargetLanguage, default_generation_context};
use implementation_core::stable_v03::{DefaultProjectGenerator, ProjectGenerator};

#[test]
fn project_layout_places_files_under_valid_structure() {
    let context = default_generation_context(TargetLanguage::Python, None);
    let (layout, plan) = DefaultProjectGenerator.generate(
        "demo",
        vec![GeneratedFile {
            path: "app/service.py".to_string(),
            content: "def run():\n    pass\n".to_string(),
        }],
        vec![context],
        Vec::new(),
    );

    assert_eq!(layout.root_dir, "demo");
    assert!(
        layout
            .files
            .iter()
            .any(|file| file.path == "demo/app/service.py")
    );
    assert!(
        layout
            .files
            .iter()
            .any(|file| file.path == "demo/pyproject.toml")
    );
    assert_eq!(plan.test_plan.test_commands[0], "pytest");
}
