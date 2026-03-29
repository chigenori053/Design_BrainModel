use agent_core::IntentProfile;
use serde::Serialize;

use crate::command::{CommandError, Output, SubCommandHandler};
use crate::dbm::analyzer::{self, Complexity, ProjectAnalysisResult};
use crate::design_output::{DesignExtractor, MarkdownDesignExtractor};
use crate::renderer::{render_unified_analyze_detailed, render_unified_analyze_summary};
use crate::report::{Language, ReportGenerator, TemplateReportGenerator};
use crate::session::AgentSession;

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("project", execute_unified)
}

pub fn execute_unified(
    args: &[String],
    _session: &mut AgentSession,
) -> Result<Output, CommandError> {
    let options = parse_options(args).map_err(CommandError::ExecutionError)?;
    let result = analyze_with_options(&options).map_err(CommandError::ExecutionError)?;
    Ok(Output::text(dispatch_output(&result, &options)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AnalyzeMode {
    Summary,
    Detailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyzeOptions {
    pub path: String,
    pub mode: AnalyzeMode,
    pub report: bool,
    pub design: bool,
    pub language: Language,
    pub intent: Option<IntentProfile>,
    pub json: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnifiedAnalyzeResult {
    pub path: String,
    pub mode: AnalyzeMode,
    pub intent: String,
    pub modules: usize,
    pub cycles: usize,
    pub coupling: String,
    pub top_issue: String,
    pub violations: Vec<String>,
    pub metrics: DecisionMetrics,
    pub decision: DecisionContext,
    pub analysis: ProjectAnalysisResult,
    pub report: Option<String>,
    pub design: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DecisionMetrics {
    pub si: f64,
    pub cs: f64,
    pub rp: f64,
    pub er: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DecisionContext {
    pub action: String,
    pub expected_impact: String,
    pub score: f64,
    pub confidence: f64,
    pub risk: String,
    pub intent_match: String,
}

pub fn parse_options(args: &[String]) -> Result<AnalyzeOptions, String> {
    let mut path = ".".to_string();
    let mut mode = AnalyzeMode::Summary;
    let mut report = false;
    let mut design = false;
    let mut language = Language::English;
    let mut intent = None;
    let mut json = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--summary" => mode = AnalyzeMode::Summary,
            "--detailed" => mode = AnalyzeMode::Detailed,
            "--report" => report = true,
            "--design" => design = true,
            "--json" => json = true,
            "--lang" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--lang requires ja or en".to_string())?;
                language = Language::parse(value)
                    .ok_or_else(|| format!("unsupported language: {value}"))?;
            }
            "--intent" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--intent requires a profile name".to_string())?;
                intent = Some(
                    parse_intent(value).ok_or_else(|| format!("unsupported intent: {value}"))?,
                );
            }
            value if value.starts_with("--lang=") => {
                let lang_value = value.trim_start_matches("--lang=");
                language = Language::parse(lang_value)
                    .ok_or_else(|| format!("unsupported language: {lang_value}"))?;
            }
            value if value.starts_with("--intent=") => {
                let intent_value = value.trim_start_matches("--intent=");
                intent = Some(
                    parse_intent(intent_value)
                        .ok_or_else(|| format!("unsupported intent: {intent_value}"))?,
                );
            }
            value if value.starts_with("--") => {
                return Err(format!("unsupported option: {value}"));
            }
            value => path = value.to_string(),
        }
    }

    Ok(AnalyzeOptions {
        path,
        mode,
        report,
        design,
        language,
        intent,
        json,
    })
}

pub fn analyze_with_options(options: &AnalyzeOptions) -> Result<UnifiedAnalyzeResult, String> {
    let analysis = analyzer::analyze_project(&options.path)?;
    let cycles = count_cycles(&analysis);
    let coupling_score = coupling_score(&analysis);
    let coupling = coupling_label(coupling_score).to_string();
    let violations = collect_violations(&analysis, cycles);
    let top_issue = violations
        .first()
        .cloned()
        .unwrap_or_else(|| "No significant issues detected".to_string());
    let metrics = compute_metrics(&analysis, cycles, coupling_score);
    let decision = derive_decision(&analysis, &violations, options.intent, &metrics);
    let report = options
        .report
        .then(|| TemplateReportGenerator::generate(&analysis, options.language).text);
    let design = options
        .design
        .then(|| MarkdownDesignExtractor::extract(&analysis).markdown);

    Ok(UnifiedAnalyzeResult {
        path: options.path.clone(),
        mode: options.mode,
        intent: format_intent(options.intent.unwrap_or(IntentProfile::Balanced)).to_string(),
        modules: analysis.modules.len(),
        cycles,
        coupling,
        top_issue,
        violations,
        metrics,
        decision,
        analysis,
        report,
        design,
    })
}

fn dispatch_output(result: &UnifiedAnalyzeResult, options: &AnalyzeOptions) -> String {
    if options.json {
        return serde_json::to_string_pretty(result)
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string());
    }

    let mut sections = vec![format!("Project: {}", result.path)];
    sections.push(match options.mode {
        AnalyzeMode::Summary => render_unified_analyze_summary(result),
        AnalyzeMode::Detailed => render_unified_analyze_detailed(result),
    });

    if let Some(report) = &result.report {
        sections.push(format!("=== Report ===\n\n{report}"));
    }
    if let Some(design) = &result.design {
        sections.push(format!("=== Design ===\n\n{design}"));
    }

    sections.join("\n\n")
}

fn parse_intent(value: &str) -> Option<IntentProfile> {
    match value.to_ascii_lowercase().as_str() {
        "balanced" => Some(IntentProfile::Balanced),
        "maintainability" => Some(IntentProfile::Maintainability),
        "performance" => Some(IntentProfile::Performance),
        "safety" | "lowrisk" | "low-risk" => Some(IntentProfile::LowRisk),
        "refactor" | "refactor-priority" | "refactor_priority" => Some(IntentProfile::Refactor),
        "minimal-change" | "minimal_change" | "minimalchange" => Some(IntentProfile::LowRisk),
        _ => None,
    }
}

fn count_cycles(result: &ProjectAnalysisResult) -> usize {
    let mut count = 0usize;
    for dep in &result.dependencies {
        if result
            .dependencies
            .iter()
            .any(|other| other.from == dep.to && other.to == dep.from && dep.from < dep.to)
        {
            count += 1;
        }
    }
    count
}

fn coupling_score(result: &ProjectAnalysisResult) -> f64 {
    let module_count = result.modules.len().max(1) as f64;
    (result.dependencies.len() as f64 / module_count).clamp(0.0, 3.0) / 3.0
}

fn coupling_label(score: f64) -> &'static str {
    if score >= 0.67 {
        "High"
    } else if score >= 0.34 {
        "Medium"
    } else {
        "Low"
    }
}

fn collect_violations(result: &ProjectAnalysisResult, cycles: usize) -> Vec<String> {
    let mut violations = Vec::new();
    if cycles > 0 {
        violations.push(format!("{} dependency cycle(s) detected", cycles));
    }
    for file in result.files.iter().filter(|file| !file.todos.is_empty()) {
        violations.push(format!("TODO/FIXME remains in {}", file.path));
    }
    let high_complexity = result
        .files
        .iter()
        .filter(|file| file.complexity == Complexity::High)
        .count();
    if high_complexity > 0 {
        violations.push(format!(
            "{} high-complexity file(s) require review",
            high_complexity
        ));
    }
    if violations.is_empty() {
        violations.push("No critical structural violations detected".to_string());
    }
    violations
}

fn compute_metrics(
    result: &ProjectAnalysisResult,
    cycles: usize,
    coupling_score: f64,
) -> DecisionMetrics {
    let total_files = result.summary.total_files.max(1) as f64;
    let todo_ratio = result
        .files
        .iter()
        .filter(|file| !file.todos.is_empty())
        .count() as f64
        / total_files;
    let complexity_score = match result.summary.avg_complexity {
        Complexity::Low => 0.2,
        Complexity::Medium => 0.5,
        Complexity::High => 0.85,
    };
    let cycle_pressure = (cycles as f64 / result.modules.len().max(1) as f64).clamp(0.0, 1.0);
    let cs = coupling_score;
    let er = (0.4 * complexity_score + 0.35 * cs + 0.25 * cycle_pressure).clamp(0.0, 1.0);
    let rp = (0.4 + 0.35 * todo_ratio + 0.25 * cycle_pressure).clamp(0.0, 1.0);
    let si = (1.0 - (0.45 * cs + 0.3 * todo_ratio + 0.25 * cycle_pressure)).clamp(0.0, 1.0);
    DecisionMetrics { si, cs, rp, er }
}

fn derive_decision(
    result: &ProjectAnalysisResult,
    violations: &[String],
    intent: Option<IntentProfile>,
    metrics: &DecisionMetrics,
) -> DecisionContext {
    let intent_value = intent.unwrap_or(IntentProfile::Balanced);
    let risk = if metrics.er >= 0.67 {
        "High"
    } else if metrics.er >= 0.34 {
        "Medium"
    } else {
        "Low"
    };
    let action = match intent_value {
        IntentProfile::Maintainability => largest_module_action(result),
        IntentProfile::Performance => highest_complexity_action(result),
        IntentProfile::LowRisk => safest_action(result, violations),
        IntentProfile::Refactor => largest_module_action(result),
        IntentProfile::Balanced => balanced_action(result, violations),
    };
    let confidence = (0.96 - metrics.er * 0.35).clamp(0.55, 0.96);
    let score = match intent_value {
        IntentProfile::Maintainability => {
            (0.45 * metrics.si + 0.35 * metrics.rp + 0.20 * confidence).clamp(0.0, 1.0)
        }
        IntentProfile::Performance => {
            (0.30 * metrics.si + 0.25 * metrics.rp + 0.45 * confidence).clamp(0.0, 1.0)
        }
        IntentProfile::LowRisk => {
            (0.55 * confidence + 0.30 * metrics.si + 0.15 * (1.0 - metrics.er)).clamp(0.0, 1.0)
        }
        IntentProfile::Refactor => {
            (0.30 * metrics.si + 0.50 * metrics.rp + 0.20 * confidence).clamp(0.0, 1.0)
        }
        IntentProfile::Balanced => {
            (0.35 * metrics.si + 0.30 * metrics.rp + 0.20 * confidence + 0.15 * (1.0 - metrics.er))
                .clamp(0.0, 1.0)
        }
    };

    DecisionContext {
        action,
        expected_impact: expected_impact(intent_value, result, violations).to_string(),
        score,
        confidence,
        risk: risk.to_string(),
        intent_match: format_intent(intent_value).to_string(),
    }
}

fn expected_impact(
    intent: IntentProfile,
    result: &ProjectAnalysisResult,
    violations: &[String],
) -> &'static str {
    match intent {
        IntentProfile::Maintainability | IntentProfile::Refactor => {
            "coupling down, ownership clearer"
        }
        IntentProfile::Performance => "complexity down, execution hotspots reduced",
        IntentProfile::LowRisk => {
            if violations.iter().any(|issue| issue.contains("cycle"))
                || !result.dependencies.is_empty()
            {
                "risk reduced by removing fragile dependencies"
            } else {
                "stability preserved with minimal changes"
            }
        }
        IntentProfile::Balanced => {
            if violations.iter().any(|issue| issue.contains("cycle")) {
                "coupling down"
            } else {
                "maintainability up with controlled change scope"
            }
        }
    }
}

