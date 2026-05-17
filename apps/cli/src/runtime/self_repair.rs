use std::fs;
use std::path::PathBuf;

use super::workspace_awareness::{
    MutationRiskLevel, WorkspaceModule, WorkspaceTopologySnapshot, validate_workspace_topology,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SelfMutationTransaction {
    pub mutation_id: u64,
    pub workspace_snapshot: WorkspaceTopologySnapshot,
    pub mutation_targets: Vec<WorkspaceModule>,
    pub mutation_plan: SelfMutationPlan,
    pub validation: SelfMutationValidation,
    pub rollback_snapshot: FilesystemRollbackSnapshot,
    pub deterministic_checksum: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelfMutationPlan {
    pub mutation_operations: Vec<SelfMutationOperation>,
    pub affected_modules: Vec<String>,
    pub expected_runtime_effects: Vec<RuntimeEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfMutationOperation {
    pub operation_id: u64,
    pub target_module: String,
    pub operation_type: SelfMutationOperationType,
    pub deterministic_order: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelfMutationOperationType {
    PreviewOnlyNoop,
    DeterministicCleanup,
    LocalRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeEffect {
    WorkspaceTopologyValidated,
    RollbackSnapshotCreated,
    ProtectedBoundaryChecked,
    SandboxVerified,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelfMutationValidation {
    pub compile_safe: bool,
    pub topology_safe: bool,
    pub rollback_safe: bool,
    pub protected_boundary_safe: bool,
    pub replay_invariant: bool,
    pub validation_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemRollbackSnapshot {
    pub snapshot_id: u64,
    pub file_hashes: Vec<FileHash>,
    pub workspace_revision: u64,
    pub deterministic_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHash {
    pub path: PathBuf,
    pub hash: u64,
    pub existed: bool,
    pub contents: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxMutationResult {
    pub sandbox_safe: bool,
    pub compile_success: bool,
    pub runtime_validation_success: bool,
    pub replay_validation_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfMutationResult {
    pub applied: bool,
    pub rolled_back: bool,
    pub sandbox: SandboxMutationResult,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackResult {
    pub rolled_back: bool,
    pub restored_files: usize,
    pub replay_invariant: bool,
    pub errors: Vec<String>,
}

pub fn self_mutation_transaction(workspace: &WorkspaceTopologySnapshot) -> SelfMutationTransaction {
    let mutation_targets = deterministic_mutation_targets(workspace);
    let mutation_plan = self_mutation_plan(&mutation_targets);
    let rollback_snapshot =
        filesystem_rollback_snapshot(workspace.deterministic_checksum, &mutation_targets);
    let mut transaction = SelfMutationTransaction {
        mutation_id: stable_hash_u64s([
            workspace.workspace_id,
            workspace.deterministic_checksum,
            mutation_targets.len() as u64,
        ]),
        workspace_snapshot: workspace.clone(),
        mutation_targets,
        mutation_plan,
        validation: SelfMutationValidation {
            compile_safe: false,
            topology_safe: false,
            rollback_safe: false,
            protected_boundary_safe: false,
            replay_invariant: false,
            validation_errors: Vec::new(),
        },
        rollback_snapshot,
        deterministic_checksum: 0,
    };
    transaction.validation = validate_self_mutation(&transaction);
    transaction.deterministic_checksum = deterministic_self_mutation_checksum(&transaction);
    transaction
}

pub fn validate_self_mutation(transaction: &SelfMutationTransaction) -> SelfMutationValidation {
    let mut validation_errors = Vec::new();
    let topology_safe = validate_workspace_topology(&transaction.workspace_snapshot);
    if !topology_safe {
        validation_errors.push("workspace topology checksum or ordering is invalid".to_string());
    }

    if transaction.mutation_targets.is_empty() {
        validation_errors.push("no safe self-mutation targets are available".to_string());
    }

    let protected_boundary_safe = transaction.mutation_targets.iter().all(|target| {
        !matches!(
            target.mutation_risk,
            MutationRiskLevel::Protected | MutationRiskLevel::Critical
        )
    });
    if !protected_boundary_safe {
        validation_errors
            .push("protected or critical module selected for self-mutation".to_string());
    }

    let operations_ordered = transaction
        .mutation_plan
        .mutation_operations
        .windows(2)
        .all(|pair| pair[0].deterministic_order <= pair[1].deterministic_order);
    if !operations_ordered {
        validation_errors
            .push("self-mutation operations are not deterministically ordered".to_string());
    }

    let rollback_safe = validate_filesystem_rollback_snapshot(&transaction.rollback_snapshot);
    if !rollback_safe {
        validation_errors.push("filesystem rollback snapshot checksum is invalid".to_string());
    }

    let replay_invariant =
        topology_safe && rollback_safe && protected_boundary_safe && operations_ordered;
    let compile_safe = replay_invariant && !transaction.mutation_targets.is_empty();

    SelfMutationValidation {
        compile_safe,
        topology_safe,
        rollback_safe,
        protected_boundary_safe,
        replay_invariant,
        validation_errors,
    }
}

pub fn sandbox_self_mutation(transaction: &SelfMutationTransaction) -> SandboxMutationResult {
    let validation = validate_self_mutation(transaction);
    let checksum_matches =
        transaction.deterministic_checksum == deterministic_self_mutation_checksum(transaction);
    let replay_validation_success = validation.replay_invariant && checksum_matches;
    let runtime_validation_success =
        validation.topology_safe && validation.rollback_safe && validation.protected_boundary_safe;

    SandboxMutationResult {
        sandbox_safe: validation.validation_errors.is_empty()
            && replay_validation_success
            && runtime_validation_success,
        compile_success: validation.compile_safe,
        runtime_validation_success,
        replay_validation_success,
    }
}

pub fn apply_self_mutation(transaction: SelfMutationTransaction) -> SelfMutationResult {
    let validation = validate_self_mutation(&transaction);
    let sandbox = sandbox_self_mutation(&transaction);
    if !validation.validation_errors.is_empty() || !sandbox.sandbox_safe {
        let rollback = rollback_self_mutation(transaction.rollback_snapshot);
        let mut errors = validation.validation_errors;
        errors.extend(rollback.errors);
        return SelfMutationResult {
            applied: false,
            rolled_back: rollback.rolled_back,
            sandbox,
            errors,
        };
    }

    SelfMutationResult {
        applied: true,
        rolled_back: false,
        sandbox,
        errors: Vec::new(),
    }
}

pub fn rollback_self_mutation(snapshot: FilesystemRollbackSnapshot) -> RollbackResult {
    if !validate_filesystem_rollback_snapshot(&snapshot) {
        return RollbackResult {
            rolled_back: false,
            restored_files: 0,
            replay_invariant: false,
            errors: vec!["rollback snapshot checksum is invalid".to_string()],
        };
    }

    let mut errors = Vec::new();
    let mut restored_files = 0usize;
    for entry in &snapshot.file_hashes {
        if entry.existed {
            if let Some(parent) = entry.path.parent()
                && let Err(err) = fs::create_dir_all(parent)
            {
                errors.push(format!(
                    "failed to create parent for {}: {err}",
                    entry.path.display()
                ));
                continue;
            }
            let Some(contents) = &entry.contents else {
                errors.push(format!(
                    "missing rollback contents for {}",
                    entry.path.display()
                ));
                continue;
            };
            if let Err(err) = fs::write(&entry.path, contents) {
                errors.push(format!("failed to restore {}: {err}", entry.path.display()));
                continue;
            }
            restored_files += 1;
        } else if entry.path.exists() {
            if let Err(err) = fs::remove_file(&entry.path) {
                errors.push(format!("failed to remove {}: {err}", entry.path.display()));
                continue;
            }
            restored_files += 1;
        }
    }

    let replay_invariant = errors.is_empty() && rollback_hashes_match(&snapshot);
    RollbackResult {
        rolled_back: errors.is_empty(),
        restored_files,
        replay_invariant,
        errors,
    }
}

pub fn render_self_mutation_preview(transaction: &SelfMutationTransaction) -> String {
    let targets = transaction
        .mutation_targets
        .iter()
        .map(|target| format!("{}:{:?}", target.module_name, target.mutation_risk))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "self repair preview: targets={} operations={} rollback_files={} plan_checksum={}",
        if targets.is_empty() {
            "none".to_string()
        } else {
            targets
        },
        transaction.mutation_plan.mutation_operations.len(),
        transaction.rollback_snapshot.file_hashes.len(),
        transaction.deterministic_checksum,
    )
}

pub fn render_self_mutation_validation(validation: &SelfMutationValidation) -> String {
    format!(
        "self repair validation: compile_safe={} topology_safe={} rollback_safe={} protected_boundary_safe={} replay_invariant={} errors={}",
        validation.compile_safe,
        validation.topology_safe,
        validation.rollback_safe,
        validation.protected_boundary_safe,
        validation.replay_invariant,
        validation.validation_errors.join(", ")
    )
}

pub fn render_sandbox_mutation_result(result: &SandboxMutationResult) -> String {
    format!(
        "self repair sandbox: sandbox_safe={} compile_success={} runtime_validation_success={} replay_validation_success={}",
        result.sandbox_safe,
        result.compile_success,
        result.runtime_validation_success,
        result.replay_validation_success,
    )
}

pub fn render_self_mutation_result(result: &SelfMutationResult) -> String {
    format!(
        "self repair apply: applied={} rolled_back={} sandbox_safe={} replay_validation_success={} errors={}",
        result.applied,
        result.rolled_back,
        result.sandbox.sandbox_safe,
        result.sandbox.replay_validation_success,
        result.errors.join(", ")
    )
}

pub fn render_self_rollback_result(result: &RollbackResult) -> String {
    format!(
        "self rollback: rolled_back={} restored_files={} replay_invariant={} errors={}",
        result.rolled_back,
        result.restored_files,
        result.replay_invariant,
        result.errors.join(", ")
    )
}

fn deterministic_mutation_targets(workspace: &WorkspaceTopologySnapshot) -> Vec<WorkspaceModule> {
    workspace
        .modules
        .iter()
        .filter(|module| module.mutation_risk == MutationRiskLevel::Safe)
        .take(1)
        .cloned()
        .collect()
}

fn self_mutation_plan(targets: &[WorkspaceModule]) -> SelfMutationPlan {
    let mut mutation_operations = targets
        .iter()
        .enumerate()
        .map(|(index, target)| SelfMutationOperation {
            operation_id: stable_hash_strs([
                target.module_name.as_str(),
                target.module_path.to_string_lossy().as_ref(),
                "preview-only-noop",
            ]),
            target_module: target.module_name.clone(),
            operation_type: SelfMutationOperationType::PreviewOnlyNoop,
            deterministic_order: index as u64,
        })
        .collect::<Vec<_>>();
    mutation_operations.sort_by(|left, right| {
        left.deterministic_order
            .cmp(&right.deterministic_order)
            .then_with(|| left.target_module.cmp(&right.target_module))
    });

    let affected_modules = mutation_operations
        .iter()
        .map(|operation| operation.target_module.clone())
        .collect::<Vec<_>>();
    let expected_runtime_effects = if mutation_operations.is_empty() {
        Vec::new()
    } else {
        vec![
            RuntimeEffect::WorkspaceTopologyValidated,
            RuntimeEffect::RollbackSnapshotCreated,
            RuntimeEffect::ProtectedBoundaryChecked,
            RuntimeEffect::SandboxVerified,
        ]
    };

    SelfMutationPlan {
        mutation_operations,
        affected_modules,
        expected_runtime_effects,
    }
}

fn filesystem_rollback_snapshot(
    workspace_revision: u64,
    targets: &[WorkspaceModule],
) -> FilesystemRollbackSnapshot {
    let mut file_hashes = targets
        .iter()
        .map(|target| file_hash(target.module_path.clone()))
        .collect::<Vec<_>>();
    file_hashes.sort_by(|left, right| left.path.cmp(&right.path));
    let mut snapshot = FilesystemRollbackSnapshot {
        snapshot_id: stable_hash_u64s([
            workspace_revision,
            file_hashes.len() as u64,
            stable_hash_strs(file_hashes.iter().map(|entry| entry.path.to_string_lossy())),
        ]),
        file_hashes,
        workspace_revision,
        deterministic_checksum: 0,
    };
    snapshot.deterministic_checksum = deterministic_rollback_checksum(&snapshot);
    snapshot
}

fn file_hash(path: PathBuf) -> FileHash {
    match fs::read(&path) {
        Ok(contents) => FileHash {
            path,
            hash: stable_hash_bytes(&contents),
            existed: true,
            contents: Some(contents),
        },
        Err(_) => FileHash {
            path,
            hash: 0,
            existed: false,
            contents: None,
        },
    }
}

fn validate_filesystem_rollback_snapshot(snapshot: &FilesystemRollbackSnapshot) -> bool {
    deterministic_rollback_checksum(snapshot) == snapshot.deterministic_checksum
        && snapshot
            .file_hashes
            .windows(2)
            .all(|pair| pair[0].path <= pair[1].path)
}

fn rollback_hashes_match(snapshot: &FilesystemRollbackSnapshot) -> bool {
    snapshot.file_hashes.iter().all(|entry| {
        if entry.existed {
            fs::read(&entry.path)
                .map(|contents| stable_hash_bytes(&contents) == entry.hash)
                .unwrap_or(false)
        } else {
            !entry.path.exists()
        }
    })
}

fn deterministic_self_mutation_checksum(transaction: &SelfMutationTransaction) -> u64 {
    stable_hash_u64s([
        transaction.mutation_id,
        transaction.workspace_snapshot.deterministic_checksum,
        transaction.rollback_snapshot.deterministic_checksum,
        stable_hash_strs(
            transaction
                .mutation_plan
                .mutation_operations
                .iter()
                .map(|operation| operation.target_module.as_str()),
        ),
    ])
}

fn deterministic_rollback_checksum(snapshot: &FilesystemRollbackSnapshot) -> u64 {
    let mut values = vec![snapshot.snapshot_id, snapshot.workspace_revision];
    values.extend(snapshot.file_hashes.iter().flat_map(|entry| {
        [
            stable_hash_strs([entry.path.to_string_lossy().as_ref()]),
            entry.hash,
            u64::from(entry.existed),
        ]
    }));
    stable_hash_u64s(values)
}

fn stable_hash_strs<I, S>(values: I) -> u64
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.as_ref().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_hash_u64s<I>(values: I) -> u64
where
    I: IntoIterator<Item = u64>,
{
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::workspace_awareness::{
        ModuleOwnership, SemanticModuleRole, WorkspaceDependencyGraph,
    };

    #[test]
    fn self_mutation_transaction_is_deterministically_ordered() {
        let workspace = fixture_workspace(vec![
            fixture_module("z_mod", "z.rs", MutationRiskLevel::Safe),
            fixture_module("a_mod", "a.rs", MutationRiskLevel::Safe),
        ]);
        let transaction = self_mutation_transaction(&workspace);

        assert_eq!(transaction.mutation_targets.len(), 1);
        assert_eq!(
            transaction.mutation_targets[0].module_path,
            PathBuf::from("a.rs")
        );
        assert!(
            transaction.validation.replay_invariant,
            "{:?}",
            transaction.validation.validation_errors
        );
    }

    #[test]
    fn protected_self_mutation_is_rejected() {
        let workspace = fixture_workspace(vec![fixture_module(
            "runtime_core",
            "runtime.rs",
            MutationRiskLevel::Critical,
        )]);
        let mut transaction = self_mutation_transaction(&workspace);
        transaction.mutation_targets = workspace.modules.clone();
        transaction.mutation_plan = self_mutation_plan(&transaction.mutation_targets);
        transaction.rollback_snapshot = filesystem_rollback_snapshot(
            workspace.deterministic_checksum,
            &transaction.mutation_targets,
        );
        transaction.validation = validate_self_mutation(&transaction);

        assert!(!transaction.validation.protected_boundary_safe);
        assert!(
            transaction
                .validation
                .validation_errors
                .iter()
                .any(|error| error.contains("protected or critical"))
        );
    }

    #[test]
    fn rollback_restores_filesystem_contents() {
        let root = std::env::temp_dir().join(format!(
            "dbm_self_repair_{}",
            stable_hash_strs([std::thread::current().name().unwrap_or("test")])
        ));
        let path = root.join("module.rs");
        fs::create_dir_all(&root).expect("create temp root");
        fs::write(&path, b"before").expect("write before");
        let mut snapshot = FilesystemRollbackSnapshot {
            snapshot_id: 1,
            file_hashes: vec![file_hash(path.clone())],
            workspace_revision: 2,
            deterministic_checksum: 0,
        };
        snapshot.deterministic_checksum = deterministic_rollback_checksum(&snapshot);
        fs::write(&path, b"after").expect("write after");

        let result = rollback_self_mutation(snapshot);

        assert!(result.rolled_back);
        assert_eq!(fs::read(&path).expect("read restored"), b"before");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sandbox_does_not_mutate_live_files() {
        let root = std::env::temp_dir().join(format!(
            "dbm_self_sandbox_{}",
            stable_hash_strs(["sandbox"])
        ));
        let path = root.join("module.rs");
        fs::create_dir_all(&root).expect("create temp root");
        fs::write(&path, b"stable").expect("write stable");
        let workspace = fixture_workspace(vec![fixture_module(
            "module",
            path.to_string_lossy().as_ref(),
            MutationRiskLevel::Safe,
        )]);
        let transaction = self_mutation_transaction(&workspace);

        let result = sandbox_self_mutation(&transaction);

        assert!(
            result.sandbox_safe,
            "{:?}",
            transaction.validation.validation_errors
        );
        assert_eq!(fs::read(&path).expect("read stable"), b"stable");
        let _ = fs::remove_dir_all(root);
    }

    fn fixture_workspace(mut modules: Vec<WorkspaceModule>) -> WorkspaceTopologySnapshot {
        modules.sort_by(|left, right| left.module_path.cmp(&right.module_path));
        WorkspaceTopologySnapshot {
            workspace_id: 7,
            crates: Vec::new(),
            modules,
            dependency_graph: WorkspaceDependencyGraph::default(),
            deterministic_checksum: 0,
        }
        .with_checksum()
    }

    fn fixture_module(
        module_name: &str,
        module_path: &str,
        mutation_risk: MutationRiskLevel,
    ) -> WorkspaceModule {
        WorkspaceModule {
            module_name: module_name.to_string(),
            module_path: PathBuf::from(module_path),
            ownership: if mutation_risk == MutationRiskLevel::Critical {
                ModuleOwnership::RuntimeCore
            } else {
                ModuleOwnership::Execution
            },
            semantic_role: if mutation_risk == MutationRiskLevel::Critical {
                SemanticModuleRole::RuntimeAuthority
            } else {
                SemanticModuleRole::ExecutionProposal
            },
            mutation_risk,
        }
    }

    trait WorkspaceChecksum {
        fn with_checksum(self) -> WorkspaceTopologySnapshot;
    }

    impl WorkspaceChecksum for WorkspaceTopologySnapshot {
        fn with_checksum(mut self) -> WorkspaceTopologySnapshot {
            let mut values = vec![self.workspace_id];
            for module in &self.modules {
                values.extend([
                    stable_hash_strs([module.module_name.as_str()]),
                    stable_hash_strs([module.module_path.display().to_string().as_str()]),
                    module.ownership as u64,
                    module.semantic_role as u64,
                    module.mutation_risk as u64,
                ]);
            }
            self.deterministic_checksum = stable_hash_u64s(values);
            self
        }
    }
}
