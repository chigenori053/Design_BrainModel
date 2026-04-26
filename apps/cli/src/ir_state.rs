//! DBM IR State Layer  (DBM-IR-STATE-SPEC v1.0, Phase B-2)
//!
//! Manages IR as **persistent, versioned, validated state** – not as a
//! transient generation artefact.
//!
//! # State machine (per file)
//!
//! ```text
//!          ┌──────────────┐
//!          │   Invalid    │   (no entry in IrStateManager)
//!          └──────┬───────┘
//!                 │ build_ir / reload
//!                 ▼
//!          ┌──────────────┐
//!          │   Synced     │   dirty = false, hash matches FS
//!          └──────┬───────┘
//!                 │ file change / mark_dirty / check_drift detects change
//!                 ▼
//!          ┌──────────────┐
//!          │   Drifted    │   dirty = true
//!          └──────┬───────┘
//!                 │ reload
//!                 ▼
//!          ┌──────────────┐
//!          │   Synced     │
//!          └──────────────┘
//! ```
//!
//! # Core invariant
//! [`get_ir`] returns `Err` when `dirty == true` (Drifted state).
//!
//! # Single responsibility split (§ 2.2)
//! - **IR State Layer** (this module): holds snapshots, manages dirty flag.
//! - **Executor**: calls operations; never holds IR directly.
//! - **ir_sync**: low-level hashing primitives (FNV-1a) – reused here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

// ─── § 3 Core structures ──────────────────────────────────────────────────────

/// Lightweight deterministic representation of a parsed source file.
///
/// Derived purely from raw file content: same bytes → same [`CodeIr`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeIr {
    /// Raw source text – the basis for all refactor operations.
    pub source: String,
    /// File extension hint (e.g. `"rs"`, `"py"`).  Empty if unknown.
    pub lang: String,
}

impl CodeIr {
    fn from_source(source: &str, path: &Path) -> Self {
        let lang = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        Self {
            source: source.to_string(),
            lang,
        }
    }
}

/// § 3.1 – Immutable point-in-time snapshot of a file's IR.
///
/// `(file_content) → IrSnapshot` is a pure function (§ 2.3 Deterministic
/// Snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrSnapshot {
    pub file_path: PathBuf,
    /// FNV-1a hash of the file content at snapshot time.
    pub file_hash: u64,
    pub ir: CodeIr,
    /// Unix-seconds wall clock at snapshot creation (for telemetry only).
    pub generated_at: u64,
}

/// § 3.2 – Per-file IR state: snapshot + dirty flag.
///
/// | `dirty` | hash match | State   |
/// |---------|------------|---------|
/// | `false` | yes        | Synced  |
/// | `true`  | —          | Drifted |
/// | absent  | —          | Invalid |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrState {
    pub snapshot: IrSnapshot,
    /// `true` when the on-disk file content has diverged from `snapshot.file_hash`.
    pub dirty: bool,
    /// Files this file depends on in the project graph.
    pub dependencies: Vec<PathBuf>,
    /// Files that depend on this file in the project graph.
    pub dependents: Vec<PathBuf>,
}

/// Phase B-3 Project Graph IR: all tracked file states plus dependency edges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectIr {
    pub nodes: HashMap<PathBuf, IrState>,
    pub edges: Vec<DependencyEdge>,
}

/// Directed dependency edge: `from` imports/depends on `to`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyEdge {
    pub from: PathBuf,
    pub to: PathBuf,
    pub kind: DependencyKind,
}

/// Phase B-3 minimum dependency kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    Import,
}

/// § 3.3 – Manages per-file [`IrState`] records.
///
/// Single authority for all IR snapshots in a session.  Keyed by canonical
/// `PathBuf`; the session owns exactly one instance (held in
/// `ConversationState`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IrStateManager {
    states: HashMap<PathBuf, IrState>,
    edges: Vec<DependencyEdge>,
    /// Cumulative successful [`reload`] calls (used in telemetry, § 9).
    sync_count: u64,
}

// ─── Error constants ──────────────────────────────────────────────────────────

