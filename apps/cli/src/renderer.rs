mod decision;
mod detailed;
mod formatter;
mod header;
mod summary;

use std::io::{self, Write};

use design_search_engine::stable_v03::ReasoningTrace;
use runtime_core::intent_refiner::{CoreSlot, SlotMap};
use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{Clarification, Explanation, source_to_message};
use unified_design_ir::{
    AppliedFix as DesignAppliedFix, ChangeType as DesignChangeType, ConvergenceStatus,
    DiffResult as DesignDiffResult, Issue as DesignIssue, IssueReason as DesignIssueReason,
    IssueSummary as DesignIssueSummary, IssueType as DesignIssueType, Severity as DesignSeverity,
};

use crate::autonomous_execute::AutonomousExecuteReport;
use crate::commands::analyze::project::UnifiedAnalyzeResult;
use crate::commands::design::analyze::AnalysisResult as DbmAnalysisResult;
use crate::execution_foundation::ExecReport;
use crate::refactor::{RefactorApplyReport, RefactorPreviewReport};
use crate::service::dto::{
    AnalysisReport, CodeIssue, CodingReport, DesignReport, RefactorReport, RulesReport, RunReport,
    ValidationReport,
};
use integration_layer::{
    EvidenceType, Issue, IssueType, LayerType, NodeRole, PatchOperation, Pattern, PhaseType,
    RefactorAction, RefactorPlanAction, Severity,
};

pub fn render_unified_analyze_summary(result: &UnifiedAnalyzeResult) -> String {
    formatter::join_sections(&[
        header::render_header(result),
        summary::render_summary(result),
        decision::render_decision(result),
    ])
}

pub fn render_unified_analyze_detailed(result: &UnifiedAnalyzeResult) -> String {
    formatter::join_sections(&[
        header::render_header(result),
        detailed::render_detailed(result),
        decision::render_decision(result),
    ])
}

pub fn render_dbm_analyze(
    input: &str,
    summary: &DesignIssueSummary,
    issues: &[DesignIssue],
) -> String {
    let result = DbmAnalysisResult {
        input: input.to_string(),
        stage: infer_stage_from_issues(issues),
        converged: summary.critical == 0 && summary.high == 0,
        status: if summary.critical == 0 && summary.high == 0 {
            crate::commands::design::analyze::AnalysisStatus::Converged
        } else {
            crate::commands::design::analyze::AnalysisStatus::InProgress
        },
        summary: summary.clone(),
        issues: issues.to_vec(),
    };
    render_dbm_analysis_result(&result)
}

pub fn render_dbm_analysis_result(result: &DbmAnalysisResult) -> String {
    let mut sorted = result.issues.clone();
    sorted.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(issue_type_rank(&a.issue_type).cmp(&issue_type_rank(&b.issue_type)))
            .then(a.path.segments.join(".").cmp(&b.path.segments.join(".")))
    });

    let mut output = String::new();
    output.push_str("=== DBM Analyze ===\n\n");
    output.push_str(&format!("Input: {}\n", result.input));
    output.push_str(&format!("Stage: {:?}\n", result.stage));
    output.push_str(&format!("Status: {}\n", result.status.as_str()));
    output.push_str(&format_analysis_summary(result));
    output.push_str("\nIssues:\n");
    output.push_str("\n---\n");
    output.push_str("Details:\n");

    if sorted.is_empty() {
        output.push_str("No issues.\n");
        return output;
    }

    for issue in sorted {
        output.push_str(&format!(
            "\n[{}] {}\n  Path: {}\n  Reason: {}\n",
            format_design_severity(&issue.severity),
            format_issue_type(&issue.issue_type),
            issue.path.segments.join("."),
            format_issue_reason(&issue.reason)
        ));
    }

    output
}

fn infer_stage_from_issues(_issues: &[DesignIssue]) -> unified_design_ir::Stage {
    unified_design_ir::Stage::Context
}

pub fn render_dbm_diff(input: &str, diff: &DesignDiffResult) -> String {
    let mut changes = diff.changes.clone();
    changes.sort_by(|a, b| {
        change_type_rank(&a.change_type)
            .cmp(&change_type_rank(&b.change_type))
            .then(a.path.segments.join(".").cmp(&b.path.segments.join(".")))
    });

    let mut output = String::new();
    output.push_str("=== DBM Diff ===\n\n");
    output.push_str(&format!("Input: {input}\n\n"));
    output.push_str("Changes:\n");

    if changes.is_empty() {
        output.push_str("\nNo changes.\n");
        return output;
    }

    for change in changes {
        let path = change.path.segments.join(".");
        match change.change_type {
            DesignChangeType::Added => {
                output.push_str(&format!(
                    "\n[ADD]\n  Path: {path}\n  Value: {}\n",
                    display_json_opt(change.after.as_ref())
                ));
            }
            DesignChangeType::Removed => {
                output.push_str(&format!(
                    "\n[REMOVE]\n  Path: {path}\n  Value: {}\n",
                    display_json_opt(change.before.as_ref())
                ));
            }
            DesignChangeType::Modified => {
                output.push_str(&format!(
                    "\n[MODIFY]\n  Path: {path}\n  Before: {}\n  After:  {}\n",
                    display_json_opt(change.before.as_ref()),
                    display_json_opt(change.after.as_ref())
                ));
            }
            DesignChangeType::Moved => {
                output.push_str(&format!("\n[MOVE]\n  Path: {path}\n"));
            }
        }
    }

    output
}

pub fn render_dbm_step(
    input: &str,
    status: &str,
    applied_fix: Option<&DesignAppliedFix>,
    summary: &DesignIssueSummary,
) -> String {
    let mut output = String::new();
    output.push_str("=== DBM Step ===\n\n");
    output.push_str(&format!("Input: {input}\n\n"));
    output.push_str("Applied Fix:\n");
    match applied_fix {
        Some(fix) => {
            output.push_str(&format!("  Type: {}\n", format_fix_kind(fix)));
            output.push_str(&format!("  Path: {}\n", fix.path.segments.join(".")));
        }
        None => {
            output.push_str("  Type: None\n");
            output.push_str("  Path: -\n");
        }
    }
    output.push_str("\nResult:\n");
    output.push_str(&indent_summary(summary));
    output.push_str(&format!("\nStatus: {status}\n"));
    output
}

pub fn render_dbm_converge(
    input: &str,
    status: &ConvergenceStatus,
    iterations: u64,
    summary: &DesignIssueSummary,
    trace_lines: &[String],
) -> String {
    let mut output = String::new();
    output.push_str("=== DBM Converge ===\n\n");
    output.push_str(&format!("Input: {input}\n"));
    output.push_str(&format!("Status: {}\n\n", format_converge_status(status)));
    output.push_str(&format!("Iterations: {iterations}\n\n"));
    output.push_str("Final State:\n");
    output.push_str(&indent_summary(summary));
    output.push_str("\nTrace:\n");
    if trace_lines.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for line in trace_lines {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

fn format_summary(summary: &DesignIssueSummary) -> String {
    format!(
        "\nSummary:\n  Critical: {}\n  High: {}\n  Medium: {}\n  Low: {}\n",
        summary.critical, summary.high, summary.medium, summary.low
    )
}

fn format_analysis_summary(result: &DbmAnalysisResult) -> String {
    format_summary(&result.summary)
}

fn indent_summary(summary: &DesignIssueSummary) -> String {
    format!(
        "  Critical: {}\n  High: {}\n  Medium: {}\n  Low: {}\n",
        summary.critical, summary.high, summary.medium, summary.low
    )
}

fn format_design_severity(severity: &DesignSeverity) -> &'static str {
    match severity {
        DesignSeverity::Critical => "Critical",
        DesignSeverity::High => "High",
        DesignSeverity::Medium => "Medium",
        DesignSeverity::Low => "Low",
    }
}

fn format_issue_type(issue_type: &DesignIssueType) -> &'static str {
    match issue_type {
        DesignIssueType::Missing => "Missing",
        DesignIssueType::Conflict => "Conflict",
        DesignIssueType::Redundancy => "Redundancy",
        DesignIssueType::OverSpecification => "OverSpecification",
        DesignIssueType::UnderSpecification => "UnderSpecification",
    }
}

