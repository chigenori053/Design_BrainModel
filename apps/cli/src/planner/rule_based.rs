/// DBM_CLI: ルールベース Planner（拡張版）
///
/// キーワードマッチングによる決定論的プラン生成（LLM不使用）。
/// 日英両対応。
///
/// ルール（優先順）：
/// 1. "project" / "プロジェクト"         → analyze project + generate design（2段）
/// 2. "check" && "fix" / "チェック"&&"修正" → validate + refactor（2段）
/// 3. "validate" / "検証" / "チェック"    → validate
/// 4. "refactor" / "リファクタ" / "改善"  → refactor
/// 5. "coding" / "apply" / "適用" / "実装"→ coding
/// 6. "run" / "実行" / "execute"         → exec（→ design run）
/// 7. "analyze" / "分析" / "解析" / "調べ" → analyze code
/// 8. "design" / "設計" / "architecture" → generate design
/// 9. "spec" / "仕様"                    → generate spec
/// 10. それ以外（デフォルト）             → analyze code（現在地を解析）
use crate::plan::{CommandInvocation, Plan, PlanStatus, Step};

pub struct RuleBasedPlanner;

impl RuleBasedPlanner {
    pub fn new() -> Self {
        Self
    }

    /// テキストから Plan を生成する（常に成功）
    ///
    /// `last_path` はセッションの last_path.as_deref() から渡す。
    /// 入力にパスが含まれない場合のフォールバックに使われる。
    pub fn plan(&self, input: &str, last_path: Option<&str>) -> Plan {
        let id = Self::make_id(input);
        let steps = Self::build_steps(input, last_path);
        let mut plan = Plan::new(id, steps);
        plan.status = PlanStatus::Ready;
        plan
    }

    fn make_id(input: &str) -> String {
        let trimmed: String = input.chars().take(32).collect();
        format!("plan:{}", trimmed.replace(' ', "_"))
    }

    fn build_steps(input: &str, last_path: Option<&str>) -> Vec<Step> {
        let lower = input.to_lowercase();
        let target = Self::extract_target_with_fallback(input, last_path);

        // ── 2段プラン ──────────────────────────────────────────────────────

        if lower.contains("project") || lower.contains("プロジェクト") {
            // プロジェクト全体: analyze project → generate design
            return vec![
                Step::new(
                    0,
                    format!("Analyze project: {target}"),
                    Some(CommandInvocation::new(
                        "analyze",
                        Some("project"),
                        &[target.as_str()],
                    )),
                ),
                Step::new(
                    1,
                    format!("Generate design: {target}"),
                    Some(CommandInvocation::new(
                        "generate",
                        Some("design"),
                        &[target.as_str()],
                    )),
                ),
            ];
        }

        // チェック＆修正: validate → refactor
        let wants_check = lower.contains("check") || lower.contains("チェック");
        let wants_fix = lower.contains("fix")
            || lower.contains("修正")
            || lower.contains("直し")
            || lower.contains("repair");
        if wants_check && wants_fix {
            return vec![
                Step::new(
                    0,
                    format!("Validate: {target}"),
                    Some(CommandInvocation::new("validate", None, &[target.as_str()])),
                ),
                Step::new(
                    1,
                    format!("Refactor: {target}"),
                    Some(CommandInvocation::new("refactor", None, &[target.as_str()])),
                ),
            ];
        }

        // ── 1段プラン ──────────────────────────────────────────────────────

        if lower.contains("validate")
            || lower.contains("検証")
            || lower.contains("チェック")
            || lower.contains("check")
        {
            return vec![Step::new(
                0,
                format!("Validate: {target}"),
                Some(CommandInvocation::new("validate", None, &[target.as_str()])),
            )];
        }

        if lower.contains("refactor")
            || lower.contains("リファクタ")
            || lower.contains("改善")
            || lower.contains("整理")
            || lower.contains("restructure")
            || lower.contains("循環")
            || lower.contains("layer violation")
            || lower.contains("層違反")
        {
            return vec![Step::new(
                0,
                format!("Refactor: {target}"),
                Some(CommandInvocation::new(
                    if lower.contains("切る")
                        || lower.contains("直す")
                        || lower.contains("apply")
                        || lower.contains("適用")
                    {
                        "refactoring"
                    } else {
                        "refactor"
                    },
                    None,
                    &[target.as_str()],
                )),
            )];
        }

        if lower.contains("structure")
            || lower.contains("viewer")
            || lower.contains("graph")
            || lower.contains("見せて")
            || lower.contains("構造")
            || lower.contains("立体")
        {
            let args = vec![target.as_str()];
            let subcommand = if lower.contains("立体") || lower.contains("3d") {
                Some("3d")
            } else {
                Some("2d")
            };
            return vec![Step::new(
                0,
                format!("View structure: {target}"),
                Some(CommandInvocation::new("structure", subcommand, &args)),
            )];
        }

        if lower.contains("coding")
            || lower.contains("apply")
            || lower.contains("適用")
            || lower.contains("実装")
            || lower.contains("implement")
        {
            return vec![Step::new(
                0,
                format!("Apply coding changes: {target}"),
                Some(CommandInvocation::new("coding", None, &[target.as_str()])),
            )];
        }

        if lower.contains(" run ")
            || lower.starts_with("run ")
            || lower.contains("実行")
            || lower.contains("execute")
        {
            return vec![Step::new(
                0,
                format!("Run: {target}"),
                Some(CommandInvocation::new("exec", None, &[target.as_str()])),
            )];
        }

        if lower.contains("analyze")
            || lower.contains("analyse")
            || lower.contains("解析")
            || lower.contains("分析")
            || lower.contains("調べ")
            || lower.contains("audit")
        {
            return vec![Step::new(
                0,
                format!("Analyze: {target}"),
                Some(CommandInvocation::new(
                    "analyze",
                    Some("code"),
                    &[target.as_str()],
                )),
            )];
        }

        if lower.contains("design")
            || lower.contains("設計")
            || lower.contains("architecture")
            || lower.contains("アーキテクチャ")
        {
            return vec![Step::new(
                0,
                format!("Generate design: {target}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("design"),
                    &[target.as_str()],
                )),
            )];
        }

