/// Phase2: 逐次 Executor
///
/// Plan の各 Step を順番に CommandRegistry 経由で実行する。
/// エラー発生時は該当 Step を Failed にして即時停止する。
use crate::command::CommandRegistry;
use crate::plan::{Plan, PlanStatus, StepStatus};
use crate::session::AgentSession;

pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Plan を逐次実行する
    ///
    /// 各ステップを順に実行し、結果メッセージのリストを返す。
    /// いずれかのステップが失敗した場合はそこで停止する。
    pub fn execute(
        &self,
        plan: &mut Plan,
        session: &mut AgentSession,
        registry: &CommandRegistry,
    ) -> Vec<String> {
        plan.status = PlanStatus::Running;
        let mut outputs = Vec::new();

        for step in plan.steps.iter_mut() {
            step.status = StepStatus::Running;

            let result = if let Some(cmd) = &step.command {
                registry.execute(&cmd.name, cmd.subcommand.as_deref(), &cmd.args, session)
            } else {
                // コマンドなし → スキップ（Done扱い）
                step.status = StepStatus::Done;
                outputs.push(format!("[step {}] {} (skipped)", step.id, step.description));
                continue;
            };

            match result {
                Ok(output) => {
                    step.status = StepStatus::Done;
                    outputs.push(format!("[step {}] {}", step.id, output.message));
                }
                Err(e) => {
                    step.status = StepStatus::Failed;
                    outputs.push(format!("[step {}] Error: {e}", step.id));
                    plan.status = PlanStatus::Failed;
                    return outputs;
                }
            }
        }

        plan.status = PlanStatus::Completed;
        outputs
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRegistry;
    use crate::commands::register_defaults;
    use crate::plan::{CommandInvocation, Step};
    use crate::session::AgentSession;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        registry
    }

    #[test]
    fn executor_runs_single_step() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![Step::new(
            0,
            "Generate spec",
            Some(CommandInvocation::new("generate", Some("spec"), &["api"])),
        )];
        let mut plan = crate::plan::Plan::new("p1", steps);

        let outputs = executor.execute(&mut plan, &mut session, &registry);
        assert_eq!(plan.status, PlanStatus::Completed);
        assert_eq!(plan.steps[0].status, StepStatus::Done);
        assert!(!outputs.is_empty());
        assert!(outputs[0].contains("# Spec: api"));
    }

    #[test]
    fn executor_fails_on_unknown_command() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![Step::new(
            0,
            "Unknown",
            Some(CommandInvocation::new("nonexistent", None, &[])),
        )];
        let mut plan = crate::plan::Plan::new("p2", steps);

        let outputs = executor.execute(&mut plan, &mut session, &registry);
        assert_eq!(plan.status, PlanStatus::Failed);
        assert_eq!(plan.steps[0].status, StepStatus::Failed);
        assert!(outputs[0].contains("Error"));
    }

    #[test]
    fn executor_stops_on_first_failure() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![
            Step::new(
                0,
                "Bad step",
                Some(CommandInvocation::new("bad", None, &[])),
            ),
            Step::new(
                1,
                "Good step",
                Some(CommandInvocation::new("generate", Some("spec"), &["x"])),
            ),
        ];
        let mut plan = crate::plan::Plan::new("p3", steps);

        let outputs = executor.execute(&mut plan, &mut session, &registry);
        assert_eq!(plan.status, PlanStatus::Failed);
        assert_eq!(plan.steps[0].status, StepStatus::Failed);
        // step1 は実行されていない
        assert_eq!(plan.steps[1].status, StepStatus::Pending);
        assert_eq!(outputs.len(), 1);
    }

    #[test]
    fn executor_skips_step_without_command() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![Step::new(0, "No-op step", None)];
        let mut plan = crate::plan::Plan::new("p4", steps);

        let outputs = executor.execute(&mut plan, &mut session, &registry);
        assert_eq!(plan.status, PlanStatus::Completed);
        assert_eq!(plan.steps[0].status, StepStatus::Done);
        assert!(outputs[0].contains("skipped"));
    }

    #[test]
    fn executor_runs_multiple_steps() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![
            Step::new(
                0,
                "spec",
                Some(CommandInvocation::new("generate", Some("spec"), &["cli"])),
            ),
            Step::new(
                1,
                "design",
                Some(CommandInvocation::new("generate", Some("design"), &["cli"])),
            ),
        ];
        let mut plan = crate::plan::Plan::new("p5", steps);

        let outputs = executor.execute(&mut plan, &mut session, &registry);
        assert_eq!(plan.status, PlanStatus::Completed);
        assert_eq!(outputs.len(), 2);
        assert!(outputs[0].contains("# Spec: cli"));
        assert!(outputs[1].contains("# Design: cli"));
    }
}
