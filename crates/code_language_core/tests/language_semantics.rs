use code_ir::program_v1::{
    BinaryOp, BinaryOperator, Block, Effect, Expression, Function, FunctionInput, FunctionOutput,
    Module, Program, Statement, TargetDomain, TypeRef, Visibility,
};
use code_language_core::stable_v03::{
    dynamic_ir::{
        DefaultRuleValidator, DynamicInterpreter, EvaluationResult, LanguageInterpreter,
        LearningEngine, MappingRule, MemoryRuleRecord, RuleEngine, RuleSource, RuleStore,
        RuleValidator, SemanticCondition, TargetScope, TransformTemplate, ValidationCheck,
        ValidationContext, bootstrap_rule_store, lower_program_with_profile,
        promote_validated_rule, python_profile, resolve_profile, resolve_profile_from_memory,
        rollback_rule, rust_profile, select_rule_records, should_promote,
        should_promote_validated, ts_profile, validate_candidate_rule,
    },
    DefaultSemanticValidator, LanguageBackend, PythonBackend, PythonRenderer, Renderer, RustBackend,
    RustRenderer, SafeGenerationError, TypeScriptBackend, TypeScriptRenderer, Validator,
    safe_generate_program,
};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};

fn sample_program(effects: Vec<Effect>) -> Program {
    Program {
        metadata: code_ir::program_v1::Metadata {
            name: "Example".to_string(),
            version: "1.0.0".to_string(),
            target_domains: vec![TargetDomain::Backend],
        },
        modules: vec![Module {
            name: "main".to_string(),
            visibility: Visibility::Public,
            imports: Vec::new(),
            types: Vec::new(),
            functions: vec![Function {
                name: "add".to_string(),
                visibility: Visibility::Public,
                inputs: vec![
                    FunctionInput {
                        name: "a".to_string(),
                        r#type: TypeRef::primitive("Int"),
                        borrow: Some(code_ir::program_v1::BorrowInfo {
                            is_mut: effects.contains(&Effect::Mutation),
                            lifetime: None,
                        }),
                    },
                    FunctionInput {
                        name: "b".to_string(),
                        r#type: TypeRef::primitive("Int"),
                        borrow: None,
                    },
                ],
                outputs: FunctionOutput {
                    r#type: TypeRef::primitive("Int"),
                },
                effects,
                can_fail: false,
                body: Block {
                    statements: vec![Statement::Return {
                        value: Some(Expression::BinaryOp(BinaryOp {
                            op: BinaryOperator::Add,
                            left: Box::new(Expression::Variable("a".to_string())),
                            right: Box::new(Expression::Variable("b".to_string())),
                        })),
                    }],
                },
            }],
            state: Vec::new(),
        }],
        dependencies: Vec::new(),
        generation_strategy: code_ir::program_v1::GenerationStrategy::default(),
        build_validation: code_ir::program_v1::BuildValidation::for_backend(
            code_ir::program_v1::BackendLanguage::Rust,
        ),
    }
}

#[test]
fn rust_backend_lowers_async_and_mutation_semantics() {
    let program = sample_program(vec![Effect::Async, Effect::Mutation]);
    let lowered = RustBackend.lower_program(&program);
    let rendered = RustRenderer.render_program(&lowered);

    assert!(rendered.contains("pub async fn add"));
    assert!(rendered.contains("a: &mut i32"));
    assert!(rendered.contains("return a + b"));
}

#[test]
fn python_backend_renders_async_function() {
    let program = sample_program(vec![Effect::Async]);
    let lowered = PythonBackend.lower_program(&program);
    let rendered = PythonRenderer.render_program(&lowered);

    assert!(rendered.contains("async def add"));
    assert!(rendered.contains("a: int"));
    assert!(rendered.contains("return a + b"));
}

#[test]
fn typescript_backend_returns_promise_for_async() {
    let program = sample_program(vec![Effect::Async]);
    let lowered = TypeScriptBackend.lower_program(&program);
    let rendered = TypeScriptRenderer.render_program(&lowered);

    assert!(rendered.contains("export async function add"));
    assert!(rendered.contains(": Promise<number>"));
    assert!(rendered.contains("return a + b"));
}

#[test]
fn lowering_is_deterministic() {
    let program = sample_program(vec![Effect::Pure]);
    let lhs = RustRenderer.render_program(&RustBackend.lower_program(&program));
    let rhs = RustRenderer.render_program(&RustBackend.lower_program(&program));
    assert_eq!(lhs, rhs);
}

#[test]
fn validator_rejects_error_effect_without_can_fail() {
    let program = sample_program(vec![Effect::Error]);
    let result = DefaultSemanticValidator.validate_program(&program);
    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|issue| issue.code == "error_effect_requires_can_fail"));
}

