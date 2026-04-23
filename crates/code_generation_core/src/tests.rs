use code_ir::{IrStep, IrFunction, IrParam, IrType, CodeIr};
use crate::{
    error::CodegenError,
    generator::{generate_function, generate_program, StructuredCodeGenerator},
    scope::{ScopeStack, validate_steps},
    spec::LanguageSpec,
    type_render::render_type,
};

// ── Step3 helpers ─────────────────────────────────────────────────────────────

fn rust_gen() -> StructuredCodeGenerator {
    StructuredCodeGenerator::new(LanguageSpec::rust())
}

fn python_gen() -> StructuredCodeGenerator {
    StructuredCodeGenerator::new(LanguageSpec::python())
}

// ════════════════════════════════════════════════════════════════════════════
// Step3 tests (preserved)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn rust_if_no_else() {
    let steps = vec![IrStep::branch("x > 0", vec![IrStep::return_val("x")], None)];
    let got = rust_gen().emit_steps(&steps);
    let want = "if x > 0 {\n    return x;\n}\n";
    assert_eq!(got, want);
}

#[test]
fn rust_if_else() {
    let steps = vec![IrStep::branch(
        "flag",
        vec![IrStep::assign("a", "1")],
        Some(vec![IrStep::assign("a", "0")]),
    )];
    let got = rust_gen().emit_steps(&steps);
    let want = "if flag {\n    let a = 1;\n} else {\n    let a = 0;\n}\n";
    assert_eq!(got, want);
}

#[test]
fn python_if_no_else() {
    let steps = vec![IrStep::branch("x > 0", vec![IrStep::return_val("x")], None)];
    let got = python_gen().emit_steps(&steps);
    let want = "if x > 0:\n    return x;\n";
    assert_eq!(got, want);
}

#[test]
fn python_if_else() {
    let steps = vec![IrStep::branch(
        "flag",
        vec![IrStep::assign("a", "1")],
        Some(vec![IrStep::assign("a", "0")]),
    )];
    let got = python_gen().emit_steps(&steps);
    let want = "if flag:\n    let a = 1;\nelse:\n    let a = 0;\n";
    assert_eq!(got, want);
}

#[test]
fn rust_nested_if() {
    let inner = IrStep::branch("y > 0", vec![IrStep::return_val("y")], None);
    let outer = IrStep::branch("x > 0", vec![inner], None);
    let got = rust_gen().emit_steps(&[outer]);
    let want = "if x > 0 {\n    if y > 0 {\n        return y;\n    }\n}\n";
    assert_eq!(got, want);
}

#[test]
fn python_nested_if() {
    let inner = IrStep::branch("y > 0", vec![IrStep::return_val("y")], None);
    let outer = IrStep::branch("x > 0", vec![inner], None);
    let got = python_gen().emit_steps(&[outer]);
    let want = "if x > 0:\n    if y > 0:\n        return y;\n";
    assert_eq!(got, want);
}

#[test]
fn rust_for_loop() {
    let steps = vec![IrStep::loop_step(
        "i in items",
        vec![IrStep::call("process", vec!["i"])],
    )];
    let got = rust_gen().emit_steps(&steps);
    let want = "for i in items {\n    process(i);\n}\n";
    assert_eq!(got, want);
}

#[test]
fn python_for_loop() {
    let steps = vec![IrStep::loop_step(
        "i in items",
        vec![IrStep::call("process", vec!["i"])],
    )];
    let got = python_gen().emit_steps(&steps);
    let want = "for i in items:\n    process(i);\n";
    assert_eq!(got, want);
}

#[test]
fn indentation_level_correctness() {
    let steps = vec![
        IrStep::assign("x", "1"),
        IrStep::branch(
            "x > 0",
            vec![
                IrStep::assign("y", "x"),
                IrStep::loop_step("i in 0..y", vec![IrStep::call("f", vec!["i"])]),
            ],
            None,
        ),
        IrStep::return_val("x"),
    ];
    let got = rust_gen().emit_steps(&steps);
    let want = concat!(
        "let x = 1;\n",
        "if x > 0 {\n",
        "    let y = x;\n",
        "    for i in 0..y {\n",
        "        f(i);\n",
        "    }\n",
        "}\n",
        "return x;\n",
    );
    assert_eq!(got, want);
}

