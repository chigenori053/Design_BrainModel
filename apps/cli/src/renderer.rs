use std::io::{self, Write};

use design_search_engine::stable_v03::ReasoningTrace;
use runtime_core::intent_refiner::{CoreSlot, SlotMap};
use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{Clarification, Explanation, source_to_message};

use crate::app::{AnalysisReport, CodingReport, DesignReport, RefactorReport, RunReport, ValidationReport};
use integration_layer::{
    Issue, IssueType, LayerType, NodeRole, PatchOperation, Pattern, PhaseType, RefactorAction,
    RefactorPlanAction, Severity,
};

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

pub fn render_reasoning_trace<W: Write>(
    writer: &mut W,
    trace: &ReasoningTrace,
) -> io::Result<()> {
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
            writeln!(writer, "  {} -> {} ({} <= {})", violation.from, violation.to, violation.from_level, violation.to_level)?;
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
            writeln!(writer, "- Layer {}: {}", layer.level, layer_type_label(&layer.layer_type))?;
        }
    }
    if !report.data_flow.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Data Flow:")?;
        for flow in &report.data_flow {
            writeln!(writer, "- {} -> {} ({:.2})", flow.from, flow.to, flow.weight)?;
        }
    }
    writeln!(writer)?;
    render_issue_group(writer, "Structural Issues", &report.issues, "Structural")?;
    render_issue_group(writer, "Semantic Issues", &report.issues, "Semantic")?;
    render_issue_group(writer, "Data Flow Issues", &report.issues, "Data Flow")?;
    writeln!(writer, "Summary: Critical: {} | High: {} | Medium: {}", report.summary.critical, report.summary.high, report.summary.medium)?;
    writeln!(writer)?;
    writeln!(writer, "Next Action:")?;
    writeln!(writer, "{}", report.next_action)?;
    writer.flush()
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
        writeln!(writer, "- none observed")?;
        return Ok(());
    }
    writeln!(writer, "{title}")?;
    for issue in group {
        writeln!(
            writer,
            "- ({}) {}",
            severity_label(&issue.severity),
            issue.description
        )?;
    }
    Ok(())
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
        if report.cycles.has_cycle { "INVALID" } else { "OK" }
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
            writeln!(writer, "- {}: {}", role.node_name, node_role_label(&role.role))?;
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
            writeln!(writer, " - Layer {}: {}", layer.level, layer.nodes.join(", "))?;
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
            format!("Introduce Interface between {} and {}", between.0, between.1)
        }
        RefactorPlanAction::RemoveDependency { from, to } => {
            format!("Remove Dependency {} -> {}", from, to)
        }
        RefactorPlanAction::SplitModule { target } => format!("Split Module {}", target),
        RefactorPlanAction::MoveDependency { from, to, via } => match via {
            Some(via) => format!("Move Dependency {} -> {} via {}", from, to, via),
            None => format!("Move Dependency {} -> {}", from, to),
        }
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

pub fn render_refactor_report<W: Write>(writer: &mut W, report: &RefactorReport) -> io::Result<()> {
    writeln!(writer, "Refactor Plan")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer)?;
    for (index, phase) in report.plan.phases.iter().enumerate() {
        writeln!(writer, "Phase {}: {}", index + 1, phase_type_label(&phase.phase_type))?;
        for action in &phase.actions {
            writeln!(writer, "- {}", refactor_plan_action_label(action))?;
        }
        writeln!(writer)?;
    }
    writeln!(writer, "Code Patches:")?;
    for (index, patch) in report.patches.iter().enumerate() {
        writeln!(writer)?;
        writeln!(writer, "[Patch {}]", index + 1)?;
        writeln!(writer, "Action: {}", refactor_plan_action_label(&patch.action))?;
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

pub fn render_coding_report<W: Write>(writer: &mut W, report: &CodingReport) -> io::Result<()> {
    writeln!(writer, "Code Changes")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Mode: {}", if report.dry_run { "dry-run" } else { "apply" })?;
    writeln!(writer, "Status: {}", report.execution.status)?;
    writeln!(
        writer,
        "Build: {}",
        if report.execution.build_ok { "OK" } else { "FAILED" }
    )?;
    writeln!(writer, "Checked: {}", report.execution.checked)?;
    writeln!(writer, "Applied: {}", report.execution.applied)?;
    writeln!(writer, "Rollback: {}", report.execution.rolled_back)?;
    writeln!(writer, "Files changed: {}", report.execution.files_changed)?;
    if let Some(reason) = &report.execution.reason {
        writeln!(writer, "Reason: {reason}")?;
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

fn patch_operation_label(operation: &PatchOperation) -> String {
    match operation {
        PatchOperation::CreateInterface { name, .. } => {
            format!("Create interface {}", name)
        }
        PatchOperation::UpdateDependency { from, to, via } => match via {
            Some(via) => format!("Update dependency {} -> {} via {}", from, to, via),
            None => format!("Update dependency {} -> {}", from, to),
        },
        PatchOperation::SplitModule { module, new_modules } => {
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