fn largest_module_action(result: &ProjectAnalysisResult) -> String {
    result
        .modules
        .iter()
        .max_by_key(|module| (module.files.len(), module.name.clone()))
        .map(|module| format!("SplitModule({})", module.name))
        .unwrap_or_else(|| "PreserveCurrentStructure".to_string())
}

fn highest_complexity_action(result: &ProjectAnalysisResult) -> String {
    result
        .files
        .iter()
        .find(|file| file.complexity == Complexity::High)
        .map(|file| format!("OptimizeFile({})", file.path))
        .unwrap_or_else(|| largest_module_action(result))
}

fn safest_action(result: &ProjectAnalysisResult, violations: &[String]) -> String {
    if let Some(dep) = result.dependencies.first() {
        if violations.iter().any(|issue| issue.contains("cycle")) {
            return format!("RemoveDependency({} -> {})", dep.from, dep.to);
        }
    }
    "PreserveCurrentStructure".to_string()
}

fn balanced_action(result: &ProjectAnalysisResult, violations: &[String]) -> String {
    if violations.iter().any(|issue| issue.contains("cycle")) {
        if let Some(dep) = result.dependencies.first() {
            return format!("RemoveDependency({} -> {})", dep.from, dep.to);
        }
    }
    if result.files.iter().any(|file| !file.todos.is_empty()) {
        return "ResolveTodoMarkers".to_string();
    }
    largest_module_action(result)
}