#[test]
fn snapshot_rust_block_scope() {
    let steps = vec![IrStep::block(vec![
        IrStep::assign("tmp", "compute()"),
        IrStep::return_val("tmp"),
    ])];
    let got = rust_gen().emit_steps(&steps);
    let want = "{\n    let tmp = compute();\n    return tmp;\n}\n";
    assert_eq!(got, want);
}

#[test]
fn snapshot_python_block_scope_passthrough() {
    let steps = vec![IrStep::block(vec![
        IrStep::assign("tmp", "compute()"),
        IrStep::return_val("tmp"),
    ])];
    let got = python_gen().emit_steps(&steps);
    let want = "    let tmp = compute();\n    return tmp;\n";
    assert_eq!(got, want);
}

#[test]
fn snapshot_full_rust_function_body() {
    let steps = vec![
        IrStep::assign("result", "Vec::new()"),
        IrStep::loop_step(
            "item in input",
            vec![IrStep::branch(
                "item.is_valid()",
                vec![IrStep::call("result.push", vec!["item"])],
                None,
            )],
        ),
        IrStep::return_val("result"),
    ];
    let got = rust_gen().emit_steps(&steps);
    let want = concat!(
        "let result = Vec::new();\n",
        "for item in input {\n",
        "    if item.is_valid() {\n",
        "        result.push(item);\n",
        "    }\n",
        "}\n",
        "return result;\n",
    );
    assert_eq!(got, want);
}

#[test]
fn snapshot_full_python_function_body() {
    let steps = vec![
        IrStep::assign("result", "[]"),
        IrStep::loop_step(
            "item in input",
            vec![IrStep::branch(
                "item.is_valid()",
                vec![IrStep::call("result.append", vec!["item"])],
                None,
            )],
        ),
        IrStep::return_val("result"),
    ];
    let got = python_gen().emit_steps(&steps);
    let want = concat!(
        "let result = [];\n",
        "for item in input:\n",
        "    if item.is_valid():\n",
        "        result.append(item);\n",
        "return result;\n",
    );
    assert_eq!(got, want);
}

// ════════════════════════════════════════════════════════════════════════════
// Step4 tests
// ════════════════════════════════════════════════════════════════════════════

// ── 11.1 Function Rendering ───────────────────────────────────────────────────

