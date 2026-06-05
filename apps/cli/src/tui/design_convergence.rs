#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesignConvergenceState {
    pub raw_intent: Option<String>,
    pub intent: Option<ExtractedIntent>,
    pub questions: Vec<String>,
    pub answers: Vec<String>,
    pub decisions: Vec<String>,
    pub generated_spec: Option<String>,
    pub convergence_score: f32,
    pub log: Vec<ConvergenceLogEntry>,
    pub timeline: Vec<ConvergenceEntry>,
}

impl Eq for DesignConvergenceState {}

impl DesignConvergenceState {
    pub fn timeline_lines(&self) -> Vec<String> {
        let entries = if self.timeline.is_empty() {
            self.legacy_timeline_entries()
        } else {
            self.timeline.clone()
        };
        if entries.is_empty() {
            return vec![
                "User Intent".to_string(),
                "  (waiting for natural language intent)".to_string(),
                String::new(),
                "Convergence".to_string(),
                format!("  {}%", self.convergence_percent()),
            ];
        }

        let mut lines = Vec::new();
        for entry in entries {
            lines.push(entry.title().to_string());
            for line in entry.message().lines() {
                lines.push(format!("  {line}"));
            }
            lines.push(String::new());
        }
        lines.push("Convergence".to_string());
        lines.push(format!("  {}%", self.convergence_percent()));
        lines
    }

    pub fn input_lines(&self, current_input: &[String]) -> Vec<String> {
        let mut lines = Vec::new();
        if current_input.iter().any(|line| !line.trim().is_empty()) {
            lines.extend(current_input.iter().cloned());
        } else {
            lines.push(
                "(type natural language request, answer, constraint, or verification condition)"
                    .to_string(),
            );
        }
        lines
    }

    pub fn workspace_lines(&self, current_input: &[String]) -> Vec<String> {
        let mut lines = self.timeline_lines();
        lines.push(String::new());
        lines.push("Natural Language Input".to_string());
        lines.extend(self.input_lines(current_input));
        lines
    }

    pub fn convergence_percent(&self) -> u8 {
        self.convergence_score.clamp(0.0, 100.0).round() as u8
    }

    fn legacy_timeline_entries(&self) -> Vec<ConvergenceEntry> {
        let mut entries = Vec::new();
        if let Some(raw_intent) = &self.raw_intent {
            entries.push(ConvergenceEntry::UserIntent(raw_intent.clone()));
        }
        for question in &self.questions {
            entries.push(ConvergenceEntry::Question(question.clone()));
        }
        for answer in &self.answers {
            entries.push(ConvergenceEntry::Answer(answer.clone()));
        }
        for decision in &self.decisions {
            entries.push(ConvergenceEntry::Decision(decision.clone()));
        }
        if let Some(spec) = &self.generated_spec {
            entries.push(ConvergenceEntry::Specification(spec.clone()));
        }
        entries
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
pub enum ConvergenceEntry {
    UserIntent(String),
    Question(String),
    Answer(String),
    Decision(String),
    Specification(String),
    Analysis(String),
}

impl ConvergenceEntry {
    pub fn title(&self) -> &'static str {
        match self {
            Self::UserIntent(_) => "User Intent",
            Self::Question(_) => "Question",
            Self::Answer(_) => "Answer",
            Self::Decision(_) => "Decision",
            Self::Specification(_) => "Generated Specification",
            Self::Analysis(_) => "Analysis",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::UserIntent(message)
            | Self::Question(message)
            | Self::Answer(message)
            | Self::Decision(message)
            | Self::Specification(message)
            | Self::Analysis(message) => message,
        }
    }
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
        let answers = extract_answers(input);
        let decisions = record_decisions(input, &intent, previous);
        if !questions.is_empty() && answers_scope_question(input) {
            questions.retain(|question| !question.contains("RuntimeCore"));
        }
        let generated_spec = generate_spec(&intent, &decisions);
        let convergence_score = calculate_convergence_score(input, &questions, &decisions);

        let mut log = previous.log.clone();
        let mut timeline = previous.timeline.clone();
        timeline.push(ConvergenceEntry::UserIntent(input.trim().to_string()));
        log.push(ConvergenceLogEntry {
            kind: "Intent",
            message: format!(
                "domain={} objective={} target={}",
                intent.domain, intent.objective, intent.target
            ),
        });
        for question in &questions {
            timeline.push(ConvergenceEntry::Question(question.clone()));
            log.push(ConvergenceLogEntry {
                kind: "Question",
                message: question.clone(),
            });
        }
        for answer in &answers {
            timeline.push(ConvergenceEntry::Answer(answer.clone()));
        }
        for decision in &decisions {
            if !previous.decisions.contains(decision) {
                timeline.push(ConvergenceEntry::Decision(decision.clone()));
                log.push(ConvergenceLogEntry {
                    kind: "Decision",
                    message: decision.clone(),
                });
            }
        }
        timeline.push(ConvergenceEntry::Specification(generated_spec.clone()));
        log.push(ConvergenceLogEntry {
            kind: "Spec",
            message: "generated design specification for Analyze".to_string(),
        });

        ConvergenceResult {
            state: DesignConvergenceState {
                raw_intent: Some(input.trim().to_string()),
                intent: Some(intent),
                questions,
                answers,
                decisions,
                generated_spec: Some(generated_spec.clone()),
                convergence_score,
                log,
                timeline,
            },
            generated_spec,
        }
    }
}

fn extract_answers(input: &str) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let mut answers = Vec::new();
    if contains_any(
        &lower,
        &["runtimecore", "runtime core", "tui", "ui", "両方"],
    ) {
        answers.push(input.trim().to_string());
    }
    answers
}

fn calculate_convergence_score(input: &str, questions: &[String], decisions: &[String]) -> f32 {
    let lower = input.to_ascii_lowercase();
    let intent_confirmed = !input.trim().is_empty();
    let scope_confirmed = contains_any(&lower, &["runtimecore", "runtime core", "tui", "ui"])
        || decisions
            .iter()
            .any(|decision| decision.contains("Set target"));
    let constraints_confirmed = contains_any(
        &lower,
        &[
            "preserve",
            "維持",
            "制約",
            "constraint",
            "governance",
            "replay",
            "audit",
        ],
    );
    let verification_confirmed = contains_any(
        &lower,
        &["verify", "verification", "検証", "test", "acceptance"],
    ) || !questions
        .iter()
        .any(|question| question.contains("検証条件"));

    [
        intent_confirmed,
        scope_confirmed,
        constraints_confirmed,
        verification_confirmed,
    ]
    .into_iter()
    .filter(|confirmed| *confirmed)
    .count() as f32
        * 25.0
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
