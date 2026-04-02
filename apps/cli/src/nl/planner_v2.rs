use crate::nl::context::merge_target;
use crate::nl::intent::{
    wants_analyze, wants_coding, wants_memory, wants_rules, wants_run, wants_structure_view,
    wants_validate,
};
use crate::session::AgentSession;

use super::session::ConversationState;
use super::types::{CodingOptions, CommandPlan, PlannedStep};

fn mentions_pr(lower: &str) -> bool {
    lower.contains("pull request")
        || lower.contains("pr作")
        || lower.contains("prを")
        || lower.contains("prして")
        || lower
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '。' | '、' | ';' | ':'))
            .any(|token| token == "pr")
}

pub fn plan_input(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> Option<CommandPlan> {
    let lower = input.to_lowercase();
    if lower.contains("whole project")
        || lower.contains("analyze project")
        || lower.contains("project .")
    {
        return None;
    }

    let merged = merge_target(input, session, conversation);
    let mut steps = Vec::new();

    if lower.contains("undo") || lower.contains("戻して") {
        steps.push(PlannedStep::StructureUndo(merged.path));
        return Some(CommandPlan { steps });
    }
    if lower.contains("redo") || lower.contains("やり直") {
        steps.push(PlannedStep::StructureRedo(merged.path));
        return Some(CommandPlan { steps });
    }
    if (lower.contains("gui") || lower.contains("viewer") || lower.contains("差分"))
        && (lower.contains("diff") || lower.contains("差分"))
    {
        steps.push(PlannedStep::StructureDiff(
            merged.path,
            merged.node.clone(),
        ));
        return Some(CommandPlan { steps });
    }

    if wants_structure_view(&lower) {
        steps.push(PlannedStep::StructureView(merged.path));
        return Some(CommandPlan { steps });
    }

    if wants_rules(&lower) {
        steps.push(PlannedStep::Rules);
        return Some(CommandPlan { steps });
    }

    if wants_memory(&lower) {
        steps.push(PlannedStep::Memory(merged.path));
        return Some(CommandPlan { steps });
    }

    let analyze_first = wants_analyze(&lower)
        || (wants_coding(&lower)
            && ["unsafe", "循環", "cycle", "problem", "問題"]
                .iter()
                .any(|keyword| lower.contains(keyword)));

    if analyze_first {
        steps.push(PlannedStep::Analyze(merged.path.clone()));
    }

    if wants_coding(&lower) {
        steps.push(PlannedStep::Coding(
            merged.path.clone(),
            CodingOptions::default(),
        ));
    }

    if wants_validate(&lower) {
        steps.push(PlannedStep::Validate(merged.path.clone()));
    }

    if lower.contains("commit") || lower.contains("コミット") {
        steps.push(PlannedStep::GitCommit(merged.path.clone()));
    }

    if mentions_pr(&lower) || lower.contains("pushして") {
        steps.push(PlannedStep::GitPR(merged.path.clone()));
    }

    if wants_run(&lower) {
        steps.push(PlannedStep::Run(merged.path.clone()));
    }

    if steps.is_empty() && wants_analyze(&lower) {
        steps.push(PlannedStep::Analyze(merged.path));
    }

    if steps.is_empty() {
        None
    } else {
        Some(CommandPlan { steps })
    }
}

pub fn update_conversation_after_plan(
    input: &str,
    plan: &CommandPlan,
    conversation: &mut ConversationState,
) {
    conversation.last_plan = Some(plan.clone());
    for step in &plan.steps {
        match step {
            PlannedStep::Analyze(path)
            | PlannedStep::Validate(path)
            | PlannedStep::Run(path)
            | PlannedStep::Memory(path)
            | PlannedStep::GitCommit(path)
            | PlannedStep::GitPR(path)
            | PlannedStep::StructureView(path)
            | PlannedStep::StructureEdit(path)
            | PlannedStep::StructureUndo(path)
            | PlannedStep::StructureRedo(path)
            | PlannedStep::Coding(path, _) => conversation.last_target = Some(path.clone()),
            PlannedStep::StructureDiff(path, node) => {
                conversation.last_target = Some(path.clone());
                if let Some(node) = node {
                    conversation.last_node = Some(node.clone());
                }
            }
            PlannedStep::Rules => {}
        }
    }

    let lower = input.to_lowercase();
    if lower.contains("presentation") {
        conversation.last_node = Some("presentation".to_string());
    } else if lower.contains("viewer") || lower.contains("gui") {
        conversation.last_node = Some("viewer".to_string());
    }

    if lower.contains("解析") || lower.contains("analyze") {
        conversation.last_analysis_summary = Some("analysis requested".to_string());
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nl::session::ConversationState;
    use crate::nl::types::PlannedStep;

    #[test]
    fn ambiguous_turn_inherits_target_and_node() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from(".")),
            last_node: Some("presentation".to_string()),
            ..ConversationState::default()
        };
        let plan = plan_input("presentation layer 側だけ直して", &session, &conversation)
            .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(PathBuf::from("."), CodingOptions::default())]
        );
    }

    #[test]
    fn viewer_undo_maps_to_structure_undo() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from(".")),
            last_viewer_session: Some("viewer-session".to_string()),
            ..ConversationState::default()
        };
        let plan = plan_input("1つ戻して", &session, &conversation).expect("plan");
        assert_eq!(plan.steps, vec![PlannedStep::StructureUndo(PathBuf::from("."))]);
    }

    #[test]
    fn commit_and_pr_expands_to_git_steps() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from(".")),
            ..ConversationState::default()
        };
        let plan = plan_input("commitしてPR作って", &session, &conversation).expect("plan");
        assert_eq!(
            plan.steps,
            vec![
                PlannedStep::GitCommit(PathBuf::from(".")),
                PlannedStep::GitPR(PathBuf::from("."))
            ]
        );
    }
}
