use crate::model::test_case::{TestCase, TestKind};
use code_language_core::stable_v03::{GenerationContext, TargetLanguage};
use unified_design_ir::{ImplementationUnit, InterfaceSpec, StructSpec};

pub fn interface_tests(unit: &ImplementationUnit, ctx: &GenerationContext) -> Vec<TestCase> {
    unit.public_interfaces
        .iter()
        .flat_map(|interface| {
            let interface_name = interface.name.clone();
            interface
                .methods
                .iter()
                .map(move |method| TestCase {
                    name: format!("{}_{}_interface_exists", unit.module_name, method.name),
                    kind: TestKind::InterfaceExistence,
                    target: interface_name.clone(),
                    code: contains_assertion(
                        ctx.language_profile.language,
                        unit,
                        &format!("pub fn {}(", render_method_name(method.name.as_str(), ctx)),
                        &format!("def {}(", render_method_name(method.name.as_str(), ctx)),
                        &format!(
                            "export function {}(",
                            render_method_name(method.name.as_str(), ctx)
                        ),
                    ),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn signature_tests(unit: &ImplementationUnit, ctx: &GenerationContext) -> Vec<TestCase> {
    unit.public_interfaces
        .iter()
        .flat_map(|interface| signature_tests_for_interface(unit, interface, ctx))
        .collect()
}

pub fn serialization_tests(unit: &ImplementationUnit, ctx: &GenerationContext) -> Vec<TestCase> {
    unit.internal_structs
        .iter()
        .map(|structure| TestCase {
            name: format!("{}_{}_serialization", unit.module_name, structure.name),
            kind: TestKind::Serialization,
            target: structure.name.clone(),
            code: serialization_assertion(ctx.language_profile.language, unit, structure),
        })
        .collect()
}

pub fn dependency_wiring_tests(
    unit: &ImplementationUnit,
    ctx: &GenerationContext,
) -> Vec<TestCase> {
    if unit.dependencies.is_empty() {
        return Vec::new();
    }
    let expected = unit
        .dependencies
        .iter()
        .map(|dependency| expected_dependency_token(ctx.language_profile.language, dependency))
        .collect::<Vec<_>>();
    vec![TestCase {
        name: format!("{}_dependency_wiring", unit.module_name),
        kind: TestKind::DependencyWiring,
        target: unit.module_name.clone(),
        code: multi_contains_assertion(ctx.language_profile.language, unit, &expected),
    }]
}

fn signature_tests_for_interface(
    unit: &ImplementationUnit,
    interface: &InterfaceSpec,
    ctx: &GenerationContext,
) -> Vec<TestCase> {
    interface
        .methods
        .iter()
        .map(|method| {
            let signature = match ctx.language_profile.language {
                TargetLanguage::Rust => {
                    let args = method
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| format!("arg_{index}: {}", rust_type_name(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    match &method.output {
                        Some(output) => format!(
                            "pub fn {}({args}) -> {}",
                            render_method_name(method.name.as_str(), ctx),
                            rust_type_name(output)
                        ),
                        None => format!(
                            "pub fn {}({args})",
                            render_method_name(method.name.as_str(), ctx)
                        ),
                    }
                }
                TargetLanguage::Python => {
                    let args = if method.inputs.is_empty() {
                        "self".to_string()
                    } else {
                        let inputs = method
                            .inputs
                            .iter()
                            .enumerate()
                            .map(|(index, ty)| format!("arg_{index}: {}", python_type_name(ty)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("self, {inputs}")
                    };
                    match &method.output {
                        Some(output) => format!(
                            "def {}({args}) -> {}",
                            render_method_name(method.name.as_str(), ctx),
                            python_type_name(output)
                        ),
                        None => format!(
                            "def {}({args})",
                            render_method_name(method.name.as_str(), ctx)
                        ),
                    }
                }
                TargetLanguage::TypeScript => {
                    let args = method
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| format!("arg_{index}: {}", typescript_type_name(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let output = method
                        .output
                        .as_ref()
                        .map(typescript_type_name)
                        .unwrap_or_else(|| "void".to_string());
                    format!(
                        "export function {}({args}): {output}",
                        render_method_name(method.name.as_str(), ctx)
                    )
                }
            };

            TestCase {
                name: format!("{}_{}_signature", unit.module_name, method.name),
                kind: TestKind::SignatureValidation,
                target: interface.name.clone(),
                code: contains_literal_assertion(ctx.language_profile.language, unit, &signature),
            }
        })
        .collect()
}

fn serialization_assertion(
    language: TargetLanguage,
    unit: &ImplementationUnit,
    structure: &StructSpec,
) -> String {
    let markers = structure
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();
    multi_contains_assertion(language, unit, &markers)
}

fn contains_assertion(
    language: TargetLanguage,
    unit: &ImplementationUnit,
    rust_expected: &str,
    python_expected: &str,
    typescript_expected: &str,
) -> String {
    match language {
        TargetLanguage::Rust => contains_literal_assertion(language, unit, rust_expected),
        TargetLanguage::Python => contains_literal_assertion(language, unit, python_expected),
        TargetLanguage::TypeScript => {
            contains_literal_assertion(language, unit, typescript_expected)
        }
    }
}

fn contains_literal_assertion(
    language: TargetLanguage,
    _unit: &ImplementationUnit,
    expected: &str,
) -> String {
    match language {
        TargetLanguage::Rust => {
            format!("let source = source_text();\n    assert!(source.contains({expected:?}));")
        }
        TargetLanguage::Python => {
            format!("source = source_text()\n    assert {expected:?} in source")
        }
        TargetLanguage::TypeScript => {
            format!("const source = sourceText();\n  expect(source).toContain({expected:?});")
        }
    }
}

fn multi_contains_assertion(
    language: TargetLanguage,
    _unit: &ImplementationUnit,
    expected: &[String],
) -> String {
    match language {
        TargetLanguage::Rust => {
            let lines = expected
                .iter()
                .map(|item| format!("    assert!(source.contains({item:?}));"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("let source = source_text();\n{lines}")
        }
        TargetLanguage::Python => {
            let lines = expected
                .iter()
                .map(|item| format!("    assert {item:?} in source"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("source = source_text()\n{lines}")
        }
        TargetLanguage::TypeScript => {
            let lines = expected
                .iter()
                .map(|item| format!("  expect(source).toContain({item:?});"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("const source = sourceText();\n{lines}")
        }
    }
}

fn expected_dependency_token(language: TargetLanguage, dependency: &str) -> String {
    match language {
        TargetLanguage::Rust => format!("crate::{dependency}"),
        TargetLanguage::Python => format!("import {dependency}"),
        TargetLanguage::TypeScript => format!("from './{dependency}'"),
    }
}

fn render_method_name(name: &str, ctx: &GenerationContext) -> String {
    apply_case_style(name, ctx.language_profile.naming_rules.method_case)
}

fn rust_type_name(ty: &unified_design_ir::TypeRef) -> String {
    match ty {
        unified_design_ir::TypeRef::Primitive(name) => match name.to_ascii_lowercase().as_str() {
            "int" => "i64".to_string(),
            "string" => "String".to_string(),
            "bool" => "bool".to_string(),
            other => other.to_string(),
        },
        unified_design_ir::TypeRef::Custom(name) => name.clone(),
        unified_design_ir::TypeRef::List(inner) => format!("Vec<{}>", rust_type_name(inner)),
        unified_design_ir::TypeRef::Optional(inner) => format!("Option<{}>", rust_type_name(inner)),
    }
}

fn python_type_name(ty: &unified_design_ir::TypeRef) -> String {
    match ty {
        unified_design_ir::TypeRef::Primitive(name) => match name.to_ascii_lowercase().as_str() {
            "int" => "int".to_string(),
            "string" => "str".to_string(),
            "bool" => "bool".to_string(),
            other => other.to_string(),
        },
        unified_design_ir::TypeRef::Custom(name) => name.clone(),
        unified_design_ir::TypeRef::List(inner) => format!("list[{}]", python_type_name(inner)),
        unified_design_ir::TypeRef::Optional(inner) => {
            format!("Optional[{}]", python_type_name(inner))
        }
    }
}

fn typescript_type_name(ty: &unified_design_ir::TypeRef) -> String {
    match ty {
        unified_design_ir::TypeRef::Primitive(name) => match name.to_ascii_lowercase().as_str() {
            "int" => "number".to_string(),
            "string" => "string".to_string(),
            "bool" => "boolean".to_string(),
            other => other.to_string(),
        },
        unified_design_ir::TypeRef::Custom(name) => name.clone(),
        unified_design_ir::TypeRef::List(inner) => format!("{}[]", typescript_type_name(inner)),
        unified_design_ir::TypeRef::Optional(inner) => {
            format!("{} | null", typescript_type_name(inner))
        }
    }
}

fn apply_case_style(value: &str, style: code_language_core::stable_v03::CaseStyle) -> String {
    let parts = value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return String::new();
    }
    match style {
        code_language_core::stable_v03::CaseStyle::SnakeCase => parts.join("_"),
        code_language_core::stable_v03::CaseStyle::PascalCase => parts
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
        code_language_core::stable_v03::CaseStyle::CamelCase => {
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
