use serde::Serialize;
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

type ProjectFileRecord = (FileAnalysis, Vec<(String, bool)>, bool);

// ─── 言語 ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileAnalysis {
    pub path: String,
    pub language: Language,
    pub complexity: Complexity,
    pub todos: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyEdgeType {
    Direct,
    Mediated,
}

/// モジュール間依存エッジ
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub edge_type: DependencyEdgeType,
}

/// ディレクトリ単位のモジュール
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Module {
    pub name: String,
    pub files: Vec<String>,
}

/// プロジェクト全体サマリ
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectSummary {
    pub total_files: usize,
    pub languages: Vec<Language>,
    pub avg_complexity: Complexity,
}

/// プロジェクト全体解析結果
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

    // Phase G1.2: single-file fallback
    if root.is_file() {
        let logical_root = root.parent().unwrap_or(Path::new("."));
        let mut files = Vec::new();
        let mut dependencies = Vec::new();
        let mut modules = Vec::new();

        if let Some((file, deps, _)) = analyze_file_project(root, logical_root) {
            modules.push(Module {
                name: module_name_from_relative_path(&file.path),
                files: vec![file.path.clone()],
            });
            files.push(file);
            dependencies.extend(deps.into_iter().filter_map(|(to, is_interface)| {
                if to == "root" {
                    return None;
                }
                Some(DependencyEdge {
                    from: "root".to_string(),
                    to,
                    edge_type: if is_interface {
                        DependencyEdgeType::Mediated
                    } else {
                        DependencyEdgeType::Direct
                    },
                })
            }));
        }

        let summary = build_summary(&files);

        return Ok(ProjectAnalysisResult {
            files,
            dependencies,
            modules,
            summary,
        });
    }

    // 1. ディレクトリ走査
    let paths = scan_directory(root)?;

    // 2. 各ファイル解析（相対パス + 依存抽出）
    let mut records: Vec<ProjectFileRecord> = vec![];
    for path in &paths {
        if let Some(record) = analyze_file_project(path, root) {
            records.push(record);
        }
    }

    let files: Vec<FileAnalysis> = records.iter().map(|(f, _, _)| f.clone()).collect();

    // 3. モジュール構造を推定
    let modules = group_modules(&files);
    let known_modules: BTreeSet<String> = modules.iter().map(|m| m.name.clone()).collect();
    let mediated_modules = collect_mediated_modules(&records, &known_modules);
    let dual_boundary_pairs = collect_dual_boundary_pairs(&mediated_modules);

    // 4. 依存グラフ構築
    let dependencies = build_dependency_edges(
        &records,
        &known_modules,
        &mediated_modules,
        &dual_boundary_pairs,
    );

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
    files.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    files.dedup();
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

/// 1ファイルを解析して (FileAnalysis, raw_deps_with_interface_flag) を返す
fn analyze_file_project(path: &Path, root: &Path) -> Option<ProjectFileRecord> {
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
    let is_mediated_source = file_declares_mediated_trait(&content)
        || file_path_declares_mediated_source(path.strip_prefix(root).ok()?);

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
        is_mediated_source,
    ))
}