#[test]
fn validator_rejects_missing_await() {
    let mut program = sample_program(vec![Effect::Pure]);
    program.modules[0].functions.push(Function {
        name: "fetch".to_string(),
        visibility: Visibility::Public,
        inputs: Vec::new(),
        outputs: FunctionOutput {
            r#type: TypeRef::primitive("String"),
        },
        effects: vec![Effect::Async],
        can_fail: false,
        body: Block::default(),
    });
    program.modules[0].functions[0].body = Block {
        statements: vec![Statement::Expression(Expression::Call(code_ir::program_v1::Call {
            function: "fetch".to_string(),
            args: Vec::new(),
        }))],
    };

    let result = DefaultSemanticValidator.validate_program(&program);
    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|issue| issue.code == "missing_await"));
}

#[test]
fn safe_generation_runs_validation_and_build() {
    let program = sample_program(vec![Effect::Pure]);
    let output = safe_generate_program(
        &program,
        &RustBackend,
        &RustRenderer,
        &DefaultSemanticValidator,
    )
    .expect("safe generation");
    assert!(!output.files.is_empty());
}

#[test]
fn safe_generation_stops_on_validation_error() {
    let program = sample_program(vec![Effect::Error]);
    let err = safe_generate_program(
        &program,
        &RustBackend,
        &RustRenderer,
        &DefaultSemanticValidator,
    )
    .expect_err("validation error");
    assert!(matches!(err, SafeGenerationError::Validation(_)));
}

#[test]
fn rust_profile_matches_backend_output() {
    let program = sample_program(vec![Effect::Async, Effect::Mutation]);
    let backend = RustBackend.lower_program(&program);
    let dynamic = lower_program_with_profile(&program, &rust_profile());
    assert_eq!(backend, dynamic);
    assert_eq!(
        RustRenderer.render_program(&backend),
        RustRenderer.render_program(&dynamic)
    );
}

#[test]
fn profile_resolver_is_deterministic() {
    assert_eq!(resolve_profile("rust"), resolve_profile("rust"));
    assert_eq!(resolve_profile("python"), python_profile());
    assert_eq!(resolve_profile("ts"), ts_profile());
}

#[test]
fn dynamic_interpreter_maps_effects_and_errors() {
    let mut program = sample_program(vec![Effect::Async, Effect::Error]);
    program.modules[0].functions[0].can_fail = true;

    let rust = DynamicInterpreter::new(rust_profile()).interpret_program(&program, &rust_profile());
    let python =
        DynamicInterpreter::new(python_profile()).interpret_program(&program, &python_profile());
    let ts = DynamicInterpreter::new(ts_profile()).interpret_program(&program, &ts_profile());

    let rust_code = RustRenderer.render_program(&rust);
    let python_code = PythonRenderer.render_program(&python);
    let ts_code = TypeScriptRenderer.render_program(&ts);

    assert!(rust_code.contains("pub async fn add"));
    assert!(rust_code.contains("-> Result<i32, String>"));
    assert!(python_code.contains("async def add"));
    assert!(ts_code.contains("export async function add"));
    assert!(ts_code.contains(": Promise<number | Error>"));
}

#[test]
fn rule_engine_rejects_conflicts() {
    let engine = RuleEngine::new(vec![
        MappingRule {
            id: "r1".to_string(),
            priority: 10,
            condition: SemanticCondition::CanFail,
            transform: TransformTemplate::ReturnWrapper {
                pattern: "Result<{T}, String>".to_string(),
            },
            target: TargetScope::Function,
            source: RuleSource::Static,
            confidence: 1.0,
            usage_count: 0,
        },
        MappingRule {
            id: "r2".to_string(),
            priority: 10,
            condition: SemanticCondition::CanFail,
            transform: TransformTemplate::ReturnWrapper {
                pattern: "{T} | Error".to_string(),
            },
            target: TargetScope::Function,
            source: RuleSource::Static,
            confidence: 1.0,
            usage_count: 0,
        },
    ]);

    let err = engine.validate().expect_err("conflict expected");
    assert!(err.message.contains("rule conflict"));
}

#[test]
fn memory_can_override_language_profile() {
    let memory: InMemoryEngine = InMemoryEngine::default();
    memory.store(MemoryRecord {
        id: "rust-profile".to_string(),
        text: "language profile rust".to_string(),
        tags: vec!["profile:rust".to_string()],
        embedding: None,
        architecture: None,
        relations: vec![
            "primitive:Int=i64".to_string(),
            "import:Result=core::result::Result".to_string(),
        ],
    });

    let profile = resolve_profile_from_memory(&memory as &dyn MemoryEngine, "rust");
    assert_eq!(profile.type_system.primitive_map.get("Int"), Some(&"i64".to_string()));
    assert!(profile
        .import_rules
        .iter()
        .any(|rule| rule.import == "core::result::Result"));
}

#[test]
fn learning_engine_increases_confidence_for_good_results() {
    let mut record = MemoryRuleRecord {
        rule: MappingRule {
            id: "learned_async".to_string(),
            priority: 10,
            condition: SemanticCondition::IsAsync,
            transform: TransformTemplate::FunctionSignature {
                pattern: "async fn {name}".to_string(),
            },
            target: TargetScope::Function,
            source: RuleSource::Learned,
            confidence: 0.5,
            usage_count: 0,
        },
        history: Vec::new(),
    };
    let evaluation = EvaluationResult {
        build_success: true,
        test_pass: true,
        lint_score: 1.0,
        performance_score: 1.0,
        errors: Vec::new(),
    };

    LearningEngine.evaluate_rule(&mut record, &evaluation);

    assert!(record.rule.confidence > 0.5);
    assert_eq!(record.rule.usage_count, 1);
    assert_eq!(record.history.len(), 1);
}

