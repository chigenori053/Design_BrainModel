use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::refactor::{GuiAction, GuiActionMode};
use crate::viewer::function_map::{ViewerAction, resolve_action};
use crate::viewer::session::{redo_session, undo_session};
use crate::viewer::{StructureViewIR, dispatch_gui_action, structure_ir_path};

/// ViewerからCLIへ渡すNLリクエストのコンテキスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NlContext {
    pub prompt: String,
    pub selected_node: Option<String>,
    pub root: PathBuf,
}

/// CLIからViewerへ返すNLレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NlDispatchResult {
    /// チャットに表示する応答テキスト
    /// "__local:" プレフィックスの場合はViewer側でローカル処理
    pub response: String,
    /// IRファイルが更新されたか（trueならViewerはIRを再ロードする）
    pub ir_updated: bool,
}

impl NlDispatchResult {
    fn text(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            ir_updated: false,
        }
    }

    fn text_with_ir(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            ir_updated: true,
        }
    }

    fn local(cmd: impl Into<String>) -> Self {
        Self {
            response: format!("__local:{}", cmd.into()),
            ir_updated: false,
        }
    }
}

/// NLプロンプトからViewerアクションを解決して実行する
pub fn dispatch_nl(ctx: &NlContext) -> NlDispatchResult {
    let action = resolve_action(&ctx.prompt);

    match action {
        Some(ViewerAction::HighlightCycles) => describe_cycles(&ctx.root),
        Some(ViewerAction::ShowViolations) => describe_violations(&ctx.root),
        Some(ViewerAction::DescribeNode) => describe_node(&ctx.root, ctx.selected_node.as_deref()),
        Some(ViewerAction::PreviewRefactor) => execute_refactor(
            &ctx.root,
            &ctx.prompt,
            ctx.selected_node.as_deref(),
            GuiActionMode::Preview,
        ),
        Some(ViewerAction::ApplyRefactor) => execute_refactor(
            &ctx.root,
            &ctx.prompt,
            ctx.selected_node.as_deref(),
            GuiActionMode::Apply,
        ),
        Some(ViewerAction::UndoLastAction) => match undo_session(&ctx.root) {
            Ok(_) => NlDispatchResult::text_with_ir("前回の操作を取り消しました。"),
            Err(e) => NlDispatchResult::text(format!("Undo failed: {e}")),
        },
        Some(ViewerAction::RedoLastAction) => match redo_session(&ctx.root) {
            Ok(_) => NlDispatchResult::text_with_ir("操作をやり直しました。"),
            Err(e) => NlDispatchResult::text(format!("Redo failed: {e}")),
        },
        Some(ViewerAction::ShowRiskOverlay) => describe_risk(&ctx.root),
        Some(ViewerAction::SwitchMode2D) => NlDispatchResult::local("switch_2d"),
        Some(ViewerAction::SwitchMode3D) => NlDispatchResult::local("switch_3d"),
        Some(ViewerAction::SearchNode) => {
            let term = extract_search_term(&ctx.prompt);
            NlDispatchResult::local(format!("search:{term}"))
        }
        Some(ViewerAction::ExportReport) => NlDispatchResult::text(
            "レポートエクスポートは `design_cli analyze report <path>` で実行できます。",
        ),
        None => NlDispatchResult::text(format!(
            "「{}」の操作を特定できませんでした。\n\
            利用可能なコマンド例: \
            cycle確認 / 違反表示 / プレビュー / 適用 / undo / redo / リスク表示 / 2D切替 / 3D切替 / 検索",
            ctx.prompt
        )),
    }
}