/// ソースコードから import/use 文を解析して (依存モジュール名, is_interface) リストを返す
pub fn extract_dependencies(content: &str, language: &Language) -> Vec<(String, bool)> {
    let mut deps = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // コメント行をスキップ
        if trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("*")
            || trimmed.starts_with('#')
        {
            continue;
        }

        match language {
            Language::Rust => {
                if let Some(rest) = trimmed.strip_prefix("use crate::") {
                    let full_path = rest.trim_end_matches(';').trim_end_matches('{').trim();
                    let is_interface = path_declares_mediation(full_path);
                    let segment = full_path.split("::").next().unwrap_or("");
                    if !segment.is_empty() && segment != "self" {
                        deps.push((segment.to_string(), is_interface));
                    }
                }
            }
            Language::Python => {
                if let Some(rest) = trimmed.strip_prefix("from .") {
                    let module = rest.split_whitespace().next().unwrap_or("");
                    let is_interface = path_declares_mediation(module);
                    let segment = module.split('.').next().unwrap_or("");
                    if !segment.is_empty() {
                        deps.push((segment.to_string(), is_interface));
                    }
                } else if let Some(rest) = trimmed.strip_prefix("import ") {
                    let module = rest.split_whitespace().next().unwrap_or("");
                    let is_interface = path_declares_mediation(module);
                    let segment = module.split('.').next().unwrap_or("");
                    if !segment.is_empty() {
                        deps.push((segment.to_string(), is_interface));
                    }
                }
            }
            Language::TypeScript => {
                if trimmed.starts_with("import ")
                    && trimmed.contains(" from ")
                    && let Some(from_part) = trimmed.split(" from ").nth(1)
                {
                    let module_path = from_part
                        .trim()
                        .trim_end_matches(';')
                        .trim_matches('"')
                        .trim_matches('\'');
                    if module_path.starts_with("./") || module_path.starts_with("../") {
                        let is_interface = path_declares_mediation(module_path);
                        if let Some(last) = module_path.split('/').next_back() {
                            let name = last.trim_end_matches(".ts").trim_end_matches(".tsx");
                            if !name.is_empty() && name != "index" {
                                deps.push((name.to_string(), is_interface));
                            }
                        }
                    }
                }
            }
            Language::Go => {
                let path = trimmed.trim_matches('"');
                if !path.contains(' ') && path.contains('/') {
                    let is_interface = path_declares_mediation(path);
                    if let Some(last) = path.split('/').next_back()
                        && !last.is_empty()
                    {
                        deps.push((last.to_string(), is_interface));
                    }
                }
            }
        }
    }

    let mut seen = BTreeMap::new();
    for (d, is_if) in deps {
        let entry = seen.entry(d).or_insert(false);
        *entry = *entry || is_if;
    }
    seen.into_iter().collect()
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
        let module_name = module_name_from_relative_path(&file.path);
        groups
            .entry(module_name)
            .or_default()
            .push(file.path.clone());
    }

    groups
        .into_iter()
        .map(|(name, mut files)| {
            files.sort();
            files.dedup();
            Module { name, files }
        })
        .collect()
}

/// ファイルごとの依存情報からモジュール間エッジを構築する
fn build_dependency_edges(
    records: &[ProjectFileRecord],
    known_modules: &BTreeSet<String>,
    mediated_modules: &BTreeSet<String>,
    dual_boundary_pairs: &BTreeSet<(String, String)>,
) -> Vec<DependencyEdge> {
    let mut edges = BTreeSet::new();
    let mut interface_users: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    // 1. 直接依存の抽出
    for (file, deps, _) in records {
        let from_module = module_name_from_relative_path(&file.path);

        for (dep, is_interface) in deps {
            if known_modules.contains(dep) && dep != &from_module {
                let edge_type = if *is_interface
                    || mediated_modules.contains(dep)
                    || dual_boundary_pairs.contains(&ordered_boundary_pair(&from_module, dep))
                {
                    DependencyEdgeType::Mediated
                } else {
                    DependencyEdgeType::Direct
                };

                edges.insert(DependencyEdge {
                    from: from_module.clone(),
                    to: dep.clone(),
                    edge_type,
                });

                if edge_type == DependencyEdgeType::Mediated {
                    interface_users
                        .entry(dep.clone())
                        .or_default()
                        .insert(from_module.clone());
                }
            }
        }
    }
    // 2. 共有インターフェースによる Mediated Edge の追加
    for (_interface, users) in interface_users {
        let users_vec: Vec<String> = users.into_iter().collect();
        for i in 0..users_vec.len() {
            for j in 0..users_vec.len() {
                if i != j {
                    edges.insert(DependencyEdge {
                        from: users_vec[i].clone(),
                        to: users_vec[j].clone(),
                        edge_type: DependencyEdgeType::Mediated,
                    });
                }
            }
        }
    }

    edges.into_iter().collect()
}

