use code_language_core::stable_v03::{GeneratedFile, TargetLanguage, default_generation_context};
use implementation_core::stable_v03::{DefaultProjectGenerator, ProjectGenerator};

#[test]
fn generated_structural_tests_are_written_into_project_layout_and_plan() {
    let context = default_generation_context(TargetLanguage::Rust, None);
    let (layout, plan) = DefaultProjectGenerator.generate(
        "phase6_demo",
        vec![GeneratedFile {
            path: "src/user_service.rs".to_string(),
            content:
                "pub fn execute(arg_0: String) -> Option<UserServiceResult> { unimplemented!() }\n"
                    .to_string(),
        }],
        vec![context],
        vec![GeneratedFile {
            path: "test_user_service.rs".to_string(),
            content: "#[test]\nfn structural() { assert!(true); }\n".to_string(),
        }],
    );

    assert!(
        layout
            .files
            .iter()
            .any(|file| file.path == "phase6_demo/tests/test_user_service.rs")
    );
    assert_eq!(
        plan.test_plan.test_files,
        vec!["phase6_demo/tests/test_user_service.rs".to_string()]
    );
    assert_eq!(plan.project_root.to_string_lossy(), "phase6_demo");
}
