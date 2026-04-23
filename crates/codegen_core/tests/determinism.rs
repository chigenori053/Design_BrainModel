mod common;
use codegen_core::*;

#[test]
fn same_ir_produces_same_output_repeated() {
    let ir = CodeIr::new(vec![
        common::assign_step("x", "42"),
        common::call_step("println", &["x"]),
    ]);
    let spec = common::rust_spec();

    let first = generate_code(&ir, &spec).unwrap();
    for _ in 0..9 {
        assert_eq!(generate_code(&ir, &spec).unwrap(), first);
    }
}

#[test]
fn ir_step_order_is_preserved() {
    let spec = common::rust_spec();

    let ir_ab = CodeIr::new(vec![
        common::assign_step("a", "1"),
        common::assign_step("b", "2"),
    ]);
    let ir_ba = CodeIr::new(vec![
        common::assign_step("b", "2"),
        common::assign_step("a", "1"),
    ]);

    assert_ne!(
        generate_code(&ir_ab, &spec).unwrap(),
        generate_code(&ir_ba, &spec).unwrap(),
    );
    assert_eq!(generate_code(&ir_ab, &spec).unwrap(), "let a = 1;\nlet b = 2;\n");
    assert_eq!(generate_code(&ir_ba, &spec).unwrap(), "let b = 2;\nlet a = 1;\n");
}

#[test]
fn empty_ir_produces_empty_string() {
    let ir = CodeIr::new(vec![]);
    let spec = common::rust_spec();
    assert_eq!(generate_code(&ir, &spec).unwrap(), "");
}