fn format_issue_reason(reason: &DesignIssueReason) -> &'static str {
    match reason {
        DesignIssueReason::MissingRequiredField => "MissingRequiredField",
        DesignIssueReason::ValueConflict => "ValueConflict",
        DesignIssueReason::DuplicateDefinition => "DuplicateDefinition",
        DesignIssueReason::ExcessiveComplexity => "ExcessiveComplexity",
        DesignIssueReason::InsufficientSpecification => "InsufficientSpecification",
    }
}

fn format_fix_kind(fix: &DesignAppliedFix) -> &'static str {
    match fix.action {
        unified_design_ir::FixAction::Add => "Missing",
        unified_design_ir::FixAction::Replace => "Conflict",
        unified_design_ir::FixAction::Remove => "Redundancy",
        unified_design_ir::FixAction::Normalize => "Normalize",
    }
}

fn format_converge_status(status: &ConvergenceStatus) -> &'static str {
    match status {
        ConvergenceStatus::Converged => "Converged",
        ConvergenceStatus::MaxIterationsReached => "MaxIterationsReached",
        ConvergenceStatus::Deadlock => "Deadlock",
        ConvergenceStatus::Failed => "Failed",
    }
}

fn display_json_opt(value: Option<&serde_json::Value>) -> String {
    value
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
        .unwrap_or_else(|| "null".to_string())
}

fn severity_rank(severity: &DesignSeverity) -> u8 {
    match severity {
        DesignSeverity::Critical => 0,
        DesignSeverity::High => 1,
        DesignSeverity::Medium => 2,
        DesignSeverity::Low => 3,
    }
}

fn issue_type_rank(issue_type: &DesignIssueType) -> u8 {
    match issue_type {
        DesignIssueType::Missing => 0,
        DesignIssueType::Conflict => 1,
        DesignIssueType::Redundancy => 2,
        DesignIssueType::OverSpecification => 3,
        DesignIssueType::UnderSpecification => 4,
    }
}

fn change_type_rank(change_type: &DesignChangeType) -> u8 {
    match change_type {
        DesignChangeType::Added => 0,
        DesignChangeType::Removed => 1,
        DesignChangeType::Modified => 2,
        DesignChangeType::Moved => 3,
    }
}

pub fn render_result<W: Write>(writer: &mut W, result: &RuntimeResult) -> io::Result<()> {
    writeln!(writer, "✔ Project generated")?;
    if let Some(explanation) = &result.explanation {
        writeln!(writer)?;
        render_explanation(writer, explanation)?;
    } else if let Some(trace) = &result.intent_trace {
        writeln!(writer)?;
        render_summary(writer, &trace.final_slots)?;
    }
    if let Some(trace) = &result.reasoning_trace {
        writeln!(writer)?;
        render_reasoning_trace(writer, trace)?;
    }
    writeln!(writer)?;
    writeln!(writer, "Files:")?;
    for file in &result.project_layout.files {
        writeln!(writer, " - {}", file.path)?;
    }
    writer.flush()
}

pub fn render_question<W: Write>(
    writer: &mut W,
    clarification: &Clarification,
    current_slots: Option<&SlotMap>,
) -> io::Result<()> {
    writeln!(writer, "?")?;
    writeln!(writer, "{}", clarification.message)?;
    if let Some(slots) = current_slots.filter(|slots| has_visible_core_slots(slots)) {
        writeln!(writer)?;
        writeln!(writer, "Current:")?;
        render_summary(writer, slots)?;
    }
    writer.flush()
}

pub fn render_summary<W: Write>(writer: &mut W, slots: &SlotMap) -> io::Result<()> {
    writeln!(writer, "---")?;
    for slot in [
        CoreSlot::Language,
        CoreSlot::Framework,
        CoreSlot::InterfaceType,
    ] {
        if let Some(value) = slots.core.get(&slot) {
            writeln!(writer, "{}: {}", slot_label(slot), value.value)?;
        }
    }
    Ok(())
}

pub fn render_reasoning_trace<W: Write>(writer: &mut W, trace: &ReasoningTrace) -> io::Result<()> {
    writeln!(writer, "[Reasoning]")?;
    writeln!(
        writer,
        "request_id={} total_nodes={} max_depth={} recall_hit_rate={:.2}",
        trace.request_id.0,
        trace.stats.total_nodes,
        trace.stats.max_depth,
        trace.stats.recall_hit_rate
    )?;
    writeln!(
        writer,
        "stats avg_branching={:.2} steps={}",
        trace.stats.avg_branching,
        trace.steps.len()
    )?;

    if !trace.steps.is_empty() {
        writeln!(writer, "Steps:")?;
        for step in &trace.steps {
            writeln!(
                writer,
                " - depth {} beam={} candidates={} pruned={} recall_hits={}",
                step.depth, step.beam_width, step.candidates, step.pruned, step.recall_hits
            )?;
        }
    }

    Ok(())
}

pub fn render_explanation<W: Write>(writer: &mut W, explanation: &Explanation) -> io::Result<()> {
    writeln!(writer, "[Intent]")?;
    for item in &explanation.intent {
        writeln!(
            writer,
            "{}: {} ({})",
            display_slot_name(&item.slot),
            item.value,
            source_to_message(&item.source)
        )?;
    }

    if !explanation.decisions.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "[Decisions]")?;
        for decision in &explanation.decisions {
            writeln!(writer, "- {}", decision.message)?;
        }
    }

    if let Some(reasoning) = &explanation.reasoning {
        writeln!(writer)?;
        writeln!(writer, "[Reasoning Proof]")?;
        writeln!(writer, "strategy: {:?}", reasoning.strategy_reason.strategy)?;
        writeln!(writer, "{}", reasoning.text)?;
    }

    Ok(())
}

fn has_visible_core_slots(slots: &SlotMap) -> bool {
    slots.core.contains_key(&CoreSlot::Language)
        || slots.core.contains_key(&CoreSlot::Framework)
        || slots.core.contains_key(&CoreSlot::InterfaceType)
}

fn slot_label(slot: CoreSlot) -> &'static str {
    match slot {
        CoreSlot::Language => "Language",
        CoreSlot::Framework => "Framework",
        CoreSlot::InterfaceType => "Interface",
    }
}

fn display_slot_name(slot: &str) -> &str {
    match slot {
        "Language" => "Language",
        "Framework" => "Framework",
        "InterfaceType" => "Interface",
        other => other,
    }
}