/// Canonical error returned by [`get_ir`] when the state is Drifted.
pub const IR_DRIFT_ERROR: &str = "ERROR: IR out of sync. Run 'reload' to refresh.";
pub const DEPENDENCY_DRIFT_ERROR: &str = "ERROR: dependency drift detected. Run 'reload'.";

// ─── § 5 Operations ───────────────────────────────────────────────────────────

/// § 5.1 – Build a fresh **Synced** [`IrState`] from a file path.
///
/// Reads the file, FNV-1a hashes the content, and constructs [`CodeIr`].
/// The returned state has `dirty = false`.
///
/// # Errors
/// Returns `Err` when the file cannot be read.
pub fn build_ir(path: &Path) -> Result<IrState, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("ir_state: cannot read {}: {e}", path.display()))?;
    let file_hash = crate::ir_sync::hash_content(&content);
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(IrState {
        snapshot: IrSnapshot {
            file_path: path.to_path_buf(),
            file_hash,
            ir: CodeIr::from_source(&content, path),
            generated_at,
        },
        dirty: false,
        dependencies: Vec::new(),
        dependents: Vec::new(),
    })
}

/// § 5.2 – Re-evaluate the dirty flag against the current on-disk content.
///
/// - Sets `state.dirty = false` when hash still matches.
/// - Sets `state.dirty = true`  when hash differs or file is unreadable.
pub fn check_drift(state: &mut IrState) {
    match crate::ir_sync::hash_file(&state.snapshot.file_path) {
        Ok(current) => {
            state.dirty = current != state.snapshot.file_hash;
        }
        Err(_) => {
            // File missing or unreadable – treat as drifted.
            state.dirty = true;
        }
    }
}

/// § 5.3 – Reload: rebuild [`IrState`] for `path` and insert into `manager`.
///
/// After a successful call `manager.is_synced(path) == true`.
///
/// # Errors
/// Propagates `build_ir` errors (file unreadable).
pub fn reload(path: &Path, manager: &mut IrStateManager) -> Result<(), String> {
    let new_state = build_ir(path)?;
    manager.states.insert(path.to_path_buf(), new_state);
    manager.sync_count += 1;
    manager.rebuild_graph_metadata();
    Ok(())
}

/// § 5.4 – Return the [`CodeIr`] for `path`, or `Err` if Drifted / Invalid.
///
/// Returns [`IR_DRIFT_ERROR`] when `state.dirty == true`.
/// Returns a path-not-tracked error when no snapshot exists.
pub fn get_ir<'a>(path: &Path, manager: &'a IrStateManager) -> Result<&'a CodeIr, String> {
    let tracked_path = manager
        .resolve_tracked_path(path)
        .unwrap_or_else(|| path.to_path_buf());
    let state = manager
        .states
        .get(&tracked_path)
        .ok_or_else(|| format!("ir_state: no snapshot for {}", path.display()))?;
    if state.dirty {
        return Err(IR_DRIFT_ERROR.to_string());
    }
    let current_hash = crate::ir_sync::hash_file(&state.snapshot.file_path)
        .map_err(|_| IR_DRIFT_ERROR.to_string())?;
    if current_hash != state.snapshot.file_hash {
        return Err(IR_DRIFT_ERROR.to_string());
    }
    if manager.has_dependency_drift(path) {
        return Err(DEPENDENCY_DRIFT_ERROR.to_string());
    }
    Ok(&state.snapshot.ir)
}

/// § 5.5 – Invalidate the diff reference on a transaction.
///
/// Called during reload so a stale diff cannot be applied after the file has
/// changed.
pub fn invalidate_diff(tx: &mut crate::service::dto::TransactionIR) {
    tx.latest_diff_ref = None;
}

// ─── IrStateManager methods ───────────────────────────────────────────────────

