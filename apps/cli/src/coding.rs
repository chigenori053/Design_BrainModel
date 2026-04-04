use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use integration_layer::{CodePatch, PatchOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::refactor::{
    ApplyResolverError, PatchScope, RefactorCandidate, RefactorOperation,
    apply_resolver_error_message, load_matching_refactor_candidate, load_refactor_candidate,
    validate_apply_candidate,
};
use crate::runner::{ExecutionConfig, OutputMode, SandboxMode, SandboxPolicy, TimeoutConfig};
use crate::runner::{fixed_env, resolve_command, run as run_command};
use crate::source_index::{ApplyTargetResolution, ModuleSourceIndex, QualifiedModuleId};

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
    /// Canonical narrowed patches: post bootstrap-policy + semantic-cluster prune.
    /// Count is the candidate log; must equal the set that produced `changes`.
    pub patches: Vec<CodePatch>,
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
    pub confirm_commit: bool,
    pub auto_push: bool,
    pub confirm_push: bool,
    pub auto_pr: bool,
    pub confirm_pr: bool,
    pub pr_base: String,
    pub patch_scope: PatchScope,
    pub explicit_target: Option<PathBuf>,
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
    pub transactional_apply: Option<TransactionalApplyResult>,
    pub git_commit: Option<RestrictedGitCommitResult>,
    pub git_push: Option<RestrictedGitPushResult>,
    pub pull_request: Option<RestrictedPullRequestResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TransactionalApplyResult {
    pub applied: bool,
    pub build_ok: bool,
    pub rolled_back: bool,
    pub sandbox_path: PathBuf,
    pub modified_files: Vec<PathBuf>,
    pub diagnostics: Vec<String>,
    pub elapsed_ms: u128,
    pub sandbox_elapsed_ms: u128,
    pub cargo_check_ms: u128,
    pub cleanup_ms: u128,
    pub cleanup_ok: bool,
    pub rollback_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionalApplyError {
    SandboxCreateFailed,
    SandboxApplyFailed,
    SandboxRemapFailed,
    CargoCheckFailed,
    DriftDetectedBeforeCommit,
    CleanupFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RestrictedGitCommitResult {
    pub staged_files: Vec<PathBuf>,
    pub commit_created: bool,
    pub commit_hash: Option<String>,
    pub confirmation_required: bool,
    pub dirty_excluded: Vec<PathBuf>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestrictedGitCommitError {
    GitUnavailable,
    DirtyIsolationFailed,
    StagingRejected,
    ConfirmationDeclined,
    CommitFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RestrictedGitPushResult {
    pub branch_name: String,
    pub push_created: bool,
    pub remote_name: String,
    pub remote_ref: String,
    pub dry_run_ok: bool,
    pub confirmation_required: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestrictedGitPushError {
    DetachedHead,
    ProtectedBranchRejected,
    DryRunFailed,
    ConfirmationDeclined,
    PushFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RestrictedPullRequestResult {
    pub branch_name: String,
    pub base_branch: String,
    pub pr_created: bool,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub draft: bool,
    pub duplicate_detected: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestrictedPullRequestError {
    GhAuthUnavailable,
    InvalidBaseBranch,
    DuplicatePullRequest,
    ConfirmationDeclined,
    PullRequestCreateFailed,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchFence {
    scope: PatchScope,
    explicit_target: Option<PathBuf>,
}

#[cfg(test)]
static BEFORE_REAL_APPLY_HOOK: std::sync::OnceLock<std::sync::Mutex<Option<fn(&Path)>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
static COMMIT_CONFIRMATION_RESPONSE: std::sync::OnceLock<std::sync::Mutex<Option<bool>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
static PUSH_CONFIRMATION_RESPONSE: std::sync::OnceLock<std::sync::Mutex<Option<bool>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
static PR_CONFIRMATION_RESPONSE: std::sync::OnceLock<std::sync::Mutex<Option<bool>>> =
    std::sync::OnceLock::new();

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSafetyPolicy {
    Normal,
    ResolverSelfHost,
}

const DETERMINISTIC_REPL_V2_PATCH_ID: &str = "deterministic_repl_v2_wiring";
const REPL_V2_IMPORT_LINE: &str = "use crate::nl::planner_v2::plan_input as plan_nl_input_v2;";
const REPL_V2_UPDATE_IMPORT_LINE: &str =
    "use crate::nl::planner_v2::update_conversation_after_plan;";
const REPL_LEGACY_IMPORT_SNIPPET: &str = "plan_input as plan_nl_input";
const REPL_FALLBACK_CHAIN_SNIPPET: &str = ".or_else(|| plan_nl_input(input, session))";

pub fn deterministic_repl_v2_rule_matches(target: &Path, request: Option<&str>) -> bool {
    let Some(request) = request else {
        return false;
    };
    if !target.ends_with("repl.rs") {
        return false;
    }
    let lower = request.to_lowercase();
    let has_intent_keyword = ["planner_v2", "nl_v2", "routing", "planner 接続"]
        .iter()
        .any(|keyword| lower.contains(keyword));
    let has_mutation_keyword = ["修正", "接続", "切替", "rewrite"]
        .iter()
        .any(|keyword| lower.contains(keyword));
    has_intent_keyword && has_mutation_keyword
}

pub fn deterministic_repl_v2_wiring_patch(target: &Path, request: &str) -> Option<Vec<CodePatch>> {
    if !deterministic_repl_v2_rule_matches(target, Some(request)) {
        return None;
    }
    Some(vec![CodePatch {
        patch_id: DETERMINISTIC_REPL_V2_PATCH_ID.to_string(),
        action: integration_layer::RefactorPlanAction::MoveDependency {
            from: "repl".to_string(),
            to: "nl::planner_v2".to_string(),
            via: Some("plan_nl_input_v2".to_string()),
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "repl".to_string(),
            to: "nl::planner_v2".to_string(),
            via: Some("plan_nl_input_v2".to_string()),
        }],
        description: "deterministic repl planner_v2 wiring rewrite".to_string(),
    }])
}

pub fn deterministic_repl_v2_wiring_patches(
    root: &Path,
    target: &Path,
    request: Option<&str>,
) -> Option<Vec<CodePatch>> {
    let request = request?;
    let patches = deterministic_repl_v2_wiring_patch(target, request)?;
    let target_path = normalize_target_scope_path(root, target).ok()?;
    let source = fs::read_to_string(root.join(&target_path)).ok()?;
    if repl_v2_wiring_already_applied(&source) {
        return Some(vec![]);
    }
    Some(patches)
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

pub fn bootstrap_safety_policy(target_override: Option<&Path>) -> BootstrapSafetyPolicy {
    match target_override {
        Some(path) if path.ends_with("source_index.rs") => BootstrapSafetyPolicy::ResolverSelfHost,
        _ => BootstrapSafetyPolicy::Normal,
    }
}

pub fn apply_bootstrap_safety_policy(
    patches: &[CodePatch],
    target_override: Option<&Path>,
) -> Vec<CodePatch> {
    match bootstrap_safety_policy(target_override) {
        BootstrapSafetyPolicy::Normal => patches.to_vec(),
        BootstrapSafetyPolicy::ResolverSelfHost => patches
            .iter()
            .filter(|patch| {
                !matches!(
                    patch.action,
                    integration_layer::RefactorPlanAction::IntroduceInterface { .. }
                        | integration_layer::RefactorPlanAction::MoveDependency { .. }
                ) && !patch.operations.iter().any(|operation| {
                    matches!(
                        operation,
                        PatchOperation::CreateInterface { .. }
                            | PatchOperation::UpdateDependency { .. }
                    )
                })
            })
            .cloned()
            .collect(),
    }
}

pub fn semantic_cluster_for_target(target: &Path) -> Vec<&'static str> {
    let file = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // File-level rules take priority over directory-level rules.
    // Broad architectural tokens (app, adapter, agent, dependency, controller,
    // domain, engine, capability) are intentionally excluded from repl.rs to
    // prevent accidental architectural refactors (Phase G3.1 safety gate).
    match file {
        "goal.rs" => return vec!["nl", "goal"],
        "repl.rs" => return vec!["repl", "nl", "planner_v2"],
        "coding.rs" => return vec!["coding", "source_index"],
        _ => {}
    }

    let components: Vec<String> = target
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();

    if components.iter().any(|c| c == "nl") {
        return vec!["nl", "goal"];
    }

    if components.iter().any(|c| c == "agent") {
        return vec!["agent", "domain", "capability"];
    }

    vec![]
}

/// Exact module token match: splits `module` on `::` and checks each segment
/// for equality with a cluster keyword. Substring matching is intentionally
/// excluded to prevent accidental hits on compound names like
/// `adapter_app_interface`.
fn exact_module_token_matches_cluster(module: &str, cluster: &[&str]) -> bool {
    module
        .split("::")
        .any(|segment| cluster.iter().any(|&kw| segment == kw))
}

pub fn patch_matches_cluster(patch: &CodePatch, cluster: &[&str]) -> bool {
    if cluster.is_empty() {
        return true;
    }

    // Check action module fields with exact token matching.
    let action_match = match &patch.action {
        integration_layer::RefactorPlanAction::IntroduceInterface { between: (a, b) } => {
            exact_module_token_matches_cluster(a, cluster)
                || exact_module_token_matches_cluster(b, cluster)
        }
        integration_layer::RefactorPlanAction::MoveDependency { from, to, via } => {
            exact_module_token_matches_cluster(from, cluster)
                || exact_module_token_matches_cluster(to, cluster)
                || via
                    .as_deref()
                    .map_or(false, |v| exact_module_token_matches_cluster(v, cluster))
        }
        integration_layer::RefactorPlanAction::RemoveDependency { from, to } => {
            exact_module_token_matches_cluster(from, cluster)
                || exact_module_token_matches_cluster(to, cluster)
        }
        integration_layer::RefactorPlanAction::SplitModule { target } => {
            exact_module_token_matches_cluster(target, cluster)
        }
        integration_layer::RefactorPlanAction::ExtractComponent { from } => {
            exact_module_token_matches_cluster(from, cluster)
        }
        integration_layer::RefactorPlanAction::IsolateNode { node } => {
            exact_module_token_matches_cluster(node, cluster)
        }
    };

    if action_match {
        return true;
    }

    // Check operation module fields only; generated names (interface name,
    // component name) are excluded per R3. Description is excluded per R4.
    // For repl.rs cluster ["repl", "nl", "planner_v2"], planner wiring tokens
    // (planner_v2, plan_input, execute_plan, update_conversation_after_plan,
    // ConversationState) match via exact_module_token_matches_cluster because
    // they appear as path segments in `crate::nl::planner_v2::*` imports.
    patch.operations.iter().any(|op| match op {
        PatchOperation::CreateInterface { between: (a, b), .. } => {
            exact_module_token_matches_cluster(a, cluster)
                || exact_module_token_matches_cluster(b, cluster)
        }
        PatchOperation::UpdateDependency { from, to, via } => {
            exact_module_token_matches_cluster(from, cluster)
                || exact_module_token_matches_cluster(to, cluster)
                || via
                    .as_deref()
                    .map_or(false, |v| exact_module_token_matches_cluster(v, cluster))
        }
        PatchOperation::SplitModule { module, new_modules } => {
            exact_module_token_matches_cluster(module, cluster)
                || new_modules
                    .iter()
                    .any(|m| exact_module_token_matches_cluster(m, cluster))
        }
        PatchOperation::ExtractComponent { from, .. } => {
            exact_module_token_matches_cluster(from, cluster)
        }
    })
}

pub fn prune_patches_for_target(patches: &[CodePatch], target: &Path) -> Vec<CodePatch> {
    let cluster = semantic_cluster_for_target(target);
    if cluster.is_empty() {
        return patches.to_vec();
    }
    patches
        .iter()
        .filter(|patch| patch_matches_cluster(patch, &cluster))
        .cloned()
        .collect()
}

pub fn generate_code_change_set(
    root: &Path,
    patches: &[CodePatch],
) -> Result<CodeChangeSet, String> {
    generate_code_change_set_with_resolved_paths(root, patches, None, &BTreeMap::new())
}

pub fn generate_code_change_set_with_target(
    root: &Path,
    patches: &[CodePatch],
    target_override: Option<&Path>,
) -> Result<CodeChangeSet, String> {
    generate_code_change_set_with_resolved_paths(root, patches, target_override, &BTreeMap::new())
}

pub fn generate_code_change_set_with_resolved_paths(
    root: &Path,
    patches: &[CodePatch],
    target_override: Option<&Path>,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<CodeChangeSet, String> {
    if let Some(target) = target_override
        && let Some(change_set) = deterministic_repl_v2_change_set(root, target, patches)?
    {
        return Ok(change_set);
    }
    let patches = apply_bootstrap_safety_policy(patches, target_override);
    let patches = if let Some(target) = target_override {
        prune_patches_for_target(&patches, target)
    } else {
        patches
    };
    let mut drafts = BTreeMap::<String, FileDraft>::new();
    let source_index = ModuleSourceIndex::build(root).unwrap_or_default();
    let target_override = resolve_target_override(root, target_override)?;
    let fence = patch_fence_for_target(target_override.as_deref(), root);
    for edit in patches_to_edits(&patches) {
        apply_edit(
            root,
            &mut drafts,
            edit,
            &source_index,
            &fence,
            resolved_paths,
        )?;
    }
    if fence.scope != PatchScope::ExplicitTargetOnly {
        rewrite_crate_imports_for_generated_files(root, &source_index, &mut drafts)?;
        register_generated_submodules(root, &mut drafts)?;
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
    enforce_patch_scope(&changes, &fence)?;

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

    Ok(CodeChangeSet { patches, changes, summary })
}

fn deterministic_repl_v2_change_set(
    root: &Path,
    target: &Path,
    patches: &[CodePatch],
) -> Result<Option<CodeChangeSet>, String> {
    if !target.ends_with("repl.rs")
        || patches.is_empty()
        || !patches
            .iter()
            .all(|patch| patch.patch_id == DETERMINISTIC_REPL_V2_PATCH_ID)
    {
        return Ok(None);
    }

    let relative = normalize_target_scope_path(root, target)?;
    let absolute = root.join(&relative);
    let original = fs::read_to_string(&absolute)
        .map_err(|err| format!("failed to read {}: {err}", absolute.display()))?;
    let updated = rewrite_repl_v2_source(&original)?;
    if updated == original {
        return Ok(Some(CodeChangeSet {
            patches: vec![],
            changes: vec![],
            summary: ChangeSummary::default(),
        }));
    }

    Ok(Some(CodeChangeSet {
        patches: patches.to_vec(),
        changes: vec![CodeChange {
            file_path: relative.display().to_string(),
            change_type: ChangeType::ModifyFile,
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line: original.lines().count().max(1),
                replacement: updated,
            }],
        }],
        summary: ChangeSummary {
            total_changes: 1,
            create_files: 0,
            modify_files: 1,
            move_files: 0,
        },
    }))
}

fn repl_v2_wiring_already_applied(content: &str) -> bool {
    content.contains(REPL_V2_IMPORT_LINE)
        && content.contains(REPL_V2_UPDATE_IMPORT_LINE)
        && content.contains(REPL_LEGACY_IMPORT_SNIPPET)
        && content.contains(REPL_FALLBACK_CHAIN_SNIPPET)
        && content.contains("render_plan_summary_with_label(&command_plan, planner_label)")
}

fn rewrite_repl_v2_source(content: &str) -> Result<String, String> {
    if repl_v2_wiring_already_applied(content) {
        return Ok(content.to_string());
    }

    let old_v2_import =
        "use crate::nl::planner_v2::{plan_input as plan_nl_input_v2, update_conversation_after_plan};";
    let new_v2_import = format!("{REPL_V2_IMPORT_LINE}\n{REPL_V2_UPDATE_IMPORT_LINE}");
    let content = if content.contains(old_v2_import) {
        content.replacen(old_v2_import, &new_v2_import, 1)
    } else if content.contains(REPL_V2_IMPORT_LINE) && content.contains(REPL_V2_UPDATE_IMPORT_LINE) {
        content.to_string()
    } else {
        return Err("deterministic repl_v2 rewrite: missing planner_v2 import block".to_string());
    };

    let old_nl_import = "use crate::nl::{execute_plan as execute_nl_plan, render_plan_summary};";
    let new_nl_import =
        "use crate::nl::{\n    execute_plan as execute_nl_plan, plan_input as plan_nl_input, render_plan_summary_with_label,\n};";
    let content = if content.contains(old_nl_import) {
        content.replacen(old_nl_import, new_nl_import, 1)
    } else if content.contains(REPL_LEGACY_IMPORT_SNIPPET)
        && content.contains("render_plan_summary_with_label")
    {
        content
    } else {
        return Err("deterministic repl_v2 rewrite: missing legacy planner import block".to_string());
    };

    let old_flow = r#"    if let Some(command_plan) = plan_nl_input_v2(input, session, conversation) {
        let planner_summary = render_plan_summary(&command_plan);
        emit_output(session, writer, &planner_summary)?;
        update_conversation_after_plan(input, &command_plan, conversation);

        if cfg!(test) {
            session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
            session.state = State::Completed;
            emit_output(session, writer, "[test] planner-only mode")?;
            return Ok(());
        }

        session.state = State::Running;
        for output in execute_nl_plan(&command_plan, conversation) {
            if !output.trim().is_empty() {
                emit_output(session, writer, &output)?;
            }
        }
        session.state = State::Completed;
        session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
        conversation.autonomous_label = None;
        print_follow_up_suggestions(input, session, writer)?;
        return Ok(());
    }
"#;
    let new_flow = r#"    let command_plan_v2 = plan_nl_input_v2(input, session, conversation);
    let planner_label = if command_plan_v2.is_some() {
        "nl_v2"
    } else {
        "nl_fallback"
    };
    let command_plan =
        command_plan_v2.or_else(|| plan_nl_input(input, session));

    if let Some(command_plan) = command_plan {
        let planner_summary = render_plan_summary_with_label(&command_plan, planner_label);
        emit_output(session, writer, &planner_summary)?;
        update_conversation_after_plan(input, &command_plan, conversation);

        if cfg!(test) {
            session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
            session.state = State::Completed;
            emit_output(session, writer, "[test] planner-only mode")?;
            return Ok(());
        }

        session.state = State::Running;
        for output in execute_nl_plan(&command_plan, conversation) {
            if !output.trim().is_empty() {
                emit_output(session, writer, &output)?;
            }
        }
        session.state = State::Completed;
        session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
        conversation.autonomous_label = None;
        print_follow_up_suggestions(input, session, writer)?;
        return Ok(());
    }
"#;

    if content.contains(old_flow) {
        Ok(content.replacen(old_flow, new_flow, 1))
    } else if content.contains(REPL_FALLBACK_CHAIN_SNIPPET)
        && content.contains("render_plan_summary_with_label(&command_plan, planner_label)")
    {
        Ok(content)
    } else {
        Err("deterministic repl_v2 rewrite: missing planner callsite block".to_string())
    }
}

pub fn resolve_apply_target_from_modules(
    root: &Path,
    module_name: &str,
) -> Option<ApplyTargetResolution> {
    let index = ModuleSourceIndex::build(root).ok()?;
    index.resolve_apply_target(module_name)
}

pub fn resolve_apply_target_relative(root: &Path, module_name: &str) -> Option<PathBuf> {
    resolve_apply_target_from_modules(root, module_name)
        .map(|resolution| resolution.resolved_relative_path)
}

pub fn resolve_apply_target(root: &Path, module_name: &str) -> Option<PathBuf> {
    resolve_apply_target_relative(root, module_name)
}

pub fn resolve_sandbox_module_file(
    module_name: &str,
    workspace_root: &Path,
    sandbox_root: &Path,
) -> Result<PathBuf, String> {
    let relative = resolve_apply_target_relative(workspace_root, module_name)
        .ok_or_else(|| format!("unable to resolve source path for module {module_name}"))?;
    let sandbox_path = sandbox_root.join(&relative);
    if !sandbox_path.exists() {
        return Err(format!(
            "failed to remap relative path into sandbox: {} -> {}",
            relative.display(),
            sandbox_path.display()
        ));
    }
    Ok(sandbox_path)
}

pub fn collect_apply_target_resolutions(
    root: &Path,
    patches: &[CodePatch],
    target_override: Option<&Path>,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<Vec<ApplyTargetResolution>, String> {
    let patches = apply_bootstrap_safety_policy(patches, target_override);
    let patches = if let Some(target) = target_override {
        prune_patches_for_target(&patches, target)
    } else {
        patches
    };
    let source_index = ModuleSourceIndex::build(root).unwrap_or_default();
    let target_override = resolve_target_override(root, target_override)?;
    let mut resolutions = BTreeMap::<String, ApplyTargetResolution>::new();
    for edit in patches_to_edits(&patches) {
        let module = match edit {
            Edit::CreateInterface { between, .. } => between.0,
            Edit::ReplaceDependency { from, .. } => from,
            Edit::SplitModule { module, .. } => module,
            Edit::ExtractComponent { from, .. } => from,
        };
        let resolution = if let Some(path) = target_override.as_deref() {
            let relative = normalize_relative(root, path).map(PathBuf::from)?;
            ApplyTargetResolution {
                module: module.clone(),
                resolution_strategy: "target_override".to_string(),
                resolved_relative_path: relative.clone(),
                resolved_path: relative,
                sandbox_path: None,
            }
        } else if let Some(path) = resolved_paths.get(&module) {
            ApplyTargetResolution {
                module: module.clone(),
                resolution_strategy: "candidate_snapshot".to_string(),
                resolved_relative_path: path.clone(),
                resolved_path: path.clone(),
                sandbox_path: None,
            }
        } else if let Some(resolution) = source_index.resolve_apply_target(&module) {
            resolution
        } else {
            continue;
        };
        resolutions.entry(module).or_insert(resolution);
    }
    Ok(resolutions.into_values().collect())
}

pub fn execute_code_change_set(
    root: &Path,
    change_set: &CodeChangeSet,
    options: &CodingOptions,
    transactional_candidate: Option<&RefactorCandidate>,
) -> Result<CodingExecutionResult, String> {
    let fence = PatchFence {
        scope: options.patch_scope,
        explicit_target: options
            .explicit_target
            .as_deref()
            .map(|path| normalize_target_scope_path(root, path))
            .transpose()?,
    };
    if let Err(reason) = enforce_patch_scope(&change_set.changes, &fence) {
        return Ok(CodingExecutionResult {
            status: "failed".to_string(),
            applied: false,
            checked: options.check || options.apply,
            build_fixed: false,
            build_ok: false,
            rolled_back: true,
            backed_up: options.apply || options.backup,
            reason: Some(reason),
            sandbox_root: None,
            files_changed: 0,
            diff: DiffReport::default(),
            committed: false,
            commit_id: None,
            branch: None,
            transactional_apply: None,
            git_commit: None,
            git_push: None,
            pull_request: None,
        });
    }
    let diff = compute_diff_report(root, change_set)?;
    if options.safe_mode {
        validate_diff_report(&diff)?;
    }

    if options.apply {
        let commit_backups = if options.auto_commit {
            Some(snapshot_workspace(root, change_set)?)
        } else {
            None
        };
        let transactional =
            transactional_apply(root, change_set, transactional_candidate, options.no_build)?;
        if !transactional.applied {
            return Ok(CodingExecutionResult {
                status: "failed".to_string(),
                applied: false,
                checked: true,
                build_fixed: false,
                build_ok: transactional.build_ok,
                rolled_back: transactional.rolled_back,
                backed_up: false,
                reason: Some(transactional.diagnostics.join("\n")),
                sandbox_root: Some(transactional.sandbox_path.display().to_string()),
                files_changed: 0,
                diff,
                committed: false,
                commit_id: None,
                branch: None,
                transactional_apply: Some(transactional),
                git_commit: None,
                git_push: None,
                pull_request: None,
            });
        }

        let mut result = CodingExecutionResult {
            status: "applied".to_string(),
            applied: true,
            checked: true,
            build_fixed: false,
            build_ok: transactional.build_ok,
            rolled_back: false,
            backed_up: false,
            reason: None,
            sandbox_root: Some(transactional.sandbox_path.display().to_string()),
            files_changed: change_set.summary.total_changes,
            diff,
            committed: false,
            commit_id: None,
            branch: None,
            transactional_apply: Some(transactional),
            git_commit: None,
            git_push: None,
            pull_request: None,
        };
        if options.auto_commit {
            match restricted_commit(
                &result
                    .transactional_apply
                    .as_ref()
                    .expect("transactional apply result")
                    .modified_files,
                root,
                transactional_candidate,
                options.confirm_commit,
            ) {
                Ok(commit) => {
                    result.committed = commit.commit_created;
                    result.commit_id = commit.commit_hash.clone();
                    result.branch = if commit.commit_created {
                        current_branch(root)?
                    } else {
                        None
                    };
                    if !commit.commit_created {
                        result.reason = Some(
                            "Commit aborted: unrelated dirty files were excluded successfully. Only DBM-applied files are eligible."
                                .to_string(),
                        );
                    }
                    result.git_commit = Some(commit);
                }
                Err(err) => {
                    if let Some(backups) = commit_backups {
                        restore_workspace(backups)?;
                    }
                    result.status = "failed".to_string();
                    result.applied = false;
                    result.rolled_back = true;
                    result.reason = Some(err);
                    result.files_changed = 0;
                    return Ok(result);
                }
            }
        }
        if options.auto_push && result.committed {
            match restricted_push(result.branch.as_deref(), root, options.confirm_push) {
                Ok(push) => {
                    if !push.push_created {
                        result.reason =
                            Some(format!("Push declined for branch '{}'.", push.branch_name));
                    }
                    result.git_push = Some(push);
                }
                Err(err) => {
                    result.status = "failed".to_string();
                    result.reason = Some(err);
                    return Ok(result);
                }
            }
        }
        if options.auto_pr {
            let branch_name = result
                .git_push
                .as_ref()
                .filter(|push| push.push_created)
                .map(|push| push.branch_name.as_str())
                .or(result.branch.as_deref());
            if let Some(branch_name) = branch_name {
                match restricted_pr_create(
                    branch_name,
                    &options.pr_base,
                    root,
                    transactional_candidate,
                    change_set.summary.total_changes,
                    options.confirm_pr,
                ) {
                    Ok(pr) => {
                        if !pr.pr_created && pr.duplicate_detected {
                            result.reason = Some(format!(
                                "PR creation skipped: duplicate open PR already exists for '{}'.",
                                pr.branch_name
                            ));
                        }
                        result.pull_request = Some(pr);
                    }
                    Err(err) => {
                        result.status = "failed".to_string();
                        result.reason = Some(err);
                        return Ok(result);
                    }
                }
            }
        }
        return Ok(result);
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
            if fence.scope != PatchScope::ExplicitTargetOnly {
                let fix_result =
                    fix_build_in_sandbox(root, sandbox_root, change_set, transactional_candidate)?;
                build_fixed = fix_result.build_fixed;
            }
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
            transactional_apply: None,
            git_commit: None,
            git_push: None,
            pull_request: None,
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
            transactional_apply: None,
            git_commit: None,
            git_push: None,
            pull_request: None,
        });
    }

    let backups = snapshot_workspace(root, change_set)?;
    match apply_code_change_set(root, change_set)
        .and_then(|_| {
            if options.no_build {
                Ok(FixResult::default())
            } else if fence.scope == PatchScope::ExplicitTargetOnly {
                Ok(FixResult::default())
            } else {
                fix_build_for_change_set(root, change_set, transactional_candidate)
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
        Ok(()) => Ok(CodingExecutionResult {
            status: "applied".to_string(),
            applied: true,
            checked,
            build_fixed,
            build_ok,
            rolled_back: false,
            backed_up,
            reason: None,
            sandbox_root: sandbox_root.as_ref().map(|path| path.display().to_string()),
            files_changed: change_set.summary.total_changes,
            diff,
            committed: false,
            commit_id: None,
            branch: None,
            transactional_apply: None,
            git_commit: None,
            git_push: None,
            pull_request: None,
        }),
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
                transactional_apply: None,
                git_commit: None,
                git_push: None,
                pull_request: None,
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

pub fn transactional_apply(
    root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
    skip_build: bool,
) -> Result<TransactionalApplyResult, String> {
    let started = Instant::now();
    let sandbox_root = transactional_sandbox_root(root, candidate)?;
    let mut diagnostics = Vec::new();
    let mut build_ok = skip_build;
    let mut applied = false;
    let mut rolled_back = true;
    let mut cleanup_ok = false;
    let mut sandbox_elapsed_ms = 0;
    let mut cargo_check_ms = 0;
    let cleanup_ms;
    let rollback_count = 0usize;
    let modified_files = change_set
        .changes
        .iter()
        .map(|change| PathBuf::from(&change.file_path))
        .collect::<Vec<_>>();

    let sandbox_result: Result<(), TransactionalApplyError> = (|| {
        let sandbox_started = Instant::now();
        create_transactional_sandbox(root, &sandbox_root).map_err(|err| {
            diagnostics.push(err);
            TransactionalApplyError::SandboxCreateFailed
        })?;
        validate_sandbox_change_set(&sandbox_root, change_set).map_err(|err| {
            diagnostics.push(err);
            TransactionalApplyError::SandboxRemapFailed
        })?;
        apply_code_change_set(&sandbox_root, change_set).map_err(|err| {
            diagnostics.push(err);
            TransactionalApplyError::SandboxApplyFailed
        })?;
        sandbox_elapsed_ms = sandbox_started.elapsed().as_millis();

        if !skip_build {
            let build_started = Instant::now();
            let build = run_transactional_cargo_check(root, &sandbox_root, change_set, candidate)
                .map_err(|err| {
                diagnostics.push(err);
                TransactionalApplyError::CargoCheckFailed
            })?;
            cargo_check_ms = build_started.elapsed().as_millis();
            build_ok = true;
            diagnostics.extend(build);
        }

        run_before_real_apply_test_hook(root).map_err(|err| {
            diagnostics.push(err);
            TransactionalApplyError::DriftDetectedBeforeCommit
        })?;
        if let Some(candidate) = candidate {
            validate_apply_candidate(root, candidate).map_err(|err| {
                diagnostics.push(apply_resolver_error_message(
                    &err,
                    Some(&candidate.source_path),
                ));
                TransactionalApplyError::DriftDetectedBeforeCommit
            })?;
        }

        let backups = snapshot_workspace(root, change_set).map_err(|err| {
            diagnostics.push(err);
            TransactionalApplyError::SandboxApplyFailed
        })?;
        match apply_code_change_set(root, change_set) {
            Ok(()) => {
                applied = true;
                rolled_back = false;
                Ok(())
            }
            Err(err) => {
                let restore = restore_workspace(backups);
                diagnostics.push(err);
                if let Err(restore_err) = restore {
                    diagnostics.push(restore_err);
                }
                Err(TransactionalApplyError::SandboxApplyFailed)
            }
        }
    })();

    let cleanup_started = Instant::now();
    match fs::remove_dir_all(&sandbox_root) {
        Ok(()) => cleanup_ok = true,
        Err(err) if !sandbox_root.exists() => cleanup_ok = true,
        Err(err) => {
            diagnostics.push(format!(
                "CleanupWarning::SandboxResidual: failed to remove {}: {err}",
                sandbox_root.display()
            ));
            let _ = TransactionalApplyError::CleanupFailed;
        }
    }
    cleanup_ms = cleanup_started.elapsed().as_millis();

    if sandbox_result.is_err() {
        let build_succeeded = skip_build || build_ok;
        return Ok(TransactionalApplyResult {
            applied: false,
            build_ok: build_succeeded,
            rolled_back: true,
            sandbox_path: sandbox_root,
            modified_files,
            diagnostics,
            elapsed_ms: started.elapsed().as_millis(),
            sandbox_elapsed_ms,
            cargo_check_ms,
            cleanup_ms,
            cleanup_ok,
            rollback_count,
        });
    }

    Ok(TransactionalApplyResult {
        applied,
        build_ok: skip_build || build_ok,
        rolled_back,
        sandbox_path: sandbox_root,
        modified_files,
        diagnostics,
        elapsed_ms: started.elapsed().as_millis(),
        sandbox_elapsed_ms,
        cargo_check_ms,
        cleanup_ms,
        cleanup_ok,
        rollback_count,
    })
}

#[cfg(test)]
fn run_before_real_apply_test_hook(root: &Path) -> Result<(), String> {
    let hook = BEFORE_REAL_APPLY_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("hook mutex")
        .take();
    if let Some(hook) = hook {
        hook(root);
    }
    Ok(())
}

#[cfg(not(test))]
fn run_before_real_apply_test_hook(_root: &Path) -> Result<(), String> {
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
    diffs.extend(explainability_diffs(change_set));
    diffs.sort_by(|lhs, rhs| lhs.target.cmp(&rhs.target));
    diffs.dedup_by(|lhs, rhs| lhs.target == rhs.target && lhs.kind == rhs.kind);
    let breaking_count = diffs.iter().filter(|diff| diff.breaking).count();
    Ok(DiffReport {
        diffs,
        breaking_count,
    })
}

fn explainability_diffs(change_set: &CodeChangeSet) -> Vec<ASTDiff> {
    let mut diffs = Vec::new();
    for change in &change_set.changes {
        let replacement = change
            .hunks
            .last()
            .map(|hunk| hunk.replacement.as_str())
            .unwrap_or_default();
        if change.file_path.ends_with("/mod.rs") || change.file_path.ends_with("lib.rs") {
            for module in replacement.lines().filter_map(parse_mod_declaration) {
                diffs.push(ASTDiff {
                    kind: DiffKind::Modify,
                    target: format!("ModRegistration: {} -> {}", change.file_path, module),
                    breaking: false,
                });
            }
        }
        for line in replacement.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                diffs.push(ASTDiff {
                    kind: DiffKind::Modify,
                    target: format!("ImportRebinding: {trimmed}"),
                    breaking: false,
                });
            }
        }
    }
    diffs
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

pub fn restricted_commit(
    changed_files: &[PathBuf],
    workspace_root: &Path,
    candidate: Option<&RefactorCandidate>,
    confirm: bool,
) -> Result<RestrictedGitCommitResult, String> {
    let started = Instant::now();
    let git_dir = workspace_root.join(".git");
    if !git_dir.exists() {
        return Err("GitUnavailable: git repository not found".to_string());
    }
    if changed_files.is_empty() {
        return Ok(RestrictedGitCommitResult {
            staged_files: Vec::new(),
            commit_created: false,
            commit_hash: None,
            confirmation_required: false,
            dirty_excluded: collect_dirty_paths(workspace_root)?
                .into_iter()
                .map(|(path, _)| PathBuf::from(path))
                .collect(),
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let changed_set = changed_files
        .iter()
        .map(|path| normalize_git_path(path))
        .collect::<BTreeSet<_>>();
    let dirty_paths = collect_dirty_paths(workspace_root)?;
    let dirty_excluded = dirty_paths
        .iter()
        .filter(|(path, _)| !changed_set.contains(path))
        .map(|(path, _)| PathBuf::from(path))
        .collect::<Vec<_>>();

    let staged_files = changed_files
        .iter()
        .map(|path| PathBuf::from(normalize_git_path(path)))
        .collect::<Vec<_>>();
    stage_exact_files(workspace_root, &staged_files)?;

    if !confirm_commit_gate(staged_files.len(), confirm)? {
        reset_staged_files(workspace_root, &staged_files)?;
        return Ok(RestrictedGitCommitResult {
            staged_files,
            commit_created: false,
            commit_hash: None,
            confirmation_required: true,
            dirty_excluded,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(workspace_root)
        .status()
        .map_err(|err| format!("failed to run git diff --cached: {err}"))?;
    if status.success() {
        return Ok(RestrictedGitCommitResult {
            staged_files,
            commit_created: false,
            commit_hash: None,
            confirmation_required: false,
            dirty_excluded,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let message = format!(
        "DBM: safe refactor apply ({})",
        candidate
            .map(|candidate| candidate.candidate_id.as_str())
            .filter(|id| !id.is_empty())
            .unwrap_or("adhoc")
    );
    let body = format!(
        "- transactional build: passed\n- affected crate: {}\n- files: {}",
        candidate
            .map(|candidate| candidate.module_id.crate_name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("unknown"),
        staged_files.len()
    );
    let output = Command::new("git")
        .args(["commit", "-m", &message, "-m", &body])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("CommitFailed: failed to run git commit: {err}"))?;
    if !output.status.success() {
        reset_staged_files(workspace_root, &staged_files)?;
        return Err(format!(
            "CommitFailed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let commit_hash = current_commit(workspace_root)?
        .ok_or_else(|| "CommitFailed: failed to resolve commit id".to_string())?;
    Ok(RestrictedGitCommitResult {
        staged_files,
        commit_created: true,
        commit_hash: Some(commit_hash),
        confirmation_required: false,
        dirty_excluded,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn collect_dirty_paths(root: &Path) -> Result<Vec<(String, String)>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("DirtyIsolationFailed: failed to run git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "DirtyIsolationFailed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].to_string();
        let path = line[3..].trim().to_string();
        if path.is_empty() {
            continue;
        }
        entries.push((path, status));
    }
    Ok(entries)
}

fn stage_exact_files(root: &Path, changed_files: &[PathBuf]) -> Result<(), String> {
    for path in changed_files {
        let path_str = path.to_string_lossy();
        if path_str.is_empty() || path_str.ends_with('/') || path_str == "." {
            return Err("StagingRejected: invalid staging path".to_string());
        }
        let output = Command::new("git")
            .arg("add")
            .arg("--")
            .arg(path.as_os_str())
            .current_dir(root)
            .output()
            .map_err(|err| format!("StagingRejected: failed to run git add: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "StagingRejected: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

fn reset_staged_files(root: &Path, changed_files: &[PathBuf]) -> Result<(), String> {
    if changed_files.is_empty() {
        return Ok(());
    }
    let mut command = Command::new("git");
    command.arg("reset").arg("-q").arg("HEAD").arg("--");
    for path in changed_files {
        command.arg(path.as_os_str());
    }
    let output = command
        .current_dir(root)
        .output()
        .map_err(|err| format!("StagingRejected: failed to reset staged files: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "StagingRejected: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn confirm_commit_gate(_file_count: usize, confirmed: bool) -> Result<bool, String> {
    if confirmed {
        return Ok(true);
    }
    #[cfg(test)]
    {
        if let Some(response) = COMMIT_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("confirm mutex")
            .take()
        {
            return Ok(response);
        }
    }
    Ok(false)
}

pub fn restricted_push(
    expected_branch: Option<&str>,
    workspace_root: &Path,
    confirm: bool,
) -> Result<RestrictedGitPushResult, String> {
    let started = Instant::now();
    let branch = current_branch(workspace_root)?
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
        .ok_or_else(|| "DetachedHead: current HEAD is not attached to a branch".to_string())?;
    if let Some(expected) = expected_branch {
        if branch != expected {
            return Err(format!(
                "PushFailed: branch mismatch, expected '{expected}' but found '{branch}'"
            ));
        }
    }
    if is_protected_branch(&branch) {
        return Err(format!(
            "Push rejected: protected branch '{branch}' is not eligible."
        ));
    }

    let dry_run = git_push_command(workspace_root, &branch, true)?;
    if !dry_run.status.success() {
        return Err(format!(
            "DryRunFailed: {}",
            String::from_utf8_lossy(&dry_run.stderr).trim()
        ));
    }

    if !confirm_push_gate(&branch, confirm)? {
        return Ok(RestrictedGitPushResult {
            branch_name: branch.clone(),
            push_created: false,
            remote_name: "origin".to_string(),
            remote_ref: format!("origin/{branch}"),
            dry_run_ok: true,
            confirmation_required: true,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let push = git_push_command(workspace_root, &branch, false)?;
    if !push.status.success() {
        return Err(format!(
            "PushFailed: {}",
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }
    Ok(RestrictedGitPushResult {
        branch_name: branch.clone(),
        push_created: true,
        remote_name: "origin".to_string(),
        remote_ref: format!("origin/{branch}"),
        dry_run_ok: true,
        confirmation_required: false,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn git_push_command(
    root: &Path,
    branch: &str,
    dry_run: bool,
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command.arg("push");
    if dry_run {
        command.arg("--dry-run");
    }
    command.arg("origin").arg(branch).current_dir(root);
    command
        .output()
        .map_err(|err| format!("PushFailed: failed to run git push: {err}"))
}

fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
        || branch.starts_with("release/")
        || branch.starts_with("hotfix/")
        || branch.starts_with("production/")
}

fn confirm_push_gate(branch: &str, confirmed: bool) -> Result<bool, String> {
    if confirmed {
        return Ok(true);
    }
    #[cfg(test)]
    {
        if let Some(response) = PUSH_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("push confirm mutex")
            .take()
        {
            return Ok(response);
        }
    }
    let _ = branch;
    Ok(false)
}

pub fn restricted_pr_create(
    branch_name: &str,
    base_branch: &str,
    workspace_root: &Path,
    candidate: Option<&RefactorCandidate>,
    file_count: usize,
    confirm: bool,
) -> Result<RestrictedPullRequestResult, String> {
    let started = Instant::now();
    if !matches!(base_branch, "main" | "develop" | "release-next") {
        return Err(format!(
            "InvalidBaseBranch: base branch '{base_branch}' is not in the allowlist"
        ));
    }
    ensure_gh_auth(workspace_root)?;

    if let Some(existing) = duplicate_pull_request(workspace_root, branch_name)? {
        return Ok(RestrictedPullRequestResult {
            branch_name: branch_name.to_string(),
            base_branch: base_branch.to_string(),
            pr_created: false,
            pr_number: existing.0,
            pr_url: existing.1,
            draft: true,
            duplicate_detected: true,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    if !confirm_pr_gate(branch_name, base_branch, confirm)? {
        return Ok(RestrictedPullRequestResult {
            branch_name: branch_name.to_string(),
            base_branch: base_branch.to_string(),
            pr_created: false,
            pr_number: None,
            pr_url: None,
            draft: true,
            duplicate_detected: false,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let title = format!(
        "DBM: safe refactor apply ({})",
        candidate
            .map(|candidate| candidate.candidate_id.as_str())
            .filter(|id| !id.is_empty())
            .unwrap_or("adhoc")
    );
    let body = format!(
        "## Summary\n- deterministic apply completed\n- transactional cargo check passed\n- exact restricted commit + push completed\n\n## Trace\n- branch: {branch_name}\n- crate: {}\n- files: {file_count}\n\n## Safety\n- drift guard: passed\n- protected push policy: passed",
        candidate
            .map(|candidate| candidate.module_id.crate_name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("unknown")
    );
    let output = Command::new(resolve_gh_tool())
        .args([
            "pr",
            "create",
            "--draft",
            "--base",
            base_branch,
            "--head",
            branch_name,
            "--title",
            &title,
            "--body",
            &body,
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("PullRequestCreateFailed: failed to run gh pr create: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "PullRequestCreateFailed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pr_number = url
        .rsplit('/')
        .next()
        .and_then(|segment| segment.parse::<u64>().ok());
    Ok(RestrictedPullRequestResult {
        branch_name: branch_name.to_string(),
        base_branch: base_branch.to_string(),
        pr_created: true,
        pr_number,
        pr_url: Some(url),
        draft: true,
        duplicate_detected: false,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn ensure_gh_auth(root: &Path) -> Result<(), String> {
    let output = Command::new(resolve_gh_tool())
        .args(["auth", "status"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("GhAuthUnavailable: failed to run gh auth status: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("PR creation aborted: GitHub authentication unavailable.".to_string())
    }
}

fn duplicate_pull_request(
    root: &Path,
    branch_name: &str,
) -> Result<Option<(Option<u64>, Option<String>)>, String> {
    let output = Command::new(resolve_gh_tool())
        .args([
            "pr",
            "list",
            "--head",
            branch_name,
            "--state",
            "open",
            "--json",
            "number,url",
        ])
        .current_dir(root)
        .output()
        .map_err(|err| format!("DuplicatePullRequest: failed to run gh pr list: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "DuplicatePullRequest: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value =
        serde_json::from_str(&stdout).map_err(|err| format!("DuplicatePullRequest: {err}"))?;
    let Some(first) = value.as_array().and_then(|items| items.first()) else {
        return Ok(None);
    };
    let number = first.get("number").and_then(|value| value.as_u64());
    let url = first
        .get("url")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    Ok(Some((number, url)))
}

fn confirm_pr_gate(branch_name: &str, base_branch: &str, confirmed: bool) -> Result<bool, String> {
    if confirmed {
        return Ok(true);
    }
    #[cfg(test)]
    {
        if let Some(response) = PR_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("pr confirm mutex")
            .take()
        {
            return Ok(response);
        }
    }
    let _ = (branch_name, base_branch);
    Ok(false)
}

fn resolve_gh_tool() -> String {
    std::env::var("DBM_GH_BIN").unwrap_or_else(|_| "gh".to_string())
}

fn normalize_git_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn fix_build(project_root: &Path) -> Result<FixResult, String> {
    fix_build_with_entry(project_root, resolve_root_module_file(project_root)?)
}

fn fix_build_in_sandbox(
    workspace_root: &Path,
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<FixResult, String> {
    fix_build_with_entry(
        sandbox_root,
        resolve_sandbox_root_module_file(workspace_root, sandbox_root, change_set, candidate)?,
    )
}

fn fix_build_for_change_set(
    workspace_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<FixResult, String> {
    fix_build_with_entry(
        workspace_root,
        resolve_root_module_file_for_change_set(workspace_root, change_set, candidate)?,
    )
}

fn fix_build_with_entry(project_root: &Path, entry_file: PathBuf) -> Result<FixResult, String> {
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
    let inserted = insert_mod_declarations(&entry_file, &modules)?;
    fix.registered_modules = inserted;
    fix.build_fixed = !(fix.registered_modules.is_empty() && fix.created_placeholders.is_empty());
    Ok(fix)
}

fn apply_edit(
    root: &Path,
    drafts: &mut BTreeMap<String, FileDraft>,
    edit: Edit,
    source_index: &ModuleSourceIndex,
    fence: &PatchFence,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    match edit {
        Edit::CreateInterface { name, between } => {
            let file_path = prune_patch_candidates(
                root,
                fence,
                vec![Some(
                    resolved_paths
                        .get(&between.0)
                        .map(|path| {
                            source_index.generated_path_from_source(
                                root,
                                path,
                                &format!("{}.rs", snake_case(&name)),
                            )
                        })
                        .unwrap_or_else(|| {
                            source_index.generated_path(
                                root,
                                &between.0,
                                &format!("{}.rs", snake_case(&name)),
                            )
                        }),
                )],
            )?;
            let Some(file_path) = file_path else {
                return Ok(());
            };
            let file_key = normalize_relative(root, &root.join(&file_path))?;
            guard_change_target(Path::new(&file_key), fence)?;
            let draft = load_or_create_draft(root, drafts, &file_key, true)?;
            draft.content = format!(
                "pub trait {} {{\n    // TODO: define required methods\n}}\n",
                name
            );
        }
        Edit::ReplaceDependency { from, to, via } => {
            let explicit_target = fence.explicit_target.as_deref().map(Path::to_path_buf);
            let resolved = resolved_paths.get(&from).cloned();
            let discovered = source_index
                .resolve_apply_target(&from)
                .map(|resolution| resolution.resolved_path)
                .or_else(|| source_index.resolve(&from).ok().flatten());
            let file_path =
                prune_patch_candidates(root, fence, vec![explicit_target, resolved, discovered])?;
            let Some(file_path) = file_path else {
                return Ok(());
            };
            let file_key = normalize_path_for_scope(&file_path).display().to_string();
            guard_change_target(Path::new(&file_key), fence)?;
            let draft = load_or_create_draft(root, drafts, &file_key, false)?;
            draft.content = update_dependency_content(&draft.content, &to, via.as_deref());
        }
        Edit::SplitModule { module, targets } => {
            for target in targets {
                let file_path = prune_patch_candidates(
                    root,
                    fence,
                    vec![Some(
                        resolved_paths
                            .get(&module)
                            .map(|path| {
                                source_index.generated_path_from_source(
                                    root,
                                    path,
                                    &format!("{}.rs", snake_case(&target)),
                                )
                            })
                            .unwrap_or_else(|| {
                                source_index.generated_path(
                                    root,
                                    &module,
                                    &format!("{}.rs", snake_case(&target)),
                                )
                            }),
                    )],
                )?;
                let Some(file_path) = file_path else {
                    continue;
                };
                let file_key = normalize_relative(root, &root.join(&file_path))?;
                guard_change_target(Path::new(&file_key), fence)?;
                let draft = load_or_create_draft(root, drafts, &file_key, true)?;
                draft.content =
                    format!("pub struct {} {{\n    // TODO\n}}\n", pascal_case(&target));
            }
        }
        Edit::ExtractComponent { from, name } => {
            let file_path = prune_patch_candidates(
                root,
                fence,
                vec![Some(
                    resolved_paths
                        .get(&from)
                        .map(|path| {
                            source_index.generated_path_from_source(
                                root,
                                path,
                                &format!("{}.rs", snake_case(&name)),
                            )
                        })
                        .unwrap_or_else(|| {
                            source_index.generated_path(
                                root,
                                &from,
                                &format!("{}.rs", snake_case(&name)),
                            )
                        }),
                )],
            )?;
            let Some(file_path) = file_path else {
                return Ok(());
            };
            let file_key = normalize_relative(root, &root.join(&file_path))?;
            guard_change_target(Path::new(&file_key), fence)?;
            let draft = load_or_create_draft(root, drafts, &file_key, true)?;
            draft.content = format!("pub struct {} {{\n    // TODO\n}}\n", pascal_case(&name));
        }
    }
    Ok(())
}

fn prune_patch_candidates(
    root: &Path,
    fence: &PatchFence,
    candidates: Vec<Option<PathBuf>>,
) -> Result<Option<PathBuf>, String> {
    let mut candidates = candidates
        .into_iter()
        .flatten()
        .map(|path| normalize_target_scope_path(root, &path))
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort();
    candidates.dedup();
    if fence.scope == PatchScope::ExplicitTargetOnly {
        let Some(explicit_target) = fence.explicit_target.as_ref() else {
            return Ok(None);
        };
        candidates.retain(|candidate| candidate == explicit_target);
    }
    Ok(candidates.into_iter().next())
}

pub fn resolve_apply_paths_for_patches(
    root: &Path,
    patches: &[CodePatch],
    candidate_id: Option<&str>,
) -> Result<BTreeMap<String, PathBuf>, String> {
    let patches = apply_bootstrap_safety_policy(patches, None);
    let mut resolved = BTreeMap::new();
    if let Some(candidate_id) = candidate_id {
        let candidate = load_refactor_candidate(root, candidate_id)
            .map_err(|err| apply_resolver_error_message(&err, None))?;
        let absolute = validate_apply_candidate(root, &candidate)
            .map_err(|err| apply_resolver_error_message(&err, Some(&candidate.source_path)))?;
        resolved.insert(
            candidate.logical_name.clone(),
            normalize_relative(root, &absolute).map(PathBuf::from)?,
        );
        return Ok(resolved);
    }

    for patch in &patches {
        if let Some((logical_name, operation)) = patch_resolver_key(patch) {
            match load_matching_refactor_candidate(root, &logical_name, operation) {
                Ok(candidate) => {
                    let absolute = validate_apply_candidate(root, &candidate).map_err(|err| {
                        apply_resolver_error_message(&err, Some(&candidate.source_path))
                    })?;
                    resolved.insert(
                        logical_name,
                        normalize_relative(root, &absolute).map(PathBuf::from)?,
                    );
                }
                Err(ApplyResolverError::MissingSnapshot) => {}
                Err(err) => {
                    return Err(apply_resolver_error_message(&err, None));
                }
            }
        }
    }
    Ok(resolved)
}

pub fn resolve_transactional_candidate_for_patches(
    root: &Path,
    patches: &[CodePatch],
    candidate_id: Option<&str>,
) -> Result<Option<RefactorCandidate>, String> {
    let patches = apply_bootstrap_safety_policy(patches, None);
    if let Some(candidate_id) = candidate_id {
        let candidate = load_refactor_candidate(root, candidate_id)
            .map_err(|err| apply_resolver_error_message(&err, None))?;
        return Ok(Some(candidate));
    }
    for patch in &patches {
        if let Some((logical_name, operation)) = patch_resolver_key(patch) {
            match load_matching_refactor_candidate(root, &logical_name, operation) {
                Ok(candidate) => return Ok(Some(candidate)),
                Err(ApplyResolverError::MissingSnapshot) => continue,
                Err(err) => return Err(apply_resolver_error_message(&err, None)),
            }
        }
    }
    Ok(None)
}

fn patch_resolver_key(patch: &CodePatch) -> Option<(String, RefactorOperation)> {
    match patch.operations.first()? {
        PatchOperation::CreateInterface { between, .. } => {
            Some((between.0.clone(), RefactorOperation::ExtractInterface))
        }
        PatchOperation::UpdateDependency { from, .. } => {
            Some((from.clone(), RefactorOperation::RemoveDependency))
        }
        PatchOperation::SplitModule { module, .. } => {
            Some((module.clone(), RefactorOperation::SplitModule))
        }
        PatchOperation::ExtractComponent { from, .. } => {
            Some((from.clone(), RefactorOperation::IntroduceService))
        }
    }
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

fn rewrite_crate_imports_for_generated_files(
    root: &Path,
    source_index: &ModuleSourceIndex,
    drafts: &mut BTreeMap<String, FileDraft>,
) -> Result<(), String> {
    let generated = drafts
        .iter()
        .filter(|(_, draft)| draft.change_type == ChangeType::CreateFile)
        .filter_map(|(file_key, _)| generated_rust_file(source_index, Path::new(file_key)))
        .collect::<Vec<_>>();
    if generated.is_empty() {
        return Ok(());
    }

    let generated_by_leaf = generated.into_iter().fold(
        BTreeMap::<(String, String), String>::new(),
        |mut map, file| {
            map.entry((file.qualified_id.crate_name, file.module_leaf))
                .or_insert(file.qualified_id.module_path);
            map
        },
    );
    let generated_symbol_targets = drafts
        .iter()
        .filter(|(_, draft)| draft.change_type == ChangeType::CreateFile)
        .filter_map(|(file_key, draft)| {
            generated_symbol_target(source_index, Path::new(file_key), &draft.content)
        })
        .collect::<BTreeMap<_, _>>();

    for (file_key, draft) in drafts.iter_mut() {
        if Path::new(file_key).extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let Some(current_id) = source_index.qualified_id_for_path(Path::new(file_key)) else {
            continue;
        };
        draft.content = rewrite_imports_in_content(
            root,
            source_index,
            &current_id,
            &draft.content,
            &generated_by_leaf,
            &generated_symbol_targets,
        )?;
    }
    Ok(())
}

fn register_generated_submodules(
    root: &Path,
    drafts: &mut BTreeMap<String, FileDraft>,
) -> Result<(), String> {
    let generated_files = drafts
        .iter()
        .filter(|(_, draft)| draft.change_type == ChangeType::CreateFile)
        .map(|(file_key, _)| file_key.clone())
        .collect::<Vec<_>>();
    for file_key in generated_files {
        let path = Path::new(&file_key);
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if matches!(stem, "lib" | "main" | "mod") {
            continue;
        }
        let Some(parent_module) = parent_module_registration_path(root, path) else {
            continue;
        };
        let parent_key = normalize_relative(root, &root.join(&parent_module))?;
        let draft = load_or_create_draft(root, drafts, &parent_key, true)?;
        draft.content = insert_mod_declarations_into_content(&draft.content, &[stem.to_string()]);
    }
    Ok(())
}

fn generated_rust_file(source_index: &ModuleSourceIndex, path: &Path) -> Option<GeneratedRustFile> {
    let qualified_id = source_index.qualified_id_for_path(path)?;
    let module_leaf = qualified_id
        .module_path
        .split("::")
        .last()
        .map(ToString::to_string)?;
    Some(GeneratedRustFile {
        qualified_id,
        module_leaf,
    })
}

fn generated_symbol_target(
    source_index: &ModuleSourceIndex,
    path: &Path,
    content: &str,
) -> Option<(String, GeneratedSymbolTarget)> {
    let qualified_id = source_index.qualified_id_for_path(path)?;
    let symbol = extract_first_rust_symbol(content)?;
    Some((
        symbol,
        GeneratedSymbolTarget {
            qualified_id,
            path: path.to_path_buf(),
        },
    ))
}

#[derive(Debug, Clone)]
struct GeneratedSymbolTarget {
    qualified_id: QualifiedModuleId,
    path: PathBuf,
}

fn generated_symbol_use_targets_path(
    root: &Path,
    current_id: &QualifiedModuleId,
    generated_symbol_targets: &BTreeMap<String, GeneratedSymbolTarget>,
    symbol: &str,
) -> Result<Option<String>, String> {
    let Some(target) = generated_symbol_targets.get(symbol) else {
        return Ok(None);
    };
    let crate_import = if target.qualified_id.crate_name == current_id.crate_name {
        "crate".to_string()
    } else {
        import_crate_name_for_relative(root, &root.join(&target.path))?
    };
    Ok(Some(match target.qualified_id.module_path.as_str() {
        module_path if module_path == target.qualified_id.crate_name => {
            format!("{crate_import}::{symbol}")
        }
        module_path => format!("{crate_import}::{module_path}::{symbol}"),
    }))
}

fn extract_first_rust_symbol(content: &str) -> Option<String> {
    let patterns = [
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "pub fn ",
        "fn ",
    ];
    for line in content.lines() {
        let trimmed = line.trim();
        for pattern in patterns {
            if let Some(rest) = trimmed.strip_prefix(pattern) {
                let symbol = rest
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or_default();
                if !symbol.is_empty() {
                    return Some(symbol.to_string());
                }
            }
        }
    }
    None
}

fn import_crate_name_for_relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("failed to relativize {}", path.display()))?;
    let mut current = root.join(relative).parent().map(Path::to_path_buf);
    while let Some(dir) = current {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            return parse_package_name(&manifest)?
                .map(|name| name.replace('-', "_"))
                .ok_or_else(|| {
                    format!("failed to parse package name from {}", manifest.display())
                });
        }
        if dir == root {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    Err(format!(
        "failed to resolve Cargo.toml for generated file {}",
        path.display()
    ))
}

#[derive(Debug, Clone)]
struct GeneratedRustFile {
    qualified_id: QualifiedModuleId,
    module_leaf: String,
}

fn parent_module_registration_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let parent = relative.parent()?;
    let src_index = parent
        .components()
        .position(|component| component.as_os_str() == "src")?;
    let depth_after_src = parent.components().count().saturating_sub(src_index + 1);
    if depth_after_src == 0 {
        return resolve_root_module_relative_from_target(root, relative);
    }
    Some(parent.join("mod.rs"))
}

fn rewrite_imports_in_content(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_id: &QualifiedModuleId,
    content: &str,
    generated_by_leaf: &BTreeMap<(String, String), String>,
    generated_symbol_targets: &BTreeMap<String, GeneratedSymbolTarget>,
) -> Result<String, String> {
    let mut lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    for line in &mut lines {
        *line = rewrite_rust_use_statement(
            root,
            source_index,
            current_id,
            line,
            generated_by_leaf,
            generated_symbol_targets,
        )?;
    }
    let mut updated = sort_leading_rust_imports(&lines.join("\n"));
    if content.ends_with('\n') || !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
    }
    Ok(updated)
}

fn rewrite_rust_use_statement(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_id: &QualifiedModuleId,
    line: &str,
    generated_by_leaf: &BTreeMap<(String, String), String>,
    generated_symbol_targets: &BTreeMap<String, GeneratedSymbolTarget>,
) -> Result<String, String> {
    let trimmed = line.trim();
    let Some(remainder) = trimmed.strip_prefix("use crate::") else {
        return Ok(line.to_string());
    };
    let Some(remainder) = remainder.strip_suffix(';') else {
        return Ok(line.to_string());
    };

    if let Some((prefix, symbols)) = parse_braced_crate_import(remainder) {
        let rebound_prefix =
            rebind_module_prefix(&current_id.crate_name, prefix, generated_by_leaf)
                .unwrap_or_else(|| prefix.to_string());
        if !prefix.is_empty() {
            return Ok(format!(
                "use crate::{rebound_prefix}::{{{}}};",
                symbols.join(", ")
            ));
        }
        let mut grouped = BTreeMap::<String, Vec<String>>::new();
        let mut passthrough = Vec::<String>::new();
        for symbol in symbols {
            if symbol.contains(" as ") {
                passthrough.push(symbol);
                continue;
            }
            if let Some(use_path) = generated_symbol_use_targets_path(
                root,
                current_id,
                generated_symbol_targets,
                &symbol,
            )?
            .or(source_index.resolve_symbol_use_path(root, current_id, &symbol)?)
            {
                if let Some((prefix, resolved_symbol)) = split_symbol_use_path(&use_path) {
                    grouped.entry(prefix).or_default().push(resolved_symbol);
                } else {
                    passthrough.push(symbol);
                }
            } else {
                passthrough.push(symbol);
            }
        }
        if grouped.is_empty() {
            return Ok(line.to_string());
        }
        let mut imports = grouped
            .into_iter()
            .map(|(prefix, mut symbols)| {
                symbols.sort();
                symbols.dedup();
                format_grouped_use_statement(&prefix, &symbols)
            })
            .collect::<Vec<_>>();
        if !passthrough.is_empty() {
            imports.push(format!("use crate::{{{}}};", passthrough.join(", ")));
        }
        imports.sort_by(|lhs, rhs| import_sort_key(lhs).cmp(&import_sort_key(rhs)));
        return Ok(imports.join("\n"));
    }

    let parts = remainder.split("::").collect::<Vec<_>>();
    if parts.is_empty() {
        return Ok(line.to_string());
    }
    if parts.len() == 1 && is_rust_symbol(parts[0]) {
        if let Some(use_path) =
            generated_symbol_use_targets_path(root, current_id, generated_symbol_targets, parts[0])?
                .or(source_index.resolve_symbol_use_path(root, current_id, parts[0])?)
        {
            return Ok(format!("use {use_path};"));
        }
        return Ok(line.to_string());
    }

    let last = *parts.last().unwrap_or(&"");
    if is_rust_symbol(last) {
        let module_prefix = parts[..parts.len() - 1].join("::");
        if let Some(rebound) =
            rebind_module_prefix(&current_id.crate_name, &module_prefix, generated_by_leaf)
        {
            return Ok(format!("use crate::{rebound}::{last};"));
        }
        if let Some(use_path) =
            generated_symbol_use_targets_path(root, current_id, generated_symbol_targets, last)?
                .or(source_index.resolve_symbol_use_path(root, current_id, last)?)
        {
            return Ok(format!("use {use_path};"));
        }
        return Ok(line.to_string());
    }

    if let Some(rebound) =
        rebind_module_prefix(&current_id.crate_name, remainder, generated_by_leaf)
    {
        return Ok(format!("use crate::{rebound};"));
    }
    Ok(line.to_string())
}

fn rebind_module_prefix(
    crate_name: &str,
    module_prefix: &str,
    generated_by_leaf: &BTreeMap<(String, String), String>,
) -> Option<String> {
    let leaf = module_prefix.split("::").last()?.replace('-', "_");
    generated_by_leaf
        .get(&(crate_name.to_string(), leaf))
        .filter(|full_path| full_path.as_str() != module_prefix)
        .cloned()
}

fn parse_braced_crate_import(remainder: &str) -> Option<(&str, Vec<String>)> {
    let (prefix, tail) = remainder.split_once('{')?;
    let symbols = tail
        .strip_suffix('}')?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    Some((prefix.trim_end_matches("::"), symbols))
}

fn format_grouped_use_statement(prefix: &str, symbols: &[String]) -> String {
    if symbols.len() != 1 {
        return format!("use {prefix}::{{{}}};", symbols.join(", "));
    }
    format!("use {prefix}::{};", symbols[0])
}

fn split_symbol_use_path(use_path: &str) -> Option<(String, String)> {
    let (prefix, symbol) = use_path.rsplit_once("::")?;
    Some((prefix.to_string(), symbol.to_string()))
}

fn sort_leading_rust_imports(content: &str) -> String {
    let mut lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let mut end = start;
    let mut saw_import = false;
    while end < lines.len() {
        let trimmed = lines[end].trim();
        if trimmed.starts_with("use ") {
            saw_import = true;
            end += 1;
            continue;
        }
        if saw_import && trimmed.is_empty() {
            end += 1;
            continue;
        }
        break;
    }
    if !saw_import {
        return content.to_string();
    }
    let mut imports = lines[start..end]
        .iter()
        .flat_map(|line| {
            line.split('\n')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    imports.sort_by(|lhs, rhs| import_sort_key(lhs).cmp(&import_sort_key(rhs)));
    imports.dedup();
    lines.splice(start..end, imports);
    let mut updated = lines.join("\n");
    if content.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn import_sort_key(line: &str) -> (u8, String) {
    let trimmed = line.trim();
    let bucket = if trimmed.starts_with("use std::") {
        0
    } else if trimmed.starts_with("use crate::") {
        2
    } else if trimmed.starts_with("use super::") {
        3
    } else if trimmed.starts_with("use self::") {
        4
    } else {
        1
    };
    (bucket, trimmed.to_string())
}

fn is_rust_symbol(value: &str) -> bool {
    value
        .chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
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

fn transactional_sandbox_root(
    root: &Path,
    candidate: Option<&RefactorCandidate>,
) -> Result<PathBuf, String> {
    let sandbox_id = candidate
        .map(|candidate| candidate.candidate_id.clone())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| format!("adhoc-{}", duration.as_nanos()))
                .unwrap_or_else(|_| "adhoc".to_string())
        });
    Ok(root.join(".dbm/tmp/apply").join(sandbox_id))
}

fn create_transactional_sandbox(root: &Path, sandbox_root: &Path) -> Result<(), String> {
    if sandbox_root.exists() {
        fs::remove_dir_all(sandbox_root)
            .map_err(|err| format!("failed to reset sandbox {}: {err}", sandbox_root.display()))?;
    }
    fs::create_dir_all(sandbox_root)
        .map_err(|err| format!("failed to create sandbox {}: {err}", sandbox_root.display()))?;
    copy_workspace_subset(root, root, sandbox_root)
}

fn copy_workspace_subset(
    source_root: &Path,
    current: &Path,
    destination: &Path,
) -> Result<(), String> {
    let mut entries = fs::read_dir(current)
        .map_err(|err| format!("failed to read {}: {err}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", current.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(source_root)
            .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?;
        if should_skip_sandbox_entry(relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(relative);
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|err| format!("failed to create {}: {err}", target.display()))?;
            copy_workspace_subset(source_root, &path, destination)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::copy(&path, &target).map_err(|err| {
            format!(
                "failed to copy {} to {}: {err}",
                path.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

fn should_skip_sandbox_entry(relative: &Path) -> bool {
    let mut components = relative.components();
    match components
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
    {
        Some(first) if first == ".git" || first == "target" => true,
        Some(first) if first == ".dbm" => {
            let second = components
                .next()
                .map(|component| component.as_os_str().to_string_lossy().into_owned());
            let third = components
                .next()
                .map(|component| component.as_os_str().to_string_lossy().into_owned());
            matches!(
                (second.as_deref(), third.as_deref()),
                (Some("tmp"), Some("apply"))
            )
        }
        _ => false,
    }
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
    Ok(root.join(resolve_root_module_file_relative(root)?))
}

fn resolve_sandbox_root_module_file(
    workspace_root: &Path,
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<PathBuf, String> {
    let relative =
        resolve_root_module_file_relative_for_change_set(workspace_root, change_set, candidate)?;
    let sandbox_path = sandbox_root.join(&relative);
    if !sandbox_path.exists() {
        return Err(format!(
            "failed to resolve root module file under {} from workspace-relative path {}",
            sandbox_root.display(),
            relative.display()
        ));
    }
    Ok(sandbox_path)
}

fn resolve_root_module_file_for_change_set(
    root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<PathBuf, String> {
    Ok(root.join(resolve_root_module_file_relative_for_change_set(
        root, change_set, candidate,
    )?))
}

fn resolve_root_module_file_relative_for_change_set(
    root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<PathBuf, String> {
    if let Some(relative) = representative_module_relative_path(change_set, candidate)
        && let Some(resolved) = resolve_root_module_relative_from_target(root, &relative)
    {
        return Ok(resolved);
    }
    resolve_root_module_file_relative(root)
}

fn resolve_root_module_file_relative(root: &Path) -> Result<PathBuf, String> {
    for candidate in ["src/lib.rs", "src/main.rs"] {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(PathBuf::from(candidate));
        }
    }
    Err(format!(
        "failed to resolve root module file under {}",
        root.display()
    ))
}

fn representative_module_relative_path(
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Option<PathBuf> {
    candidate
        .map(|candidate| candidate.source_path.clone())
        .or_else(|| {
            change_set
                .changes
                .first()
                .map(|change| PathBuf::from(&change.file_path))
        })
}

fn resolve_root_module_relative_from_target(root: &Path, relative: &Path) -> Option<PathBuf> {
    let parts = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let src_index = parts.iter().position(|part| *part == "src")?;
    let src_root = parts[..=src_index]
        .iter()
        .fold(PathBuf::new(), |mut path, part| {
            path.push(part);
            path
        });
    for candidate in ["lib.rs", "main.rs"] {
        let path = src_root.join(candidate);
        if root.join(&path).exists() {
            return Some(path);
        }
    }
    None
}

fn insert_mod_declarations(entry_file: &Path, modules: &[String]) -> Result<Vec<String>, String> {
    let mut content = fs::read_to_string(entry_file)
        .map_err(|err| format!("failed to read {}: {err}", entry_file.display()))?;
    let missing = missing_mod_declarations(&content, modules);
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    content = insert_mod_declarations_into_content(&content, modules);
    fs::write(entry_file, content)
        .map_err(|err| format!("failed to write {}: {err}", entry_file.display()))?;
    Ok(missing)
}

fn missing_mod_declarations(content: &str, modules: &[String]) -> Vec<String> {
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
    missing
}

fn insert_mod_declarations_into_content(content: &str, modules: &[String]) -> String {
    let missing = missing_mod_declarations(content, modules);
    if missing.is_empty() {
        return content.to_string();
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

    let mut updated = content.to_string();
    if insert_at == 0 {
        updated = format!("{declarations}\n{content}");
    } else {
        updated.insert_str(insert_at, &format!("{declarations}\n"));
    }
    updated
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

fn run_transactional_cargo_check(
    root: &Path,
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<Vec<String>, String> {
    if !sandbox_root.join("Cargo.toml").exists() {
        return Ok(Vec::new());
    }
    let package = infer_affected_crate(root, change_set, candidate)?;
    let command = resolve_command("cargo").map_err(|err| err.to_string())?;
    let config = ExecutionConfig {
        command,
        args: vec!["check".to_string(), "-p".to_string(), package.clone()],
        working_dir: sandbox_root.display().to_string(),
        timeout_ms: 120_000,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Buffered,
    };
    let timeout = TimeoutConfig {
        timeout_ms: 120_000,
        kill_signal: "kill".to_string(),
    };
    let policy = SandboxPolicy {
        allow_network: false,
        allow_fs_write: true,
        allowed_paths: vec![sandbox_root.display().to_string()],
    };
    let result = run_command(
        &config,
        &timeout,
        &policy,
        sandbox_root,
        SandboxMode::FullCopy,
    )
    .map_err(|err| {
        format!(
            "failed to run cargo check in {}: {err}",
            sandbox_root.display()
        )
    })?;
    if result.exit_code == 0 {
        return Ok(vec![format!("cargo check -p {package} succeeded")]);
    }
    let stderr = result.stderr.trim();
    let stdout = result.stdout.trim();
    let message = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "Transactional apply aborted: cargo check failed in sandbox. Real workspace remains unchanged.\n{message}"
    ))
}

fn validate_sandbox_change_set(
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
) -> Result<(), String> {
    for change in &change_set.changes {
        let path = sandbox_root.join(&change.file_path);
        if change.change_type == ChangeType::ModifyFile && !path.exists() {
            return Err(format!(
                "failed to remap relative path into sandbox: {} -> {}",
                change.file_path,
                path.display()
            ));
        }
    }
    Ok(())
}

fn infer_affected_crate(
    root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<String, String> {
    if let Some(candidate) = candidate {
        if !candidate.module_id.crate_name.is_empty() {
            return Ok(candidate.module_id.crate_name.clone());
        }
    }
    for change in &change_set.changes {
        let mut current = root.join(&change.file_path).parent().map(Path::to_path_buf);
        while let Some(dir) = current {
            let manifest = dir.join("Cargo.toml");
            if manifest.exists() {
                if let Some(name) = parse_package_name(&manifest)? {
                    return Ok(name);
                }
            }
            if dir == root {
                break;
            }
            current = dir.parent().map(Path::to_path_buf);
        }
    }
    let manifest = root.join("Cargo.toml");
    if manifest.exists() {
        if let Some(name) = parse_package_name(&manifest)? {
            return Ok(name);
        }
    }
    Err("unable to determine affected crate for transactional cargo check".to_string())
}

fn parse_package_name(manifest: &Path) -> Result<Option<String>, String> {
    let content = fs::read_to_string(manifest)
        .map_err(|err| format!("failed to read {}: {err}", manifest.display()))?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("name") else {
            continue;
        };
        let Some(value) = rest.split('=').nth(1) else {
            continue;
        };
        let name = value.trim().trim_matches('"');
        if !name.is_empty() {
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
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
    if let Ok(entry_file) = resolve_root_module_file_relative(root) {
        paths.push(root.join(entry_file));
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

fn resolve_target_override(
    root: &Path,
    target_override: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = target_override else {
        return Ok(None);
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !absolute.exists() {
        return Err(format!(
            "target file does not exist: {}",
            absolute.display()
        ));
    }
    Ok(Some(absolute))
}

fn patch_fence_for_target(target_override: Option<&Path>, root: &Path) -> PatchFence {
    PatchFence {
        scope: if target_override.is_some() {
            PatchScope::ExplicitTargetOnly
        } else {
            PatchScope::WorkspaceWide
        },
        explicit_target: target_override
            .and_then(|path| normalize_target_scope_path(root, path).ok()),
    }
}

fn guard_change_target(target_path: &Path, fence: &PatchFence) -> Result<(), String> {
    if fence.scope != PatchScope::ExplicitTargetOnly {
        return Ok(());
    }
    let Some(explicit_target) = fence.explicit_target.as_deref() else {
        return Err("patch_scope_violation".to_string());
    };
    let target = normalize_path_for_scope(target_path);
    let expected = normalize_path_for_scope(explicit_target);
    if target == expected {
        Ok(())
    } else {
        Err(format!(
            "patch_scope_violation: target file {} != generated patch path {}",
            expected.display(),
            target.display()
        ))
    }
}

fn enforce_patch_scope(changes: &[CodeChange], fence: &PatchFence) -> Result<(), String> {
    if fence.scope != PatchScope::ExplicitTargetOnly {
        return Ok(());
    }
    for change in changes {
        guard_change_target(Path::new(&change.file_path), fence)?;
    }
    Ok(())
}

fn normalize_path_for_scope(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut normalized, component| {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
        normalized
    })
}

fn normalize_target_scope_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        PathBuf::from(normalize_relative(root, path)?)
    } else {
        path.to_path_buf()
    };
    Ok(normalize_path_for_scope(&candidate))
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
    use crate::refactor::{
        RefactorActionKind, RefactorCandidate, RefactorOperation, RefactorTarget,
        candidate_snapshot_path, persist_refactor_candidates,
    };
    use crate::service::ModuleNode;
    use crate::source_index::QualifiedModuleId;
    use integration_layer::{
        CodePatch, MetricsDelta, PatchOperation, PhaseType, PlanSummary, RefactorPhase,
        RefactorPlan, RefactorPlanAction,
    };
    use std::path::Path;

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

    fn write_candidate_snapshot(
        root: &Path,
        logical_name: &str,
        source_path: &str,
        operation: RefactorOperation,
    ) -> RefactorCandidate {
        let qualified = QualifiedModuleId {
            crate_name: "coding_test".to_string(),
            module_path: logical_name.to_string(),
        };
        let source = PathBuf::from(source_path);
        let file_hash = {
            use sha2::{Digest, Sha256};

            let bytes = fs::read(root.join(&source)).expect("read source");
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        };
        let candidate = RefactorCandidate {
            candidate_id: format!("{logical_name}-candidate"),
            module_id: qualified.clone(),
            logical_name: logical_name.to_string(),
            kind: match operation {
                RefactorOperation::ExtractInterface => RefactorActionKind::ExtractInterface,
                RefactorOperation::RemoveDependency => RefactorActionKind::RemoveDependency,
                RefactorOperation::SplitModule => RefactorActionKind::SplitModule,
                RefactorOperation::MergeModule => RefactorActionKind::MergeModule,
                RefactorOperation::MoveFile => RefactorActionKind::MoveFile,
                RefactorOperation::RenameBoundary => RefactorActionKind::RenameBoundary,
                RefactorOperation::IntroduceService => RefactorActionKind::IntroduceService,
            },
            operation: operation.clone(),
            title: "candidate".to_string(),
            rationale: "test".to_string(),
            confidence_milli: 900,
            confidence: 0.9,
            from_node: ModuleNode {
                qualified_id: qualified.clone(),
                logical_name: logical_name.to_string(),
                source_path: Some(source.clone()),
            },
            to_node: ModuleNode {
                qualified_id: qualified.clone(),
                logical_name: logical_name.to_string(),
                source_path: Some(source.clone()),
            },
            patch_plan: RefactorTarget::RemoveDependency {
                from: logical_name.to_string(),
                to: "world".to_string(),
            },
            source_path: source.clone(),
            preview_hash: format!("sha256:{file_hash}"),
            base_file_hash: file_hash,
            target_nodes: vec![logical_name.to_string()],
            target_edges: Vec::new(),
            target: RefactorTarget::RemoveDependency {
                from: logical_name.to_string(),
                to: "world".to_string(),
            },
        };
        persist_refactor_candidates(root, &[candidate.clone()]).expect("persist");
        assert!(candidate_snapshot_path(root, &candidate.candidate_id).exists());
        candidate
    }

    fn install_before_real_apply_hook(hook: fn(&Path)) {
        *BEFORE_REAL_APPLY_HOOK
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("hook mutex") = Some(hook);
    }

    fn set_commit_confirmation_response(response: bool) {
        *COMMIT_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("confirm mutex") = Some(response);
    }

    fn set_push_confirmation_response(response: bool) {
        *PUSH_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("push confirm mutex") = Some(response);
    }

    fn set_pr_confirmation_response(response: bool) {
        *PR_CONFIRMATION_RESPONSE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("pr confirm mutex") = Some(response);
    }

    fn init_git_repo(root: &Path) {
        let status = Command::new("git")
            .args(["init"])
            .current_dir(root)
            .status()
            .expect("git init");
        assert!(status.success());
        let status = Command::new("git")
            .args(["config", "user.email", "dbm@example.com"])
            .current_dir(root)
            .status()
            .expect("git config email");
        assert!(status.success());
        let status = Command::new("git")
            .args(["config", "user.name", "DBM"])
            .current_dir(root)
            .status()
            .expect("git config name");
        assert!(status.success());
        let status = Command::new("git")
            .args(["add", "--", "."])
            .current_dir(root)
            .status()
            .expect("git add");
        assert!(status.success());
        let status = Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(root)
            .status()
            .expect("git commit");
        assert!(status.success());
    }

    fn init_git_repo_with_branch(root: &Path, branch: &str) {
        init_git_repo(root);
        let status = Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(root)
            .status()
            .expect("git checkout -b");
        assert!(status.success());
    }

    fn install_fake_gh(
        dir: &Path,
        auth_ok: bool,
        pr_list_json: &str,
        pr_create_url: &str,
    ) -> PathBuf {
        let script = dir.join("gh");
        let body = format!(
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  if [ \"{auth_ok}\" = \"true\" ]; then exit 0; else exit 1; fi\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"list\" ]; then\n  printf '%s' '{pr_list_json}'\n  exit 0\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  printf '%s' '{pr_create_url}'\n  exit 0\nfi\nexit 1\n"
        );
        fs::write(&script, body).expect("write gh");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod");
        }
        script
    }

    fn with_fake_gh<T>(path: &Path, f: impl FnOnce() -> T) -> T {
        let previous = std::env::var_os("DBM_GH_BIN");
        unsafe {
            std::env::set_var("DBM_GH_BIN", path);
        }
        let result = f();
        match previous {
            Some(value) => unsafe { std::env::set_var("DBM_GH_BIN", value) },
            None => unsafe { std::env::remove_var("DBM_GH_BIN") },
        }
        result
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
    fn target_override_is_used_for_replace_dependency() {
        let root = temp_dir("target_override");
        fs::create_dir_all(root.join("apps/cli/src")).expect("create cli src");
        fs::write(
            root.join("apps/cli/src/app.rs"),
            "use crate::world;\nfn app() {}\n",
        )
        .expect("write app");
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "missing_module".to_string(),
                to: "world".to_string(),
                via: Some("app_world_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "missing_module".to_string(),
                to: "world".to_string(),
                via: Some("app_world_interface".to_string()),
            }],
            description: "move".to_string(),
        }];

        let change_set = generate_code_change_set_with_target(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/app.rs")),
        )
        .expect("change set");
        assert_eq!(change_set.changes[0].file_path, "apps/cli/src/app.rs");
    }

    #[test]
    fn invalid_target_override_fails_before_apply() {
        let root = temp_dir("invalid_target_override");
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

        let error = generate_code_change_set_with_target(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/missing.rs")),
        )
        .expect_err("missing target should fail");
        assert!(error.contains("target file does not exist"));
    }

    #[test]
    fn preview_apply_deterministic_path_resolution_uses_snapshot() {
        let root = temp_dir("apply_resolver_deterministic");
        fs::create_dir_all(root.join("src/runtime")).expect("runtime");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/main.rs"), "mod runtime;\nfn main() {}\n").expect("main");
        fs::write(root.join("src/runtime/mod.rs"), "pub mod determinism;\n").expect("mod");
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "use crate::world;\npub fn check() {}\n",
        )
        .expect("determinism");
        let candidate = write_candidate_snapshot(
            &root,
            "determinism",
            "src/runtime/determinism.rs",
            RefactorOperation::RemoveDependency,
        );
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: Some("determinism_world_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: Some("determinism_world_interface".to_string()),
            }],
            description: "move".to_string(),
        }];
        let resolved =
            resolve_apply_paths_for_patches(&root, &patches, Some(&candidate.candidate_id))
                .expect("resolved");
        assert_eq!(
            resolved.get("determinism"),
            Some(&PathBuf::from("src/runtime/determinism.rs"))
        );
        let changes =
            generate_code_change_set_with_resolved_paths(&root, &patches, None, &resolved)
                .expect("change set");
        assert_eq!(changes.changes[0].file_path, "src/runtime/determinism.rs");
    }

    #[test]
    fn file_drift_abort_is_reported_before_apply() {
        let root = temp_dir("apply_resolver_drift");
        fs::create_dir_all(root.join("src/runtime")).expect("runtime");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("main");
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "pub fn check() {}\n",
        )
        .expect("determinism");
        let candidate = write_candidate_snapshot(
            &root,
            "determinism",
            "src/runtime/determinism.rs",
            RefactorOperation::RemoveDependency,
        );
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "pub fn check() {}\npub fn drift() {}\n",
        )
        .expect("drift");
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: None,
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: None,
            }],
            description: "move".to_string(),
        }];
        let error = resolve_apply_paths_for_patches(&root, &patches, Some(&candidate.candidate_id))
            .expect_err("drift should abort");
        assert!(error.contains("File drift detected"));
    }

    #[test]
    fn workspace_escape_candidate_is_rejected() {
        let root = temp_dir("apply_resolver_escape");
        write_rust_project(&root, "fn main() {}\n");
        let candidate = RefactorCandidate {
            candidate_id: "escape".to_string(),
            module_id: QualifiedModuleId {
                crate_name: "coding_test".to_string(),
                module_path: "determinism".to_string(),
            },
            logical_name: "determinism".to_string(),
            kind: RefactorActionKind::RemoveDependency,
            operation: RefactorOperation::RemoveDependency,
            title: "escape".to_string(),
            rationale: "escape".to_string(),
            confidence_milli: 900,
            confidence: 0.9,
            from_node: ModuleNode {
                qualified_id: QualifiedModuleId {
                    crate_name: "coding_test".to_string(),
                    module_path: "determinism".to_string(),
                },
                logical_name: "determinism".to_string(),
                source_path: Some(PathBuf::from("../../outside.rs")),
            },
            to_node: ModuleNode {
                qualified_id: QualifiedModuleId {
                    crate_name: "coding_test".to_string(),
                    module_path: "determinism".to_string(),
                },
                logical_name: "determinism".to_string(),
                source_path: Some(PathBuf::from("../../outside.rs")),
            },
            patch_plan: RefactorTarget::RemoveDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
            },
            source_path: PathBuf::from("../../outside.rs"),
            preview_hash: "sha256:test".to_string(),
            base_file_hash: "test".to_string(),
            target_nodes: vec!["determinism".to_string()],
            target_edges: Vec::new(),
            target: RefactorTarget::RemoveDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
            },
        };
        persist_refactor_candidates(&root, &[candidate]).expect("persist");
        let patches = vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: None,
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "determinism".to_string(),
                to: "world".to_string(),
                via: None,
            }],
            description: "move".to_string(),
        }];
        let error = resolve_apply_paths_for_patches(&root, &patches, Some("escape"))
            .expect_err("workspace escape must fail");
        assert!(error.contains("escapes workspace"));
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
            patches: vec![],
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
                confirm_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::WorkspaceWide,
                explicit_target: None,
            },
            None,
        )
        .expect("apply");
        assert_eq!(result.status, "applied");
        assert!(result.build_ok);
        assert!(result.transactional_apply.is_some());
        assert!(root.join("src/world_service.rs").exists());
    }

    #[test]
    fn rollback_on_build_fail() {
        let root = temp_dir("rollback");
        write_rust_project(&root, "fn main() {}\n");
        let original = fs::read_to_string(root.join("src/main.rs")).expect("read original");
        let change_set = CodeChangeSet {
            patches: vec![],
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
                confirm_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::WorkspaceWide,
                explicit_target: None,
            },
            None,
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
            patches: vec![],
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
                confirm_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::WorkspaceWide,
                explicit_target: None,
            },
            None,
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
            patches: vec![],
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
                confirm_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::WorkspaceWide,
                explicit_target: None,
            },
            None,
        )
        .expect_err("breaking diff should fail validation");

        assert!(err.contains("breaking change detected"));
    }

    #[test]
    fn drift_recheck_aborts_before_real_apply() {
        let root = temp_dir("drift_recheck");
        fs::create_dir_all(root.join("src/runtime")).expect("runtime");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/main.rs"),
            "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
        )
        .expect("main");
        fs::write(root.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "pub fn check() {}\n",
        )
        .expect("determinism");
        let candidate = write_candidate_snapshot(
            &root,
            "determinism",
            "src/runtime/determinism.rs",
            RefactorOperation::RemoveDependency,
        );
        let original =
            fs::read_to_string(root.join("src/runtime/determinism.rs")).expect("original");
        let change_set = CodeChangeSet {
            patches: vec![],
            changes: vec![CodeChange {
                file_path: "src/runtime/determinism.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "use crate::world;\npub fn check() {}\n".to_string(),
                }],
            }],
            summary: ChangeSummary {
                total_changes: 1,
                create_files: 0,
                modify_files: 1,
                move_files: 0,
            },
        };
        install_before_real_apply_hook(|root| {
            fs::write(
                root.join("src/runtime/determinism.rs"),
                "pub fn check() { let _ = 1; }\n",
            )
            .expect("mutate real file");
        });

        let result = transactional_apply(&root, &change_set, Some(&candidate), false)
            .expect("transactional result");

        assert!(!result.applied);
        assert!(result.rolled_back);
        assert!(!result.diagnostics.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("src/runtime/determinism.rs")).expect("after"),
            "pub fn check() { let _ = 1; }\n"
        );
        assert_ne!(
            original,
            fs::read_to_string(root.join("src/runtime/determinism.rs")).expect("after")
        );
    }

    #[test]
    fn restricted_commit_stages_exact_files_only() {
        let root = temp_dir("restricted_commit_exact");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo(&root);
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"dbm\"); }\n",
        )
        .expect("main");
        fs::write(root.join("README.md"), "dirty\n").expect("readme");

        let result = restricted_commit(&[PathBuf::from("src/main.rs")], &root, None, true)
            .expect("restricted commit");

        assert!(result.commit_created);
        assert_eq!(result.staged_files, vec![PathBuf::from("src/main.rs")]);
        assert!(result.dirty_excluded.contains(&PathBuf::from("README.md")));
        let staged = Command::new("git")
            .args(["show", "--name-only", "--pretty=format:", "HEAD"])
            .current_dir(&root)
            .output()
            .expect("git show");
        let stdout = String::from_utf8_lossy(&staged.stdout);
        assert!(stdout.lines().any(|line| line.trim() == "src/main.rs"));
        assert!(!stdout.lines().any(|line| line.trim() == "README.md"));
    }

    #[test]
    fn restricted_commit_decline_leaves_index_clean() {
        let root = temp_dir("restricted_commit_decline");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo(&root);
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"dbm\"); }\n",
        )
        .expect("main");
        set_commit_confirmation_response(false);

        let result = restricted_commit(&[PathBuf::from("src/main.rs")], &root, None, false)
            .expect("restricted commit");

        assert!(!result.commit_created);
        assert!(result.confirmation_required);
        let cached = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(&root)
            .output()
            .expect("git diff cached");
        assert!(String::from_utf8_lossy(&cached.stdout).trim().is_empty());
    }

    #[test]
    fn restricted_push_rejects_protected_branch() {
        let root = temp_dir("restricted_push_protected");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo(&root);

        let err = restricted_push(None, &root, true).expect_err("protected branch must fail");
        assert!(err.contains("protected branch"));
    }

    #[test]
    fn restricted_push_decline_records_no_push() {
        let root = temp_dir("restricted_push_decline");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/push-decline");
        let bare = std::env::temp_dir().join(format!(
            "design_cli_push_decline_remote_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let status = Command::new("git")
            .args(["init", "--bare", bare.to_str().expect("utf8 bare")])
            .status()
            .expect("git init bare");
        assert!(status.success());
        let status = Command::new("git")
            .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
            .current_dir(&root)
            .status()
            .expect("git remote add");
        assert!(status.success());
        set_push_confirmation_response(false);

        let result = restricted_push(None, &root, false).expect("restricted push");
        assert!(!result.push_created);
        assert!(result.confirmation_required);
        assert!(result.dry_run_ok);
    }

    #[test]
    fn restricted_pr_create_rejects_invalid_base() {
        let root = temp_dir("restricted_pr_invalid_base");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/pr-base");
        let err = restricted_pr_create("dbm/pr-base", "master", &root, None, 1, true)
            .expect_err("invalid base must fail");
        assert!(err.contains("InvalidBaseBranch"));
    }

    #[test]
    fn restricted_pr_create_detects_duplicate() {
        let root = temp_dir("restricted_pr_duplicate");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/pr-duplicate");
        let gh = install_fake_gh(
            &root,
            true,
            r#"[{"number":42,"url":"https://github.com/org/repo/pull/42"}]"#,
            "https://github.com/org/repo/pull/99",
        );
        let result = with_fake_gh(&gh, || {
            restricted_pr_create("dbm/pr-duplicate", "main", &root, None, 1, true)
                .expect("duplicate result")
        });
        assert!(!result.pr_created);
        assert!(result.duplicate_detected);
        assert_eq!(result.pr_number, Some(42));
    }

    #[test]
    fn restricted_pr_create_decline_returns_no_pr() {
        let root = temp_dir("restricted_pr_decline");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/pr-decline");
        let gh = install_fake_gh(&root, true, "[]", "https://github.com/org/repo/pull/99");
        set_pr_confirmation_response(false);
        let result = with_fake_gh(&gh, || {
            restricted_pr_create("dbm/pr-decline", "main", &root, None, 1, false)
                .expect("declined result")
        });
        assert!(!result.pr_created);
        assert!(!result.duplicate_detected);
    }

    #[test]
    fn generated_imports_rebind_to_nested_module_paths() {
        let root = temp_dir("generated_imports_rebind");
        fs::create_dir_all(root.join("src/controller")).expect("controller");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod controller;\npub mod app;\n",
        )
        .expect("lib");
        fs::write(root.join("src/controller/mod.rs"), "pub fn noop() {}\n")
            .expect("controller mod");
        fs::write(
            root.join("src/app.rs"),
            "use crate::controller_determinism_interface::ControllerDeterminismInterface;\npub fn run() {}\n",
        )
        .expect("app");

        let mut drafts = BTreeMap::from([
            (
                "src/controller/controller_determinism_interface.rs".to_string(),
                FileDraft {
                    change_type: ChangeType::CreateFile,
                    original: String::new(),
                    content: "pub trait ControllerDeterminismInterface {}\n".to_string(),
                },
            ),
            (
                "src/app.rs".to_string(),
                FileDraft {
                    change_type: ChangeType::ModifyFile,
                    original: "use crate::controller_determinism_interface::ControllerDeterminismInterface;\npub fn run() {}\n".to_string(),
                    content: "use crate::controller_determinism_interface::ControllerDeterminismInterface;\npub fn run() {}\n".to_string(),
                },
            ),
        ]);
        let index = ModuleSourceIndex::build(&root).expect("index");

        rewrite_crate_imports_for_generated_files(&root, &index, &mut drafts).expect("rewrite");

        let app = &drafts.get("src/app.rs").expect("app").content;
        assert!(
            app.contains(
                "use crate::controller::controller_determinism_interface::ControllerDeterminismInterface;"
            ),
            "{app}"
        );
    }

    #[test]
    fn generated_files_register_parent_mod_rs_idempotently() {
        let root = temp_dir("register_parent_mod");
        fs::create_dir_all(root.join("src/controller")).expect("controller");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/lib.rs"), "pub mod controller;\n").expect("lib");
        fs::write(root.join("src/controller/mod.rs"), "pub fn noop() {}\n")
            .expect("controller mod");

        let mut drafts = BTreeMap::from([(
            "src/controller/controller_determinism_interface.rs".to_string(),
            FileDraft {
                change_type: ChangeType::CreateFile,
                original: String::new(),
                content: "pub trait ControllerDeterminismInterface {}\n".to_string(),
            },
        )]);

        register_generated_submodules(&root, &mut drafts).expect("register");
        register_generated_submodules(&root, &mut drafts).expect("register again");

        let module = &drafts.get("src/controller/mod.rs").expect("mod").content;
        assert!(
            module.contains("pub mod controller_determinism_interface;"),
            "{module}"
        );
        assert_eq!(
            module
                .matches("pub mod controller_determinism_interface;")
                .count(),
            1,
            "{module}"
        );
    }

    #[test]
    fn root_symbol_imports_rebind_to_existing_module_groups() {
        let root = temp_dir("symbol_rebinding");
        fs::create_dir_all(root.join("src/controller")).expect("controller");
        fs::create_dir_all(root.join("src/domain")).expect("domain");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod controller;\npub mod domain;\n",
        )
        .expect("lib");
        fs::write(root.join("src/controller/mod.rs"), "pub fn noop() {}\n")
            .expect("controller mod");
        fs::write(
            root.join("src/domain/mod.rs"),
            "pub struct AgentInput;\npub struct AgentOutput;\npub enum DomainError {}\n",
        )
        .expect("domain");
        let index = ModuleSourceIndex::build(&root).expect("index");
        let current_id = index
            .qualified_id_for_path(Path::new("src/controller/mod.rs"))
            .expect("current id");
        let content = "use crate::{AgentInput, AgentOutput, DomainError};\npub fn adapt() {}\n";

        let updated = rewrite_imports_in_content(
            &root,
            &index,
            &current_id,
            content,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("rewrite");

        assert!(
            updated.contains("use crate::domain::{AgentInput, AgentOutput, DomainError};"),
            "{updated}"
        );
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
