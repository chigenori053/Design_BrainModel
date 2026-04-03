use language_core::{LanguageState, language_search, semantic_parser};

/// Viewer上で実行可能なアクションの一覧
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerAction {
    HighlightCycles,
    ShowViolations,
    DescribeNode,
    PreviewRefactor,
    ApplyRefactor,
    UndoLastAction,
    RedoLastAction,
    ShowRiskOverlay,
    SwitchMode2D,
    SwitchMode3D,
    SearchNode,
    ExportReport,
}

/// Viewer機能のメタデータ（機能マップの1エントリ）
pub struct ViewerFunction {
    pub action: ViewerAction,
    pub name: &'static str,
    pub description: &'static str,
    pub keywords_en: &'static [&'static str],
    pub keywords_ja: &'static [&'static str],
    /// language_core ConceptMemory のラベルと対応するアンカー概念
    pub concepts: &'static [&'static str],
}

/// 全Viewer機能の定義（機能マップ）
pub fn viewer_function_map() -> Vec<ViewerFunction> {
    vec![
        ViewerFunction {
            action: ViewerAction::HighlightCycles,
            name: "Highlight Cycles",
            description: "Show circular dependencies highlighted in the map",
            keywords_en: &["cycle", "circular", "circular dependency", "loop"],
            keywords_ja: &["循環", "サイクル", "循環依存", "cycle"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::ShowViolations,
            name: "Show Violations",
            description: "Show layer violations in the architecture",
            keywords_en: &["violation", "layer violation", "dependency violation"],
            keywords_ja: &["違反", "レイヤー違反", "依存違反", "violation"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::DescribeNode,
            name: "Describe Node",
            description: "Describe the currently selected node",
            keywords_en: &["describe", "what is", "explain", "info about", "tell me about"],
            keywords_ja: &["説明", "とは", "教えて", "詳細", "情報"],
            concepts: &["service", "controller", "repository"],
        },
        ViewerFunction {
            action: ViewerAction::PreviewRefactor,
            name: "Preview Refactor",
            description: "Preview a refactoring action without applying",
            keywords_en: &["preview", "what if", "show diff", "simulate", "dry run"],
            keywords_ja: &["プレビュー", "確認", "差分", "シミュレート", "preview"],
            concepts: &["build"],
        },
        ViewerFunction {
            action: ViewerAction::ApplyRefactor,
            name: "Apply Refactor",
            description: "Apply the current refactoring action safely",
            keywords_en: &["apply", "safe apply", "execute", "do it"],
            keywords_ja: &["適用", "実行", "やって", "反映", "apply"],
            concepts: &["build"],
        },
        ViewerFunction {
            action: ViewerAction::UndoLastAction,
            name: "Undo",
            description: "Undo the last applied action",
            keywords_en: &["undo", "revert", "rollback", "go back"],
            keywords_ja: &["元に戻す", "戻して", "undo", "取り消し"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::RedoLastAction,
            name: "Redo",
            description: "Redo the last undone action",
            keywords_en: &["redo", "reapply"],
            keywords_ja: &["やり直し", "redo", "再適用"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::ShowRiskOverlay,
            name: "Show Risk Overlay",
            description: "Show risk analysis overlay on the map",
            keywords_en: &["risk", "danger", "hotspot", "issue", "problem"],
            keywords_ja: &["リスク", "危険", "問題", "ホットスポット", "懸念"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::SwitchMode2D,
            name: "Switch to 2D",
            description: "Switch the map to 2D view",
            keywords_en: &["2d", "two dimensional", "flat view"],
            keywords_ja: &["2d", "2D", "2次元", "フラット"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::SwitchMode3D,
            name: "Switch to 3D",
            description: "Switch the map to 3D isometric view",
            keywords_en: &["3d", "three dimensional", "isometric"],
            keywords_ja: &["3d", "3D", "3次元", "立体"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::SearchNode,
            name: "Search Node",
            description: "Search for a node in the map",
            keywords_en: &["search", "find", "locate", "look for"],
            keywords_ja: &["検索", "探す", "見つけて", "探して"],
            concepts: &[],
        },
        ViewerFunction {
            action: ViewerAction::ExportReport,
            name: "Export Report",
            description: "Export the analysis report",
            keywords_en: &["export", "report", "generate report", "save report"],
            keywords_ja: &["エクスポート", "レポート", "出力", "保存"],
            concepts: &[],
        },
    ]
}

/// キーワードスコア + LanguageCoreセマンティクスの合成スコアで機能を評価する
pub fn score(function: &ViewerFunction, input: &str, state: &LanguageState) -> f32 {
    let lower = input.to_ascii_lowercase();

    let kw_hits = function
        .keywords_en
        .iter()
        .chain(function.keywords_ja.iter())
        .filter(|kw| lower.contains(*kw))
        .count() as f32;
    let kw_score = (kw_hits / 3.0).min(1.0);

    let sem_score = if function.concepts.is_empty() {
        0.0_f32
    } else {
        let total: f64 = function
            .concepts
            .iter()
            .map(|label| {
                state
                    .semantic_graph
                    .concepts
                    .values()
                    .find(|c| c.label == *label)
                    .map(|c| state.semantic_field.activation_of(c.concept_id))
                    .unwrap_or(0.0)
            })
            .sum();
        (total / function.concepts.len() as f64) as f32
    };

    kw_score * 0.65 + sem_score * 0.35
}

/// NL入力からViewerActionを解決する（LanguageCore + 機能マップ）
pub fn resolve_action(input: &str) -> Option<ViewerAction> {
    let initial = semantic_parser(input);
    let state = language_search(initial);
    let map = viewer_function_map();

    let best = map
        .iter()
        .map(|f| (f.action, score(f, input, &state)))
        .max_by(|a, b| a.1.total_cmp(&b.1));

    match best {
        Some((action, s)) if s > 0.0 => Some(action),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_cycle_intent_ja() {
        let action = resolve_action("循環依存を見せて");
        assert_eq!(action, Some(ViewerAction::HighlightCycles));
    }

    #[test]
    fn resolves_apply_intent_en() {
        let action = resolve_action("safe apply the refactor");
        assert_eq!(action, Some(ViewerAction::ApplyRefactor));
    }

    #[test]
    fn resolves_undo_intent_ja() {
        let action = resolve_action("元に戻して");
        assert_eq!(action, Some(ViewerAction::UndoLastAction));
    }

    #[test]
    fn resolves_preview_intent() {
        let action = resolve_action("preview the changes");
        assert_eq!(action, Some(ViewerAction::PreviewRefactor));
    }

    #[test]
    fn resolves_3d_mode_switch() {
        let action = resolve_action("3Dで見せて");
        assert_eq!(action, Some(ViewerAction::SwitchMode3D));
    }

    #[test]
    fn returns_none_for_empty() {
        let action = resolve_action("");
        assert_eq!(action, None);
    }
}
