use crate::nl::context::merge_target;
use crate::nl::intent::{
    wants_analyze, wants_coding, wants_memory, wants_rules, wants_run, wants_structure_view,
    wants_validate,
};
use crate::session::AgentSession;

use super::session::ConversationState;
use super::types::{CodingOptions, CommandPlan, PlannedStep};

/// R1: detect file-path tokens in raw input (.rs / .toml / .md / src/ / apps/ / crates/)
fn has_file_path_target(input: &str) -> bool {
    input.split_whitespace().any(|raw| {
        let token = raw.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '。' | '.' | '、' | ':' | ';' | '"' | '\'' | '「' | '」' | '(' | ')'
            )
        });
        token.ends_with(".rs")
            || token.ends_with(".toml")
            || token.ends_with(".md")
            || token.contains("src/")
            || token.contains("apps/")
            || token.contains("crates/")
    })
}

/// R2: mutation verbs that require Coding intent when a file target is present
fn wants_mutation_verb(lower: &str) -> bool {
    [
        "修正",
        "改善",
        "厳密化",
        "除去",
        "最適化",
        "直して",
        "直す",
        "refactor",
        "prune",
        "rebind",
        "fix",
        "repair",
        "抽象化",
        "trait",
        "registry",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

/// R4: previous-target reuse phrases ("さっきの場所", "前回", "さっきのファイル")
fn references_previous_target(lower: &str) -> bool {
    ["さっきの", "前回", "さきほど", "先ほど"]
        .iter()
        .any(|k| lower.contains(k))
}

fn mentions_pr(lower: &str) -> bool {
    lower.contains("pull request")
        || lower.contains("pr作")
        || lower.contains("prを")
        || lower.contains("prして")
        || lower
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '。' | '、' | ';' | ':'))
            .any(|token| token == "pr")
}

fn is_targeted_continuation(
    input: &str,
    conversation: &ConversationState,
    merged: &super::types::ResolvedTarget,
) -> bool {
    !has_file_path_target(input)
        && !references_previous_target(&input.to_lowercase())
        && conversation.last_target.is_some()
        && merged.path != std::path::PathBuf::from(".")
}

fn coding_options_for_request(input: &str) -> CodingOptions {
    CodingOptions {
        request: Some(input.to_string()),
        ..CodingOptions::default()
    }
}

pub fn plan_input(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> Option<CommandPlan> {
    // R1: continuation 上の exact "coding --apply" → ApplyPreviousCodingStep へ昇格。
    // generic planner を bypass し、前回 dry-run transaction を再利用する (R2)。
    if input.trim() == "coding --apply"
        && (conversation.has_pending_coding_transaction() || conversation.has_reapply_guard())
    {
        return Some(CommandPlan {
            steps: vec![PlannedStep::ApplyPreviousCodingStep],
        });
    }

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
        steps.push(PlannedStep::StructureDiff(merged.path, merged.node.clone()));
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

    // R1+R3: file target + mutation verb → force Coding with file target.
    // Previous-target reuse (R4) is handled by merge_target falling back to
    // conversation.last_target when no explicit path is present.
    if (has_file_path_target(input)
        || references_previous_target(&lower)
        || is_targeted_continuation(input, conversation, &merged))
        && wants_mutation_verb(&lower)
    {
        steps.push(PlannedStep::Coding(
            merged.path.clone(),
            coding_options_for_request(input),
        ));
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
            coding_options_for_request(input),
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
            // R2: ApplyPreviousCodingStep は last_target を更新しない。
            // 前回 Coding step が設定した last_target を continuity のためそのまま保持する (R3)。
            PlannedStep::ApplyPreviousCodingStep => {}
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
        let plan =
            plan_input("presentation layer 側だけ直して", &session, &conversation).expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("."),
                coding_options_for_request("presentation layer 側だけ直して")
            )]
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
        assert_eq!(
            plan.steps,
            vec![PlannedStep::StructureUndo(PathBuf::from("."))]
        );
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

    #[test]
    fn continuation_prompt_reuses_last_target_via_v2() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let plan = plan_input("trait + registry に抽象化して", &session, &conversation)
            .expect("continuation plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("apps/cli/src/coding.rs"),
                coding_options_for_request("trait + registry に抽象化して")
            )]
        );
    }
}
