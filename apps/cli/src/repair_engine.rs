//! DBM Repair Engine (DBM-REPAIR-ENGINE-SPEC v1.0, Phase C).
//!
//! Builds deterministic repair plans from consistency violations and applies
//! only fixes that are explicitly safe.

use std::fs;
use std::path::{Path, PathBuf};

use crate::consistency_engine::{ConsistencyReport, Violation};
use crate::ir_state::{IrStateManager, ProjectIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairInput {
    pub project: ProjectIr,
    pub report: ConsistencyReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPlan {
    pub fixes: Vec<FixAction>,
    pub is_safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FixAction {
    RemoveUnusedImport {
        file: PathBuf,
        line: usize,
    },
    RemoveUnresolvedImport {
        file: PathBuf,
        line: usize,
    },
    AddMissingModule {
        file: PathBuf,
        path: PathBuf,
    },
    ReloadFile {
        file: PathBuf,
    },
    ReloadSubgraph {
        roots: Vec<PathBuf>,
    },
    BreakCycle {
        scc: Vec<PathBuf>,
        strategy: CycleBreakStrategy,
    },
    NoOp {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CycleBreakStrategy {
    RemoveImport,
    SplitModule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairApplyResult {
    pub applied: Vec<FixAction>,
}

pub struct RepairEngine;

impl RepairEngine {
    pub fn build_plan(input: RepairInput) -> RepairPlan {
        let mut fixes = Vec::new();

        for violation in input.report.violations {
            let fix = match violation {
                Violation::UnresolvedImport(file) => match import_line_for(&input.project, &file) {
                    Some(line) => FixAction::RemoveUnresolvedImport { file, line },
                    None => FixAction::NoOp {
                        reason: format!(
                            "UnresolvedImport in {} has no deterministic import line",
                            file.display()
                        ),
                    },
                },
                Violation::MissingDependency(path) => FixAction::AddMissingModule {
                    file: path.clone(),
                    path,
                },
                Violation::DriftDetected(file) | Violation::InconsistentState(file) => {
                    FixAction::ReloadFile { file }
                }
                Violation::DirtyDependency(file) => FixAction::ReloadSubgraph { roots: vec![file] },
                Violation::CyclicDependency(scc) => FixAction::BreakCycle {
                    scc,
                    strategy: CycleBreakStrategy::RemoveImport,
                },
            };
            fixes.push(fix);
        }

        fixes.sort();
        fixes.dedup();
        let is_safe = is_safe_fixes(&fixes);
        RepairPlan { fixes, is_safe }
    }

    pub fn is_safe(plan: &RepairPlan) -> bool {
        is_safe_fixes(&plan.fixes)
    }

    pub fn preview(plan: &RepairPlan) -> String {
        let mut lines = Vec::new();
        lines.push("[REPAIR PREVIEW]".to_string());
        if plan.fixes.is_empty() {
            lines.push("(no fixes)".to_string());
            return lines.join("\n");
        }

        for fix in &plan.fixes {
            match fix {
                FixAction::RemoveUnusedImport { file, line }
                | FixAction::RemoveUnresolvedImport { file, line } => {
                    lines.push(format!("--- {}", file.display()));
                    lines.push(format!("+++ {}", file.display()));
                    match read_line(file, *line) {
                        Some(content) => {
                            lines.push(format!("@@ -{line},1 +{line},0 @@\n-{content}"))
                        }
                        None => {
                            lines.push(format!("# unable to preview {}:{line}", file.display()))
                        }
                    }
                }
                FixAction::ReloadFile { file } => {
                    lines.push(format!("# reload {}", file.display()));
                }
                FixAction::ReloadSubgraph { roots } => {
                    lines.push(format!("# reload subgraph [{}]", join_paths(roots)));
                }
                FixAction::AddMissingModule { file, path } => {
                    lines.push(format!(
                        "# unsafe: add missing module {} for {}",
                        path.display(),
                        file.display()
                    ));
                }
                FixAction::BreakCycle { scc, strategy } => {
                    lines.push(format!(
                        "# unsafe: break cycle [{}] via {:?}",
                        join_paths(scc),
                        strategy
                    ));
                }
                FixAction::NoOp { reason } => {
                    lines.push(format!("# noop: {reason}"));
                }
            }
        }
        lines.join("\n")
    }

    pub fn apply(
        plan: &RepairPlan,
        manager: &mut IrStateManager,
    ) -> Result<RepairApplyResult, String> {
        if !plan.is_safe {
            return Err("Unsafe fixes require confirmation".to_string());
        }

        let mut applied = Vec::new();
        for fix in &plan.fixes {
            match fix {
                FixAction::RemoveUnusedImport { file, line }
                | FixAction::RemoveUnresolvedImport { file, line } => {
                    remove_import_line(file, *line)?;
                    manager.reload_recursive(file)?;
                    applied.push(fix.clone());
                }
                FixAction::ReloadFile { file } => {
                    manager.reload_recursive(file)?;
                    applied.push(fix.clone());
                }
                FixAction::ReloadSubgraph { roots } => {
                    let mut sorted_roots = roots.clone();
                    sorted_roots.sort();
                    sorted_roots.dedup();
                    for root in &sorted_roots {
                        manager.reload_recursive(root)?;
                    }
                    applied.push(FixAction::ReloadSubgraph {
                        roots: sorted_roots,
                    });
                }
                FixAction::AddMissingModule { .. }
                | FixAction::BreakCycle { .. }
                | FixAction::NoOp { .. } => {
                    return Err("Unsafe fixes require confirmation".to_string());
                }
            }
        }
        Ok(RepairApplyResult { applied })
    }
}

impl RepairPlan {
    pub fn render_plan(&self) -> String {
        let mut lines = Vec::new();
        lines.push("[REPAIR PLAN]".to_string());
        if self.fixes.is_empty() {
            lines.push("- NoOp: no violations".to_string());
        } else {
            for fix in &self.fixes {
                lines.push(format!("- {}", fix.describe()));
            }
        }
        lines.push(String::new());
        lines.push(format!("[SAFE] {}", self.is_safe));
        lines.join("\n")
    }
}

impl FixAction {
    pub fn describe(&self) -> String {
        match self {
            FixAction::RemoveUnusedImport { file, line } => {
                format!("RemoveUnusedImport: {}:{line}", file.display())
            }
            FixAction::RemoveUnresolvedImport { file, line } => {
                format!("RemoveUnresolvedImport: {}:{line}", file.display())
            }
            FixAction::AddMissingModule { file, path } => {
                format!("AddMissingModule: {} -> {}", file.display(), path.display())
            }
            FixAction::ReloadFile { file } => format!("ReloadFile: {}", file.display()),
            FixAction::ReloadSubgraph { roots } => {
                format!("ReloadSubgraph: [{}]", join_paths(roots))
            }
            FixAction::BreakCycle { scc, strategy } => {
                format!("BreakCycle: [{}] via {:?}", join_paths(scc), strategy)
            }
            FixAction::NoOp { reason } => format!("NoOp: {reason}"),
        }
    }
}

fn is_safe_fixes(fixes: &[FixAction]) -> bool {
    fixes.iter().all(|fix| {
        matches!(
            fix,
            FixAction::RemoveUnusedImport { .. }
                | FixAction::RemoveUnresolvedImport { .. }
                | FixAction::ReloadFile { .. }
                | FixAction::ReloadSubgraph { .. }
        )
    })
}

fn import_line_for(project: &ProjectIr, file: &Path) -> Option<usize> {
    project
        .nodes
        .get(file)
        .and_then(|state| first_import_like_line(&state.snapshot.ir.source))
        .or_else(|| {
            fs::read_to_string(file)
                .ok()
                .and_then(|source| first_import_like_line(&source))
        })
}

fn first_import_like_line(source: &str) -> Option<usize> {
    source.lines().enumerate().find_map(|(index, line)| {
        let trimmed = line.trim_start();
        (trimmed.starts_with("use ")
            || trimmed.starts_with("mod ")
            || trimmed.starts_with("pub mod "))
        .then_some(index + 1)
    })
}

fn read_line(path: &Path, line: usize) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .nth(line.saturating_sub(1))
        .map(|line| line.to_string())
}

fn remove_import_line(path: &Path, line: usize) -> Result<(), String> {
    if line == 0 {
        return Err(format!("repair: invalid line 0 for {}", path.display()));
    }
    let source = fs::read_to_string(path)
        .map_err(|e| format!("repair: cannot read {}: {e}", path.display()))?;
    let mut lines: Vec<&str> = source.lines().collect();
    if line > lines.len() {
        return Err(format!(
            "repair: line {line} out of range for {}",
            path.display()
        ));
    }
    let candidate = lines[line - 1].trim_start();
    if !(candidate.starts_with("use ")
        || candidate.starts_with("mod ")
        || candidate.starts_with("pub mod "))
    {
        return Err(format!(
            "repair: refusing to remove non-import line {}:{line}",
            path.display()
        ));
    }
    lines.remove(line - 1);
    let mut next = lines.join("\n");
    if source.ends_with('\n') && !next.is_empty() {
        next.push('\n');
    }
    fs::write(path, next).map_err(|e| format!("repair: cannot write {}: {e}", path.display()))
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::ir_state::{CodeIr, DependencyEdge, DependencyKind, IrSnapshot, IrState};

    fn tmp(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    fn state(path: &Path, source: &str, dirty: bool) -> IrState {
        IrState {
            snapshot: IrSnapshot {
                file_path: path.to_path_buf(),
                file_hash: crate::ir_sync::hash_content(source),
                ir: CodeIr {
                    source: source.to_string(),
                    lang: "rs".to_string(),
                },
                generated_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            },
            dirty,
            dependencies: Vec::new(),
            dependents: Vec::new(),
        }
    }

    #[test]
    fn unresolved_import_maps_to_remove_import() {
        let file = tmp(
            "repair_unresolved.rs",
            "use missing::Thing;\nfn main() {}\n",
        );
        let mut nodes = HashMap::new();
        nodes.insert(
            file.clone(),
            state(&file, "use missing::Thing;\nfn main() {}\n", false),
        );
        let project = ProjectIr {
            nodes,
            edges: vec![DependencyEdge {
                from: file.clone(),
                to: PathBuf::from("/missing.rs"),
                kind: DependencyKind::Import,
            }],
        };
        let report = ConsistencyReport {
            is_consistent: false,
            violations: vec![Violation::UnresolvedImport(file.clone())],
        };

        let plan = RepairEngine::build_plan(RepairInput { project, report });

        assert_eq!(
            plan.fixes,
            vec![FixAction::RemoveUnresolvedImport { file, line: 1 }]
        );
        assert!(plan.is_safe);
    }

    #[test]
    fn drift_maps_to_reload_file() {
        let file = PathBuf::from("apps/a.rs");
        let plan = RepairEngine::build_plan(RepairInput {
            project: ProjectIr::default(),
            report: ConsistencyReport {
                is_consistent: false,
                violations: vec![Violation::DriftDetected(file.clone())],
            },
        });
        assert_eq!(plan.fixes, vec![FixAction::ReloadFile { file }]);
        assert!(plan.is_safe);
    }

    #[test]
    fn dirty_dependency_maps_to_reload_subgraph() {
        let file = PathBuf::from("apps/b.rs");
        let plan = RepairEngine::build_plan(RepairInput {
            project: ProjectIr::default(),
            report: ConsistencyReport {
                is_consistent: false,
                violations: vec![Violation::DirtyDependency(file.clone())],
            },
        });
        assert_eq!(
            plan.fixes,
            vec![FixAction::ReloadSubgraph { roots: vec![file] }]
        );
        assert!(plan.is_safe);
    }

    #[test]
    fn unsafe_plan_refuses_apply() {
        let plan = RepairPlan {
            fixes: vec![FixAction::BreakCycle {
                scc: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
                strategy: CycleBreakStrategy::RemoveImport,
            }],
            is_safe: false,
        };
        let mut manager = IrStateManager::new();

        let err = RepairEngine::apply(&plan, &mut manager).unwrap_err();

        assert_eq!(err, "Unsafe fixes require confirmation");
    }

    #[test]
    fn unresolved_import_without_import_line_is_noop() {
        let file = tmp("repair_no_import_line.rs", "fn main() {}\n");
        let mut nodes = HashMap::new();
        nodes.insert(file.clone(), state(&file, "fn main() {}\n", false));
        let project = ProjectIr {
            nodes,
            edges: Vec::new(),
        };
        let report = ConsistencyReport {
            is_consistent: false,
            violations: vec![Violation::UnresolvedImport(file.clone())],
        };

        let plan = RepairEngine::build_plan(RepairInput { project, report });

        assert!(matches!(plan.fixes.as_slice(), [FixAction::NoOp { .. }]));
        assert!(!plan.is_safe);
        assert!(!RepairEngine::is_safe(&plan));
        let _ = fs::remove_file(file);
    }

    #[test]
    fn safe_plan_applies_remove_import() {
        let file = tmp("repair_apply.rs", "use missing::Thing;\nfn main() {}\n");
        let mut manager = IrStateManager::new();
        crate::ir_state::reload(&file, &mut manager).unwrap();
        let plan = RepairPlan {
            fixes: vec![FixAction::RemoveUnresolvedImport {
                file: file.clone(),
                line: 1,
            }],
            is_safe: true,
        };

        RepairEngine::apply(&plan, &mut manager).unwrap();

        let source = fs::read_to_string(&file).unwrap();
        assert_eq!(source, "fn main() {}\n");
        assert!(manager.is_synced(&file));
        let _ = fs::remove_file(file);
    }

    #[test]
    fn safe_plan_refuses_to_remove_non_import_line() {
        let file = tmp("repair_refuse_non_import.rs", "fn main() {}\n");
        let mut manager = IrStateManager::new();
        crate::ir_state::reload(&file, &mut manager).unwrap();
        let plan = RepairPlan {
            fixes: vec![FixAction::RemoveUnresolvedImport {
                file: file.clone(),
                line: 1,
            }],
            is_safe: true,
        };

        let err = RepairEngine::apply(&plan, &mut manager).unwrap_err();

        assert!(err.contains("refusing to remove non-import line"));
        assert_eq!(fs::read_to_string(&file).unwrap(), "fn main() {}\n");
        let _ = fs::remove_file(file);
    }
}