pub fn render_analysis_report<W: Write>(writer: &mut W, report: &AnalysisReport) -> io::Result<()> {
    writeln!(writer, "Analysis")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Files: {}", report.total_files)?;
    writeln!(writer, "Source files: {}", report.source_files)?;
    writeln!(writer, "Avg Complexity: {}", report.avg_complexity)?;
    if !report.languages.is_empty() {
        writeln!(writer, "Languages:")?;
        for (language, count) in &report.languages {
            writeln!(writer, " - {language}: {count}")?;
        }
    }
    if !report.architecture_hints.is_empty() {
        writeln!(writer, "Hints: {}", report.architecture_hints.join(", "))?;
    }
    if !report.modules.is_empty() {
        writeln!(writer, "Modules:")?;
        for module in &report.modules {
            writeln!(writer, " - {} ({} files)", module.name, module.file_count)?;
        }
    }
    if !report.dependencies.is_empty() {
        writeln!(writer, "Dependencies:")?;
        for dependency in &report.dependencies {
            writeln!(writer, " - {} -> {}", dependency.from, dependency.to)?;
        }
    }
    writeln!(writer, "Cycles Detected: {}", report.cycles.cycles.len())?;
    for (index, cycle) in report.cycles.cycles.iter().enumerate() {
        writeln!(writer)?;
        writeln!(writer, "Cycle #{}:", index + 1)?;
        let mut path = cycle.nodes.clone();
        if cycle.nodes.len() >= 2 {
            path.push(cycle.nodes[0].clone());
        }
        writeln!(writer, "  {}", path.join(" -> "))?;
    }
    if !report.layers.layers.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Layers:")?;
        for layer in &report.layers.layers {
            writeln!(writer, "Layer {}:", layer.level)?;
            for node in &layer.nodes {
                writeln!(writer, "  {}", node)?;
            }
        }
    }
    if !report.violations.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Violations:")?;
        for violation in &report.violations {
            let tag = if violation.from_level == violation.to_level {
                "SAME-LAYER CYCLE"
            } else if violation.from_level > violation.to_level {
                "DOWNWARD VIOLATION"
            } else {
                "LAYER SKIP"
            };
            let from_layer = if violation.from_layer_name.is_empty() {
                format!("Layer {}", violation.from_level)
            } else {
                format!(
                    "Layer {}: {}",
                    violation.from_level, violation.from_layer_name
                )
            };
            let to_layer = if violation.to_layer_name.is_empty() {
                format!("Layer {}", violation.to_level)
            } else {
                format!("Layer {}: {}", violation.to_level, violation.to_layer_name)
            };
            writeln!(
                writer,
                "  [{tag}] {} ({}) -> {} ({})",
                violation.from, from_layer, violation.to, to_layer
            )?;
        }
    }
    if !report.roles.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Roles:")?;
        for role in &report.roles {
            writeln!(
                writer,
                "- {}: {} ({:.2})",
                role.node_name,
                node_role_label(&role.role),
                f32::from(role.confidence_milli) / 1000.0
            )?;
        }
    }
    if !report.semantic_layers.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Semantic Layers:")?;
        for layer in &report.semantic_layers {
            writeln!(
                writer,
                "- Layer {}: {}",
                layer.level,
                layer_type_label(&layer.layer_type)
            )?;
        }
    }
    if !report.data_flow.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Data Flow:")?;
        for flow in &report.data_flow {
            writeln!(
                writer,
                "- {} -> {} ({:.2})",
                flow.from, flow.to, flow.weight
            )?;
        }
    }
    writeln!(writer)?;
    render_issue_group(writer, "Structural Issues", &report.issues, "Structural")?;
    render_issue_group(writer, "Semantic Issues", &report.issues, "Semantic")?;
    render_issue_group(writer, "Data Flow Issues", &report.issues, "Data Flow")?;
    render_code_issue_group(writer, &report.code_issues)?;
    writeln!(
        writer,
        "Summary: Critical: {} | High: {} | Medium: {}",
        report.summary.critical, report.summary.high, report.summary.medium
    )?;
    writeln!(writer)?;
    writeln!(writer, "Next Action:")?;
    writeln!(writer, "{}", report.next_action)?;
    writer.flush()
}

pub fn render_analysis_report_markdown(report: &AnalysisReport) -> String {
    let mut out = String::new();
    out.push_str("# Analyze Report\n\n");
    out.push_str(&format!("- Root: `{}`\n", report.root));
    out.push_str(&format!("- Files: `{}`\n", report.total_files));
    out.push_str(&format!("- Source files: `{}`\n", report.source_files));
    out.push_str(&format!("- Avg Complexity: `{}`\n", report.avg_complexity));
    if !report.languages.is_empty() {
        out.push_str("- Languages:\n");
        for (language, count) in &report.languages {
            out.push_str(&format!("  - {}: {}\n", language, count));
        }
    }
    out.push('\n');
    out.push_str("## Structural Diagnostics\n\n");
    write_markdown_issue_group(&mut out, "Structural Issues", &report.issues, "Structural");
    write_markdown_issue_group(&mut out, "Semantic Issues", &report.issues, "Semantic");
    write_markdown_issue_group(&mut out, "Data Flow Issues", &report.issues, "Data Flow");
    out.push_str("## Code Diagnostics\n\n");
    if report.code_issues.is_empty() {
        out.push_str("- No code-level structural issues detected.\n\n");
    } else {
        for issue in &report.code_issues {
            out.push_str(&format!(
                "### [{}] {}:{} {}\n\n",
                issue.severity.to_uppercase(),
                issue.file,
                issue.line,
                issue.title
            ));
            out.push_str(&format!("{}\n\n", issue.issue));
            out.push_str("```text\n");
            out.push_str(&issue.snippet);
            out.push_str("\n```\n\n");
        }
    }
    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "- Critical: {}\n- High: {}\n- Medium: {}\n\n",
        report.summary.critical, report.summary.high, report.summary.medium
    ));
    out.push_str("## Next Action\n\n");
    out.push_str(&format!("`{}`\n", report.next_action));
    out
}

fn render_issue_group<W: Write>(
    writer: &mut W,
    title: &str,
    issues: &[Issue],
    category: &str,
) -> io::Result<()> {
    let group = issues
        .iter()
        .filter(|issue| issue_category(issue) == category)
        .collect::<Vec<_>>();
    if group.is_empty() {
        writeln!(writer, "{title}")?;
        writeln!(writer, "  None")?;
        return Ok(());
    }
    writeln!(writer, "{title}")?;
    for issue in group {
        writeln!(
            writer,
            "  ({}) {}",
            severity_label(&issue.severity),
            issue.description
        )?;
        if let Some(hint) = issue_hint(issue) {
            writeln!(writer, "       {hint}")?;
        }
    }
    Ok(())
}

fn render_code_issue_group<W: Write>(writer: &mut W, issues: &[CodeIssue]) -> io::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "Code Diagnostics")?;
    if issues.is_empty() {
        writeln!(writer, " - none")?;
        writeln!(writer)?;
        return Ok(());
    }

    for issue in issues {
        writeln!(
            writer,
            " - [{}] {}:{} {}",
            issue.severity.to_uppercase(),
            issue.file,
            issue.line,
            issue.title
        )?;
        writeln!(writer, "   {}", issue.issue)?;
        for line in issue.snippet.lines() {
            writeln!(writer, "   {}", line)?;
        }
    }
    writeln!(writer)?;
    Ok(())
}

fn write_markdown_issue_group(out: &mut String, title: &str, issues: &[Issue], category: &str) {
    out.push_str(&format!("### {}\n\n", title));
    let mut matched = issues
        .iter()
        .filter(|issue| issue_category(issue) == category)
        .peekable();
    if matched.peek().is_none() {
        out.push_str("- None\n\n");
        return;
    }

    for issue in matched {
        out.push_str(&format!(
            "- [{}] {}\n",
            severity_label(&issue.severity),
            issue.description
        ));
    }
    out.push('\n');
}

