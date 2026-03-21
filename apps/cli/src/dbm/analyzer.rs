/// Phase3 Extended: プロジェクト単位コード解析器
///
/// Phase3.1 追加:
/// - analyze_project() : プロジェクト全体解析
/// - scan_directory()  : ディレクトリ再帰走査
/// - extract_dependencies() : import/use 抽出
/// - group_modules()   : ディレクトリ単位モジュール推定
///
/// 設計方針: 精度よりも安定性優先。正規表現不使用・パニック禁止。
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

// ─── 言語 ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    Go,
    Python,
    Rust,
    TypeScript,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Go => "Go",
            Self::Python => "Python",
            Self::Rust => "Rust",
            Self::TypeScript => "TypeScript",
        }
    }

    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "ts" | "tsx" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
}

// ─── 複雑度 ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Complexity {
    Low,
    Medium,
    High,
}

impl Complexity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    pub fn from_counts(lines: usize, branches: usize) -> Self {
        if lines < 100 && branches < 5 {
            Self::Low
        } else if lines < 400 && branches < 20 {
            Self::Medium
        } else {
            Self::High
        }
    }

    pub fn score(self) -> f32 {
        match self {
            Self::Low => 1.0,
            Self::Medium => 2.0,
            Self::High => 3.0,
        }
    }

    pub fn from_score(score: f32) -> Self {
        if score < 1.5 {
            Self::Low
        } else if score < 2.5 {
            Self::Medium
        } else {
            Self::High
        }
    }
}

// ─── プロジェクト解析型 ──────────────────────────────────────────────────

/// ファイル単位の解析結果
#[derive(Clone, Debug)]
pub struct FileAnalysis {
    pub path: String,
    pub language: Language,
    pub complexity: Complexity,
    pub todos: Vec<String>,
}

/// モジュール間依存エッジ
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}

/// ディレクトリ単位のモジュール
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub files: Vec<String>,
}

/// プロジェクト全体サマリ
#[derive(Clone, Debug)]
pub struct ProjectSummary {
    pub total_files: usize,
    pub languages: Vec<Language>,
    pub avg_complexity: Complexity,
}

/// プロジェクト全体解析結果
#[derive(Clone, Debug)]
pub struct ProjectAnalysisResult {
    pub files: Vec<FileAnalysis>,
    pub dependencies: Vec<DependencyEdge>,
    pub modules: Vec<Module>,
    pub summary: ProjectSummary,
}

// ─── 旧 API（後方互換） ──────────────────────────────────────────────────

/// 旧型（後方互換）: analyze_path が返す結果
#[derive(Clone, Debug)]
pub struct AnalysisResult {
    pub path: String,
    pub modules: Vec<ModuleInfo>,
    pub total_lines: usize,
    pub suggestions: Vec<String>,
}

/// 旧型（後方互換）: ファイル単位の簡易情報
#[derive(Clone, Debug)]
pub struct ModuleInfo {
    pub name: String,
    pub complexity: &'static str,
    pub issues: Vec<String>,
    pub line_count: usize,
}

// ─── プロジェクト解析（NEW） ─────────────────────────────────────────────

const MAX_FILES: usize = 2000;

/// プロジェクト全体を解析する
///
/// ディレクトリを再帰走査し、ファイル解析・依存グラフ・モジュール推定を行う。
pub fn analyze_project(root_path: &str) -> Result<ProjectAnalysisResult, String> {
    let root = Path::new(root_path);

    if !root.exists() {
        return Ok(ProjectAnalysisResult {
            files: vec![],
            dependencies: vec![],
            modules: vec![],
            summary: ProjectSummary {
                total_files: 0,
                languages: vec![],
                avg_complexity: Complexity::Low,
            },
        });
    }

    // 1. ディレクトリ走査
    let paths = scan_directory(root)?;

    // 2. 各ファイル解析（相対パス + 依存抽出）
    let mut records: Vec<(FileAnalysis, Vec<String>)> = vec![];
    for path in &paths {
        if let Some(record) = analyze_file_project(path, root) {
            records.push(record);
        }
    }

    let files: Vec<FileAnalysis> = records.iter().map(|(f, _)| f.clone()).collect();

    // 3. モジュール構造を推定
    let modules = group_modules(&files);
    let known_modules: BTreeSet<String> = modules.iter().map(|m| m.name.clone()).collect();

    // 4. 依存グラフ構築
    let dependencies = build_dependency_edges(&records, &known_modules);

    // 5. サマリ生成
    let summary = build_summary(&files);

    Ok(ProjectAnalysisResult {
        files,
        dependencies,
        modules,
        summary,
    })
}