fn format_intent(intent: IntentProfile) -> &'static str {
    match intent {
        IntentProfile::Balanced => "Balanced",
        IntentProfile::Maintainability => "Maintainability",
        IntentProfile::Performance => "Performance",
        IntentProfile::LowRisk => "Safety",
        IntentProfile::Refactor => "Refactor Priority",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn analyze_project_current_dir_contains_summary() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[".".to_string()], &mut session).unwrap();
        assert!(out.message.contains("DBM Analyze Report"), "got: {}", out.message);
        assert!(out.message.contains("Target: ."), "got: {}", out.message);
        assert!(out.message.contains("Top Issue:"), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_detailed_shows_structured_sections() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(
            &["src/".to_string(), "--detailed".to_string()],
            &mut session,
        )
        .unwrap();
        assert!(out.message.contains("[Modules]"), "got: {}", out.message);
        assert!(
            out.message.contains("[Top Candidates]"),
            "got: {}",
            out.message
        );
    }

    #[test]
    fn analyze_project_report_supports_japanese() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(
            &[
                ".".to_string(),
                "--report".to_string(),
                "--lang".to_string(),
                "ja".to_string(),
            ],
            &mut session,
        )
        .unwrap();
        assert!(
            out.message.contains("=== Report ==="),
            "got: {}",
            out.message
        );
        assert!(out.message.contains("【主な問題】"), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_design_outputs_markdown() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[".".to_string(), "--design".to_string()], &mut session).unwrap();
        assert!(
            out.message.contains("=== Design ==="),
            "got: {}",
            out.message
        );
        assert!(
            out.message.contains("# System Design"),
            "got: {}",
            out.message
        );
    }

    #[test]
    fn analyze_project_json_is_machine_readable() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[".".to_string(), "--json".to_string()], &mut session).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.message).expect("json");
        assert!(parsed.get("decision").is_some(), "got: {}", out.message);
        assert!(parsed.get("analysis").is_some(), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_json_takes_priority_over_report_and_design() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(
            &[
                ".".to_string(),
                "--json".to_string(),
                "--report".to_string(),
                "--design".to_string(),
            ],
            &mut session,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.message).expect("json");
        assert!(parsed.get("decision").is_some(), "got: {}", out.message);
        assert!(
            !out.message.contains("DBM Analyze Report"),
            "got: {}",
            out.message
        );
    }

    #[test]
    fn analyze_project_report_and_design_follow_decision_context() {
        let options = AnalyzeOptions {
            path: ".".to_string(),
            mode: AnalyzeMode::Detailed,
            report: true,
            design: true,
            language: Language::English,
            intent: Some(IntentProfile::Maintainability),
            json: false,
        };
        let result = analyze_with_options(&options).unwrap();
        let output = dispatch_output(&result, &options);
        let header = output.find("DBM Analyze Report").expect("header");
        let decision = output.find("Decision Context").expect("decision");
        let report = output.find("=== Report ===").expect("report");
        let design = output.find("=== Design ===").expect("design");
        assert!(header < decision);
        assert!(decision < report);
        assert!(report < design);
    }

    #[test]
    fn parse_options_accepts_intent_and_detailed() {
        let opts = parse_options(&[
            ".".to_string(),
            "--detailed".to_string(),
            "--intent".to_string(),
            "maintainability".to_string(),
        ])
        .unwrap();
        assert_eq!(opts.mode, AnalyzeMode::Detailed);
        assert_eq!(opts.intent, Some(IntentProfile::Maintainability));
    }
}