fn issue_hint(issue: &Issue) -> Option<String> {
    match issue.kind {
        IssueType::OrphanNode => Some(
            "→ Consider: remove, merge into a lower-level module, or add intentional dependency"
                .to_string(),
        ),
        IssueType::GodObject => Some(
            "→ Consider splitting into sub-modules or introducing an abstraction layer".to_string(),
        ),
        IssueType::RoleMismatch => {
            let role_specific = issue
                .evidence
                .iter()
                .find(|ev| ev.kind == EvidenceType::Role)
                .and_then(|ev| {
                    let (from_role, to_role) = ev.value.split_once("->")?;

                    Some(format!(
                        "→ {} layer should not depend on {} layer. \
                         Suggested fix: extract shared interface to a lower-level (Core/Service) layer.",
                        from_role, to_role
                    ))
                });
            Some(role_specific.unwrap_or_else(|| {
                "→ Suggested fix: extract shared interface to a lower-level (Core/Service) layer."
                    .to_string()
            }))
        }
        IssueType::Hub => Some(
            "→ Consider extracting responsibilities or introducing an abstraction layer"
                .to_string(),
        ),
        IssueType::Cycle => Some(
            "→ Break the cycle by introducing a trait/interface or reversing a dependency"
                .to_string(),
        ),
        IssueType::LayerViolation | IssueType::DataFlowAnomaly => None,
    }
}

fn issue_category(issue: &Issue) -> &'static str {
    match issue.kind {
        IssueType::Cycle | IssueType::LayerViolation | IssueType::OrphanNode => "Structural",
        IssueType::GodObject | IssueType::RoleMismatch => "Semantic",
        IssueType::Hub | IssueType::DataFlowAnomaly => "Data Flow",
    }
}

fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
    }
}

pub fn render_design_report<W: Write>(writer: &mut W, report: &DesignReport) -> io::Result<()> {
    writeln!(writer, "Design")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Style: {}", report.inferred_style)?;
    writeln!(writer, "Components: {}", report.components.join(", "))?;
    writeln!(writer, "Design units: {}", report.design_units.join(", "))?;
    writeln!(writer)?;
    writeln!(writer, "Design Analysis:")?;
    writeln!(
        writer,
        "- Cycles: {} ({})",
        report.cycles.cycles.len(),
        if report.cycles.has_cycle {
            "INVALID"
        } else {
            "OK"
        }
    )?;
    writeln!(writer, "- Layers: {}", report.layers.layers.len())?;
    writeln!(writer, "- Violations: {}", report.violations.len())?;
    if !report.layers.layers.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Layer Model:")?;
        for layer in &report.layers.layers {
            writeln!(writer, "Layer {}: {}", layer.level, layer.nodes.join(", "))?;
        }
    }
    if !report.violations.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Violations:")?;
        for violation in &report.violations {
            writeln!(writer, "- {} -> {}", violation.from, violation.to)?;
        }
    }
    if !report.roles.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Roles:")?;
        for role in &report.roles {
            writeln!(
                writer,
                "- {}: {}",
                role.node_name,
                node_role_label(&role.role)
            )?;
        }
    }
    if !report.patterns.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Patterns:")?;
        for pattern in &report.patterns {
            writeln!(writer, "- {}", pattern_label(pattern))?;
        }
    }
    if !report.suggestions.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Insights:")?;
        for suggestion in &report.suggestions {
            writeln!(
                writer,
                "- {} [{}]: {}",
                suggestion.target,
                refactor_action_label(&suggestion.action),
                suggestion.reason
            )?;
        }
    }
    if !report.recommended_next_steps.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Recommendation:")?;
        for step in &report.recommended_next_steps {
            writeln!(writer, "- {step}")?;
        }
    }
    writer.flush()
}

pub fn render_validation_report<W: Write>(
    writer: &mut W,
    report: &ValidationReport,
) -> io::Result<()> {
    writeln!(writer, "Validation")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Valid: {}", report.valid)?;
    if !report.issues.is_empty() {
        writeln!(writer, "Issues:")?;
        for issue in &report.issues {
            writeln!(writer, " - {issue}")?;
        }
    }
    if !report.warnings.is_empty() {
        writeln!(writer, "Warnings:")?;
        for warning in &report.warnings {
            writeln!(writer, " - {warning}")?;
        }
    }
    if !report.patterns.is_empty() {
        writeln!(writer, "Patterns:")?;
        for pattern in &report.patterns {
            writeln!(writer, " - {}", pattern_label(pattern))?;
        }
    }
    if !report.layers.layers.is_empty() {
        writeln!(writer, "Layers:")?;
        for layer in &report.layers.layers {
            writeln!(
                writer,
                " - Layer {}: {}",
                layer.level,
                layer.nodes.join(", ")
            )?;
        }
    }
    writer.flush()
}

fn node_role_label(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Core => "Core",
        NodeRole::Service => "Service",
        NodeRole::Infrastructure => "Infrastructure",
        NodeRole::Interface => "Interface",
        NodeRole::Presentation => "Presentation",
        NodeRole::Utility => "Utility",
        NodeRole::Test => "Test",
        NodeRole::Unknown => "Unknown",
    }
}

fn layer_type_label(layer: &LayerType) -> &'static str {
    match layer {
        LayerType::CoreLayer => "CoreLayer",
        LayerType::DomainLayer => "DomainLayer",
        LayerType::ApplicationLayer => "ApplicationLayer",
        LayerType::InterfaceLayer => "InterfaceLayer",
        LayerType::InfrastructureLayer => "InfrastructureLayer",
    }
}

fn pattern_label(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Layered => "Layered Architecture".to_string(),
        Pattern::Cyclic { nodes } => format!("Cyclic Modular ({})", nodes.join(" <-> ")),
        Pattern::Hub { node } => format!("Hub Structure ({node})"),
        Pattern::GodObject { node } => format!("God Object ({node})"),
    }
}

fn refactor_action_label(action: &RefactorAction) -> &'static str {
    match action {
        RefactorAction::IntroduceAbstraction => "IntroduceAbstraction",
        RefactorAction::InvertDependency => "InvertDependency",
        RefactorAction::SplitModule => "SplitModule",
        RefactorAction::ExtractInterface => "ExtractInterface",
    }
}

fn refactor_plan_action_label(action: &RefactorPlanAction) -> String {
    match action {
        RefactorPlanAction::IntroduceInterface { between } => {
            format!(
                "Introduce Interface between {} and {}",
                between.0, between.1
            )
        }
        RefactorPlanAction::RemoveDependency { from, to } => {
            format!("Remove Dependency {} -> {}", from, to)
        }
        RefactorPlanAction::SplitModule { target } => format!("Split Module {}", target),
        RefactorPlanAction::MoveDependency { from, to, via } => match via {
            Some(via) => format!("Move Dependency {} -> {} via {}", from, to, via),
            None => format!("Move Dependency {} -> {}", from, to),
        },
        RefactorPlanAction::ExtractComponent { from } => {
            format!("Extract Component from {}", from)
        }
        RefactorPlanAction::IsolateNode { node } => format!("Isolate Node {}", node),
    }
}

pub fn render_run_report<W: Write>(writer: &mut W, report: &RunReport) -> io::Result<()> {
    writeln!(writer, "Run")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Status: {}", report.status)?;
    writeln!(writer, "Exit code: {}", report.exit_code)?;
    writeln!(writer, "Duration: {} ms", report.duration_ms)?;
    writeln!(
        writer,
        "Command: {} {}",
        report.command,
        report.args.join(" ")
    )?;
    writeln!(writer, "Sandbox:")?;
    writeln!(
        writer,
        " - timeout={}ms network={} fs_write={} deterministic={}",
        report.sandbox.max_execution_time_ms,
        report.sandbox.allow_network,
        report.sandbox.allow_fs_write,
        report.deterministic
    )?;
    writeln!(
        writer,
        " - stdout_size={} stderr_size={} timed_out={}",
        report.telemetry.stdout_size, report.telemetry.stderr_size, report.sandbox.timed_out
    )?;
    writeln!(
        writer,
        " - cpu_idle_recovery={}ms threads={}->{} children_after={} zombie_detected={}",
        report.telemetry.cpu_release.cpu_idle_recovery_ms,
        report.telemetry.cpu_release.baseline_threads,
        report.telemetry.cpu_release.final_threads,
        report.telemetry.cpu_release.child_processes_after,
        report.telemetry.cpu_release.zombie_detected
    )?;
    if !report.stdout.is_empty() {
        writeln!(writer, "Stdout:")?;
        writeln!(writer, "{}", report.stdout)?;
    }
    if !report.stderr.is_empty() {
        writeln!(writer, "Stderr:")?;
        writeln!(writer, "{}", report.stderr)?;
    }
    writer.flush()
}

