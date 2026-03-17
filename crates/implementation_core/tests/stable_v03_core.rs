use code_language_core::stable_v03::{
    default_generation_context, GeneratedFile, TargetLanguage,
};
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
    );

    assert_eq!(layout.root_dir, "demo");
    assert!(layout.files.iter().any(|file| file.path == "demo/app/service.py"));
    assert!(layout.files.iter().any(|file| file.path == "demo/pyproject.toml"));
    assert_eq!(plan.steps[0], "pytest");
}
