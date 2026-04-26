use std::path::{Path, PathBuf};

use crate::service::dto::SessionAppliedDiff;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityValidationInput {
    pub intent: String,
    pub target: Option<PathBuf>,
    pub diff: Option<SessionAppliedDiff>,
    pub plan_ids: Vec<String>,
    pub diff_fingerprints: Vec<String>,
    pub violations_before: Option<usize>,
    pub violations_after: Option<usize>,
    pub panic_free: bool,
    pub build_ok: bool,
    pub tests_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QualityScoreBreakdown {
    pub minimality: u8,
    pub intent: u8,
    pub determinism: u8,
    pub design: u8,
    pub safety: u8,
    pub productivity: u8,
}

impl QualityScoreBreakdown {
    pub fn total(self) -> u8 {
        self.minimality
            + self.intent
            + self.determinism
            + self.design
            + self.safety
            + self.productivity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityFailureClass {
    ExcessiveDiff,
    IntentMismatch,
    NonDeterministic,
    DesignRegression,
    UnsafeExecution,
    UnderGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityValidationReport {
    pub score: u8,
    pub passed: bool,
    pub breakdown: QualityScoreBreakdown,
    pub failures: Vec<QualityFailureClass>,
}

pub fn evaluate_quality(input: &QualityValidationInput) -> QualityValidationReport {
    let breakdown = QualityScoreBreakdown {
        minimality: score_minimality(input),
        intent: score_intent_alignment(input),
        determinism: score_determinism(input),
        design: score_design_improvement(input),
        safety: score_safety(input),
        productivity: score_productivity(input),
    };
    let mut failures = Vec::new();

    if breakdown.minimality < 18 {
        failures.push(QualityFailureClass::ExcessiveDiff);
    }
    if breakdown.intent < 20 {
        failures.push(QualityFailureClass::IntentMismatch);
    }
    if breakdown.determinism < 20 {
        failures.push(QualityFailureClass::NonDeterministic);
    }
    if breakdown.design < 20 {
        failures.push(QualityFailureClass::DesignRegression);
    }
    if breakdown.safety < 10 {
        failures.push(QualityFailureClass::UnsafeExecution);
    }
    if breakdown.productivity < 10 && expects_productive_diff(input) {
        failures.push(QualityFailureClass::UnderGeneration);
    }

    let score = breakdown.total();
    QualityValidationReport {
        score,
        passed: score >= 80
            && !failures.contains(&QualityFailureClass::NonDeterministic)
            && !failures.contains(&QualityFailureClass::UnsafeExecution)
            && !failures.contains(&QualityFailureClass::UnderGeneration),
        breakdown,
        failures,
    }
}

fn score_minimality(input: &QualityValidationInput) -> u8 {
    let Some(diff) = &input.diff else {
        return 20;
    };
    let changed_lines = diff.lines_added + diff.lines_removed;
    let unrelated = unrelated_file_count(input);

    if unrelated > 0 {
        return 0;
    }
    if diff.files_changed <= 1 && changed_lines <= 10 {
        20
    } else if diff.files_changed <= 3 && changed_lines <= 50 {
        15
    } else {
        0
    }
}

fn score_intent_alignment(input: &QualityValidationInput) -> u8 {
    let Some(diff) = &input.diff else {
        return 20;
    };
    if diff.files_changed == 0 {
        return 20;
    }
    if unrelated_file_count(input) > 0 {
        return 0;
    }
    if input.target.is_some() {
        return 20;
    }
    let intent = input.intent.to_lowercase();
    let all_excerpts_match = diff.files.iter().all(|file| {
        let text = format!(
            "{}\n{}",
            file.file_path.to_lowercase(),
            file.unified_diff_excerpt.to_lowercase()
        );
        intent
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '/')
            .filter(|token| token.len() >= 4)
            .any(|token| text.contains(token))
    });
    if all_excerpts_match { 16 } else { 8 }
}

fn score_determinism(input: &QualityValidationInput) -> u8 {
    let plan_ids_match = all_same_non_empty(&input.plan_ids);
    let diffs_match = all_same_non_empty(&input.diff_fingerprints);
    if plan_ids_match && diffs_match { 20 } else { 0 }
}

fn score_design_improvement(input: &QualityValidationInput) -> u8 {
    match (input.violations_before, input.violations_after) {
        (Some(before), Some(after)) if before == 0 && after == 0 => 20,
        (Some(before), Some(after)) if after < before => 20,
        (Some(_), Some(_)) => 0,
        _ => 10,
    }
}

fn score_safety(input: &QualityValidationInput) -> u8 {
    let mut score = 0;
    if input.panic_free {
        score += 4;
    }
    if input.build_ok {
        score += 3;
    }
    if input.tests_ok {
        score += 3;
    }
    score
}

fn score_productivity(input: &QualityValidationInput) -> u8 {
    if !expects_productive_diff(input) {
        return 10;
    }
    let Some(diff) = &input.diff else {
        return 0;
    };
    if diff.files_changed > 0 && score_intent_alignment(input) >= 16 {
        10
    } else {
        0
    }
}

fn expects_productive_diff(input: &QualityValidationInput) -> bool {
    let intent = input.intent.to_lowercase();
    [
        "改善",
        "修正",
        "変更",
        "リファクタ",
        "責務",
        "分離",
        "refactor",
        "fix",
        "improve",
    ]
    .iter()
    .any(|keyword| intent.contains(keyword))
}

fn unrelated_file_count(input: &QualityValidationInput) -> usize {
    let Some(target) = &input.target else {
        return 0;
    };
    let Some(diff) = &input.diff else {
        return 0;
    };
    diff.files
        .iter()
        .filter(|file| !paths_match_or_overlap(target, Path::new(&file.file_path)))
        .count()
}

fn paths_match_or_overlap(target: &Path, changed: &Path) -> bool {
    target == changed || changed.starts_with(target) || target.starts_with(changed)
}

fn all_same_non_empty(values: &[String]) -> bool {
    let Some(first) = values.first() else {
        return false;
    };
    !first.is_empty() && values.iter().all(|value| value == first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::dto::SessionAppliedFileDiff;

    fn diff(file: &str, added: usize, removed: usize) -> SessionAppliedDiff {
        SessionAppliedDiff {
            summary: format!("1 file changed, +{added} -{removed}"),
            files: vec![SessionAppliedFileDiff {
                file_path: file.to_string(),
                unified_diff_excerpt: "+ extracted helper".to_string(),
            }],
            files_changed: 1,
            lines_added: added,
            lines_removed: removed,
        }
    }

    fn base_input(diff: Option<SessionAppliedDiff>) -> QualityValidationInput {
        QualityValidationInput {
            intent: "apps/cli/src/repl.rs refactor single function".to_string(),
            target: Some(PathBuf::from("apps/cli/src/repl.rs")),
            diff,
            plan_ids: vec![
                "stable-plan".to_string(),
                "stable-plan".to_string(),
                "stable-plan".to_string(),
            ],
            diff_fingerprints: vec![
                "stable-diff".to_string(),
                "stable-diff".to_string(),
                "stable-diff".to_string(),
            ],
            violations_before: Some(3),
            violations_after: Some(2),
            panic_free: true,
            build_ok: true,
            tests_ok: true,
        }
    }

    #[test]
    fn tc01_minor_refactor_passes_minimality_and_quality_gate() {
        let report = evaluate_quality(&base_input(Some(diff("apps/cli/src/repl.rs", 6, 3))));

        assert!(report.passed, "{report:?}");
        assert_eq!(report.breakdown.minimality, 20);
        assert!(report.score >= 80);
    }

    #[test]
    fn tc02_structural_change_allows_logical_multi_file_diff() {
        let mut input = base_input(Some(SessionAppliedDiff {
            summary: "3 files changed, +30 -12".to_string(),
            files: vec![
                SessionAppliedFileDiff {
                    file_path: "crates/runtime/runtime_vm".to_string(),
                    unified_diff_excerpt: "+ pub mod adapter_service_interface;".to_string(),
                },
                SessionAppliedFileDiff {
                    file_path: "crates/runtime/runtime_vm/src/lib.rs".to_string(),
                    unified_diff_excerpt: "+ pub mod adapter_service_interface;".to_string(),
                },
            ],
            files_changed: 2,
            lines_added: 30,
            lines_removed: 12,
        }));
        input.intent = "split runtime_vm module".to_string();
        input.target = Some(PathBuf::from("crates/runtime/runtime_vm"));

        let report = evaluate_quality(&input);

        assert!(report.passed, "{report:?}");
        assert_eq!(report.breakdown.minimality, 15);
    }

    #[test]
    fn tc03_unrelated_file_change_is_intent_mismatch() {
        let input = base_input(Some(diff("apps/cli/src/world.rs", 2, 1)));

        let report = evaluate_quality(&input);

        assert!(!report.passed);
        assert!(
            report
                .failures
                .contains(&QualityFailureClass::IntentMismatch)
        );
        assert!(
            report
                .failures
                .contains(&QualityFailureClass::ExcessiveDiff)
        );
    }

    #[test]
    fn tc04_detects_plan_id_or_diff_nondeterminism() {
        let mut input = base_input(Some(diff("apps/cli/src/repl.rs", 2, 1)));
        input.plan_ids[2] = "different-plan".to_string();

        let report = evaluate_quality(&input);

        assert!(!report.passed);
        assert!(
            report
                .failures
                .contains(&QualityFailureClass::NonDeterministic)
        );
    }

    #[test]
    fn tc05_noop_clean_code_passes_with_zero_diff_and_zero_violations() {
        let mut input = base_input(None);
        input.intent = "problem-free code".to_string();
        input.violations_before = Some(0);
        input.violations_after = Some(0);

        let report = evaluate_quality(&input);

        assert!(report.passed, "{report:?}");
        assert_eq!(report.score, 100);
    }

    #[test]
    fn phase6_productivity_detects_under_generation_for_strong_intent() {
        let mut input = base_input(None);
        input.intent = "責務分離してリファクタリングして".to_string();

        let report = evaluate_quality(&input);

        assert!(!report.passed);
        assert!(
            report
                .failures
                .contains(&QualityFailureClass::UnderGeneration)
        );
    }
}
