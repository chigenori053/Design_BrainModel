/// Phase3: DBM出力 → CLI Plan変換
///
/// ArchitectureResult / AnalysisResult を Plan に変換する。
///
/// 変換例:
///   DBM output: { actions: [{ type: "generate_spec", target: "cli" }] }
///   CLI Plan:   Step0: /generate spec cli
use crate::dbm::analyzer::{AnalysisResult, Complexity, ProjectAnalysisResult};
use crate::dbm::client::ArchitectureResult;
use crate::plan::{CommandInvocation, Plan, PlanStatus, Step};

pub struct Translator;

impl Translator {
    pub fn new() -> Self {
        Self
    }

    /// ArchitectureResult → Plan
    ///
    /// result.actions の各エントリーを CommandInvocation に変換した Step を生成する。
    pub fn architecture_to_plan(&self, result: &ArchitectureResult, input: &str) -> Plan {
        let id = make_id(input);
        let steps: Vec<Step> = result
            .actions
            .iter()
            .enumerate()
            .map(|(i, action)| {
                let cmd = action_to_invocation(&action.action_type, &action.target);
                Step::new(
                    i,
                    format!("{}: {}", action.action_type, action.target),
                    Some(cmd),
                )
            })
            .collect();

        // actions が空なら generate spec にフォールバック
        let steps = if steps.is_empty() {
            let target = last_token(input);
            vec![Step::new(
                0,
                format!("generate_spec: {target}"),
                Some(CommandInvocation::new(
                    "generate",
                    Some("spec"),
                    &[target.as_str()],
                )),
            )]
        } else {
            steps
        };

        let mut plan = Plan::new(id, steps);
        plan.status = PlanStatus::Ready;
        plan
    }

    /// AnalysisResult → Plan
    pub fn analysis_to_plan(&self, result: &AnalysisResult, input: &str) -> Plan {
        let id = make_id(input);
        let step = Step::new(
            0,
            format!("Analysis: {}", result.path),
            Some(CommandInvocation::new(
                "analyze",
                Some("code"),
                &[result.path.as_str()],
            )),
        );
        let mut plan = Plan::new(id, vec![step]);
        plan.status = PlanStatus::Ready;
        plan
    }

    /// ProjectAnalysisResult → Plan
    ///
    /// プロジェクト解析結果から多段 Plan を生成する。
    /// 複雑度の高いモジュールが存在する場合は設計生成ステップを追加する。
    pub fn project_to_plan(&self, result: &ProjectAnalysisResult, input: &str) -> Plan {
        let id = make_id(input);
        let mut steps = Vec::new();

        // Step 0: プロジェクト解析（参照用）
        steps.push(Step::new(
            0,
            format!(
                "Analysis: {} files across {} modules",
                result.summary.total_files,
                result.modules.len()
            ),
            Some(CommandInvocation::new("analyze", Some("project"), &["."])),
        ));

        // Step 1: 複雑度が高い場合は設計生成を追加
        let has_high = result
            .files
            .iter()
            .any(|f| f.complexity == Complexity::High)
            || result.summary.avg_complexity == Complexity::High;
        if has_high {
            steps.push(Step::new(
                1,
                "Generate design: refactor".to_string(),
                Some(CommandInvocation::new(
                    "generate",
                    Some("design"),
                    &["refactor"],
                )),
            ));
        }

        let mut plan = Plan::new(id, steps);
        plan.status = PlanStatus::Ready;
        plan
    }
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
}

fn action_to_invocation(action_type: &str, target: &str) -> CommandInvocation {
    match action_type {
        "generate_spec" => CommandInvocation::new("generate", Some("spec"), &[target]),
        "generate_design" => CommandInvocation::new("generate", Some("design"), &[target]),
        "analyze_code" => CommandInvocation::new("analyze", Some("code"), &[target]),
        _ => CommandInvocation::new("generate", Some("spec"), &[target]),
    }
}

fn make_id(input: &str) -> String {
    let trimmed: String = input.chars().take(32).collect();
    format!("plan:{}", trimmed.replace(' ', "_"))
}

fn last_token(input: &str) -> String {
    input
        .split_whitespace()
        .last()
        .unwrap_or("target")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbm::client::ArchitectureAction;
    use crate::plan::StepStatus;

    fn make_arch_result(actions: Vec<(&str, &str)>) -> ArchitectureResult {
        ArchitectureResult {
            intent: "test".to_string(),
            components: vec![],
            layers: vec![],
            actions: actions
                .into_iter()
                .map(|(t, tgt)| ArchitectureAction {
                    action_type: t.to_string(),
                    target: tgt.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn architecture_to_plan_maps_generate_spec() {
        let t = Translator::new();
        let result = make_arch_result(vec![("generate_spec", "cli")]);
        let plan = t.architecture_to_plan(&result, "spec for cli");
        assert_eq!(plan.steps.len(), 1);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
        assert_eq!(cmd.args, vec!["cli"]);
    }

    #[test]
    fn architecture_to_plan_maps_generate_design() {
        let t = Translator::new();
        let result = make_arch_result(vec![("generate_design", "api")]);
        let plan = t.architecture_to_plan(&result, "design api");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
        assert_eq!(cmd.args, vec!["api"]);
    }

    #[test]
    fn architecture_to_plan_maps_analyze_code() {
        let t = Translator::new();
        let result = make_arch_result(vec![("analyze_code", "src/")]);
        let plan = t.architecture_to_plan(&result, "analyze src/");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.subcommand.as_deref(), Some("code"));
    }

    #[test]
    fn architecture_to_plan_unknown_action_falls_back_to_spec() {
        let t = Translator::new();
        let result = make_arch_result(vec![("unknown_action", "target")]);
        let plan = t.architecture_to_plan(&result, "something");
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn architecture_to_plan_empty_actions_defaults_to_spec() {
        let t = Translator::new();
        let result = make_arch_result(vec![]);
        let plan = t.architecture_to_plan(&result, "do something");
        assert_eq!(plan.steps.len(), 1);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
    }

    #[test]
    fn architecture_to_plan_multiple_actions() {
        let t = Translator::new();
        let result = make_arch_result(vec![("generate_spec", "api"), ("generate_design", "api")]);
        let plan = t.architecture_to_plan(&result, "spec and design for api");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].id, 0);
        assert_eq!(plan.steps[1].id, 1);
    }

    #[test]
    fn architecture_to_plan_status_is_ready() {
        let t = Translator::new();
        let result = make_arch_result(vec![("generate_spec", "x")]);
        let plan = t.architecture_to_plan(&result, "x");
        assert_eq!(plan.status, crate::plan::PlanStatus::Ready);
    }

    #[test]
    fn analysis_to_plan_creates_analyze_step() {
        let t = Translator::new();
        let result = AnalysisResult {
            path: "src/".to_string(),
            modules: vec![],
            total_lines: 0,
            suggestions: vec![],
        };
        let plan = t.analysis_to_plan(&result, "analyze src/");
        assert_eq!(plan.steps.len(), 1);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.subcommand.as_deref(), Some("code"));
        assert_eq!(cmd.args, vec!["src/"]);
    }

    #[test]
    fn steps_default_to_pending() {
        let t = Translator::new();
        let result = make_arch_result(vec![("generate_spec", "foo")]);
        let plan = t.architecture_to_plan(&result, "foo");
        assert!(plan.steps.iter().all(|s| s.status == StepStatus::Pending));
    }
}
