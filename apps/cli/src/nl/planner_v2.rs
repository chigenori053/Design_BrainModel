use crate::nl::context::merge_target;
use crate::nl::intent::{
    wants_analyze, wants_coding, wants_memory, wants_rules, wants_run, wants_structure_view,
    wants_validate,
};
use crate::nl::language_intent_bridge::{PlannerIntent, infer_planner_intent};
use crate::nl::target::{extract_explicit_path, has_explicit_target_reference};
use crate::session::AgentSession;

use super::session::ConversationState;
use super::types::{
    CodingIntent, CodingOptions, CommandPlan, IntentScope, IntentType, PlannedStep, ResolvedTarget,
    SupportedLanguage,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct IntentClarification {
    message: String,
}

fn explicit_target_count(input: &str) -> usize {
    input
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | '。' | '.' | '、' | ':' | ';' | '"' | '\'' | '「' | '」' | '(' | ')'
                )
            })
        })
        .filter(|token| {
            !token.is_empty()
                && (token.contains('/')
                    || token.ends_with(".rs")
                    || token.ends_with(".toml")
                    || token.ends_with(".json")
                    || token.ends_with(".md"))
        })
        .count()
}

fn references_feature_addition(lower: &str) -> bool {
    [
        "機能追加",
        "feature",
        "追加する",
        "追加して",
        "implement",
        "add ",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn references_refactor(lower: &str) -> bool {
    [
        "refactor",
        "設計変更",
        "trait",
        "責務",
        "coupling",
        "循環",
        "cycle",
        "抽象化",
        "分離",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn references_bug_fix(lower: &str) -> bool {
    [
        "fix",
        "bug",
        "修正",
        "直して",
        "直す",
        "failure",
        "broken",
        "panic",
        "error",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn resolve_intent_scope(merged: &ResolvedTarget) -> Option<IntentScope> {
    if merged.scope.as_deref() == Some("project") || merged.path == std::path::PathBuf::from(".") {
        Some(IntentScope::Workspace)
    } else if let Some(node) = &merged.node {
        Some(IntentScope::Node(node.clone()))
    } else if merged.path.as_os_str().is_empty() {
        None
    } else {
        Some(IntentScope::Target(merged.path.clone()))
    }
}

fn formalize_coding_intent(
    input: &str,
    merged: &ResolvedTarget,
    conversation: &ConversationState,
) -> Result<Option<CodingIntent>, IntentClarification> {
    let lower = input.to_lowercase();
    if explicit_target_count(input) > 1 {
        return Err(IntentClarification {
            message: "Which file should be modified?".to_string(),
        });
    }

    let has_action = references_bug_fix(&lower)
        || references_feature_addition(&lower)
        || references_refactor(&lower)
        || wants_design_delta_reasoning(&lower)
        || wants_alternative_mutation_search(&lower)
        || wants_design_tradeoff_explanation(&lower);
    if !has_action {
        return Ok(None);
    }

    if references_feature_addition(&lower) {
        let target = if merged.path != std::path::PathBuf::from(".") {
            merged.path.clone()
        } else if let Some(target) = conversation.last_target.clone() {
            target
        } else {
            return Err(IntentClarification {
                message: "Which file should be modified?".to_string(),
            });
        };
        return Ok(Some(CodingIntent::AddFeature {
            target,
            spec: input.to_string(),
        }));
    }

    if references_refactor(&lower)
        || wants_design_delta_reasoning(&lower)
        || wants_alternative_mutation_search(&lower)
        || wants_design_tradeoff_explanation(&lower)
    {
        let scope = resolve_intent_scope(merged).ok_or_else(|| IntentClarification {
            message: "Which scope should be refactored?".to_string(),
        })?;
        return Ok(Some(CodingIntent::Refactor { scope }));
    }

    if references_bug_fix(&lower) {
        if merged.path == std::path::PathBuf::from(".")
            && let Some(node) = &merged.node
        {
            return Ok(Some(CodingIntent::Refactor {
                scope: IntentScope::Node(node.clone()),
            }));
        }
        let target = if merged.path != std::path::PathBuf::from(".") {
            merged.path.clone()
        } else if let Some(target) = conversation.last_target.clone() {
            target
        } else {
            return Err(IntentClarification {
                message: "Which file should be modified?".to_string(),
            });
        };
        return Ok(Some(CodingIntent::FixBug {
            target,
            description: input.to_string(),
        }));
    }

    Ok(None)
}

fn validate_formalized_intent(intent: &CodingIntent) -> Result<(), IntentClarification> {
    match intent {
        CodingIntent::FixBug { target, .. } | CodingIntent::AddFeature { target, .. } => {
            if !target.as_os_str().is_empty() && target != &std::path::PathBuf::from(".") {
                Ok(())
            } else {
                Err(IntentClarification {
                    message: format!("Target does not exist: {}", target.display()),
                })
            }
        }
        CodingIntent::Refactor { scope } => match scope {
            IntentScope::Workspace => Ok(()),
            IntentScope::Target(path) => {
                if !path.as_os_str().is_empty() {
                    Ok(())
                } else {
                    Err(IntentClarification {
                        message: format!("Target does not exist: {}", path.display()),
                    })
                }
            }
            IntentScope::Node(node) => {
                if node.trim().is_empty() {
                    Err(IntentClarification {
                        message: "Which scope should be refactored?".to_string(),
                    })
                } else {
                    Ok(())
                }
            }
        },
    }
}

fn validated_coding_intent(
    input: &str,
    merged: &ResolvedTarget,
    conversation: &ConversationState,
) -> Option<Option<CodingIntent>> {
    let intent = match formalize_coding_intent(input, merged, conversation) {
        Ok(intent) => intent,
        Err(_) => return None,
    };
    if let Some(intent_ref) = &intent
        && validate_formalized_intent(intent_ref).is_err()
    {
        return None;
    }
    Some(intent)
}

/// Returns true only when the input is the exact canonical apply command.
/// Nothing else qualifies as "apply intent" — even partial matches must not fire.
fn is_explicit_apply_intent(input: &str) -> bool {
    input.trim() == "coding --apply"
}

fn is_rollback_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "rollback"
}

/// Returns true when the input contains an explicit new coding or refactor request.
/// Such requests must NEVER be preempted by `ApplyPreviousCodingStep`.
///
/// - mutation verbs ("refactor", "fix", "修正", …) indicate new work
/// - "coding" without "--apply" means a new coding check, not apply-previous
fn is_new_coding_or_refactor_request(lower: &str) -> bool {
    wants_mutation_verb(lower) || (wants_coding(lower) && !lower.contains("--apply"))
}

/// Resolve the path to store in PlannedStep::Coding.
///
/// When workspace root is "." (e.g. anchored by "このプロジェクト") but the input
/// contains an explicit file target, the file path is returned so the executor can
/// remap `Coding(file.rs, _)` → `coding . --target file.rs`.
/// For all other cases the resolved workspace path is returned unchanged.
fn coding_path(input: &str, merged: &ResolvedTarget) -> std::path::PathBuf {
    if merged.path == std::path::PathBuf::from(".") {
        if let Some(file_path) = extract_explicit_path(input) {
            return file_path;
        }
    }
    merged.path.clone()
}

/// Resolve the workspace root for non-coding steps (Validate, Analyze, Run, …).
///
/// These commands require a directory argument. When the REPL context carries a file
/// target (e.g. `last_target = apps/cli/src/coding.rs` from a prior coding apply),
/// using it directly causes "path is not a directory". This helper normalises any
/// file path to `"."` so those steps always receive a valid workspace root.
fn workspace_path(merged: &ResolvedTarget) -> std::path::PathBuf {
    let ext = merged.path.extension().and_then(|e| e.to_str());
    if matches!(
        ext,
        Some("rs" | "toml" | "md" | "json" | "lock" | "yaml" | "yml")
    ) {
        std::path::PathBuf::from(".")
    } else {
        merged.path.clone()
    }
}

fn wants_design_delta_reasoning(lower: &str) -> bool {
    [
        "最小の設計変更",
        "設計変更",
        "設計合理性",
        "crate 境界",
        "trait 分離",
        "責務を崩さず",
        "architecture-native",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn wants_alternative_mutation_search(lower: &str) -> bool {
    [
        "複数の設計変更案",
        "比較して",
        "trait分離案",
        "adapter案",
        "crate split案",
        "最適案",
        "最も保守性の高い案",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn wants_design_tradeoff_explanation(lower: &str) -> bool {
    [
        "なぜこの設計案を採択",
        "棄却した案との違い",
        "設計トレードオフ",
        "tradeoff",
        "採択した理由",
        "説明して",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn wants_issue_driven_refactor(lower: &str) -> bool {
    [
        "問題",
        "issue",
        "coupling",
        "依存",
        "edge",
        "cycle",
        "循環",
        "修正して",
        "直して",
        "refactor",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn heuristic_issue_spec(
    input: &str,
    merged: &ResolvedTarget,
    conversation: &ConversationState,
) -> String {
    let target = if merged.path == std::path::PathBuf::from(".") {
        conversation
            .last_target
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        merged.path.clone()
    };
    format!(
        "Analyze dependency edges for {:?}. Generate one concrete issue from the strongest coupling or cycle, map it to an action (ExtractInterface, RemoveDependency, or SplitModule), and prepare a refactor plan for: {}",
        target.display().to_string(),
        input
    )
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
    coding_intent: Option<CodingIntent>,
) -> CommandPlan {
    let coding_target =
        if is_meta_planner_intent(intent) || intent.primary_intent == IntentType::RulesLearn {
            planner_edit_target()
        } else {
            coding_path(input, merged)
        };

    CommandPlan {
        intent: coding_intent,
        steps: vec![
            PlannedStep::Analyze(workspace_path(merged)),
            PlannedStep::Coding(coding_target, coding_options_for_request(input)),
            PlannedStep::Validate(workspace_path(merged)),
        ],
    }
}

fn synthesize_steps_from_intent(
    intent: &PlannerIntent,
    input: &str,
    merged: &ResolvedTarget,
    coding_intent: Option<CodingIntent>,
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
        return Some(generate_multi_step_plan(
            input,
            intent,
            merged,
            coding_intent,
        ));
    }

    let mut steps = Vec::new();
    match intent.primary_intent {
        IntentType::RulesList => steps.push(PlannedStep::Rules),
        IntentType::StructureView => steps.push(PlannedStep::StructureView(workspace_path(merged))),
        IntentType::Validate => steps.push(PlannedStep::Validate(workspace_path(merged))),
        IntentType::AnalyzeArchitecture => steps.push(PlannedStep::Analyze(workspace_path(merged))),
        IntentType::CodingEdit => steps.push(PlannedStep::Coding(
            coding_path(input, merged),
            coding_options_for_request(input),
        )),
        IntentType::DesignDeltaReasoning => {
            steps.push(PlannedStep::DesignDeltaReasoning(input.to_string()))
        }
        IntentType::ExplainDesignTradeoff => {
            steps.push(PlannedStep::ExplainDesignTradeoff(input.to_string()))
        }
        IntentType::RulesLearn | IntentType::MetaPlannerEdit => {
            return Some(generate_multi_step_plan(
                input,
                intent,
                merged,
                coding_intent,
            ));
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
                    steps.insert(0, PlannedStep::Analyze(workspace_path(merged)));
                }
            }
            IntentType::CodingEdit => {
                if !steps
                    .iter()
                    .any(|step| matches!(step, PlannedStep::Coding(_, _)))
                {
                    steps.push(PlannedStep::Coding(
                        coding_path(input, merged),
                        coding_options_for_request(input),
                    ));
                }
            }
            IntentType::Validate => {
                if !steps
                    .iter()
                    .any(|step| matches!(step, PlannedStep::Validate(_)))
                {
                    steps.push(PlannedStep::Validate(workspace_path(merged)));
                }
            }
            IntentType::MetaPlannerEdit | IntentType::RulesLearn => {
                return Some(generate_multi_step_plan(
                    input,
                    intent,
                    merged,
                    coding_intent,
                ));
            }
            IntentType::DesignDeltaReasoning => {
                steps.push(PlannedStep::DesignDeltaReasoning(input.to_string()));
            }
            IntentType::ExplainDesignTradeoff => {
                steps.push(PlannedStep::ExplainDesignTradeoff(input.to_string()));
            }
            _ => {}
        }
    }

    if steps.is_empty() {
        None
    } else {
        Some(CommandPlan {
            intent: coding_intent,
            steps,
        })
    }
}

pub fn plan_input(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> Option<CommandPlan> {
    let lower = input.to_lowercase();
    let merged = merge_target(input, session, conversation);

    if is_rollback_command(input) {
        return Some(CommandPlan {
            intent: None,
            steps: vec![PlannedStep::RollbackCurrentTransaction],
        });
    }

    // R1: "coding --apply" with a pending (not-yet-applied) transaction →
    // ApplyPreviousCodingStep, bypassing the generic planner (R2).
    //
    // Guard: `is_new_coding_or_refactor_request` ensures that an explicit new
    // coding/refactor input (e.g. "refactor ...", "coding check ...") is NEVER
    // preempted by apply_previous, even when a pending transaction exists.
    // Priority: explicit coding/refactor > apply_previous route.
    if is_explicit_apply_intent(input)
        && !is_new_coding_or_refactor_request(&lower)
        && conversation.has_pending_transaction()
    {
        return Some(CommandPlan {
            intent: None,
            steps: vec![PlannedStep::ApplyPreviousCodingStep],
        });
    }

    if lower.contains("whole project")
        || lower.contains("analyze project")
        || lower.contains("project .")
    {
        return None;
    }

    if wants_alternative_mutation_search(&lower) {
        return Some(CommandPlan {
            intent: Some(CodingIntent::Refactor {
                scope: resolve_intent_scope(&merged).unwrap_or(IntentScope::Workspace),
            }),
            steps: vec![PlannedStep::AlternativeMutationSearch(input.to_string())],
        });
    }
    if wants_design_tradeoff_explanation(&lower) {
        return Some(CommandPlan {
            intent: Some(CodingIntent::Refactor {
                scope: resolve_intent_scope(&merged).unwrap_or(IntentScope::Workspace),
            }),
            steps: vec![PlannedStep::ExplainDesignTradeoff(input.to_string())],
        });
    }
    if wants_design_delta_reasoning(&lower) {
        return Some(CommandPlan {
            intent: Some(CodingIntent::Refactor {
                scope: resolve_intent_scope(&merged).unwrap_or(IntentScope::Workspace),
            }),
            steps: vec![PlannedStep::DesignDeltaReasoning(input.to_string())],
        });
    }
    let mut steps = Vec::new();

    if (wants_analyze(&lower) || lower.contains("問題を") || lower.contains("issue"))
        && wants_issue_driven_refactor(&lower)
    {
        return Some(CommandPlan {
            intent: Some(CodingIntent::Refactor {
                scope: resolve_intent_scope(&merged).unwrap_or(IntentScope::Workspace),
            }),
            steps: vec![PlannedStep::DesignDeltaReasoning(heuristic_issue_spec(
                input,
                &merged,
                conversation,
            ))],
        });
    }

    if conversation.last_analysis_summary.is_some()
        && wants_issue_driven_refactor(&lower)
        && !wants_validate(&lower)
        && !wants_run(&lower)
        && !wants_rules(&lower)
        && !wants_memory(&lower)
    {
        return Some(CommandPlan {
            intent: Some(CodingIntent::Refactor {
                scope: resolve_intent_scope(&merged).unwrap_or(IntentScope::Workspace),
            }),
            steps: vec![PlannedStep::DesignDeltaReasoning(heuristic_issue_spec(
                input,
                &merged,
                conversation,
            ))],
        });
    }

    if lower.contains("undo") || lower.contains("戻して") {
        steps.push(PlannedStep::StructureUndo(workspace_path(&merged)));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }
    if lower.contains("redo") || lower.contains("やり直") {
        steps.push(PlannedStep::StructureRedo(workspace_path(&merged)));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }
    if (lower.contains("gui") || lower.contains("viewer") || lower.contains("差分"))
        && (lower.contains("diff") || lower.contains("差分"))
    {
        steps.push(PlannedStep::StructureDiff(
            workspace_path(&merged),
            merged.node.clone(),
        ));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    if wants_structure_view(&lower) {
        steps.push(PlannedStep::StructureView(workspace_path(&merged)));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    if wants_memory(&lower) {
        steps.push(PlannedStep::Memory(merged.path));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
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
        let coding_intent = validated_coding_intent(input, &merged, conversation)?;
        steps.push(PlannedStep::Coding(
            coding_path(input, &merged),
            coding_options_for_request(input),
        ));
        return Some(CommandPlan {
            intent: coding_intent,
            steps,
        });
    }

    let has_git_route = lower.contains("commit")
        || lower.contains("コミット")
        || mentions_pr(&lower)
        || lower.contains("pushして");
    if !has_git_route {
        let semantic_intent = infer_planner_intent(input, conversation);
        let coding_intent = validated_coding_intent(input, &merged, conversation);
        if should_use_semantic_frontend(&semantic_intent)
            && let Some(plan) = synthesize_steps_from_intent(
                &semantic_intent,
                input,
                &merged,
                coding_intent.flatten(),
            )
        {
            return Some(plan);
        }
    }

    if wants_rules(&lower) {
        steps.push(PlannedStep::Rules);
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    let analyze_first = wants_analyze(&lower)
        || (wants_coding(&lower)
            && ["unsafe", "循環", "cycle", "problem", "問題"]
                .iter()
                .any(|keyword| lower.contains(keyword)));

    if analyze_first {
        steps.push(PlannedStep::Analyze(workspace_path(&merged)));
    }

    if wants_coding(&lower) {
        steps.push(PlannedStep::Coding(
            coding_path(input, &merged),
            coding_options_for_request(input),
        ));
    }

    if wants_validate(&lower) {
        steps.push(PlannedStep::Validate(workspace_path(&merged)));
    }

    if lower.contains("commit") || lower.contains("コミット") {
        steps.push(PlannedStep::GitCommit(workspace_path(&merged)));
    }

    if mentions_pr(&lower) || lower.contains("pushして") {
        steps.push(PlannedStep::GitPR(workspace_path(&merged)));
    }

    if wants_run(&lower) {
        steps.push(PlannedStep::Run(workspace_path(&merged)));
    }

    if steps.is_empty() && wants_analyze(&lower) {
        steps.push(PlannedStep::Analyze(workspace_path(&merged)));
    }

    if steps.is_empty() {
        None
    } else {
        let intent = if steps.iter().any(|step| {
            matches!(
                step,
                PlannedStep::Coding(_, _)
                    | PlannedStep::DesignDeltaReasoning(_)
                    | PlannedStep::AlternativeMutationSearch(_)
                    | PlannedStep::ExplainDesignTradeoff(_)
            )
        }) {
            validated_coding_intent(input, &merged, conversation)?
        } else {
            None
        };
        Some(CommandPlan { intent, steps })
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
            | PlannedStep::Coding(path, _) => {
                conversation.clear_transaction_for_new_target(path);
                conversation.note_target(path.clone());
            }
            PlannedStep::DesignDeltaReasoning(_)
            | PlannedStep::AlternativeMutationSearch(_)
            | PlannedStep::ExplainDesignTradeoff(_) => {}
            PlannedStep::StructureDiff(path, node) => {
                conversation.clear_transaction_for_new_target(path);
                conversation.note_target(path.clone());
                if let Some(node) = node {
                    conversation.last_node = Some(node.clone());
                }
            }
            PlannedStep::Rules => {}
            // R2: ApplyPreviousCodingStep は last_target を更新しない。
            // 前回 Coding step が設定した last_target を continuity のためそのまま保持する (R3)。
            PlannedStep::ApplyPreviousCodingStep | PlannedStep::RollbackCurrentTransaction => {}
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
    fn analyze_issue_prompt_produces_actionable_refactor_plan() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();

        let plan = plan_input("設計上の問題を1つ挙げて", &session, &conversation).expect("plan");

        assert!(
            matches!(plan.steps.as_slice(), [PlannedStep::DesignDeltaReasoning(spec)] if spec.contains("Generate one concrete issue"))
        );
    }

    #[test]
    fn followup_fix_after_analysis_stays_on_refactor_path() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_analysis_summary: Some("High coupling between adapter and world".to_string()),
            last_target: Some(PathBuf::from("apps/cli/src/nl/planner_v2.rs")),
            ..ConversationState::default()
        };

        let plan = plan_input("その問題を修正して", &session, &conversation).expect("plan");

        assert!(
            matches!(plan.steps.as_slice(), [PlannedStep::DesignDeltaReasoning(spec)] if spec.contains("ExtractInterface"))
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
    fn design_delta_reasoning_route_is_selected_for_architecture_native_specs() {
        let session = AgentSession::new();
        let plan = plan_input(
            "責務を崩さず trait 分離して crate 境界を維持して機能追加して",
            &session,
            &ConversationState::default(),
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::DesignDeltaReasoning(
                "責務を崩さず trait 分離して crate 境界を維持して機能追加して".to_string()
            )]
        );
    }

    #[test]
    fn alternative_mutation_search_route_is_selected_for_comparison_specs() {
        let session = AgentSession::new();
        let plan = plan_input(
            "複数の設計変更案を比較して最適案で実装して trait分離案とadapter案を比較して",
            &session,
            &ConversationState::default(),
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::AlternativeMutationSearch(
                "複数の設計変更案を比較して最適案で実装して trait分離案とadapter案を比較して"
                    .to_string()
            )]
        );
    }

    #[test]
    fn design_tradeoff_explanation_route_is_selected_for_follow_up_questions() {
        let session = AgentSession::new();
        let plan = plan_input(
            "棄却した案との違いを説明して",
            &session,
            &ConversationState::default(),
        )
        .expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::ExplainDesignTradeoff(
                "棄却した案との違いを説明して".to_string()
            )]
        );
    }

    /// Regression: REPL NL coding request with explicit file target must not rebind
    /// the workspace root to the file path.
    ///
    /// Invariants:
    ///   - PlannedStep::Coding must carry the explicit file path (executor remaps to
    ///     `coding . --target <file>`).
    ///   - PlannedStep::Validate/Analyze must use "." (workspace root), never the file path.
    #[test]
    fn repl_single_file_coding_target_does_not_rebind_root() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        let plan = plan_input(
            "このプロジェクトを coding check して target は apps/cli/src/coding.rs",
            &session,
            &conversation,
        )
        .expect("plan must be produced");

        // The Coding step must preserve the explicit file path so the executor can
        // remap it to `coding . --target apps/cli/src/coding.rs`.
        let coding_step = plan
            .steps
            .iter()
            .find(|s| matches!(s, PlannedStep::Coding(_, _)));
        let Some(PlannedStep::Coding(coding_target, _)) = coding_step else {
            panic!("expected at least one Coding step in plan: {plan:?}");
        };
        assert_eq!(
            coding_target,
            &std::path::PathBuf::from("apps/cli/src/coding.rs"),
            "explicit file target must be preserved in Coding step"
        );

        // Non-coding steps must use workspace root "."; the file path must never appear
        // as a directory argument to Analyze or Validate.
        for step in &plan.steps {
            match step {
                PlannedStep::Analyze(path) | PlannedStep::Validate(path) => {
                    assert_eq!(
                        path,
                        &std::path::PathBuf::from("."),
                        "workspace root must remain '.' for {:?}, got: {}",
                        step,
                        path.display()
                    );
                }
                _ => {}
            }
        }
    }

    /// Regression: a pending coding transaction must NEVER preempt an explicit new
    /// coding or refactor request.
    ///
    /// The ApplyPreviousCodingStep route must only fire for the exact "coding --apply"
    /// command. Any new coding/refactor work must produce a Coding step instead.
    #[test]
    fn no_dual_authority_pending_transaction_path() {
        use crate::service::dto::{IRActiveTransaction, IRState};

        let session = AgentSession::new();
        let conversation = ConversationState {
            ir_state: IRState {
                active_transaction: Some(IRActiveTransaction {
                    transaction_id: "tx:apps/cli/src/repl.rs".to_string(),
                    canonical_target: PathBuf::from("apps/cli/src/repl.rs"),
                    pending: true,
                    applied: false,
                    validated: false,
                    rollback_available: false,
                    latest_diff_ref: None,
                    latest_build_ok: None,
                }),
                next_allowed_actions: Vec::new(),
                ..IRState::default()
            },
            last_target: Some(PathBuf::from("apps/cli/src/repl.rs")),
            ..ConversationState::default()
        };

        let new_requests = [
            "refactor apps/cli/src/repl.rs",
            "このプロジェクトを coding check して",
            "coding check apps/cli",
            "fix the planner routing",
        ];

        for input in &new_requests {
            let plan = plan_input(input, &session, &conversation)
                .unwrap_or_else(|| panic!("plan must be produced for: {input}"));

            let hijacked = plan
                .steps
                .iter()
                .any(|s| matches!(s, PlannedStep::ApplyPreviousCodingStep));
            assert!(
                !hijacked,
                "pending transaction must NOT hijack new request '{input}': got {:?}",
                plan.steps
            );

            let has_coding = plan
                .steps
                .iter()
                .any(|s| matches!(s, PlannedStep::Coding(_, _)));
            assert!(
                has_coding,
                "new request '{input}' must produce a Coding step, got: {:?}",
                plan.steps
            );
        }
    }

    #[test]
    fn rollback_route_test() {
        let session = AgentSession::new();
        let plan =
            plan_input("rollback", &session, &ConversationState::default()).expect("rollback plan");
        assert_eq!(plan.steps, vec![PlannedStep::RollbackCurrentTransaction]);
    }

    #[test]
    fn rollback_route_skips_analyze() {
        let session = AgentSession::new();
        let plan =
            plan_input("rollback", &session, &ConversationState::default()).expect("rollback plan");
        assert!(
            !plan
                .steps
                .iter()
                .any(|step| matches!(step, PlannedStep::Analyze(_))),
            "rollback must never fall back to analyze: {:?}",
            plan.steps
        );
    }

    #[test]
    fn planner_precedence_tree_is_ir_only() {
        let source = include_str!("planner_v2.rs");
        let alias = ["/coding", " rollback"].concat();
        let phrase = ["undo previous", " transaction"].concat();
        let token = ["\"un", "do\" |"].concat();
        assert!(!source.contains(&alias));
        assert!(!source.contains(&phrase));
        assert!(!source.contains(&token));
    }

    /// After a single-file coding apply, `last_target` holds a file path.
    /// The validate route must NOT inherit that file path — it must use workspace root ".".
    #[test]
    fn repl_validate_after_single_file_coding_apply_uses_workspace_root() {
        let session = AgentSession::new();
        // Simulate post-coding state: last_target = single file
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };

        let plan = plan_input("validate", &session, &conversation).expect("plan must succeed");

        let validate_path = plan.steps.iter().find_map(|s| {
            if let PlannedStep::Validate(p) = s {
                Some(p.clone())
            } else {
                None
            }
        });
        assert!(validate_path.is_some(), "plan must contain a Validate step");
        assert_eq!(
            validate_path.unwrap(),
            PathBuf::from("."),
            "Validate step must use workspace root '.', not the last file path"
        );
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

    #[test]
    fn ambiguous_input_rejected() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        assert!(plan_input("fix bug", &session, &conversation).is_none());
    }

    #[test]
    fn intent_parsed_deterministically() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/nl/planner_v2.rs")),
            ..ConversationState::default()
        };
        let lhs = plan_input("fix bug in planner", &session, &conversation).expect("lhs");
        let rhs = plan_input("fix bug in planner", &session, &conversation).expect("rhs");
        assert_eq!(lhs.intent, rhs.intent);
    }

    #[test]
    fn missing_target_fails() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        let merged = merge_target("新機能を追加して", &session, &conversation);
        assert!(formalize_coding_intent("新機能を追加して", &merged, &conversation).is_err());
    }
}
