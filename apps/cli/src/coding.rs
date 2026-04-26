use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Write, stdin, stdout};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use integration_layer::{CodePatch, PatchOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::nl::r#loop::{LoopOrigin, LoopPromotable, PromotionError, RepairLoopContext};
use crate::refactor::{
    ApplyResolverError, PatchScope, RefactorCandidate, RefactorOperation,
    apply_resolver_error_message, load_matching_refactor_candidate, load_refactor_candidate,
    validate_apply_candidate,
};
use crate::runner::{fixed_env, resolve_command};
use crate::service::{MutationOperation, MutationPlan, MutationStrategy};
use crate::source_index::{ApplyTargetResolution, ModuleSourceIndex, QualifiedModuleId};

const CURRENT_RESOLVER_VERSION: &str = "3";
const MAX_HUNK_LINES: usize = 120;
const PREFERRED_HUNK_LINES: usize = 40;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CanonicalizationTelemetry {
    pub normalization_path_used: bool,
    pub normalization_issue_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalization_issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChangeSet {
    /// Canonical narrowed patches: post bootstrap-policy + semantic-cluster prune.
    /// Count is the candidate log; must equal the set that produced `changes`.
    pub patches: Vec<CodePatch>,
    pub changes: Vec<CodeChange>,
    pub summary: ChangeSummary,
    /// Truth source for representative target resolution. Set from
    /// `MutationResolutionTelemetry::canonical_target_path` when available.
    #[serde(default)]
    pub canonical_target: Option<PathBuf>,
}

const UNIFIED_DIFF_CONTEXT_LINES: usize = 3;
pub const MAX_UNIFIED_DIFF_PREVIEW_LINES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnifiedDiffPreview {
    pub files: Vec<CodeDiff>,
    pub summary: UnifiedDiffSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeDiff {
    pub file: PathBuf,
    pub before_label: String,
    pub after_label: String,
    pub hunks: Vec<Hunk>,
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLine {
    Added(String),
    Removed(String),
    Context(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnifiedDiffSummary {
    pub file_count: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub skipped_binary_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeIrProgram {
    source: String,
    imports: Vec<RustImport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RustImport {
    line_index: usize,
    imported_symbols: Vec<String>,
    wildcard: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorRule {
    RemoveUnusedImports,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorDiffResult {
    pub before_ir: CodeIrProgram,
    pub after_ir: CodeIrProgram,
    pub after_code: String,
    pub diff: Option<String>,
    pub removed_lines: usize,
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
        target_file: Option<PathBuf>,
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
    pub prompt_commit: bool,
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
    pub canonical_target_path: Option<String>,
    pub resolution_pipeline_hits: u64,
    pub degraded_resolution_hits: u64,
    pub stale_artifact_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MutationResolutionTelemetry {
    pub canonical_target_path: Option<PathBuf>,
    pub resolution_pipeline_hits: u64,
    pub degraded_resolution_hits: u64,
    pub stale_artifact_detected: bool,
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

impl LoopPromotable for TransactionalApplyResult {
    fn promote(self) -> anyhow::Result<RepairLoopContext> {
        let changed_files = self.modified_files;
        let diagnostics = self.diagnostics;
        let rollback_token = Some(self.sandbox_path.display().to_string());

        if !diagnostics.is_empty() {
            return Ok(RepairLoopContext {
                target: changed_files.first().cloned(),
                logical_node: None,
                changed_files,
                diagnostics,
                rollback_token,
                previous_strategy: None,
                origin: LoopOrigin::Coding,
            });
        }

        if changed_files.is_empty() {
            return Err(PromotionError::MissingDiagnostics.into());
        }

        Ok(RepairLoopContext {
            target: changed_files.first().cloned(),
            logical_node: None,
            changed_files,
            diagnostics,
            rollback_token: None,
            previous_strategy: None,
            origin: LoopOrigin::Coding,
        })
    }
}

#[cfg(test)]
mod loop_promotion_tests {
    use super::*;
    use crate::nl::r#loop::{LoopEntryState, LoopPromotable};

    #[test]
    fn transactional_apply_with_changed_files_promotes_to_verify() {
        let context = TransactionalApplyResult {
            applied: true,
            build_ok: true,
            rolled_back: false,
            sandbox_path: PathBuf::from("/tmp/dbm-sandbox"),
            modified_files: vec![PathBuf::from("apps/cli/src/repl.rs")],
            diagnostics: Vec::new(),
            elapsed_ms: 10,
            sandbox_elapsed_ms: 5,
            cargo_check_ms: 3,
            cleanup_ms: 2,
            cleanup_ok: true,
            rollback_count: 0,
        }
        .promote()
        .expect("successful coding promotion should succeed");
        assert_eq!(context.origin, LoopOrigin::Coding);
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::Verify
        );
    }

    #[test]
    fn transactional_apply_with_diagnostics_promotes_to_retry_decision() {
        let context = TransactionalApplyResult {
            applied: false,
            build_ok: false,
            rolled_back: true,
            sandbox_path: PathBuf::from("/tmp/dbm-sandbox"),
            modified_files: vec![PathBuf::from("apps/cli/src/repl.rs")],
            diagnostics: vec![String::from("cargo check failed")],
            elapsed_ms: 10,
            sandbox_elapsed_ms: 5,
            cargo_check_ms: 3,
            cleanup_ms: 2,
            cleanup_ok: true,
            rollback_count: 1,
        }
        .promote()
        .expect("failing coding promotion should succeed");
        assert_eq!(context.rollback_token.as_deref(), Some("/tmp/dbm-sandbox"));
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::RetryDecision
        );
    }
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
    pub confirmation_granted: bool,
    pub dirty_excluded: Vec<PathBuf>,
    pub status_before: GitStatusSnapshot,
    pub status_after: Option<GitStatusSnapshot>,
    pub diff_preview: Vec<GitDiffEntry>,
    pub telemetry_path: Option<PathBuf>,
    pub warning: Option<String>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GitStatusSnapshot {
    pub branch_name: String,
    pub dirty_files: Vec<PathBuf>,
    pub untracked_files: Vec<PathBuf>,
    pub ahead: usize,
    pub behind: usize,
    pub conflicted: bool,
    pub detached_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GitDiffEntry {
    pub change_type: String,
    pub path: PathBuf,
    pub hunk_count: usize,
    pub line_delta: isize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocalIntegrationTelemetry {
    pub git: LocalIntegrationGitTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocalIntegrationGitTelemetry {
    pub branch: String,
    pub dirty_before: bool,
    pub dirty_after: bool,
    pub files_added: usize,
    pub commit_created: bool,
    pub commit_hash: Option<String>,
    pub confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteIntegrationTelemetry {
    pub remote: RemoteIntegrationTelemetryData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteIntegrationTelemetryData {
    pub branch: String,
    pub dry_run_ok: bool,
    pub push_ok: bool,
    pub pr_created: bool,
    pub pr_duplicate: bool,
    pub base: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SandboxCopyTelemetry {
    pub sandbox_copy: SandboxCopyTelemetryData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SandboxCopyTelemetryData {
    pub copied_files: usize,
    pub skipped_files: usize,
    pub ignored_dirs: Vec<String>,
    pub copy_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CargoResolutionTelemetry {
    pub cargo_resolution: CargoResolutionTelemetryData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CargoResolutionTelemetryData {
    pub offline: bool,
    pub lockfile_used: bool,
    pub cache_hit: bool,
    pub dependency_unavailable: Vec<String>,
    pub graceful_degradation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticRecoveryTelemetry {
    pub semantic_recovery: SemanticRecoveryTelemetryData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticRecoveryTelemetryData {
    pub error_type: String,
    pub used_rustc_help: bool,
    pub patch_family: String,
    pub green_state_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MalformedImportRecoveryTelemetry {
    pub malformed_import_recovery: MalformedImportRecoveryTelemetryData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MalformedImportRecoveryTelemetryData {
    pub file: String,
    pub imports_fixed: usize,
    pub group_normalized: bool,
    pub used_rustc_help_batch: bool,
    pub stable_preserved: bool,
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
    pub confirmation_granted: bool,
    pub telemetry_path: Option<PathBuf>,
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
    pub telemetry_path: Option<PathBuf>,
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

#[derive(Debug, Clone)]
struct SynthesizedHunk {
    start_line: usize,
    end_line: usize,
    replacement_lines: Vec<String>,
}

#[derive(Debug, Clone)]
enum LineDiffOp {
    Equal,
    Remove(String),
    Add(String),
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
        target_file: Default::default(),
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

pub fn load_mutation_plan_from_json(path: &Path) -> Result<MutationPlan, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_str(&raw).map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

pub fn load_patches_from_design_snapshot(
    root: &Path,
    path: &Path,
) -> Result<(MutationPlan, Vec<CodePatch>, MutationResolutionTelemetry), String> {
    let plan = load_mutation_plan_from_json(path)?;
    let resolution = resolve_mutation_target(root, &plan)?;
    let patches = mutation_plan_to_patches(root, &plan, &resolution)?;
    Ok((plan, patches, resolution))
}

pub fn resolve_mutation_target_path(
    root: &Path,
    plan: &MutationPlan,
) -> Result<Option<PathBuf>, String> {
    Ok(resolve_mutation_target(root, plan)?.canonical_target_path)
}

pub fn mutation_plan_to_patches(
    root: &Path,
    plan: &MutationPlan,
    resolution: &MutationResolutionTelemetry,
) -> Result<Vec<CodePatch>, String> {
    let (from, to) = if matches!(plan.operation, MutationOperation::BreakCycle) {
        resolve_edge_id_exact(root, &plan.edge_id).or_else(|_| {
            parse_edge_id(&plan.edge_id).ok_or_else(|| format!("invalid edge id: {}", plan.edge_id))
        })?
    } else {
        resolve_edge_id_exact(root, &plan.edge_id)?
    };
    let target_file = resolution.canonical_target_path.clone().unwrap_or_default();
    let patches = match (&plan.operation, &plan.strategy) {
        (MutationOperation::RemoveDependency, MutationStrategy::ExtractInterface) => {
            let description = format!("remove dependency {from} -> {to} via ports");
            vec![CodePatch {
                patch_id: format!("mutation-remove-dependency-{}", plan.edge_id),
                action: integration_layer::RefactorPlanAction::MoveDependency {
                    from: from.clone(),
                    to: "ports".to_string(),
                    via: None,
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from,
                    to: "ports".to_string(),
                    via: None,
                }],
                description,
                target_file: target_file.clone(),
            }]
        }
        (MutationOperation::RemoveDependency, MutationStrategy::ImportRebinding) => {
            let description = format!("rebind dependency {from} -> {to} into ports");
            vec![CodePatch {
                patch_id: format!("mutation-remove-dependency-{}", plan.edge_id),
                action: integration_layer::RefactorPlanAction::MoveDependency {
                    from: from.clone(),
                    to: "ports".to_string(),
                    via: None,
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from,
                    to: "ports".to_string(),
                    via: None,
                }],
                description,
                target_file: target_file.clone(),
            }]
        }
        (MutationOperation::ExtractInterface, MutationStrategy::ExtractInterface) => {
            let name = format!("{}Port", pascal_case(&from));
            vec![CodePatch {
                patch_id: format!("mutation-extract-interface-{}", plan.edge_id),
                action: integration_layer::RefactorPlanAction::IntroduceInterface {
                    between: (from.clone(), to.clone()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name,
                    between: (from, to),
                }],
                description: format!("extract interface for {}", plan.edge_id),
                target_file: target_file.clone(),
            }]
        }
        (MutationOperation::MoveDependency, MutationStrategy::BoundaryMove) => vec![CodePatch {
            patch_id: format!("mutation-move-dependency-{}", plan.edge_id),
            action: integration_layer::RefactorPlanAction::MoveDependency {
                from: from.clone(),
                to: to.clone(),
                via: Some("ports".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from,
                to,
                via: Some("ports".to_string()),
            }],
            description: format!("move dependency boundary for {}", plan.edge_id),
            target_file: target_file.clone(),
        }],
        (MutationOperation::BreakCycle, MutationStrategy::ExtractInterfaceBothSides) => {
            build_break_cycle_extract_interface_both_sides_patches(
                root,
                &resolution
                    .canonical_target_path
                    .clone()
                    .ok_or_else(|| "break_cycle requires canonical_target_file".to_string())?,
                &from,
                &to,
            )?
        }
        _ => {
            return Err(format!(
                "unsupported mutation combination: {:?} + {:?}",
                plan.operation, plan.strategy
            ));
        }
    };
    Ok(patches)
}

fn build_break_cycle_extract_interface_both_sides_patches(
    root: &Path,
    canonical_target_file: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<CodePatch>, String> {
    let from_target = canonical_target_file.to_path_buf();
    debug_assert_break_cycle_target(&from_target);
    let from_interface = format!("{}{}Interface", pascal_case(from), pascal_case(to));
    if let Some(to_target) = derive_break_cycle_peer_target(root, &from_target, to) {
        debug_assert_break_cycle_target(&to_target);
        let to_interface = format!("{}{}Interface", pascal_case(to), pascal_case(from));
        return Ok(vec![
            CodePatch {
                patch_id: format!("mutation-break-cycle-create-interface-{from}-{to}"),
                action: integration_layer::RefactorPlanAction::IntroduceInterface {
                    between: (from.to_string(), to.to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: from_interface,
                    between: (from.to_string(), to.to_string()),
                }],
                description: format!("create interface to decouple {from} from {to}"),
                target_file: from_target.clone(),
            },
            CodePatch {
                patch_id: format!("mutation-break-cycle-update-dependency-{from}-{to}"),
                action: integration_layer::RefactorPlanAction::MoveDependency {
                    from: from.to_string(),
                    to: to.to_string(),
                    via: Some("ports".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: from.to_string(),
                    to: to.to_string(),
                    via: Some("ports".to_string()),
                }],
                description: format!("reroute dependency {from} -> {to} through ports"),
                target_file: from_target,
            },
            CodePatch {
                patch_id: format!("mutation-break-cycle-create-interface-{to}-{from}"),
                action: integration_layer::RefactorPlanAction::IntroduceInterface {
                    between: (to.to_string(), from.to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: to_interface,
                    between: (to.to_string(), from.to_string()),
                }],
                description: format!("create interface to decouple {to} from {from}"),
                target_file: to_target.clone(),
            },
            CodePatch {
                patch_id: format!("mutation-break-cycle-update-dependency-{to}-{from}"),
                action: integration_layer::RefactorPlanAction::MoveDependency {
                    from: to.to_string(),
                    to: from.to_string(),
                    via: Some("ports".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: to.to_string(),
                    to: from.to_string(),
                    via: Some("ports".to_string()),
                }],
                description: format!("reroute dependency {to} -> {from} through ports"),
                target_file: to_target,
            },
        ]);
    }

    Ok(vec![
        CodePatch {
            patch_id: format!("mutation-break-cycle-create-interface-{from}-{to}"),
            action: integration_layer::RefactorPlanAction::IntroduceInterface {
                between: (from.to_string(), to.to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: from_interface.clone(),
                between: (from.to_string(), to.to_string()),
            }],
            description: format!("create interface to decouple {from} from {to}"),
            target_file: from_target.clone(),
        },
        CodePatch {
            patch_id: format!("mutation-break-cycle-update-dependency-{from}-{to}"),
            action: integration_layer::RefactorPlanAction::MoveDependency {
                from: from.to_string(),
                to: to.to_string(),
                via: Some(from_interface.clone()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: from.to_string(),
                to: to.to_string(),
                via: Some(from_interface),
            }],
            description: format!("reroute dependency {from} -> {to} through interface mediation"),
            target_file: from_target,
        },
    ])
}

fn resolve_edge_id_exact(root: &Path, edge_id: &str) -> Result<(String, String), String> {
    let analysis = crate::service::analyze_path(root)?;
    analysis
        .dependencies
        .iter()
        .find(|edge| format!("{}->{}", edge.from, edge.to) == edge_id)
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .ok_or_else(|| format!("edge id not found in design snapshot: {edge_id}"))
}

fn parse_edge_id(edge_id: &str) -> Option<(String, String)> {
    let (from, to) = edge_id.split_once("->")?;
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some((from.to_string(), to.to_string()))
}

fn resolve_mutation_target(
    root: &Path,
    plan: &MutationPlan,
) -> Result<MutationResolutionTelemetry, String> {
    let stale_artifact_detected = plan
        .resolver_version
        .as_deref()
        .map(|resolver| resolver != CURRENT_RESOLVER_VERSION)
        .unwrap_or(false);

    if let Some(source_path) = plan
        .source_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(MutationResolutionTelemetry {
            canonical_target_path: Some(normalize_target_scope_path(root, Path::new(source_path))?),
            stale_artifact_detected,
            ..MutationResolutionTelemetry::default()
        });
    }

    let (from, _) = resolve_edge_id_exact(root, &plan.edge_id)?;
    let canonical_target_path = resolve_mutation_module_path(root, &from, true);

    Ok(MutationResolutionTelemetry {
        canonical_target_path,
        resolution_pipeline_hits: 1,
        degraded_resolution_hits: if matches!(plan.operation, MutationOperation::BreakCycle) {
            0
        } else {
            1
        },
        stale_artifact_detected,
    })
}

fn resolve_mutation_module_path(
    root: &Path,
    module_name: &str,
    allow_degraded_fallback: bool,
) -> Option<PathBuf> {
    let analysis = crate::service::analyze_path(root).ok()?;
    let mut candidates = analysis
        .modules
        .iter()
        .filter(|module| module.name == module_name)
        .map(|module| PathBuf::from(module.source_path.clone()))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| (path_rank(path), path_to_sort_key(path)));
    candidates.dedup();
    let primary = candidates.into_iter().next();
    if primary.is_some() || !allow_degraded_fallback {
        return primary;
    }
    resolve_apply_target_relative(root, module_name)
}

pub fn derive_break_cycle_peer_target(
    root: &Path,
    canonical_target_file: &Path,
    peer_module: &str,
) -> Option<PathBuf> {
    let source_root = semantic_source_root_for_module(canonical_target_file)?;
    let direct = source_root.join(format!("{peer_module}.rs"));
    if root.join(&direct).exists() {
        return Some(direct);
    }
    let nested = source_root.join(peer_module).join("mod.rs");
    if root.join(&nested).exists() {
        return Some(nested);
    }
    None
}

fn semantic_source_root_for_module(source_target: &Path) -> Option<PathBuf> {
    let parent = source_target.parent()?;
    if source_target.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
        return parent.parent().map(Path::to_path_buf);
    }
    Some(parent.to_path_buf())
}

fn debug_assert_break_cycle_target(target: &Path) {
    let normalized = normalized_path_string(target);
    debug_assert!(!normalized.contains("tests/fixtures"));
    debug_assert!(!normalized.contains("debug"));
}

fn path_rank(path: &Path) -> usize {
    if is_production_src(path) {
        0
    } else if is_workspace_crate(path) {
        1
    } else if is_test_support(path) {
        2
    } else if is_tests(path) {
        3
    } else {
        4
    }
}

fn is_production_src(path: &Path) -> bool {
    let normalized = normalized_path_string(path);
    normalized.contains("/src/")
        && !normalized.contains("/tests/")
        && !normalized.contains("/fixtures/")
        && !normalized.contains("/examples/")
}

fn is_workspace_crate(path: &Path) -> bool {
    normalized_path_string(path).starts_with("crates/")
}

fn is_test_support(path: &Path) -> bool {
    let normalized = normalized_path_string(path);
    normalized.contains("/tests/support/")
        || normalized.contains("/tests/integration/support/")
        || normalized.contains("/test_support/")
}

fn is_tests(path: &Path) -> bool {
    normalized_path_string(path).contains("/tests/")
}

fn normalized_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn path_to_sort_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
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
                        target_file: (!patch.target_file.as_os_str().is_empty())
                            .then(|| patch.target_file.clone()),
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
    let file = target.file_name().and_then(|n| n.to_str()).unwrap_or("");

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
        PatchOperation::CreateInterface {
            between: (a, b), ..
        } => {
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
        PatchOperation::SplitModule {
            module,
            new_modules,
        } => {
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

pub fn patches_to_change_set(
    root: &Path,
    patches: &[CodePatch],
    target_override: Option<&Path>,
    resolved_paths: &BTreeMap<String, PathBuf>,
    canonical_target: Option<&Path>,
) -> Result<CodeChangeSet, String> {
    let mut change_set = generate_code_change_set_with_resolved_paths(
        root,
        patches,
        target_override,
        resolved_paths,
    )?;
    change_set.canonical_target = canonical_target
        .map(|path| normalize_target_scope_path(root, path))
        .transpose()?
        .or_else(|| {
            canonical_target_fallback(
                root,
                target_override,
                &change_set.changes,
                &change_set.patches,
            )
        });
    Ok(change_set)
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
    let patches =
        drop_unstable_interface_synthesis_patches(root, &source_index, &patches, resolved_paths)?;
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
    if fence.scope != PatchScope::ExplicitTargetOnly
        || drafts
            .values()
            .any(|draft| draft.change_type == ChangeType::CreateFile)
    {
        rewrite_crate_imports_for_created_drafts(root, &source_index, &mut drafts)?;
        register_generated_submodules(root, &mut drafts)?;
    }

    let mut changes = drafts
        .into_iter()
        .filter(|(_, draft)| draft.original != draft.content)
        .map(|(file_path, draft)| CodeChange {
            hunks: synthesize_change_hunks(
                &file_path,
                &draft.original,
                &draft.content,
                &draft.change_type,
            ),
            file_path,
            change_type: draft.change_type.clone(),
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

    let canonical_target =
        canonical_target_fallback(root, target_override.as_deref(), &changes, &patches);
    Ok(CodeChangeSet {
        patches,
        changes,
        summary,
        canonical_target,
    })
}

fn canonical_target_fallback(
    root: &Path,
    explicit_target: Option<&Path>,
    changes: &[CodeChange],
    patches: &[CodePatch],
) -> Option<PathBuf> {
    explicit_target
        .and_then(|path| normalize_target_scope_path(root, path).ok())
        .or_else(|| {
            changes
                .iter()
                .find(|change| matches!(change.change_type, ChangeType::ModifyFile))
                .map(|change| PathBuf::from(&change.file_path))
        })
        .or_else(|| {
            changes
                .iter()
                .find(|change| matches!(change.change_type, ChangeType::CreateFile))
                .map(|change| PathBuf::from(&change.file_path))
        })
        .or_else(|| canonical_patch_target_file(patches))
}

fn synthesize_change_hunks(
    file_path: &str,
    original: &str,
    updated: &str,
    change_type: &ChangeType,
) -> Vec<DiffHunk> {
    match change_type {
        ChangeType::CreateFile | ChangeType::MoveFile => vec![DiffHunk {
            start_line: 1,
            end_line: original.lines().count().max(1),
            replacement: updated.to_string(),
        }],
        ChangeType::ModifyFile => {
            if file_path.ends_with("apps/cli/src/service.rs")
                || file_path.ends_with("src/service.rs")
            {
                let dedicated = synthesize_service_file_hunks(original, updated);
                if !dedicated.is_empty() {
                    return dedicated;
                }
            }
            let original_lines = original
                .lines()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let updated_lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
            let ops = synthesize_line_diff_ops(&original_lines, &updated_lines);
            let hunks = synthesize_fragmented_hunks(&ops);
            if hunks.is_empty() {
                synthesize_bounded_full_replacement_hunks(original, updated)
            } else {
                hunks
                    .into_iter()
                    .map(|hunk| DiffHunk {
                        start_line: hunk.start_line,
                        end_line: hunk.end_line,
                        replacement: join_replacement_lines(
                            &hunk.replacement_lines,
                            updated.ends_with('\n'),
                        ),
                    })
                    .collect()
            }
        }
    }
}

fn synthesize_bounded_full_replacement_hunks(original: &str, updated: &str) -> Vec<DiffHunk> {
    let updated_lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    if updated_lines.is_empty() {
        return vec![DiffHunk {
            start_line: 1,
            end_line: original.lines().count().max(1),
            replacement: String::new(),
        }];
    }

    let original_total = original.lines().count().max(1);
    let trailing_newline = updated.ends_with('\n');
    let mut hunks = Vec::new();
    let mut old_cursor = 1usize;
    let mut offset = 0usize;

    while offset < updated_lines.len() {
        let next = (offset + MAX_HUNK_LINES).min(updated_lines.len());
        let is_last = next == updated_lines.len();
        let replacement_lines = &updated_lines[offset..next];
        let covered_lines = if is_last {
            original_total.saturating_sub(old_cursor).saturating_add(1)
        } else {
            replacement_lines
                .len()
                .min(original_total.saturating_sub(old_cursor).saturating_add(1))
        };
        let end_line = if covered_lines == 0 {
            old_cursor.saturating_sub(1)
        } else {
            old_cursor + covered_lines - 1
        };
        hunks.push(DiffHunk {
            start_line: old_cursor,
            end_line,
            replacement: join_replacement_lines(replacement_lines, trailing_newline && is_last),
        });
        old_cursor = end_line.saturating_add(1);
        offset = next;
    }

    hunks
}

fn join_replacement_lines(lines: &[String], trailing_newline: bool) -> String {
    let mut replacement = lines.join("\n");
    if trailing_newline && !replacement.is_empty() {
        replacement.push('\n');
    }
    replacement
}

fn synthesize_line_diff_ops(old_lines: &[String], new_lines: &[String]) -> Vec<LineDiffOp> {
    let mut dp = vec![vec![0usize; new_lines.len() + 1]; old_lines.len() + 1];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            dp[i][j] = if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut i = 0usize;
    let mut j = 0usize;
    let mut ops = Vec::new();
    while i < old_lines.len() && j < new_lines.len() {
        if old_lines[i] == new_lines[j] {
            ops.push(LineDiffOp::Equal);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(LineDiffOp::Remove(old_lines[i].clone()));
            i += 1;
        } else {
            ops.push(LineDiffOp::Add(new_lines[j].clone()));
            j += 1;
        }
    }
    while i < old_lines.len() {
        ops.push(LineDiffOp::Remove(old_lines[i].clone()));
        i += 1;
    }
    while j < new_lines.len() {
        ops.push(LineDiffOp::Add(new_lines[j].clone()));
        j += 1;
    }
    ops
}

fn synthesize_fragmented_hunks(ops: &[LineDiffOp]) -> Vec<SynthesizedHunk> {
    let mut hunks = Vec::new();
    let mut old_line = 1usize;
    let mut pending: Option<SynthesizedHunk> = None;

    for op in ops {
        match op {
            LineDiffOp::Equal => {
                if let Some(hunk) = pending.take() {
                    hunks.push(hunk);
                }
                old_line += 1;
            }
            LineDiffOp::Remove(line) => {
                let hunk = pending.get_or_insert_with(|| SynthesizedHunk {
                    start_line: old_line,
                    end_line: old_line.saturating_sub(1),
                    replacement_lines: Vec::new(),
                });
                hunk.end_line = old_line;
                if hunk.end_line.saturating_sub(hunk.start_line) + hunk.replacement_lines.len() + 1
                    >= MAX_HUNK_LINES
                {
                    hunks.push(pending.take().expect("pending hunk"));
                    pending = Some(SynthesizedHunk {
                        start_line: old_line,
                        end_line: old_line,
                        replacement_lines: Vec::new(),
                    });
                }
                let _removed_line = line;
                old_line += 1;
            }
            LineDiffOp::Add(line) => {
                let hunk = pending.get_or_insert_with(|| SynthesizedHunk {
                    start_line: old_line,
                    end_line: old_line.saturating_sub(1),
                    replacement_lines: Vec::new(),
                });
                hunk.replacement_lines.push(line.clone());
                if hunk.replacement_lines.len() >= MAX_HUNK_LINES {
                    hunks.push(pending.take().expect("pending hunk"));
                } else if hunk.replacement_lines.len() >= PREFERRED_HUNK_LINES {
                    // Prefer shorter hunks when a long insertion sequence continues.
                    hunks.push(pending.take().expect("pending hunk"));
                }
            }
        }
    }
    if let Some(hunk) = pending.take() {
        hunks.push(hunk);
    }
    hunks
}

fn synthesize_service_file_hunks(original: &str, updated: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    for kind in [
        ServiceBlockKind::Use,
        ServiceBlockKind::Mod,
        ServiceBlockKind::PubUse,
    ] {
        let Some(original_range) = service_block_range(original, kind) else {
            continue;
        };
        let Some(updated_range) = service_block_range(updated, kind) else {
            continue;
        };
        let original_block = original
            .lines()
            .skip(original_range.0)
            .take(original_range.1.saturating_sub(original_range.0))
            .collect::<Vec<_>>()
            .join("\n");
        let updated_block = updated
            .lines()
            .skip(updated_range.0)
            .take(updated_range.1.saturating_sub(updated_range.0))
            .collect::<Vec<_>>()
            .join("\n");
        if original_block == updated_block {
            continue;
        }
        let replacement = if updated_block.is_empty() {
            String::new()
        } else {
            format!("{updated_block}\n")
        };
        let start_line = original_range.0 + 1;
        let end_line = original_range.1.max(original_range.0 + 1);
        let touched = end_line.saturating_sub(start_line) + 1;
        if touched > 120 {
            return Vec::new();
        }
        hunks.push(DiffHunk {
            start_line,
            end_line,
            replacement,
        });
    }
    hunks
}

#[derive(Clone, Copy)]
enum ServiceBlockKind {
    Use,
    Mod,
    PubUse,
}

fn service_block_range(content: &str, kind: ServiceBlockKind) -> Option<(usize, usize)> {
    let lines = content.lines().collect::<Vec<_>>();
    let start = lines.iter().position(|line| match kind {
        ServiceBlockKind::Use => line.trim_start().starts_with("use "),
        ServiceBlockKind::Mod => {
            line.trim_start().starts_with("#[path = \"service/")
                || line.trim_start().starts_with("pub mod ")
        }
        ServiceBlockKind::PubUse => line.trim_start().starts_with("pub use "),
    })?;
    let mut end = start;
    while end < lines.len() {
        let trimmed = lines[end].trim();
        let belongs = match kind {
            ServiceBlockKind::Use => trimmed.is_empty() || trimmed.starts_with("use "),
            ServiceBlockKind::Mod => {
                trimmed.is_empty()
                    || trimmed.starts_with("#[path = \"service/")
                    || trimmed.starts_with("pub mod ")
            }
            ServiceBlockKind::PubUse => {
                trimmed.is_empty()
                    || trimmed.starts_with("pub use ")
                    || trimmed.starts_with("Deterministic")
                    || trimmed.starts_with("IssueAggregator")
                    || trimmed.starts_with("generate_plan")
                    || trimmed.starts_with("infer_root_cause")
                    || trimmed == "};"
                    || trimmed.ends_with(',')
            }
        };
        if !belongs {
            break;
        }
        end += 1;
    }
    Some((start, end))
}

fn render_change_replacement(change: &CodeChange, original: &str) -> Result<String, String> {
    match change.change_type {
        ChangeType::CreateFile | ChangeType::MoveFile => Ok(change
            .hunks
            .last()
            .map(|hunk| hunk.replacement.clone())
            .unwrap_or_default()),
        ChangeType::ModifyFile => apply_hunks_to_content(original, &change.hunks),
    }
}

fn apply_hunks_to_content(original: &str, hunks: &[DiffHunk]) -> Result<String, String> {
    let mut lines = original
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let trailing_newline = original.ends_with('\n');
    let mut sorted = hunks.to_vec();
    sorted.sort_by(|left, right| right.start_line.cmp(&left.start_line));
    for hunk in sorted {
        let start = hunk.start_line.saturating_sub(1).min(lines.len());
        let end_exclusive = hunk.end_line.min(lines.len());
        let replacement = hunk
            .replacement
            .lines()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if hunk.start_line <= hunk.end_line {
            if start > end_exclusive {
                return Err("invalid hunk line range".to_string());
            }
            lines.splice(start..end_exclusive, replacement);
        } else {
            lines.splice(start..start, replacement);
        }
    }
    let mut rendered = lines.join("\n");
    if (trailing_newline || hunks.iter().any(|h| h.replacement.ends_with('\n')))
        && !rendered.is_empty()
    {
        rendered.push('\n');
    }
    Ok(rendered)
}

pub fn build_unified_diff_preview(
    root: &Path,
    change_set: &CodeChangeSet,
) -> Result<UnifiedDiffPreview, String> {
    let mut files = Vec::new();
    let mut summary = UnifiedDiffSummary::default();

    for change in &change_set.changes {
        match build_code_diff(root, change)? {
            Some(diff) => {
                summary.file_count += 1;
                summary.added_lines += diff.added_lines;
                summary.removed_lines += diff.removed_lines;
                files.push(diff);
            }
            None => {
                summary.skipped_binary_files += 1;
            }
        }
    }

    Ok(UnifiedDiffPreview { files, summary })
}

pub fn render_code_diff_lines(diff: &CodeDiff) -> Vec<String> {
    let mut rendered = vec![
        format!("--- {}", diff.before_label),
        format!("+++ {}", diff.after_label),
    ];
    for hunk in &diff.hunks {
        rendered.push(String::new());
        rendered.push(hunk.header.clone());
        rendered.extend(hunk.lines.iter().map(render_diff_line));
    }
    rendered
}

pub fn code_to_ir_program(code: &str) -> Result<CodeIrProgram, String> {
    Ok(CodeIrProgram {
        source: code.to_string(),
        imports: parse_rust_imports(code),
    })
}

pub fn apply_refactor_rule(
    ir: &CodeIrProgram,
    rule: RefactorRule,
) -> Result<CodeIrProgram, String> {
    match rule {
        RefactorRule::RemoveUnusedImports => {
            let source_without_imports = ir
                .source
                .lines()
                .enumerate()
                .filter(|(index, _)| !ir.imports.iter().any(|import| import.line_index == *index))
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
                .join("\n");
            let unused_import_lines = ir
                .imports
                .iter()
                .filter(|import| import_is_unused(import, &source_without_imports))
                .map(|import| import.line_index)
                .collect::<BTreeSet<_>>();
            code_to_ir_program(&remove_lines(&ir.source, &unused_import_lines))
        }
    }
}

pub fn remove_unused_imports_refactor(code: &str) -> Result<RefactorDiffResult, String> {
    let before_ir = code_to_ir_program(code)?;
    let after_ir = apply_refactor_rule(&before_ir, RefactorRule::RemoveUnusedImports)?;
    let after_code = after_ir.source.clone();
    let diff = if before_ir == after_ir || code.trim() == after_code.trim() {
        None
    } else {
        let diff = generate_minimal_unified_diff(code, &after_code);
        (!diff.trim().is_empty()).then_some(diff)
    };
    let removed_lines = diff
        .as_ref()
        .map(|diff| diff.lines().filter(|line| line.starts_with("- ")).count())
        .unwrap_or(0);

    Ok(RefactorDiffResult {
        before_ir,
        after_ir,
        after_code,
        diff,
        removed_lines,
    })
}

pub fn generate_minimal_unified_diff(before: &str, after: &str) -> String {
    let before_lines = before.lines().map(ToString::to_string).collect::<Vec<_>>();
    let after_lines = after.lines().map(ToString::to_string).collect::<Vec<_>>();
    diff_ops(&before_lines, &after_lines)
        .into_iter()
        .filter_map(|op| match op {
            UnifiedDiffOp::Remove(line) => Some(format!("- {line}")),
            UnifiedDiffOp::Add(line) => Some(format!("+ {line}")),
            UnifiedDiffOp::Equal(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_rust_imports(code: &str) -> Vec<RustImport> {
    code.lines()
        .enumerate()
        .filter_map(|(line_index, line)| parse_rust_import(line_index, line))
        .collect()
}

fn parse_rust_import(line_index: usize, line: &str) -> Option<RustImport> {
    let trimmed = line.trim();
    let path = trimmed
        .strip_prefix("use ")
        .or_else(|| trimmed.strip_prefix("pub use "))?
        .trim_end_matches(';')
        .trim();
    Some(RustImport {
        line_index,
        imported_symbols: imported_symbols_from_use_path(path),
        wildcard: path.ends_with("::*"),
    })
}

fn imported_symbols_from_use_path(path: &str) -> Vec<String> {
    if path.ends_with("::*") {
        return Vec::new();
    }
    if let Some(group_start) = path.rfind('{') {
        let group_end = path.rfind('}').unwrap_or(path.len());
        return path[group_start + 1..group_end]
            .split(',')
            .filter_map(imported_symbol_from_segment)
            .collect();
    }
    imported_symbol_from_segment(path).into_iter().collect()
}

fn imported_symbol_from_segment(segment: &str) -> Option<String> {
    let trimmed = segment.trim();
    if trimmed.is_empty() || trimmed == "self" {
        return None;
    }
    let symbol = trimmed
        .split(" as ")
        .nth(1)
        .or_else(|| trimmed.rsplit("::").next())
        .unwrap_or(trimmed)
        .trim();
    (!symbol.is_empty() && symbol != "*").then(|| symbol.to_string())
}

fn import_is_unused(import: &RustImport, source_without_imports: &str) -> bool {
    if import.wildcard {
        return true;
    }
    !import.imported_symbols.is_empty()
        && import
            .imported_symbols
            .iter()
            .all(|symbol| !contains_rust_identifier(source_without_imports, symbol))
}

fn contains_rust_identifier(source: &str, symbol: &str) -> bool {
    source.match_indices(symbol).any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = source[index + symbol.len()..].chars().next();
        !before.is_some_and(is_rust_identifier_char) && !after.is_some_and(is_rust_identifier_char)
    })
}

fn is_rust_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn remove_lines(source: &str, line_indexes: &BTreeSet<usize>) -> String {
    let mut rendered = source
        .lines()
        .enumerate()
        .filter(|(index, _)| !line_indexes.contains(index))
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    if source.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

#[cfg(test)]
mod refactor_diff_tests {
    use super::*;

    #[test]
    fn remove_unused_imports_deletes_unreferenced_import() {
        let code = "use std::collections::HashMap;\n\nfn main() {}\n";
        let result = remove_unused_imports_refactor(code).expect("refactor");
        assert_ne!(result.before_ir, result.after_ir);
        assert_eq!(
            result.diff.as_deref(),
            Some("- use std::collections::HashMap;")
        );
    }

    #[test]
    fn remove_unused_imports_keeps_referenced_import() {
        let code =
            "use std::collections::HashMap;\n\nfn main() {\n    let m = HashMap::new();\n}\n";
        let result = remove_unused_imports_refactor(code).expect("refactor");
        assert_eq!(result.before_ir, result.after_ir);
        assert_eq!(result.diff, None);
    }

    #[test]
    fn remove_unused_imports_diff_is_deterministic() {
        let code = "use std::collections::HashMap;\n\nfn main() {}\n";
        let diffs = (0..3)
            .map(|_| {
                remove_unused_imports_refactor(code)
                    .expect("refactor")
                    .diff
                    .expect("diff")
            })
            .collect::<Vec<_>>();
        assert_eq!(diffs[0], diffs[1]);
        assert_eq!(diffs[1], diffs[2]);
    }
}

pub fn render_unified_diff_excerpt(
    preview: &UnifiedDiffPreview,
    file_limit: usize,
    line_limit: usize,
) -> Vec<(String, String)> {
    let mut excerpts = Vec::new();
    let mut remaining = line_limit;

    for diff in preview.files.iter().take(file_limit) {
        if remaining == 0 {
            break;
        }
        let lines = render_code_diff_lines(diff);
        let total_lines = lines.len();
        let take = remaining.min(total_lines);
        let mut excerpt = lines.into_iter().take(take).collect::<Vec<_>>();
        remaining = remaining.saturating_sub(take);
        if take == 0 {
            break;
        }
        if take < total_lines {
            excerpt.push("... diff truncated ...".to_string());
        }
        excerpts.push((diff.file.display().to_string(), excerpt.join("\n")));
    }

    excerpts
}

pub fn format_unified_diff_summary(summary: &UnifiedDiffSummary) -> String {
    let mut rendered = format!(
        "{} files changed, +{} -{} lines",
        summary.file_count, summary.added_lines, summary.removed_lines
    );
    if summary.skipped_binary_files > 0 {
        rendered.push_str(&format!(
            " ({} binary skipped)",
            summary.skipped_binary_files
        ));
    }
    if summary.truncated {
        rendered.push_str(" [truncated]");
    }
    rendered
}

fn build_code_diff(root: &Path, change: &CodeChange) -> Result<Option<CodeDiff>, String> {
    let path = root.join(&change.file_path);
    let old = read_diff_text(&path)?;
    let Some(old) = old else {
        return Ok(None);
    };
    let new = render_change_replacement(change, &old)?;
    let old_lines = old.lines().map(ToString::to_string).collect::<Vec<_>>();
    let new_lines = new.lines().map(ToString::to_string).collect::<Vec<_>>();
    let ops = diff_ops(&old_lines, &new_lines);
    let hunks = build_unified_hunks(&ops, UNIFIED_DIFF_CONTEXT_LINES);
    let added_lines = ops
        .iter()
        .filter(|op| matches!(op, UnifiedDiffOp::Add(_)))
        .count();
    let removed_lines = ops
        .iter()
        .filter(|op| matches!(op, UnifiedDiffOp::Remove(_)))
        .count();

    let (before_label, after_label) = match change.change_type {
        ChangeType::CreateFile => ("/dev/null".to_string(), format!("b/{}", change.file_path)),
        ChangeType::MoveFile => (format!("a/{}", change.file_path), "/dev/null".to_string()),
        ChangeType::ModifyFile => (
            format!("a/{}", change.file_path),
            format!("b/{}", change.file_path),
        ),
    };

    Ok(Some(CodeDiff {
        file: PathBuf::from(&change.file_path),
        before_label,
        after_label,
        hunks,
        added_lines,
        removed_lines,
    }))
}

fn read_diff_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Ok(Some(text)),
            Err(_) => Ok(None),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Some(String::new())),
        Err(err) => Err(format!("failed to read {}: {err}", path.display())),
    }
}

fn render_diff_line(line: &DiffLine) -> String {
    match line {
        DiffLine::Added(value) => format!("+{value}"),
        DiffLine::Removed(value) => format!("-{value}"),
        DiffLine::Context(value) => format!(" {value}"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UnifiedDiffOp {
    Equal(String),
    Remove(String),
    Add(String),
}

fn diff_ops(old_lines: &[String], new_lines: &[String]) -> Vec<UnifiedDiffOp> {
    let mut dp = vec![vec![0usize; new_lines.len() + 1]; old_lines.len() + 1];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            dp[i][j] = if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut i = 0usize;
    let mut j = 0usize;
    let mut ops = Vec::new();
    while i < old_lines.len() && j < new_lines.len() {
        if old_lines[i] == new_lines[j] {
            ops.push(UnifiedDiffOp::Equal(old_lines[i].clone()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(UnifiedDiffOp::Remove(old_lines[i].clone()));
            i += 1;
        } else {
            ops.push(UnifiedDiffOp::Add(new_lines[j].clone()));
            j += 1;
        }
    }
    while i < old_lines.len() {
        ops.push(UnifiedDiffOp::Remove(old_lines[i].clone()));
        i += 1;
    }
    while j < new_lines.len() {
        ops.push(UnifiedDiffOp::Add(new_lines[j].clone()));
        j += 1;
    }
    ops
}

fn build_unified_hunks(ops: &[UnifiedDiffOp], context: usize) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut pending_context = Vec::<String>::new();
    let mut current: Option<PendingUnifiedHunk> = None;
    let mut trailing_context = 0usize;

    for op in ops {
        match op {
            UnifiedDiffOp::Equal(line) => {
                pending_context.push(line.clone());
                if pending_context.len() > context {
                    pending_context.remove(0);
                }
                if let Some(hunk) = current.as_mut() {
                    hunk.lines.push(DiffLine::Context(line.clone()));
                    hunk.old_count += 1;
                    hunk.new_count += 1;
                    trailing_context += 1;
                    if trailing_context > context {
                        hunk.lines.pop();
                        hunk.old_count -= 1;
                        hunk.new_count -= 1;
                        hunks.push(current.take().expect("hunk").finish());
                        trailing_context = 0;
                    }
                }
                old_line += 1;
                new_line += 1;
            }
            UnifiedDiffOp::Remove(line) => {
                if current.is_none() {
                    current = Some(PendingUnifiedHunk::new(
                        old_line.saturating_sub(pending_context.len()),
                        new_line.saturating_sub(pending_context.len()),
                        pending_context.clone(),
                    ));
                }
                let hunk = current.as_mut().expect("hunk");
                hunk.lines.push(DiffLine::Removed(line.clone()));
                hunk.old_count += 1;
                trailing_context = 0;
                old_line += 1;
            }
            UnifiedDiffOp::Add(line) => {
                if current.is_none() {
                    current = Some(PendingUnifiedHunk::new(
                        old_line.saturating_sub(pending_context.len()),
                        new_line.saturating_sub(pending_context.len()),
                        pending_context.clone(),
                    ));
                }
                let hunk = current.as_mut().expect("hunk");
                hunk.lines.push(DiffLine::Added(line.clone()));
                hunk.new_count += 1;
                trailing_context = 0;
                new_line += 1;
            }
        }
    }

    if let Some(hunk) = current.take() {
        hunks.push(hunk.finish());
    }

    hunks
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingUnifiedHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<DiffLine>,
}

impl PendingUnifiedHunk {
    fn new(old_start: usize, new_start: usize, context_lines: Vec<String>) -> Self {
        let context_count = context_lines.len();
        Self {
            old_start,
            old_count: context_count,
            new_start,
            new_count: context_count,
            lines: context_lines
                .into_iter()
                .map(DiffLine::Context)
                .collect::<Vec<_>>(),
        }
    }

    fn finish(self) -> Hunk {
        Hunk {
            header: format!(
                "@@ -{},{} +{},{} @@",
                self.old_start, self.old_count, self.new_start, self.new_count
            ),
            lines: self.lines,
        }
    }
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
            canonical_target: Some(relative),
        }));
    }

    Ok(Some(CodeChangeSet {
        patches: patches.to_vec(),
        changes: vec![CodeChange {
            file_path: relative.display().to_string(),
            change_type: ChangeType::ModifyFile,
            hunks: synthesize_change_hunks(
                &relative.display().to_string(),
                &original,
                &updated,
                &ChangeType::ModifyFile,
            ),
        }],
        summary: ChangeSummary {
            total_changes: 1,
            create_files: 0,
            modify_files: 1,
            move_files: 0,
        },
        canonical_target: Some(relative),
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

    let old_v2_import = "use crate::nl::planner_v2::{plan_input as plan_nl_input_v2, update_conversation_after_plan};";
    let new_v2_import = format!("{REPL_V2_IMPORT_LINE}\n{REPL_V2_UPDATE_IMPORT_LINE}");
    let content = if content.contains(old_v2_import) {
        content.replacen(old_v2_import, &new_v2_import, 1)
    } else if content.contains(REPL_V2_IMPORT_LINE) && content.contains(REPL_V2_UPDATE_IMPORT_LINE)
    {
        content.to_string()
    } else {
        return Err("deterministic repl_v2 rewrite: missing planner_v2 import block".to_string());
    };

    let old_nl_import = "use crate::nl::{execute_plan as execute_nl_plan, render_plan_summary};";
    let new_nl_import = "use crate::nl::{\n    execute_plan as execute_nl_plan, plan_input as plan_nl_input, render_plan_summary_with_label,\n};";
    let content = if content.contains(old_nl_import) {
        content.replacen(old_nl_import, new_nl_import, 1)
    } else if content.contains(REPL_LEGACY_IMPORT_SNIPPET)
        && content.contains("render_plan_summary_with_label")
    {
        content
    } else {
        return Err("deterministic repl_v2 rewrite: missing planner import block".to_string());
    };

    let old_flow = r#"    if let Some(command_plan) = plan_nl_input_v2(input, session, conversation) {
        let planner_summary = render_plan_summary(&command_plan);
        emit_output(session, writer, &planner_summary)?;
        update_conversation_after_plan(input, &command_plan, conversation);

        if cfg!(test) {
            session.current_plan = Some(crate::nl::to_runtime_plan(&command_plan));
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
        session.current_plan = Some(crate::nl::to_runtime_plan(&command_plan));
        conversation.autonomous_label = None;
        print_follow_up_suggestions(input, session, writer)?;
        return Ok(());
    }
"#;
    let new_flow = r#"    let command_plan_v2 = plan_nl_input_v2(input, session, conversation);
    let planner_label = if command_plan_v2.is_some() {
        "nl_v2"
    } else {
        "nl_rule_based"
    };
    let command_plan =
        command_plan_v2.or_else(|| plan_nl_input(input, session));

    if let Some(command_plan) = command_plan {
        let planner_summary = render_plan_summary_with_label(&command_plan, planner_label);
        emit_output(session, writer, &planner_summary)?;
        update_conversation_after_plan(input, &command_plan, conversation);

        if cfg!(test) {
            session.current_plan = Some(crate::nl::to_runtime_plan(&command_plan));
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
        session.current_plan = Some(crate::nl::to_runtime_plan(&command_plan));
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
    for patch in &patches {
        for resolution in expand_companion_targets(
            root,
            &source_index,
            patch,
            target_override.as_deref(),
            resolved_paths,
        )? {
            resolutions
                .entry(resolution.module.clone())
                .or_insert(resolution);
        }
    }
    Ok(resolutions.into_values().collect())
}

pub fn build_apply_resolutions(
    root: &Path,
    change_set: &CodeChangeSet,
    target_override: Option<&Path>,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<Vec<ApplyTargetResolution>, String> {
    collect_apply_target_resolutions(root, &change_set.patches, target_override, resolved_paths)
}

pub fn attach_sandbox_paths_to_apply_resolutions(
    root: &Path,
    resolutions: &mut [ApplyTargetResolution],
    transactional: &TransactionalApplyResult,
) {
    for resolution in resolutions {
        resolution.sandbox_path =
            resolve_sandbox_module_file(&resolution.module, root, &transactional.sandbox_path)
                .ok()
                .or_else(|| {
                    Some(
                        transactional
                            .sandbox_path
                            .join(&resolution.resolved_relative_path),
                    )
                });
    }
}

pub fn build_canonicalization_telemetry(
    change_set: &CodeChangeSet,
    apply_resolutions: &[ApplyTargetResolution],
    execution: &CodingExecutionResult,
) -> CanonicalizationTelemetry {
    let mut normalization_issues = Vec::new();

    if execution.resolution_pipeline_hits > 0 {
        normalization_issues.push("mutation_resolution_pipeline".to_string());
    }
    if execution.degraded_resolution_hits > 0 {
        normalization_issues.push("degraded_resolution_path".to_string());
    }
    if change_set.canonical_target.is_none() {
        normalization_issues.push("canonical_target_relay_drift".to_string());
    }
    if change_set.changes.iter().any(|change| {
        change.file_path == "apps/cli/src/service.rs"
            && change.hunks.iter().any(|hunk| {
                let touched = if hunk.start_line <= hunk.end_line {
                    hunk.end_line.saturating_sub(hunk.start_line) + 1
                } else {
                    hunk.replacement.lines().count()
                };
                touched > MAX_HUNK_LINES
            })
    }) {
        normalization_issues.push("generic_full_file_regeneration".to_string());
    }
    if change_set.changes.iter().any(|change| {
        change
            .hunks
            .iter()
            .any(|hunk| hunk.replacement.contains("TODO: define required methods"))
    }) {
        normalization_issues.push("todo_trait_template".to_string());
    }

    for patch in &change_set.patches {
        for operation in &patch.operations {
            if let PatchOperation::CreateInterface { name, .. } = operation {
                let module = snake_case(name);
                let expected_suffix = format!("{module}.rs");
                let resolution = apply_resolutions
                    .iter()
                    .find(|resolution| resolution.module == module);
                match resolution {
                    Some(resolution)
                        if resolution
                            .resolved_relative_path
                            .to_string_lossy()
                            .ends_with(&expected_suffix) => {}
                    Some(_) => {
                        normalization_issues.push("explicit_target_companion_collapse".to_string())
                    }
                    None => normalization_issues
                        .push("create_interface_materialization_drop".to_string()),
                }
            }
        }
    }

    normalization_issues.sort();
    normalization_issues.dedup();
    CanonicalizationTelemetry {
        normalization_path_used: !normalization_issues.is_empty(),
        normalization_issue_count: normalization_issues.len() as u64,
        normalization_issues,
    }
}

pub fn normalization_issue_count(telemetry: &CanonicalizationTelemetry) -> u64 {
    telemetry.normalization_issue_count
}

pub fn ensure_canonical_target_dto_continuity(
    root: &Path,
    change_set: &mut CodeChangeSet,
    execution: &CodingExecutionResult,
    explicit_target: Option<&Path>,
) -> Result<(), String> {
    change_set.canonical_target = change_set
        .canonical_target
        .clone()
        .or_else(|| {
            execution
                .canonical_target_path
                .as_deref()
                .map(PathBuf::from)
        })
        .or_else(|| explicit_target.and_then(|path| normalize_target_scope_path(root, path).ok()));
    Ok(())
}

fn expand_companion_targets(
    root: &Path,
    source_index: &ModuleSourceIndex,
    patch: &CodePatch,
    target_override: Option<&Path>,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<Vec<ApplyTargetResolution>, String> {
    let mut resolutions = Vec::new();
    let Some(operation) = patch.operations.first() else {
        return Ok(resolutions);
    };
    match operation {
        PatchOperation::CreateInterface { name, between } => {
            let module = snake_case(name);
            let file_name = format!("{module}.rs");
            let resolved = resolved_paths
                .get(&module)
                .cloned()
                .or_else(|| {
                    resolved_paths
                        .get(&between.0)
                        .map(|path| source_index.generated_path_from_source(root, path, &file_name))
                })
                .unwrap_or_else(|| source_index.generated_path(root, &between.0, &file_name));
            let relative = normalize_target_scope_path(root, &resolved)?;
            resolutions.push(ApplyTargetResolution {
                module,
                resolution_strategy: "companion_create_file".to_string(),
                resolved_relative_path: relative.clone(),
                resolved_path: relative,
                sandbox_path: None,
            });
        }
        PatchOperation::UpdateDependency { from, .. } => {
            let module = from.clone();
            let resolution = if let Some(path) = target_override {
                let relative = normalize_relative(root, path).map(PathBuf::from)?;
                ApplyTargetResolution {
                    module,
                    resolution_strategy: "target_override".to_string(),
                    resolved_relative_path: relative.clone(),
                    resolved_path: relative,
                    sandbox_path: None,
                }
            } else if !patch.target_file.as_os_str().is_empty() {
                ApplyTargetResolution {
                    module,
                    resolution_strategy: "canonical_target_file".to_string(),
                    resolved_relative_path: patch.target_file.clone(),
                    resolved_path: patch.target_file.clone(),
                    sandbox_path: None,
                }
            } else if let Some(path) = resolved_paths.get(from) {
                ApplyTargetResolution {
                    module,
                    resolution_strategy: "candidate_snapshot".to_string(),
                    resolved_relative_path: path.clone(),
                    resolved_path: path.clone(),
                    sandbox_path: None,
                }
            } else if let Some(resolution) = source_index.resolve_apply_target(from) {
                resolution
            } else {
                return Ok(resolutions);
            };
            resolutions.push(resolution);
        }
        _ => {
            let Some(module) = patch_target_module(patch) else {
                return Ok(resolutions);
            };
            let resolution = if let Some(path) = target_override {
                let relative = normalize_relative(root, path).map(PathBuf::from)?;
                ApplyTargetResolution {
                    module: module.clone(),
                    resolution_strategy: "target_override".to_string(),
                    resolved_relative_path: relative.clone(),
                    resolved_path: relative,
                    sandbox_path: None,
                }
            } else if !patch.target_file.as_os_str().is_empty() {
                ApplyTargetResolution {
                    module: module.clone(),
                    resolution_strategy: "canonical_target_file".to_string(),
                    resolved_relative_path: patch.target_file.clone(),
                    resolved_path: patch.target_file.clone(),
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
                return Ok(resolutions);
            };
            resolutions.push(resolution);
        }
    }
    Ok(resolutions)
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
            canonical_target_path: None,
            resolution_pipeline_hits: 0,
            degraded_resolution_hits: 0,
            stale_artifact_detected: false,
        });
    }
    let diff = compute_diff_report(root, change_set)?;
    let representative_target = representative_target_file(
        root,
        change_set,
        options.explicit_target.as_deref(),
        transactional_candidate.map(|candidate| candidate.source_path.as_path()),
    )
    .map(|path| path.display().to_string());
    if options.safe_mode {
        validate_diff_report(&diff)?;
    }

    if options.apply {
        let pre_apply_dirty = if options.auto_commit {
            Some(
                collect_dirty_paths(root)?
                    .into_iter()
                    .map(|(path, _)| path)
                    .collect::<BTreeSet<_>>(),
            )
        } else {
            None
        };
        let transactional = transactional_apply(
            root,
            change_set,
            transactional_candidate,
            options.no_build,
            options.explicit_target.as_deref(),
        )?;
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
                canonical_target_path: representative_target.clone(),
                resolution_pipeline_hits: 0,
                degraded_resolution_hits: 0,
                stale_artifact_detected: false,
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
            canonical_target_path: representative_target.clone(),
            resolution_pipeline_hits: 0,
            degraded_resolution_hits: 0,
            stale_artifact_detected: false,
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
                options.prompt_commit,
                pre_apply_dirty.as_ref(),
            ) {
                Ok(commit) => {
                    result.committed = commit.commit_created;
                    result.commit_id = commit.commit_hash.clone();
                    result.branch = if commit.status_before.detached_head {
                        None
                    } else if commit.status_before.branch_name.is_empty() {
                        None
                    } else {
                        Some(commit.status_before.branch_name.clone())
                    };
                    if !commit.commit_created {
                        result.reason = Some(
                            if commit.confirmation_required && !commit.confirmation_granted {
                                "commit_skipped".to_string()
                            } else {
                                "no_commit_created".to_string()
                            },
                        );
                    }
                    if let Some(warning) = &commit.warning {
                        result.reason = Some(match &result.reason {
                            Some(reason) => format!("{warning}\n{reason}"),
                            None => warning.clone(),
                        });
                    }
                    result.git_commit = Some(commit);

                    if result.committed && (options.auto_push || options.auto_pr) {
                        let push = match restricted_push(
                            result.branch.as_deref(),
                            root,
                            options.confirm_push,
                        ) {
                            Ok(push) => push,
                            Err(err) => {
                                persist_remote_integration_telemetry(
                                    root,
                                    &RemoteIntegrationTelemetry {
                                        remote: RemoteIntegrationTelemetryData {
                                            branch: result.branch.clone().unwrap_or_default(),
                                            dry_run_ok: false,
                                            push_ok: false,
                                            pr_created: false,
                                            pr_duplicate: false,
                                            base: "main".to_string(),
                                            remote: "origin".to_string(),
                                            auth_failure: Some(err.starts_with("RemoteAuthFailed")),
                                            confirmation: None,
                                        },
                                    },
                                )?;
                                result.status = "failed".to_string();
                                result.reason = Some(err);
                                return Ok(result);
                            }
                        };
                        let remote_confirmation = if push.confirmation_required {
                            Some(push.confirmation_granted)
                        } else {
                            None
                        };
                        let push_declined =
                            push.confirmation_required && !push.confirmation_granted;
                        result.git_push = Some(push.clone());
                        if push_declined {
                            result.reason = Some("push_skipped\npr_skipped".to_string());
                            return Ok(result);
                        }

                        if options.auto_pr {
                            match restricted_pr_create(
                                &push.branch_name,
                                "main",
                                root,
                                transactional_candidate,
                                result.files_changed,
                                true,
                            ) {
                                Ok(pr) => {
                                    if pr.duplicate_detected {
                                        result.reason = Some("PRAlreadyExists".to_string());
                                    }
                                    result.pull_request = Some(pr);
                                }
                                Err(err) => {
                                    persist_remote_integration_telemetry(
                                        root,
                                        &RemoteIntegrationTelemetry {
                                            remote: RemoteIntegrationTelemetryData {
                                                branch: push.branch_name.clone(),
                                                dry_run_ok: push.dry_run_ok,
                                                push_ok: push.push_created,
                                                pr_created: false,
                                                pr_duplicate: false,
                                                base: "main".to_string(),
                                                remote: push.remote_name.clone(),
                                                auth_failure: Some(
                                                    err.starts_with("RemoteAuthFailed"),
                                                ),
                                                confirmation: remote_confirmation,
                                            },
                                        },
                                    )?;
                                    result.status = "failed".to_string();
                                    result.reason = Some(err);
                                    return Ok(result);
                                }
                            }
                        } else {
                            persist_remote_integration_telemetry(
                                root,
                                &RemoteIntegrationTelemetry {
                                    remote: RemoteIntegrationTelemetryData {
                                        branch: push.branch_name.clone(),
                                        dry_run_ok: push.dry_run_ok,
                                        push_ok: push.push_created,
                                        pr_created: false,
                                        pr_duplicate: false,
                                        base: "main".to_string(),
                                        remote: push.remote_name.clone(),
                                        auth_failure: None,
                                        confirmation: remote_confirmation,
                                    },
                                },
                            )?;
                        }
                    }
                }
                Err(err) => {
                    result.status = "failed".to_string();
                    result.reason = Some(err);
                    return Ok(result);
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
            match run_build_validation(sandbox_root, root) {
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
            canonical_target_path: representative_target.clone(),
            resolution_pipeline_hits: 0,
            degraded_resolution_hits: 0,
            stale_artifact_detected: false,
        });
    }

    if !options.apply {
        let git_commit = if options.auto_commit {
            Some(build_dry_run_commit_preview(root, change_set)?)
        } else {
            None
        };
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
            git_commit,
            git_push: None,
            pull_request: None,
            canonical_target_path: representative_target.clone(),
            resolution_pipeline_hits: 0,
            degraded_resolution_hits: 0,
            stale_artifact_detected: false,
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
                run_build_validation(root, root)
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
            canonical_target_path: representative_target.clone(),
            resolution_pipeline_hits: 0,
            degraded_resolution_hits: 0,
            stale_artifact_detected: false,
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
                canonical_target_path: representative_target,
                resolution_pipeline_hits: 0,
                degraded_resolution_hits: 0,
                stale_artifact_detected: false,
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
        let original = fs::read_to_string(&path).unwrap_or_default();
        let replacement = render_change_replacement(change, &original)?;
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
    explicit_target: Option<&Path>,
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
            let build = run_transactional_cargo_check(
                root,
                &sandbox_root,
                change_set,
                candidate,
                explicit_target,
            )
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
        let replacement = render_change_replacement(change, &original)?;
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
    diffs.extend(explainability_diffs(root, change_set));
    diffs.sort_by(|lhs, rhs| lhs.target.cmp(&rhs.target));
    diffs.dedup_by(|lhs, rhs| lhs.target == rhs.target && lhs.kind == rhs.kind);
    let breaking_count = diffs.iter().filter(|diff| diff.breaking).count();
    Ok(DiffReport {
        diffs,
        breaking_count,
    })
}

fn explainability_diffs(root: &Path, change_set: &CodeChangeSet) -> Vec<ASTDiff> {
    let mut diffs = Vec::new();
    for change in &change_set.changes {
        let original = fs::read_to_string(root.join(&change.file_path)).unwrap_or_default();
        let replacement = render_change_replacement(change, &original).unwrap_or_default();
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
    prompt: bool,
    pre_apply_dirty: Option<&BTreeSet<String>>,
) -> Result<RestrictedGitCommitResult, String> {
    let started = Instant::now();
    let git_root = resolve_git_root(workspace_root)?;
    let status_before = collect_git_status(&git_root)?;
    let warning = status_before
        .detached_head
        .then_some("warning: detached HEAD".to_string());
    if changed_files.is_empty() {
        let telemetry_path = persist_local_integration_telemetry(
            &git_root,
            &LocalIntegrationTelemetry {
                git: LocalIntegrationGitTelemetry {
                    branch: status_before.branch_name.clone(),
                    dirty_before: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    dirty_after: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    files_added: 0,
                    commit_created: false,
                    commit_hash: None,
                    confirmation: false,
                },
            },
        )?;
        return Ok(RestrictedGitCommitResult {
            staged_files: Vec::new(),
            commit_created: false,
            commit_hash: None,
            confirmation_required: false,
            confirmation_granted: false,
            dirty_excluded: status_before
                .dirty_files
                .iter()
                .chain(status_before.untracked_files.iter())
                .cloned()
                .collect(),
            status_before: status_before.clone(),
            status_after: Some(status_before),
            diff_preview: Vec::new(),
            telemetry_path: Some(telemetry_path),
            warning,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let changed_set = changed_files
        .iter()
        .map(|path| normalize_git_path(path))
        .collect::<BTreeSet<_>>();
    if let Some(pre_apply_dirty) = pre_apply_dirty
        && changed_set
            .iter()
            .any(|path| pre_apply_dirty.contains(path))
    {
        return Err("CommitBlocked: workspace contains overlapping manual edits".to_string());
    }
    let dirty_excluded = status_before
        .dirty_files
        .iter()
        .chain(status_before.untracked_files.iter())
        .filter(|path| !changed_set.contains(&normalize_git_path(path)))
        .cloned()
        .collect::<Vec<_>>();
    let diff_preview = collect_git_diff_preview(&git_root, changed_files)?;

    let staged_files = changed_files
        .iter()
        .map(|path| PathBuf::from(normalize_git_path(path)))
        .collect::<Vec<_>>();

    if !confirm_commit_gate(diff_preview.len(), confirm, prompt)? {
        let telemetry_path = persist_local_integration_telemetry(
            &git_root,
            &LocalIntegrationTelemetry {
                git: LocalIntegrationGitTelemetry {
                    branch: status_before.branch_name.clone(),
                    dirty_before: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    dirty_after: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    files_added: 0,
                    commit_created: false,
                    commit_hash: None,
                    confirmation: false,
                },
            },
        )?;
        return Ok(RestrictedGitCommitResult {
            staged_files,
            commit_created: false,
            commit_hash: None,
            confirmation_required: true,
            confirmation_granted: false,
            dirty_excluded,
            status_before,
            status_after: None,
            diff_preview,
            telemetry_path: Some(telemetry_path),
            warning,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    stage_exact_files(&git_root, &staged_files)?;

    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&git_root)
        .status()
        .map_err(|err| format!("failed to run git diff --cached: {err}"))?;
    if status.success() {
        let telemetry_path = persist_local_integration_telemetry(
            &git_root,
            &LocalIntegrationTelemetry {
                git: LocalIntegrationGitTelemetry {
                    branch: status_before.branch_name.clone(),
                    dirty_before: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    dirty_after: status_before.conflicted
                        || !status_before.dirty_files.is_empty()
                        || !status_before.untracked_files.is_empty(),
                    files_added: 0,
                    commit_created: false,
                    commit_hash: None,
                    confirmation: true,
                },
            },
        )?;
        return Ok(RestrictedGitCommitResult {
            staged_files,
            commit_created: false,
            commit_hash: None,
            confirmation_required: false,
            confirmation_granted: true,
            dirty_excluded,
            status_before,
            status_after: None,
            diff_preview,
            telemetry_path: Some(telemetry_path),
            warning,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let _ = candidate;
    let output = Command::new("git")
        .args(["commit", "-m", "auto fix"])
        .current_dir(&git_root)
        .output()
        .map_err(|err| format!("CommitFailed: failed to run git commit: {err}"))?;
    if !output.status.success() {
        reset_staged_files(&git_root, &staged_files)?;
        return Err(format!(
            "CommitFailed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let commit_hash = current_commit(&git_root)?
        .ok_or_else(|| "CommitFailed: failed to resolve commit id".to_string())?;
    let status_after = collect_git_status(&git_root)?;
    let telemetry_path = persist_local_integration_telemetry(
        &git_root,
        &LocalIntegrationTelemetry {
            git: LocalIntegrationGitTelemetry {
                branch: status_before.branch_name.clone(),
                dirty_before: status_before.conflicted
                    || !status_before.dirty_files.is_empty()
                    || !status_before.untracked_files.is_empty(),
                dirty_after: status_after.conflicted
                    || !status_after.dirty_files.is_empty()
                    || !status_after.untracked_files.is_empty(),
                files_added: staged_files.len(),
                commit_created: true,
                commit_hash: Some(commit_hash.clone()),
                confirmation: true,
            },
        },
    )?;
    Ok(RestrictedGitCommitResult {
        staged_files,
        commit_created: true,
        commit_hash: Some(commit_hash),
        confirmation_required: false,
        confirmation_granted: true,
        dirty_excluded,
        status_before,
        status_after: Some(status_after),
        diff_preview,
        telemetry_path: Some(telemetry_path),
        warning,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn build_dry_run_commit_preview(
    root: &Path,
    change_set: &CodeChangeSet,
) -> Result<RestrictedGitCommitResult, String> {
    let git_root = resolve_git_root(root)?;
    let status_before = collect_git_status(&git_root)?;
    let warning = status_before
        .detached_head
        .then_some("warning: detached HEAD".to_string());
    let staged_files = change_set
        .changes
        .iter()
        .map(|change| PathBuf::from(&change.file_path))
        .collect::<Vec<_>>();
    let diff_preview = change_set
        .changes
        .iter()
        .map(|change| GitDiffEntry {
            change_type: match change.change_type {
                ChangeType::CreateFile => "A",
                ChangeType::ModifyFile => "M",
                ChangeType::MoveFile => "D",
            }
            .to_string(),
            path: PathBuf::from(&change.file_path),
            hunk_count: change.hunks.len(),
            line_delta: change
                .hunks
                .last()
                .map(|hunk| hunk.replacement.lines().count() as isize - hunk.end_line as isize)
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    Ok(RestrictedGitCommitResult {
        staged_files,
        commit_created: false,
        commit_hash: None,
        confirmation_required: false,
        confirmation_granted: false,
        dirty_excluded: status_before
            .dirty_files
            .iter()
            .chain(status_before.untracked_files.iter())
            .cloned()
            .collect(),
        status_before,
        status_after: None,
        diff_preview,
        telemetry_path: None,
        warning,
        elapsed_ms: 0,
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

fn resolve_git_root(root: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("GitUnavailable: failed to resolve git root: {err}"))?;
    if !output.status.success() {
        return Err("GitUnavailable: not a git repository".to_string());
    }
    let git_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if git_root.as_os_str().is_empty() {
        return Err("GitUnavailable: not a git repository".to_string());
    }
    Ok(git_root)
}

fn collect_git_status(root: &Path) -> Result<GitStatusSnapshot, String> {
    let output = Command::new("git")
        .args([
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ])
        .current_dir(root)
        .output()
        .map_err(|err| format!("DirtyIsolationFailed: failed to run git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "DirtyIsolationFailed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut snapshot = GitStatusSnapshot::default();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            snapshot.branch_name = head.trim().to_string();
            snapshot.detached_head = snapshot.branch_name == "(detached)";
            if snapshot.detached_head {
                snapshot.branch_name = "HEAD".to_string();
            }
            continue;
        }
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let parts = ab.split_whitespace().collect::<Vec<_>>();
            for part in parts {
                if let Some(value) = part.strip_prefix('+') {
                    snapshot.ahead = value.parse::<usize>().unwrap_or(0);
                } else if let Some(value) = part.strip_prefix('-') {
                    snapshot.behind = value.parse::<usize>().unwrap_or(0);
                }
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("? ") {
            snapshot.untracked_files.push(PathBuf::from(path.trim()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("u ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            if let Some(path) = parts.last() {
                snapshot.dirty_files.push(PathBuf::from(path));
            }
            snapshot.conflicted = true;
            continue;
        }
        if line.starts_with('1') || line.starts_with('2') {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if let Some(path) = parts.last() {
                snapshot.dirty_files.push(PathBuf::from(path));
            }
        }
    }
    snapshot.dirty_files.sort();
    snapshot.dirty_files.dedup();
    snapshot.untracked_files.sort();
    snapshot.untracked_files.dedup();
    if snapshot.branch_name.is_empty() {
        snapshot.branch_name = current_branch(root)?
            .filter(|branch| !branch.is_empty())
            .unwrap_or_else(|| "HEAD".to_string());
        snapshot.detached_head = snapshot.branch_name == "HEAD";
    }
    Ok(snapshot)
}

fn collect_git_diff_preview(
    root: &Path,
    changed_files: &[PathBuf],
) -> Result<Vec<GitDiffEntry>, String> {
    let mut preview = Vec::new();
    for path in changed_files {
        let path_str = normalize_git_path(path);
        let diff = Command::new("git")
            .args(["diff", "--", &path_str])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git diff: {err}"))?;
        if !diff.status.success() {
            return Err(format!(
                "failed to collect git diff preview: {}",
                String::from_utf8_lossy(&diff.stderr).trim()
            ));
        }
        let diff_stdout = String::from_utf8_lossy(&diff.stdout);
        let change_type = if diff_stdout.contains("new file mode") {
            "create"
        } else if diff_stdout.contains("deleted file mode") {
            "delete"
        } else if diff_stdout.contains("rename from ") {
            "move"
        } else {
            "modify"
        };
        let hunk_count = diff_stdout.matches("\n@@").count();
        let numstat = Command::new("git")
            .args(["diff", "--numstat", "--", &path_str])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git diff --numstat: {err}"))?;
        if !numstat.status.success() {
            return Err(format!(
                "failed to collect git diff stats: {}",
                String::from_utf8_lossy(&numstat.stderr).trim()
            ));
        }
        let mut line_delta = 0isize;
        if let Some(line) = String::from_utf8_lossy(&numstat.stdout).lines().next() {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() >= 2 {
                let added = parts[0].parse::<isize>().unwrap_or(0);
                let removed = parts[1].parse::<isize>().unwrap_or(0);
                line_delta = added - removed;
            }
        }
        preview.push(GitDiffEntry {
            change_type: change_type.to_string(),
            path: PathBuf::from(path_str),
            hunk_count,
            line_delta,
        });
    }
    preview.sort_by(|left, right| {
        left.change_type
            .cmp(&right.change_type)
            .then(left.path.cmp(&right.path))
            .then(left.hunk_count.cmp(&right.hunk_count))
            .then(left.line_delta.cmp(&right.line_delta))
    });
    Ok(preview)
}

fn persist_local_integration_telemetry(
    root: &Path,
    telemetry: &LocalIntegrationTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("local_integration.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn persist_remote_integration_telemetry(
    root: &Path,
    telemetry: &RemoteIntegrationTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("remote_integration.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn persist_sandbox_copy_telemetry(
    root: &Path,
    telemetry: &SandboxCopyTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("sandbox_copy.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn persist_cargo_resolution_telemetry(
    root: &Path,
    telemetry: &CargoResolutionTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("cargo_resolution.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn persist_semantic_recovery_telemetry(
    root: &Path,
    telemetry: &SemanticRecoveryTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("semantic_recovery.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn persist_malformed_import_recovery_telemetry(
    root: &Path,
    telemetry: &MalformedImportRecoveryTelemetry,
) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("malformed_import_recovery.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn ensure_origin_remote(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("GitUnavailable: failed to inspect origin remote: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("RemoteBlocked: origin only".to_string())
    }
}

fn path_parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
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

fn confirm_commit_gate(file_count: usize, confirmed: bool, prompt: bool) -> Result<bool, String> {
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
    if prompt {
        let mut writer = stdout().lock();
        writeln!(writer, "Apply succeeded.").map_err(|err| err.to_string())?;
        writeln!(writer, "Diff files: {file_count}").map_err(|err| err.to_string())?;
        write!(writer, "Proceed to git add + commit? (y/n) ").map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;

        let mut input = String::new();
        stdin()
            .read_line(&mut input)
            .map_err(|err| err.to_string())?;
        return Ok(matches!(
            input.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ));
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
        return Err("RemoteBlocked: protected branch".to_string());
    }
    ensure_origin_remote(workspace_root)?;
    ensure_gh_auth(workspace_root)?;

    let dry_run = git_push_command(workspace_root, &branch, true)?;
    if !dry_run.status.success() {
        persist_remote_integration_telemetry(
            workspace_root,
            &RemoteIntegrationTelemetry {
                remote: RemoteIntegrationTelemetryData {
                    branch: branch.clone(),
                    dry_run_ok: false,
                    push_ok: false,
                    pr_created: false,
                    pr_duplicate: false,
                    base: "main".to_string(),
                    remote: "origin".to_string(),
                    auth_failure: None,
                    confirmation: None,
                },
            },
        )?;
        return Err(format!(
            "RemoteDryRunFailed: {}",
            String::from_utf8_lossy(&dry_run.stderr).trim()
        ));
    }

    if !confirm_push_gate(confirm)? {
        let telemetry_path = persist_remote_integration_telemetry(
            workspace_root,
            &RemoteIntegrationTelemetry {
                remote: RemoteIntegrationTelemetryData {
                    branch: branch.clone(),
                    dry_run_ok: true,
                    push_ok: false,
                    pr_created: false,
                    pr_duplicate: false,
                    base: "main".to_string(),
                    remote: "origin".to_string(),
                    auth_failure: None,
                    confirmation: Some(false),
                },
            },
        )?;
        return Ok(RestrictedGitPushResult {
            branch_name: branch.clone(),
            push_created: false,
            remote_name: "origin".to_string(),
            remote_ref: format!("origin/{branch}"),
            dry_run_ok: true,
            confirmation_required: true,
            confirmation_granted: false,
            telemetry_path: Some(telemetry_path),
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
    let telemetry_path = persist_remote_integration_telemetry(
        workspace_root,
        &RemoteIntegrationTelemetry {
            remote: RemoteIntegrationTelemetryData {
                branch: branch.clone(),
                dry_run_ok: true,
                push_ok: true,
                pr_created: false,
                pr_duplicate: false,
                base: "main".to_string(),
                remote: "origin".to_string(),
                auth_failure: None,
                confirmation: Some(true),
            },
        },
    )?;
    Ok(RestrictedGitPushResult {
        branch_name: branch.clone(),
        push_created: true,
        remote_name: "origin".to_string(),
        remote_ref: format!("origin/{branch}"),
        dry_run_ok: true,
        confirmation_required: !confirm,
        confirmation_granted: true,
        telemetry_path: Some(telemetry_path),
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

fn confirm_push_gate(confirmed: bool) -> Result<bool, String> {
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
    let mut writer = stdout().lock();
    write!(writer, "Remote push and create PR? (y/n) ").map_err(|err| err.to_string())?;
    writer.flush().map_err(|err| err.to_string())?;

    let mut input = String::new();
    stdin()
        .read_line(&mut input)
        .map_err(|err| err.to_string())?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
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
    if base_branch != "main" {
        return Err(format!(
            "InvalidBaseBranch: base branch '{base_branch}' is not in the allowlist"
        ));
    }
    ensure_gh_auth(workspace_root)?;

    if let Some(existing) = duplicate_pull_request(workspace_root, branch_name)? {
        let telemetry_path = persist_remote_integration_telemetry(
            workspace_root,
            &RemoteIntegrationTelemetry {
                remote: RemoteIntegrationTelemetryData {
                    branch: branch_name.to_string(),
                    dry_run_ok: true,
                    push_ok: true,
                    pr_created: false,
                    pr_duplicate: true,
                    base: base_branch.to_string(),
                    remote: "origin".to_string(),
                    auth_failure: None,
                    confirmation: Some(true),
                },
            },
        )?;
        return Ok(RestrictedPullRequestResult {
            branch_name: branch_name.to_string(),
            base_branch: base_branch.to_string(),
            pr_created: false,
            pr_number: existing.0,
            pr_url: existing.1,
            draft: false,
            duplicate_detected: true,
            telemetry_path: Some(telemetry_path),
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
            draft: false,
            duplicate_detected: false,
            telemetry_path: None,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let title = format!(
        "auto fix: {}",
        candidate
            .map(|candidate| candidate.candidate_id.as_str())
            .filter(|id| !id.is_empty())
            .unwrap_or("deterministic update")
    );
    let commit_hash = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("PullRequestCreateFailed: failed to inspect HEAD: {err}"))?;
    if !commit_hash.status.success() {
        return Err(format!(
            "PullRequestCreateFailed: {}",
            String::from_utf8_lossy(&commit_hash.stderr).trim()
        ));
    }
    let commit_hash = String::from_utf8_lossy(&commit_hash.stdout)
        .trim()
        .to_string();
    let body = format!(
        "- generated by design_cli\n- sandbox build verified\n- local commit hash: {commit_hash}\n- files: {file_count}\n- crate: {}",
        candidate
            .map(|candidate| candidate.module_id.crate_name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("unknown"),
    );
    let output = Command::new(resolve_gh_tool())
        .args([
            "pr",
            "create",
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
        draft: false,
        duplicate_detected: false,
        telemetry_path: Some(persist_remote_integration_telemetry(
            workspace_root,
            &RemoteIntegrationTelemetry {
                remote: RemoteIntegrationTelemetryData {
                    branch: branch_name.to_string(),
                    dry_run_ok: true,
                    push_ok: true,
                    pr_created: true,
                    pr_duplicate: false,
                    base: base_branch.to_string(),
                    remote: "origin".to_string(),
                    auth_failure: None,
                    confirmation: Some(true),
                },
            },
        )?),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn ensure_gh_auth(root: &Path) -> Result<(), String> {
    let output = Command::new(resolve_gh_tool())
        .args(["auth", "status"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("RemoteAuthFailed: failed to run gh auth status: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        persist_remote_integration_telemetry(
            root,
            &RemoteIntegrationTelemetry {
                remote: RemoteIntegrationTelemetryData {
                    branch: current_branch(root)?.unwrap_or_default(),
                    dry_run_ok: false,
                    push_ok: false,
                    pr_created: false,
                    pr_duplicate: false,
                    base: "main".to_string(),
                    remote: "origin".to_string(),
                    auth_failure: Some(true),
                    confirmation: None,
                },
            },
        )?;
        Err("RemoteAuthFailed".to_string())
    }
}

fn duplicate_pull_request(
    root: &Path,
    branch_name: &str,
) -> Result<Option<(Option<u64>, Option<String>)>, String> {
    let output = Command::new(resolve_gh_tool())
        .args(["pr", "view", branch_name, "--json", "number,url"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("DuplicatePullRequest: failed to run gh pr view: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value =
        serde_json::from_str(&stdout).map_err(|err| format!("DuplicatePullRequest: {err}"))?;
    let Some(first) = value.as_object() else {
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
    fix_build_with_entry(
        project_root,
        project_root,
        resolve_root_module_file(project_root)?,
    )
}

fn fix_build_in_sandbox(
    workspace_root: &Path,
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
) -> Result<FixResult, String> {
    fix_build_with_entry(
        sandbox_root,
        workspace_root,
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
        workspace_root,
        resolve_root_module_file_for_change_set(workspace_root, change_set, candidate)?,
    )
}

fn fix_build_with_entry(
    project_root: &Path,
    telemetry_root: &Path,
    entry_file: PathBuf,
) -> Result<FixResult, String> {
    let mut fix = FixResult::default();
    let mut semantic_recovered = false;
    for _ in 0..4 {
        let recovered = attempt_semantic_compile_recovery(project_root, telemetry_root)?;
        if !recovered {
            break;
        }
        semantic_recovered = true;
    }
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
    fix.build_fixed = semantic_recovered
        || !(fix.registered_modules.is_empty() && fix.created_placeholders.is_empty());
    Ok(fix)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticCompileError {
    MalformedImportBatch {
        target_file: PathBuf,
        help_uses: Vec<String>,
    },
    MissingImport {
        target_file: PathBuf,
        unresolved_use: Option<String>,
        help_use: Option<String>,
    },
    MissingType {
        target_file: PathBuf,
        help_use: String,
    },
    MissingFunction {
        target_file: PathBuf,
        symbol: String,
        help_use: Option<String>,
    },
    UnresolvedCratePath {
        target_file: PathBuf,
        unresolved_use: String,
        symbol: String,
    },
    TraitBoundMissing {
        target_file: PathBuf,
        trait_name: String,
    },
}

fn attempt_semantic_compile_recovery(
    project_root: &Path,
    telemetry_root: &Path,
) -> Result<bool, String> {
    if !project_root.join("Cargo.toml").exists() {
        return Ok(false);
    }
    let lockfile_used = project_root.join("Cargo.lock").exists();
    let args = offline_cargo_check_args(None, lockfile_used);
    let result = run_cargo_process(project_root, &args).map_err(|err| {
        format!(
            "failed to run cargo check in {}: {err}",
            project_root.display()
        )
    })?;
    if result.0 {
        return Ok(false);
    }

    let message = primary_cargo_message(&result.1, &result.2);
    if is_dependency_unavailable_message(&message) {
        return Ok(false);
    }

    let Some(error) = classify_semantic_compile_error(&message) else {
        return Ok(false);
    };
    let source_index = ModuleSourceIndex::build(project_root)?;
    let (applied, telemetry) =
        apply_semantic_recovery(project_root, telemetry_root, &source_index, error)?;
    if applied {
        persist_semantic_recovery_telemetry(telemetry_root, &telemetry)?;
    }
    Ok(applied)
}

fn classify_semantic_compile_error(message: &str) -> Option<SemanticCompileError> {
    let target_file = parse_primary_error_file(message)?;
    if looks_like_malformed_import_batch(message) {
        return Some(SemanticCompileError::MalformedImportBatch {
            target_file,
            help_uses: extract_all_help_use_statements(message),
        });
    }
    if message.contains("unresolved import") {
        return Some(SemanticCompileError::MissingImport {
            target_file,
            unresolved_use: extract_primary_use_statement(message),
            help_use: extract_help_use_statement(message),
        });
    }
    if message.contains("cannot find type `") || message.contains("cannot find type ") {
        return extract_help_use_statement(message).map(|help_use| {
            SemanticCompileError::MissingType {
                target_file,
                help_use,
            }
        });
    }
    if message.contains("cannot find function `") && message.contains("in this scope") {
        return extract_missing_function_name(message).map(|symbol| {
            SemanticCompileError::MissingFunction {
                target_file,
                symbol,
                help_use: extract_help_use_statement(message),
            }
        });
    }
    if message.contains("failed to resolve: use of unresolved module or unlinked crate")
        || message.contains("use of undeclared crate or module")
    {
        let unresolved_use = extract_primary_use_statement(message)?;
        let symbol = unresolved_use
            .trim_end_matches(';')
            .split("::")
            .last()
            .map(str::trim)?
            .to_string();
        return Some(SemanticCompileError::UnresolvedCratePath {
            target_file,
            unresolved_use,
            symbol,
        });
    }
    let trait_name = ["Send", "Sync", "Clone", "Debug"]
        .into_iter()
        .find(|name| message.contains(&format!("trait bound")) && message.contains(name))?;
    Some(SemanticCompileError::TraitBoundMissing {
        target_file,
        trait_name: trait_name.to_string(),
    })
}

fn apply_semantic_recovery(
    project_root: &Path,
    telemetry_root: &Path,
    source_index: &ModuleSourceIndex,
    error: SemanticCompileError,
) -> Result<(bool, SemanticRecoveryTelemetry), String> {
    match error {
        SemanticCompileError::MalformedImportBatch {
            target_file,
            help_uses,
        } => {
            let path = project_root.join(&target_file);
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let stable_preserved = stable_imports_in_content(&content);
            let normalization = normalize_malformed_import_block(&content, &help_uses)
                .ok_or_else(|| "malformed_import_unrecoverable".to_string())?;
            let applied = normalization.content != content;
            if applied {
                fs::write(&path, &normalization.content)
                    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
                persist_malformed_import_recovery_telemetry(
                    telemetry_root,
                    &MalformedImportRecoveryTelemetry {
                        malformed_import_recovery: MalformedImportRecoveryTelemetryData {
                            file: target_file.display().to_string(),
                            imports_fixed: normalization.imports_fixed,
                            group_normalized: normalization.group_normalized,
                            used_rustc_help_batch: !help_uses.is_empty(),
                            stable_preserved,
                        },
                    },
                )?;
            }
            Ok((
                applied,
                SemanticRecoveryTelemetry {
                    semantic_recovery: SemanticRecoveryTelemetryData {
                        error_type: if applied {
                            "MalformedImportBatch".to_string()
                        } else {
                            "malformed_import_unrecoverable".to_string()
                        },
                        used_rustc_help: !help_uses.is_empty(),
                        patch_family: if applied {
                            "batch_import_normalization".to_string()
                        } else {
                            "classified_stop".to_string()
                        },
                        green_state_preserved: stable_preserved,
                    },
                },
            ))
        }
        SemanticCompileError::MissingImport {
            target_file,
            unresolved_use,
            help_use,
        } => {
            let mut content =
                fs::read_to_string(project_root.join(&target_file)).map_err(|err| {
                    format!(
                        "failed to read {}: {err}",
                        project_root.join(&target_file).display()
                    )
                })?;
            let green_state_preserved = preserves_stable_domain_import_hub(&content);
            let before = content.clone();
            if green_state_preserved {
                content = remove_hallucinated_root_imports(
                    project_root,
                    source_index,
                    &target_file,
                    &content,
                )?;
            } else if let Some(unresolved_use) = unresolved_use.as_deref() {
                content = remove_exact_use_line(&content, unresolved_use);
            }
            let mut used_rustc_help = false;
            if let Some(help_use) = help_use.as_deref()
                && !content.lines().any(|line| line.trim() == help_use)
            {
                content = insert_use_statement(&content, help_use);
                used_rustc_help = true;
            }
            let applied = content != before;
            if applied {
                fs::write(project_root.join(&target_file), content).map_err(|err| {
                    format!(
                        "failed to write {}: {err}",
                        project_root.join(&target_file).display()
                    )
                })?;
            }
            Ok((
                applied,
                SemanticRecoveryTelemetry {
                    semantic_recovery: SemanticRecoveryTelemetryData {
                        error_type: "MissingImport".to_string(),
                        used_rustc_help,
                        patch_family: "safe_import_fix".to_string(),
                        green_state_preserved,
                    },
                },
            ))
        }
        SemanticCompileError::MissingType {
            target_file,
            help_use,
        } => {
            let path = project_root.join(&target_file);
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let green_state_preserved = preserves_stable_domain_import_hub(&content);
            let updated = if content.lines().any(|line| line.trim() == help_use) {
                content.clone()
            } else {
                insert_use_statement(&content, &help_use)
            };
            let applied = updated != content;
            if applied {
                fs::write(&path, updated)
                    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
            }
            Ok((
                applied,
                SemanticRecoveryTelemetry {
                    semantic_recovery: SemanticRecoveryTelemetryData {
                        error_type: "MissingType".to_string(),
                        used_rustc_help: true,
                        patch_family: "safe_import_fix".to_string(),
                        green_state_preserved,
                    },
                },
            ))
        }
        SemanticCompileError::MissingFunction {
            target_file,
            symbol,
            help_use,
        } => {
            let path = project_root.join(&target_file);
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let green_state_preserved = preserves_stable_domain_import_hub(&content);
            let current_id = source_index
                .qualified_id_for_path(&target_file)
                .ok_or_else(|| {
                    format!("failed to resolve module id for {}", target_file.display())
                })?;
            let used_rustc_help = help_use.is_some();
            let preferred_use =
                crate_root_reexport_use_line(project_root, &current_id.crate_name, &symbol)?
                    .or(help_use);
            let updated = if let Some(use_line) = preferred_use {
                if content.lines().any(|line| line.trim() == use_line) {
                    content.clone()
                } else {
                    insert_use_statement(&content, &use_line)
                }
            } else {
                content.clone()
            };
            let applied = updated != content;
            if applied {
                fs::write(&path, updated)
                    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
            }
            Ok((
                applied,
                SemanticRecoveryTelemetry {
                    semantic_recovery: SemanticRecoveryTelemetryData {
                        error_type: "MissingFunction".to_string(),
                        used_rustc_help,
                        patch_family: "safe_import_fix".to_string(),
                        green_state_preserved,
                    },
                },
            ))
        }
        SemanticCompileError::UnresolvedCratePath {
            target_file,
            unresolved_use,
            symbol,
        } => {
            let path = project_root.join(&target_file);
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let green_state_preserved = preserves_stable_domain_import_hub(&content);
            let updated = apply_local_trait_fallback_to_content(&content, &unresolved_use, &symbol);
            let applied = updated != content;
            if applied {
                fs::write(&path, updated)
                    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
            }
            Ok((
                applied,
                SemanticRecoveryTelemetry {
                    semantic_recovery: SemanticRecoveryTelemetryData {
                        error_type: "UnresolvedCratePath".to_string(),
                        used_rustc_help: false,
                        patch_family: "local_trait_fallback".to_string(),
                        green_state_preserved,
                    },
                },
            ))
        }
        SemanticCompileError::TraitBoundMissing {
            target_file: _,
            trait_name: _,
        } => Ok((
            false,
            SemanticRecoveryTelemetry {
                semantic_recovery: SemanticRecoveryTelemetryData {
                    error_type: "TraitBoundMissing".to_string(),
                    used_rustc_help: false,
                    patch_family: "classified_stop".to_string(),
                    green_state_preserved: true,
                },
            },
        )),
    }
}

fn parse_primary_error_file(message: &str) -> Option<PathBuf> {
    let section = primary_error_section(message);
    section.lines().find_map(|line| {
        line.trim()
            .strip_prefix("--> ")
            .and_then(|rest| rest.split(':').next())
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
    })
}

fn extract_primary_use_statement(message: &str) -> Option<String> {
    let section = primary_error_section(message);
    section.lines().find_map(normalize_embedded_use_statement)
}

fn primary_error_section(message: &str) -> String {
    let start = message
        .lines()
        .position(|line| line.trim_start().starts_with("error"))
        .unwrap_or(0);
    message.lines().skip(start).collect::<Vec<_>>().join("\n")
}

fn extract_help_use_statement(message: &str) -> Option<String> {
    let section = primary_error_section(message);
    let mut saw_help = false;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("help:") {
            saw_help = true;
            continue;
        }
        if saw_help {
            if let Some(use_line) = normalize_embedded_use_statement(trimmed) {
                return Some(use_line);
            }
            if !trimmed.is_empty() && trimmed != "|" {
                saw_help = false;
            }
        }
    }
    None
}

fn extract_all_help_use_statements(message: &str) -> Vec<String> {
    let section = primary_error_section(message);
    let mut uses = Vec::new();
    let mut saw_help = false;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("help:") {
            saw_help = true;
            continue;
        }
        if saw_help {
            if let Some(use_line) = normalize_embedded_use_statement(trimmed) {
                uses.push(use_line);
                continue;
            }
            if !trimmed.is_empty() && trimmed != "|" {
                saw_help = false;
            }
        }
    }
    uses.sort();
    uses.dedup();
    uses
}

fn normalize_embedded_use_statement(line: &str) -> Option<String> {
    let start = line.find("use ")?;
    let candidate = line[start..].trim();
    candidate.ends_with(';').then(|| candidate.to_string())
}

fn extract_missing_function_name(message: &str) -> Option<String> {
    let section = primary_error_section(message);
    section.lines().find_map(|line| {
        let trimmed = line.trim();
        let start = trimmed.find("cannot find function `")?;
        let rest = &trimmed[start + "cannot find function `".len()..];
        let end = rest.find('`')?;
        let symbol = rest[..end].trim();
        (!symbol.is_empty()).then(|| symbol.to_string())
    })
}

fn insert_use_statement(content: &str, use_line: &str) -> String {
    let mut lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    lines.insert(0, use_line.to_string());
    sort_leading_rust_imports(&lines.join("\n"))
}

fn remove_exact_use_line(content: &str, use_line: &str) -> String {
    let filtered = content
        .lines()
        .filter(|line| line.trim() != use_line.trim())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut updated = filtered.join("\n");
    if content.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn remove_hallucinated_root_imports(
    root: &Path,
    source_index: &ModuleSourceIndex,
    target_file: &Path,
    content: &str,
) -> Result<String, String> {
    let current_id = source_index
        .qualified_id_for_path(target_file)
        .ok_or_else(|| format!("failed to resolve module id for {}", target_file.display()))?;
    let mut retained = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let keep = if trimmed.starts_with("use crate::")
            && trimmed.ends_with(';')
            && trimmed.matches("::").count() == 1
        {
            let item = trimmed
                .trim_start_matches("use crate::")
                .trim_end_matches(';')
                .trim();
            source_index.crate_root_publicly_exports(root, &current_id.crate_name, item)?
        } else {
            true
        };
        if keep {
            retained.push(line.to_string());
        }
    }
    let mut updated = retained.join("\n");
    if content.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }
    Ok(updated)
}

fn apply_local_trait_fallback_to_content(
    content: &str,
    unresolved_use: &str,
    symbol: &str,
) -> String {
    if !symbol.ends_with("Interface") || content.contains(&format!("pub trait {symbol}")) {
        return content.to_string();
    }
    let without_use = remove_exact_use_line(content, unresolved_use);
    let fallback = generate_interface_trait_source(
        symbol,
        &(symbol.to_string(), "semantic_recovery".to_string()),
    );
    if without_use.starts_with("use ") {
        let lines = without_use
            .lines()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let insert_at = lines
            .iter()
            .position(|line| !line.trim_start().starts_with("use "))
            .unwrap_or(lines.len());
        let mut updated = lines;
        updated.insert(insert_at, String::new());
        updated.insert(insert_at + 1, fallback.trim_end().to_string());
        let mut body = updated.join("\n");
        if !body.ends_with('\n') {
            body.push('\n');
        }
        return body;
    }
    format!("{fallback}\n{without_use}")
}

fn looks_like_malformed_import_batch(message: &str) -> bool {
    message.contains("expected identifier, found keyword `use`")
        || message.contains("expected one of `,`, `::`, `as`, or `}`")
        || message.contains("this file contains an unclosed delimiter")
        || message.contains("unresolved import `")
            && message.contains("stable_v03::dynamic_ir::r#use")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportBlockNormalization {
    content: String,
    imports_fixed: usize,
    group_normalized: bool,
}

fn normalize_malformed_import_block(
    content: &str,
    help_uses: &[String],
) -> Option<ImportBlockNormalization> {
    let (start, end) = top_import_block_range(content)?;
    let block_lines = content
        .lines()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let parsed = parse_tolerant_import_block(&block_lines)?;
    let mut imports = parsed.imports;
    let original_imports = imports.clone();
    let stable_existing = stable_imports_in_lines(&block_lines);
    for help_use in help_uses {
        if !imports
            .iter()
            .any(|existing| existing.trim() == help_use.trim())
        {
            imports.push(help_use.clone());
        }
    }
    let mut normalized_block = imports.join("\n");
    if !normalized_block.is_empty() {
        normalized_block.push('\n');
    }

    let mut rebuilt = String::new();
    let lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    for line in &lines[..start] {
        rebuilt.push_str(line);
        rebuilt.push('\n');
    }
    rebuilt.push_str(&normalized_block);
    for line in &lines[end..] {
        rebuilt.push_str(line);
        rebuilt.push('\n');
    }
    if !content.ends_with('\n') {
        rebuilt.pop();
    }

    Some(ImportBlockNormalization {
        content: rebuilt,
        imports_fixed: imports.len().saturating_sub(stable_existing.len()),
        group_normalized: parsed.group_normalized || imports != original_imports,
    })
}

fn top_import_block_range(content: &str) -> Option<(usize, usize)> {
    let lines = content.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim_start().starts_with("use "))?;
    let mut end = start;
    let mut brace_depth = 0isize;
    let mut saw_group = false;
    while end < lines.len() {
        let trimmed = lines[end].trim();
        if trimmed.contains('!') {
            return None;
        }
        if trimmed.is_empty() {
            end += 1;
            continue;
        }
        if !trimmed.starts_with("use ")
            && !trimmed.starts_with("pub use ")
            && !(brace_depth > 0 && looks_like_import_continuation(trimmed))
        {
            break;
        }
        brace_depth += trimmed.matches('{').count() as isize;
        brace_depth -= trimmed.matches('}').count() as isize;
        if brace_depth < 0 || brace_depth > 2 {
            return None;
        }
        saw_group |= trimmed.contains('{');
        end += 1;
        if brace_depth == 0
            && saw_group
            && end < lines.len()
            && !lines[end].trim().starts_with("use ")
        {
            break;
        }
    }
    Some((start, end))
}

fn looks_like_import_continuation(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "}"
        || trimmed == "};"
        || trimmed.ends_with(',')
        || trimmed.ends_with("::")
        || trimmed.contains("::{")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedImportBlock {
    imports: Vec<String>,
    group_normalized: bool,
}

fn parse_tolerant_import_block(lines: &[String]) -> Option<ParsedImportBlock> {
    let mut imports = Vec::new();
    let mut current_group_prefix: Option<String> = None;
    let mut current_group_items = Vec::<String>::new();
    let mut group_normalized = false;

    for raw_line in lines {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("use ")
            .or_else(|| trimmed.strip_prefix("pub use "))
        {
            if let Some((prefix, symbols)) = parse_braced_crate_import(rest.trim_end_matches(';')) {
                flush_group(
                    &mut imports,
                    &mut current_group_prefix,
                    &mut current_group_items,
                );
                current_group_prefix = Some(prefix.to_string());
                current_group_items.extend(symbols);
                group_normalized = true;
                continue;
            }
            if rest.contains('{') && !rest.contains('}') {
                flush_group(
                    &mut imports,
                    &mut current_group_prefix,
                    &mut current_group_items,
                );
                let prefix = rest.split('{').next()?.trim().trim_end_matches("::");
                current_group_prefix = Some(prefix.to_string());
                let trailing = rest.split('{').nth(1).unwrap_or_default();
                collect_group_items(trailing, &mut current_group_items);
                group_normalized = true;
                continue;
            }
            imports.push(format!("use {};", rest.trim_end_matches(';').trim()));
            continue;
        }
        if current_group_prefix.is_some() {
            collect_group_items(trimmed, &mut current_group_items);
            if trimmed.contains('}') {
                flush_group(
                    &mut imports,
                    &mut current_group_prefix,
                    &mut current_group_items,
                );
            }
            group_normalized = true;
            continue;
        }
        return None;
    }
    flush_group(
        &mut imports,
        &mut current_group_prefix,
        &mut current_group_items,
    );
    Some(ParsedImportBlock {
        imports,
        group_normalized,
    })
}

fn collect_group_items(fragment: &str, items: &mut Vec<String>) {
    let cleaned = fragment.replace("};", "").replace('}', "").replace('{', "");
    for item in cleaned.split(',') {
        let trimmed = item.trim();
        if let Some(normalized) = normalize_group_item(trimmed) {
            items.push(normalized);
        }
    }
}

fn normalize_group_item(item: &str) -> Option<String> {
    let trimmed = item.trim().trim_end_matches("::").trim();
    if trimmed.is_empty() || trimmed == ";" {
        return None;
    }
    let valid = trimmed.split("::").all(is_valid_module_token);
    valid.then(|| trimmed.to_string())
}

fn is_valid_module_token(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn flush_group(imports: &mut Vec<String>, prefix: &mut Option<String>, items: &mut Vec<String>) {
    let Some(prefix) = prefix.take() else {
        return;
    };
    items.sort();
    items.dedup();
    if items.is_empty() {
        return;
    }
    if items.len() == 1 {
        imports.push(format!("use {prefix}::{};", items[0]));
    } else {
        imports.push(format!("use {prefix}::{{{}}};", items.join(", ")));
    }
    items.clear();
}

fn stable_imports_in_content(content: &str) -> bool {
    !stable_imports_in_lines(&content.lines().map(ToString::to_string).collect::<Vec<_>>())
        .is_empty()
}

fn stable_imports_in_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.trim().to_string())
        .filter(|line| {
            line.starts_with("use ")
                && !line.contains('{')
                && line.ends_with(';')
                && !line.contains("::r#use")
        })
        .collect()
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
            draft.content = generate_interface_trait_source(&name, &between);
        }
        Edit::ReplaceDependency {
            from,
            to,
            via,
            target_file,
        } => {
            let explicit_target = fence.explicit_target.as_deref().map(Path::to_path_buf);
            let canonical = target_file;
            let file_path = if canonical.is_some() {
                prune_patch_candidates(root, fence, vec![explicit_target, canonical])?
            } else {
                let resolved = resolved_paths.get(&from).cloned();
                let discovered = source_index
                    .resolve_apply_target(&from)
                    .map(|resolution| resolution.resolved_path)
                    .or_else(|| source_index.resolve(&from).ok().flatten());
                prune_patch_candidates(root, fence, vec![explicit_target, resolved, discovered])?
            };
            let Some(file_path) = file_path else {
                return Ok(());
            };
            let file_key = normalize_path_for_scope(&file_path).display().to_string();
            guard_change_target(Path::new(&file_key), fence)?;
            let draft = load_or_create_draft(root, drafts, &file_key, false)?;
            draft.content = update_dependency_content(
                root,
                source_index,
                Path::new(&file_key),
                &draft.content,
                &to,
                via.as_deref(),
            )?;
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
        candidates.retain(|candidate| explicit_scope_allows_target(explicit_target, candidate));
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

fn patch_target_module(patch: &CodePatch) -> Option<String> {
    match patch.operations.first()? {
        PatchOperation::CreateInterface { between, .. } => Some(between.0.clone()),
        PatchOperation::UpdateDependency { from, .. } => Some(from.clone()),
        PatchOperation::SplitModule { module, .. } => Some(module.clone()),
        PatchOperation::ExtractComponent { from, .. } => Some(from.clone()),
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

fn rewrite_crate_imports_for_created_drafts(
    root: &Path,
    source_index: &ModuleSourceIndex,
    drafts: &mut BTreeMap<String, FileDraft>,
) -> Result<(), String> {
    let mut created_only = drafts
        .iter()
        .filter(|(_, draft)| draft.change_type == ChangeType::CreateFile)
        .map(|(file_key, draft)| (file_key.clone(), draft.clone()))
        .collect::<BTreeMap<_, _>>();
    rewrite_crate_imports_for_generated_files(root, source_index, &mut created_only)?;
    for (file_key, draft) in created_only {
        if let Some(existing) = drafts.get_mut(&file_key) {
            existing.content = draft.content;
        }
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
        if is_trait_definition_hub(&draft.content) {
            continue;
        }
        draft.content = insert_mod_declarations_into_content(&draft.content, &[stem.to_string()]);
    }
    Ok(())
}

fn drop_unstable_interface_synthesis_patches(
    root: &Path,
    source_index: &ModuleSourceIndex,
    patches: &[CodePatch],
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<Vec<CodePatch>, String> {
    let mut retained = Vec::new();
    for patch in patches {
        let Some(target_path) =
            patch_primary_target_path(root, source_index, patch, resolved_paths)?
        else {
            retained.push(patch.clone());
            continue;
        };
        let content = match fs::read_to_string(root.join(&target_path)) {
            Ok(content) => content,
            Err(_) => {
                retained.push(patch.clone());
                continue;
            }
        };
        if preserves_stable_domain_import_hub(&content) && patch_is_interface_synthesis(patch) {
            continue;
        }
        retained.push(patch.clone());
    }
    Ok(retained)
}

fn patch_primary_target_path(
    root: &Path,
    source_index: &ModuleSourceIndex,
    patch: &CodePatch,
    resolved_paths: &BTreeMap<String, PathBuf>,
) -> Result<Option<PathBuf>, String> {
    let module = match patch.operations.first() {
        Some(PatchOperation::CreateInterface { between, .. }) => Some(between.0.as_str()),
        Some(PatchOperation::UpdateDependency { from, .. }) => Some(from.as_str()),
        _ => None,
    };
    let Some(module) = module else {
        return Ok(None);
    };
    if let Some(path) = resolved_paths.get(module) {
        return Ok(Some(path.clone()));
    }
    Ok(source_index
        .resolve_apply_target(module)
        .map(|resolution| resolution.resolved_relative_path)
        .or_else(|| source_index.resolve(module).ok().flatten())
        .map(|path| normalize_target_scope_path(root, &path))
        .transpose()?)
}

fn patch_is_interface_synthesis(patch: &CodePatch) -> bool {
    patch.operations.iter().any(|operation| match operation {
        PatchOperation::CreateInterface { .. } => true,
        PatchOperation::UpdateDependency { via, .. } => via
            .as_deref()
            .map(|value| value.to_ascii_lowercase().contains("interface"))
            .unwrap_or(false),
        _ => false,
    }) || patch
        .description
        .to_ascii_lowercase()
        .contains("trait abstraction")
}

fn preserves_stable_domain_import_hub(content: &str) -> bool {
    has_stable_domain_reexport_import(content) && is_trait_definition_hub(content)
}

fn has_stable_domain_reexport_import(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim().replace(' ', "");
        trimmed == "usecrate::domain::{AgentInput,AgentOutput,DomainError};"
            || trimmed == "pubusecrate::domain::{AgentInput,AgentOutput,DomainError};"
            || (trimmed.starts_with("usecrate::domain::")
                && trimmed.contains("AgentInput")
                && trimmed.contains("AgentOutput")
                && trimmed.contains("DomainError"))
            || (trimmed.starts_with("pubusecrate::domain::")
                && trimmed.contains("AgentInput")
                && trimmed.contains("AgentOutput")
                && trimmed.contains("DomainError"))
    })
}

fn is_trait_definition_hub(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.trim_start().starts_with("pub trait "))
        && content
            .lines()
            .any(|line| line.trim_start().starts_with("pub use "))
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
    let mut updated = lines.join("\n");
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
            .or(resolve_symbol_use_path_with_reexports(
                root,
                source_index,
                current_id,
                &symbol,
            )?) {
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
    if parts.len() == 1 && is_rust_identifier(parts[0]) {
        if let Some(use_path) =
            generated_symbol_use_targets_path(root, current_id, generated_symbol_targets, parts[0])?
                .or(resolve_symbol_use_path_with_reexports(
                    root,
                    source_index,
                    current_id,
                    parts[0],
                )?)
        {
            return Ok(format!("use {use_path};"));
        }
        return Ok(line.to_string());
    }

    let last = *parts.last().unwrap_or(&"");
    if is_rust_identifier(last) {
        let module_prefix = parts[..parts.len() - 1].join("::");
        if let Some(rebound) =
            rebind_module_prefix(&current_id.crate_name, &module_prefix, generated_by_leaf)
        {
            return Ok(format!("use crate::{rebound}::{last};"));
        }
        if let Some(use_path) =
            generated_symbol_use_targets_path(root, current_id, generated_symbol_targets, last)?.or(
                resolve_symbol_use_path_with_reexports(root, source_index, current_id, last)?,
            )
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

fn resolve_symbol_use_path_with_reexports(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_id: &QualifiedModuleId,
    symbol: &str,
) -> Result<Option<String>, String> {
    if let Some(module_path) = source_index.resolve_symbol_module(
        root,
        &current_id.crate_name,
        Some(&current_id.module_path),
        symbol,
    )? {
        if is_local_symbol_module(&current_id.module_path, &module_path) {
            return Ok(Some(format!("crate::{module_path}::{symbol}")));
        }
        if let Some(use_line) = crate_root_reexport_use_line(root, &current_id.crate_name, symbol)?
        {
            let use_path = use_line
                .trim()
                .strip_prefix("use ")
                .and_then(|value| value.strip_suffix(';'))
                .map(ToString::to_string);
            if use_path.is_some() {
                return Ok(use_path);
            }
        }
        return Ok(Some(format!("crate::{module_path}::{symbol}")));
    }
    if let Some(use_line) = crate_root_reexport_use_line(root, &current_id.crate_name, symbol)? {
        let use_path = use_line
            .trim()
            .strip_prefix("use ")
            .and_then(|value| value.strip_suffix(';'))
            .map(ToString::to_string);
        if use_path.is_some() {
            return Ok(use_path);
        }
    }
    source_index.resolve_symbol_use_path(root, current_id, symbol)
}

fn is_local_symbol_module(current_module: &str, candidate_module: &str) -> bool {
    if current_module == candidate_module {
        return true;
    }
    let current_parent = current_module
        .rsplit_once("::")
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    let candidate_parent = candidate_module
        .rsplit_once("::")
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    current_parent == candidate_parent || current_parent == candidate_module
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

fn crate_root_reexport_use_line(
    root: &Path,
    crate_name: &str,
    symbol: &str,
) -> Result<Option<String>, String> {
    let Some(root_module) = crate_root_module_path(root, crate_name) else {
        return Ok(None);
    };
    let content = fs::read_to_string(root.join(&root_module)).map_err(|err| {
        format!(
            "failed to read {}: {err}",
            root.join(&root_module).display()
        )
    })?;
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(remainder) = trimmed
            .strip_prefix("pub use ")
            .and_then(|value| value.strip_suffix(';'))
        else {
            continue;
        };
        if reexport_statement_exports_symbol(remainder, symbol) {
            return Ok(Some(format!("use crate::{symbol};")));
        }
    }
    Ok(None)
}

fn crate_root_module_path(root: &Path, crate_name: &str) -> Option<PathBuf> {
    let normalized = crate_name.replace('-', "_");
    let candidates = [
        root.join("apps")
            .join(&normalized)
            .join("src")
            .join("lib.rs"),
        root.join("apps")
            .join(&normalized)
            .join("src")
            .join("main.rs"),
        root.join("crates")
            .join(&normalized)
            .join("src")
            .join("lib.rs"),
        root.join("crates")
            .join(&normalized)
            .join("src")
            .join("main.rs"),
        root.join("core")
            .join(&normalized)
            .join("src")
            .join("lib.rs"),
        root.join("core")
            .join(&normalized)
            .join("src")
            .join("main.rs"),
        root.join("contracts")
            .join(&normalized)
            .join("src")
            .join("lib.rs"),
        root.join("contracts")
            .join(&normalized)
            .join("src")
            .join("main.rs"),
        root.join("src").join("lib.rs"),
        root.join("src").join("main.rs"),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .and_then(|absolute| absolute.strip_prefix(root).ok().map(Path::to_path_buf))
}

fn reexport_statement_exports_symbol(remainder: &str, symbol: &str) -> bool {
    if let Some((_, symbols)) = parse_braced_crate_import(remainder) {
        return symbols
            .into_iter()
            .any(|candidate| reexported_symbol_name(&candidate).as_deref() == Some(symbol));
    }
    reexported_symbol_name(remainder).as_deref() == Some(symbol)
}

fn reexported_symbol_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, alias)) = trimmed.rsplit_once(" as ") {
        let alias = alias.trim();
        if !alias.is_empty() && alias != "_" {
            return Some(alias.to_string());
        }
        return None;
    }
    trimmed
        .rsplit("::")
        .next()
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty() && *candidate != "*")
        .map(ToString::to_string)
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

fn is_rust_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => (),
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn create_sandbox_workspace(root: &Path) -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let sandbox_root = std::env::temp_dir().join(format!("dbm_sandbox_{unique}"));
    copy_workspace_with_ignore_guard(root, &sandbox_root)?;
    Ok(sandbox_root)
}

pub fn create_validation_sandbox(root: &Path) -> Result<PathBuf, String> {
    create_sandbox_workspace(root)
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
    copy_workspace_with_ignore_guard(root, sandbox_root)
}

pub fn create_transactional_preview_sandbox(
    root: &Path,
    sandbox_root: &Path,
) -> Result<(), String> {
    create_transactional_sandbox(root, sandbox_root)
}

fn copy_workspace_with_ignore_guard(source_root: &Path, destination: &Path) -> Result<(), String> {
    let mut telemetry = SandboxCopyTelemetry::default();
    copy_workspace_subset(source_root, source_root, destination, &mut telemetry)?;
    telemetry.sandbox_copy.ignored_dirs.sort();
    telemetry.sandbox_copy.ignored_dirs.dedup();
    telemetry.sandbox_copy.copy_warnings.sort();
    telemetry.sandbox_copy.copy_warnings.dedup();
    persist_sandbox_copy_telemetry(source_root, &telemetry)?;
    Ok(())
}

fn copy_workspace_subset(
    source_root: &Path,
    current: &Path,
    destination: &Path,
    telemetry: &mut SandboxCopyTelemetry,
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
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        match sandbox_copy_decision(relative, file_type.is_dir()) {
            SandboxCopyDecision::Copy => {}
            SandboxCopyDecision::SkipDir(name, warning) => {
                telemetry.sandbox_copy.skipped_files += 1;
                telemetry.sandbox_copy.ignored_dirs.push(name);
                if let Some(warning) = warning {
                    telemetry.sandbox_copy.copy_warnings.push(warning);
                }
                continue;
            }
            SandboxCopyDecision::SkipFile(warning) => {
                telemetry.sandbox_copy.skipped_files += 1;
                telemetry.sandbox_copy.copy_warnings.push(warning);
                continue;
            }
        }
        if file_type.is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|err| format!("failed to inspect symlink {}: {err}", path.display()))?;
            let resolved = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(source_root).join(target)
            };
            let canonical = resolved
                .canonicalize()
                .map_err(|err| format!("failed to resolve symlink {}: {err}", path.display()))?;
            if !canonical.starts_with(source_root) {
                return Err(format!(
                    "symlink escape rejected during sandbox copy: {} -> {}",
                    path.display(),
                    canonical.display()
                ));
            }
            telemetry.sandbox_copy.skipped_files += 1;
            telemetry
                .sandbox_copy
                .copy_warnings
                .push(format!("symlink skipped: {}", relative.display()));
            continue;
        }
        let target = destination.join(relative);
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|err| format!("failed to create {}: {err}", target.display()))?;
            copy_workspace_subset(source_root, &path, destination, telemetry)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let metadata = fs::metadata(&path)
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if metadata.len() > 100 * 1024 * 1024 {
            telemetry.sandbox_copy.skipped_files += 1;
            telemetry.sandbox_copy.copy_warnings.push(format!(
                "{} skipped: file exceeds 100MB",
                relative.display()
            ));
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        match fs::copy(&path, &target) {
            Ok(_) => telemetry.sandbox_copy.copied_files += 1,
            Err(err) if should_tolerate_copy_failure(relative) => {
                telemetry.sandbox_copy.skipped_files += 1;
                telemetry.sandbox_copy.copy_warnings.push(format!(
                    "{} skipped: {}",
                    relative.display(),
                    err
                ));
            }
            Err(err) => {
                return Err(format!(
                    "failed to copy {} to {}: {err}",
                    path.display(),
                    target.display()
                ));
            }
        }
    }
    Ok(())
}

enum SandboxCopyDecision {
    Copy,
    SkipDir(String, Option<String>),
    SkipFile(String),
}

fn sandbox_copy_decision(relative: &Path, is_dir: bool) -> SandboxCopyDecision {
    let parts = path_parts(relative);
    let ignored_dirs = [
        ".git",
        ".dbm",
        "target",
        "node_modules",
        "dist",
        "build",
        "coverage",
        ".tmp",
        ".cache",
    ];
    if let Some(first) = parts.first()
        && ignored_dirs.contains(&first.as_str())
    {
        return if is_dir {
            SandboxCopyDecision::SkipDir(
                first.clone(),
                (first == "target").then(|| "*.part.bin skipped".to_string()),
            )
        } else {
            SandboxCopyDecision::SkipFile(format!("{first} skipped"))
        };
    }
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    if file_name == "Cargo.lock" {
        return SandboxCopyDecision::Copy;
    }
    let ignored_suffixes = [
        ".rmeta",
        ".rlib",
        ".o",
        ".so",
        ".dylib",
        ".part.bin",
        ".swp",
        ".tmp",
        ".lock",
    ];
    if file_name == ".DS_Store"
        || ignored_suffixes
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
    {
        return SandboxCopyDecision::SkipFile(format!("{file_name} skipped"));
    }
    SandboxCopyDecision::Copy
}

fn should_tolerate_copy_failure(relative: &Path) -> bool {
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    [".part.bin", ".tmp", ".lock", ".swp"]
        .iter()
        .any(|suffix| file_name.ends_with(suffix))
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
            if module.is_empty()
                || matches!(module, "self" | "super" | "crate")
                || !is_valid_module_token(module)
            {
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
    if let Some(relative) = representative_module_relative_path(
        root,
        change_set,
        None,
        candidate.map(|candidate| candidate.source_path.as_path()),
    ) {
        if relative.file_name().and_then(|name| name.to_str()) == Some("app.rs")
            && root.join(&relative).exists()
        {
            return Ok(relative);
        }
        return resolve_root_module_relative_from_target(root, &relative).ok_or_else(|| {
            format!(
                "failed to resolve root module file from canonical target {}",
                relative.display()
            )
        });
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
    root: &Path,
    change_set: &CodeChangeSet,
    explicit_target: Option<&Path>,
    last_successful_target: Option<&Path>,
) -> Option<PathBuf> {
    representative_target_file(root, change_set, explicit_target, last_successful_target)
}

fn representative_target_file(
    root: &Path,
    change_set: &CodeChangeSet,
    explicit_target: Option<&Path>,
    last_successful_target: Option<&Path>,
) -> Option<PathBuf> {
    change_set
        .canonical_target
        .as_deref()
        .and_then(sanitize_representative_target)
        .or_else(|| {
            explicit_target
                .and_then(|target| normalize_target_scope_path(root, target).ok())
                .and_then(|target| sanitize_representative_target(&target))
        })
        .or_else(|| canonical_patch_target_file(&change_set.patches))
        .or_else(|| {
            last_successful_target
                .and_then(|target| normalize_target_scope_path(root, target).ok())
                .and_then(|target| sanitize_representative_target(&target))
        })
        .or_else(|| {
            best_ranked_change_target(change_set)
                .and_then(|target| sanitize_representative_target(&target))
        })
        .or_else(|| sanitize_representative_target(Path::new("apps/cli/src/app.rs")))
}

fn sanitize_representative_target(path: &Path) -> Option<PathBuf> {
    let normalized = normalize_path_for_scope(path);
    if normalized.as_os_str().is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn best_ranked_change_target(change_set: &CodeChangeSet) -> Option<PathBuf> {
    change_set
        .changes
        .iter()
        .min_by_key(|change| {
            let path = Path::new(&change.file_path);
            (patch_canonical_rank(path), change.file_path.clone())
        })
        .map(|change| PathBuf::from(&change.file_path))
}

/// Returns the best representative target file from a patch set, using canonical rank
/// priority: adapter-style targets > semantic peers > interface mediation files > registration.
/// Does not depend on patch ordering.
fn canonical_patch_target_file(patches: &[CodePatch]) -> Option<PathBuf> {
    patches
        .iter()
        .filter_map(|patch| sanitize_representative_target(&patch.target_file))
        .min_by_key(|path| {
            let rank = patch_canonical_rank(path);
            (rank, path.as_os_str().to_owned())
        })
}

/// Rank a patch target file for representative selection.
/// Lower is higher priority.
///   0 — canonical adapter target (non-interface, non-registration)
///   1 — interface mediation file (name contains "interface" or "_world_interface")
///   2 — registration file (lib.rs / main.rs)
fn patch_canonical_rank(path: &Path) -> u8 {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if name == "lib.rs" || name == "main.rs" {
        2
    } else if stem.contains("_world_interface") || stem.to_lowercase().contains("interface") {
        1
    } else {
        0
    }
}

fn resolve_root_module_relative_from_target(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = relative.parent().map(Path::to_path_buf)?;
    loop {
        for candidate in ["lib.rs", "main.rs"] {
            let direct = current.join(candidate);
            if root.join(&direct).exists() {
                return Some(direct);
            }
        }
        for candidate in ["src/lib.rs", "src/main.rs"] {
            let nested = current.join(candidate);
            if root.join(&nested).exists() {
                return Some(nested);
            }
        }
        let manifest = current.join("Cargo.toml");
        if root.join(&manifest).exists() {
            for candidate in ["src/lib.rs", "src/main.rs"] {
                let package_root = current.join(candidate);
                if root.join(&package_root).exists() {
                    return Some(package_root);
                }
            }
        }
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent.to_path_buf();
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

fn run_transactional_cargo_check(
    root: &Path,
    sandbox_root: &Path,
    change_set: &CodeChangeSet,
    candidate: Option<&RefactorCandidate>,
    explicit_target: Option<&Path>,
) -> Result<Vec<String>, String> {
    if !sandbox_root.join("Cargo.toml").exists() {
        return Ok(Vec::new());
    }
    let canonical_target = primary_canonical_target(root, change_set, explicit_target)?;
    let package = infer_affected_crate(root, candidate, &canonical_target)?;
    match run_offline_cargo_check(sandbox_root, root, Some(&package)) {
        Ok(diagnostic) => Ok(vec![diagnostic]),
        Err(message) => Err(format!(
            "Transactional apply aborted: cargo check failed in sandbox. Real workspace remains unchanged.\ncanonical target: {}\n{message}",
            canonical_target.display()
        )),
    }
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

fn primary_canonical_target(
    root: &Path,
    change_set: &CodeChangeSet,
    explicit_target: Option<&Path>,
) -> Result<PathBuf, String> {
    representative_target_file(root, change_set, explicit_target, None).ok_or_else(|| {
        "unable to determine canonical target path for transactional apply".to_string()
    })
}

fn infer_affected_crate_from_workspace_path(
    workspace_root: &Path,
    workspace_target: &Path,
) -> Option<String> {
    let workspace_path = workspace_root.join(workspace_target);
    let mut current = workspace_path.parent().map(Path::to_path_buf);
    while let Some(dir) = current {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            if let Some(name) = parse_package_name(&manifest).ok().flatten() {
                return Some(name);
            }
        }
        if dir == workspace_root {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

fn infer_affected_crate(
    root: &Path,
    candidate: Option<&RefactorCandidate>,
    canonical_target_path: &Path,
) -> Result<String, String> {
    // Priority 1: candidate carries a known crate name.
    if let Some(candidate) = candidate {
        if !candidate.module_id.crate_name.is_empty() {
            return Ok(candidate.module_id.crate_name.clone());
        }
    }
    // Priority 2: infer directly from the canonical workspace-relative target path.
    if let Some(name) = infer_affected_crate_from_workspace_path(root, canonical_target_path) {
        return Ok(name);
    }
    // Priority 3: root-level Cargo.toml fallback (single-package workspace).
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

fn run_build_validation(root: &Path, telemetry_root: &Path) -> Result<(), String> {
    if !root.join("Cargo.toml").exists() {
        return Ok(());
    }
    run_offline_cargo_check(root, telemetry_root, None).map(|_| ())
}

fn run_offline_cargo_check(
    build_root: &Path,
    telemetry_root: &Path,
    package: Option<&str>,
) -> Result<String, String> {
    let lockfile_used = build_root.join("Cargo.lock").exists();
    let args = offline_cargo_check_args(package, lockfile_used);
    let result = run_cargo_process(build_root, &args).map_err(|err| {
        format!(
            "failed to run cargo check in {}: {err}",
            build_root.display()
        )
    })?;
    if result.0 {
        let telemetry = CargoResolutionTelemetry {
            cargo_resolution: CargoResolutionTelemetryData {
                offline: true,
                lockfile_used,
                cache_hit: true,
                dependency_unavailable: Vec::new(),
                graceful_degradation: false,
            },
        };
        persist_cargo_resolution_telemetry(telemetry_root, &telemetry)?;
        return Ok(format!(
            "cargo check{} succeeded (offline)",
            package.map(|pkg| format!(" -p {pkg}")).unwrap_or_default()
        ));
    }

    let mut message = primary_cargo_message(&result.1, &result.2);
    if is_dependency_unavailable_message(&message) && lockfile_used {
        let _ = run_cargo_process(
            build_root,
            &["metadata".to_string(), "--offline".to_string()],
        );
        let retry = run_cargo_process(build_root, &args).map_err(|err| {
            format!(
                "failed to run cargo check in {}: {err}",
                build_root.display()
            )
        })?;
        if retry.0 {
            let telemetry = CargoResolutionTelemetry {
                cargo_resolution: CargoResolutionTelemetryData {
                    offline: true,
                    lockfile_used,
                    cache_hit: true,
                    dependency_unavailable: Vec::new(),
                    graceful_degradation: false,
                },
            };
            persist_cargo_resolution_telemetry(telemetry_root, &telemetry)?;
            return Ok(format!(
                "cargo check{} succeeded (offline)",
                package.map(|pkg| format!(" -p {pkg}")).unwrap_or_default()
            ));
        }
        message = primary_cargo_message(&retry.1, &retry.2);
    }

    if is_dependency_unavailable_message(&message) {
        let dependencies = extract_unavailable_dependencies(&message);
        let telemetry = CargoResolutionTelemetry {
            cargo_resolution: CargoResolutionTelemetryData {
                offline: true,
                lockfile_used,
                cache_hit: false,
                dependency_unavailable: dependencies.clone(),
                graceful_degradation: true,
            },
        };
        persist_cargo_resolution_telemetry(telemetry_root, &telemetry)?;
        return Err(format!(
            "dependency_unavailable (offline cache miss)\n{message}"
        ));
    }

    let telemetry = CargoResolutionTelemetry {
        cargo_resolution: CargoResolutionTelemetryData {
            offline: true,
            lockfile_used,
            cache_hit: true,
            dependency_unavailable: Vec::new(),
            graceful_degradation: false,
        },
    };
    persist_cargo_resolution_telemetry(telemetry_root, &telemetry)?;
    Err(format!("build_error: {message}"))
}

pub fn run_transactional_preview_cargo_check(
    root: &Path,
    sandbox_root: &Path,
    package: &str,
) -> Result<String, String> {
    run_offline_cargo_check(sandbox_root, root, Some(package))
}

fn offline_cargo_check_args(package: Option<&str>, lockfile_used: bool) -> Vec<String> {
    let mut args = vec!["check".to_string(), "--offline".to_string()];
    if lockfile_used {
        args.push("--locked".to_string());
    }
    if let Some(package) = package {
        args.push("-p".to_string());
        args.push(package.to_string());
    }
    args
}

fn run_cargo_process(build_root: &Path, args: &[String]) -> Result<(bool, String, String), String> {
    let command = resolve_command("cargo").map_err(|err| err.to_string())?;
    let mut process = Command::new(command);
    process.current_dir(build_root);
    process.env_clear();
    for (key, value) in cargo_verification_env() {
        process.env(key, value);
    }
    process.args(args);
    if args.last().map(String::as_str) == Some("-p") {
        return Err("cargo package name missing".to_string());
    }
    let output = process.output().map_err(|err| err.to_string())?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn cargo_verification_env() -> Vec<(String, String)> {
    let mut env = fixed_env();
    env.push(("CARGO_NET_OFFLINE".to_string(), "true".to_string()));
    env.push(("CARGO_TERM_COLOR".to_string(), "never".to_string()));
    env
}

fn primary_cargo_message(stdout: &str, stderr: &str) -> String {
    if !stderr.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        stdout.trim().to_string()
    }
}

fn is_dependency_unavailable_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "failed to get",
        "download of config.json failed",
        "index update failed",
        "spurious network error",
        "could not resolve host",
        "no matching package named",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn extract_unavailable_dependencies(message: &str) -> Vec<String> {
    let mut dependencies = BTreeSet::new();
    for line in message.lines() {
        if let Some(rest) = line.split("failed to get `").nth(1)
            && let Some(name) = rest.split('`').next()
            && !name.trim().is_empty()
        {
            dependencies.insert(name.trim().to_string());
        }
        if let Some(rest) = line.split("no matching package named `").nth(1)
            && let Some(name) = rest.split('`').next()
            && !name.trim().is_empty()
        {
            dependencies.insert(name.trim().to_string());
        }
    }
    dependencies.into_iter().collect()
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
        let replacement = render_change_replacement(
            change,
            &fs::read_to_string(root.join(&change.file_path)).unwrap_or_default(),
        )
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
    if explicit_scope_allows_target(&expected, &target) {
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
    path.components()
        .fold(PathBuf::new(), |mut normalized, component| {
            match component {
                Component::CurDir => {}
                other => normalized.push(other.as_os_str()),
            }
            normalized
        })
}

fn explicit_scope_allows_target(explicit_target: &Path, candidate: &Path) -> bool {
    let explicit = normalize_path_for_scope(explicit_target);
    let candidate = normalize_path_for_scope(candidate);
    if candidate == explicit {
        return true;
    }
    if matches!(
        candidate.file_name().and_then(|value| value.to_str()),
        Some("lib.rs" | "main.rs")
    ) {
        return true;
    }
    let is_interface_file = candidate
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.ends_with("_interface"))
        .unwrap_or(false);
    if is_interface_file {
        return true;
    }
    candidate.parent() == explicit.parent()
}

fn normalize_target_scope_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        PathBuf::from(normalize_relative(root, path)?)
    } else {
        path.to_path_buf()
    };
    Ok(normalize_path_for_scope(&candidate))
}

fn update_dependency_content(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    content: &str,
    target: &str,
    via: Option<&str>,
) -> Result<String, String> {
    if preserves_stable_domain_import_hub(content)
        && via
            .map(|value| value.to_ascii_lowercase().contains("interface"))
            .unwrap_or(false)
    {
        return Ok(content.to_string());
    }

    let desired = desired_dependency_use_line(root, source_index, current_path, target, via)?;
    if desired.starts_with("use crate::")
        && !desired.contains("::{")
        && desired.matches("::").count() == 1
        && let Some(current_id) = source_index.qualified_id_for_path(current_path)
        && let Some(root_item) = desired
            .trim_start_matches("use crate::")
            .trim_end_matches(';')
            .split("::")
            .next()
        && !source_index.crate_root_publicly_exports(root, &current_id.crate_name, root_item)?
    {
        return Ok(content.to_string());
    }
    let target_prefix = format!("use crate::{}", snake_case(target));
    let mut lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    if matches!(via, Some(value) if value.ends_with("Interface")) {
        let interface_is_used = symbol_used_outside_imports(content, via.unwrap_or_default());
        if let Some(index) = lines
            .iter()
            .position(|line| line.trim_start().starts_with(&target_prefix))
        {
            let preserve_target_import = imported_symbols_still_required(
                root,
                source_index,
                current_path,
                content,
                &lines[index],
            )?;
            if interface_is_used && !preserve_target_import {
                lines[index] = desired.clone();
            } else if !interface_is_used && !preserve_target_import {
                lines.remove(index);
            } else if interface_is_used && !lines.iter().any(|line| line.trim() == desired) {
                lines.insert(0, desired);
            }
        } else if !lines.iter().any(|line| line.trim() == desired) {
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
    updated = preserve_interface_only_break_cycle_semantics(&updated);
    Ok(updated)
}

fn deterministic_interface_method_name(name: &str, between: &(String, String)) -> String {
    let trait_name = name.to_ascii_lowercase();
    if trait_name.contains("adapterserviceinterface") {
        return "execute_service".to_string();
    }
    if trait_name.contains("consistencyrecommendinterface") {
        return "evaluate_consistency".to_string();
    }
    if trait_name.contains("patternextractorpatternmatcherinterface") {
        return "extract_pattern_signature".to_string();
    }
    if trait_name.contains("explanationintentrefinerinterface") {
        return "refine_explanation_intent".to_string();
    }
    let to = snake_case(&between.1);
    let rhs = to
        .split('_')
        .filter(|token| !token.is_empty() && *token != "interface")
        .collect::<Vec<_>>()
        .join("_");
    if rhs.is_empty() {
        "handle_target".to_string()
    } else {
        format!("handle_{rhs}")
    }
}

fn generate_interface_trait_source(name: &str, between: &(String, String)) -> String {
    format!(
        "pub trait {name} {{\n    fn {}(&self);\n}}\n",
        deterministic_interface_method_name(name, between)
    )
}

fn desired_dependency_use_line(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    target: &str,
    via: Option<&str>,
) -> Result<String, String> {
    Ok(match via {
        Some(via) if via.ends_with("Interface") => {
            if let Some(current_id) = source_index.qualified_id_for_path(current_path) {
                let module_leaf = snake_case(via);
                interface_use_line_for_target(
                    root,
                    source_index,
                    current_path,
                    &current_id,
                    target,
                    &module_leaf,
                    via,
                )
            } else {
                format!("use crate::{}::{};", snake_case(via), via)
            }
        }
        Some(via) => format!("use crate::{};", snake_case(via)),
        None => {
            let _ = root;
            format!("use crate::{};", snake_case(target))
        }
    })
}

fn interface_use_line_for_target(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    current_id: &QualifiedModuleId,
    target: &str,
    module_leaf: &str,
    interface_name: &str,
) -> String {
    if let Some(target_path) = source_index
        .resolve_apply_target(target)
        .map(|resolution| resolution.resolved_relative_path)
        .or_else(|| source_index.resolve(target).ok().flatten())
        && let Some(target_id) = source_index.qualified_id_for_path(&target_path)
    {
        if target_id.crate_name == current_id.crate_name {
            if interface_is_sibling_module(root, current_path, module_leaf)
                && !is_module_root_file(current_path)
            {
                return format!("use super::{module_leaf}::{interface_name};");
            }

            return match parent_module_root(current_path, current_id) {
                Some(parent) if parent == current_id.crate_name => {
                    format!("use crate::{module_leaf}::{interface_name};")
                }
                Some(parent) => format!("use crate::{parent}::{module_leaf}::{interface_name};"),
                None => format!("use crate::{module_leaf}::{interface_name};"),
            };
        }

        return format!(
            "use {}::{module_leaf}::{interface_name};",
            target_id.crate_name
        );
    }
    format!("use crate::{module_leaf}::{interface_name};")
}

fn interface_is_sibling_module(root: &Path, current_path: &Path, module_leaf: &str) -> bool {
    let absolute = root.join(current_path);
    let Some(parent) = absolute.parent() else {
        return false;
    };
    parent.join(format!("{module_leaf}.rs")).exists()
        || parent.join(module_leaf).join("mod.rs").exists()
}

fn is_module_root_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("mod.rs" | "lib.rs" | "main.rs")
    )
}

fn parent_module_root(current_path: &Path, current_id: &QualifiedModuleId) -> Option<String> {
    if is_module_root_file(current_path) {
        return Some(current_id.module_path.clone());
    }
    let (parent, _) = current_id.module_path.rsplit_once("::")?;
    Some(parent.to_string())
}

fn imported_symbols_still_required(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    content: &str,
    use_line: &str,
) -> Result<bool, String> {
    let Some(imported_symbols) = imported_symbols_from_use_line(use_line) else {
        return Ok(true);
    };
    for symbol in imported_symbols {
        if symbol_used_outside_imports(content, &symbol)
            || imported_trait_method_is_used(
                root,
                source_index,
                current_path,
                use_line,
                &symbol,
                content,
            )?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn imported_symbols_from_use_line(use_line: &str) -> Option<Vec<String>> {
    let trimmed = use_line.trim();
    let remainder = trimmed.strip_prefix("use crate::")?.strip_suffix(';')?;
    if let Some((_, symbols)) = parse_braced_crate_import(remainder) {
        let imported = symbols
            .into_iter()
            .filter_map(|symbol| imported_symbol_name(&symbol))
            .collect::<Vec<_>>();
        return Some(imported);
    }
    let symbol = remainder.rsplit("::").next()?;
    if !is_rust_symbol(symbol) {
        return Some(Vec::new());
    }
    Some(vec![imported_symbol_name(symbol)?])
}

fn imported_symbol_name(symbol: &str) -> Option<String> {
    let trimmed = symbol.trim();
    if let Some((_, alias)) = trimmed.split_once(" as ") {
        return Some(alias.trim().to_string());
    }
    Some(trimmed.to_string())
}

fn imported_trait_method_is_used(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    use_line: &str,
    symbol: &str,
    content: &str,
) -> Result<bool, String> {
    let Some(trait_methods) =
        imported_trait_methods(root, source_index, current_path, use_line, symbol)?
    else {
        return Ok(false);
    };
    Ok(trait_methods
        .iter()
        .any(|method| content.contains(&format!(".{method}("))))
}

fn imported_trait_methods(
    root: &Path,
    source_index: &ModuleSourceIndex,
    current_path: &Path,
    use_line: &str,
    symbol: &str,
) -> Result<Option<Vec<String>>, String> {
    let Some(module_prefix) = imported_symbol_module_prefix(use_line, symbol) else {
        return Ok(None);
    };
    let current_id = match source_index.qualified_id_for_path(current_path) {
        Some(id) => id,
        None => return Ok(None),
    };
    let Some(module_path) = source_index
        .resolve_symbol_module(
            root,
            &current_id.crate_name,
            Some(&current_id.module_path),
            symbol,
        )?
        .filter(|resolved| resolved == &module_prefix)
        .or(Some(module_prefix))
    else {
        return Ok(None);
    };
    let Some(module_file) = source_index.resolve(&module_path)? else {
        return Ok(None);
    };
    let trait_source = fs::read_to_string(root.join(module_file))
        .map_err(|err| format!("failed to read trait source for {symbol}: {err}"))?;
    Ok(parse_trait_methods(&trait_source, symbol))
}

fn imported_symbol_module_prefix(use_line: &str, symbol: &str) -> Option<String> {
    let trimmed = use_line.trim();
    let remainder = trimmed.strip_prefix("use crate::")?.strip_suffix(';')?;
    if let Some((prefix, symbols)) = parse_braced_crate_import(remainder) {
        return symbols
            .iter()
            .any(|candidate| imported_symbol_name(candidate).as_deref() == Some(symbol))
            .then(|| prefix.to_string());
    }
    let (prefix, imported) = remainder.rsplit_once("::")?;
    (imported_symbol_name(imported).as_deref() == Some(symbol)).then(|| prefix.to_string())
}

fn parse_trait_methods(content: &str, trait_name: &str) -> Option<Vec<String>> {
    let trait_header = format!("trait {trait_name}");
    let mut in_trait = false;
    let mut brace_depth = 0usize;
    let mut methods = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !in_trait {
            if !trimmed.contains(&trait_header) {
                continue;
            }
            in_trait = true;
        }
        brace_depth += trimmed.matches('{').count();
        brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            let method = rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default();
            if !method.is_empty() {
                methods.push(method.to_string());
            }
        }
        if in_trait && brace_depth == 0 {
            break;
        }
    }
    (!methods.is_empty()).then_some(methods)
}

fn symbol_used_outside_imports(content: &str, symbol: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with("use ") && trimmed.contains(symbol)
    })
}

fn preserve_interface_only_break_cycle_semantics(content: &str) -> String {
    let content = remove_unused_adapter_world_interface_import(content);
    enforce_break_cycle_tuple_arity_invariants(&content)
}

fn remove_unused_adapter_world_interface_import(content: &str) -> String {
    if symbol_used_outside_imports(content, "AdapterWorldInterface") {
        return content.to_string();
    }
    remove_exact_use_line(
        content,
        "use crate::adapter_world_interface::AdapterWorldInterface;",
    )
}

fn enforce_break_cycle_tuple_arity_invariants(content: &str) -> String {
    if !content.contains("inferred_knowledge_graph") {
        return content.to_string();
    }

    let lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
    let destructure_start = lines.iter().position(|line| line.contains("let ("));
    let Some(destructure_start) = destructure_start else {
        return content.to_string();
    };
    let destructure_end = lines
        .iter()
        .enumerate()
        .skip(destructure_start)
        .find_map(|(index, line)| line.contains(") = if").then_some(index));
    let Some(destructure_end) = destructure_end else {
        return content.to_string();
    };
    if !lines[destructure_start..=destructure_end]
        .iter()
        .any(|line| line.contains("inferred_knowledge_graph"))
    {
        return content.to_string();
    }
    let lhs_bindings = lines[destructure_start..=destructure_end]
        .iter()
        .flat_map(|line| line.split(','))
        .filter_map(|part| {
            let token = part.trim();
            if token.is_empty()
                || token == "let ("
                || token == ") = if"
                || token.starts_with(") = if")
            {
                None
            } else {
                Some(token.trim_start_matches("let ").trim().to_string())
            }
        })
        .collect::<Vec<_>>();
    let lhs_arity = lhs_bindings.len();
    let Some(inferred_index) = lhs_bindings
        .iter()
        .position(|binding| binding == "inferred_knowledge_graph")
    else {
        return content.to_string();
    };

    let if_tuple_open = lines
        .iter()
        .enumerate()
        .skip(destructure_end + 1)
        .find_map(|(index, line)| (line.trim() == "(").then_some(index));
    let Some(if_tuple_open) = if_tuple_open else {
        return content.to_string();
    };
    let if_tuple_close = lines
        .iter()
        .enumerate()
        .skip(if_tuple_open + 1)
        .find_map(|(index, line)| (line.trim() == ")").then_some(index));
    let Some(if_tuple_close) = if_tuple_close else {
        return content.to_string();
    };

    let else_start = lines
        .iter()
        .enumerate()
        .skip(if_tuple_close)
        .find_map(|(index, line)| (line.trim() == "} else {").then_some(index));
    let Some(else_start) = else_start else {
        return content.to_string();
    };
    let else_tuple_open = lines
        .iter()
        .enumerate()
        .skip(else_start + 1)
        .find_map(|(index, line)| (line.trim() == "(").then_some(index));
    let Some(else_tuple_open) = else_tuple_open else {
        return content.to_string();
    };
    let else_tuple_close = lines
        .iter()
        .enumerate()
        .skip(else_tuple_open + 1)
        .find_map(|(index, line)| (line.trim() == ")").then_some(index));
    let Some(else_tuple_close) = else_tuple_close else {
        return content.to_string();
    };

    let mut updated = lines;
    let mut modified = false;
    for (tuple_open, tuple_close) in [
        (else_tuple_open, else_tuple_close),
        (if_tuple_open, if_tuple_close),
    ] {
        let tuple_items = updated[tuple_open + 1..tuple_close]
            .iter()
            .enumerate()
            .filter_map(|(offset, line)| {
                let trimmed = line.trim();
                (!trimmed.is_empty() && trimmed != "," && trimmed.ends_with(','))
                    .then_some((tuple_open + 1 + offset, trimmed.to_string()))
            })
            .collect::<Vec<_>>();
        if tuple_items.len() >= lhs_arity {
            continue;
        }
        if tuple_items.len() + 1 != lhs_arity || inferred_index > tuple_items.len() {
            return content.to_string();
        }
        let indent = tuple_items
            .first()
            .and_then(|(index, _)| {
                updated.get(*index).map(|candidate| {
                    let trimmed = candidate.trim_start();
                    candidate[..candidate.len().saturating_sub(trimmed.len())].to_string()
                })
            })
            .unwrap_or_else(|| "            ".to_string());
        let insert_index = tuple_items
            .get(inferred_index)
            .map(|(index, _)| *index)
            .unwrap_or(tuple_close);
        updated.insert(insert_index, format!("{indent}KnowledgeGraph::default(),"));
        modified = true;
    }
    if !modified {
        return content.to_string();
    }
    let mut rebuilt = updated.join("\n");
    if content.ends_with('\n') && !rebuilt.ends_with('\n') {
        rebuilt.push('\n');
    }
    rebuilt
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
    use crate::service::{CodingReport, ModuleNode};
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
        pr_view_json: &str,
        pr_create_url: &str,
    ) -> PathBuf {
        let script = dir.join("gh");
        let body = format!(
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  if [ \"{auth_ok}\" = \"true\" ]; then exit 0; else exit 1; fi\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"view\" ]; then\n  if [ -n '{pr_view_json}' ]; then\n    printf '%s' '{pr_view_json}'\n    exit 0\n  fi\n  exit 1\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  printf '%s' '{pr_create_url}'\n  exit 0\nfi\nexit 1\n"
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

    struct GhEnvGuard {
        prev: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl GhEnvGuard {
        fn new(path: &Path) -> Self {
            let _lock = crate::test_support::gh_bin_env_lock();
            let prev = std::env::var_os("DBM_GH_BIN");
            unsafe {
                std::env::set_var("DBM_GH_BIN", path);
            }
            Self { prev, _lock }
        }
    }

    impl Drop for GhEnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var("DBM_GH_BIN", v) },
                None => unsafe { std::env::remove_var("DBM_GH_BIN") },
            }
        }
    }

    fn with_fake_gh<T>(path: &Path, f: impl FnOnce() -> T) -> T {
        let _guard = GhEnvGuard::new(path);
        f()
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
            target_file: Default::default(),
        }];
        let edits = patches_to_edits(&patches);
        assert_eq!(
            edits,
            vec![Edit::ReplaceDependency {
                from: "renderer".to_string(),
                to: "world".to_string(),
                via: Some("renderer_world_interface".to_string()),
                target_file: None,
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
            target_file: Default::default(),
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
            target_file: Default::default(),
        }];

        let resolutions = collect_apply_target_resolutions(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/app.rs")),
            &BTreeMap::new(),
        )
        .expect("resolutions");
        assert_eq!(resolutions.len(), 1);
        assert_eq!(
            resolutions[0].resolved_relative_path,
            PathBuf::from("apps/cli/src/app.rs")
        );
        assert_eq!(resolutions[0].resolution_strategy, "target_override");
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
            target_file: Default::default(),
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
            target_file: Default::default(),
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
            target_file: Default::default(),
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
            target_file: Default::default(),
        }];
        let lhs = generate_code_change_set(&root, &patches).expect("lhs");
        let rhs = generate_code_change_set(&root, &patches).expect("rhs");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn break_cycle_change_set_representative_target() {
        let root = temp_dir("break_cycle_representative_target");
        fs::create_dir_all(root.join("crates/runtime/runtime_vm/src")).expect("runtime_vm");
        fs::write(
            root.join("crates/runtime/runtime_vm/src/lib.rs"),
            "pub mod adapter;\npub mod adapter_world_interface;\n",
        )
        .expect("lib");
        fs::write(
            root.join("crates/runtime/runtime_vm/src/adapter.rs"),
            "pub fn adapt() {}\n",
        )
        .expect("adapter");
        fs::write(
            root.join("crates/runtime/runtime_vm/src/adapter_world_interface.rs"),
            "pub trait AdapterWorldInterface {}\n",
        )
        .expect("interface");

        let adapter = PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs");
        let interface = PathBuf::from("crates/runtime/runtime_vm/src/adapter_world_interface.rs");
        let registration = PathBuf::from("crates/runtime/runtime_vm/src/lib.rs");
        let orderings = vec![
            vec![adapter.clone(), interface.clone(), registration.clone()],
            vec![registration.clone(), interface.clone(), adapter.clone()],
            vec![interface.clone(), registration.clone(), adapter.clone()],
        ];

        for ordering in orderings {
            let patches = ordering
                .iter()
                .enumerate()
                .map(|(index, path)| CodePatch {
                    patch_id: format!("p{}", index + 1),
                    action: RefactorPlanAction::MoveDependency {
                        from: "adapter".to_string(),
                        to: "world".to_string(),
                        via: Some("adapter_world_interface".to_string()),
                    },
                    operations: vec![PatchOperation::UpdateDependency {
                        from: "adapter".to_string(),
                        to: "world".to_string(),
                        via: Some("adapter_world_interface".to_string()),
                    }],
                    description: path.display().to_string(),
                    target_file: path.clone(),
                })
                .collect::<Vec<_>>();
            let change_set = patches_to_change_set(
                &root,
                &patches,
                None,
                &BTreeMap::new(),
                Some(adapter.as_path()),
            )
            .expect("change set");

            assert_eq!(
                representative_module_relative_path(&root, &change_set, None, None),
                Some(adapter.clone())
            );
            assert_eq!(
                primary_canonical_target(&root, &change_set, None).expect("canonical target"),
                adapter.clone()
            );
            assert_eq!(
                resolve_root_module_file_relative_for_change_set(&root, &change_set, None)
                    .expect("root module"),
                registration.clone()
            );
        }
    }

    #[test]
    fn plain_check_auto_commit_uses_representative_target() {
        let root = temp_dir("plain_check_auto_commit_representative_target");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[[bin]]\nname = \"design_cli\"\npath = \"src/app.rs\"\n",
        )
        .expect("cli cargo");
        fs::write(root.join("apps/cli/src/app.rs"), "pub fn run() {}\n").expect("app");
        init_git_repo(&root);

        let change_set = CodeChangeSet {
            patches: vec![],
            changes: vec![],
            summary: ChangeSummary::default(),
            canonical_target: Some(PathBuf::from(".")),
        };

        let result = execute_code_change_set(
            &root,
            &change_set,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: true,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: true,
                confirm_commit: false,
                prompt_commit: false,
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
        assert!(result.build_ok);
        assert_eq!(
            result.canonical_target_path.as_deref(),
            Some("apps/cli/src/app.rs")
        );
        assert_ne!(result.canonical_target_path.as_deref(), Some("."));
        assert!(result.git_commit.is_some());
        assert_eq!(
            result
                .git_commit
                .as_ref()
                .expect("commit preview")
                .diff_preview
                .len(),
            0
        );
        assert_eq!(
            resolve_root_module_file_relative_for_change_set(&root, &change_set, None)
                .expect("root module"),
            PathBuf::from("apps/cli/src/app.rs")
        );
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
            canonical_target: None,
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
                prompt_commit: false,
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
            canonical_target: None,
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
                prompt_commit: false,
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
            canonical_target: None,
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
                prompt_commit: false,
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
            canonical_target: None,
        };
        install_before_real_apply_hook(|root| {
            fs::write(
                root.join("src/runtime/determinism.rs"),
                "pub fn check() { let _ = 1; }\n",
            )
            .expect("mutate real file");
        });

        let result = transactional_apply(&root, &change_set, Some(&candidate), false, None)
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

        let result = restricted_commit(
            &[PathBuf::from("src/main.rs")],
            &root,
            None,
            true,
            false,
            None,
        )
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

        let result = restricted_commit(
            &[PathBuf::from("src/main.rs")],
            &root,
            None,
            false,
            false,
            None,
        )
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
    fn restricted_commit_blocks_overlapping_dirty_workspace() {
        let root = temp_dir("restricted_commit_overlap");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo(&root);
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"manual\"); }\n",
        )
        .expect("main");
        let pre_apply_dirty = BTreeSet::from([String::from("src/main.rs")]);

        let err = restricted_commit(
            &[PathBuf::from("src/main.rs")],
            &root,
            None,
            true,
            false,
            Some(&pre_apply_dirty),
        )
        .expect_err("overlap must block commit");
        assert!(err.contains("CommitBlocked"));
    }

    #[test]
    fn restricted_commit_allows_detached_head_with_warning_and_persists_telemetry() {
        let root = temp_dir("restricted_commit_detached");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/detached");
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .expect("rev-parse");
        let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        let checkout = Command::new("git")
            .args(["checkout", &sha])
            .current_dir(&root)
            .output()
            .expect("checkout detached");
        assert!(
            checkout.status.success(),
            "{}",
            String::from_utf8_lossy(&checkout.stderr)
        );
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"dbm\"); }\n",
        )
        .expect("main");

        let result = restricted_commit(
            &[PathBuf::from("src/main.rs")],
            &root,
            None,
            true,
            false,
            None,
        )
        .expect("restricted commit");

        assert!(result.commit_created);
        assert_eq!(result.warning.as_deref(), Some("warning: detached HEAD"));
        let telemetry_path = result.telemetry_path.expect("telemetry path");
        let telemetry = fs::read_to_string(telemetry_path).expect("telemetry");
        assert!(
            telemetry.contains("\"commit_created\": true"),
            "{telemetry}"
        );
        assert!(telemetry.contains("\"confirmation\": true"), "{telemetry}");
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
        let gh = install_fake_gh(&root, true, "", "https://github.com/org/repo/pull/99");

        let result = with_fake_gh(&gh, || {
            restricted_push(None, &root, false).expect("restricted push")
        });
        assert!(!result.push_created);
        assert!(result.confirmation_required);
        assert!(!result.confirmation_granted);
        assert!(result.dry_run_ok);
        let telemetry =
            fs::read_to_string(result.telemetry_path.expect("telemetry path")).expect("telemetry");
        assert!(telemetry.contains("\"push_ok\": false"), "{telemetry}");
    }

    #[test]
    fn restricted_push_requires_gh_auth() {
        let root = temp_dir("restricted_push_auth");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/push-auth");
        let bare = std::env::temp_dir().join(format!(
            "design_cli_push_auth_remote_{}",
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
        let gh = install_fake_gh(&root, false, "", "https://github.com/org/repo/pull/99");

        let err = with_fake_gh(&gh, || {
            restricted_push(None, &root, true).expect_err("auth failure")
        });
        assert_eq!(err, "RemoteAuthFailed");
        let telemetry = fs::read_to_string(root.join(".dbm/telemetry/remote_integration.json"))
            .expect("telemetry");
        assert!(telemetry.contains("\"auth_failure\": true"), "{telemetry}");
    }

    #[test]
    fn restricted_pr_create_rejects_invalid_base() {
        let root = temp_dir("restricted_pr_invalid_base");
        write_rust_project(&root, "fn main() {}\n");
        init_git_repo_with_branch(&root, "dbm/pr-base");
        let err = restricted_pr_create("dbm/pr-base", "develop", &root, None, 1, true)
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
            r#"{"number":42,"url":"https://github.com/org/repo/pull/42"}"#,
            "https://github.com/org/repo/pull/99",
        );
        let result = with_fake_gh(&gh, || {
            restricted_pr_create("dbm/pr-duplicate", "main", &root, None, 1, true)
                .expect("duplicate result")
        });
        assert!(!result.pr_created);
        assert!(result.duplicate_detected);
        assert_eq!(result.pr_number, Some(42));
        let telemetry =
            fs::read_to_string(result.telemetry_path.expect("telemetry path")).expect("telemetry");
        assert!(telemetry.contains("\"pr_duplicate\": true"), "{telemetry}");
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
    fn crate_root_reexport_imports_prefer_root_symbol_path() {
        let root = temp_dir("crate_root_reexport_imports");
        fs::create_dir_all(root.join("src/controller")).expect("controller");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"memory_space\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod controller;\npub mod patterns;\npub use patterns::layer_sequence_from_state;\n",
        )
        .expect("lib");
        fs::write(root.join("src/controller/mod.rs"), "pub mod matcher;\n")
            .expect("controller mod");
        fs::write(
            root.join("src/controller/matcher.rs"),
            "use crate::patterns::layer_sequence_from_state;\npub fn run() {}\n",
        )
        .expect("matcher");
        fs::write(
            root.join("src/patterns.rs"),
            "pub fn layer_sequence_from_state() {}\n",
        )
        .expect("patterns");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let current_id = index
            .qualified_id_for_path(Path::new("src/controller/matcher.rs"))
            .expect("current id");
        let updated = rewrite_imports_in_content(
            &root,
            &index,
            &current_id,
            "use crate::patterns::layer_sequence_from_state;\npub fn run() {}\n",
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("rewrite");

        assert!(
            updated.contains("use crate::layer_sequence_from_state;"),
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

    #[test]
    fn semantic_recovery_local_trait_fallback_recovers_unresolved_crate_path() {
        let root = temp_dir("semantic_recovery_trait_fallback");
        fs::create_dir_all(root.join("src/engine")).expect("engine");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/main.rs"), "mod engine;\nfn main() {}\n").expect("main");
        fs::write(
            root.join("src/engine/mod.rs"),
            "use execution_core::dependency::dependency_engine_interface::DependencyEngineInterface;\n\npub struct Engine;\n",
        )
        .expect("engine mod");

        let fix = fix_build(&root).expect("fix build");
        assert!(fix.build_fixed);
        let engine = fs::read_to_string(root.join("src/engine/mod.rs")).expect("engine");
        assert!(
            engine.contains("pub trait DependencyEngineInterface"),
            "{engine}"
        );
        assert!(!engine.contains("use execution_core::"), "{engine}");
        run_build_validation(&root, &root).expect("cargo check after fallback");
    }

    #[test]
    fn semantic_recovery_parses_primary_error_after_warnings() {
        let message = "\
warning: unused import: `crate::controller::controller_determinism_interface::ControllerDeterminismInterface`\n\
 --> crates/execution_stability_core/src/determinism/mod.rs:1:5\n\
  |\n\
1 | use crate::controller::controller_determinism_interface::ControllerDeterminismInterface;\n\
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n\
\n\
error: expected identifier, found keyword `use`\n\
 --> apps/cli/src/app.rs:8:1\n\
  |\n\
8 | use runtime_vm::adapter_app_interface::AdapterAppInterface;\n\
  | ^^^ expected identifier, found keyword\n";
        assert_eq!(
            parse_primary_error_file(message),
            Some(PathBuf::from("apps/cli/src/app.rs"))
        );
        assert_eq!(
            extract_primary_use_statement(message),
            Some("use runtime_vm::adapter_app_interface::AdapterAppInterface;".to_string())
        );
    }

    #[test]
    fn service_patch_hunks_stay_bounded() {
        let original = (1..=220)
            .map(|idx| format!("line_{idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let updated = format!(
            "use crate::adapter_service_interface::AdapterServiceInterface;\n{original}\n#[path = \"service/dto.rs\"]\npub mod dto;\npub use reasoning::ServiceReasoning;\n"
        );
        let hunks = synthesize_change_hunks(
            "apps/cli/src/service.rs",
            &original,
            &updated,
            &ChangeType::ModifyFile,
        );
        assert!(!hunks.is_empty());
        assert!(hunks.iter().all(|hunk| {
            let touched = if hunk.start_line <= hunk.end_line {
                hunk.end_line.saturating_sub(hunk.start_line) + 1
            } else {
                hunk.replacement.lines().count()
            };
            touched <= 120
        }));
    }

    #[test]
    fn create_interface_uses_deterministic_placeholder_method() {
        let root = temp_dir("create_interface_placeholder");
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("main");

        let patch = CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("adapter-service".to_string(), "service".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "AdapterServiceInterface".to_string(),
                between: ("adapter-service".to_string(), "service".to_string()),
            }],
            description: "introduce interface".to_string(),
            target_file: PathBuf::new(),
        };
        let change_set = generate_code_change_set(&root, &[patch]).expect("change set");
        let replacement =
            render_change_replacement(&change_set.changes[0], "").expect("replacement");
        assert!(
            replacement.contains("fn execute_service(&self);"),
            "{replacement}"
        );
        assert!(!replacement.contains("TODO"), "{replacement}");
    }

    #[test]
    fn generated_change_set_has_canonical_target() {
        let root = temp_dir("canonical_target_relay");
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/service.rs"), "pub fn run() {}\n").expect("service");

        let patch = CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "service".to_string(),
                to: "adapter".to_string(),
                via: Some("adapter_service_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "service".to_string(),
                to: "adapter".to_string(),
                via: Some("adapter_service_interface".to_string()),
            }],
            description: "service import update".to_string(),
            target_file: PathBuf::from("src/service.rs"),
        };
        let change_set = generate_code_change_set(&root, &[patch]).expect("change set");
        assert!(change_set.canonical_target.is_some());
    }

    #[test]
    fn coding_check_json_production_path_snapshot() {
        let root = temp_dir("coding_check_json_production_path_snapshot");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("package cargo");
        fs::write(
            root.join("apps/cli/src/lib.rs"),
            "pub mod service;\npub mod adapter_service_interface;\n",
        )
        .expect("lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use std::collections::BTreeMap;\nuse std::fs;\n\n#[path = \"service/dto.rs\"]\npub mod dto;\n#[path = \"service/reasoning.rs\"]\npub mod reasoning;\n\npub use dto::*;\npub use reasoning::{IssueAggregator, generate_plan};\n\npub fn analyze_path() {}\n",
        )
        .expect("service");

        let patches = vec![
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
            CodePatch {
                patch_id: "service-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "introduce adapter service interface".to_string(),
                target_file: PathBuf::new(),
            },
        ];

        let changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: true,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
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
        .expect("execution");
        let report = CodingReport {
            root: root.display().to_string(),
            dry_run: true,
            execution: execution.clone(),
            patches: changes.patches.clone(),
            changes: changes.clone(),
            apply_resolutions: build_apply_resolutions(
                &root,
                &changes,
                Some(Path::new("apps/cli/src/service.rs")),
                &BTreeMap::new(),
            )
            .expect("resolutions"),
            telemetry: build_canonicalization_telemetry(
                &changes,
                &build_apply_resolutions(
                    &root,
                    &changes,
                    Some(Path::new("apps/cli/src/service.rs")),
                    &BTreeMap::new(),
                )
                .expect("telemetry resolutions"),
                &execution,
            ),
        };
        let json = serde_json::to_string_pretty(&report).expect("json");

        let service_change = changes
            .changes
            .iter()
            .find(|change| change.file_path == "apps/cli/src/service.rs")
            .expect("service change");
        assert!(changes.summary.create_files > 0);
        assert!(service_change.hunks.iter().all(|hunk| {
            let touched = if hunk.start_line <= hunk.end_line {
                hunk.end_line.saturating_sub(hunk.start_line) + 1
            } else {
                hunk.replacement.lines().count()
            };
            touched <= 120
        }));
        assert!(!json.contains("// TODO: define required methods"), "{json}");
        assert!(!json.contains("\"canonical_target\": null"), "{json}");
        assert!(changes.canonical_target.is_some());
    }

    #[test]
    fn coding_check_json_create_interface_materialization_e2e() {
        let root = temp_dir("coding_check_json_create_interface_materialization_e2e");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("package cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use std::collections::BTreeMap;\n\npub fn analyze_path() -> BTreeMap<String, String> { BTreeMap::new() }\n",
        )
        .expect("service");

        let patches = vec![
            CodePatch {
                patch_id: "service-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "introduce adapter service interface".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
        ];

        let changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        assert!(changes.summary.create_files > 0);
        assert!(
            changes
                .changes
                .iter()
                .any(|change| change.file_path.ends_with("adapter_service_interface.rs"))
        );
        let lib_change = changes
            .changes
            .iter()
            .find(|change| change.file_path == "apps/cli/src/lib.rs")
            .expect("crate root registration");
        let lib_after =
            render_change_replacement(lib_change, "pub mod service;\n").expect("lib after");
        assert!(
            lib_after.contains("pub mod adapter_service_interface;"),
            "{lib_after}"
        );

        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::ExplicitTargetOnly,
                explicit_target: Some(PathBuf::from("apps/cli/src/service.rs")),
            },
            None,
        )
        .expect("execution");
        assert!(execution.build_ok, "{:?}", execution.reason);
        assert!(
            execution
                .reason
                .as_deref()
                .map(|reason| !reason.contains("E0432"))
                .unwrap_or(true)
        );
    }

    #[test]
    fn coding_check_json_explicit_target_companion_override_e2e() {
        let root = temp_dir("coding_check_json_explicit_target_companion_override_e2e");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("package cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use std::collections::BTreeMap;\n\npub fn analyze_path() -> BTreeMap<String, String> { BTreeMap::new() }\n",
        )
        .expect("service");

        let patches = vec![
            CodePatch {
                patch_id: "service-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "introduce adapter service interface".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
        ];

        let changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::ExplicitTargetOnly,
                explicit_target: Some(PathBuf::from("apps/cli/src/service.rs")),
            },
            None,
        )
        .expect("execution");
        let report = CodingReport {
            root: root.display().to_string(),
            dry_run: true,
            execution: execution.clone(),
            patches: changes.patches.clone(),
            changes: changes.clone(),
            apply_resolutions: build_apply_resolutions(
                &root,
                &changes,
                Some(Path::new("apps/cli/src/service.rs")),
                &BTreeMap::new(),
            )
            .expect("resolutions"),
            telemetry: build_canonicalization_telemetry(
                &changes,
                &build_apply_resolutions(
                    &root,
                    &changes,
                    Some(Path::new("apps/cli/src/service.rs")),
                    &BTreeMap::new(),
                )
                .expect("telemetry resolutions"),
                &execution,
            ),
        };
        let json = serde_json::to_string_pretty(&report).expect("json");
        let interface_resolution = report
            .apply_resolutions
            .iter()
            .find(|resolution| resolution.module == "adapter_service_interface")
            .expect("interface resolution");
        let service_resolution = report
            .apply_resolutions
            .iter()
            .find(|resolution| resolution.module == "service")
            .expect("service resolution");

        assert!(changes.summary.create_files > 0);
        assert!(
            changes
                .changes
                .iter()
                .any(|change| change.file_path.ends_with("adapter_service_interface.rs"))
        );
        assert_eq!(
            interface_resolution.resolved_relative_path,
            PathBuf::from("apps/cli/src/adapter_service_interface.rs")
        );
        assert_eq!(
            interface_resolution.resolution_strategy,
            "companion_create_file"
        );
        assert_eq!(
            service_resolution.resolved_relative_path,
            PathBuf::from("apps/cli/src/service.rs")
        );
        assert_eq!(service_resolution.resolution_strategy, "target_override");
        assert!(!json.contains("\"module\": \"*\""), "{json}");
        assert!(!json.contains("// TODO: define required methods"), "{json}");
        assert!(execution.build_ok, "{:?}", execution.reason);
        assert!(
            execution
                .reason
                .as_deref()
                .map(|reason| !reason.contains("E0432"))
                .unwrap_or(true)
        );
    }

    #[test]
    fn canonicalization_issue_count_must_be_zero() {
        let root = temp_dir("canonicalization_issue_count_must_be_zero");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("package cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use std::collections::BTreeMap;\n\npub fn analyze_path() -> BTreeMap<String, String> { BTreeMap::new() }\n",
        )
        .expect("service");

        let patches = vec![
            CodePatch {
                patch_id: "service-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "introduce adapter service interface".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
        ];

        let changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::ExplicitTargetOnly,
                explicit_target: Some(PathBuf::from("apps/cli/src/service.rs")),
            },
            None,
        )
        .expect("execution");
        let apply_resolutions = build_apply_resolutions(
            &root,
            &changes,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
        )
        .expect("apply resolutions");
        let telemetry = build_canonicalization_telemetry(&changes, &apply_resolutions, &execution);

        assert_eq!(normalization_issue_count(&telemetry), 0, "{telemetry:?}");
        assert!(!telemetry.normalization_path_used, "{telemetry:?}");
    }

    #[test]
    fn coding_check_json_explicit_target_cross_crate_import_coherence() {
        let root = temp_dir("coding_check_json_explicit_target_cross_crate_import_coherence");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::create_dir_all(root.join("crates/architecture_search/src")).expect("arch src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\", \"crates/architecture_search\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\narchitecture_search = { path = \"../../crates/architecture_search\" }\n",
        )
        .expect("cli cargo");
        fs::write(
            root.join("crates/architecture_search/Cargo.toml"),
            "[package]\nname = \"architecture_search\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("arch cargo");
        fs::write(
            root.join("apps/cli/src/lib.rs"),
            "pub mod service;\npub use architecture_search::grammar;\n",
        )
        .expect("cli lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use crate::grammar;\n\npub fn bridge(_bridge: &dyn GrammarGrammarEngineInterface) {}\n",
        )
        .expect("service");
        fs::write(
            root.join("crates/architecture_search/src/lib.rs"),
            "pub mod grammar;\npub mod grammar_engine;\n",
        )
        .expect("arch lib");
        fs::write(
            root.join("crates/architecture_search/src/grammar.rs"),
            "pub fn parse() -> &'static str { \"ok\" }\n",
        )
        .expect("grammar");
        fs::write(
            root.join("crates/architecture_search/src/grammar_engine.rs"),
            "pub fn evaluate() -> &'static str { \"ok\" }\n",
        )
        .expect("grammar engine");

        let patches = vec![
            CodePatch {
                patch_id: "grammar-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("grammar".to_string(), "grammar_engine".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "GrammarGrammarEngineInterface".to_string(),
                    between: ("grammar".to_string(), "grammar_engine".to_string()),
                }],
                description: "introduce grammar/grammar_engine interface".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "grammar".to_string(),
                    via: Some("GrammarGrammarEngineInterface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "grammar".to_string(),
                    via: Some("GrammarGrammarEngineInterface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
        ];

        let changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::ExplicitTargetOnly,
                explicit_target: Some(PathBuf::from("apps/cli/src/service.rs")),
            },
            None,
        )
        .expect("execution");
        let apply_resolutions = build_apply_resolutions(
            &root,
            &changes,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
        )
        .expect("apply resolutions");
        let report = CodingReport {
            root: root.display().to_string(),
            dry_run: true,
            execution: execution.clone(),
            patches: changes.patches.clone(),
            changes: changes.clone(),
            apply_resolutions: apply_resolutions.clone(),
            telemetry: build_canonicalization_telemetry(&changes, &apply_resolutions, &execution),
        };

        let service_change = report
            .changes
            .changes
            .iter()
            .find(|change| change.file_path == "apps/cli/src/service.rs")
            .expect("service change");
        let replacement = service_change
            .hunks
            .iter()
            .map(|hunk| hunk.replacement.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(report.execution.build_ok, "{:?}", report.execution.reason);
        assert_eq!(report.telemetry.normalization_issue_count, 0);
        assert!(
            report
                .changes
                .changes
                .iter()
                .any(|change| change.file_path == "apps/cli/src/service.rs")
        );
        assert!(
            !replacement.contains("use crate::grammar_grammar_engine_interface"),
            "{replacement}"
        );
        assert!(
            replacement.contains(
                "use architecture_search::grammar_grammar_engine_interface::GrammarGrammarEngineInterface;"
            ),
            "{replacement}"
        );
        assert!(report.changes.summary.create_files > 0);
        assert_eq!(
            report.changes.canonical_target.as_deref(),
            Some(Path::new("apps/cli/src/service.rs"))
        );
        assert!(report.changes.changes.iter().any(|change| {
            change.file_path == "crates/architecture_search/src/grammar_grammar_engine_interface.rs"
        }));
    }

    #[test]
    fn coding_check_json_canonical_target_dto_continuity_e2e() {
        let root = temp_dir("coding_check_json_canonical_target_dto_continuity_e2e");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("package cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "use std::collections::BTreeMap;\n\npub fn analyze_path() -> BTreeMap<String, String> { BTreeMap::new() }\n",
        )
        .expect("service");

        let patches = vec![
            CodePatch {
                patch_id: "service-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "introduce adapter service interface".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "service-import".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "service".to_string(),
                    to: "adapter".to_string(),
                    via: Some("adapter_service_interface".to_string()),
                }],
                description: "service import update".to_string(),
                target_file: PathBuf::from("apps/cli/src/service.rs"),
            },
        ];

        let mut changes = patches_to_change_set(
            &root,
            &patches,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
            None,
        )
        .expect("change set");
        changes.canonical_target = None;
        let execution = execute_code_change_set(
            &root,
            &changes,
            &CodingOptions {
                apply: false,
                check: true,
                no_build: false,
                backup: false,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: PatchScope::ExplicitTargetOnly,
                explicit_target: Some(PathBuf::from("apps/cli/src/service.rs")),
            },
            None,
        )
        .expect("execution");
        ensure_canonical_target_dto_continuity(
            &root,
            &mut changes,
            &execution,
            Some(Path::new("apps/cli/src/service.rs")),
        )
        .expect("continuity");
        let apply_resolutions = build_apply_resolutions(
            &root,
            &changes,
            Some(Path::new("apps/cli/src/service.rs")),
            &BTreeMap::new(),
        )
        .expect("apply resolutions");
        let report = CodingReport {
            root: root.display().to_string(),
            dry_run: true,
            execution: execution.clone(),
            patches: changes.patches.clone(),
            changes: changes.clone(),
            apply_resolutions: apply_resolutions.clone(),
            telemetry: build_canonicalization_telemetry(&changes, &apply_resolutions, &execution),
        };

        assert_eq!(
            report.execution.canonical_target_path.as_deref(),
            Some("apps/cli/src/service.rs")
        );
        assert_eq!(
            report.changes.canonical_target.as_deref(),
            Some(Path::new("apps/cli/src/service.rs"))
        );
        assert_eq!(
            report.execution.canonical_target_path.as_deref(),
            report
                .changes
                .canonical_target
                .as_deref()
                .and_then(|path| path.to_str())
        );
    }

    #[test]
    fn coding_check_json_no_todo_trait_stub_production_e2e() {
        let root = temp_dir("coding_check_json_no_todo_trait_stub_production_e2e");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::create_dir_all(root.join("crates/recomposer/src")).expect("recomposer src");
        fs::create_dir_all(root.join("crates/runtime/runtime_core/src/intent_refiner"))
            .expect("runtime core src");
        fs::create_dir_all(root.join("crates/memory_space/src")).expect("memory space src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\", \"crates/recomposer\", \"crates/runtime/runtime_core\", \"crates/memory_space\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("cli cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("cli lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "pub fn analyze_path() {}\n",
        )
        .expect("service");
        fs::write(
            root.join("crates/recomposer/Cargo.toml"),
            "[package]\nname = \"recomposer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("recomposer cargo");
        fs::write(
            root.join("crates/recomposer/src/lib.rs"),
            "pub mod recommend;\n",
        )
        .expect("recomposer lib");
        fs::write(
            root.join("crates/recomposer/src/recommend.rs"),
            "pub fn recommend() {}\n",
        )
        .expect("recommend");
        fs::write(
            root.join("crates/runtime/runtime_core/Cargo.toml"),
            "[package]\nname = \"runtime_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("runtime cargo");
        fs::write(
            root.join("crates/runtime/runtime_core/src/lib.rs"),
            "pub mod intent_refiner;\n",
        )
        .expect("runtime lib");
        fs::write(
            root.join("crates/runtime/runtime_core/src/intent_refiner/mod.rs"),
            "pub fn refine() {}\n",
        )
        .expect("intent refiner");
        fs::write(
            root.join("crates/memory_space/Cargo.toml"),
            "[package]\nname = \"memory_space\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("memory cargo");
        fs::write(
            root.join("crates/memory_space/src/lib.rs"),
            "pub mod pattern_matcher;\n",
        )
        .expect("memory lib");
        fs::write(
            root.join("crates/memory_space/src/pattern_matcher.rs"),
            "pub fn match_patterns() {}\n",
        )
        .expect("pattern matcher");

        let patches = vec![
            CodePatch {
                patch_id: "adapter-service".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "adapter service".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "consistency-recommend".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("consistency".to_string(), "recommend".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "ConsistencyRecommendInterface".to_string(),
                    between: ("consistency".to_string(), "recommend".to_string()),
                }],
                description: "consistency recommend".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "explanation-intent".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("explanation".to_string(), "intent_refiner".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "ExplanationIntentRefinerInterface".to_string(),
                    between: ("explanation".to_string(), "intent_refiner".to_string()),
                }],
                description: "explanation intent".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "pattern-extractor-matcher".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: (
                        "pattern_extractor".to_string(),
                        "pattern_matcher".to_string(),
                    ),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "PatternExtractorPatternMatcherInterface".to_string(),
                    between: (
                        "pattern_extractor".to_string(),
                        "pattern_matcher".to_string(),
                    ),
                }],
                description: "pattern extractor matcher".to_string(),
                target_file: PathBuf::new(),
            },
        ];

        let changes = generate_code_change_set(&root, &patches).expect("change set");
        let json = serde_json::to_string_pretty(&changes).expect("json");

        assert!(!json.contains("TODO: define required methods"), "{json}");
    }

    #[test]
    fn coding_check_json_trait_placeholder_method_determinism_e2e() {
        let root = temp_dir("coding_check_json_trait_placeholder_method_determinism_e2e");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::create_dir_all(root.join("crates/recomposer/src")).expect("recomposer src");
        fs::create_dir_all(root.join("crates/runtime/runtime_core/src/intent_refiner"))
            .expect("runtime core src");
        fs::create_dir_all(root.join("crates/memory_space/src")).expect("memory space src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\", \"crates/recomposer\", \"crates/runtime/runtime_core\", \"crates/memory_space\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("apps/cli/Cargo.toml"),
            "[package]\nname = \"design_cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("cli cargo");
        fs::write(root.join("apps/cli/src/lib.rs"), "pub mod service;\n").expect("cli lib");
        fs::write(
            root.join("apps/cli/src/service.rs"),
            "pub fn analyze_path() {}\n",
        )
        .expect("service");
        fs::write(
            root.join("crates/recomposer/Cargo.toml"),
            "[package]\nname = \"recomposer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("recomposer cargo");
        fs::write(
            root.join("crates/recomposer/src/lib.rs"),
            "pub mod recommend;\n",
        )
        .expect("recomposer lib");
        fs::write(
            root.join("crates/recomposer/src/recommend.rs"),
            "pub fn recommend() {}\n",
        )
        .expect("recommend");
        fs::write(
            root.join("crates/runtime/runtime_core/Cargo.toml"),
            "[package]\nname = \"runtime_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("runtime cargo");
        fs::write(
            root.join("crates/runtime/runtime_core/src/lib.rs"),
            "pub mod intent_refiner;\n",
        )
        .expect("runtime lib");
        fs::write(
            root.join("crates/runtime/runtime_core/src/intent_refiner/mod.rs"),
            "pub fn refine() {}\n",
        )
        .expect("intent refiner");
        fs::write(
            root.join("crates/memory_space/Cargo.toml"),
            "[package]\nname = \"memory_space\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("memory cargo");
        fs::write(
            root.join("crates/memory_space/src/lib.rs"),
            "pub mod pattern_matcher;\n",
        )
        .expect("memory lib");
        fs::write(
            root.join("crates/memory_space/src/pattern_matcher.rs"),
            "pub fn match_patterns() {}\n",
        )
        .expect("pattern matcher");

        let patches = vec![
            CodePatch {
                patch_id: "adapter-service".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("adapter-service".to_string(), "service".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "AdapterServiceInterface".to_string(),
                    between: ("adapter-service".to_string(), "service".to_string()),
                }],
                description: "adapter service".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "consistency-recommend".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("consistency".to_string(), "recommend".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "ConsistencyRecommendInterface".to_string(),
                    between: ("consistency".to_string(), "recommend".to_string()),
                }],
                description: "consistency recommend".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "explanation-intent".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("explanation".to_string(), "intent_refiner".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "ExplanationIntentRefinerInterface".to_string(),
                    between: ("explanation".to_string(), "intent_refiner".to_string()),
                }],
                description: "explanation intent".to_string(),
                target_file: PathBuf::new(),
            },
            CodePatch {
                patch_id: "pattern-extractor-matcher".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: (
                        "pattern_extractor".to_string(),
                        "pattern_matcher".to_string(),
                    ),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "PatternExtractorPatternMatcherInterface".to_string(),
                    between: (
                        "pattern_extractor".to_string(),
                        "pattern_matcher".to_string(),
                    ),
                }],
                description: "pattern extractor matcher".to_string(),
                target_file: PathBuf::new(),
            },
        ];

        let changes = generate_code_change_set(&root, &patches).expect("change set");
        let json = serde_json::to_string_pretty(&changes).expect("json");

        assert!(json.contains("fn execute_service(&self);"), "{json}");
        assert!(json.contains("fn evaluate_consistency(&self);"), "{json}");
        assert!(
            json.contains("fn refine_explanation_intent(&self);"),
            "{json}"
        );
        assert!(
            json.contains("fn extract_pattern_signature(&self);"),
            "{json}"
        );
    }

    #[test]
    fn coding_check_json_trait_fallback_pair_method_name_e2e() {
        let root = temp_dir("coding_check_json_trait_fallback_pair_method_name_e2e");
        fs::create_dir_all(root.join("crates/architecture_search/src")).expect("arch src");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/architecture_search\"]\nresolver = \"2\"\n",
        )
        .expect("workspace cargo");
        fs::write(
            root.join("crates/architecture_search/Cargo.toml"),
            "[package]\nname = \"architecture_search\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .expect("arch cargo");
        fs::write(
            root.join("crates/architecture_search/src/lib.rs"),
            "pub mod grammar;\npub mod grammar_engine;\n",
        )
        .expect("arch lib");
        fs::write(
            root.join("crates/architecture_search/src/grammar.rs"),
            "pub fn parse() {}\n",
        )
        .expect("grammar");
        fs::write(
            root.join("crates/architecture_search/src/grammar_engine.rs"),
            "pub fn eval() {}\n",
        )
        .expect("grammar engine");

        let changes = generate_code_change_set(
            &root,
            &[CodePatch {
                patch_id: "grammar-interface".to_string(),
                action: RefactorPlanAction::IntroduceInterface {
                    between: ("grammar".to_string(), "grammar_engine".to_string()),
                },
                operations: vec![PatchOperation::CreateInterface {
                    name: "GrammarGrammarEngineInterface".to_string(),
                    between: ("grammar".to_string(), "grammar_engine".to_string()),
                }],
                description: "grammar interface".to_string(),
                target_file: PathBuf::new(),
            }],
        )
        .expect("change set");
        let json = serde_json::to_string_pretty(&changes).expect("json");

        assert!(json.contains("fn handle_grammar_engine(&self);"), "{json}");
    }
}
