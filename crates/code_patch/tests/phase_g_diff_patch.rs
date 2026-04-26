use code_diff::{IrChange, diff_programs};
use code_ir::program_v1::BackendLanguage;
use code_parser::{SupportedLanguage, parse_source_to_ir};
use code_patch::{
    ApplyError, SemanticValidationError, apply_patch, ensure_safe_changes,
    generate_patch_for_backend, rollback, validate_patch_semantics,
};
use refactor_engine::rules::rename_variable;

#[test]
fn diff_generates_ordered_rename_changeset() {
    let old_ir = parse_source_to_ir(
        SupportedLanguage::Rust,
        "sample",
        "pub fn compute(value: Int) -> Int {\n    let result = value;\n    return result;\n}\n",
    )
    .expect("old ir");
    let new_ir = parse_source_to_ir(
        SupportedLanguage::Rust,
        "sample",
        "pub fn compute(value: Int) -> Int {\n    let output = value;\n    return output;\n}\n",
    )
    .expect("new ir");

    let changes = diff_programs(&old_ir, &new_ir);
    assert_eq!(changes.changes.len(), 1);
    assert!(matches!(
        &changes.changes[0],
        IrChange::RenameSymbol {
            old_name,
            new_name,
            ..
        } if old_name == "result" && new_name == "output"
    ));
}

#[test]
fn patch_apply_and_semantic_validation_roundtrip() {
    let old_code =
        "pub fn compute(value: Int) -> Int {\n    let result = value;\n    return result;\n}\n";
    let old_ir = parse_source_to_ir(SupportedLanguage::Rust, "sample", old_code).expect("old ir");
    let changes =
        rename_variable(&old_ir, "sample", "compute", "result", "output").expect("changes");

    ensure_safe_changes(&old_ir, &changes).expect("safe");
    let patch =
        generate_patch_for_backend(&old_ir, &changes, BackendLanguage::Rust).expect("patch");
    let new_code = apply_patch(old_code, &patch).expect("apply");

    assert_eq!(
        new_code,
        "pub fn compute(value: Int) -> Int {\n    let output = value;\n    return output;\n}\n"
    );

    let validated = validate_patch_semantics(SupportedLanguage::Rust, &old_ir, &patch, &changes)
        .expect("valid");
    assert_eq!(validated.modules[0].functions[0].name, "compute");
}

#[test]
fn rollback_restores_snapshot() {
    let snapshot = "pub fn compute() -> Int {\n    return 1;\n}\n";
    assert_eq!(rollback(snapshot), snapshot);
}

#[test]
fn unsafe_condition_change_is_detected() {
    let old_ir = parse_source_to_ir(
        SupportedLanguage::Python,
        "sample",
        "def compute(flag: Bool) -> Int:\n    if flag:\n        return 1\n    else:\n        return 0\n",
    )
    .expect("old ir");
    let new_ir = parse_source_to_ir(
        SupportedLanguage::Python,
        "sample",
        "def compute(flag: Bool) -> Int:\n    if ready:\n        return 1\n    else:\n        return 0\n",
    )
    .expect("new ir");

    let changes = diff_programs(&old_ir, &new_ir);
    let err = ensure_safe_changes(&old_ir, &changes).expect_err("unsafe");
    assert!(err.to_string().contains("unsafe condition change"));
}

#[test]
fn semantic_mismatch_is_hard_error() {
    let old_code =
        "pub fn compute(value: Int) -> Int {\n    let result = value;\n    return result;\n}\n";
    let old_ir = parse_source_to_ir(SupportedLanguage::Rust, "sample", old_code).expect("old ir");
    let changes =
        rename_variable(&old_ir, "sample", "compute", "result", "output").expect("changes");
    let mut patch =
        generate_patch_for_backend(&old_ir, &changes, BackendLanguage::Rust).expect("patch");
    patch.edits[0].replacement =
        "pub fn compute(value: Int) -> Int {\n    let output = value;\n    return value;\n}\n"
            .to_string();

    let err = validate_patch_semantics(SupportedLanguage::Rust, &old_ir, &patch, &changes)
        .expect_err("mismatch");
    assert!(matches!(err, SemanticValidationError::Mismatch { .. }));
}

#[test]
fn overlapping_edits_are_rejected() {
    let err = apply_patch(
        "abc",
        &code_patch::Patch {
            edits: vec![
                code_patch::TextEdit {
                    file: "sample.rs".to_string(),
                    start: 0,
                    end: 2,
                    replacement: "xy".to_string(),
                },
                code_patch::TextEdit {
                    file: "sample.rs".to_string(),
                    start: 1,
                    end: 3,
                    replacement: "zz".to_string(),
                },
            ],
        },
    )
    .expect_err("must fail");
    assert!(matches!(err, ApplyError::OverlappingEdits));
}