pub fn render_exec_report<W: Write>(writer: &mut W, report: &ExecReport) -> io::Result<()> {
    writeln!(writer, "Exec")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Project: {}", report.project_type.as_str())?;
    writeln!(writer, "Action: {}", report.action.as_str())?;
    writeln!(writer, "Status: {}", report.status)?;
    writeln!(writer, "Exit code: {}", report.exit_code)?;
    writeln!(writer, "Duration: {} ms", report.duration_ms)?;
    if let Some(telemetry) = &report.telemetry {
        writeln!(
            writer,
            "CPU Release: idle={}ms threads={}->{} children_after={} zombie_detected={}",
            telemetry.cpu_release.cpu_idle_recovery_ms,
            telemetry.cpu_release.baseline_threads,
            telemetry.cpu_release.final_threads,
            telemetry.cpu_release.child_processes_after,
            telemetry.cpu_release.zombie_detected
        )?;
    }
    if let Some(command) = &report.command {
        writeln!(writer, "Command: {} {}", command, report.args.join(" "))?;
    }
    if !report.stdout.is_empty() {
        writeln!(writer, "Stdout:")?;
        writeln!(writer, "{}", report.stdout)?;
    }
    if !report.stderr.is_empty() {
        writeln!(writer, "Stderr:")?;
        writeln!(writer, "{}", report.stderr)?;
    }
    writer.flush()
}

pub fn render_autonomous_execute_report<W: Write>(
    writer: &mut W,
    report: &AutonomousExecuteReport,
) -> io::Result<()> {
    if report.completed {
        writeln!(writer, "✔ Completed")?;
    } else {
        writeln!(writer, "✖ Failed")?;
        if let Some(reason) = &report.reason {
            writeln!(writer, "Reason: {reason}")?;
        }
    }
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Project: {}", report.project_type.as_str())?;
    writeln!(writer, "Retries: {}", report.retry_count)?;
    if !report.tasks.is_empty() {
        writeln!(writer, "Tasks: {}", report.tasks.join(" -> "))?;
    }
    if !report.attempts.is_empty() {
        writeln!(writer)?;
        for attempt in &report.attempts {
            writeln!(writer, "Attempt {}:", attempt.attempt)?;
            if attempt.exec_report.success {
                writeln!(writer, "  Success")?;
            } else {
                writeln!(
                    writer,
                    "  Error: {}",
                    attempt
                        .debug
                        .as_ref()
                        .map(|debug| debug.primary.signature_hint.as_str())
                        .unwrap_or(&attempt.exec_report.error_type)
                )?;
                if let Some(debug) = &attempt.debug {
                    writeln!(writer, "  Signature: {}", debug.primary.signature)?;
                    writeln!(writer, "  Action: {}", debug.primary.action)?;
                    writeln!(writer, "  Confidence: {:.2}", debug.confidence)?;
                    writeln!(writer, "  Context Adjusted: {}", debug.context_adjusted)?;
                }
                if let Some(fix) = &attempt.fix {
                    writeln!(writer, "  Fix: {}", fix.content)?;
                }
            }
        }
    }
    if let Some(git) = &report.git {
        writeln!(writer)?;
        writeln!(writer, "Git:")?;
        if git.changed_files.is_empty() {
            writeln!(writer, "  Files: none")?;
        } else {
            writeln!(writer, "  Files: {}", git.changed_files.join(", "))?;
        }
        if !git.diff.trim().is_empty() {
            writeln!(writer, "  Diff:")?;
            for line in git.diff.lines() {
                writeln!(writer, "    {}", line)?;
            }
        }
        writeln!(
            writer,
            "  Diff Stats: +{} -{}",
            git.diff_stats.lines_added, git.diff_stats.lines_removed
        )?;
        writeln!(writer, "  Committed: {}", git.committed)?;
        writeln!(writer, "  Rolled Back: {}", git.rolled_back)?;
        if let Some(commit_id) = &git.commit_id {
            writeln!(writer, "  Commit: {}", commit_id)?;
        }
        if let Some(reason) = &git.reason {
            writeln!(writer, "  Reason: {}", reason)?;
        }
    }
    if let Some(remote) = &report.remote {
        writeln!(writer)?;
        writeln!(writer, "Remote:")?;
        if let Some(branch) = &remote.branch {
            writeln!(writer, "  Branch: {}", branch)?;
        }
        if let Some(base_branch) = &remote.base_branch {
            writeln!(writer, "  Base: {}", base_branch)?;
        }
        writeln!(
            writer,
            "  Push: {}",
            if remote.pushed { "success" } else { "skipped" }
        )?;
        writeln!(writer, "  PR Created: {}", remote.pr_created)?;
        if let Some(pr_url) = &remote.pr_url {
            writeln!(writer, "  PR: {}", pr_url)?;
        }
        if let Some(reason) = &remote.reason {
            writeln!(writer, "  Reason: {}", reason)?;
        }
    }
    writeln!(writer)?;
    writeln!(writer, "Metrics:")?;
    writeln!(writer, "  attempts: {}", report.metrics.attempts)?;
    writeln!(writer, "  success: {}", report.metrics.success)?;
    writeln!(
        writer,
        "  fix_chain: {}",
        report.metrics.fix_chain.join(" -> ")
    )?;
    writeln!(writer, "  commit: {}", report.metrics.commit)?;
    writeln!(writer, "  success_rate: {}", report.metrics.success_rate)?;
    writeln!(
        writer,
        "  avg_retry_count: {}",
        report.metrics.avg_retry_count
    )?;
    writer.flush()
}

pub fn render_refactor_report<W: Write>(writer: &mut W, report: &RefactorReport) -> io::Result<()> {
    writeln!(writer, "Refactor Plan")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer)?;
    for (index, phase) in report.plan.phases.iter().enumerate() {
        writeln!(
            writer,
            "Phase {}: {}",
            index + 1,
            phase_type_label(&phase.phase_type)
        )?;
        for action in &phase.actions {
            writeln!(writer, "- {}", refactor_plan_action_label(action))?;
        }
        writeln!(writer)?;
    }
    writeln!(writer, "Code Patches:")?;
    for (index, patch) in report.patches.iter().enumerate() {
        writeln!(writer)?;
        writeln!(writer, "[Patch {}]", index + 1)?;
        writeln!(
            writer,
            "Action: {}",
            refactor_plan_action_label(&patch.action)
        )?;
        for operation in &patch.operations {
            writeln!(writer, "- {}", patch_operation_label(operation))?;
        }
    }
    writeln!(writer)?;
    writeln!(writer, "Simulation:")?;
    writeln!(
        writer,
        "Cycles: {} -> {}",
        report.simulation.before.cycle_count, report.simulation.after.cycle_count
    )?;
    writeln!(
        writer,
        "Violations: {} -> {}",
        report.simulation.before.layer_violations, report.simulation.after.layer_violations
    )?;
    writeln!(
        writer,
        "Coupling: {:.2} -> {:.2}",
        f32::from(report.simulation.before.coupling_score_milli) / 1000.0,
        f32::from(report.simulation.after.coupling_score_milli) / 1000.0
    )?;
    writer.flush()
}