#[test]
fn promotion_requires_confidence_and_usage_threshold() {
    let mut store = RuleStore {
        active_rules: Vec::new(),
        candidate_rules: vec![MemoryRuleRecord {
            rule: MappingRule {
                id: "candidate".to_string(),
                priority: 10,
                condition: SemanticCondition::Always,
                transform: TransformTemplate::TypeMapping {
                    pattern: "i64".to_string(),
                },
                target: TargetScope::Type,
                source: RuleSource::Learned,
                confidence: 0.81,
                usage_count: 11,
            },
            history: vec![0.9; 11],
        }],
        validated_rules: Vec::new(),
        deprecated_rules: Vec::new(),
    };

    assert!(should_promote(&store.candidate_rules[0].rule));
    LearningEngine.promote_candidates(&mut store);

    assert_eq!(store.active_rules.len(), 1);
    assert!(store.candidate_rules.is_empty());
}

#[test]
fn safe_mode_keeps_rule_selection_deterministic() {
    let store = RuleStore {
        active_rules: vec![MemoryRuleRecord {
            rule: MappingRule {
                id: "active".to_string(),
                priority: 10,
                condition: SemanticCondition::Always,
                transform: TransformTemplate::TypeMapping {
                    pattern: "i32".to_string(),
                },
                target: TargetScope::Type,
                source: RuleSource::Static,
                confidence: 1.0,
                usage_count: 1,
            },
            history: vec![1.0],
        }],
        candidate_rules: vec![MemoryRuleRecord {
            rule: MappingRule {
                id: "candidate".to_string(),
                priority: 5,
                condition: SemanticCondition::Always,
                transform: TransformTemplate::TypeMapping {
                    pattern: "i64".to_string(),
                },
                target: TargetScope::Type,
                source: RuleSource::Learned,
                confidence: 0.9,
                usage_count: 20,
            },
            history: vec![0.9; 20],
        }],
        validated_rules: Vec::new(),
        deprecated_rules: Vec::new(),
    };

    let lhs = select_rule_records(&store, true, "same-input", 0.5);
    let rhs = select_rule_records(&store, true, "same-input", 0.5);

    assert_eq!(lhs, rhs);
    assert_eq!(lhs.len(), 1);
    assert_eq!(lhs[0].rule.id, "active");
}

#[test]
fn validation_gate_rejects_conflicting_candidate() {
    let validator = DefaultRuleValidator;
    let ctx = ValidationContext {
        active_rules: vec![MappingRule {
            id: "active_conflict".to_string(),
            priority: 10,
            condition: SemanticCondition::CanFail,
            transform: TransformTemplate::ReturnWrapper {
                pattern: "Result<{T}, String>".to_string(),
            },
            target: TargetScope::Function,
            source: RuleSource::Static,
            confidence: 1.0,
            usage_count: 1,
        }],
        regression_pass: true,
        deterministic: true,
        diff_safe: true,
        cross_language_consistent: true,
    };
    let rule = MappingRule {
        id: "candidate_conflict".to_string(),
        priority: 10,
        condition: SemanticCondition::CanFail,
        transform: TransformTemplate::ReturnWrapper {
            pattern: "{T} | Error".to_string(),
        },
        target: TargetScope::Function,
        source: RuleSource::Learned,
        confidence: 0.9,
        usage_count: 30,
    };

    let result = validator.validate(&rule, &ctx);
    assert!(!result.passed);
    assert!(!result.checks.contains(&ValidationCheck::NoConflict));
}

#[test]
fn validated_rule_can_be_promoted_to_active() {
    let validator = DefaultRuleValidator;
    let mut store = bootstrap_rule_store("rust");
    let validated = validate_candidate_rule(&store, "candidate_rust_bytes", &validator)
        .expect("validated candidate");
    let validation = code_language_core::stable_v03::dynamic_ir::ValidationResult {
        passed: true,
        score: validated.validation_score,
        checks: validated.passed_checks.clone(),
    };
    assert!(should_promote_validated(&validated.rule, &validation));
    store.validated_rules.push(validated);

    assert!(promote_validated_rule(&mut store, "candidate_rust_bytes"));
    assert!(store
        .active_rules
        .iter()
        .any(|record| record.rule.id == "candidate_rust_bytes"));
}

#[test]
fn active_rule_can_be_rolled_back() {
    let mut store = bootstrap_rule_store("rust");
    assert!(rollback_rule(&mut store, "async_fn"));
    assert!(!store.active_rules.iter().any(|record| record.rule.id == "async_fn"));
    assert!(store
        .deprecated_rules
        .iter()
        .any(|record| record.rule.id == "async_fn"));
}
