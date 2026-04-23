use std::collections::HashMap;

use super::{
    loader::load_from_str,
    validator::{validate, ValidationError},
    EmitCondition, EmitNode, FeatureFlags, FormattingRules, IrOp, LanguageSpec, Pattern,
    PatternKind, Placeholder,
};

// ── spec fixtures ─────────────────────────────────────────────────────────────

const RUST_SPEC: &str = include_str!("../../specs/rust.json");
const PYTHON_SPEC: &str = include_str!("../../specs/python.json");

fn rust() -> LanguageSpec {
    load_from_str(RUST_SPEC).expect("rust.json is valid")
}

fn python() -> LanguageSpec {
    load_from_str(PYTHON_SPEC).expect("python.json is valid")
}

// ── 9.1 Determinism ──────────────────────────────────────────────────────────

#[test]
fn determinism_same_json_produces_equal_spec() {
    assert_eq!(rust(), rust());
}

#[test]
fn determinism_roundtrip_serialize_deserialize() {
    let spec = rust();
    let json = serde_json::to_string(&spec).expect("serialize");
    let spec2: LanguageSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(spec, spec2);
}

// ── 9.2 Cross-language ───────────────────────────────────────────────────────

#[test]
fn cross_language_rust_differs_from_python() {
    assert_ne!(rust(), python());
}

#[test]
fn cross_language_assign_pattern_differs() {
    let r = rust();
    let p = python();
    let rust_assign = r.patterns.get(&IrOp::Assign).unwrap();
    let py_assign = p.patterns.get(&IrOp::Assign).unwrap();
    assert_ne!(rust_assign, py_assign);
}

#[test]
fn cross_language_feature_flags_differ() {
    assert!(rust().features.requires_semicolon);
    assert!(!python().features.requires_semicolon);
}

// ── 9.3 Coverage ─────────────────────────────────────────────────────────────

#[test]
fn coverage_all_ir_ops_present_in_rust_spec() {
    let spec = rust();
    for op in IrOp::all() {
        assert!(
            spec.patterns.contains_key(op),
            "rust.json is missing pattern for {op:?}"
        );
    }
}

#[test]
fn coverage_all_ir_ops_present_in_python_spec() {
    let spec = python();
    for op in IrOp::all() {
        assert!(
            spec.patterns.contains_key(op),
            "python.json is missing pattern for {op:?}"
        );
    }
}

// ── 9.4 Validation ───────────────────────────────────────────────────────────

#[test]
fn validation_rust_spec_passes() {
    assert!(validate(&rust()).is_ok());
}

#[test]
fn validation_python_spec_passes() {
    assert!(validate(&python()).is_ok());
}

#[test]
fn validation_empty_patterns_fails_with_all_missing() {
    let spec = LanguageSpec {
        language: "test".to_string(),
        version: "1.0".to_string(),
        patterns: HashMap::new(),
        formatting: FormattingRules {
            indent: "  ".to_string(),
            newline: "\n".to_string(),
            semicolon: false,
        },
        features: FeatureFlags {
            requires_semicolon: false,
            has_type_annotations: false,
            supports_block_scope: false,
        },
    };
    let errors = validate(&spec).unwrap_err();
    assert_eq!(errors.len(), IrOp::all().len());
    for op in IrOp::all() {
        assert!(
            errors.contains(&ValidationError::MissingPattern(op.clone())),
            "expected MissingPattern for {op:?}"
        );
    }
}

#[test]
fn validation_unknown_feature_flag_is_hard_error() {
    let mut spec = rust();
    spec.patterns.insert(
        IrOp::Assign,
        Pattern {
            kind: PatternKind::Statement,
            emit: vec![EmitNode::Optional {
                condition: EmitCondition::FeatureEnabled("no_such_flag".to_string()),
                node: Box::new(EmitNode::Text(";".to_string())),
            }],
        },
    );
    let errors = validate(&spec).unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, ValidationError::UnknownFeatureFlag(n) if n == "no_such_flag")));
}

#[test]
fn validation_feature_enabled_known_flags_pass() {
    for flag in ["requires_semicolon", "has_type_annotations", "supports_block_scope"] {
        let mut spec = rust();
        spec.patterns.insert(
            IrOp::Assign,
            Pattern {
                kind: PatternKind::Statement,
                emit: vec![EmitNode::Optional {
                    condition: EmitCondition::FeatureEnabled(flag.to_string()),
                    node: Box::new(EmitNode::Text(";".to_string())),
                }],
            },
        );
        assert!(validate(&spec).is_ok(), "flag \"{flag}\" should be valid");
    }
}

// ── Structural correctness of Assign / Call (spec §5) ────────────────────────

#[test]
fn assign_rust_starts_with_let_keyword() {
    let spec = rust();
    let assign = spec.patterns.get(&IrOp::Assign).unwrap();
    assert_eq!(assign.kind, PatternKind::Statement);
    assert!(
        matches!(&assign.emit[0], EmitNode::Text(t) if t == "let "),
        "first emit node must be Text(\"let \")"
    );
}

#[test]
fn assign_python_has_no_let_keyword() {
    let spec = python();
    let assign = spec.patterns.get(&IrOp::Assign).unwrap();
    // Python assign starts with the variable name placeholder, not "let"
    assert!(
        matches!(&assign.emit[0], EmitNode::Placeholder(Placeholder::VarName)),
        "Python assign must start with VarName placeholder"
    );
}

#[test]
fn call_contains_join_for_args() {
    let spec = rust();
    let call = spec.patterns.get(&IrOp::Call).unwrap();
    let has_join = call.emit.iter().any(|n| matches!(n, EmitNode::Join { .. }));
    assert!(has_join, "Call pattern must contain a Join node for args");
}

#[test]
fn call_ends_with_newline() {
    let spec = rust();
    let call = spec.patterns.get(&IrOp::Call).unwrap();
    assert!(
        matches!(call.emit.last(), Some(EmitNode::NewLine)),
        "Call pattern must end with NewLine"
    );
}

// ── Immutability / clone independence ────────────────────────────────────────

#[test]
fn spec_clone_is_independent() {
    let spec1 = rust();
    let mut spec2 = spec1.clone();
    spec2.language = "modified".to_string();
    assert_eq!(spec1.language, "rust");
    assert_eq!(spec2.language, "modified");
}

// ── Loader error paths ────────────────────────────────────────────────────────

#[test]
fn loader_invalid_json_returns_error() {
    assert!(load_from_str("not json").is_err());
}

#[test]
fn loader_missing_field_returns_error() {
    assert!(load_from_str(r#"{"language":"x"}"#).is_err());
}
