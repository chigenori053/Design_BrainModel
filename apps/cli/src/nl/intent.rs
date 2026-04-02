use crate::nl::types::IntentType;

pub fn primary_intent(input: &str) -> IntentType {
    let lower = input.to_lowercase();

    if wants_structure_view(&lower) {
        return IntentType::StructureView;
    }
    if wants_structure_edit(&lower) {
        return IntentType::StructureEdit;
    }
    if wants_coding(&lower) {
        return IntentType::Coding;
    }
    if wants_validate(&lower) {
        return IntentType::Validate;
    }
    if wants_analyze(&lower) {
        return IntentType::Analyze;
    }
    if wants_run(&lower) {
        return IntentType::Run;
    }
    if wants_rules(&lower) {
        return IntentType::Rules;
    }
    if wants_memory(&lower) {
        return IntentType::Memory;
    }

    IntentType::Unknown
}

pub fn wants_analyze(lower: &str) -> bool {
    ["analyze", "analyse", "解析", "分析", "調べ", "audit"]
        .iter()
        .any(|keyword| lower.contains(keyword))
}

pub fn wants_coding(lower: &str) -> bool {
    [
        "coding",
        "safe fix",
        "fix",
        "repair",
        "修正",
        "直して",
        "直す",
        "安全に",
        "unsafe",
        "実装",
        "減らして",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

pub fn wants_validate(lower: &str) -> bool {
    [
        "validate",
        "検証",
        "cargo check",
        "check",
        "確認",
        "lint",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

pub fn wants_structure_view(lower: &str) -> bool {
    [
        "gui",
        "viewer",
        "view structure",
        "open structure",
        "構造",
        "開いて",
        "見せて",
        "graph",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
        && !wants_structure_edit(lower)
}

pub fn wants_structure_edit(lower: &str) -> bool {
    ["edit structure", "構造を編集", "viewer edit", "session attach", "編集"]
        .iter()
        .any(|keyword| lower.contains(keyword))
}

pub fn wants_run(lower: &str) -> bool {
    (["workflow", "run", "実行", "execute"]
        .iter()
        .any(|keyword| lower.contains(keyword)))
        && !lower.contains("cargo check")
}

pub fn wants_rules(lower: &str) -> bool {
    ["rules", "rule", "ルール"].iter().any(|keyword| lower.contains(keyword))
}

pub fn wants_memory(lower: &str) -> bool {
    ["memory", "seed", "import memory", "メモリ", "インポート"]
        .iter()
        .any(|keyword| lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_analyze_intent() {
        assert_eq!(primary_intent("このプロジェクトを解析して"), IntentType::Analyze);
    }

    #[test]
    fn detects_structure_view_intent() {
        assert_eq!(
            primary_intent("GUIで構造を開いて"),
            IntentType::StructureView
        );
    }

    #[test]
    fn detects_coding_intent() {
        assert_eq!(
            primary_intent("unsafeを減らして cargo check して"),
            IntentType::Coding
        );
    }
}