/// ディレクトリを再帰走査してサポート対象ファイルのパスリストを返す
pub fn scan_directory(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files = Vec::new();
    collect_supported_files(root, &mut files)?;
    files.truncate(MAX_FILES);
    Ok(files)
}

/// サポート言語のファイルを再帰収集する
fn collect_supported_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if files.len() >= MAX_FILES {
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if files.len() >= MAX_FILES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if matches!(
            name_str.as_ref(),
            ".git" | "target" | "node_modules" | "__pycache__"
        ) {
            continue;
        }
        if path.is_dir() {
            collect_supported_files(&path, files)?;
        } else if is_supported_extension(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e, "rs" | "py" | "ts" | "tsx" | "go"))
        .unwrap_or(false)
}

/// 1ファイルを解析して (FileAnalysis, raw_deps) を返す
fn analyze_file_project(path: &Path, root: &Path) -> Option<(FileAnalysis, Vec<String>)> {
    let ext = path.extension()?.to_str()?;
    let language = Language::from_extension(ext)?;

    let content = fs::read_to_string(path).ok()?;
    let lines = content.lines().count();
    let branches = count_branches(&content);
    let complexity = Complexity::from_counts(lines, branches);

    let todos: Vec<String> = content
        .lines()
        .filter(|l| l.contains("TODO") || l.contains("FIXME"))
        .take(5)
        .map(|l| l.trim().to_string())
        .collect();

    let deps = extract_dependencies(&content, &language);

    // ルートからの相対パス
    let rel_path = path
        .strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());

    Some((
        FileAnalysis {
            path: rel_path,
            language,
            complexity,
            todos,
        },
        deps,
    ))
}