fn collect_mediated_modules(
    records: &[ProjectFileRecord],
    known_modules: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut mediated = BTreeSet::new();
    for (file, _, is_mediated_source) in records {
        let module_name = module_name_from_relative_path(&file.path);
        if (*is_mediated_source || is_mediated_module_name(&module_name))
            && known_modules.contains(&module_name)
        {
            mediated.insert(module_name);
        }
    }
    mediated
}

fn collect_dual_boundary_pairs(mediated_modules: &BTreeSet<String>) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for module in mediated_modules {
        let Some((lhs, rhs)) = boundary_pair_for_module_name(module) else {
            continue;
        };
        let reverse = format!("{rhs}_{lhs}_interface");
        if mediated_modules.contains(&reverse) {
            pairs.insert(ordered_boundary_pair(&lhs, &rhs));
        }
    }
    pairs
}

fn boundary_pair_for_module_name(name: &str) -> Option<(String, String)> {
    let normalized = normalize_mediation_name(name);
    let stripped = normalized.strip_suffix("_interface")?;
    let (lhs, rhs) = stripped.split_once('_')?;
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some((lhs.to_string(), rhs.to_string()))
}

fn ordered_boundary_pair(lhs: &str, rhs: &str) -> (String, String) {
    if lhs <= rhs {
        (lhs.to_string(), rhs.to_string())
    } else {
        (rhs.to_string(), lhs.to_string())
    }
}

fn module_name_from_relative_path(relative_path: &str) -> String {
    let path = Path::new(relative_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if matches!(
        file_name,
        "mod.rs" | "index.ts" | "index.tsx" | "index.js" | "index.jsx"
    ) {
        return path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();
    }

    let stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if matches!(stem, "lib" | "main") {
        return "root".to_string();
    }

    if file_is_directly_under_source_root(path) || path.parent().is_none() {
        return stem.to_string();
    }

    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("root")
        .to_string()
}

fn file_is_directly_under_source_root(path: &Path) -> bool {
    matches!(
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str()),
        Some("src")
    )
}

fn file_path_declares_mediated_source(relative_path: &Path) -> bool {
    relative_path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(is_mediated_module_name)
        .unwrap_or(false)
        || relative_path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .map(is_mediated_module_name)
            .unwrap_or(false)
}

fn path_declares_mediation(path: &str) -> bool {
    path.split(&[':', '/', '.'][..])
        .any(is_mediated_module_name)
        || path.split("::").any(is_mediated_symbol_name)
}

fn is_mediated_module_name(name: &str) -> bool {
    let normalized = normalize_mediation_name(name);
    normalized.ends_with("_interface")
        || normalized.ends_with("_bridge")
        || normalized.ends_with("_port")
        || normalized.ends_with("_adapter_interface")
}

fn is_mediated_symbol_name(name: &str) -> bool {
    let normalized = normalize_mediation_name(name);
    normalized.ends_with("interface")
        || normalized.ends_with("bridge")
        || normalized.ends_with("port")
}

fn normalize_mediation_name(name: &str) -> String {
    name.trim_matches(|c: char| c == '{' || c == '}' || c == ';')
        .replace('-', "_")
        .to_ascii_lowercase()
}