        if lower.contains("spec")
            || lower.contains("仕様")
            || lower.contains("specification")
            || lower.contains("要件")
        {
            return vec![Step::new(
                0,
                format!("Generate spec: {target}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("spec"),
                    &[target.as_str()],
                )),
            )];
        }

        // デフォルト: カレントディレクトリを解析
        vec![Step::new(
            0,
            format!("Analyze: {target}"),
            Some(CommandInvocation::new(
                "analyze",
                Some("code"),
                &[target.as_str()],
            )),
        )]
    }

    /// パス抽出。入力にパスがない場合は last_path、それもなければ "." を返す。
    fn extract_target_with_fallback(input: &str, last_path: Option<&str>) -> String {
        // パスに見えるトークンを探す: '/' を含む、'.' で始まる、'.rs'/'.toml' などで終わる
        for token in input.split_whitespace() {
            if Self::is_likely_path_token(token) {
                return token.to_string();
            }
        }

        // パスが見つからない自然文では last_path を優先する。
        if let Some(path) = last_path {
            return path.to_string();
        }

        // それでもフォールバックがない場合だけ、最後の path-like トークン候補を使う。
        let last_token = input
            .split_whitespace()
            .filter(|t| {
                !matches!(
                    *t,
                    "analyze"
                        | "analyse"
                        | "design"
                        | "validate"
                        | "refactor"
                        | "coding"
                        | "spec"
                        | "run"
                        | "execute"
                        | "the"
                        | "a"
                        | "an"
                        | "for"
                        | "of"
                        | "in"
                        | "to"
                        | "and"
                        | "して"
                        | "を"
                        | "の"
                        | "に"
                        | "で"
                )
            })
            .filter(|t| Self::is_likely_path_token(t))
            .last();

        match last_token {
            Some(t) => t.to_string(),
            // トークンが見つからない場合: last_path → "."
            None => last_path.unwrap_or(".").to_string(),
        }
    }

    fn is_likely_path_token(token: &str) -> bool {
        token.contains('/')
            || token.starts_with('.')
            || token.ends_with(".rs")
            || token.ends_with(".toml")
            || token.ends_with(".json")
            || token.ends_with(".yaml")
            || token.ends_with(".yml")
            || token.ends_with(".md")
    }
}