pub fn render_refactor_preview_report<W: Write>(
    writer: &mut W,
    report: &RefactorPreviewReport,
) -> io::Result<()> {
    writeln!(writer, "Refactor Preview")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Confidence: {:.2}", report.plan.confidence)?;
    writeln!(writer, "Validation: {}", report.validation.valid)?;
    for issue in &report.validation.issues {
        writeln!(writer, "- validation issue: {issue}")?;
    }
    writeln!(writer)?;
    writeln!(writer, "Preview:")?;
    writeln!(writer, "{}", report.preview.cli_text_preview)?;
    if let Some(edge) = &report.preview.removed_cycle_edge {
        writeln!(writer)?;
        writeln!(writer, "Removed edge: {} -> {}", edge.from, edge.to)?;
    }
    if !report.preview.moved_files.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Moved files:")?;
        for moved in &report.preview.moved_files {
            writeln!(writer, "- {moved}")?;
        }
    }
    writer.flush()
}

pub fn render_refactor_apply_report<W: Write>(
    writer: &mut W,
    report: &RefactorApplyReport,
) -> io::Result<()> {
    render_refactor_preview_report(
        writer,
        &RefactorPreviewReport {
            root: report.root.clone(),
            plan: report.plan.clone(),
            preview: report.preview.clone(),
            validation: report.validation.clone(),
        },
    )?;
    writeln!(writer)?;
    writeln!(writer, "Apply:")?;
    writeln!(writer, "  applied: {}", report.apply.applied)?;
    writeln!(writer, "  build_ok: {}", report.apply.build_ok)?;
    writeln!(writer, "  rolled_back: {}", report.apply.rolled_back)?;
    if !report.apply.changed_files.is_empty() {
        writeln!(writer, "  changed_files:")?;
        for path in &report.apply.changed_files {
            writeln!(writer, "  - {}", path.display())?;
        }
    }
    if let Some(commit_id) = &report.apply.commit_id {
        writeln!(writer, "  commit: {commit_id}")?;
    }
    writer.flush()
}

pub fn render_coding_report<W: Write>(writer: &mut W, report: &CodingReport) -> io::Result<()> {
    writeln!(writer, "Code Changes")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(
        writer,
        "Mode: {}",
        if report.dry_run { "dry-run" } else { "apply" }
    )?;
    writeln!(writer, "Status: {}", report.execution.status)?;
    writeln!(
        writer,
        "Build: {}",
        if report.execution.build_ok {
            "OK"
        } else {
            "FAILED"
        }
    )?;
    writeln!(writer, "Checked: {}", report.execution.checked)?;
    writeln!(writer, "Applied: {}", report.execution.applied)?;
    writeln!(writer, "Rollback: {}", report.execution.rolled_back)?;
    writeln!(writer, "Files changed: {}", report.execution.files_changed)?;
    if let Some(target) = &report.execution.canonical_target_path {
        writeln!(writer, "Canonical target: {}", target)?;
    }
    if report.execution.stale_artifact_detected {
        writeln!(writer, "Warning: stale snapshot artifact detected")?;
    }
    if report.execution.resolution_pipeline_hits > 0
        || report.execution.degraded_resolution_hits > 0
    {
        writeln!(
            writer,
            "Resolution pipeline hits: {} (degraded={})",
            report.execution.resolution_pipeline_hits, report.execution.degraded_resolution_hits
        )?;
    }
    if let Some(transactional) = &report.execution.transactional_apply {
        writeln!(writer, "Transactional build: {}", transactional.build_ok)?;
        writeln!(writer, "Sandbox cleanup: {}", transactional.cleanup_ok)?;
        writeln!(
            writer,
            "Sandbox path: {}",
            transactional.sandbox_path.display()
        )?;
    }
    // Canonical stream count — must equal changes.summary.total_changes and
    // execution.diff.diffs.len() for no-op targets (R5).
    writeln!(writer, "Patches (canonical): {}", report.patches.len())?;
    writeln!(
        writer,
        "Diffs: {} (breaking={})",
        report.execution.diff.diffs.len(),
        report.execution.diff.breaking_count
    )?;
    if !report.execution.diff.diffs.is_empty() {
        writeln!(writer, "Diff Analysis:")?;
        for diff in &report.execution.diff.diffs {
            writeln!(
                writer,
                "- {:?} {}{}",
                diff.kind,
                diff.target,
                if diff.breaking { " [breaking]" } else { "" }
            )?;
        }
    }
    if report.execution.committed {
        writeln!(writer, "Committed: true")?;
        if let Some(branch) = &report.execution.branch {
            writeln!(writer, "Branch: {}", branch)?;
        }
        if let Some(commit_id) = &report.execution.commit_id {
            writeln!(writer, "Commit: {}", commit_id)?;
        }
    }
    if let Some(git) = &report.execution.git_commit {
        writeln!(writer, "Git staged files: {}", git.staged_files.len())?;
        writeln!(writer, "Git dirty excluded: {}", git.dirty_excluded.len())?;
        writeln!(writer, "Git diff preview files: {}", git.diff_preview.len())?;
        if let Some(path) = &git.telemetry_path {
            writeln!(writer, "Git telemetry: {}", path.display())?;
        }
        if let Some(warning) = &git.warning {
            writeln!(writer, "Git warning: {warning}")?;
        }
    }
    if let Some(reason) = &report.execution.reason {
        writeln!(writer, "Reason: {reason}")?;
    }
    if !report.apply_resolutions.is_empty() {
        writeln!(writer, "Apply target resolution:")?;
        for resolution in &report.apply_resolutions {
            writeln!(
                writer,
                "- {} -> {} ({})",
                resolution.module,
                resolution.resolved_relative_path.display(),
                resolution.resolution_strategy
            )?;
            if let Some(sandbox_path) = &resolution.sandbox_path {
                writeln!(writer, "  sandbox: {}", sandbox_path.display())?;
            }
        }
    }
    for change in &report.changes.changes {
        writeln!(writer)?;
        writeln!(writer, "[{:?}] {}", change.change_type, change.file_path)?;
        for hunk in &change.hunks {
            for line in hunk.replacement.lines() {
                writeln!(writer, "+ {}", line)?;
            }
        }
    }
    writer.flush()
}

pub fn render_rules_report<W: Write>(writer: &mut W, report: &RulesReport) -> io::Result<()> {
    writeln!(writer, "Rules")?;
    writeln!(writer, "Language: {}", report.language)?;
    writeln!(writer, "Action: {}", report.action)?;
    writeln!(writer, "Active: {}", report.active.len())?;
    writeln!(writer, "Candidate: {}", report.candidate.len())?;
    writeln!(writer, "Validated: {}", report.validated.len())?;
    writeln!(writer, "Retired: {}", report.retired.len())?;
    if !report.active.is_empty() {
        writeln!(writer, "Active Rules:")?;
        for rule in &report.active {
            writeln!(
                writer,
                "- {} [{}] conf={:.2} usage={}",
                rule.id, rule.source, rule.confidence, rule.usage_count
            )?;
        }
    }
    if !report.candidate.is_empty() {
        writeln!(writer, "Candidate Rules:")?;
        for rule in &report.candidate {
            writeln!(
                writer,
                "- {} [{}] conf={:.2} usage={}",
                rule.id, rule.source, rule.confidence, rule.usage_count
            )?;
        }
    }
    if !report.validated.is_empty() {
        writeln!(writer, "Validated Rules:")?;
        for rule in &report.validated {
            writeln!(
                writer,
                "- {} [{}] score={:.2} checks={}",
                rule.id,
                rule.source,
                rule.validation_score,
                rule.passed_checks.join(", ")
            )?;
        }
    }
    if !report.retired.is_empty() {
        writeln!(writer, "Retired Rules:")?;
        for rule in &report.retired {
            writeln!(
                writer,
                "- {} [{}] conf={:.2} usage={}",
                rule.id, rule.source, rule.confidence, rule.usage_count
            )?;
        }
    }
    if let Some(message) = &report.message {
        writeln!(writer, "Message: {}", message)?;
    }
    writer.flush()
}

