use code_language_core::stable_v03::{
    FrameworkProfile, InterfaceConvention, ProjectLayoutPolicy, TargetLanguage, TestConvention,
    default_generation_context, default_language_profile,
};
use test_generation_core::stable_v03::{
    DefaultStructuralTestGenerator, TestGenerator, TestKind, render_test_file, validate_test_suite,
};
use unified_design_ir::{
    FieldSpec, ImplementationUnit, InterfaceSpec, MethodSpec, StructSpec, TypeRef,
};

fn sample_unit() -> ImplementationUnit {
    ImplementationUnit {
        module_name: "user_service".to_string(),
        dependencies: vec!["repository".to_string()],
        public_interfaces: vec![InterfaceSpec {
            name: "UserServiceInterface".to_string(),
            methods: vec![MethodSpec {
                name: "execute".to_string(),
                inputs: vec![TypeRef::Primitive("String".to_string())],
                output: Some(TypeRef::Optional(Box::new(TypeRef::Custom(
                    "UserServiceResult".to_string(),
                )))),
            }],
        }],
        internal_structs: vec![StructSpec {
            name: "UserServiceState".to_string(),
            fields: vec![FieldSpec {
                name: "id".to_string(),
                ty: TypeRef::Primitive("String".to_string()),
            }],
        }],
        language_hint: Some("rust".to_string()),
        annotations: vec![],
    }
}

#[test]
fn structural_generation_is_deterministic() {
    let unit = sample_unit();
    let ctx = default_generation_context(TargetLanguage::Rust, None);
    let generator = DefaultStructuralTestGenerator;

    let lhs = generator.generate(&unit, &ctx);
    let rhs = generator.generate(&unit, &ctx);

    assert_eq!(lhs, rhs);
}

#[test]
fn structural_suite_covers_public_interfaces() {
    let unit = sample_unit();
    let ctx = default_generation_context(TargetLanguage::Rust, None);
    let suite = DefaultStructuralTestGenerator.generate(&unit, &ctx);

    validate_test_suite(&suite, &unit, &ctx).expect("suite should be valid");
    assert!(
        suite
            .test_cases
            .iter()
            .any(|case| matches!(case.kind, TestKind::Serialization))
    );
}

#[test]
fn framework_modules_receive_endpoint_tests() {
    let unit = sample_unit();
    let ctx = code_language_core::stable_v03::GenerationContext {
        language_profile: default_language_profile(TargetLanguage::Python),
        framework_profile: Some(FrameworkProfile {
            name: "fastapi".to_string(),
            project_layout: ProjectLayoutPolicy::PythonPackage,
            dependency_overrides: vec![],
            interface_conventions: InterfaceConvention {
                trait_prefix: "Api".to_string(),
                method_prefix: "route_".to_string(),
            },
            test_conventions: TestConvention {
                file_suffix: "_test.py".to_string(),
                command: "pytest".to_string(),
            },
        }),
        dependency_policy: default_generation_context(TargetLanguage::Python, None)
            .dependency_policy,
        template_policy: default_generation_context(TargetLanguage::Python, None).template_policy,
        test_policy: default_generation_context(TargetLanguage::Python, None).test_policy,
    };

    let suite = DefaultStructuralTestGenerator.generate(&unit, &ctx);

    assert!(
        suite
            .test_cases
            .iter()
            .any(|case| matches!(case.kind, TestKind::EndpointAvailability))
    );
}

#[test]
fn rendered_test_files_follow_phase6_paths() {
    let unit = sample_unit();
    let ctx = default_generation_context(TargetLanguage::TypeScript, None);
    let suite = DefaultStructuralTestGenerator.generate(&unit, &ctx);
    let file = render_test_file(&suite, &ctx);

    assert_eq!(file.path, "test_user_service.ts");
    assert!(file.content.contains("sourceText"));
}
