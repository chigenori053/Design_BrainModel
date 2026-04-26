use code_ir::program_v1::{BackendLanguage, Program, Statement};
use code_parser::{
    ParseErrorKind, SupportedLanguage, parse_source_to_ast, parse_source_to_ir, roundtrip_ir,
};

#[test]
fn rust_code_parses_to_ast_and_ir() {
    let source = r#"
use crate::math;

pub fn compute(value: Int) -> Int {
    let result = add(value, 1);
    if ready {
        return result;
    } else {
        return value;
    }
}
"#;

    let ast = parse_source_to_ast(SupportedLanguage::Rust, "sample", source).expect("ast");
    assert_eq!(ast.functions.len(), 1);
    let ir = parse_source_to_ir(SupportedLanguage::Rust, "sample", source).expect("ir");
    assert_eq!(ir.modules.len(), 1);
    assert_eq!(ir.modules[0].functions[0].name, "compute");
}

#[test]
fn python_code_parses_to_ast_and_ir() {
    let source = r#"
import math

def compute(value: Int) -> Int:
    result = add(value, 1)
    while ready:
        return result
"#;

    let ast = parse_source_to_ast(SupportedLanguage::Python, "sample", source).expect("ast");
    assert_eq!(ast.functions.len(), 1);
    let ir = parse_source_to_ir(SupportedLanguage::Python, "sample", source).expect("ir");
    assert_eq!(ir.modules[0].imports, vec!["import math".to_string()]);
}

#[test]
fn rust_roundtrip_is_deterministic() {
    let source = r#"
use crate::math;

pub fn compute(value: Int) -> Int {
    let result = add(value, 1);
    for item in items {
        process(item);
    }
    return result;
}
"#;

    let ir = parse_source_to_ir(SupportedLanguage::Rust, "sample", source).expect("ir");
    let generated = ir.render_canonical_source_tree(BackendLanguage::Rust);
    assert_eq!(generated.len(), 1);
    let expected = r#"use crate::math;

pub fn compute(value: Int) -> Int {
    let result = add(value, 1);
    for item in items {
        process(item);
    }
    return result;
}
"#;
    assert_eq!(generated[0].1, expected);
    let reparsed = roundtrip_ir(SupportedLanguage::Rust, &ir).expect("roundtrip");
    assert_eq!(reparsed, ir);
}

#[test]
fn python_roundtrip_is_deterministic() {
    let source = r#"
from math import sqrt

def compute(value: Int) -> Int:
    result = add(value, 1)
    if ready:
        return result
    else:
        return value
"#;

    let ir = parse_source_to_ir(SupportedLanguage::Python, "sample", source).expect("ir");
    let generated = ir.render_canonical_source_tree(BackendLanguage::Python);
    assert_eq!(generated.len(), 1);
    let expected = r#"from math import sqrt

def compute(value: Int) -> Int:
    result = add(value, 1)
    if ready:
        return result
    else:
        return value
"#;
    assert_eq!(generated[0].1, expected);
    let reparsed = roundtrip_ir(SupportedLanguage::Python, &ir).expect("roundtrip");
    assert_eq!(reparsed, ir);
}

#[test]
fn invalid_code_returns_error() {
    let err = parse_source_to_ir(
        SupportedLanguage::Rust,
        "sample",
        "pub fn broken(value: Int) -> Int {\n    match value {\n    }\n}\n",
    )
    .expect_err("must fail");
    assert_eq!(err.kind, ParseErrorKind::ParseError);
}

#[test]
fn ir_preserves_step_order() {
    let ir = parse_source_to_ir(
        SupportedLanguage::Python,
        "sample",
        "def compute(value: Int) -> Int:\n    first = prepare(value)\n    second = refine(first)\n    return second\n",
    )
    .expect("ir");

    let statements = &ir.modules[0].functions[0].body.statements;
    assert!(matches!(statements[0], Statement::Assign { .. }));
    assert!(matches!(statements[1], Statement::Assign { .. }));
    assert!(matches!(statements[2], Statement::Return { .. }));
}

#[test]
fn canonical_generation_is_parseable_as_program() {
    let mut program = Program::new("sample");
    let ir = parse_source_to_ir(
        SupportedLanguage::Rust,
        "sample",
        "pub fn ping() -> Int {\n    return 1;\n}\n",
    )
    .expect("ir");
    program.modules = ir.modules;
    let generated = program.render_canonical_source_tree(BackendLanguage::Rust);
    let reparsed =
        parse_source_to_ir(SupportedLanguage::Rust, "sample", &generated[0].1).expect("parse");
    assert_eq!(reparsed.modules.len(), 1);
}
