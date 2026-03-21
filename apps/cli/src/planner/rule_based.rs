/// Phase2: ルールベース Planner
///
/// キーワードマッチングによる決定論的プラン生成（LLM不使用）。
///
/// ルール（優先順）：
/// 1. "analyze" / "分析" / "解析" → /analyze code <target>
/// 2. "design" / "設計"           → /generate design <target>
/// 3. "spec" / "仕様"             → /generate spec <target>
/// 4. それ以外（デフォルト）       → /generate spec <target>
use crate::plan::{CommandInvocation, Plan, PlanStatus, Step};

pub struct RuleBasedPlanner;

impl RuleBasedPlanner {
    pub fn new() -> Self {
        Self
    }

    /// テキストから Plan を生成する（常に成功）
    pub fn plan(&self, input: &str) -> Plan {
        let id = Self::make_id(input);
        let steps = Self::build_steps(input);
        let mut plan = Plan::new(id, steps);
        plan.status = PlanStatus::Ready;
        plan
    }

    fn make_id(input: &str) -> String {
        let trimmed: String = input.chars().take(32).collect();
        format!("plan:{}", trimmed.replace(' ', "_"))
    }

    fn build_steps(input: &str) -> Vec<Step> {
        let lower = input.to_lowercase();
        let target = Self::extract_target(input);

        if lower.contains("project") || lower.contains("プロジェクト") {
            // プロジェクト解析 → 設計生成の2段プラン
            vec![
                Step::new(
                    0,
                    format!("Analyze project: {input}"),
                    Some(CommandInvocation::new(
                        "analyze",
                        Some("project"),
                        &[target.as_str()],
                    )),
                ),
                Step::new(
                    1,
                    format!("Generate design: {input}"),
                    Some(CommandInvocation::new(
                        "generate",
                        Some("design"),
                        &[target.as_str()],
                    )),
                ),
            ]
        } else if lower.contains("analyze") || lower.contains("解析") || lower.contains("分析")
        {
            vec![Step::new(
                0,
                format!("Analysis: {input}"),
                Some(CommandInvocation::new(
                    "analyze",
                    Some("code"),
                    &[target.as_str()],
                )),
            )]
        } else if lower.contains("design") || lower.contains("設計") {
            vec![Step::new(
                0,
                format!("Generate design: {input}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("design"),
                    &[target.as_str()],
                )),
            )]
        } else if lower.contains("spec") || lower.contains("仕様") {
            vec![Step::new(
                0,
                format!("Generate spec: {input}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("spec"),
                    &[target.as_str()],
                )),
            )]
        } else {
            vec![Step::new(
                0,
                format!("Generate spec: {input}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("spec"),
                    &[target.as_str()],
                )),
            )]
        }
    }

    fn extract_target(input: &str) -> String {
        input
            .split_whitespace()
            .last()
            .unwrap_or("target")
            .to_string()
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

    #[test]
    fn default_generates_spec() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("build something");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn spec_keyword_generates_spec() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("write a spec for the api");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn design_keyword_generates_design() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("design the database schema");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn analyze_keyword_generates_analyze() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze the source code");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.subcommand.as_deref(), Some("code"));
    }

    #[test]
    fn plan_status_is_ready() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("anything");
        assert_eq!(plan.status, PlanStatus::Ready);
    }

    #[test]
    fn plan_steps_are_pending() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("anything");
        assert!(plan.steps.iter().all(|s| s.status == StepStatus::Pending));
    }

    #[test]
    fn plan_id_is_deterministic() {
        let p = RuleBasedPlanner::new();
        let plan1 = p.plan("design the api");
        let plan2 = p.plan("design the api");
        assert_eq!(plan1.id, plan2.id);
    }

    #[test]
    fn japanese_spec_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("仕様を作成して");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn japanese_design_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("設計書を書いて");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn japanese_analyze_keyword() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("コードを分析して");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn project_keyword_generates_two_step_plan() {
        let p = RuleBasedPlanner::new();
        let plan = p.plan("analyze project .");
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
        let plan = p.plan("プロジェクト全体を解析して");
        assert_eq!(plan.steps.len(), 2);
        let cmd0 = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd0.subcommand.as_deref(), Some("project"));
    }
}