impl IrStateManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when `path` has a snapshot and `dirty == false` (**Synced**).
    pub fn is_synced(&self, path: &Path) -> bool {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| !s.dirty)
            .unwrap_or(false)
    }

    /// `true` when `path` has a snapshot and `dirty == true` (**Drifted**).
    pub fn is_drifted(&self, path: &Path) -> bool {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| s.dirty)
            .unwrap_or(false)
    }

    /// Phase B-3 drift definition: self dirty OR any dependency dirty.
    pub fn is_drifted_with_dependencies(&self, path: &Path) -> bool {
        self.is_drifted(path) || self.has_dependency_drift(path)
    }

    /// `true` when `path` has any managed state (Synced or Drifted).
    pub fn is_tracked(&self, path: &Path) -> bool {
        self.resolve_tracked_path(path).is_some()
    }

    /// Mark `path` as dirty without re-reading the file.
    ///
    /// Use this after writing to disk (e.g. after a successful `apply`) so
    /// that the next `refactor` call reloads a fresh snapshot.
    pub fn mark_dirty(&mut self, path: &Path) {
        let mut visited = HashSet::new();
        let target = self
            .resolve_tracked_path(path)
            .unwrap_or_else(|| path.to_path_buf());
        self.mark_dirty_recursive(&target, &mut visited);
    }

    /// Re-evaluate the dirty flag by hashing the current on-disk content.
    ///
    /// No-op if `path` is not yet tracked (Invalid state).
    pub fn check_and_update_drift(&mut self, path: &Path) {
        let target = self
            .resolve_tracked_path(path)
            .unwrap_or_else(|| path.to_path_buf());
        let became_dirty = if let Some(state) = self.states.get_mut(&target) {
            check_drift(state);
            state.dirty
        } else {
            false
        };
        if became_dirty {
            self.mark_dirty(&target);
        }
    }

    /// Check `path` and all known dependencies against the filesystem.
    pub fn check_and_update_drift_closure(&mut self, path: &Path) {
        let mut paths = vec![path.to_path_buf()];
        if let Some(state) = self.states.get(path) {
            paths.extend(state.dependencies.clone());
        }
        paths.sort();
        paths.dedup();
        for candidate in paths {
            self.check_and_update_drift(&candidate);
        }
    }

    /// Return the snapshot file hash for `path`, or `None` if not tracked.
    pub fn snapshot_hash(&self, path: &Path) -> Option<u64> {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| s.snapshot.file_hash)
    }

    /// Return telemetry for `path`, or `None` if `path` is not tracked.
    pub fn telemetry(&self, path: &Path) -> Option<IrStateTelemetry> {
        let state = self.states.get(path)?;
        Some(IrStateTelemetry {
            path: state.snapshot.file_path.display().to_string(),
            file_hash: format!("{:016x}", state.snapshot.file_hash),
            dirty: state.dirty,
            dependency_dirty: self.has_dependency_drift(path),
            generated_at: state.snapshot.generated_at,
            sync_count: self.sync_count,
        })
    }

    /// Number of files currently tracked (for diagnostics).
    pub fn tracked_count(&self) -> usize {
        self.states.len()
    }

    /// Build the Project Graph IR for all Rust source files under `root`.
    pub fn build_project(&mut self, root: &Path) -> Result<(), String> {
        let files = collect_rust_files(root)?;
        self.states.clear();
        for file in files {
            self.states.insert(file.clone(), build_ir(&file)?);
        }
        self.sync_count += 1;
        self.rebuild_graph_metadata();
        Ok(())
    }

    /// Reload one file plus every known dependent, with cycle protection.
    pub fn reload_recursive(&mut self, path: &Path) -> Result<(), String> {
        let mut visited = HashSet::new();
        self.reload_recursive_inner(path, &mut visited)
    }

    /// Reload every tracked file. If nothing is tracked yet, build a project from `root`.
    pub fn reload_all(&mut self, root: &Path) -> Result<(), String> {
        if self.states.is_empty() {
            return self.build_project(root);
        }
        let mut paths: Vec<PathBuf> = self.states.keys().cloned().collect();
        paths.sort();
        for path in paths {
            let state = build_ir(&path)?;
            self.states.insert(path, state);
        }
        self.sync_count += 1;
        self.rebuild_graph_metadata();
        Ok(())
    }

    pub fn project_ir(&self) -> ProjectIr {
        ProjectIr {
            nodes: self.states.clone(),
            edges: self.edges.clone(),
        }
    }

    pub fn dependencies(&self, path: &Path) -> Vec<PathBuf> {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| s.dependencies.clone())
            .unwrap_or_default()
    }

    pub fn dependents(&self, path: &Path) -> Vec<PathBuf> {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| s.dependents.clone())
            .unwrap_or_default()
    }

    pub fn render_dependencies(&self, path: &Path) -> String {
        let display_path = self
            .resolve_tracked_path(path)
            .unwrap_or_else(|| path.to_path_buf());
        let mut lines = vec![display_path.display().to_string()];
        let deps = self.dependencies(path);
        if deps.is_empty() {
            lines.push(" (no dependencies)".to_string());
        } else {
            for dep in deps {
                lines.push(format!(" ├── {}", dep.display()));
            }
        }
        lines.join("\n")
    }

    fn has_dependency_drift(&self, path: &Path) -> bool {
        self.resolve_tracked_path(path)
            .and_then(|p| self.states.get(&p))
            .map(|s| {
                s.dependencies
                    .iter()
                    .any(|dep| self.states.get(dep).map(|d| d.dirty).unwrap_or(false))
            })
            .unwrap_or(false)
    }

    fn resolve_tracked_path(&self, path: &Path) -> Option<PathBuf> {
        if self.states.contains_key(path) {
            return Some(path.to_path_buf());
        }
        self.states
            .keys()
            .filter(|candidate| candidate.ends_with(path))
            .min()
            .cloned()
    }

    fn mark_dirty_recursive(&mut self, path: &Path, visited: &mut HashSet<PathBuf>) {
        if !visited.insert(path.to_path_buf()) {
            return;
        }
        let dependents = if let Some(state) = self.states.get_mut(path) {
            state.dirty = true;
            state.dependents.clone()
        } else {
            Vec::new()
        };
        for dependent in dependents {
            self.mark_dirty_recursive(&dependent, visited);
        }
    }

    fn reload_recursive_inner(
        &mut self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<(), String> {
        if !visited.insert(path.to_path_buf()) {
            return Ok(());
        }
        let dependents = self.dependents(path);
        let state = build_ir(path)?;
        self.states.insert(path.to_path_buf(), state);
        self.sync_count += 1;
        self.rebuild_graph_metadata();
        for dependent in dependents {
            self.reload_recursive_inner(&dependent, visited)?;
        }
        Ok(())
    }

    fn rebuild_graph_metadata(&mut self) {
        let keys: HashSet<PathBuf> = self.states.keys().cloned().collect();
        let mut edges = Vec::new();
        for (path, state) in &self.states {
            for dep in extract_dependencies(path, &state.snapshot.ir.source, &keys) {
                edges.push(DependencyEdge {
                    from: path.clone(),
                    to: dep,
                    kind: DependencyKind::Import,
                });
            }
        }
        edges.sort_by(|a, b| {
            a.from
                .cmp(&b.from)
                .then_with(|| a.to.cmp(&b.to))
                .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
        });
        edges.dedup();

        for state in self.states.values_mut() {
            state.dependencies.clear();
            state.dependents.clear();
        }
        for edge in &edges {
            if let Some(from) = self.states.get_mut(&edge.from) {
                from.dependencies.push(edge.to.clone());
            }
            if let Some(to) = self.states.get_mut(&edge.to) {
                to.dependents.push(edge.from.clone());
            }
        }
        for state in self.states.values_mut() {
            state.dependencies.sort();
            state.dependencies.dedup();
            state.dependents.sort();
            state.dependents.dedup();
        }
        self.edges = edges;
    }
}

