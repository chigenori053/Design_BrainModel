use crate::generator::python_test_generator::PythonStructuralTestGenerator;
use crate::generator::rust_test_generator::RustStructuralTestGenerator;
use crate::generator::typescript_test_generator::TypeScriptStructuralTestGenerator;
use crate::model::test_suite::TestSuite;
use code_language_core::stable_v03::{GeneratedFile, GenerationContext, TargetLanguage};
use unified_design_ir::ImplementationUnit;

pub trait TestGenerator: Send + Sync {
    fn generate(&self, unit: &ImplementationUnit, ctx: &GenerationContext) -> TestSuite;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultStructuralTestGenerator;

impl TestGenerator for DefaultStructuralTestGenerator {
    fn generate(&self, unit: &ImplementationUnit, ctx: &GenerationContext) -> TestSuite {
        match ctx.language_profile.language {
            TargetLanguage::Rust => RustStructuralTestGenerator.generate(unit, ctx),
            TargetLanguage::Python => PythonStructuralTestGenerator.generate(unit, ctx),
            TargetLanguage::TypeScript => TypeScriptStructuralTestGenerator.generate(unit, ctx),
        }
    }
}

pub fn render_test_file(suite: &TestSuite, ctx: &GenerationContext) -> GeneratedFile {
    match ctx.language_profile.language {
        TargetLanguage::Rust => GeneratedFile {
            path: format!("test_{}.rs", suite.module_name),
            content: render_rust_suite(suite, ctx),
        },
        TargetLanguage::Python => GeneratedFile {
            path: format!("test_{}.py", suite.module_name),
            content: render_python_suite(suite, ctx),
        },
        TargetLanguage::TypeScript => GeneratedFile {
            path: format!("test_{}.ts", suite.module_name),
            content: render_typescript_suite(suite, ctx),
        },
    }
}

fn render_rust_suite(suite: &TestSuite, ctx: &GenerationContext) -> String {
    let source_path = format!(
        "../{}/{}.{}",
        ctx.language_profile.file_layout_rules.source_dir,
        apply_case_style(
            &suite.module_name,
            ctx.language_profile.naming_rules.module_case
        ),
        ctx.language_profile.file_layout_rules.extension
    );
    let tests = suite
        .test_cases
        .iter()
        .map(|case| {
            format!(
                "#[test]\nfn {}() {{\n    {}\n}}",
                sanitize_name(&case.name),
                case.code
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "fn source_text() -> &'static str {{\n    include_str!({source_path:?})\n}}\n\n{tests}\n"
    )
}

fn render_python_suite(suite: &TestSuite, ctx: &GenerationContext) -> String {
    let source_path = format!(
        "{}/{}.{}",
        ctx.language_profile.file_layout_rules.source_dir,
        apply_case_style(
            &suite.module_name,
            ctx.language_profile.naming_rules.module_case
        ),
        ctx.language_profile.file_layout_rules.extension
    );
    let tests = suite
        .test_cases
        .iter()
        .map(|case| {
            format!(
                "def {}() -> None:\n    {}\n",
                sanitize_name(&case.name),
                indent_python(&case.code, 1)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "from pathlib import Path\n\nSOURCE_PATH = Path(__file__).resolve().parents[1] / {source_path:?}\n\n\
def source_text() -> str:\n    return SOURCE_PATH.read_text(encoding=\"utf-8\")\n\n{tests}"
    )
}

fn render_typescript_suite(suite: &TestSuite, ctx: &GenerationContext) -> String {
    let source_path = format!(
        "{}/{}.{}",
        ctx.language_profile.file_layout_rules.source_dir,
        apply_case_style(
            &suite.module_name,
            ctx.language_profile.naming_rules.module_case
        ),
        ctx.language_profile.file_layout_rules.extension
    );
    let tests = suite
        .test_cases
        .iter()
        .map(|case| format!("test({:?}, () => {{\n  {}\n}});", case.name, case.code))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "import fs from 'node:fs';\nimport path from 'node:path';\n\n\
const SOURCE_PATH = path.join(__dirname, '..', {source_path:?});\n\n\
function sourceText(): string {{\n  return fs.readFileSync(SOURCE_PATH, 'utf8');\n}}\n\n{tests}\n"
    )
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn indent_python(value: &str, level: usize) -> String {
    let indent = "    ".repeat(level);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
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
