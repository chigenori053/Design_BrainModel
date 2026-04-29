/// Phase2: Plan 型定義
///
/// Planner が生成し、Executor が実行する計画を表す。
/// 各 Step は CommandInvocation（コマンド呼び出し仕様）を持つ。
/// コマンド呼び出しの仕様
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub name: String,
    pub subcommand: Option<String>,
    pub args: Vec<String>,
}

impl CommandInvocation {
    pub fn new(name: impl Into<String>, subcommand: Option<&str>, args: &[&str]) -> Self {
        Self {
            name: name.into(),
            subcommand: subcommand.map(|s| s.to_string()),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// ステップの実行状態
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Done,
    Failed,
}

impl StepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

/// プランの1ステップ
#[derive(Clone, Debug)]
pub struct Step {
    pub id: usize,
    pub description: String,
    pub command: Option<CommandInvocation>,
    pub status: StepStatus,
}

impl Step {
    pub fn new(
        id: usize,
        description: impl Into<String>,
        command: Option<CommandInvocation>,
    ) -> Self {
        Self {
            id,
            description: description.into(),
            command,
            status: StepStatus::Pending,
        }
    }
}

/// プラン全体の状態
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlanStatus {
    #[default]
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// 実行計画
#[derive(Clone, Debug)]
pub struct Plan {
    pub id: String,
    pub steps: Vec<Step>,
    pub status: PlanStatus,
}

impl Plan {
    pub fn new(id: impl Into<String>, steps: Vec<Step>) -> Self {
        Self {
            id: id.into(),
            steps,
            status: PlanStatus::Pending,
        }
    }

    /// 全ステップが Done かどうか
    pub fn is_completed(&self) -> bool {
        !self.steps.is_empty() && self.steps.iter().all(|s| s.status == StepStatus::Done)
    }

    /// いずれかのステップが Failed かどうか
    pub fn has_failed(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_new_defaults_to_pending() {
        let step = Step::new(0, "test", None);
        assert_eq!(step.status, StepStatus::Pending);
        assert_eq!(step.id, 0);
    }

    #[test]
    fn plan_is_completed_when_all_done() {
        let mut plan = Plan::new(
            "p1",
            vec![Step::new(0, "step0", None), Step::new(1, "step1", None)],
        );
        assert!(!plan.is_completed());
        plan.steps[0].status = StepStatus::Done;
        plan.steps[1].status = StepStatus::Done;
        assert!(plan.is_completed());
    }

    #[test]
    fn plan_has_failed_when_any_failed() {
        let mut plan = Plan::new("p1", vec![Step::new(0, "step0", None)]);
        assert!(!plan.has_failed());
        plan.steps[0].status = StepStatus::Failed;
        assert!(plan.has_failed());
    }

    #[test]
    fn command_invocation_new() {
        let cmd = CommandInvocation::new("generate", Some("spec"), &["cli"]);
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand, Some("spec".to_string()));
        assert_eq!(cmd.args, vec!["cli".to_string()]);
    }

    #[test]
    fn empty_plan_is_not_completed() {
        let plan = Plan::new("empty", vec![]);
        assert!(!plan.is_completed());
    }

    #[test]
    fn step_status_as_str() {
        assert_eq!(StepStatus::Pending.as_str(), "pending");
        assert_eq!(StepStatus::Running.as_str(), "running");
        assert_eq!(StepStatus::Done.as_str(), "done");
        assert_eq!(StepStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn plan_status_as_str() {
        assert_eq!(PlanStatus::Pending.as_str(), "pending");
        assert_eq!(PlanStatus::Ready.as_str(), "ready");
        assert_eq!(PlanStatus::Running.as_str(), "running");
        assert_eq!(PlanStatus::Completed.as_str(), "completed");
        assert_eq!(PlanStatus::Failed.as_str(), "failed");
    }
}
