use crate::commands::analyze::project::{AnalyzeMode, UnifiedAnalyzeResult};

pub fn render_header(result: &UnifiedAnalyzeResult) -> String {
    let mode = match result.mode {
        AnalyzeMode::Summary => "Summary",
        AnalyzeMode::Detailed => "Detailed",
    };
    format!(
        "DBM Analyze Report\nTarget: {}\nMode: {}\nIntent: {}",
        result.path, mode, result.intent
    )
}
