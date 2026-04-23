mod common;
use codegen_core::*;

fn shared_ir() -> CodeIr {
    CodeIr::new(vec![
        common::assign_step("result", "0"),
        common::call_step("compute", &["result"]),
    ])
}

#[test]
fn same_ir_different_specs_produce_different_output() {
    let ir = shared_ir();
    let rust_code = generate_code(&ir, &common::rust_spec()).unwrap();
    let python_code = generate_code(&ir, &common::python_spec()).unwrap();
    assert_ne!(rust_code, python_code);
}

#[test]
fn rust_uses_let_keyword() {
    let ir = CodeIr::new(vec![common::assign_step("x", "1")]);
    let code = generate_code(&ir, &common::rust_spec()).unwrap();
    assert!(code.starts_with("let "));
}

#[test]
fn python_omits_let_keyword() {
    let ir = CodeIr::new(vec![common::assign_step("x", "1")]);
    let code = generate_code(&ir, &common::python_spec()).unwrap();
    assert!(!code.contains("let "));
    assert!(code.starts_with("x = "));
}

#[test]
fn rust_call_has_semicolon() {
    let ir = CodeIr::new(vec![common::call_step("f", &["a"])]);
    let code = generate_code(&ir, &common::rust_spec()).unwrap();
    assert!(code.contains(");"));
}

#[test]
fn python_call_has_no_semicolon() {
    let ir = CodeIr::new(vec![common::call_step("f", &["a"])]);
    let code = generate_code(&ir, &common::python_spec()).unwrap();
    assert!(!code.contains(";"));
}