fn patch_operation_label(operation: &PatchOperation) -> String {
    match operation {
        PatchOperation::CreateInterface { name, .. } => {
            format!("Create interface {}", name)
        }
        PatchOperation::UpdateDependency { from, to, via } => match via {
            Some(via) => format!("Update dependency {} -> {} via {}", from, to, via),
            None => format!("Update dependency {} -> {}", from, to),
        },
        PatchOperation::SplitModule {
            module,
            new_modules,
        } => {
            format!("Split {} into {}", module, new_modules.join(", "))
        }
        PatchOperation::ExtractComponent { from, component } => {
            format!("Extract component {} from {}", component, from)
        }
    }
}

fn phase_type_label(phase_type: &PhaseType) -> &'static str {
    match phase_type {
        PhaseType::BreakCycle => "Break Cycle",
        PhaseType::FixLayering => "Fix Layering",
        PhaseType::RestructureModules => "Restructure Modules",
        PhaseType::OptimizeFlow => "Optimize Flow",
    }
}

#[cfg(test)]
mod tests {
    use integration_layer::{
        Evidence, EvidenceType, IssueScope, IssueType, LayerViolation, Severity,
    };

    use super::*;
    use crate::commands::analyze::project::{
        AnalyzeMode, DecisionContext, DecisionMetrics, UnifiedAnalyzeResult,
    };
    use crate::dbm::analyzer::{
        Complexity as ProjectComplexity, DependencyEdge, DependencyEdgeType, FileAnalysis,
        Language as ProjectLanguage, Module as ProjectModule, ProjectAnalysisResult,
        ProjectSummary,
    };
    use crate::service::dto::{AnalysisReport, AnalysisSummary};
    use integration_layer::{CycleReport, Issue, LayerModel};

    fn make_role_mismatch_issue(from_role: &str, to_role: &str) -> Issue {
        Issue {
            id: format!("role-mismatch-{from_role}-{to_role}"),
            kind: IssueType::RoleMismatch,
            severity: Severity::Medium,
            scope: IssueScope::Edge("mod_a".to_string(), "mod_b".to_string()),
            description: format!(
                "Role boundary mismatch: `mod_a` ({from_role}) depends on `mod_b` ({to_role})"
            ),
            evidence: vec![Evidence {
                kind: EvidenceType::Role,
                value: format!("{from_role}->{to_role}"),
            }],
        }
    }

    fn make_orphan_issue(node: &str) -> Issue {
        Issue {
            id: format!("orphan-{node}"),
            kind: IssueType::OrphanNode,
            severity: Severity::Low,
            scope: IssueScope::Node(node.to_string()),
            description: format!("Node is isolated from the dependency graph: `{node}`"),
            evidence: vec![],
        }
    }

    fn empty_report() -> AnalysisReport {
        AnalysisReport {
            root: ".".to_string(),
            total_files: 0,
            source_files: 0,
            avg_complexity: "Low".to_string(),
            manifests: vec![],
            languages: Default::default(),
            top_level_entries: vec![],
            architecture_hints: vec![],
            modules: vec![],
            graph_nodes: vec![],
            dependencies: vec![],
            todo_files: 0,
            cycles: CycleReport {
                has_cycle: false,
                cycles: vec![],
            },
            layers: LayerModel { layers: vec![] },
            violations: vec![],
            roles: vec![],
            semantic_layers: vec![],
            data_flow: vec![],
            issues: vec![],
            code_issues: vec![],
            summary: AnalysisSummary::default(),
            next_action: String::new(),
            root_cause: None,
            refactor_plan: vec![],
        }
    }

    fn sample_unified_result(mode: AnalyzeMode) -> UnifiedAnalyzeResult {
        UnifiedAnalyzeResult {
            path: ".".to_string(),
            mode,
            intent: "Maintainability".to_string(),
            modules: 2,
            cycles: 1,
            coupling: "High".to_string(),
            top_issue: "debug ↔ renderer cycle".to_string(),
            violations: vec!["1 dependency cycle(s) detected".to_string()],
            metrics: DecisionMetrics {
                si: 0.91,
                cs: 0.98,
                rp: 0.38,
                er: 0.05,
            },
            decision: DecisionContext {
                action: "RemoveDependency(debug -> renderer)".to_string(),
                expected_impact: "coupling down, ownership clearer".to_string(),
                score: 0.82,
                confidence: 0.91,
                risk: "Low".to_string(),
                intent_match: "Maintainability".to_string(),
            },
            analysis: ProjectAnalysisResult {
                files: vec![
                    FileAnalysis {
                        path: "src/debug.rs".to_string(),
                        language: ProjectLanguage::Rust,
                        complexity: ProjectComplexity::Medium,
                        todos: Vec::new(),
                    },
                    FileAnalysis {
                        path: "src/renderer.rs".to_string(),
                        language: ProjectLanguage::Rust,
                        complexity: ProjectComplexity::High,
                        todos: vec!["TODO: break cycle".to_string()],
                    },
                ],
                dependencies: vec![
                    DependencyEdge {
                        from: "debug".to_string(),
                        to: "renderer".to_string(),
                        edge_type: DependencyEdgeType::Direct,
                    },
                    DependencyEdge {
                        from: "renderer".to_string(),
                        to: "debug".to_string(),
                        edge_type: DependencyEdgeType::Direct,
                    },
                ],
                modules: vec![
                    ProjectModule {
                        name: "debug".to_string(),
                        files: vec!["src/debug.rs".to_string()],
                    },
                    ProjectModule {
                        name: "renderer".to_string(),
                        files: vec!["src/renderer.rs".to_string()],
                    },
                ],
                summary: ProjectSummary {
                    total_files: 2,
                    languages: vec![ProjectLanguage::Rust],
                    avg_complexity: ProjectComplexity::Medium,
                },
            },
            report: None,
            design: None,
        }
    }

    // ── P2-A: RoleMismatch role-specific hint ──────────────────────────────

    #[test]
    fn issue_hint_role_mismatch_includes_from_role() {
        let issue = make_role_mismatch_issue("Presentation", "Utility");
        let hint = issue_hint(&issue).expect("hint should be Some");
        assert!(
            hint.contains("Presentation"),
            "hint should mention from role: {hint}"
        );
    }

    #[test]
    fn issue_hint_role_mismatch_includes_to_role() {
        let issue = make_role_mismatch_issue("Presentation", "Utility");
        let hint = issue_hint(&issue).expect("hint should be Some");
        assert!(
            hint.contains("Utility"),
            "hint should mention to role: {hint}"
        );
    }

    #[test]
    fn issue_hint_role_mismatch_fallback_when_no_evidence() {
        let issue = Issue {
            id: "rm-no-ev".to_string(),
            kind: IssueType::RoleMismatch,
            severity: Severity::Medium,
            scope: IssueScope::Edge("a".to_string(), "b".to_string()),
            description: "Role boundary mismatch".to_string(),
            evidence: vec![],
        };
        let hint = issue_hint(&issue).expect("hint should be Some even without evidence");
        assert!(!hint.is_empty());
    }

    #[test]
    fn issue_hint_orphan_is_some() {
        let issue = make_orphan_issue("types");
        assert!(issue_hint(&issue).is_some());
    }

    #[test]
    fn issue_hint_layer_violation_is_none() {
        let issue = Issue {
            id: "lv".to_string(),
            kind: IssueType::LayerViolation,
            severity: Severity::High,
            scope: IssueScope::Edge("a".to_string(), "b".to_string()),
            description: "Layer violation".to_string(),
            evidence: vec![],
        };
        assert!(issue_hint(&issue).is_none());
    }

