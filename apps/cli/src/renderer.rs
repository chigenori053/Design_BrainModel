use std::io::{self, Write};

use runtime_core::intent_refiner::{CoreSlot, SlotMap};
use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{Clarification, Explanation, source_to_message};

use crate::app::{AnalysisReport, DesignReport, RunReport, ValidationReport};

pub fn render_result<W: Write>(writer: &mut W, result: &RuntimeResult) -> io::Result<()> {
    writeln!(writer, "✔ Project generated")?;
    if let Some(explanation) = &result.explanation {
        writeln!(writer)?;
        render_explanation(writer, explanation)?;
    } else if let Some(trace) = &result.intent_trace {
        writeln!(writer)?;
        render_summary(writer, &trace.final_slots)?;
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
    if !report.languages.is_empty() {
        writeln!(writer, "Languages:")?;
        for (language, count) in &report.languages {
            writeln!(writer, " - {language}: {count}")?;
        }
    }
    if !report.architecture_hints.is_empty() {
        writeln!(writer, "Hints: {}", report.architecture_hints.join(", "))?;
    }
    writer.flush()
}

pub fn render_design_report<W: Write>(writer: &mut W, report: &DesignReport) -> io::Result<()> {
    writeln!(writer, "Design")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Style: {}", report.inferred_style)?;
    writeln!(writer, "Components: {}", report.components.join(", "))?;
    writeln!(writer, "Design units: {}", report.design_units.join(", "))?;
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
    writer.flush()
}

pub fn render_run_report<W: Write>(writer: &mut W, report: &RunReport) -> io::Result<()> {
    writeln!(writer, "Run")?;
    writeln!(writer, "Root: {}", report.root)?;
    writeln!(writer, "Mode: {}", report.mode)?;
    match &report.selected_command {
        Some(command) => writeln!(writer, "Command: {command}")?,
        None => writeln!(writer, "Command: <none>")?,
    }
    writeln!(writer, "Reason: {}", report.reason)?;
    writer.flush()
}
