use crate::commands::analyze::project::UnifiedAnalyzeResult;

pub fn render_summary(result: &UnifiedAnalyzeResult) -> String {
    let top_issue = if result.top_issue.is_empty() {
        "No issues detected"
    } else {
        &result.top_issue
    };
    [
        format!("Modules: {}", result.modules),
        format!("Cycles: {}", result.cycles),
        format!("Coupling: {}", result.coupling),
        format!("Top Issue: {}", top_issue),
    ]
    .join("\n")
}
