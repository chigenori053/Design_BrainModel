mod common;
use codegen_core::*;

#[test]
fn assign_rust_snapshot() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Assign,
        target: None,
        inputs: vec!["42".to_string()],
        outputs: vec!["x".to_string()],
        condition: None,
    }]);
    assert_eq!(generate_code(&ir, &common::rust_spec()).unwrap(), "let x = 42;\n");
}

#[test]
fn call_rust_snapshot() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Call,
        target: Some("println".to_string()),
        inputs: vec!["x".to_string(), "y".to_string()],
        outputs: vec![],
        condition: None,
    }]);
    assert_eq!(generate_code(&ir, &common::rust_spec()).unwrap(), "println(x, y);\n");
}

#[test]
fn multi_step_rust_snapshot() {
    let ir = CodeIr::new(vec![
        common::assign_step("count", "10"),
        common::call_step("process", &["count"]),
    ]);
    assert_eq!(
        generate_code(&ir, &common::rust_spec()).unwrap(),
        "let count = 10;\nprocess(count);\n",
    );
}

#[test]
fn call_no_args_rust_snapshot() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Call,
        target: Some("init".to_string()),
        inputs: vec![],
        outputs: vec![],
        condition: None,
    }]);
    assert_eq!(generate_code(&ir, &common::rust_spec()).unwrap(), "init();\n");
}

#[test]
fn assign_python_snapshot() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Assign,
        target: None,
        inputs: vec!["1".to_string()],
        outputs: vec!["x".to_string()],
        condition: None,
    }]);
    assert_eq!(generate_code(&ir, &common::python_spec()).unwrap(), "x = 1\n");
}

#[test]
fn call_python_snapshot() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Call,
        target: Some("print".to_string()),
        inputs: vec!["x".to_string()],
        outputs: vec![],
        condition: None,
    }]);
    assert_eq!(generate_code(&ir, &common::python_spec()).unwrap(), "print(x)\n");
}
