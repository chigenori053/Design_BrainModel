use crate::dbm::analyzer::{Complexity, ProjectAnalysisResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Japanese,
    English,
}

impl Language {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ja" | "jp" | "japanese" => Some(Self::Japanese),
            "en" | "english" => Some(Self::English),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub text: String,
}

pub trait ReportGenerator {
    fn generate(result: &ProjectAnalysisResult, lang: Language) -> Report;
}

pub struct TemplateReportGenerator;

impl ReportGenerator for TemplateReportGenerator {
    fn generate(result: &ProjectAnalysisResult, lang: Language) -> Report {
        Report {
            text: match lang {
                Language::Japanese => generate_japanese_report(result),
                Language::English => generate_english_report(result),
            },
        }
    }
}

fn generate_japanese_report(result: &ProjectAnalysisResult) -> String {
    let mut lines = vec![
        "このプロジェクトでは以下の構造的問題が検出されました：".to_string(),
        String::new(),
        "【Summary】".to_string(),
        format!(
            "- 対象ファイル数: {} / モジュール数: {} / 依存関係数: {}",
            result.summary.total_files,
            result.modules.len(),
            result.dependencies.len()
        ),
        format!(
            "- 平均複雑度: {} / 言語: {}",
            result.summary.avg_complexity.as_str(),
            join_languages(result)
        ),
        String::new(),
        "【主な問題】".to_string(),
    ];
    lines.extend(issue_lines_ja(result));
    lines.push(String::new());
    lines.push("【影響】".to_string());
    lines.extend(impact_lines_ja(result));
    lines.push(String::new());
    lines.push("【推奨対応】".to_string());
    lines.extend(recommendation_lines_ja(result));
    lines.join("\n")
}

fn generate_english_report(result: &ProjectAnalysisResult) -> String {
    let mut lines = vec![
        "The following structural issues were identified:".to_string(),
        String::new(),
        "[Summary]".to_string(),
        format!(
            "- Files: {} / Modules: {} / Dependencies: {}",
            result.summary.total_files,
            result.modules.len(),
            result.dependencies.len()
        ),
        format!(
            "- Average complexity: {} / Languages: {}",
            result.summary.avg_complexity.as_str(),
            join_languages(result)
        ),
        String::new(),
        "[Key Issues]".to_string(),
    ];
    lines.extend(issue_lines_en(result));
    lines.push(String::new());
    lines.push("[Impact]".to_string());
    lines.extend(impact_lines_en(result));
    lines.push(String::new());
    lines.push("[Recommendations]".to_string());
    lines.extend(recommendation_lines_en(result));
    lines.join("\n")
}

fn issue_lines_ja(result: &ProjectAnalysisResult) -> Vec<String> {
    let todo_files = result
        .files
        .iter()
        .filter(|file| !file.todos.is_empty())
        .count();
    let high_complexity = result
        .files
        .iter()
        .filter(|file| file.complexity == Complexity::High)
        .count();

    let mut lines = Vec::new();
    if result.summary.total_files == 0 {
        lines.push("- 解析対象のサポートファイルは見つかりませんでした。".to_string());
    } else {
        lines.push(format!("- TODO/FIXME を含むファイル: {}", todo_files));
        lines.push(format!("- 高複雑度ファイル: {}", high_complexity));
        lines.push(format!(
            "- モジュール間依存エッジ: {}",
            result.dependencies.len()
        ));
    }
    lines
}

fn issue_lines_en(result: &ProjectAnalysisResult) -> Vec<String> {
    let todo_files = result
        .files
        .iter()
        .filter(|file| !file.todos.is_empty())
        .count();
    let high_complexity = result
        .files
        .iter()
        .filter(|file| file.complexity == Complexity::High)
        .count();

    let mut lines = Vec::new();
    if result.summary.total_files == 0 {
        lines.push("- No supported source files were found.".to_string());
    } else {
        lines.push(format!(
            "- Files containing TODO/FIXME markers: {}",
            todo_files
        ));
        lines.push(format!("- High-complexity files: {}", high_complexity));
        lines.push(format!(
            "- Inter-module dependency edges: {}",
            result.dependencies.len()
        ));
    }
    lines
}

fn impact_lines_ja(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.summary.total_files == 0 {
        return vec!["- 対象ファイルがないため、設計評価は限定的です。".to_string()];
    }
    vec![
        format!(
            "- 複雑度が {} のため、変更時の理解コストに影響する可能性があります。",
            result.summary.avg_complexity.as_str()
        ),
        format!(
            "- モジュール数 {} と依存関係数 {} から、責務分散と結合度の確認が必要です。",
            result.modules.len(),
            result.dependencies.len()
        ),
    ]
}

fn impact_lines_en(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.summary.total_files == 0 {
        return vec![
            "- Design evaluation is limited because no target files were found.".to_string(),
        ];
    }
    vec![
        format!(
            "- Average complexity is {}, which may increase comprehension cost during changes.",
            result.summary.avg_complexity.as_str()
        ),
        format!(
            "- {} modules and {} dependency edges suggest that coupling and responsibility boundaries should be reviewed.",
            result.modules.len(),
            result.dependencies.len()
        ),
    ]
}