fn collect_rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rs") {
            files.push(root.to_path_buf());
        }
    } else {
        collect_rust_files_inner(root, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_rust_files_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("ir_state: cannot read dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("ir_state: cannot read dir entry: {e}"))?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_rust_files_inner(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_dependencies(path: &Path, source: &str, known: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut deps = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let candidates = if let Some(rest) = trimmed.strip_prefix("mod ") {
            vec![rest.trim_end_matches(';').trim().to_string()]
        } else if let Some(rest) = trimmed.strip_prefix("pub mod ") {
            vec![rest.trim_end_matches(';').trim().to_string()]
        } else if let Some(rest) = trimmed.strip_prefix("use ") {
            vec![rest.trim_end_matches(';').trim().to_string()]
        } else {
            Vec::new()
        };
        for candidate in candidates {
            if let Some(dep) = resolve_dependency(path, &candidate, known) {
                if dep != path {
                    deps.push(dep);
                }
            }
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn resolve_dependency(path: &Path, raw: &str, known: &HashSet<PathBuf>) -> Option<PathBuf> {
    let cleaned = raw
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .split("::")
        .next()?
        .trim_matches('{')
        .trim();
    if cleaned.is_empty() || cleaned == "*" {
        return None;
    }
    let parent = path.parent()?;
    let file = parent.join(format!("{cleaned}.rs"));
    if known.contains(&file) {
        return Some(file);
    }
    let module = parent.join(cleaned).join("mod.rs");
    if known.contains(&module) {
        return Some(module);
    }
    let src_parent = parent
        .parent()
        .filter(|_| parent.ends_with("src"))
        .unwrap_or(parent);
    let root_file = src_parent.join("src").join(format!("{cleaned}.rs"));
    if known.contains(&root_file) {
        return Some(root_file);
    }
    let root_module = src_parent.join("src").join(cleaned).join("mod.rs");
    if known.contains(&root_module) {
        return Some(root_module);
    }
    let file_name = format!("{cleaned}.rs");
    let mod_suffix = PathBuf::from(cleaned).join("mod.rs");
    known
        .iter()
        .filter(|candidate| {
            candidate.file_name().and_then(|n| n.to_str()) == Some(file_name.as_str())
                || candidate.ends_with(&mod_suffix)
        })
        .min()
        .cloned()
}

// ─── § 9 Telemetry ────────────────────────────────────────────────────────────

/// Telemetry snapshot emitted at build / reload / drift-check points (§ 9).
#[derive(Debug, Clone, Serialize)]
pub struct IrStateTelemetry {
    pub path: String,
    /// FNV-1a snapshot hash, 16 lowercase hex digits.
    pub file_hash: String,
    /// `true` when the on-disk content differs from the snapshot.
    pub dirty: bool,
    pub dependency_dirty: bool,
    /// Unix-seconds wall clock of the snapshot.
    pub generated_at: u64,
    /// Total successful reload calls for this manager.
    pub sync_count: u64,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn temp_project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{name}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        root
    }

    // ── build_ir ────────────────────────────────────────────────────────────

    #[test]
    fn build_ir_produces_synced_state() {
        let path = write_temp("ir_state_build.rs", "fn main() {}");
        let state = build_ir(&path).unwrap();
        assert!(!state.dirty);
        assert_eq!(state.snapshot.ir.source, "fn main() {}");
        assert_eq!(state.snapshot.ir.lang, "rs");
        assert_eq!(state.snapshot.file_path, path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn build_ir_is_deterministic() {
        let path = write_temp("ir_state_det.rs", "fn foo() {}");
        let s1 = build_ir(&path).unwrap();
        let s2 = build_ir(&path).unwrap();
        assert_eq!(s1.snapshot.file_hash, s2.snapshot.file_hash);
        assert_eq!(s1.snapshot.ir, s2.snapshot.ir);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn build_ir_fails_for_missing_file() {
        let path = PathBuf::from("/nonexistent/__ir_state_missing.rs");
        assert!(build_ir(&path).is_err());
    }

    // ── check_drift ─────────────────────────────────────────────────────────

    #[test]
    fn check_drift_detects_file_change() {
        let path = write_temp("ir_state_drift.rs", "fn main() {}");
        let mut state = build_ir(&path).unwrap();
        assert!(!state.dirty);

        std::fs::write(&path, "fn main() { /* changed */ }").unwrap();
        check_drift(&mut state);
        assert!(state.dirty);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn check_drift_stays_false_when_unchanged() {
        let path = write_temp("ir_state_nodrift.rs", "fn stable() {}");
        let mut state = build_ir(&path).unwrap();
        check_drift(&mut state);
        assert!(!state.dirty);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn check_drift_sets_dirty_when_file_missing() {
        let path = write_temp("ir_state_missing.rs", "fn x() {}");
        let mut state = build_ir(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        check_drift(&mut state);
        assert!(state.dirty);
    }

    // ── reload ──────────────────────────────────────────────────────────────

    #[test]
    fn reload_inserts_synced_state() {
        let path = write_temp("ir_state_reload.rs", "fn main() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));
        assert!(!mgr.is_drifted(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_resets_dirty_flag() {
        let path = write_temp("ir_state_reload2.rs", "fn main() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        mgr.mark_dirty(&path);
        assert!(mgr.is_drifted(&path));

        std::fs::write(&path, "fn main() { /* v2 */ }").unwrap();
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_increments_sync_count() {
        let path = write_temp("ir_state_synccount.rs", "fn v1() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        reload(&path, &mut mgr).unwrap();
        let tele = mgr.telemetry(&path).unwrap();
        assert_eq!(tele.sync_count, 2);
        let _ = std::fs::remove_file(&path);
    }

    // ── get_ir ──────────────────────────────────────────────────────────────

    #[test]
    fn get_ir_returns_source_when_synced() {
        let path = write_temp("ir_state_get_ok.rs", "fn synced() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        let ir = get_ir(&path, &mgr).unwrap();
        assert_eq!(ir.source, "fn synced() {}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn get_ir_errors_when_drifted() {
        let path = write_temp("ir_state_get_drift.rs", "fn drift() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        mgr.mark_dirty(&path);
        let err = get_ir(&path, &mgr).unwrap_err();
        assert_eq!(err, IR_DRIFT_ERROR);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn get_ir_detects_filesystem_drift_without_prior_check() {
        let path = write_temp("ir_state_get_fs_drift.rs", "fn v1() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        std::fs::write(&path, "fn v2() {}").unwrap();

        let err = get_ir(&path, &mgr).unwrap_err();
        assert_eq!(err, IR_DRIFT_ERROR);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn get_ir_errors_when_not_tracked() {
        let mgr = IrStateManager::new();
        let path = PathBuf::from("/nonexistent/__ir_state_untracked.rs");
        assert!(get_ir(&path, &mgr).is_err());
    }

    // ── IrStateManager methods ───────────────────────────────────────────────

    #[test]
    fn mark_dirty_transitions_to_drifted() {
        let path = write_temp("ir_state_markdirty.rs", "fn x() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));
        mgr.mark_dirty(&path);
        assert!(mgr.is_drifted(&path));
        assert!(!mgr.is_synced(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn mark_dirty_noop_when_not_tracked() {
        let mut mgr = IrStateManager::new();
        let path = PathBuf::from("/nonexistent/__untracked.rs");
        mgr.mark_dirty(&path); // must not panic
        assert!(!mgr.is_synced(&path));
        assert!(!mgr.is_drifted(&path));
        assert!(!mgr.is_tracked(&path));
    }

    #[test]
    fn check_and_update_drift_updates_flag() {
        let path = write_temp("ir_state_checkupdate.rs", "fn original() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));

        std::fs::write(&path, "fn modified() {}").unwrap();
        mgr.check_and_update_drift(&path);
        assert!(mgr.is_drifted(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snapshot_hash_returns_fnv_hash() {
        let path = write_temp("ir_state_hash.rs", "fn hash_me() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        let hash = mgr.snapshot_hash(&path);
        assert_eq!(hash, Some(crate::ir_sync::hash_content("fn hash_me() {}")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snapshot_hash_none_when_not_tracked() {
        let mgr = IrStateManager::new();
        assert!(mgr.snapshot_hash(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn telemetry_reflects_synced_state() {
        let path = write_temp("ir_state_tele.rs", "fn tele() {}");
        let mut mgr = IrStateManager::new();
        reload(&path, &mut mgr).unwrap();
        let tele = mgr.telemetry(&path).unwrap();
        assert!(!tele.dirty);
        assert_eq!(tele.sync_count, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn telemetry_none_when_not_tracked() {
        let mgr = IrStateManager::new();
        assert!(mgr.telemetry(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn build_project_extracts_import_dependencies() {
        let root = temp_project("ir_state_project_deps");
        let coding = root.join("src/coding.rs");
        let util = root.join("src/util.rs");
        std::fs::write(&coding, "use util::Thing;\nfn main() {}\n").unwrap();
        std::fs::write(&util, "pub struct Thing;\n").unwrap();

        let mut mgr = IrStateManager::new();
        mgr.build_project(&root).unwrap();

        assert_eq!(mgr.dependencies(&coding), vec![util.clone()]);
        assert_eq!(mgr.dependents(&util), vec![coding.clone()]);
        assert_eq!(mgr.project_ir().edges.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dirty_propagates_to_dependents() {
        let root = temp_project("ir_state_dirty_propagates");
        let coding = root.join("src/coding.rs");
        let util = root.join("src/util.rs");
        std::fs::write(&coding, "use util::Thing;\nfn main() {}\n").unwrap();
        std::fs::write(&util, "pub struct Thing;\n").unwrap();

        let mut mgr = IrStateManager::new();
        mgr.build_project(&root).unwrap();
        mgr.mark_dirty(&util);

        assert!(mgr.is_drifted(&util));
        assert!(mgr.is_drifted(&coding));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dependency_drift_blocks_get_ir() {
        let root = temp_project("ir_state_dependency_drift");
        let coding = root.join("src/coding.rs");
        let util = root.join("src/util.rs");
        std::fs::write(&coding, "use util::Thing;\nfn main() {}\n").unwrap();
        std::fs::write(&util, "pub struct Thing;\n").unwrap();

        let mut mgr = IrStateManager::new();
        mgr.build_project(&root).unwrap();
        mgr.mark_dirty(&util);

        let err = get_ir(&coding, &mgr).unwrap_err();
        assert_eq!(err, IR_DRIFT_ERROR);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reload_recursive_reloads_dependents() {
        let root = temp_project("ir_state_reload_recursive");
        let coding = root.join("src/coding.rs");
        let util = root.join("src/util.rs");
        std::fs::write(&coding, "use util::Thing;\nfn main() {}\n").unwrap();
        std::fs::write(&util, "pub struct Thing;\n").unwrap();

        let mut mgr = IrStateManager::new();
        mgr.build_project(&root).unwrap();
        mgr.mark_dirty(&util);
        std::fs::write(&util, "pub struct Thing;\npub struct Other;\n").unwrap();

        mgr.reload_recursive(&util).unwrap();

        assert!(mgr.is_synced(&util));
        assert!(mgr.is_synced(&coding));
        assert!(get_ir(&coding, &mgr).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    // ── invalidate_diff ──────────────────────────────────────────────────────

    #[test]
    fn invalidate_diff_clears_diff_ref() {
        let mut tx = crate::service::dto::TransactionIR {
            latest_diff_ref: Some(crate::service::dto::SessionAppliedDiff {
                summary: "test diff".to_string(),
                files: vec![],
                files_changed: 0,
                lines_added: 0,
                lines_removed: 0,
            }),
            ..Default::default()
        };
        assert!(tx.latest_diff_ref.is_some());
        invalidate_diff(&mut tx);
        assert!(tx.latest_diff_ref.is_none());
    }

    #[test]
    fn invalidate_diff_noop_when_already_none() {
        let mut tx = crate::service::dto::TransactionIR::default();
        invalidate_diff(&mut tx); // must not panic
        assert!(tx.latest_diff_ref.is_none());
    }

    // ── Full state-machine cycle ─────────────────────────────────────────────

    #[test]
    fn state_machine_invalid_synced_drifted_synced() {
        let path = write_temp("ir_state_cycle.rs", "fn v1() {}");
        let mut mgr = IrStateManager::new();

        // Invalid → Synced
        assert!(!mgr.is_synced(&path));
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));

        // Synced → Drifted (file changed on disk)
        std::fs::write(&path, "fn v2() {}").unwrap();
        mgr.check_and_update_drift(&path);
        assert!(mgr.is_drifted(&path));
        assert!(get_ir(&path, &mgr).is_err());

        // Drifted → Synced (reload)
        reload(&path, &mut mgr).unwrap();
        assert!(mgr.is_synced(&path));
        let ir = get_ir(&path, &mgr).unwrap();
        assert_eq!(ir.source, "fn v2() {}");

        let _ = std::fs::remove_file(&path);
    }
}
