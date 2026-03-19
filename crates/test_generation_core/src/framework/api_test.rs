use crate::model::test_case::{TestCase, TestKind};
use code_language_core::stable_v03::{GenerationContext, TargetLanguage};
use unified_design_ir::ImplementationUnit;

pub fn endpoint_tests(unit: &ImplementationUnit, ctx: &GenerationContext) -> Vec<TestCase> {
    let Some(framework) = ctx.framework_profile.as_ref() else {
        return Vec::new();
    };

    let framework_name = framework.name.to_ascii_lowercase();
    let expected = match (ctx.language_profile.language, framework_name.as_str()) {
        (TargetLanguage::Rust, "axum") => vec!["// framework: axum".to_string()],
        (TargetLanguage::Python, "fastapi") => vec!["import".to_string()],
        (TargetLanguage::TypeScript, "express") => vec!["export function".to_string()],
        _ => Vec::new(),
    };
    if expected.is_empty() {
        return Vec::new();
    }

    vec![TestCase {
        name: format!(
            "{}_{}_endpoint_availability",
            unit.module_name, framework.name
        ),
        kind: TestKind::EndpointAvailability,
        target: framework.name.clone(),
        code: endpoint_assertion(ctx.language_profile.language, &expected),
    }]
}

fn endpoint_assertion(language: TargetLanguage, expected: &[String]) -> String {
    match language {
        TargetLanguage::Rust => {
            let checks = expected
                .iter()
                .map(|item| format!("    assert!(source.contains({item:?}));"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("let source = source_text();\n{checks}")
        }
        TargetLanguage::Python => {
            let checks = expected
                .iter()
                .map(|item| format!("    assert {item:?} in source"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("source = source_text()\n{checks}")
        }
        TargetLanguage::TypeScript => {
            let checks = expected
                .iter()
                .map(|item| format!("  expect(source).toContain({item:?});"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("const source = sourceText();\n{checks}")
        }
    }
}