impl Default for RuleBasedPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::StepStatus;

    // ── 基本動作 ─────────────────────────────────────────────────────────

    #[test]
    fn plan_status_is_ready() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("anything", None);
        assert_eq!(plan.status, PlanStatus::Ready);
    }

    #[test]
    fn plan_steps_are_pending() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("anything", None);
        assert!(plan.steps.iter().all(|s| s.status == StepStatus::Pending));
    }

    #[test]
    fn plan_id_is_deterministic() {
        let p = RuleBasedPlanner::new();
        let plan1 = p.plan("design the api", None);
        let plan2 = p.plan("design the api", None);
        assert_eq!(plan1.id, plan2.id);
    }

    // ── 2段プラン ────────────────────────────────────────────────────────

    #[test]
    fn project_keyword_generates_two_step_plan() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze project .", None);
        assert_eq!(plan.steps.len(), 2, "project input should produce 2 steps");
        let cmd0 = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd0.name, "analyze");
        assert_eq!(cmd0.subcommand.as_deref(), Some("project"));
        let cmd1 = plan.steps[1].command.as_ref().unwrap();
        assert_eq!(cmd1.name, "generate");
        assert_eq!(cmd1.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn japanese_project_keyword_two_step_plan() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("プロジェクト全体を解析して", None);
        assert_eq!(plan.steps.len(), 2);
        let cmd0 = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd0.subcommand.as_deref(), Some("project"));
    }

    #[test]
    fn check_and_fix_generates_validate_then_refactor() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("check and fix the code", None);
        assert_eq!(plan.steps.len(), 2);
        let cmd0 = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd0.name, "validate");
        let cmd1 = plan.steps[1].command.as_ref().unwrap();
        assert_eq!(cmd1.name, "refactor");
    }

    #[test]
    fn japanese_check_fix_two_step_plan() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("チェックして修正して", None);
        assert_eq!(plan.steps.len(), 2);
    }

    // ── 1段プラン: validate ──────────────────────────────────────────────

    #[test]
    fn validate_keyword_generates_validate() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("validate the architecture", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "validate");
    }

    #[test]
    fn japanese_validate_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("アーキテクチャを検証して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "validate");
    }

    #[test]
    fn check_keyword_generates_validate() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("check the code", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "validate");
    }

    // ── 1段プラン: refactor ──────────────────────────────────────────────

    #[test]
    fn refactor_keyword_generates_refactor() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("refactor the module", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "refactor");
    }

    #[test]
    fn japanese_refactor_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("コードをリファクタリングして", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "refactor");
    }

    #[test]
    fn improve_keyword_generates_refactor() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("改善案を出して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "refactor");
    }

    // ── 1段プラン: coding ────────────────────────────────────────────────

    #[test]
    fn coding_keyword_generates_coding() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("apply the coding changes", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "coding");
    }

    #[test]
    fn implement_keyword_generates_coding() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("implement the changes", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "coding");
    }

    #[test]
    fn japanese_apply_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("変更を適用して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "coding");
    }

    // ── 1段プラン: exec (run) ────────────────────────────────────────────

    #[test]
    fn run_keyword_generates_exec() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("run main.rs", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "exec");
    }

    #[test]
    fn execute_keyword_generates_exec() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("execute the program", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "exec");
    }

    #[test]
    fn japanese_run_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("プログラムを実行して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "exec");
    }

    // ── 1段プラン: analyze ───────────────────────────────────────────────

    #[test]
    fn analyze_keyword_generates_analyze() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze the source code", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.subcommand.as_deref(), Some("code"));
    }

    #[test]
    fn japanese_analyze_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("コードを分析して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn audit_keyword_generates_analyze() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("audit the codebase", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    // ── 1段プラン: design ────────────────────────────────────────────────

    #[test]
    fn design_keyword_generates_design() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("design the database schema", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn japanese_design_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("設計書を書いて", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn architecture_keyword_generates_design() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("show me the architecture", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    // ── 1段プラン: spec ──────────────────────────────────────────────────

    #[test]
    fn spec_keyword_generates_spec() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("write a spec for the api", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn japanese_spec_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("仕様を作成して", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    // ── デフォルト ───────────────────────────────────────────────────────

    #[test]
    fn default_generates_analyze() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("build something", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    // ── パス抽出 ─────────────────────────────────────────────────────────

    #[test]
    fn path_with_slash_is_extracted() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze src/main.rs", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.args[0], "src/main.rs");
    }

    #[test]
    fn dot_path_is_extracted() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze .", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.args[0], ".");
    }

    #[test]
    fn rs_extension_path_is_extracted() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze main.rs", None);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.args[0], "main.rs");
    }

    // ── last_path フォールバック ──────────────────────────────────────────

    #[test]
    fn last_path_used_when_no_path_in_input() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("さっきのコードを検証して", Some("src/lib.rs"));
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.args[0], "src/lib.rs");
    }

    #[test]
    fn explicit_path_overrides_last_path() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze src/main.rs", Some("src/lib.rs"));
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.args[0], "src/main.rs");
    }
}
