use crate::commands::analyze::project::UnifiedAnalyzeResult;
use crate::renderer::formatter::format_score;

pub fn render_detailed(result: &UnifiedAnalyzeResult) -> String {
    let mut out = String::new();
    out.push_str(&render_modules(result));
    out.push_str("\n\n");
    out.push_str(&render_dependencies(result));
    out.push_str("\n\n");
    out.push_str(&render_violations(result));
    out.push_str("\n\n");
    out.push_str(&render_metrics(result));
    out.push_str("\n\n");
    out.push_str(&render_top_candidates(result));
    out
}

fn render_modules(result: &UnifiedAnalyzeResult) -> String {
    let mut out = String::from("[Modules]\n");
    if result.analysis.modules.is_empty() {
        out.push_str("No issues detected");
        return out;
    }
    for module in &result.analysis.modules {
        out.push_str(&format!("- {}\n", module.name));
    }
    out.trim_end().to_string()
}

fn render_dependencies(result: &UnifiedAnalyzeResult) -> String {
    let mut out = String::from("[Dependencies]\n");
    if result.analysis.dependencies.is_empty() {
        out.push_str("No issues detected");
        return out;
    }
    for dep in &result.analysis.dependencies {
        let marker = if result
            .analysis
            .dependencies
            .iter()
            .any(|other| other.from == dep.to && other.to == dep.from)
        {
            " (cycle)"
        } else {
            ""
        };
        out.push_str(&format!("- {} -> {}{}\n", dep.from, dep.to, marker));
    }
    out.trim_end().to_string()
}

fn render_violations(result: &UnifiedAnalyzeResult) -> String {
    let mut out = String::from("[Violations]\n");
    if result.violations.is_empty() {
        out.push_str("No issues detected");
        return out;
    }
    for violation in &result.violations {
        out.push_str(&format!("- {violation}\n"));
    }
    out.trim_end().to_string()
}

fn render_metrics(result: &UnifiedAnalyzeResult) -> String {
    format!(
        "[Metrics: SI/CS/RP/ER]\nSI={} CS={} RP={} ER={}",
        format_score(result.metrics.si),
        format_score(result.metrics.cs),
        format_score(result.metrics.rp),
        format_score(result.metrics.er),
    )
}

fn render_top_candidates(result: &UnifiedAnalyzeResult) -> String {
    format!("[Top Candidates]\n- {}", result.decision.action)
}
