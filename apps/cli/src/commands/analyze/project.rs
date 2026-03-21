/// /analyze project <path>
///
/// プロジェクト全体を解析してサマリ・モジュール・依存関係を出力する。
///
/// 出力例:
/// ```text
/// Project Summary:
/// - Files: 42
/// - Languages: Rust
/// - Avg Complexity: Medium
///
/// Modules:
/// - planner (5 files)
/// - dbm (4 files)
///
/// Dependencies:
/// - repl → planner
/// - planner → dbm
///
/// Issues:
/// - TODO found in planner/rule_based.rs
/// ```
use crate::command::{CommandError, Output, SubCommandHandler};
use crate::dbm::analyzer;
use crate::session::AgentSession;

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("project", execute)
}

fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = args.first().map(|s| s.as_str()).unwrap_or(".");

    let result = analyzer::analyze_project(path).map_err(|e| CommandError::ExecutionError(e))?;

    let mut out = String::new();

    // ── Project Summary ──
    out.push_str("Project Summary:\n");
    out.push_str(&format!("- Files: {}\n", result.summary.total_files));
    if !result.summary.languages.is_empty() {
        let langs: Vec<&str> = result
            .summary
            .languages
            .iter()
            .map(|l| l.as_str())
            .collect();
        out.push_str(&format!("- Languages: {}\n", langs.join(", ")));
    }
    out.push_str(&format!(
        "- Avg Complexity: {}\n",
        result.summary.avg_complexity.as_str()
    ));

    // ── Modules ──
    if !result.modules.is_empty() {
        out.push('\n');
        out.push_str("Modules:\n");
        for module in &result.modules {
            out.push_str(&format!(
                "- {} ({} files)\n",
                module.name,
                module.files.len()
            ));
        }
    }

    // ── Dependencies ──
    if !result.dependencies.is_empty() {
        out.push('\n');
        out.push_str("Dependencies:\n");
        for dep in &result.dependencies {
            out.push_str(&format!("- {} → {}\n", dep.from, dep.to));
        }
    }

    // ── Issues ──
    let issue_files: Vec<_> = result
        .files
        .iter()
        .filter(|f| !f.todos.is_empty())
        .collect();
    if !issue_files.is_empty() {
        out.push('\n');
        out.push_str("Issues:\n");
        for file in issue_files {
            out.push_str(&format!("- TODO found in {}\n", file.path));
        }
    }

    if result.summary.total_files == 0 {
        out.push_str("(no supported files found)\n");
    }

    Ok(Output::text(out))
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
        assert!(
            out.message.contains("Project Summary:"),
            "got: {}",
            out.message
        );
        assert!(out.message.contains("Files:"));
    }

    #[test]
    fn analyze_project_src_shows_modules() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["src/".to_string()], &mut session).unwrap();
        assert!(out.message.contains("Modules:"), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_src_shows_rust_language() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["src/".to_string()], &mut session).unwrap();
        assert!(out.message.contains("Rust"), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_nonexistent_path_shows_no_files() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["/nonexistent/xyz123".to_string()], &mut session).unwrap();
        assert!(out.message.contains("Files: 0") || out.message.contains("no supported files"));
    }

    #[test]
    fn analyze_project_default_path_is_dot() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[], &mut session).unwrap();
        assert!(out.message.contains("Project Summary:"));
    }
}
