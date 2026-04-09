use crate::nl::context::merge_target;
use crate::nl::intent::{
    wants_analyze, wants_coding, wants_memory, wants_rules, wants_run, wants_structure_view,
    wants_validate,
};
use crate::nl::language_intent_bridge::{PlannerIntent, infer_planner_intent};
use crate::nl::target::has_explicit_target_reference;
use crate::session::AgentSession;

use super::session::ConversationState;
use super::types::{
    CodingOptions, CommandPlan, IntentType, PlannedStep, ResolvedTarget, SupportedLanguage,
};

/// R1: only validated explicit targets can force a targeted coding route.
fn has_file_path_target(input: &str) -> bool {
    has_explicit_target_reference(input)
}

/// R2: mutation verbs that require Coding intent when a file target is present
fn wants_mutation_verb(lower: &str) -> bool {
    [
        "修正",
        "変更",
        "改善",
        "追加",
        "追加する",
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

fn should_use_semantic_frontend(intent: &PlannerIntent) -> bool {
    intent.mixed_language
        || intent.detected_language == SupportedLanguage::Japanese
        || matches!(
            intent.primary_intent,
            IntentType::RulesLearn | IntentType::RulesList | IntentType::MetaPlannerEdit
        )
        || intent.secondary_intents.iter().any(|secondary| {
            matches!(
                secondary,
                IntentType::RulesLearn | IntentType::MetaPlannerEdit
            )
        })
}

fn planner_edit_target() -> std::path::PathBuf {
    std::path::PathBuf::from("apps/cli/src/nl/planner_v2.rs")
}

fn is_meta_planner_intent(intent: &PlannerIntent) -> bool {
    intent.primary_intent == IntentType::MetaPlannerEdit
        || intent
            .secondary_intents
            .contains(&IntentType::MetaPlannerEdit)
}

fn generate_multi_step_plan(
    input: &str,
    intent: &PlannerIntent,
    merged: &ResolvedTarget,
) -> CommandPlan {
    let coding_target =
        if is_meta_planner_intent(intent) || intent.primary_intent == IntentType::RulesLearn {
            planner_edit_target()
        } else {
            merged.path.clone()
        };

    CommandPlan {
        steps: vec![
            PlannedStep::Analyze(merged.path.clone()),
            PlannedStep::Coding(coding_target, coding_options_for_request(input)),
            PlannedStep::Validate(merged.path.clone()),
        ],
    }
}

fn synthesize_steps_from_intent(
    intent: &PlannerIntent,
    input: &str,
    merged: &ResolvedTarget,
) -> Option<CommandPlan> {
    if intent.primary_intent == IntentType::Unknown && intent.secondary_intents.is_empty() {
        return None;
    }

    if intent.ambiguity_score > 0.35
        && (is_meta_planner_intent(intent)
            || intent.primary_intent == IntentType::RulesLearn
            || intent.secondary_intents.iter().any(|secondary| {
                matches!(
                    secondary,
                    IntentType::MetaPlannerEdit | IntentType::RulesLearn
                )
            }))
    {
        return Some(generate_multi_step_plan(input, intent, merged));
    }

    let mut steps = Vec::new();
    match intent.primary_intent {
        IntentType::RulesList => steps.push(PlannedStep::Rules),
        IntentType::StructureView => steps.push(PlannedStep::StructureView(merged.path.clone())),
        IntentType::Validate => steps.push(PlannedStep::Validate(merged.path.clone())),
        IntentType::AnalyzeArchitecture => steps.push(PlannedStep::Analyze(merged.path.clone())),
        IntentType::CodingEdit => steps.push(PlannedStep::Coding(
            merged.path.clone(),
            coding_options_for_request(input),
        )),
        IntentType::RulesLearn | IntentType::MetaPlannerEdit => {
            return Some(generate_multi_step_plan(input, intent, merged));
        }
        _ => {}
    }

    for secondary in &intent.secondary_intents {
        match secondary {
            IntentType::AnalyzeArchitecture => {
                if !steps
                    .iter()
                    .any(|step| matches!(step, PlannedStep::Analyze(_)))
                {
                    steps.insert(0, PlannedStep::Analyze(merged.path.clone()));
                }
            }
            IntentType::CodingEdit => {
                if !steps
                    .iter()
                    .any(|step| matches!(step, PlannedStep::Coding(_, _)))
                {
                    steps.push(PlannedStep::Coding(
                        merged.path.clone(),
                        coding_options_for_request(input),
                    ));
                }
            }
            IntentType::Validate => {
                if !steps
                    .iter()
                    .any(|step| matches!(step, PlannedStep::Validate(_)))
                {
                    steps.push(PlannedStep::Validate(merged.path.clone()));
                }
            }
            IntentType::MetaPlannerEdit | IntentType::RulesLearn => {
                return Some(generate_multi_step_plan(input, intent, merged));
            }
            _ => {}
        }
    }

    if steps.is_empty() {
        None
    } else {
        Some(CommandPlan { steps })
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

    if wants_memory(&lower) {
        steps.push(PlannedStep::Memory(merged.path));
        return Some(CommandPlan { steps });
    }

    // R1+R3: file target + mutation verb → force Coding with file target.
    // Previous-target reuse (R4) is handled by merge_target falling back to
    // conversation.last_target when no explicit path is present.
    let contextual_mutation_route = wants_mutation_verb(&lower)
        && (has_file_path_target(input)
            || references_previous_target(&lower)
            || is_targeted_continuation(input, conversation, &merged)
            || conversation.last_target.is_some());
    if contextual_mutation_route {
        steps.push(PlannedStep::Coding(
            merged.path.clone(),
            coding_options_for_request(input),
        ));
        return Some(CommandPlan { steps });
    }

    let has_git_route = lower.contains("commit")
        || lower.contains("コミット")
        || mentions_pr(&lower)
        || lower.contains("pushして");
    if !has_git_route {
        let semantic_intent = infer_planner_intent(input, conversation);
        if should_use_semantic_frontend(&semantic_intent)
            && let Some(plan) = synthesize_steps_from_intent(&semantic_intent, input, &merged)
        {
            return Some(plan);
        }
    }

    if wants_rules(&lower) {
        steps.push(PlannedStep::Rules);
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
    use crate::nl::language_intent_bridge::infer_planner_intent;
    use crate::nl::session::ConversationState;
    use crate::nl::types::PlannedStep;

    fn assert_deterministic_intent(input: &str, conversation: &ConversationState) {
        let intents = std::iter::repeat_with(|| infer_planner_intent(input, conversation))
            .take(2)
            .collect::<Vec<_>>();
        assert!(intents.windows(2).all(|pair| pair[0] == pair[1]));
    }

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

    #[test]
    fn explicit_target_sentence_updates_context_target() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/previous.rs")),
            ..ConversationState::default()
        };
        let plan = plan_input(
            "Semantic Interface Extraction Guard を追加する。\n対象は apps/cli/src/coding.rs。",
            &session,
            &conversation,
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("apps/cli/src/coding.rs"),
                coding_options_for_request(
                    "Semantic Interface Extraction Guard を追加する。\n対象は apps/cli/src/coding.rs。"
                )
            )]
        );
    }

    #[test]
    fn previous_canonical_target_survives_multiline_spec() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let plan = plan_input(
            "Semantic Interface Extraction Guard を追加する。\nImportRebinding-only の diff では *_interface.rs を生成しない。",
            &session,
            &conversation,
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("apps/cli/src/coding.rs"),
                coding_options_for_request(
                    "Semantic Interface Extraction Guard を追加する。\nImportRebinding-only の diff では *_interface.rs を生成しない。"
                )
            )]
        );
    }

    #[test]
    fn wildcard_suffix_in_prose_does_not_update_context_target() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let plan = plan_input(
            "ImportRebinding-only の diff では *_interface.rs を生成しないので修正して",
            &session,
            &conversation,
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("apps/cli/src/coding.rs"),
                coding_options_for_request(
                    "ImportRebinding-only の diff では *_interface.rs を生成しないので修正して"
                )
            )]
        );
    }

    #[test]
    fn quoted_semantic_learn_routes_to_planner_edit_plan() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        let plan = plan_input(
            "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して",
            &session,
            &conversation,
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![
                PlannedStep::Analyze(PathBuf::from(".")),
                PlannedStep::Coding(
                    PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                    coding_options_for_request(
                        "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して"
                    )
                ),
                PlannedStep::Validate(PathBuf::from(".")),
            ]
        );
    }

    #[test]
    fn rules_list_remains_single_step() {
        let session = AgentSession::new();
        let plan = plan_input("rules list", &session, &ConversationState::default()).expect("plan");
        assert_eq!(plan.steps, vec![PlannedStep::Rules]);
    }

    #[test]
    fn intent_inference_is_deterministic() {
        assert_deterministic_intent(
            "さっきの unresolved import 失敗から学習して次回は自動修正して",
            &ConversationState {
                last_target: Some(PathBuf::from(".")),
                ..ConversationState::default()
            },
        );
    }
}
