use crate::dbm::analyzer::ProjectAnalysisResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignDocument {
    pub markdown: String,
}

pub trait DesignExtractor {
    fn extract(result: &ProjectAnalysisResult) -> DesignDocument;
}

pub struct MarkdownDesignExtractor;

impl DesignExtractor for MarkdownDesignExtractor {
    fn extract(result: &ProjectAnalysisResult) -> DesignDocument {
        let mut sections = vec![
            "# System Design".to_string(),
            String::new(),
            "## Overview".to_string(),
            format!("- Files: {}", result.summary.total_files),
            format!("- Modules: {}", result.modules.len()),
            format!("- Dependencies: {}", result.dependencies.len()),
            format!(
                "- Average Complexity: {}",
                result.summary.avg_complexity.as_str()
            ),
            String::new(),
            "## Architecture Layers".to_string(),
        ];
        sections.extend(architecture_layers(result));
        sections.push(String::new());
        sections.push("## Modules".to_string());
        sections.extend(module_lines(result));
        sections.push(String::new());
        sections.push("## Dependencies".to_string());
        sections.extend(dependency_lines(result));
        sections.push(String::new());
        sections.push("## Issues".to_string());
        sections.extend(issue_lines(result));
        sections.push(String::new());
        sections.push("## Recommendations".to_string());
        sections.extend(recommendation_lines(result));

        DesignDocument {
            markdown: sections.join("\n"),
        }
    }
}

fn architecture_layers(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.modules.is_empty() {
        return vec!["- No modules were identified.".to_string()];
    }
    vec![
        "- Entry Layer: modules that directly own application bootstrapping or top-level files."
            .to_string(),
        "- Coordination Layer: modules that orchestrate dependencies between other modules."
            .to_string(),
        "- Implementation Layer: modules that mainly provide concrete behavior or utilities."
            .to_string(),
    ]
}

fn module_lines(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.modules.is_empty() {
        return vec!["- None".to_string()];
    }
    result
        .modules
        .iter()
        .map(|module| format!("- {}: {} file(s)", module.name, module.files.len()))
        .collect()
}

fn dependency_lines(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.dependencies.is_empty() {
        return vec!["- None".to_string()];
    }
    result
        .dependencies
        .iter()
        .map(|dep| format!("- {} -> {}", dep.from, dep.to))
        .collect()
}

fn issue_lines(result: &ProjectAnalysisResult) -> Vec<String> {
    let todo_files = result
        .files
        .iter()
        .filter(|file| !file.todos.is_empty())
        .map(|file| format!("- TODO/FIXME in {}", file.path))
        .collect::<Vec<_>>();
    if !todo_files.is_empty() {
        return todo_files;
    }
    vec!["- No explicit TODO/FIXME issues were detected.".to_string()]
}

fn recommendation_lines(result: &ProjectAnalysisResult) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(module) = result
        .modules
        .iter()
        .max_by_key(|module| (module.files.len(), module.name.clone()))
    {
        lines.push(format!(
            "- Review {} for possible responsibility split.",
            module.name
        ));
    }
    if result.dependencies.len() > result.modules.len().saturating_add(2) {
        lines.push("- Reduce inter-module dependency density.".to_string());
    }
    if result.files.iter().any(|file| !file.todos.is_empty()) {
        lines.push("- Replace TODO/FIXME markers with tracked implementation work.".to_string());
    }
    if lines.is_empty() {
        lines.push("- No immediate architectural recommendation.".to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbm::analyzer::{
        Complexity, DependencyEdge, FileAnalysis, Language, Module, ProjectSummary,
    };

    fn sample_result() -> ProjectAnalysisResult {
        ProjectAnalysisResult {
            files: vec![FileAnalysis {
                path: "src/app.rs".to_string(),
                language: Language::Rust,
                complexity: Complexity::Medium,
                todos: vec!["TODO: clean module split".to_string()],
            }],
            dependencies: vec![DependencyEdge {
                from: "app".to_string(),
                to: "renderer".to_string(),
            }],
            modules: vec![
                Module {
                    name: "app".to_string(),
                    files: vec!["src/app.rs".to_string()],
                },
                Module {
                    name: "renderer".to_string(),
                    files: vec!["src/renderer.rs".to_string()],
                },
            ],
            summary: ProjectSummary {
                total_files: 1,
                languages: vec![Language::Rust],
                avg_complexity: Complexity::Medium,
            },
        }
    }

    #[test]
    fn design_document_is_deterministic() {
        let result = sample_result();
        let first = MarkdownDesignExtractor::extract(&result);
        let second = MarkdownDesignExtractor::extract(&result);
        assert_eq!(first, second);
    }

    #[test]
    fn design_document_contains_expected_sections() {
        let result = sample_result();
        let document = MarkdownDesignExtractor::extract(&result);
        assert!(document.markdown.contains("# System Design"));
        assert!(document.markdown.contains("## Modules"));
        assert!(document.markdown.contains("## Dependencies"));
        assert!(document.markdown.contains("## Recommendations"));
    }

    #[test]
    fn design_document_handles_empty_result() {
        let result = ProjectAnalysisResult {
            files: Vec::new(),
            dependencies: Vec::new(),
            modules: Vec::new(),
            summary: ProjectSummary {
                total_files: 0,
                languages: Vec::new(),
                avg_complexity: Complexity::Low,
            },
        };
        let document = MarkdownDesignExtractor::extract(&result);
        assert!(document.markdown.contains("- None"));
    }
}
