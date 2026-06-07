use crate::runtime::runtime_events::RuntimeEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionSeverity {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanProjectionEvent {
    pub title: String,
    pub summary: String,
    pub severity: ProjectionSeverity,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NarrativeSnapshot {
    pub headline: String,
    pub details: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanSemanticEvent {
    RuntimeRunning,
    RuntimeWaiting,
    RuntimeCompleted,
    RuntimeFailed {
        reason: Option<String>,
    },
    MutationPlanCreated,
    ValidationPassed,
    ValidationFailed {
        reason: Option<String>,
    },
    PreviewGenerated,
    ApplyCompleted,
    RollbackCompleted,
    AnalyzeCompleted {
        project_name: Option<String>,
        primary_areas: Vec<String>,
    },
    SecurityChecked {
        issue_count: usize,
    },
}

pub struct HumanOutputProjection;

impl HumanOutputProjection {
    pub fn project_runtime_event(event: &RuntimeEvent) -> HumanProjectionEvent {
        let semantic_event = match event {
            RuntimeEvent::BootstrapStarted | RuntimeEvent::LoopStarted => {
                HumanSemanticEvent::RuntimeRunning
            }
            RuntimeEvent::InputAccepted(_) => HumanSemanticEvent::RuntimeRunning,
            RuntimeEvent::ShutdownRequested => HumanSemanticEvent::RuntimeCompleted,
            RuntimeEvent::RuntimeFailed(reason) => HumanSemanticEvent::RuntimeFailed {
                reason: Some(reason.clone()),
            },
            RuntimeEvent::Validation(validation)
                if validation.message.to_ascii_lowercase().contains("fail") =>
            {
                HumanSemanticEvent::ValidationFailed {
                    reason: Some(validation.message.clone()),
                }
            }
            RuntimeEvent::Validation(_) => HumanSemanticEvent::ValidationPassed,
            RuntimeEvent::StateTransition(transition) => {
                semantic_event_from_state_label(&transition.to)
            }
            RuntimeEvent::Route(_)
            | RuntimeEvent::Git(_)
            | RuntimeEvent::Replay(_)
            | RuntimeEvent::Debug(_)
            | RuntimeEvent::Render(_) => HumanSemanticEvent::RuntimeRunning,
        };
        Self::project(&semantic_event)
    }

    pub fn project(event: &HumanSemanticEvent) -> HumanProjectionEvent {
        match event {
            HumanSemanticEvent::RuntimeRunning => projection(
                "処理中",
                "処理を実行しています...",
                ProjectionSeverity::Info,
                &[],
            ),
            HumanSemanticEvent::RuntimeWaiting => projection(
                "入力待機",
                "次の入力を待機しています。",
                ProjectionSeverity::Info,
                &[],
            ),
            HumanSemanticEvent::RuntimeCompleted => projection(
                "完了",
                "処理が完了しました。",
                ProjectionSeverity::Success,
                &[],
            ),
            HumanSemanticEvent::RuntimeFailed { reason } => HumanProjectionEvent {
                title: "問題を検出".to_string(),
                summary: reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .map(|reason| format!("処理中に問題が発生しました。{reason}"))
                    .unwrap_or_else(|| "処理中に問題が発生しました。".to_string()),
                severity: ProjectionSeverity::Error,
                suggested_actions: vec!["詳細を確認".to_string(), "再実行または復元".to_string()],
            },
            HumanSemanticEvent::MutationPlanCreated => projection(
                "変更計画",
                "変更計画を作成しました。",
                ProjectionSeverity::Success,
                &["変更内容を確認"],
            ),
            HumanSemanticEvent::ValidationPassed => projection(
                "整合性確認",
                "設計との整合性が確認されました。",
                ProjectionSeverity::Success,
                &[],
            ),
            HumanSemanticEvent::ValidationFailed { reason } => HumanProjectionEvent {
                title: "整合性の問題".to_string(),
                summary: reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .map(|reason| format!("変更内容に設計上の問題が見つかりました。{reason}"))
                    .unwrap_or_else(|| "変更内容に設計上の問題が見つかりました。".to_string()),
                severity: ProjectionSeverity::Error,
                suggested_actions: vec![
                    "問題の詳細を確認".to_string(),
                    "変更計画を修正".to_string(),
                ],
            },
            HumanSemanticEvent::PreviewGenerated => projection(
                "事前確認",
                "変更結果を事前確認できます。",
                ProjectionSeverity::Success,
                &["Previewを確認", "Applyを実行"],
            ),
            HumanSemanticEvent::ApplyCompleted => projection(
                "変更完了",
                "変更を適用しました。",
                ProjectionSeverity::Success,
                &["変更結果を確認"],
            ),
            HumanSemanticEvent::RollbackCompleted => projection(
                "復元完了",
                "変更前の状態へ復元しました。",
                ProjectionSeverity::Success,
                &["復元結果を確認"],
            ),
            HumanSemanticEvent::AnalyzeCompleted {
                project_name,
                primary_areas,
            } => {
                let mut summary = "解析が完了しました。".to_string();
                if let Some(project_name) = project_name
                    .as_deref()
                    .filter(|name| !name.trim().is_empty())
                {
                    summary.push_str(&format!("\n{project_name} の構造情報を更新しました。"));
                } else {
                    summary.push_str("\n構造情報を更新しました。");
                }
                if !primary_areas.is_empty() {
                    summary.push_str(&format!(
                        "\n主要構成は {} に集中しています。",
                        primary_areas.join(" と ")
                    ));
                }
                HumanProjectionEvent {
                    title: "解析完了".to_string(),
                    summary,
                    severity: ProjectionSeverity::Success,
                    suggested_actions: vec!["解析結果の詳細を確認".to_string()],
                }
            }
            HumanSemanticEvent::SecurityChecked { issue_count: 0 } => projection(
                "安全性確認",
                "安全性の確認が完了しました。確認事項はありません。",
                ProjectionSeverity::Success,
                &[],
            ),
            HumanSemanticEvent::SecurityChecked { issue_count } => HumanProjectionEvent {
                title: "安全性確認".to_string(),
                summary: format!(
                    "安全性の確認を実施しました。\n{issue_count}件の確認事項があります。"
                ),
                severity: ProjectionSeverity::Warning,
                suggested_actions: vec!["詳細を確認".to_string()],
            },
        }
    }
}

pub struct ActionSuggestionEngine;

impl ActionSuggestionEngine {
    pub fn suggestions(events: &[HumanProjectionEvent]) -> Vec<String> {
        let mut suggestions = Vec::new();
        for event in events.iter().rev() {
            for action in &event.suggested_actions {
                if !suggestions.contains(action) {
                    suggestions.push(action.clone());
                }
            }
        }
        suggestions
    }
}

pub struct NarrativeEngine;

impl NarrativeEngine {
    pub fn summarize(events: &[HumanProjectionEvent]) -> NarrativeSnapshot {
        let headline = events
            .iter()
            .rev()
            .find(|event| !is_runtime_status_title(&event.title))
            .or_else(|| events.last())
            .map(|event| event.title.clone())
            .unwrap_or_else(|| "状況サマリー".to_string());
        let mut details = Vec::new();
        for event in events {
            for line in event.summary.lines() {
                let line = line.trim();
                if !line.is_empty() && !details.iter().any(|detail| detail == line) {
                    details.push(line.to_string());
                }
            }
        }
        NarrativeSnapshot {
            headline,
            details,
            next_actions: ActionSuggestionEngine::suggestions(events),
        }
    }
}

fn semantic_event_from_state_label(label: &str) -> HumanSemanticEvent {
    match label.trim().to_ascii_uppercase().as_str() {
        "PLAN" | "PLAN_CREATED" => HumanSemanticEvent::MutationPlanCreated,
        "VALIDATION_PASSED" | "VALIDATED" => HumanSemanticEvent::ValidationPassed,
        "VALIDATION_FAILED" | "FAILED" => HumanSemanticEvent::ValidationFailed { reason: None },
        "PREVIEW_READY" | "PREVIEW_GENERATED" | "READY_TO_APPLY" => {
            HumanSemanticEvent::PreviewGenerated
        }
        "APPLIED" | "APPLY_COMPLETED" | "GIT" => HumanSemanticEvent::ApplyCompleted,
        "ROLLBACK_COMPLETED" | "ROLLED_BACK" => HumanSemanticEvent::RollbackCompleted,
        "IDLE" | "WAITING" => HumanSemanticEvent::RuntimeWaiting,
        "COMPLETED" => HumanSemanticEvent::RuntimeCompleted,
        _ => HumanSemanticEvent::RuntimeRunning,
    }
}

fn is_runtime_status_title(title: &str) -> bool {
    matches!(title, "処理中" | "入力待機" | "完了" | "問題を検出")
}

fn projection(
    title: &str,
    summary: &str,
    severity: ProjectionSeverity,
    suggested_actions: &[&str],
) -> HumanProjectionEvent {
    HumanProjectionEvent {
        title: title.to_string(),
        summary: summary.to_string(),
        severity,
        suggested_actions: suggested_actions
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_mutation_events_have_human_projections() {
        let cases = [
            (
                HumanSemanticEvent::MutationPlanCreated,
                "変更計画を作成しました。",
            ),
            (
                HumanSemanticEvent::ValidationPassed,
                "設計との整合性が確認されました。",
            ),
            (
                HumanSemanticEvent::ValidationFailed { reason: None },
                "変更内容に設計上の問題が見つかりました。",
            ),
            (
                HumanSemanticEvent::PreviewGenerated,
                "変更結果を事前確認できます。",
            ),
            (HumanSemanticEvent::ApplyCompleted, "変更を適用しました。"),
            (
                HumanSemanticEvent::RollbackCompleted,
                "変更前の状態へ復元しました。",
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(HumanOutputProjection::project(&event).summary, expected);
        }
    }

    #[test]
    fn preview_suggests_preview_before_apply() {
        let event = HumanOutputProjection::project(&HumanSemanticEvent::PreviewGenerated);

        assert_eq!(
            event.suggested_actions,
            vec!["Previewを確認", "Applyを実行"]
        );
    }

    #[test]
    fn narrative_is_deterministic_and_deduplicated() {
        let events = [
            HumanOutputProjection::project(&HumanSemanticEvent::MutationPlanCreated),
            HumanOutputProjection::project(&HumanSemanticEvent::ValidationPassed),
            HumanOutputProjection::project(&HumanSemanticEvent::PreviewGenerated),
        ];

        let first = NarrativeEngine::summarize(&events);
        let second = NarrativeEngine::summarize(&events);

        assert_eq!(first, second);
        assert_eq!(first.headline, "事前確認");
        assert_eq!(
            first.details,
            vec![
                "変更計画を作成しました。",
                "設計との整合性が確認されました。",
                "変更結果を事前確認できます。",
            ]
        );
    }

    #[test]
    fn security_projection_reports_issue_count_without_raw_label() {
        let event =
            HumanOutputProjection::project(&HumanSemanticEvent::SecurityChecked { issue_count: 3 });

        assert!(event.summary.contains("3件の確認事項があります。"));
        assert!(!event.summary.contains("Security Violations"));
    }

    #[test]
    fn runtime_event_is_projected_without_internal_state_label() {
        let event =
            RuntimeEvent::StateTransition(crate::runtime::runtime_events::StateTransitionEvent {
                from: "VALIDATE".to_string(),
                to: "PREVIEW_READY".to_string(),
            });

        let projected = HumanOutputProjection::project_runtime_event(&event);

        assert_eq!(projected.summary, "変更結果を事前確認できます。");
        assert!(!projected.summary.contains("PREVIEW_READY"));
    }
}