    #[test]
    fn render_role_mismatch_hint_appears_in_output() {
        let mut report = empty_report();
        report.issues = vec![make_role_mismatch_issue("Presentation", "Utility")];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("Presentation"),
            "rendered output should contain role name: {output}"
        );
        assert!(
            output.contains("Utility"),
            "rendered output should contain role name: {output}"
        );
    }

    // ── P3: Violation rendering with semantic layer names ──────────────────

    #[test]
    fn render_violation_shows_layer_name_in_output() {
        let mut report = empty_report();
        report.violations = vec![LayerViolation {
            from: "renderer".to_string(),
            to: "debug".to_string(),
            from_level: 1,
            to_level: 1,
            from_layer_name: "ApplicationLayer".to_string(),
            to_layer_name: "ApplicationLayer".to_string(),
        }];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("ApplicationLayer"),
            "output should contain semantic layer name: {output}"
        );
    }

    #[test]
    fn render_violation_same_layer_cycle_tag_present() {
        let mut report = empty_report();
        report.violations = vec![LayerViolation {
            from: "a".to_string(),
            to: "b".to_string(),
            from_level: 1,
            to_level: 1,
            from_layer_name: "ApplicationLayer".to_string(),
            to_layer_name: "ApplicationLayer".to_string(),
        }];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("SAME-LAYER CYCLE"),
            "output should contain SAME-LAYER CYCLE tag: {output}"
        );
    }

    #[test]
    fn render_violation_downward_tag_present() {
        let mut report = empty_report();
        report.violations = vec![LayerViolation {
            from: "a".to_string(),
            to: "b".to_string(),
            from_level: 2,
            to_level: 1,
            from_layer_name: "InterfaceLayer".to_string(),
            to_layer_name: "ApplicationLayer".to_string(),
        }];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("DOWNWARD VIOLATION"),
            "output should contain DOWNWARD VIOLATION tag: {output}"
        );
    }

    #[test]
    fn render_violation_layer_skip_tag_present() {
        let mut report = empty_report();
        report.violations = vec![LayerViolation {
            from: "a".to_string(),
            to: "b".to_string(),
            from_level: 0,
            to_level: 2,
            from_layer_name: "CoreLayer".to_string(),
            to_layer_name: "InterfaceLayer".to_string(),
        }];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("LAYER SKIP"),
            "output should contain LAYER SKIP tag: {output}"
        );
    }

    #[test]
    fn render_violation_graceful_when_layer_name_empty() {
        let mut report = empty_report();
        report.violations = vec![LayerViolation {
            from: "a".to_string(),
            to: "b".to_string(),
            from_level: 1,
            to_level: 1,
            from_layer_name: String::new(),
            to_layer_name: String::new(),
        }];
        let mut buf = Vec::new();
        render_analysis_report(&mut buf, &report).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("Layer 1"),
            "output should fall back to numeric layer: {output}"
        );
    }

    #[test]
    fn render_dbm_analyze_is_deterministic() {
        let issues = vec![
            DesignIssue {
                id: unified_design_ir::IssueId { value: "b".into() },
                issue_type: DesignIssueType::OverSpecification,
                severity: DesignSeverity::Medium,
                priority: unified_design_ir::Priority::P2,
                order: 2,
                blocks: Vec::new(),
                path: unified_design_ir::FieldPath {
                    segments: vec!["interface".into()],
                },
                reason: DesignIssueReason::ExcessiveComplexity,
                evidence: unified_design_ir::IssueEvidence {
                    before: None,
                    after: None,
                    semantic_reason: None,
                    impact_reason: None,
                },
                fix_hint: None,
            },
            DesignIssue {
                id: unified_design_ir::IssueId { value: "a".into() },
                issue_type: DesignIssueType::Missing,
                severity: DesignSeverity::Critical,
                priority: unified_design_ir::Priority::P0,
                order: 1,
                blocks: Vec::new(),
                path: unified_design_ir::FieldPath {
                    segments: vec!["function".into(), "functions".into()],
                },
                reason: DesignIssueReason::MissingRequiredField,
                evidence: unified_design_ir::IssueEvidence {
                    before: None,
                    after: None,
                    semantic_reason: None,
                    impact_reason: None,
                },
                fix_hint: None,
            },
        ];
        let summary = DesignIssueSummary {
            total: 2,
            critical: 1,
            high: 0,
            medium: 1,
            low: 0,
        };

        let output = render_dbm_analyze("/tmp/design.md", &summary, &issues);

        assert_eq!(
            output,
            "=== DBM Analyze ===\n\nInput: /tmp/design.md\nStage: Context\nStatus: In Progress\n\nSummary:\n  Critical: 1\n  High: 0\n  Medium: 1\n  Low: 0\n\nIssues:\n\n---\nDetails:\n\n[Critical] Missing\n  Path: function.functions\n  Reason: MissingRequiredField\n\n[Medium] OverSpecification\n  Path: interface\n  Reason: ExcessiveComplexity\n"
        );
    }

    #[test]
    fn unified_summary_uses_expected_order() {
        let output = render_unified_analyze_summary(&sample_unified_result(AnalyzeMode::Summary));
        let header = output.find("DBM Analyze Report").expect("header");
        let summary = output.find("Modules: 2").expect("summary");
        let decision = output.find("Decision Context").expect("decision");
        assert!(header < summary);
        assert!(summary < decision);
    }

    #[test]
    fn unified_detailed_includes_decision_and_metrics() {
        let output = render_unified_analyze_detailed(&sample_unified_result(AnalyzeMode::Detailed));
        assert!(output.contains("[Modules]"), "got: {output}");
        assert!(output.contains("[Metrics: SI/CS/RP/ER]"), "got: {output}");
        assert!(output.contains("Top Recommendation:"), "got: {output}");
        assert!(output.contains("Confidence: High (0.91)"), "got: {output}");
    }

    #[test]
    fn unified_detailed_handles_empty_data() {
        let mut result = sample_unified_result(AnalyzeMode::Detailed);
        result.analysis.modules.clear();
        result.analysis.dependencies.clear();
        result.violations.clear();
        result.top_issue = String::new();
        let output = render_unified_analyze_detailed(&result);
        assert!(output.contains("No issues detected"), "got: {output}");
    }

    #[test]
    fn render_dbm_diff_is_deterministic() {
        let diff = DesignDiffResult {
            changes: vec![
                unified_design_ir::FieldChange {
                    path: unified_design_ir::FieldPath {
                        segments: vec!["context".into(), "use_case".into()],
                    },
                    before: Some(serde_json::json!("build application")),
                    after: Some(serde_json::json!("build CLI tool")),
                    change_type: DesignChangeType::Modified,
                },
                unified_design_ir::FieldChange {
                    path: unified_design_ir::FieldPath {
                        segments: vec!["function".into(), "functions".into()],
                    },
                    before: None,
                    after: Some(serde_json::json!(["build"])),
                    change_type: DesignChangeType::Added,
                },
            ],
            summary: unified_design_ir::DiffSummary {
                added: 1,
                removed: 0,
                modified: 1,
                net_complexity: 0,
            },
            semantic: unified_design_ir::SemanticDiff {
                is_equivalent: false,
                reason: unified_design_ir::SemanticReason::ValueMismatch,
            },
            impact: unified_design_ir::Impact::Neutral,
            impact_reason: unified_design_ir::ImpactReason::MixedChange,
        };

        let output = render_dbm_diff("/tmp/design.md", &diff);

        assert_eq!(
            output,
            "=== DBM Diff ===\n\nInput: /tmp/design.md\n\nChanges:\n\n[ADD]\n  Path: function.functions\n  Value: [\"build\"]\n\n[MODIFY]\n  Path: context.use_case\n  Before: \"build application\"\n  After:  \"build CLI tool\"\n"
        );
    }
}
