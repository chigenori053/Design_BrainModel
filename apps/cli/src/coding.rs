use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use integration_layer::{CodePatch, PatchOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    CreateFile,
    ModifyFile,
    MoveFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub start_line: usize,
    pub end_line: usize,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChange {
    pub file_path: String,
    pub change_type: ChangeType,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChangeSummary {
    pub total_changes: usize,
    pub create_files: usize,
    pub modify_files: usize,
    pub move_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChangeSet {
    pub changes: Vec<CodeChange>,
    pub summary: ChangeSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    CreateInterface {
        name: String,
        between: (String, String),
    },
    ReplaceDependency {
        from: String,
        to: String,
        via: Option<String>,
    },
    SplitModule {
        module: String,
        targets: Vec<String>,
    },
    ExtractComponent {
        from: String,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodingOptions {
    pub apply: bool,
    pub check: bool,
    pub no_build: bool,
    pub backup: bool,
    pub format: bool,
    pub safe_mode: bool,
    pub auto_commit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodingExecutionResult {
    pub status: String,
    pub applied: bool,
    pub checked: bool,
    pub build_fixed: bool,
    pub build_ok: bool,
    pub rolled_back: bool,
    pub backed_up: bool,
    pub reason: Option<String>,
    pub sandbox_root: Option<String>,
    pub files_changed: usize,
    pub diff: DiffReport,
    pub committed: bool,
    pub commit_id: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
struct FileDraft {
    change_type: ChangeType,
    original: String,
    content: String,
}

#[derive(Debug, Clone)]
struct BackupEntry {
    path: PathBuf,
    original: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FixResult {
    pub build_fixed: bool,
    pub registered_modules: Vec<String>,
    pub created_placeholders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Add,
    Modify,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ASTDiff {
    pub kind: DiffKind,
    pub target: String,
    pub breaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiffReport {
    pub diffs: Vec<ASTDiff>,
    pub breaking_count: usize,
}

pub fn load_patches_from_json(path: &Path) -> Result<Vec<CodePatch>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    if value.is_array() {
        return serde_json::from_value(value).map_err(|err| err.to_string());
    }
    if let Some(patches) = value.get("patches") {
        return serde_json::from_value(patches.clone()).map_err(|err| err.to_string());
    }
    Err("input JSON does not contain patches".to_string())
}

pub fn patches_to_edits(patches: &[CodePatch]) -> Vec<Edit> {
    let mut edits = Vec::new();
    for patch in patches {
        for operation in &patch.operations {
            match operation {
                PatchOperation::CreateInterface { name, between } => {
                    edits.push(Edit::CreateInterface {
                        name: name.clone(),
                        between: between.clone(),
                    })
                }
                PatchOperation::UpdateDependency { from, to, via } => {
                    edits.push(Edit::ReplaceDependency {
                        from: from.clone(),
                        to: to.clone(),
                        via: via.clone(),
                    })
                }
                PatchOperation::SplitModule {
                    module,
                    new_modules,
                } => edits.push(Edit::SplitModule {
                    module: module.clone(),
                    targets: new_modules.clone(),
                }),
                PatchOperation::ExtractComponent { from, component } => {
                    edits.push(Edit::ExtractComponent {
                        from: from.clone(),
                        name: component.clone(),
                    })
                }
            }
        }
    }
    edits
}

pub fn generate_code_change_set(
    root: &Path,
    patches: &[CodePatch],
) -> Result<CodeChangeSet, String> {
    let mut drafts = BTreeMap::<String, FileDraft>::new();
    for edit in patches_to_edits(patches) {
        apply_edit(root, &mut drafts, edit)?;
    }

    let mut changes = drafts
        .into_iter()
        .map(|(file_path, draft)| CodeChange {
            file_path,
            change_type: draft.change_type.clone(),
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line: draft.original.lines().count().max(1),
                replacement: draft.content,
            }],
        })
        .collect::<Vec<_>>();
    changes.sort_by(|lhs, rhs| lhs.file_path.cmp(&rhs.file_path));

    let summary = changes
        .iter()
        .fold(ChangeSummary::default(), |mut summary, change| {
            summary.total_changes += 1;
            match change.change_type {
                ChangeType::CreateFile => summary.create_files += 1,
                ChangeType::ModifyFile => summary.modify_files += 1,
                ChangeType::MoveFile => summary.move_files += 1,
            }
            summary
        });

    Ok(CodeChangeSet { changes, summary })
}

pub fn execute_code_change_set(
    root: &Path,
    change_set: &CodeChangeSet,
    options: &CodingOptions,
) -> Result<CodingExecutionResult, String> {
    let diff = compute_diff_report(root, change_set)?;
    if options.safe_mode {
        validate_diff_report(&diff)?;
    }

    let checked = options.check || options.apply;
    let backed_up = options.apply || options.backup;
    let sandbox_root = if checked {
        Some(create_sandbox_workspace(root)?)
    } else {
        None
    };

    let mut build_ok = !checked || options.no_build;
    let mut build_fixed = false;
    let mut reason = None;

    if let Some(sandbox_root) = sandbox_root.as_ref() {
        apply_code_change_set(sandbox_root, change_set)?;
        if !options.no_build {
            let fix_result = fix_build(sandbox_root)?;
            build_fixed = fix_result.build_fixed;
            match run_build_validation(sandbox_root) {
                Ok(()) => build_ok = true,
                Err(err) => {
                    build_ok = false;
                    reason = Some(err);
                }
            }
        }
    }

    if let Some(sandbox_root) = sandbox_root.as_ref() {
        let _ = fs::remove_dir_all(sandbox_root);
    }

    if !build_ok {
        return Ok(CodingExecutionResult {
            status: "failed".to_string(),
            applied: false,
            checked,
            build_fixed,
            build_ok: false,
            rolled_back: true,
            backed_up,
            reason,
            sandbox_root: sandbox_root.as_ref().map(|path| path.display().to_string()),
            files_changed: 0,
            diff,
            committed: false,
            commit_id: None,
            branch: None,
        });
    }

    if !options.apply {
        return Ok(CodingExecutionResult {
            status: if checked {
                "checked".to_string()
            } else {
                "dry-run".to_string()
            },
            applied: false,
            checked,
            build_fixed,
            build_ok,
            rolled_back: false,
            backed_up: false,
            reason: None,
            sandbox_root: sandbox_root.as_ref().map(|path| path.display().to_string()),
            files_changed: 0,
            diff,
            committed: false,
            commit_id: None,
            branch: None,
        });
    }

    let backups = snapshot_workspace(root, change_set)?;
    match apply_code_change_set(root, change_set)
        .and_then(|_| {
            if options.no_build {
                Ok(FixResult::default())
            } else {
                fix_build(root)
            }
        })
        .and_then(|fix_result| {
            build_fixed |= fix_result.build_fixed;
            if options.no_build {
                Ok(())
            } else {
                run_build_validation(root)
            }
        }) {
        Ok(()) => {
            let (committed, commit_id, branch, reason) = if options.auto_commit {
                match git_commit(root) {
                    Ok((commit_id, branch)) => (true, Some(commit_id), Some(branch), None),
                    Err(err) => {
                        restore_workspace(backups)?;
                        return Ok(CodingExecutionResult {
                            status: "failed".to_string(),
                            applied: false,
                            checked,
                            build_fixed,
                            build_ok,
                            rolled_back: true,
                            backed_up,
                            reason: Some(err),
                            sandbox_root: sandbox_root
                                .as_ref()
                                .map(|path| path.display().to_string()),
                            files_changed: 0,
                            diff,
                            committed: false,
                            commit_id: None,
                            branch: None,
                        });
                    }
                }
            } else {
                (false, None, None, None)
            };

            Ok(CodingExecutionResult {
                status: "applied".to_string(),
                applied: true,
                checked,
                build_fixed,
                build_ok,
                rolled_back: false,
                backed_up,
                reason,
                sandbox_root: sandbox_root.as_ref().map(|path| path.display().to_string()),
                files_changed: change_set.summary.total_changes,
                diff,
                committed,
                commit_id,
                branch,
            })
        }
        Err(err) => {
            restore_workspace(backups)?;
            Ok(CodingExecutionResult {
                status: "failed".to_string(),
                applied: false,
                checked,
                build_fixed,
                build_ok,
                rolled_back: true,
                backed_up,
                reason: Some(err),
                sandbox_root: sandbox_root.as_ref().map(|path| path.display().to_string()),
                files_changed: 0,
                diff,
                committed: false,
                commit_id: None,
                branch: None,
            })
        }
    }
}

pub fn apply_code_change_set(root: &Path, change_set: &CodeChangeSet) -> Result<(), String> {
    for change in &change_set.changes {
        let path = root.join(&change.file_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        let replacement = change
            .hunks
            .last()
            .map(|hunk| hunk.replacement.clone())
            .unwrap_or_default();
        fs::write(&path, replacement)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

pub fn compute_diff_report(root: &Path, change_set: &CodeChangeSet) -> Result<DiffReport, String> {
    let mut diffs = Vec::new();
    for change in &change_set.changes {
        let path = root.join(&change.file_path);
        let original = if path.exists() {
            fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?
        } else {
            String::new()
        };
        let replacement = change
            .hunks
            .last()
            .map(|hunk| hunk.replacement.clone())
            .unwrap_or_default();
        let diff = match change.change_type {
            ChangeType::CreateFile => ASTDiff {
                kind: DiffKind::Add,
                target: change.file_path.clone(),
                breaking: false,
            },
            ChangeType::ModifyFile => ASTDiff {
                kind: DiffKind::Modify,
                target: change.file_path.clone(),
                breaking: has_breaking_public_api_change(&original, &replacement),
            },
            ChangeType::MoveFile => ASTDiff {
                kind: DiffKind::Delete,
                target: change.file_path.clone(),
                breaking: contains_public_api(&original),
            },
        };
        diffs.push(diff);
    }
    diffs.sort_by(|lhs, rhs| lhs.target.cmp(&rhs.target));
    let breaking_count = diffs.iter().filter(|diff| diff.breaking).count();
    Ok(DiffReport {
        diffs,
        breaking_count,
    })
}

pub fn validate_diff_report(diff: &DiffReport) -> Result<(), String> {
    if let Some(breaking) = diff.diffs.iter().find(|entry| entry.breaking) {
        return Err(format!(
            "breaking change detected in {} ({:?})",
            breaking.target, breaking.kind
        ));
    }
    Ok(())
}

fn contains_public_api(content: &str) -> bool {
    !public_api_signatures(content).is_empty()
}

fn has_breaking_public_api_change(original: &str, replacement: &str) -> bool {
    let before = public_api_signatures(original);
    let after = public_api_signatures(replacement);
    before.iter().any(|signature| !after.contains(signature))
}

fn public_api_signatures(content: &str) -> BTreeSet<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("pub fn ")
                || line.starts_with("pub struct ")
                || line.starts_with("pub trait ")
                || line.starts_with("pub enum ")
                || line.starts_with("export function ")
                || line.starts_with("export interface ")
                || line.starts_with("def ")
        })
        .map(ToString::to_string)
        .collect()
}

fn git_commit(root: &Path) -> Result<(String, String), String> {
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return Err(format!("git repository not found in {}", root.display()));
    }

    let branch = current_branch(root)?.unwrap_or_else(|| "dbm/auto-branch".to_string());
    if branch == "HEAD" {
        run_git(root, &["checkout", "-B", "dbm/auto-branch"])?;
    }

    run_git(root, &["add", "."])?;

    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(root)
        .status()
        .map_err(|err| format!("failed to run git diff --cached: {err}"))?;
    if status.success() {
        let head = current_commit(root)?.unwrap_or_default();
        return Ok((
            head,
            current_branch(root)?.unwrap_or_else(|| "dbm/auto-branch".to_string()),
        ));
    }

    run_git(root, &["commit", "-m", "DBM apply"])?;
    let commit_id =
        current_commit(root)?.ok_or_else(|| "failed to resolve commit id".to_string())?;
    let branch = current_branch(root)?.unwrap_or_else(|| "dbm/auto-branch".to_string());
    Ok((commit_id, branch))
}

fn run_git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git {:?}: {err}", args))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn current_branch(root: &Path) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to resolve branch: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

fn current_commit(root: &Path) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to resolve commit: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

pub fn fix_build(project_root: &Path) -> Result<FixResult, String> {
    let mut fix = FixResult::default();
    let missing_import_modules = detect_missing_import_modules(project_root)?;
    for module in &missing_import_modules {
        let path = project_root.join("src").join(format!("{module}.rs"));
        if !path.exists() {
            fs::write(&path, "// TODO: build fix placeholder\n")
                .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
            fix.created_placeholders.push(module.clone());
        }
    }

    let modules = detect_top_level_modules(project_root)?;
    let entry_file = resolve_root_module_file(project_root)?;
    let inserted = insert_mod_declarations(&entry_file, &modules)?;
    fix.registered_modules = inserted;
    fix.build_fixed = !(fix.registered_modules.is_empty() && fix.created_placeholders.is_empty());
    Ok(fix)
}

fn apply_edit(
    root: &Path,
    drafts: &mut BTreeMap<String, FileDraft>,
    edit: Edit,
) -> Result<(), String> {
    match edit {
        Edit::CreateInterface { name, .. } => {
            let file_name = format!("src/{}.rs", snake_case(&name));
            let draft = load_or_create_draft(root, drafts, &file_name, true)?;
            draft.content = format!(
                "pub trait {} {{\n    // TODO: define required methods\n}}\n",
                name
            );
        }
        Edit::ReplaceDependency { from, to, via } => {
            let path = resolve_module_file(root, &from);
            let file_key = normalize_relative(root, &path)?;
            let draft = load_or_create_draft(root, drafts, &file_key, false)?;
            draft.content = update_dependency_content(&draft.content, &to, via.as_deref());
        }
        Edit::SplitModule { targets, .. } => {
            for target in targets {
                let file_name = format!("src/{}.rs", snake_case(&target));
                let draft = load_or_create_draft(root, drafts, &file_name, true)?;
                draft.content =
                    format!("pub struct {} {{\n    // TODO\n}}\n", pascal_case(&target));
            }
        }
        Edit::ExtractComponent { name, .. } => {
            let file_name = format!("src/{}.rs", snake_case(&name));
            let draft = load_or_create_draft(root, drafts, &file_name, true)?;
            draft.content = format!("pub struct {} {{\n    // TODO\n}}\n", pascal_case(&name));
        }
    }
    Ok(())
}

fn load_or_create_draft<'a>(
    root: &Path,
    drafts: &'a mut BTreeMap<String, FileDraft>,
    file_key: &str,
    create: bool,
) -> Result<&'a mut FileDraft, String> {
    if !drafts.contains_key(file_key) {
        let path = root.join(file_key);
        let original = if path.exists() {
            fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?
        } else if create {
            String::new()
        } else {
            return Err(format!("target file does not exist: {}", path.display()));
        };
        drafts.insert(
            file_key.to_string(),
            FileDraft {
                change_type: if path.exists() {
                    ChangeType::ModifyFile
                } else {
                    ChangeType::CreateFile
                },
                content: original.clone(),
                original,
            },
        );
    }
    drafts
        .get_mut(file_key)
        .ok_or_else(|| format!("failed to track draft for {}", file_key))
}

fn create_sandbox_workspace(root: &Path) -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let sandbox_root = std::env::temp_dir().join(format!("dbm_sandbox_{unique}"));
    copy_dir_recursive(root, &sandbox_root)?;
    Ok(sandbox_root)
}

fn detect_top_level_modules(root: &Path) -> Result<Vec<String>, String> {
    let src_dir = root.join("src");
    if !src_dir.exists() {
        return Ok(Vec::new());
    }
    let mut modules = Vec::new();
    let mut entries = fs::read_dir(&src_dir)
        .map_err(|err| format!("failed to read {}: {err}", src_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", src_dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            if !matches!(stem, "lib" | "main") {
                modules.push(stem.to_string());
            }
        }
    }
    modules.sort();
    modules.dedup();
    Ok(modules)
}

fn detect_missing_import_modules(root: &Path) -> Result<Vec<String>, String> {
    let src_dir = root.join("src");
    if !src_dir.exists() {
        return Ok(Vec::new());
    }
    let mut modules = Vec::new();
    collect_missing_import_modules(root, &src_dir, &mut modules)?;
    modules.sort();
    modules.dedup();
    Ok(modules)
}

fn collect_missing_import_modules(
    root: &Path,
    dir: &Path,
    modules: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_missing_import_modules(root, &path, modules)?;
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        for line in content.lines() {
            let trimmed = line.trim();
            let Some(remainder) = trimmed.strip_prefix("use crate::") else {
                continue;
            };
            let module = remainder
                .split([':', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            if module.is_empty() || matches!(module, "self" | "super" | "crate") {
                continue;
            }
            let file = root.join("src").join(format!("{module}.rs"));
            let nested = root.join("src").join(module).join("mod.rs");
            if !file.exists() && !nested.exists() {
                modules.push(module.to_string());
            }
        }
    }
    Ok(())
}

fn resolve_root_module_file(root: &Path) -> Result<PathBuf, String> {
    for candidate in ["src/lib.rs", "src/main.rs"] {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(format!(
        "failed to resolve root module file under {}",
        root.display()
    ))
}

fn insert_mod_declarations(entry_file: &Path, modules: &[String]) -> Result<Vec<String>, String> {
    let mut content = fs::read_to_string(entry_file)
        .map_err(|err| format!("failed to read {}: {err}", entry_file.display()))?;
    let mut existing = content
        .lines()
        .filter_map(parse_mod_declaration)
        .collect::<Vec<_>>();
    existing.sort();
    existing.dedup();

    let mut missing = modules
        .iter()
        .filter(|module| !existing.iter().any(|current| current == *module))
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    let declarations = missing
        .iter()
        .map(|module| format!("pub mod {module};"))
        .collect::<Vec<_>>()
        .join("\n");

    let insert_at = content
        .lines()
        .enumerate()
        .filter(|(_, line)| parse_mod_declaration(line).is_some())
        .map(|(index, line)| {
            let mut offset = 0usize;
            for current in content.lines().take(index + 1) {
                offset += current.len() + 1;
            }
            if line.is_empty() { 0 } else { offset }
        })
        .last()
        .unwrap_or(0);

    if insert_at == 0 {
        content = format!("{declarations}\n{content}");
    } else {
        content.insert_str(insert_at, &format!("{declarations}\n"));
    }

    fs::write(entry_file, content)
        .map_err(|err| format!("failed to write {}: {err}", entry_file.display()))?;
    Ok(missing)
}

fn parse_mod_declaration(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let stripped = trimmed
        .strip_prefix("pub mod ")
        .or_else(|| trimmed.strip_prefix("mod "))?;
    stripped
        .strip_suffix(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("failed to create {}: {err}", to.display()))?;
    let mut entries = fs::read_dir(from)
        .map_err(|err| format!("failed to read {}: {err}", from.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", from.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", from_path.display()))?;
        if file_type.is_dir() {
            copy_dir_recursive(&from_path, &to_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = to_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
            }
            fs::copy(&from_path, &to_path).map_err(|err| {
                format!(
                    "failed to copy {} to {}: {err}",
                    from_path.display(),
                    to_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn run_build_validation(root: &Path) -> Result<(), String> {
    if !root.join("Cargo.toml").exists() {
        return Ok(());
    }
    let output = Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run cargo check in {}: {err}", root.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!("build_error: {message}"))
}

fn snapshot_workspace(root: &Path, change_set: &CodeChangeSet) -> Result<Vec<BackupEntry>, String> {
    let mut paths = change_set
        .changes
        .iter()
        .map(|change| root.join(&change.file_path))
        .collect::<Vec<_>>();
    if let Ok(entry_file) = resolve_root_module_file(root) {
        paths.push(entry_file);
    }
    for module in predicted_missing_import_modules(root, change_set) {
        paths.push(root.join("src").join(format!("{module}.rs")));
    }
    paths.sort();
    paths.dedup();

    let mut backups = Vec::new();
    for path in paths {
        let original = if path.exists() {
            Some(
                fs::read(&path)
                    .map_err(|err| format!("failed to snapshot {}: {err}", path.display()))?,
            )
        } else {
            None
        };
        backups.push(BackupEntry { path, original });
    }
    Ok(backups)
}

fn predicted_missing_import_modules(root: &Path, change_set: &CodeChangeSet) -> Vec<String> {
    let mut modules = Vec::new();
    for change in &change_set.changes {
        let replacement = change
            .hunks
            .last()
            .map(|hunk| hunk.replacement.as_str())
            .unwrap_or_default();
        for line in replacement.lines() {
            let trimmed = line.trim();
            let Some(remainder) = trimmed.strip_prefix("use crate::") else {
                continue;
            };
            let module = remainder
                .split([':', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            if module.is_empty() {
                continue;
            }
            let file = root.join("src").join(format!("{module}.rs"));
            let nested = root.join("src").join(module).join("mod.rs");
            let created_by_change = change_set
                .changes
                .iter()
                .any(|candidate| candidate.file_path == format!("src/{module}.rs"));
            if !file.exists() && !nested.exists() && !created_by_change {
                modules.push(module.to_string());
            }
        }
    }
    modules.sort();
    modules.dedup();
    modules
}

fn restore_workspace(backups: Vec<BackupEntry>) -> Result<(), String> {
    for backup in backups {
        match backup.original {
            Some(content) => fs::write(&backup.path, content)
                .map_err(|err| format!("failed to restore {}: {err}", backup.path.display()))?,
            None => {
                if backup.path.exists() {
                    fs::remove_file(&backup.path).map_err(|err| {
                        format!(
                            "failed to remove {} during rollback: {err}",
                            backup.path.display()
                        )
                    })?;
                }
            }
        }
    }
    Ok(())
}

fn resolve_module_file(root: &Path, module: &str) -> PathBuf {
    let direct = root.join("src").join(format!("{module}.rs"));
    if direct.exists() {
        return direct;
    }
    let nested = root.join("src").join(module).join("mod.rs");
    if nested.exists() {
        return nested;
    }
    direct
}

fn update_dependency_content(content: &str, target: &str, via: Option<&str>) -> String {
    let desired = match via {
        Some(via) if via.ends_with("Interface") => {
            format!("use crate::{}::{};", snake_case(via), via)
        }
        Some(via) => format!("use crate::{};", snake_case(via)),
        None => format!("use crate::{};", snake_case(target)),
    };
    let target_prefix = format!("use crate::{}", snake_case(target));
    let mut lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    if matches!(via, Some(value) if value.ends_with("Interface")) {
        if !lines.iter().any(|line| line.trim() == desired) {
            lines.insert(0, desired);
        }
    } else if let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with(&target_prefix))
    {
        lines[index] = desired;
    } else {
        lines.insert(0, desired);
    }
    lines.dedup();
    let mut updated = lines.join("\n");
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn normalize_relative(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .map_err(|_| format!("failed to relativize {}", path.display()))
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result.replace('-', "_")
}

fn pascal_case(value: &str) -> String {
    value
        .split(['_', '-', ' '])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use integration_layer::{
        CodePatch, MetricsDelta, PatchOperation, PhaseType, PlanSummary, RefactorPhase,
        RefactorPlan, RefactorPlanAction,
    };

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("design_cli_coding_{name}_{unique}"));
        fs::create_dir_all(dir.join("src")).expect("create src");
        dir
    }

    fn write_rust_project(dir: &Path, body: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write manifest");
        fs::write(dir.join("src/main.rs"), body).expect("write main");
    }

    #[test]
    fn patch_to_edit() {
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
            }],
            description: "move".to_string(),
        }];
        let edits = patches_to_edits(&patches);
        assert_eq!(
            edits,
            vec![Edit::ReplaceDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
            }]
        );
    }

    #[test]
    fn interface_generation() {
        let root = temp_dir("interface_generation");
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("debug".to_string(), "renderer".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "DebugRendererInterface".to_string(),
                between: ("debug".to_string(), "renderer".to_string()),
            }],
            description: "interface".to_string(),
        }];
        let change_set = generate_code_change_set(&root, &patches).expect("change set");
        assert!(
            change_set
                .changes
                .iter()
                .any(|change| change.file_path == "src/debug_renderer_interface.rs")
        );
    }

    #[test]
    fn diff_generation() {
        let root = temp_dir("diff_generation");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::world;\nfn render() {}\n",
        )
        .expect("write");
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
            }],
            description: "move".to_string(),
        }];
        let change_set = generate_code_change_set(&root, &patches).expect("change set");
        let change = change_set.changes.first().expect("change");
        assert_eq!(change.change_type, ChangeType::ModifyFile);
        assert!(
            change.hunks[0]
                .replacement
                .contains("use crate::renderer_world_interface;")
        );
    }

    #[test]
    fn coding_deterministic() {
        let root = temp_dir("deterministic");
        fs::write(root.join("src/renderer.rs"), "fn render() {}\n").expect("write");
        let plan = RefactorPlan {
            phases: vec![RefactorPhase {
                phase_type: PhaseType::OptimizeFlow,
                actions: vec![RefactorPlanAction::ExtractComponent {
                    from: "world".to_string(),
                }],
            }],
            summary: PlanSummary {
                total_actions: 1,
                phase_count: 1,
                expected_improvement: MetricsDelta {
                    cycle_count: 0,
                    layer_violations: 0,
                    coupling_score_milli: 0,
                },
            },
        };
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: plan.phases[0].actions[0].clone(),
            operations: vec![PatchOperation::ExtractComponent {
                from: "world".to_string(),
                component: "world_service".to_string(),
            }],
            description: "extract".to_string(),
        }];
        let lhs = generate_code_change_set(&root, &patches).expect("lhs");
        let rhs = generate_code_change_set(&root, &patches).expect("rhs");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn apply_success() {
        let root = temp_dir("apply_success");
        write_rust_project(&root, "fn main() {}\n");
        let change_set = CodeChangeSet {
            changes: vec![CodeChange {
                file_path: "src/world_service.rs".to_string(),
                change_type: ChangeType::CreateFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "pub struct WorldService {}\n".to_string(),
                }],
            }],
            summary: ChangeSummary {
                total_changes: 1,
                create_files: 1,
                modify_files: 0,
                move_files: 0,
            },
        };
        let result = execute_code_change_set(
            &root,
            &change_set,
            &CodingOptions {
                apply: true,
                check: false,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
            },
        )
        .expect("apply");
        assert_eq!(result.status, "applied");
        assert!(result.build_fixed);
        assert!(root.join("src/world_service.rs").exists());
        assert!(
            fs::read_to_string(root.join("src/main.rs"))
                .expect("read main")
                .contains("pub mod world_service;")
        );
    }

    #[test]
    fn rollback_on_build_fail() {
        let root = temp_dir("rollback");
        write_rust_project(&root, "fn main() {}\n");
        let original = fs::read_to_string(root.join("src/main.rs")).expect("read original");
        let change_set = CodeChangeSet {
            changes: vec![CodeChange {
                file_path: "src/main.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "fn main( {\n".to_string(),
                }],
            }],
            summary: ChangeSummary {
                total_changes: 1,
                create_files: 0,
                modify_files: 1,
                move_files: 0,
            },
        };
        let result = execute_code_change_set(
            &root,
            &change_set,
            &CodingOptions {
                apply: true,
                check: false,
                no_build: false,
                backup: true,
                format: false,
                safe_mode: true,
                auto_commit: false,
            },
        )
        .expect("execute");
        assert_eq!(result.status, "failed");
        assert!(result.rolled_back);
        assert_eq!(
            fs::read_to_string(root.join("src/main.rs")).expect("read restored"),
            original
        );
    }

    #[test]
    fn sandbox_isolated() {
        let root = temp_dir("sandbox");
        write_rust_project(&root, "fn main() {}\n");
        let change_set = CodeChangeSet {
            changes: vec![CodeChange {
                file_path: "src/world_service.rs".to_string(),
                change_type: ChangeType::CreateFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "pub struct WorldService {}\n".to_string(),
                }],
            }],
            summary: ChangeSummary {
                total_changes: 1,
                create_files: 1,
                modify_files: 0,
                move_files: 0,
            },
        };
        let result = execute_code_change_set(
            &root,
            &change_set,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
            },
        )
        .expect("execute");
        assert_eq!(result.status, "checked");
        assert!(result.build_fixed);
        assert!(!root.join("src/world_service.rs").exists());
    }

    #[test]
    fn breaking_diff_rejected_in_safe_mode() {
        let root = temp_dir("breaking_diff");
        write_rust_project(&root, "pub fn exposed() {}\nfn main() {}\n");
        let change_set = CodeChangeSet {
            changes: vec![CodeChange {
                file_path: "src/main.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 2,
                    replacement: "fn main() {}\n".to_string(),
                }],
            }],
            summary: ChangeSummary {
                total_changes: 1,
                create_files: 0,
                modify_files: 1,
                move_files: 0,
            },
        };

        let err = execute_code_change_set(
            &root,
            &change_set,
            &CodingOptions {
                apply: false,
                check: false,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
            },
        )
        .expect_err("breaking diff should fail validation");

        assert!(err.contains("breaking change detected"));
    }

    #[test]
    fn mod_insertion() {
        let root = temp_dir("mod_insertion");
        write_rust_project(&root, "fn main() {}\n");
        fs::write(
            root.join("src/world_service.rs"),
            "pub struct WorldService {}\n",
        )
        .expect("write module");
        let fix = fix_build(&root).expect("fix build");
        assert!(
            fix.registered_modules
                .iter()
                .any(|module| module == "world_service")
        );
        let main = fs::read_to_string(root.join("src/main.rs")).expect("read main");
        assert!(main.contains("pub mod world_service;"));
    }

    #[test]
    fn import_resolution() {
        let root = temp_dir("import_resolution");
        write_rust_project(
            &root,
            "pub mod renderer;\nfn main() { renderer::render(); }\n",
        );
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::renderer_world_interface;\npub fn render() {}\n",
        )
        .expect("write renderer");
        let fix = fix_build(&root).expect("fix build");
        assert!(
            fix.created_placeholders
                .iter()
                .any(|module| module == "renderer_world_interface")
        );
        assert!(root.join("src/renderer_world_interface.rs").exists());
        let main = fs::read_to_string(root.join("src/main.rs")).expect("read main");
        assert!(main.contains("pub mod renderer_world_interface;"));
    }

    #[test]
    fn build_success_after_fix() {
        let root = temp_dir("build_success_after_fix");
        write_rust_project(
            &root,
            "pub mod renderer;\nfn main() { renderer::render(); }\n",
        );
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug_renderer_interface::DebugRendererInterface;\nuse crate::renderer_world_interface;\npub fn render() {}\n",
        )
        .expect("write renderer");
        fs::write(
            root.join("src/debug_renderer_interface.rs"),
            "pub trait DebugRendererInterface {}\n",
        )
        .expect("write interface");
        let fix = fix_build(&root).expect("fix build");
        assert!(fix.build_fixed);
        run_build_validation(&root).expect("cargo check after fix");
    }

    #[test]
    fn fix_deterministic() {
        let root = temp_dir("fix_deterministic");
        write_rust_project(&root, "fn main() {}\n");
        fs::write(
            root.join("src/world_service.rs"),
            "pub struct WorldService {}\n",
        )
        .expect("write module");
        let lhs = fix_build(&root).expect("lhs");
        let main_after_lhs = fs::read_to_string(root.join("src/main.rs")).expect("read lhs");
        let rhs = fix_build(&root).expect("rhs");
        let main_after_rhs = fs::read_to_string(root.join("src/main.rs")).expect("read rhs");
        assert_eq!(lhs.registered_modules, vec!["world_service".to_string()]);
        assert!(rhs.registered_modules.is_empty());
        assert_eq!(main_after_lhs, main_after_rhs);
    }
}
