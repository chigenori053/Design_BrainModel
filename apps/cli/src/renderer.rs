use std::io::{self, Write};

use design_search_engine::stable_v03::ReasoningTrace;
use runtime_core::intent_refiner::{CoreSlot, SlotMap};
use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{Clarification, Explanation, source_to_message};

use crate::autonomous_execute::AutonomousExecuteReport;
use crate::execution_foundation::ExecReport;
use crate::service::dto::{
    AnalysisReport, CodingReport, DesignReport, RefactorReport, RulesReport, RunReport,
    ValidationReport,
};
use integration_layer::{
    EvidenceType, Issue, IssueType, LayerType, NodeRole, PatchOperation, Pattern, PhaseType,
    RefactorAction, RefactorPlanAction, Severity,
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
                    let mut parts = ev.value.splitn(2, "->");
                    let from_role = parts.next()?;
                    let to_role = parts.next()?;
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

pub fn render_rules_report<W: Write>(writer: &mut W, report: &RulesReport) -> io::Result<()> {
    writeln!(writer, "Rules")?;
    writeln!(writer, "Language: {}", report.language)?;
    writeln!(writer, "Action: {}", report.action)?;
    writeln!(writer, "Active: {}", report.active.len())?;
    writeln!(writer, "Candidate: {}", report.candidate.len())?;
    writeln!(writer, "Validated: {}", report.validated.len())?;
    writeln!(writer, "Deprecated: {}", report.deprecated.len())?;
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
    if !report.deprecated.is_empty() {
        writeln!(writer, "Deprecated Rules:")?;
        for rule in &report.deprecated {
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
            summary: AnalysisSummary::default(),
            next_action: String::new(),
            root_cause: None,
            refactor_plan: vec![],
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
}