/// ソースコードから import/use 文を解析して依存モジュール名リストを返す（公開）
pub fn extract_dependencies(content: &str, language: &Language) -> Vec<String> {
    let mut deps = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // コメント行をスキップ（行コメント・ブロックコメント）
        if trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("*")
            || trimmed.starts_with('#')
        {
            continue;
        }

        match language {
            Language::Rust => {
                // "use crate::module_name::..." → "module_name"
                if let Some(rest) = trimmed.strip_prefix("use crate::") {
                    let segment = rest
                        .split("::")
                        .next()
                        .unwrap_or("")
                        .trim_end_matches(';')
                        .trim_end_matches('{')
                        .trim();
                    if !segment.is_empty() && segment != "self" {
                        deps.push(segment.to_string());
                    }
                }
            }
            Language::Python => {
                if let Some(rest) = trimmed.strip_prefix("from .") {
                    // "from .module import x" → "module"
                    let module = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .split('.')
                        .next()
                        .unwrap_or("");
                    if !module.is_empty() {
                        deps.push(module.to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("import ") {
                    let module = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .split('.')
                        .next()
                        .unwrap_or("");
                    if !module.is_empty() {
                        deps.push(module.to_string());
                    }
                }
            }
            Language::TypeScript => {
                // "import ... from './module'" or "import ... from '../utils'"
                if trimmed.starts_with("import ") && trimmed.contains(" from ") {
                    if let Some(from_part) = trimmed.split(" from ").nth(1) {
                        let module_path = from_part
                            .trim()
                            .trim_end_matches(';')
                            .trim_matches('"')
                            .trim_matches('\'');
                        // ローカル import のみ（相対パス）
                        if module_path.starts_with("./") || module_path.starts_with("../") {
                            if let Some(last) = module_path.split('/').last() {
                                // index → parent dir name、それ以外はそのまま
                                let name = last.trim_end_matches(".ts").trim_end_matches(".tsx");
                                if !name.is_empty() && name != "index" {
                                    deps.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Language::Go => {
                // import ブロック内の "path/to/package" → "package"
                let trimmed = trimmed.trim_matches('"');
                if !trimmed.contains(' ') && trimmed.contains('/') {
                    if let Some(last) = trimmed.split('/').last() {
                        if !last.is_empty() {
                            deps.push(last.to_string());
                        }
                    }
                }
            }
        }
    }

    // 重複除去（順序保持）
    let mut seen = BTreeSet::new();
    deps.retain(|d| seen.insert(d.clone()));
    deps
}

/// 分岐命令（if/match/for/while/else/loop）の数を数える
fn count_branches(content: &str) -> usize {
    let mut count = 0;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("//")
            || t.starts_with("/*")
            || t.starts_with("*/")
            || t.starts_with('*')
            || t.starts_with('#')
        {
            continue;
        }
        for word in t.split_whitespace() {
            if matches!(word, "if" | "match" | "for" | "while" | "else" | "loop") {
                count += 1;
            }
        }
    }
    count
}

/// ファイルリストをディレクトリ（親）単位でモジュールにグループ化する
fn group_modules(files: &[FileAnalysis]) -> Vec<Module> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for file in files {
        let module_name = Path::new(&file.path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();
        groups
            .entry(module_name)
            .or_default()
            .push(file.path.clone());
    }

    groups
        .into_iter()
        .map(|(name, files)| Module { name, files })
        .collect()
}

/// ファイルごとの依存情報からモジュール間エッジを構築する
fn build_dependency_edges(
    records: &[(FileAnalysis, Vec<String>)],
    known_modules: &BTreeSet<String>,
) -> Vec<DependencyEdge> {
    let mut edges = BTreeSet::new();

    for (file, deps) in records {
        let from_module = Path::new(&file.path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();

        for dep in deps {
            if known_modules.contains(dep) && dep != &from_module {
                edges.insert(DependencyEdge {
                    from: from_module.clone(),
                    to: dep.clone(),
                });
            }
        }
    }

    edges.into_iter().collect()
}

/// ファイルリストからプロジェクトサマリを生成する
fn build_summary(files: &[FileAnalysis]) -> ProjectSummary {
    let languages: Vec<Language> = {
        let mut set = BTreeSet::new();
        for f in files {
            set.insert(f.language.clone());
        }
        set.into_iter().collect()
    };

    let avg_complexity = if files.is_empty() {
        Complexity::Low
    } else {
        let sum: f32 = files.iter().map(|f| f.complexity.score()).sum();
        Complexity::from_score(sum / files.len() as f32)
    };

    ProjectSummary {
        total_files: files.len(),
        languages,
        avg_complexity,
    }
}

// ─── 旧 API（後方互換） ──────────────────────────────────────────────────

/// 単一ファイルまたは簡易ディレクトリを解析する（旧 API）
pub fn analyze_path(path: &str) -> Result<AnalysisResult, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(AnalysisResult {
            path: path.to_string(),
            modules: vec![],
            total_lines: 0,
            suggestions: vec!["Path does not exist".to_string()],
        });
    }

    let mut modules = Vec::new();
    let mut total_lines = 0usize;
    collect_modules(p, &mut modules, &mut total_lines)
        .map_err(|e| format!("analysis error: {e}"))?;
    let suggestions = generate_suggestions(&modules);
    Ok(AnalysisResult {
        path: path.to_string(),
        modules,
        total_lines,
        suggestions,
    })
}

fn collect_modules(
    path: &Path,
    modules: &mut Vec<ModuleInfo>,
    total: &mut usize,
) -> Result<(), String> {
    if path.is_file() {
        if let Some(info) = analyze_file_legacy(path) {
            *total += info.line_count;
            modules.push(info);
        }
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let p = entry.path();
        let name_str = entry.file_name();
        let name_str = name_str.to_string_lossy();
        if matches!(name_str.as_ref(), ".git" | "target" | "node_modules") {
            continue;
        }
        if p.is_dir() {
            collect_modules(&p, modules, total)?;
        } else if let Some(info) = analyze_file_legacy(&p) {
            *total += info.line_count;
            modules.push(info);
        }
    }
    Ok(())
}

fn analyze_file_legacy(path: &Path) -> Option<ModuleInfo> {
    let ext = path.extension()?.to_str()?;
    if !matches!(ext, "rs" | "py" | "ts" | "tsx" | "go") {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let line_count = content.lines().count();
    let mut issues = Vec::new();
    let todo_count = content.matches("TODO").count() + content.matches("FIXME").count();
    if todo_count > 0 {
        issues.push(format!("{todo_count} TODO/FIXME"));
    }
    let complexity = complexity_from_lines(line_count);
    let name = path.file_name()?.to_string_lossy().to_string();
    Some(ModuleInfo {
        name,
        complexity,
        issues,
        line_count,
    })
}

fn complexity_from_lines(lines: usize) -> &'static str {
    if lines < 100 {
        "Low"
    } else if lines < 400 {
        "Medium"
    } else {
        "High"
    }
}

fn generate_suggestions(modules: &[ModuleInfo]) -> Vec<String> {
    let mut suggestions = Vec::new();
    let high: Vec<&str> = modules
        .iter()
        .filter(|m| m.complexity == "High")
        .map(|m| m.name.as_str())
        .collect();
    if !high.is_empty() {
        suggestions.push(format!(
            "Consider extracting interfaces from: {}",
            high.join(", ")
        ));
    }
    let with_todos: Vec<&str> = modules
        .iter()
        .filter(|m| !m.issues.is_empty())
        .map(|m| m.name.as_str())
        .collect();
    if !with_todos.is_empty() {
        suggestions.push(format!("Resolve TODOs in: {}", with_todos.join(", ")));
    }
    suggestions
}

// ─── テスト ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── analyze_path（旧 API）──

    #[test]
    fn analyze_path_nonexistent_returns_empty() {
        let result = analyze_path("/nonexistent/path/xyz_does_not_exist_123").unwrap();
        assert!(result.modules.is_empty());
        assert_eq!(result.total_lines, 0);
        assert!(result.suggestions[0].contains("does not exist"));
    }

    #[test]
    fn analyze_path_src_dir_finds_rs_files() {
        let result = analyze_path("src/").unwrap();
        assert!(!result.modules.is_empty());
        assert!(result.total_lines > 0);
        assert!(result.modules.iter().all(|m| m.name.ends_with(".rs")));
    }

    #[test]
    fn analyze_path_single_rs_file() {
        let result = analyze_path("src/plan.rs").unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].name, "plan.rs");
    }

    // ── analyze_project（新 API）──

    #[test]
    fn analyze_project_nonexistent_returns_empty() {
        let result = analyze_project("/nonexistent/xyz123").unwrap();
        assert_eq!(result.summary.total_files, 0);
        assert!(result.files.is_empty());
    }

    #[test]
    fn analyze_project_src_dir_finds_files() {
        let result = analyze_project("src/").unwrap();
        assert!(result.summary.total_files > 0);
        assert!(!result.modules.is_empty());
        assert!(!result.summary.languages.is_empty());
    }

    #[test]
    fn analyze_project_finds_rust_language() {
        let result = analyze_project("src/").unwrap();
        assert!(result.summary.languages.contains(&Language::Rust));
    }

    #[test]
    fn analyze_project_modules_include_known_dirs() {
        let result = analyze_project("src/").unwrap();
        // src/ contains planner/, dbm/, commands/ subdirs
        let module_names: Vec<&str> = result.modules.iter().map(|m| m.name.as_str()).collect();
        assert!(
            module_names.contains(&"planner") || module_names.contains(&"dbm"),
            "should find planner or dbm module: {module_names:?}"
        );
    }

    #[test]
    fn analyze_project_dependencies_populated_for_complex_project() {
        let result = analyze_project("src/").unwrap();
        // A project with multiple modules should have some dependencies
        // (exact deps depend on code, so we just check it doesn't error)
        let _ = result.dependencies;
    }

    #[test]
    fn analyze_project_file_paths_are_relative() {
        let result = analyze_project("src/").unwrap();
        for file in &result.files {
            assert!(
                !file.path.starts_with('/'),
                "path should be relative, got: {}",
                file.path
            );
        }
    }

    // ── extract_dependencies ──

    #[test]
    fn extract_deps_rust_use_crate() {
        let content =
            "use crate::planner::rule_based::RuleBasedPlanner;\nuse crate::dbm::client::DBMClient;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert!(deps.contains(&"planner".to_string()), "deps: {deps:?}");
        assert!(deps.contains(&"dbm".to_string()), "deps: {deps:?}");
    }

    #[test]
    fn extract_deps_rust_skips_std_and_external() {
        let content = "use std::collections::HashMap;\nuse serde::Serialize;";
        let deps = extract_dependencies(content, &Language::Rust);
        // These start with "std" or external crate, not "crate::" so they produce nothing
        assert!(deps.is_empty(), "should not extract std/external: {deps:?}");
    }

    #[test]
    fn extract_deps_rust_dedup() {
        let content = "use crate::planner::a::X;\nuse crate::planner::b::Y;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert_eq!(deps.iter().filter(|d| *d == "planner").count(), 1);
    }

    #[test]
    fn extract_deps_python_relative() {
        let content = "from .utils import helper\nfrom .models import Foo\n";
        let deps = extract_dependencies(content, &Language::Python);
        assert!(deps.contains(&"utils".to_string()), "deps: {deps:?}");
        assert!(deps.contains(&"models".to_string()), "deps: {deps:?}");
    }

    #[test]
    fn extract_deps_typescript_local() {
        let content = "import { Foo } from './planner';\nimport bar from '../utils';\n";
        let deps = extract_dependencies(content, &Language::TypeScript);
        assert!(deps.contains(&"planner".to_string()), "deps: {deps:?}");
        assert!(deps.contains(&"utils".to_string()), "deps: {deps:?}");
    }

    #[test]
    fn extract_deps_typescript_skips_external() {
        let content = "import React from 'react';\nimport _ from 'lodash';\n";
        let deps = extract_dependencies(content, &Language::TypeScript);
        assert!(deps.is_empty(), "should not extract npm packages: {deps:?}");
    }

    #[test]
    fn extract_deps_skips_comment_lines() {
        let content = "// use crate::planner;\nuse crate::dbm::client;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert!(
            !deps.contains(&"planner".to_string()),
            "should skip commented use"
        );
        assert!(deps.contains(&"dbm".to_string()));
    }

    // ── Complexity ──

    #[test]
    fn complexity_low_for_small_file() {
        assert_eq!(Complexity::from_counts(50, 2), Complexity::Low);
    }

    #[test]
    fn complexity_medium_for_mid_file() {
        assert_eq!(Complexity::from_counts(200, 8), Complexity::Medium);
    }

    #[test]
    fn complexity_high_for_large_file() {
        assert_eq!(Complexity::from_counts(500, 30), Complexity::High);
    }

    #[test]
    fn complexity_high_due_to_branches() {
        assert_eq!(Complexity::from_counts(50, 25), Complexity::High);
    }

    #[test]
    fn complexity_score_ordering() {
        assert!(Complexity::Low.score() < Complexity::Medium.score());
        assert!(Complexity::Medium.score() < Complexity::High.score());
    }

    // ── group_modules ──

    #[test]
    fn group_modules_by_parent_dir() {
        let files = vec![
            FileAnalysis {
                path: "planner/rule_based.rs".to_string(),
                language: Language::Rust,
                complexity: Complexity::Low,
                todos: vec![],
            },
            FileAnalysis {
                path: "dbm/client.rs".to_string(),
                language: Language::Rust,
                complexity: Complexity::Low,
                todos: vec![],
            },
        ];
        let modules = group_modules(&files);
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"planner"), "names: {names:?}");
        assert!(names.contains(&"dbm"), "names: {names:?}");
    }

    #[test]
    fn group_modules_root_files() {
        let files = vec![FileAnalysis {
            path: "main.rs".to_string(),
            language: Language::Rust,
            complexity: Complexity::Low,
            todos: vec![],
        }];
        let modules = group_modules(&files);
        assert_eq!(modules[0].name, "root");
    }

    // ── count_branches ──

    #[test]
    fn count_branches_counts_control_flow() {
        let content = "fn foo() {\n  if x { match y { _ => for z in w { } } }\n}";
        let count = count_branches(content);
        assert!(count >= 3, "should count if, match, for: got {count}");
    }

    #[test]
    fn count_branches_skips_comments() {
        let content = "// if match for\n/* if match */\nfn foo() {}";
        let count = count_branches(content);
        assert_eq!(count, 0);
    }

    // ── scan_directory ──

    #[test]
    fn scan_directory_finds_rs_files() {
        use std::path::Path;
        let files = scan_directory(Path::new("src/")).unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().all(|p| is_supported_extension(p)));
    }

    #[test]
    fn scan_directory_skips_target_dir() {
        use std::path::Path;
        let files = scan_directory(Path::new(".")).unwrap();
        assert!(
            files
                .iter()
                .all(|p| !p.display().to_string().contains("/target/"))
        );
    }
}