#[test]
fn rust_simple_function_no_args_no_return() {
    let func = IrFunction::new("hello")
        .with_body(vec![IrStep::call("println", vec!["\"hi\""])]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    let want = "fn hello() {\n    println(\"hi\");\n}\n";
    assert_eq!(got, want);
}

#[test]
fn python_simple_function_no_args_no_return() {
    let func = IrFunction::new("hello")
        .with_body(vec![IrStep::call("print", vec!["\"hi\""])]);
    let got = generate_function(&func, &LanguageSpec::python()).unwrap();
    let want = "def hello():\n    print(\"hi\");\n";
    assert_eq!(got, want);
}

// ── 11.2 Parameter Rendering ─────────────────────────────────────────────────

#[test]
fn rust_function_with_typed_params() {
    let func = IrFunction::new("add")
        .with_params(vec![
            IrParam::typed("a", IrType::Int),
            IrParam::typed("b", IrType::Int),
        ])
        .with_return_type(IrType::Int)
        .with_body(vec![IrStep::return_val("a")]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    let want = "fn add(a: i64, b: i64) -> i64 {\n    return a;\n}\n";
    assert_eq!(got, want);
}

#[test]
fn python_function_with_params_no_types() {
    let func = IrFunction::new("add")
        .with_params(vec![
            IrParam::new("a"),
            IrParam::new("b"),
        ])
        .with_body(vec![IrStep::return_val("a")]);
    let got = generate_function(&func, &LanguageSpec::python()).unwrap();
    // Python: no type annotations by default
    let want = "def add(a, b):\n    return a;\n";
    assert_eq!(got, want);
}

// ── 11.3 Return Type ─────────────────────────────────────────────────────────

#[test]
fn rust_bool_return_type() {
    let func = IrFunction::new("is_ok")
        .with_return_type(IrType::Bool)
        .with_body(vec![IrStep::return_val("true")]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    assert!(got.contains("-> bool"), "got: {got}");
}

#[test]
fn rust_void_return_type_omitted() {
    let func = IrFunction::new("noop")
        .with_return_type(IrType::Void)
        .with_body(vec![]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    // Void return type should be omitted in Rust
    assert!(!got.contains("->"), "got: {got}");
}

#[test]
fn rust_string_return_type() {
    let func = IrFunction::new("greet")
        .with_params(vec![IrParam::typed("s", IrType::Str)])
        .with_return_type(IrType::Str)
        .with_body(vec![IrStep::return_val("s")]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    assert!(got.contains("-> String"), "got: {got}");
}

#[test]
fn python_return_annotation() {
    let func = IrFunction::new("count")
        .with_params(vec![IrParam::new("n")])
        .with_return_type(IrType::Int)
        .with_body(vec![IrStep::return_val("n")]);
    let got = generate_function(&func, &LanguageSpec::python()).unwrap();
    assert!(got.contains("-> int"), "got: {got}");
}

// ── 11.4 Scope Resolution ────────────────────────────────────────────────────

#[test]
fn scope_resolves_param_in_body() {
    let func = IrFunction::new("double")
        .with_params(vec![IrParam::typed("x", IrType::Int)])
        .with_return_type(IrType::Int)
        .with_body(vec![IrStep::return_val("x")]);
    // `x` is a param — should resolve without error
    let result = generate_function(&func, &LanguageSpec::rust());
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn scope_resolves_local_in_nested_block() {
    let func = IrFunction::new("f")
        .with_body(vec![
            IrStep::assign("v", "42"),
            IrStep::branch("v", vec![IrStep::return_val("v")], None),
        ]);
    let result = generate_function(&func, &LanguageSpec::rust());
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn scope_inner_can_reference_outer() {
    let mut scope = ScopeStack::new();
    scope.declare("outer", None).unwrap();
    scope.push_scope();
    scope.declare("inner", None).unwrap();
    assert!(scope.is_resolved("outer"));
    assert!(scope.is_resolved("inner"));
    scope.pop_scope();
    assert!(scope.is_resolved("outer"));
    assert!(!scope.is_resolved("inner"));
}

// ── 11.5 Shadowing ───────────────────────────────────────────────────────────

#[test]
fn shadowing_allowed_in_inner_scope() {
    let mut scope = ScopeStack::new();
    scope.declare("x", None).unwrap();
    scope.push_scope();
    // Same name in inner scope — shadowing is OK
    let result = scope.declare("x", None);
    assert!(result.is_ok());
    // Inner binding found
    let b = scope.resolve("x").unwrap();
    assert_eq!(b.depth, 1);
}

#[test]
fn duplicate_in_same_scope_is_error() {
    let mut scope = ScopeStack::new();
    scope.declare("x", None).unwrap();
    let err = scope.declare("x", None).unwrap_err();
    assert_eq!(err, CodegenError::DuplicateBinding { name: "x".to_string(), depth: 0 });
}

// ── 11.6 Error Cases ─────────────────────────────────────────────────────────

#[test]
fn error_empty_function_name() {
    let func = IrFunction::new("");
    let err = generate_function(&func, &LanguageSpec::rust()).unwrap_err();
    assert_eq!(err, CodegenError::EmptyFunctionName);
}

#[test]
fn error_empty_param_name() {
    let func = IrFunction::new("f")
        .with_params(vec![IrParam::new("")]);
    let err = generate_function(&func, &LanguageSpec::rust()).unwrap_err();
    assert_eq!(err, CodegenError::EmptyParamName { function: "f".to_string() });
}

#[test]
fn error_duplicate_param() {
    let func = IrFunction::new("f")
        .with_params(vec![IrParam::new("x"), IrParam::new("x")]);
    let err = generate_function(&func, &LanguageSpec::rust()).unwrap_err();
    assert_eq!(err, CodegenError::DuplicateParam {
        function: "f".to_string(),
        param: "x".to_string(),
    });
}

#[test]
fn error_unresolved_variable() {
    let steps = vec![IrStep::return_val("ghost")];
    let mut scope = ScopeStack::new();
    let errors = validate_steps(&steps, &mut scope);
    assert_eq!(errors, vec![CodegenError::UnresolvedVariable { name: "ghost".to_string() }]);
}

#[test]
fn error_duplicate_binding_in_same_scope() {
    let steps = vec![
        IrStep::assign("x", "1"),
        IrStep::assign("x", "2"),
    ];
    let mut scope = ScopeStack::new();
    let errors = validate_steps(&steps, &mut scope);
    assert_eq!(errors, vec![CodegenError::DuplicateBinding { name: "x".to_string(), depth: 0 }]);
}

#[test]
fn no_error_for_literal_expressions_in_assign() {
    // "Vec::new()" contains "::" — should not be treated as variable reference
    let steps = vec![IrStep::assign("result", "Vec::new()")];
    let mut scope = ScopeStack::new();
    let errors = validate_steps(&steps, &mut scope);
    assert!(errors.is_empty(), "{errors:?}");
}

// ── 11.7 Snapshot ─────────────────────────────────────────────────────────────

#[test]
fn snapshot_rust_full_function() {
    let func = IrFunction::new("sum")
        .with_params(vec![
            IrParam::typed("a", IrType::Int),
            IrParam::typed("b", IrType::Int),
        ])
        .with_return_type(IrType::Int)
        .with_body(vec![
            IrStep::assign("result", "a"),
            IrStep::return_val("result"),
        ]);
    let got = generate_function(&func, &LanguageSpec::rust()).unwrap();
    let want = concat!(
        "fn sum(a: i64, b: i64) -> i64 {\n",
        "    let result = a;\n",
        "    return result;\n",
        "}\n",
    );
    assert_eq!(got, want);
}

#[test]
fn snapshot_python_full_function() {
    let func = IrFunction::new("greet")
        .with_params(vec![IrParam::new("name")])
        .with_body(vec![
            IrStep::assign("msg", "name"),
            IrStep::return_val("msg"),
        ]);
    let got = generate_function(&func, &LanguageSpec::python()).unwrap();
    let want = concat!(
        "def greet(name):\n",
        "    let msg = name;\n",
        "    return msg;\n",
    );
    assert_eq!(got, want);
}

#[test]
fn snapshot_rust_program_two_functions() {
    let mut ir = CodeIr::default();
    ir.functions = vec![
        IrFunction::new("a").with_body(vec![IrStep::return_val("1")]),
        IrFunction::new("b").with_body(vec![IrStep::return_val("2")]),
    ];
    let got = generate_program(&ir, &LanguageSpec::rust()).unwrap();
    let want = concat!(
        "fn a() {\n",
        "    return 1;\n",
        "}\n",
        "\n",
        "fn b() {\n",
        "    return 2;\n",
        "}\n",
        "\n",
    );
    assert_eq!(got, want);
}

// ── Type renderer ─────────────────────────────────────────────────────────────

#[test]
fn rust_type_rendering() {
    let spec = LanguageSpec::rust();
    assert_eq!(render_type(&IrType::Int, &spec).unwrap(), "i64");
    assert_eq!(render_type(&IrType::Float, &spec).unwrap(), "f64");
    assert_eq!(render_type(&IrType::Bool, &spec).unwrap(), "bool");
    assert_eq!(render_type(&IrType::Str, &spec).unwrap(), "String");
    assert_eq!(render_type(&IrType::Void, &spec).unwrap(), "()");
    assert_eq!(render_type(&IrType::Custom("MyStruct".into()), &spec).unwrap(), "MyStruct");
}

#[test]
fn python_type_rendering() {
    let spec = LanguageSpec::python();
    assert_eq!(render_type(&IrType::Int, &spec).unwrap(), "int");
    assert_eq!(render_type(&IrType::Float, &spec).unwrap(), "float");
    assert_eq!(render_type(&IrType::Bool, &spec).unwrap(), "bool");
    assert_eq!(render_type(&IrType::Str, &spec).unwrap(), "str");
    assert_eq!(render_type(&IrType::Void, &spec).unwrap(), "None");
}