fn recommendation_lines_ja(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.summary.total_files == 0 {
        return vec!["- 対応不要です。解析対象パスを見直してください。".to_string()];
    }

    let densest_module = result
        .modules
        .iter()
        .max_by_key(|module| (module.files.len(), module.name.clone()));

    let mut lines = Vec::new();
    if let Some(module) = densest_module {
        lines.push(format!(
            "- モジュール `{}` の責務を見直し、ファイル数 {} の集中を緩和してください。",
            module.name,
            module.files.len()
        ));
    }
    if result.files.iter().any(|file| !file.todos.is_empty()) {
        lines.push("- TODO/FIXME を解消し、暫定実装を減らしてください。".to_string());
    }
    if result.dependencies.len() > result.modules.len().saturating_add(2) {
        lines.push("- 依存関係を整理し、不要なモジュール結合を削減してください。".to_string());
    }
    if lines.is_empty() {
        lines.push("- 現状の構造は安定しています。大きな設計変更は不要です。".to_string());
    }
    lines
}

fn recommendation_lines_en(result: &ProjectAnalysisResult) -> Vec<String> {
    if result.summary.total_files == 0 {
        return vec!["- No action is required. Recheck the analysis target path.".to_string()];
    }

    let densest_module = result
        .modules
        .iter()
        .max_by_key(|module| (module.files.len(), module.name.clone()));

    let mut lines = Vec::new();
    if let Some(module) = densest_module {
        lines.push(format!(
            "- Review responsibility boundaries in module `{}` to reduce concentration across {} files.",
            module.name,
            module.files.len()
        ));
    }
    if result.files.iter().any(|file| !file.todos.is_empty()) {
        lines.push(
            "- Resolve TODO/FIXME markers to reduce provisional implementation debt.".to_string(),
        );
    }
    if result.dependencies.len() > result.modules.len().saturating_add(2) {
        lines.push(
            "- Reduce unnecessary inter-module coupling by pruning dependency edges.".to_string(),
        );
    }
    if lines.is_empty() {
        lines.push(
            "- The current structure appears stable and does not require major design changes."
                .to_string(),
        );
    }
    lines
}

fn join_languages(result: &ProjectAnalysisResult) -> String {
    if result.summary.languages.is_empty() {
        "None".to_string()
    } else {
        result
            .summary
            .languages
            .iter()
            .map(|language| language.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbm::analyzer::{
        Complexity, DependencyEdge, DependencyEdgeType, FileAnalysis, Language as ProjectLanguage,
        Module, ProjectSummary,
    };

    fn sample_result() -> ProjectAnalysisResult {
        ProjectAnalysisResult {
            files: vec![
                FileAnalysis {
                    path: "src/app.rs".to_string(),
                    language: ProjectLanguage::Rust,
                    complexity: Complexity::High,
                    todos: vec!["TODO: split responsibilities".to_string()],
                },
                FileAnalysis {
                    path: "src/renderer.rs".to_string(),
                    language: ProjectLanguage::Rust,
                    complexity: Complexity::Medium,
                    todos: Vec::new(),
                },
            ],
            dependencies: vec![DependencyEdge {
                from: "app".to_string(),
                to: "renderer".to_string(),
                edge_type: DependencyEdgeType::Direct,
            }],
            modules: vec![
                Module {
                    name: "app".to_string(),
                    files: vec!["src/app.rs".to_string(), "src/main.rs".to_string()],
                },
                Module {
                    name: "renderer".to_string(),
                    files: vec!["src/renderer.rs".to_string()],
                },
            ],
            summary: ProjectSummary {
                total_files: 2,
                languages: vec![ProjectLanguage::Rust],
                avg_complexity: Complexity::Medium,
            },
        }
    }

    #[test]
    fn report_generation_is_deterministic() {
        let result = sample_result();
        let first = TemplateReportGenerator::generate(&result, Language::Japanese);
        let second = TemplateReportGenerator::generate(&result, Language::Japanese);
        assert_eq!(first, second);
    }

    #[test]
    fn report_switches_between_languages() {
        let result = sample_result();
        let ja = TemplateReportGenerator::generate(&result, Language::Japanese);
        let en = TemplateReportGenerator::generate(&result, Language::English);
        assert!(ja.text.contains("【主な問題】"));
        assert!(en.text.contains("[Key Issues]"));
    }

    #[test]
    fn report_handles_empty_result_safely() {
        let empty = ProjectAnalysisResult {
            files: Vec::new(),
            dependencies: Vec::new(),
            modules: Vec::new(),
            summary: ProjectSummary {
                total_files: 0,
                languages: Vec::new(),
                avg_complexity: Complexity::Low,
            },
        };
        let report = TemplateReportGenerator::generate(&empty, Language::English);
        assert!(
            report
                .text
                .contains("No supported source files were found.")
        );
    }
}
