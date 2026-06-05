#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesignConvergenceState {
    pub raw_intent: Option<String>,
    pub intent: Option<ExtractedIntent>,
    pub questions: Vec<String>,
    pub decisions: Vec<String>,
    pub generated_spec: Option<String>,
    pub log: Vec<ConvergenceLogEntry>,
}

impl DesignConvergenceState {
    pub fn log_lines(&self) -> Vec<String> {
        if self.log.is_empty() {
            return vec!["- waiting for intent".to_string()];
        }
        self.log
            .iter()
            .map(|entry| format!("- {}: {}", entry.kind, entry.message))
            .collect()
    }

    pub fn workspace_lines(&self, current_input: &[String]) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push("Intent Input".to_string());
        if current_input.iter().any(|line| !line.trim().is_empty()) {
            lines.extend(current_input.iter().map(|line| format!("  {line}")));
        } else if let Some(raw_intent) = &self.raw_intent {
            lines.extend(raw_intent.lines().map(|line| format!("  {line}")));
        } else {
            lines.push("  (type natural language intent)".to_string());
        }
        lines.push(String::new());
        lines.push("Design Convergence".to_string());
        if let Some(intent) = &self.intent {
            lines.push(format!("  domain={}", intent.domain));
            lines.push(format!("  objective={}", intent.objective));
            lines.push(format!("  target={}", intent.target));
        } else {
            lines.push("  (not started)".to_string());
        }
        if !self.questions.is_empty() {
            lines.push(String::new());
            lines.push("Questions".to_string());
            lines.extend(
                self.questions
                    .iter()
                    .map(|question| format!("  - {question}")),
            );
        }
        if !self.decisions.is_empty() {
            lines.push(String::new());
            lines.push("Design Decisions".to_string());
            lines.extend(
                self.decisions
                    .iter()
                    .map(|decision| format!("  - {decision}")),
            );
        }
        lines.push(String::new());
        lines.push("Generated Design Specification".to_string());
        if let Some(spec) = &self.generated_spec {
            lines.extend(spec.lines().map(|line| format!("  {line}")));
        } else {
            lines.push("  (not generated)".to_string());
        }
        lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedIntent {
    pub domain: String,
    pub objective: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvergenceLogEntry {
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvergenceResult {
    pub state: DesignConvergenceState,
    pub generated_spec: String,
}

pub struct DesignConvergenceEngine;

impl DesignConvergenceEngine {
    pub fn converge(input: &str, previous: &DesignConvergenceState) -> ConvergenceResult {
        let intent = extract_intent(input);
        let mut questions = detect_gaps(input, &intent);
        let decisions = record_decisions(input, &intent, previous);
        if !questions.is_empty() && answers_scope_question(input) {
            questions.retain(|question| !question.contains("RuntimeCore"));
        }
        let generated_spec = generate_spec(&intent, &decisions);

        let mut log = previous.log.clone();
        log.push(ConvergenceLogEntry {
            kind: "Intent",
            message: format!(
                "domain={} objective={} target={}",
                intent.domain, intent.objective, intent.target
            ),
        });
        for question in &questions {
            log.push(ConvergenceLogEntry {
                kind: "Question",
                message: question.clone(),
            });
        }
        for decision in &decisions {
            if !previous.decisions.contains(decision) {
                log.push(ConvergenceLogEntry {
                    kind: "Decision",
                    message: decision.clone(),
                });
            }
        }
        log.push(ConvergenceLogEntry {
            kind: "Spec",
            message: "generated design specification for Analyze".to_string(),
        });

        ConvergenceResult {
            state: DesignConvergenceState {
                raw_intent: Some(input.trim().to_string()),
                intent: Some(intent),
                questions,
                decisions,
                generated_spec: Some(generated_spec.clone()),
                log,
            },
            generated_spec,
        }
    }
}

fn extract_intent(input: &str) -> ExtractedIntent {
    let lower = input.to_ascii_lowercase();
    let objective = if contains_any(&lower, &["self", "セルフ", "自己", "改修"]) {
        "self_modification"
    } else if contains_any(&lower, &["tui", "ui", "workspace", "画面", "入力"]) {
        "interface_convergence"
    } else {
        "design_convergence"
    };
    let domain = if contains_any(&lower, &["runtime", "実行", "self", "セルフ"]) {
        "runtime"
    } else if contains_any(&lower, &["tui", "ui", "workspace", "画面"]) {
        "ui"
    } else {
        "product"
    };
    let target = if contains_any(&lower, &["cli", "tui", "dbm_cli"]) {
        "design_cli"
    } else if contains_any(&lower, &["runtimecore", "runtime core"]) {
        "runtime_core"
    } else {
        "dbm"
    };
    ExtractedIntent {
        domain: domain.to_string(),
        objective: objective.to_string(),
        target: target.to_string(),
    }
}

fn detect_gaps(input: &str, intent: &ExtractedIntent) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let mut gaps = Vec::new();
    if intent.objective == "self_modification"
        && !contains_any(&lower, &["runtimecore", "runtime core", "tui", "ui"])
    {
        gaps.push("セルフ改修対象は RuntimeCore のみですか？ TUI も対象に含みますか？".to_string());
    }
    if !contains_any(
        &lower,
        &[
            "preserve",
            "維持",
            "制約",
            "constraint",
            "governance",
            "replay",
        ],
    ) {
        gaps.push("維持すべき制約は governance / replay / audit のどれですか？".to_string());
    }
    if !contains_any(
        &lower,
        &["verify", "verification", "検証", "test", "acceptance"],
    ) {
        gaps.push("収束後の検証条件は何ですか？".to_string());
    }
    gaps
}

fn record_decisions(
    input: &str,
    intent: &ExtractedIntent,
    previous: &DesignConvergenceState,
) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let mut decisions = previous.decisions.clone();
    push_unique(
        &mut decisions,
        format!("Set objective to {}", intent.objective),
    );
    push_unique(&mut decisions, format!("Set target to {}", intent.target));
    if contains_any(&lower, &["tui", "ui", "workspace", "画面"]) {
        push_unique(
            &mut decisions,
            "Include TUI workspace in convergence scope".to_string(),
        );
    }
    if contains_any(&lower, &["runtimecore", "runtime core", "runtime"]) {
        push_unique(
            &mut decisions,
            "Include RuntimeCore in convergence scope".to_string(),
        );
    }
    if contains_any(&lower, &["governance", "replay", "audit", "維持"]) {
        push_unique(
            &mut decisions,
            "Preserve governance, replay, and audit traceability".to_string(),
        );
    }
    decisions
}

fn generate_spec(intent: &ExtractedIntent, decisions: &[String]) -> String {
    let include_tui = decisions
        .iter()
        .any(|decision| decision.to_ascii_lowercase().contains("tui"));
    let include_runtime = decisions
        .iter()
        .any(|decision| decision.to_ascii_lowercase().contains("runtimecore"))
        || intent.domain == "runtime";

    let mut scope = Vec::new();
    if include_runtime {
        scope.push("runtime_core");
    }
    if include_tui {
        scope.push("tui");
    }
    if scope.is_empty() {
        scope.push(intent.target.as_str());
    }

    let architecture = scope
        .iter()
        .map(|component| {
            format!(
                "  {}:\n    responsibilities:\n      - Intent Persistence\n      - Design Persistence\n      - Decision Traceability",
                component
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "system_name: DBM\n\
goals:\n\
  - {objective}\n\
  - design_convergence\n\
constraints:\n\
  - preserve_governance\n\
  - preserve_replay\n\
  - preserve_audit_traceability\n\
architecture:\n\
{architecture}\n\
rules:\n\
  - ApplyGate required\n\
  - Analyze must use generated Design Specification\n\
  - Design decisions must be persisted in convergence history",
        objective = intent.objective,
        architecture = architecture
    )
}

fn answers_scope_question(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    contains_any(
        &lower,
        &["runtimecore", "runtime core", "tui", "ui", "両方", "含"],
    )
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.contains(&value) {
        items.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_modification_intent_generates_spec_and_questions() {
        let result = DesignConvergenceEngine::converge(
            "DBM_CLIでセルフ改修できるようにしたい",
            &Default::default(),
        );

        assert_eq!(
            result
                .state
                .intent
                .as_ref()
                .map(|intent| intent.objective.as_str()),
            Some("self_modification")
        );
        assert!(
            result
                .state
                .questions
                .iter()
                .any(|question| question.contains("RuntimeCore"))
        );
        assert!(result.generated_spec.contains("system_name: DBM"));
        assert!(
            result
                .generated_spec
                .contains("Analyze must use generated Design Specification")
        );
    }
}
