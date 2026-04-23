mod common;
use codegen_core::*;

#[test]
fn missing_pattern_returns_error() {
    let ir = CodeIr::new(vec![common::call_step("f", &["x"])]);
    // Spec only covers Assign, not Call.
    let spec = LanguageSpec::new(
        vec![(
            IrOp::Assign,
            CodePattern {
                emit: EmitNode::Sequence(vec![
                    EmitNode::Placeholder(Placeholder::VarName),
                    EmitNode::NewLine,
                ]),
            },
        )],
        Formatting::default(),
    );
    assert_eq!(
        generate_code(&ir, &spec).unwrap_err(),
        CodegenError::MissingPattern(IrOp::Call),
    );
}

#[test]
fn unresolved_varname_returns_error() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Assign,
        target: None,
        inputs: vec!["42".to_string()],
        outputs: vec![], // missing output
        condition: None,
    }]);
    assert_eq!(
        generate_code(&ir, &common::rust_spec()).unwrap_err(),
        CodegenError::UnresolvedPlaceholder(Placeholder::VarName),
    );
}

#[test]
fn unresolved_funcname_returns_error() {
    let ir = CodeIr::new(vec![IrStep {
        op: IrOp::Call,
        target: None, // missing target
        inputs: vec!["x".to_string()],
        outputs: vec![],
        condition: None,
    }]);
    assert_eq!(
        generate_code(&ir, &common::rust_spec()).unwrap_err(),
        CodegenError::UnresolvedPlaceholder(Placeholder::FuncName),
    );
}

#[test]
fn empty_spec_returns_missing_pattern() {
    let ir = CodeIr::new(vec![common::assign_step("x", "1")]);
    let spec = LanguageSpec::new(vec![], Formatting::default());
    assert!(matches!(
        generate_code(&ir, &spec).unwrap_err(),
        CodegenError::MissingPattern(_),
    ));
}