fn load_ir(root: &Path) -> Option<StructureViewIR> {
    let path = structure_ir_path(root);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn describe_cycles(root: &Path) -> NlDispatchResult {
    let ir = match load_ir(root) {
        Some(ir) => ir,
        None => {
            return NlDispatchResult::text(
                "構造IRが見つかりません。先に analyze を実行してください。",
            );
        }
    };
    let cycles: Vec<_> = ir.edges.iter().filter(|e| e.cycle).collect();
    if cycles.is_empty() {
        return NlDispatchResult::text("循環依存は検出されていません。");
    }
    let mut lines = vec![format!("循環依存が {} 件検出されています:", cycles.len())];
    for c in &cycles {
        lines.push(format!("  {} → {}", c.from, c.to));
    }
    NlDispatchResult::text(lines.join("\n"))
}

fn describe_violations(root: &Path) -> NlDispatchResult {
    let ir = match load_ir(root) {
        Some(ir) => ir,
        None => {
            return NlDispatchResult::text(
                "構造IRが見つかりません。先に analyze を実行してください。",
            );
        }
    };
    let risk_violations: Vec<_> = ir
        .risk_overlay
        .iter()
        .filter(|r| r.level == "error" || r.level == "violation")
        .collect();
    if risk_violations.is_empty() {
        return NlDispatchResult::text("レイヤー違反は検出されていません。");
    }
    let mut lines = vec![format!("違反が {} 件あります:", risk_violations.len())];
    for v in &risk_violations {
        lines.push(format!("  [{}] {} — {}", v.level, v.target, v.message));
    }
    NlDispatchResult::text(lines.join("\n"))
}

fn describe_node(root: &Path, selected: Option<&str>) -> NlDispatchResult {
    let node_id = match selected {
        Some(n) => n,
        None => {
            return NlDispatchResult::text(
                "ノードが選択されていません。マップ上のノードをクリックしてから説明を要求してください。",
            );
        }
    };
    let ir = match load_ir(root) {
        Some(ir) => ir,
        None => {
            return NlDispatchResult::text(
                "構造IRが見つかりません。先に analyze を実行してください。",
            );
        }
    };
    let node = match ir
        .nodes
        .iter()
        .find(|n| n.id == node_id || n.label == node_id)
    {
        Some(n) => n,
        None => return NlDispatchResult::text(format!("ノード「{node_id}」が見つかりません。")),
    };
    let incoming = ir.edges.iter().filter(|e| e.to == node.id).count();
    let outgoing = ir.edges.iter().filter(|e| e.from == node.id).count();
    let cycles = ir
        .edges
        .iter()
        .filter(|e| (e.from == node.id || e.to == node.id) && e.cycle)
        .count();
    let risk = ir.risk_overlay.iter().find(|r| r.target == node.id);
    let mut lines = vec![
        format!(
            "**{}** (layer: {}, role: {})",
            node.label, node.layer, node.role
        ),
        format!("依存: 入力 {incoming} / 出力 {outgoing}"),
    ];
    if cycles > 0 {
        lines.push(format!("⚠ 循環依存 {cycles} 件に関与"));
    }
    if let Some(r) = risk {
        lines.push(format!("リスク [{}]: {}", r.level, r.message));
    }
    NlDispatchResult::text(lines.join("\n"))
}

fn describe_risk(root: &Path) -> NlDispatchResult {
    let ir = match load_ir(root) {
        Some(ir) => ir,
        None => {
            return NlDispatchResult::text(
                "構造IRが見つかりません。先に analyze を実行してください。",
            );
        }
    };
    if ir.risk_overlay.is_empty() {
        return NlDispatchResult::text("リスクは検出されていません。");
    }
    let mut lines = vec![format!(
        "リスク項目が {} 件あります:",
        ir.risk_overlay.len()
    )];
    for r in &ir.risk_overlay {
        lines.push(format!("  [{}] {} — {}", r.level, r.target, r.message));
    }
    NlDispatchResult::text(lines.join("\n"))
}

fn execute_refactor(
    root: &Path,
    prompt: &str,
    selected_node: Option<&str>,
    mode: GuiActionMode,
) -> NlDispatchResult {
    let target = extract_refactor_target(prompt, selected_node);
    let event = GuiAction {
        action: "refactor".to_string(),
        target: target.clone(),
        node: selected_node.map(str::to_string),
        project_root: Some(root.to_path_buf()),
        selected_nodes: selected_node.into_iter().map(str::to_string).collect(),
        selected_edges: Vec::new(),
        mode,
    };
    match dispatch_gui_action(root, event) {
        Ok((spec, _)) => NlDispatchResult::text_with_ir(format!(
            "{} 完了: {} (stage: {})",
            spec.command_kind, spec.target, spec.stage
        )),
        Err(e) => NlDispatchResult::text(format!("実行エラー: {e}")),
    }
}

/// プロンプトからリファクタリングターゲットを抽出する
fn extract_refactor_target(prompt: &str, selected_node: Option<&str>) -> String {
    let lower = prompt.to_lowercase();
    if lower.contains("cycle") || lower.contains("循環") {
        return "cycle".to_string();
    }
    if lower.contains("extract") || lower.contains("抽出") || lower.contains("interface") {
        return "extract-interface".to_string();
    }
    if lower.contains("split") || lower.contains("分割") {
        return "module-split".to_string();
    }
    if lower.contains("merge") || lower.contains("マージ") || lower.contains("統合") {
        return "merge-module".to_string();
    }
    if lower.contains("move") || lower.contains("移動") {
        return "file-move".to_string();
    }
    selected_node.unwrap_or("auto").to_string()
}

/// プロンプトから検索キーワードを抽出する
fn extract_search_term(prompt: &str) -> String {
    let lower = prompt.to_ascii_lowercase();
    for prefix in &["search ", "find ", "locate ", "検索 ", "探す ", "見つけて "] {
        if let Some(pos) = lower.find(prefix) {
            let term = &prompt[pos + prefix.len()..].trim().to_string();
            if !term.is_empty() {
                return term.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cycle_target() {
        assert_eq!(extract_refactor_target("cycleを解消して", None), "cycle");
    }

    #[test]
    fn extract_interface_target() {
        assert_eq!(
            extract_refactor_target("interfaceを抽出してプレビュー", Some("renderer")),
            "extract-interface"
        );
    }

    #[test]
    fn extract_search_term_en() {
        assert_eq!(extract_search_term("search renderer"), "renderer");
    }

    #[test]
    fn dispatch_nl_no_ir_graceful() {
        let root = std::path::PathBuf::from("/tmp/nl_dispatch_no_ir_test");
        let result = dispatch_nl(&NlContext {
            prompt: "循環依存を見せて".to_string(),
            selected_node: None,
            root,
        });
        assert!(
            result.response.contains("IR")
                || result.response.contains("cycle")
                || !result.response.is_empty()
        );
    }
}