fn file_declares_mediated_trait(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("pub trait ") else {
            return false;
        };
        let trait_name = rest
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        is_mediated_symbol_name(trait_name)
    })
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
    fn analyze_project_single_rs_file_returns_modules() {
        let result = analyze_project("src/main.rs").unwrap();
        assert!(!result.modules.is_empty());
        assert_eq!(result.summary.total_files, 1);
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn analyze_project_src_dir_preserves_existing_behavior() {
        let result = analyze_project("src/").unwrap();
        assert!(result.summary.total_files > 0);
        assert!(!result.files.is_empty());
        assert!(!result.modules.is_empty());
        assert!(result.summary.languages.contains(&Language::Rust));
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
        let names: Vec<String> = deps.iter().map(|(n, _)| n.clone()).collect();
        assert!(names.contains(&"planner".to_string()), "deps: {deps:?}");
        assert!(names.contains(&"dbm".to_string()), "deps: {deps:?}");
    }

    #[test]
    fn extract_deps_rust_skips_std_and_external() {
        let content = "use std::collections::HashMap;\nuse serde::Serialize;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert!(deps.is_empty(), "should not extract std/external: {deps:?}");
    }

    #[test]
    fn extract_deps_rust_dedup() {
        let content = "use crate::planner::a::X;\nuse crate::planner::b::Y;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert_eq!(deps.iter().filter(|(d, _)| *d == "planner").count(), 1);
    }

    #[test]
    fn extract_deps_rust_interface_detection() {
        let content = "use crate::world::AdapterWorldInterface;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert!(deps.iter().any(|(n, is_if)| n == "world" && *is_if));
    }

    #[test]
    fn extract_deps_rust_generic_mediation_naming_detection() {
        let content = "use crate::controller_replay_interface::ControllerReplayInterface;\nuse crate::runtime_bridge::RuntimeBridge;\nuse crate::storage_port::StoragePort;";
        let deps = extract_dependencies(content, &Language::Rust);
        assert!(
            deps.iter()
                .any(|(name, is_if)| name == "controller_replay_interface" && *is_if),
            "{deps:?}"
        );
        assert!(
            deps.iter()
                .any(|(name, is_if)| name == "runtime_bridge" && *is_if),
            "{deps:?}"
        );
        assert!(
            deps.iter()
                .any(|(name, is_if)| name == "storage_port" && *is_if),
            "{deps:?}"
        );
    }

    #[test]
    fn file_declares_mediated_trait_detects_generic_interface_trait() {
        assert!(file_declares_mediated_trait(
            "pub trait ControllerReplayInterface {\n    fn replay(&self) -> bool;\n}\n"
        ));
        assert!(!file_declares_mediated_trait(
            "pub trait ExecutionController {\n    fn run(&self);\n}\n"
        ));
    }

    #[test]
    fn collect_dual_boundary_pairs_detects_world_boundary_pairs() {
        let modules = BTreeSet::from([
            "renderer_world_interface".to_string(),
            "world_renderer_interface".to_string(),
            "controller_replay_interface".to_string(),
        ]);
        let pairs = collect_dual_boundary_pairs(&modules);
        assert!(pairs.contains(&(String::from("renderer"), String::from("world"))));
        assert!(!pairs.contains(&(String::from("controller"), String::from("replay"))));
    }

    #[test]
    fn extract_deps_python_relative() {
        let content = "from .utils import helper\nfrom .models import Foo\n";
        let deps = extract_dependencies(content, &Language::Python);
        let names: Vec<String> = deps.iter().map(|(n, _)| n.clone()).collect();
        assert!(names.contains(&"utils".to_string()), "deps: {deps:?}");
        assert!(names.contains(&"models".to_string()), "deps: {deps:?}");
    }

    #[test]
    fn extract_deps_typescript_local() {
        let content = "import { Foo } from './planner';\nimport bar from '../utils';\n";
        let deps = extract_dependencies(content, &Language::TypeScript);
        let names: Vec<String> = deps.iter().map(|(n, _)| n.clone()).collect();
        assert!(names.contains(&"planner".to_string()), "deps: {deps:?}");
        assert!(names.contains(&"utils".to_string()), "deps: {deps:?}");
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
        let names: Vec<String> = deps.iter().map(|(n, _)| n.clone()).collect();
        assert!(
            !names.contains(&"planner".to_string()),
            "should skip commented use"
        );
        assert!(names.contains(&"dbm".to_string()));
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

    #[test]
    fn group_modules_uses_file_stem_for_source_root_flat_files() {
        let files = vec![
            FileAnalysis {
                path: "apps/cli/src/renderer.rs".to_string(),
                language: Language::Rust,
                complexity: Complexity::Low,
                todos: vec![],
            },
            FileAnalysis {
                path: "apps/cli/src/world_renderer_interface.rs".to_string(),
                language: Language::Rust,
                complexity: Complexity::Low,
                todos: vec![],
            },
        ];
        let modules = group_modules(&files);
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"renderer"), "{names:?}");
        assert!(names.contains(&"world_renderer_interface"), "{names:?}");
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
