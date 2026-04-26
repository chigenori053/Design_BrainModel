use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum, error::ErrorKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, File as SynFile, Item, ItemFn};

const SESSION_DIR: &str = ".dbm/verify_cli";
const SESSION_FILE: &str = "session.json";
const SNAPSHOT_DIR: &str = "snapshots";

pub type ModuleId = String;

/// A uniquely identified function within the project.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct FunctionId {
    pub module: ModuleId,
    pub name: String,
}

impl fmt::Display for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMode {
    #[default]
    Verify,
    Optimize,
    Prove,
}

#[derive(Parser, Debug)]
#[command(name = "verify_cli", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[arg(long, global = true, value_enum, default_value_t = VerifyMode::Verify)]
    mode: VerifyMode,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Load {
        path: PathBuf,
    },
    Extract,
    BuildIr,
    Analyze,
    Optimize {
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    Prove,
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    Replay {
        id: String,
    },
    Compare {
        snapshot_a: String,
        snapshot_b: String,
    },
    State,
}

#[derive(Subcommand, Debug)]
enum SnapshotCommand {
    Save,
    Load { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Session {
    pub mode: VerifyMode,
    pub path: Option<PathBuf>,
    pub extracted: Option<ExtractedGraph>,
    pub ir: Option<IR>,
    pub analysis: Option<AnalysisReport>,
    pub optimization: Option<OptimizationReport>,
    pub proof: Option<ProofReport>,
    pub metrics_stage: Option<MetricsStage>,
    pub history: Vec<Transition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExtractedGraph {
    pub modules: Vec<ExtractedModule>,
    pub function_ids: Vec<FunctionId>,
    pub functions: usize,
    pub edges: Vec<(FunctionId, FunctionId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedModule {
    pub id: ModuleId,
    pub path: PathBuf,
    pub mod_declarations: Vec<String>,
    pub function_count: usize,
}

/// Function-level IR: nodes are FunctionIds, edges are intra-file call relationships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IR {
    pub functions: Vec<FunctionId>,
    pub edges: Vec<(FunctionId, FunctionId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AnalysisReport {
    pub cycle_count: usize,
    pub cycles: Vec<Vec<FunctionId>>,
    pub scc_sizes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OptimizationReport {
    pub cuts: Vec<(FunctionId, FunctionId)>,
    pub edges_in_affected_scc: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProofReport {
    pub valid: bool,
    pub remaining_cycles: usize,
    pub edge_consistency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub id: String,
    pub ir: IR,
    pub analysis: AnalysisReport,
    pub optimization: Option<OptimizationReport>,
    pub metrics_stage: Option<MetricsStage>,
    pub hash: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transition {
    pub command: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricsStage {
    pub analyze: AnalyzeMetrics,
    pub optimize: Option<OptimizeMetrics>,
    pub prove: Option<ProveMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AnalyzeMetrics {
    pub cycles_before: usize,
    pub edge_count_before: usize,
    pub max_scc_size: usize,
    pub avg_scc_size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OptimizeMetrics {
    pub cuts: usize,
    pub edge_count_after: usize,
    pub cut_efficiency: f64,
    pub reduction_ratio: f64,
    pub normalized_cut_score: f64,
    pub baseline_v1_cuts: usize,
    pub baseline_v2_cuts: usize,
    pub optimality_v1: f64,
    pub optimality_v2: f64,
    pub max_scc_size: usize,
    pub avg_scc_size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProveMetrics {
    pub cycles_after: usize,
    pub valid: bool,
    pub deterministic: bool,
    pub hash: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Metrics {
    pub cycles_before: usize,
    pub cycles_after: usize,
    pub cuts: usize,
    pub edge_count_before: usize,
    pub edge_count_after: usize,
    pub cut_efficiency: f64,
    pub reduction_ratio: f64,
    pub normalized_cut_score: f64,
    pub baseline_v1_cuts: usize,
    pub baseline_v2_cuts: usize,
    pub optimality_v1: f64,
    pub optimality_v2: f64,
    pub max_scc_size: usize,
    pub avg_scc_size: f64,
    pub execution_time_ms: u128,
    pub deterministic: bool,
    pub hash: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct CompareReport {
    cuts_diff: isize,
    efficiency_diff: f64,
    cycle_diff: isize,
    improved: bool,
    regressed: bool,
}

pub fn run_with_args(args: Vec<OsString>) -> Result<(), String> {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                print!("{err}");
                return Ok(());
            }
            _ => return Err(err.to_string()),
        },
    };

    let cwd = std::env::current_dir().map_err(|err| format!("failed to resolve cwd: {err}"))?;
    let store = VerifyStore::new(cwd);
    let mut session = store.load_session()?;
    session.mode = cli.mode;

    match cli.command {
        Command::Load { path } => {
            let started = Instant::now();
            let resolved = canonical_project_path(&path)?;
            session.path = Some(resolved.clone());
            session.extracted = None;
            session.ir = None;
            session.analysis = None;
            session.optimization = None;
            session.proof = None;
            session.metrics_stage = None;
            session.history.push(Transition {
                command: "load".to_string(),
                summary: resolved.display().to_string(),
            });
            store.save_session(&session)?;
            print_load(&resolved, started.elapsed().as_millis());
        }
        Command::Extract => {
            let started = Instant::now();
            let path = require_loaded_path(&session)?;
            let extracted = extract_graph(&path)?;
            session.extracted = Some(extracted.clone());
            session.history.push(Transition {
                command: "extract".to_string(),
                summary: format!(
                    "modules={} functions={}",
                    extracted.modules.len(),
                    extracted.functions
                ),
            });
            store.save_session(&session)?;
            let metrics = Metrics {
                edge_count_before: extracted.edges.len(),
                edge_count_after: extracted.edges.len(),
                execution_time_ms: started.elapsed().as_millis(),
                deterministic: true,
                hash: session.ir.as_ref().map(ir_hash).unwrap_or(0),
                ..Metrics::default()
            };
            print_extract(&extracted, &metrics);
        }
        Command::BuildIr => {
            let started = Instant::now();
            let extracted = session.extracted.clone().ok_or_else(|| {
                "error: no extracted graph. Run `verify_cli extract` first.".to_string()
            })?;
            let ir = build_ir(&extracted);
            let hash = ir_hash(&ir);
            session.ir = Some(ir.clone());
            session.analysis = None;
            session.optimization = None;
            session.proof = None;
            session.metrics_stage = None;
            session.history.push(Transition {
                command: "build-ir".to_string(),
                summary: format!("edges={} hash={hash}", ir.edges.len()),
            });
            store.save_session(&session)?;
            let metrics = Metrics {
                edge_count_before: ir.edges.len(),
                edge_count_after: ir.edges.len(),
                execution_time_ms: started.elapsed().as_millis(),
                deterministic: true,
                hash,
                ..Metrics::default()
            };
            print_ir(&ir, &metrics);
        }
        Command::Analyze => {
            let started = Instant::now();
            let ir = session
                .ir
                .clone()
                .ok_or_else(|| "error: no IR. Run `verify_cli build-ir` first.".to_string())?;
            let analysis = analyze_ir(&ir)?;
            let (max_scc_size, avg_scc_size) = compute_scc_metrics_from_sizes(&analysis.scc_sizes);
            session.analysis = Some(analysis.clone());
            session.metrics_stage = Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: analysis.cycle_count,
                    edge_count_before: ir.edges.len(),
                    max_scc_size,
                    avg_scc_size,
                },
                optimize: None,
                prove: None,
            });
            session.history.push(Transition {
                command: "analyze".to_string(),
                summary: format!("cycles={}", analysis.cycle_count),
            });
            store.save_session(&session)?;
            let metrics = final_metrics_from_stage(
                session.metrics_stage.as_ref(),
                started.elapsed().as_millis(),
                true,
                ir_hash(&ir),
            );
            print_analyze(&analysis, &metrics);
        }
        Command::Optimize { dry_run, apply } => {
            let started = Instant::now();
            if dry_run == apply {
                return Err("error: specify exactly one of `--dry-run` or `--apply`.".to_string());
            }
            if apply && session.mode != VerifyMode::Optimize {
                return Err("error: `optimize --apply` requires `--mode optimize`.".to_string());
            }
            let ir = session
                .ir
                .clone()
                .ok_or_else(|| "error: no IR. Run `verify_cli build-ir` first.".to_string())?;
            let analysis = session
                .analysis
                .clone()
                .ok_or_else(|| "error: no analysis. Run `verify_cli analyze` first.".to_string())?;
            let analyze_stage = session
                .metrics_stage
                .as_ref()
                .map(|stage| stage.analyze.clone())
                .ok_or_else(|| {
                    "error: analyze metrics missing. Run `verify_cli analyze` first.".to_string()
                })?;
            let optimization = optimize_ir(&ir)?;
            let baseline_v1_cuts = baseline_cut(&ir, &analysis.cycles);
            let baseline_v2_cuts = baseline_v2_cut(&ir, &analysis.cycles);
            let (max_scc_size, avg_scc_size) = compute_scc_metrics(&analysis.cycles);
            let edge_count_after = if apply {
                let next_ir = apply_optimization(&ir, &optimization)?;
                let after = analyze_ir(&next_ir)?;
                session.ir = Some(next_ir);
                session.analysis = Some(after);
                session
                    .ir
                    .as_ref()
                    .map(|current| current.edges.len())
                    .unwrap_or(0)
            } else {
                analyze_stage
                    .edge_count_before
                    .saturating_sub(optimization.cuts.len())
            };
            let cycles_after = if apply {
                session
                    .analysis
                    .as_ref()
                    .map(|report| report.cycle_count)
                    .unwrap_or(0)
            } else {
                analysis
                    .cycle_count
                    .saturating_sub(optimization.cuts.len().min(analysis.cycle_count))
            };
            session.optimization = Some(optimization.clone());
            session.metrics_stage = Some(MetricsStage {
                analyze: analyze_stage.clone(),
                optimize: Some(OptimizeMetrics {
                    cuts: optimization.cuts.len(),
                    edge_count_after,
                    cut_efficiency: compute_cut_efficiency(
                        analyze_stage.cycles_before,
                        cycles_after,
                        optimization.cuts.len(),
                    ),
                    reduction_ratio: compute_reduction_ratio(
                        analyze_stage.edge_count_before,
                        edge_count_after,
                    ),
                    normalized_cut_score: compute_normalized_cut_score(
                        optimization.cuts.len(),
                        optimization.edges_in_affected_scc,
                    ),
                    baseline_v1_cuts: baseline_v1_cuts.len(),
                    baseline_v2_cuts: baseline_v2_cuts.len(),
                    optimality_v1: compute_optimality_ratio(
                        optimization.cuts.len(),
                        baseline_v1_cuts.len(),
                    ),
                    optimality_v2: compute_optimality_ratio(
                        optimization.cuts.len(),
                        baseline_v2_cuts.len(),
                    ),
                    max_scc_size,
                    avg_scc_size,
                }),
                prove: None,
            });
            session.history.push(Transition {
                command: if apply {
                    "optimize-apply"
                } else {
                    "optimize-dry-run"
                }
                .to_string(),
                summary: format!("cuts={}", optimization.cuts.len()),
            });
            store.save_session(&session)?;
            let hash = session
                .ir
                .as_ref()
                .map(ir_hash)
                .unwrap_or_else(|| ir_hash(&ir));
            let metrics = final_metrics_from_stage(
                session.metrics_stage.as_ref(),
                started.elapsed().as_millis(),
                true,
                hash,
            );
            print_optimize(&optimization, apply, &metrics);
        }
        Command::Prove => {
            let started = Instant::now();
            if session.mode == VerifyMode::Verify {
                return Err("error: `prove` requires `--mode prove`.".to_string());
            }
            let ir = session
                .ir
                .clone()
                .ok_or_else(|| "error: no IR. Run `verify_cli build-ir` first.".to_string())?;
            let proof = prove_ir(&ir)?;
            let prior = session.metrics_stage.clone().unwrap_or_default();
            session.proof = Some(proof.clone());
            session.metrics_stage = Some(MetricsStage {
                analyze: prior.analyze,
                optimize: prior.optimize,
                prove: Some(ProveMetrics {
                    cycles_after: proof.remaining_cycles,
                    valid: proof.valid,
                    deterministic: true,
                    hash: ir_hash(&ir),
                }),
            });
            session.history.push(Transition {
                command: "prove".to_string(),
                summary: format!(
                    "valid={} remaining_cycles={}",
                    proof.valid, proof.remaining_cycles
                ),
            });
            store.save_session(&session)?;
            let metrics = final_metrics_from_stage(
                session.metrics_stage.as_ref(),
                started.elapsed().as_millis(),
                true,
                ir_hash(&ir),
            );
            print_proof(&proof, &metrics);
        }
        Command::Snapshot { command } => match command {
            SnapshotCommand::Save => {
                let started = Instant::now();
                let ir = session
                    .ir
                    .clone()
                    .ok_or_else(|| "error: no IR. Run `verify_cli build-ir` first.".to_string())?;
                let analysis = session
                    .analysis
                    .clone()
                    .unwrap_or_else(|| analyze_ir(&ir).unwrap_or_default());
                let snapshot = Snapshot {
                    id: format!("{:016x}", ir_hash(&ir)),
                    ir: ir.clone(),
                    analysis: analysis.clone(),
                    optimization: session.optimization.clone(),
                    metrics_stage: session.metrics_stage.clone(),
                    hash: ir_hash(&ir),
                };
                store.save_snapshot(&snapshot)?;
                session.history.push(Transition {
                    command: "snapshot-save".to_string(),
                    summary: snapshot.id.clone(),
                });
                store.save_session(&session)?;
                let metrics = final_metrics_from_stage(
                    snapshot.metrics_stage.as_ref(),
                    started.elapsed().as_millis(),
                    true,
                    snapshot.hash,
                );
                print_snapshot_saved(&snapshot, &metrics);
            }
            SnapshotCommand::Load { id } => {
                let started = Instant::now();
                let snapshot = store.load_snapshot(&id)?;
                session.ir = Some(snapshot.ir.clone());
                session.analysis = Some(snapshot.analysis.clone());
                session.optimization = snapshot.optimization.clone();
                session.proof = None;
                session.metrics_stage = snapshot.metrics_stage.clone();
                session.history.push(Transition {
                    command: "snapshot-load".to_string(),
                    summary: id.clone(),
                });
                store.save_session(&session)?;
                let metrics = final_metrics_from_stage(
                    snapshot.metrics_stage.as_ref(),
                    started.elapsed().as_millis(),
                    true,
                    snapshot.hash,
                );
                print_snapshot_loaded(&snapshot, &metrics);
            }
        },
        Command::Replay { id } => {
            let started = Instant::now();
            let snapshot = store.load_snapshot(&id)?;
            let analysis = analyze_ir(&snapshot.ir)?;
            let proof = prove_ir(&snapshot.ir)?;
            let replay_hash = ir_hash(&snapshot.ir);
            let deterministic = replay_hash == snapshot.hash && analysis == snapshot.analysis;
            let metrics = final_metrics_from_stage(
                snapshot.metrics_stage.as_ref(),
                started.elapsed().as_millis(),
                deterministic,
                replay_hash,
            );
            print_replay(&snapshot, &analysis, &proof, &metrics);
        }
        Command::Compare {
            snapshot_a,
            snapshot_b,
        } => {
            let previous = store.load_snapshot(&snapshot_a)?;
            let current = store.load_snapshot(&snapshot_b)?;
            let report = compare_snapshots(&current, &previous);
            print_compare(&snapshot_a, &snapshot_b, &report);
        }
        Command::State => {
            print_state(&session);
        }
    }
    Ok(())
}

fn require_loaded_path(session: &Session) -> Result<PathBuf, String> {
    session
        .path
        .clone()
        .ok_or_else(|| "error: no project loaded. Run `verify_cli load <path>` first.".to_string())
}

fn canonical_project_path(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|err| format!("error: failed to resolve {}: {err}", path.display()))
}

// ---------------------------------------------------------------------------
// Extraction: Function-Level Call Graph (Phase 1.2 — Call-Site Aware)
// ---------------------------------------------------------------------------

/// A single resolved call site: the caller is fully identified as a FunctionId,
/// the callee is still an unresolved name (resolved later in `resolve_calls`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CallSite {
    pub caller: FunctionId,
    pub callee_name: String,
    pub callee_path: Option<Vec<String>>,
}

struct FunctionVisitor<'a> {
    current_fn: &'a FunctionId,
    calls: Vec<CallSite>,
}

impl<'ast> Visit<'ast> for FunctionVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some(callee_path) = extract_callee_path(node) {
            let callee_name = callee_path.last().cloned().unwrap_or_default();
            self.calls.push(CallSite {
                caller: self.current_fn.clone(),
                callee_name,
                callee_path: Some(callee_path),
            });
        }
        visit::visit_expr_call(self, node);
    }
}

fn parse_rust_file(content: &str) -> Option<SynFile> {
    syn::parse_file(content).ok()
}

/// Collect all top-level function definitions in a Rust file for the given module.
fn collect_functions(content: &str, module: &ModuleId) -> Vec<FunctionId> {
    let Some(file) = parse_rust_file(content) else {
        return Vec::new();
    };
    let mut functions = Vec::new();
    for item in file.items {
        if let Item::Fn(item_fn) = item {
            functions.push(FunctionId {
                module: module.clone(),
                name: item_fn.sig.ident.to_string(),
            });
        }
    }
    functions
}

fn extract_callee_path(node: &ExprCall) -> Option<Vec<String>> {
    match node.func.as_ref() {
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .into(),
        _ => None,
    }
}

/// Collect call sites within a single Rust file.
///
/// Each call site records:
/// - `caller`: the full `FunctionId` of the enclosing function (visitor context)
/// - `callee_name`: the raw name of the called function (resolved later)
/// - `callee_path`: the syntactic callee path from the AST
///
/// Only top-level functions and their expression calls are visited.
fn collect_callsites(content: &str, module_functions: &[FunctionId]) -> Vec<CallSite> {
    let Some(file) = parse_rust_file(content) else {
        return Vec::new();
    };
    let fn_by_name: BTreeMap<&str, &FunctionId> = module_functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();

    let mut sites: Vec<CallSite> = Vec::new();
    for item in &file.items {
        if let Item::Fn(item_fn) = item {
            let caller_name = item_fn.sig.ident.to_string();
            let Some(caller) = fn_by_name.get(caller_name.as_str()).copied() else {
                continue;
            };
            let mut visitor = FunctionVisitor {
                current_fn: caller,
                calls: Vec::new(),
            };
            visitor.visit_block(&item_fn.block);
            sites.extend(visitor.calls);
        }
    }

    // Sort and dedup for determinism
    sites.sort();
    sites.dedup();
    sites
}

/// Resolve call sites into concrete `(FunctionId, FunctionId)` edges.
///
/// Phase 1.3 rules:
/// 1. If the callee path has more than one segment, reject it as non-local.
/// 2. Callee must exist in `functions` (checked defensively here).
/// 3. Callee must be in the same module as the caller.
/// 4. If multiple candidates, choose the lexicographically smallest FunctionId.
/// 5. Self-loops (caller == callee, i.e. genuine recursion) are KEPT.
fn resolve_calls(
    functions: &[FunctionId],
    callsites: &[CallSite],
) -> Vec<(FunctionId, FunctionId)> {
    // Build name → candidates map
    let mut by_name: BTreeMap<&str, Vec<&FunctionId>> = BTreeMap::new();
    for fid in functions {
        by_name.entry(fid.name.as_str()).or_default().push(fid);
    }

    let mut edges = BTreeSet::new();
    for cs in callsites {
        if let Some(path) = &cs.callee_path {
            if path.len() > 1 {
                continue;
            }
        }
        let Some(candidates) = by_name.get(cs.callee_name.as_str()) else {
            continue;
        };
        // Rule 2+3: same module, lex smallest
        let callee = candidates
            .iter()
            .filter(|c| c.module == cs.caller.module)
            .min()
            .copied();
        let Some(callee) = callee else {
            continue;
        };

        // Rule 4: keep self-loops (genuine recursion)
        edges.insert((cs.caller.clone(), callee.clone()));
    }

    let mut result: Vec<_> = edges.into_iter().collect();
    result.sort();
    result.dedup();
    result
}

fn extract_graph(root: &Path) -> Result<ExtractedGraph, String> {
    let source_root = resolve_source_root(root);
    let files = collect_rust_files(&source_root)?;
    let mut modules = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(root)
            .or_else(|_| file.strip_prefix(&source_root))
            .map_err(|err| format!("error: failed to relativize {}: {err}", file.display()))?;
        let content = fs::read_to_string(file)
            .map_err(|err| format!("error: failed to read {}: {err}", file.display()))?;
        modules.push(ExtractedModule {
            id: module_id_from_relative(relative),
            path: relative.to_path_buf(),
            mod_declarations: parse_mod_declarations(&content),
            function_count: parse_function_count(&content),
        });
    }
    modules.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    // Function-level extraction
    let mut all_function_ids: Vec<FunctionId> = Vec::new();
    let mut all_edges: BTreeSet<(FunctionId, FunctionId)> = BTreeSet::new();
    let mut total_functions = 0usize;

    for file in &files {
        let relative = file
            .strip_prefix(root)
            .or_else(|_| file.strip_prefix(&source_root))
            .map_err(|err| format!("error: failed to relativize {}: {err}", file.display()))?;
        let module_id = module_id_from_relative(relative);
        let content = fs::read_to_string(file)
            .map_err(|err| format!("error: failed to read {}: {err}", file.display()))?;

        // Collect functions defined in this file
        let file_functions = collect_functions(&content, &module_id);
        total_functions += file_functions.len();

        // Collect intra-file call relationships
        let callsites = collect_callsites(&content, &file_functions);
        let file_edges = resolve_calls(&file_functions, &callsites);

        all_function_ids.extend(file_functions);
        all_edges.extend(file_edges);
    }

    // Deterministic construction: sort and dedup
    all_function_ids.sort();
    all_function_ids.dedup();

    let edges: Vec<(FunctionId, FunctionId)> = all_edges.into_iter().collect();

    Ok(ExtractedGraph {
        modules,
        function_ids: all_function_ids,
        functions: total_functions,
        edges,
    })
}

fn resolve_source_root(root: &Path) -> PathBuf {
    let candidate = root.join("src");
    if candidate.exists() {
        candidate
    } else {
        root.to_path_buf()
    }
}

fn collect_rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut collected = Vec::new();
    collect_rust_files_recursive(root, &mut collected)?;
    collected.sort();
    Ok(collected)
}

fn collect_rust_files_recursive(root: &Path, collected: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|err| format!("error: failed to read {}: {err}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "error: failed to read directory entry in {}: {err}",
                root.display()
            )
        })?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if entry
            .file_type()
            .map_err(|err| format!("error: failed to stat {}: {err}", path.display()))?
            .is_dir()
        {
            if matches!(
                file_name,
                ".git" | ".dbm" | "target" | "node_modules" | "snapshot"
            ) {
                continue;
            }
            collect_rust_files_recursive(&path, collected)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            collected.push(path);
        }
    }
    Ok(())
}

fn module_id_from_relative(relative: &Path) -> String {
    let mut components = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if components.first().map(String::as_str) == Some("src") {
        components.remove(0);
    }
    if components.last().map(String::as_str) == Some("lib.rs")
        || components.last().map(String::as_str) == Some("main.rs")
    {
        return "crate".to_string();
    }
    if components.last().map(String::as_str) == Some("mod.rs") {
        components.pop();
    } else if let Some(last) = components.last_mut() {
        if let Some(stripped) = last.strip_suffix(".rs") {
            *last = stripped.to_string();
        }
    }
    if components.is_empty() {
        "crate".to_string()
    } else {
        components.join("::")
    }
}

fn parse_mod_declarations(content: &str) -> Vec<String> {
    let mut modules = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let candidate = trimmed
            .strip_prefix("mod ")
            .or_else(|| trimmed.strip_prefix("pub mod "));
        if let Some(rest) = candidate {
            let name = rest
                .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                modules.insert(name.to_string());
            }
        }
    }
    modules.into_iter().collect()
}

fn parse_function_count(content: &str) -> usize {
    collect_top_level_item_fns(content).len()
}

fn collect_top_level_item_fns(content: &str) -> Vec<ItemFn> {
    let Some(file) = parse_rust_file(content) else {
        return Vec::new();
    };
    file.items
        .into_iter()
        .filter_map(|item| match item {
            Item::Fn(item_fn) => Some(item_fn),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// IR Construction
// ---------------------------------------------------------------------------

fn build_ir(extracted: &ExtractedGraph) -> IR {
    let mut functions = extracted.function_ids.clone();
    functions.sort();
    functions.dedup();

    let fn_set: BTreeSet<_> = functions.iter().cloned().collect();
    let mut edges: Vec<(FunctionId, FunctionId)> = extracted
        .edges
        .iter()
        .filter(|(from, to)| fn_set.contains(from) && fn_set.contains(to))
        .cloned()
        .collect();
    edges.sort();
    edges.dedup();

    IR { functions, edges }
}

// ---------------------------------------------------------------------------
// Analysis: Tarjan SCC on FunctionId Graph
// ---------------------------------------------------------------------------

fn analyze_ir(ir: &IR) -> Result<AnalysisReport, String> {
    validate_ir(ir)?;
    let mut adjacency = BTreeMap::<FunctionId, Vec<FunctionId>>::new();
    for func in &ir.functions {
        adjacency.entry(func.clone()).or_default();
    }
    for (from, to) in &ir.edges {
        adjacency.entry(from.clone()).or_default().push(to.clone());
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }

    let mut index = 0usize;
    let mut stack: Vec<FunctionId> = Vec::new();
    let mut on_stack: BTreeSet<FunctionId> = BTreeSet::new();
    let mut indices: BTreeMap<FunctionId, usize> = BTreeMap::new();
    let mut lowlinks: BTreeMap<FunctionId, usize> = BTreeMap::new();
    let mut components: Vec<Vec<FunctionId>> = Vec::new();

    for func in &ir.functions {
        if !indices.contains_key(func) {
            strong_connect(
                func,
                &adjacency,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &mut components,
            );
        }
    }

    let scc_sizes = components.iter().map(Vec::len).collect::<Vec<_>>();
    let mut cycles = extract_cycles(&components, &ir.edges);
    cycles.sort_by(|lhs, rhs| lhs.len().cmp(&rhs.len()).then(lhs.cmp(rhs)));
    Ok(AnalysisReport {
        cycle_count: cycles.len(),
        cycles,
        scc_sizes,
    })
}

#[allow(clippy::too_many_arguments)]
fn strong_connect(
    func: &FunctionId,
    adjacency: &BTreeMap<FunctionId, Vec<FunctionId>>,
    index: &mut usize,
    stack: &mut Vec<FunctionId>,
    on_stack: &mut BTreeSet<FunctionId>,
    indices: &mut BTreeMap<FunctionId, usize>,
    lowlinks: &mut BTreeMap<FunctionId, usize>,
    components: &mut Vec<Vec<FunctionId>>,
) {
    indices.insert(func.clone(), *index);
    lowlinks.insert(func.clone(), *index);
    *index += 1;
    stack.push(func.clone());
    on_stack.insert(func.clone());

    if let Some(neighbors) = adjacency.get(func) {
        for neighbor in neighbors {
            if !indices.contains_key(neighbor) {
                strong_connect(
                    neighbor, adjacency, index, stack, on_stack, indices, lowlinks, components,
                );
                let neighbor_low = *lowlinks.get(neighbor).unwrap_or(&usize::MAX);
                let current_low = *lowlinks.get(func).unwrap_or(&usize::MAX);
                lowlinks.insert(func.clone(), current_low.min(neighbor_low));
            } else if on_stack.contains(neighbor) {
                let neighbor_index = *indices.get(neighbor).unwrap_or(&usize::MAX);
                let current_low = *lowlinks.get(func).unwrap_or(&usize::MAX);
                lowlinks.insert(func.clone(), current_low.min(neighbor_index));
            }
        }
    }

    if lowlinks.get(func) == indices.get(func) {
        let mut component = Vec::new();
        while let Some(node) = stack.pop() {
            on_stack.remove(&node);
            component.push(node.clone());
            if &node == func {
                break;
            }
        }
        component.sort();
        components.push(component);
    }
}

fn canonicalize_cycle(mut cycle: Vec<FunctionId>) -> Vec<FunctionId> {
    cycle.sort();
    cycle
}

/// Extract cycles from Tarjan SCC output using the correct definition:
/// A cycle exists if SCC size ≥ 2 OR (SCC size == 1 AND self-loop exists).
fn extract_cycles(
    sccs: &[Vec<FunctionId>],
    edges: &[(FunctionId, FunctionId)],
) -> Vec<Vec<FunctionId>> {
    // Build a set of edges for O(1) self-loop lookup
    let edge_set: BTreeSet<&(FunctionId, FunctionId)> = edges.iter().collect();
    sccs.iter()
        .filter(|scc| {
            if scc.len() > 1 {
                true
            } else {
                let node = &scc[0];
                edge_set.contains(&(node.clone(), node.clone()))
            }
        })
        .map(|scc| canonicalize_cycle(scc.clone()))
        .collect()
}

/// Compute max and avg SCC size from a pre-computed sizes vector.
fn compute_scc_metrics_from_sizes(scc_sizes: &[usize]) -> (usize, f64) {
    if scc_sizes.is_empty() {
        return (0, 0.0);
    }
    let max = scc_sizes.iter().copied().max().unwrap_or(0);
    let sum: usize = scc_sizes.iter().sum();
    let avg = sum as f64 / scc_sizes.len() as f64;
    (max, avg)
}

// ---------------------------------------------------------------------------
// Baseline Strategies
// ---------------------------------------------------------------------------

fn baseline_cut(ir: &IR, sccs: &[Vec<FunctionId>]) -> Vec<(FunctionId, FunctionId)> {
    let mut cuts = Vec::new();
    let mut ordered_sccs = sccs.to_vec();
    ordered_sccs.sort_by(|lhs, rhs| lhs.len().cmp(&rhs.len()).then(lhs.cmp(rhs)));
    for scc in ordered_sccs {
        let mut edges_in_scc = collect_edges(ir, &scc);
        edges_in_scc.sort();
        if let Some(edge) = edges_in_scc.into_iter().next() {
            cuts.push(edge);
        }
    }
    cuts
}

fn compute_out_degree(
    edges: &[(FunctionId, FunctionId)],
    scc: &BTreeSet<FunctionId>,
) -> BTreeMap<FunctionId, usize> {
    let mut degrees = BTreeMap::new();
    for func in scc {
        degrees.insert(func.clone(), 0);
    }
    for (from, to) in edges {
        if scc.contains(from) && scc.contains(to) {
            *degrees.entry(from.clone()).or_insert(0) += 1;
        }
    }
    degrees
}

fn select_max_degree_node(degrees: &BTreeMap<FunctionId, usize>) -> FunctionId {
    degrees
        .iter()
        .max_by(|(lhs_fn, lhs_deg), (rhs_fn, rhs_deg)| {
            lhs_deg.cmp(rhs_deg).then_with(|| rhs_fn.cmp(lhs_fn))
        })
        .map(|(func, _)| func.clone())
        .unwrap_or_else(|| FunctionId {
            module: String::new(),
            name: String::new(),
        })
}

fn select_edge(
    edges: &[(FunctionId, FunctionId)],
    node: FunctionId,
) -> Option<(FunctionId, FunctionId)> {
    let mut candidates = edges
        .iter()
        .filter(|(from, _)| *from == node)
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

fn baseline_v2_cut(ir: &IR, sccs: &[Vec<FunctionId>]) -> Vec<(FunctionId, FunctionId)> {
    let mut cuts = Vec::new();
    let mut ordered_sccs = sccs.to_vec();
    ordered_sccs.sort_by(|lhs, rhs| lhs.len().cmp(&rhs.len()).then(lhs.cmp(rhs)));
    for scc in ordered_sccs {
        let scc_set = scc.iter().cloned().collect::<BTreeSet<_>>();
        let edges_in_scc = collect_edges(ir, &scc);
        let degrees = compute_out_degree(&edges_in_scc, &scc_set);
        let node = select_max_degree_node(&degrees);
        if let Some(edge) = select_edge(&edges_in_scc, node) {
            cuts.push(edge);
        }
    }
    cuts
}

fn collect_edges(ir: &IR, scc: &[FunctionId]) -> Vec<(FunctionId, FunctionId)> {
    let nodes: BTreeSet<_> = scc.iter().cloned().collect();
    let mut edges = ir
        .edges
        .iter()
        .filter(|(from, to)| nodes.contains(from) && nodes.contains(to))
        .cloned()
        .collect::<Vec<_>>();
    edges.sort();
    edges
}

fn compute_optimality_ratio(cuts: usize, baseline_cuts: usize) -> f64 {
    if baseline_cuts == 0 {
        1.0
    } else {
        cuts as f64 / baseline_cuts as f64
    }
}

fn compute_scc_metrics(sccs: &[Vec<FunctionId>]) -> (usize, f64) {
    let sizes: Vec<usize> = sccs.iter().map(Vec::len).collect();
    compute_scc_metrics_from_sizes(&sizes)
}

// ---------------------------------------------------------------------------
// Optimization
// ---------------------------------------------------------------------------

fn optimize_ir(ir: &IR) -> Result<OptimizationReport, String> {
    let analysis = analyze_ir(ir)?;
    let cycle_nodes = analysis
        .cycles
        .iter()
        .map(|cycle| cycle.iter().cloned().collect::<BTreeSet<_>>())
        .collect::<Vec<_>>();
    let mut cuts = Vec::new();
    let mut edges_in_affected_scc = 0usize;
    for cycle in cycle_nodes {
        let mut scc_edges = ir
            .edges
            .iter()
            .filter(|(from, to)| cycle.contains(from) && cycle.contains(to))
            .cloned()
            .collect::<Vec<_>>();
        scc_edges.sort();
        edges_in_affected_scc += scc_edges.len();
        let edge = scc_edges
            .into_iter()
            .next()
            .ok_or_else(|| "error: failed to derive deterministic cut edge.".to_string())?;
        cuts.push(edge);
    }
    cuts.sort();
    cuts.dedup();
    Ok(OptimizationReport {
        cuts,
        edges_in_affected_scc,
    })
}

fn apply_optimization(ir: &IR, optimization: &OptimizationReport) -> Result<IR, String> {
    validate_ir(ir)?;
    let cuts: BTreeSet<_> = optimization.cuts.iter().cloned().collect();
    let mut edges = ir
        .edges
        .iter()
        .filter(|edge| !cuts.contains(edge))
        .cloned()
        .collect::<Vec<_>>();
    edges.sort();
    Ok(IR {
        functions: ir.functions.clone(),
        edges,
    })
}

fn prove_ir(ir: &IR) -> Result<ProofReport, String> {
    let edge_consistency = validate_ir(ir).is_ok();
    let analysis = analyze_ir(ir)?;
    Ok(ProofReport {
        valid: analysis.cycle_count == 0 && edge_consistency,
        remaining_cycles: analysis.cycle_count,
        edge_consistency,
    })
}

fn validate_ir(ir: &IR) -> Result<(), String> {
    let functions: BTreeSet<_> = ir.functions.iter().cloned().collect();
    let edge_count = ir.edges.iter().cloned().collect::<BTreeSet<_>>().len();
    if edge_count != ir.edges.len() {
        return Err("error: duplicate edge detected".to_string());
    }
    for (from, to) in &ir.edges {
        if !functions.contains(from) || !functions.contains(to) {
            return Err(format!("error: invalid edge {} -> {}", from, to));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics Helpers
// ---------------------------------------------------------------------------

fn compute_normalized_cut_score(cuts: usize, edges_in_scc: usize) -> f64 {
    if edges_in_scc == 0 {
        0.0
    } else {
        cuts as f64 / edges_in_scc as f64
    }
}

fn compute_cut_efficiency(cycles_before: usize, cycles_after: usize, cuts: usize) -> f64 {
    if cuts == 0 {
        0.0
    } else {
        (cycles_before.saturating_sub(cycles_after)) as f64 / cuts as f64
    }
}

fn compute_reduction_ratio(edge_count_before: usize, edge_count_after: usize) -> f64 {
    if edge_count_before == 0 {
        0.0
    } else {
        (edge_count_before.saturating_sub(edge_count_after)) as f64 / edge_count_before as f64
    }
}

fn final_metrics_from_stage(
    stage: Option<&MetricsStage>,
    execution_time_ms: u128,
    deterministic: bool,
    hash: u64,
) -> Metrics {
    let Some(stage) = stage else {
        return Metrics {
            execution_time_ms,
            deterministic,
            hash,
            ..Metrics::default()
        };
    };
    Metrics {
        cycles_before: stage.analyze.cycles_before,
        cycles_after: stage
            .prove
            .as_ref()
            .map(|prove| prove.cycles_after)
            .unwrap_or(stage.analyze.cycles_before),
        cuts: stage.optimize.as_ref().map(|opt| opt.cuts).unwrap_or(0),
        edge_count_before: stage.analyze.edge_count_before,
        edge_count_after: stage
            .optimize
            .as_ref()
            .map(|opt| opt.edge_count_after)
            .unwrap_or(stage.analyze.edge_count_before),
        cut_efficiency: stage
            .optimize
            .as_ref()
            .map(|opt| opt.cut_efficiency)
            .unwrap_or(0.0),
        reduction_ratio: stage
            .optimize
            .as_ref()
            .map(|opt| opt.reduction_ratio)
            .unwrap_or(0.0),
        normalized_cut_score: stage
            .optimize
            .as_ref()
            .map(|opt| opt.normalized_cut_score)
            .unwrap_or(0.0),
        baseline_v1_cuts: stage
            .optimize
            .as_ref()
            .map(|opt| opt.baseline_v1_cuts)
            .unwrap_or(0),
        baseline_v2_cuts: stage
            .optimize
            .as_ref()
            .map(|opt| opt.baseline_v2_cuts)
            .unwrap_or(0),
        optimality_v1: stage
            .optimize
            .as_ref()
            .map(|opt| opt.optimality_v1)
            .unwrap_or(1.0),
        optimality_v2: stage
            .optimize
            .as_ref()
            .map(|opt| opt.optimality_v2)
            .unwrap_or(1.0),
        max_scc_size: stage
            .optimize
            .as_ref()
            .map(|opt| opt.max_scc_size)
            .unwrap_or(stage.analyze.max_scc_size),
        avg_scc_size: stage
            .optimize
            .as_ref()
            .map(|opt| opt.avg_scc_size)
            .unwrap_or(stage.analyze.avg_scc_size),
        execution_time_ms,
        deterministic: stage
            .prove
            .as_ref()
            .map(|prove| prove.deterministic)
            .unwrap_or(deterministic),
        hash: stage.prove.as_ref().map(|prove| prove.hash).unwrap_or(hash),
    }
}

fn compare_snapshots(current: &Snapshot, previous: &Snapshot) -> CompareReport {
    let current_metrics =
        final_metrics_from_stage(current.metrics_stage.as_ref(), 0, true, current.hash);
    let previous_metrics =
        final_metrics_from_stage(previous.metrics_stage.as_ref(), 0, true, previous.hash);
    let cuts_diff = current_metrics.cuts as isize - previous_metrics.cuts as isize;
    let efficiency_diff = current_metrics.cut_efficiency - previous_metrics.cut_efficiency;
    let cycle_diff = current_metrics.cycles_after as isize - previous_metrics.cycles_after as isize;
    let improved = current_metrics.cycles_after == 0
        && previous_metrics.cycles_after == 0
        && current_metrics.cuts <= previous_metrics.cuts
        && current_metrics.normalized_cut_score <= previous_metrics.normalized_cut_score
        && current_metrics.optimality_v2 <= previous_metrics.optimality_v2;
    let regressed = current_metrics.cycles_after > 0
        || current_metrics.cuts > previous_metrics.cuts
        || current_metrics.normalized_cut_score > previous_metrics.normalized_cut_score
        || current_metrics.optimality_v2 > previous_metrics.optimality_v2;
    CompareReport {
        cuts_diff,
        efficiency_diff,
        cycle_diff,
        improved,
        regressed,
    }
}

pub fn ir_hash(ir: &IR) -> u64 {
    let payload = serde_json::to_vec(ir).unwrap_or_default();
    let digest = Sha256::digest(payload);
    let bytes: [u8; 8] = digest[..8].try_into().unwrap_or([0; 8]);
    u64::from_be_bytes(bytes)
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

struct VerifyStore {
    root: PathBuf,
}

impl VerifyStore {
    fn new(cwd: PathBuf) -> Self {
        Self {
            root: cwd.join(SESSION_DIR),
        }
    }

    fn ensure_dirs(&self) -> Result<(), String> {
        fs::create_dir_all(self.root.join(SNAPSHOT_DIR))
            .map_err(|err| format!("error: failed to create {}: {err}", self.root.display()))
    }

    fn session_path(&self) -> PathBuf {
        self.root.join(SESSION_FILE)
    }

    fn snapshot_path(&self, id: &str) -> PathBuf {
        self.root.join(SNAPSHOT_DIR).join(format!("{id}.json"))
    }

    fn load_session(&self) -> Result<Session, String> {
        self.ensure_dirs()?;
        let path = self.session_path();
        if !path.exists() {
            return Ok(Session::default());
        }
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("error: failed to read {}: {err}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|err| format!("error: failed to parse {}: {err}", path.display()))
    }

    fn save_session(&self, session: &Session) -> Result<(), String> {
        self.ensure_dirs()?;
        let payload = serde_json::to_string_pretty(session)
            .map_err(|err| format!("error: failed to serialize session: {err}"))?;
        fs::write(self.session_path(), payload)
            .map_err(|err| format!("error: failed to write session: {err}"))
    }

    fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), String> {
        self.ensure_dirs()?;
        let payload = serde_json::to_string_pretty(snapshot)
            .map_err(|err| format!("error: failed to serialize snapshot: {err}"))?;
        fs::write(self.snapshot_path(&snapshot.id), payload)
            .map_err(|err| format!("error: failed to write snapshot {}: {err}", snapshot.id))
    }

    fn load_snapshot(&self, id: &str) -> Result<Snapshot, String> {
        self.ensure_dirs()?;
        let path = self.snapshot_path(id);
        if !path.exists() {
            return Err(format!("error: snapshot `{id}` not found."));
        }
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("error: failed to read {}: {err}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|err| format!("error: failed to parse {}: {err}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// Print helpers
// ---------------------------------------------------------------------------

fn print_load(path: &Path, time_ms: u128) {
    println!("[LOAD]");
    println!("path: {}", path.display());
    println!();
    print_metrics(&Metrics {
        execution_time_ms: time_ms,
        deterministic: true,
        ..Metrics::default()
    });
}

fn print_extract(extracted: &ExtractedGraph, metrics: &Metrics) {
    println!("[EXTRACT]");
    println!("modules: {}", extracted.modules.len());
    println!("functions: {}", extracted.functions);
    println!();
    print_metrics(metrics);
}

fn print_ir(ir: &IR, metrics: &Metrics) {
    println!("[IR]");
    println!("functions: {}", ir.functions.len());
    println!("edges: {}", ir.edges.len());
    println!();
    print_metrics(metrics);
}

fn print_analyze(analysis: &AnalysisReport, metrics: &Metrics) {
    println!("[ANALYZE]");
    println!("cycles: {}", analysis.cycle_count);
    for cycle in &analysis.cycles {
        println!("cycle:");
        for node in cycle {
            println!("  - {}", node);
        }
    }
    println!();
    // Derive SCC metrics directly from the analysis report so they are always
    // populated even when optimization has not yet been run.
    let (max_scc, avg_scc) = compute_scc_metrics_from_sizes(&analysis.scc_sizes);
    println!("[SCC]");
    println!("max_scc_size: {max_scc}");
    println!("avg_scc_size: {avg_scc:.6}");
    println!();
    print_metrics(metrics);
}

fn print_optimize(optimization: &OptimizationReport, applied: bool, metrics: &Metrics) {
    println!("[OPTIMIZE]");
    println!("mode: {}", if applied { "apply" } else { "dry-run" });
    println!("cuts: {}", optimization.cuts.len());
    println!("baseline_v1_cuts: {}", metrics.baseline_v1_cuts);
    println!("baseline_v2_cuts: {}", metrics.baseline_v2_cuts);
    println!("optimality_v1: {:.6}", metrics.optimality_v1);
    println!("optimality_v2: {:.6}", metrics.optimality_v2);
    for (from, to) in &optimization.cuts {
        println!("cut: {from} -> {to}");
    }
    println!();
    println!("[SCC]");
    println!("max_scc_size: {}", metrics.max_scc_size);
    println!("avg_scc_size: {:.6}", metrics.avg_scc_size);
    println!();
    print_metrics(metrics);
}

fn print_proof(proof: &ProofReport, metrics: &Metrics) {
    println!("[PROVE]");
    println!("valid: {}", proof.valid);
    println!("remaining_cycles: {}", proof.remaining_cycles);
    println!("edge_consistency: {}", proof.edge_consistency);
    println!();
    print_metrics(metrics);
}

fn print_snapshot_saved(snapshot: &Snapshot, metrics: &Metrics) {
    println!("[SNAPSHOT]");
    println!("saved: {}", snapshot.id);
    println!("cycle_count: {}", snapshot.analysis.cycle_count);
    println!();
    print_metrics(metrics);
}

fn print_snapshot_loaded(snapshot: &Snapshot, metrics: &Metrics) {
    println!("[SNAPSHOT]");
    println!("loaded: {}", snapshot.id);
    println!("cycle_count: {}", snapshot.analysis.cycle_count);
    println!();
    print_metrics(metrics);
}

fn print_replay(
    snapshot: &Snapshot,
    analysis: &AnalysisReport,
    proof: &ProofReport,
    metrics: &Metrics,
) {
    println!("[REPLAY]");
    println!("snapshot: {}", snapshot.id);
    println!("cycle_count: {}", analysis.cycle_count);
    println!("valid: {}", proof.valid);
    println!("deterministic: {}", metrics.deterministic);
    println!();
    print_metrics(metrics);
}

fn print_compare(snapshot_a: &str, snapshot_b: &str, report: &CompareReport) {
    println!("[COMPARE]");
    println!("snapshot_a: {snapshot_a}");
    println!("snapshot_b: {snapshot_b}");
    println!("cuts_diff: {}", report.cuts_diff);
    println!("efficiency_diff: {:+.6}", report.efficiency_diff);
    println!("cycle_diff: {}", report.cycle_diff);
    println!("improved: {}", report.improved);
    println!("regressed: {}", report.regressed);
}

fn print_state(session: &Session) {
    println!("[STATE]");
    println!(
        "path: {}",
        session
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string())
    );
    println!(
        "extracted: {}",
        session
            .extracted
            .as_ref()
            .map(|graph| graph.modules.len().to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!(
        "ir: {}",
        session
            .ir
            .as_ref()
            .map(|ir| ir.functions.len().to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!(
        "analysis: {}",
        session
            .analysis
            .as_ref()
            .map(|analysis| analysis.cycle_count.to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!(
        "optimization: {}",
        session
            .optimization
            .as_ref()
            .map(|report| report.cuts.len().to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!(
        "proof: {}",
        session
            .proof
            .as_ref()
            .map(|proof| proof.valid.to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!(
        "metrics_stage: {}",
        session
            .metrics_stage
            .as_ref()
            .map(|stage| stage.analyze.cycles_before.to_string())
            .unwrap_or_else(|| "false".to_string())
    );
    println!("history: {}", session.history.len());
}

fn print_metrics(metrics: &Metrics) {
    println!("[METRICS]");
    println!("cycles_before: {}", metrics.cycles_before);
    println!("cycles_after: {}", metrics.cycles_after);
    println!("cuts: {}", metrics.cuts);
    println!("edge_count_before: {}", metrics.edge_count_before);
    println!("edge_count_after: {}", metrics.edge_count_after);
    println!("cut_efficiency: {:.6}", metrics.cut_efficiency);
    println!("reduction_ratio: {:.6}", metrics.reduction_ratio);
    println!("normalized_cut_score: {:.6}", metrics.normalized_cut_score);
    println!("baseline_v1_cuts: {}", metrics.baseline_v1_cuts);
    println!("baseline_v2_cuts: {}", metrics.baseline_v2_cuts);
    println!("optimality_v1: {:.6}", metrics.optimality_v1);
    println!("optimality_v2: {:.6}", metrics.optimality_v2);
    println!("max_scc_size: {}", metrics.max_scc_size);
    println!("avg_scc_size: {:.6}", metrics.avg_scc_size);
    println!("time_ms: {}", metrics.execution_time_ms);
    println!("hash: {}", metrics.hash);
    println!("deterministic: {}", metrics.deterministic);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Fixture helpers
    // -----------------------------------------------------------------------

    /// Fixture with intra-file mutual recursion (foo ↔ bar in module a).
    fn write_fixture(root: &Path) {
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"verify_cli_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(root.join("src/lib.rs"), "pub mod a;\npub mod b;\n").expect("lib");
        // Module a: foo and bar call each other (mutual recursion)
        fs::write(
            root.join("src/a.rs"),
            "pub fn foo() { bar(); }\npub fn bar() { foo(); }\n",
        )
        .expect("a");
        // Module b: no recursion
        fs::write(root.join("src/b.rs"), "pub fn baz() {}\n").expect("b");
    }

    fn fid(module: &str, name: &str) -> FunctionId {
        FunctionId {
            module: module.to_string(),
            name: name.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Phase 1.2: Self-call is genuine recursion (self-loop edge preserved)
    // -----------------------------------------------------------------------

    #[test]
    fn self_call_filtered_out() {
        // Phase 1.2: self-loops are preserved in resolve_calls to detect genuine recursion.
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"self_call\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(dir.path().join("src/m.rs"), "pub fn a() { a(); }\n").expect("m");

        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");

        // Self-loop edge a→a is now preserved → detected as 1 cycle
        assert_eq!(
            analysis.cycle_count, 1,
            "self-recursion must be detected as a cycle"
        );
        assert_eq!(
            ir.edges,
            vec![(fid("m", "a"), fid("m", "a"))],
            "self-loop edge must appear in IR"
        );
    }

    /// The underlying analysis engine still detects self-loops when explicitly
    /// present in IR (e.g. from a future phase that re-enables them).
    #[test]
    fn ir_level_self_loop_still_detectable() {
        let ir = IR {
            functions: vec![fid("m", "a")],
            edges: vec![(fid("m", "a"), fid("m", "a"))],
        };
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(
            analysis.cycle_count, 1,
            "IR-level self-loop must still be detected"
        );
    }

    // -----------------------------------------------------------------------
    // Required Test 2: Mutual recursion
    // -----------------------------------------------------------------------

    #[test]
    fn mutual_recursion_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mutual_rec\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn a() { b(); }\npub fn b() { a(); }\n",
        )
        .expect("m");

        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");

        assert_eq!(
            analysis.cycle_count, 1,
            "expected 1 cycle (mutual recursion)"
        );
        assert_eq!(
            analysis.scc_sizes.iter().max().copied().unwrap_or(0),
            2,
            "max SCC size should be 2"
        );
        // Both functions should be in the cycle
        assert_eq!(analysis.cycles.len(), 1);
        let cycle = &analysis.cycles[0];
        assert!(cycle.contains(&fid("m", "a")));
        assert!(cycle.contains(&fid("m", "b")));
    }

    // -----------------------------------------------------------------------
    // Required Test 3: DAG (no cycles)
    // -----------------------------------------------------------------------

    #[test]
    fn dag_has_no_cycles() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"dag\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        // a calls b, b does not call a → DAG
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn a() { b(); }\npub fn b() {}\n",
        )
        .expect("m");

        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");

        assert_eq!(analysis.cycle_count, 0, "DAG should have zero cycles");
    }

    // -----------------------------------------------------------------------
    // Required Test 4: Determinism (hash matches across 3 runs)
    // -----------------------------------------------------------------------

    #[test]
    fn reproducibility_hash_matches_three_times() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let run = || {
            let extracted = extract_graph(dir.path()).expect("extract");
            ir_hash(&build_ir(&extracted))
        };
        let h1 = run();
        let h2 = run();
        let h3 = run();
        assert_eq!(h1, h2, "hash must be identical on run 1 vs 2");
        assert_eq!(h2, h3, "hash must be identical on run 2 vs 3");
    }

    // -----------------------------------------------------------------------
    // Additional regression tests
    // -----------------------------------------------------------------------

    #[test]
    fn extracted_graph_is_deterministic() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let lhs = extract_graph(dir.path()).expect("lhs");
        let rhs = extract_graph(dir.path()).expect("rhs");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn analyze_detects_intrafile_cycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");

        // foo ↔ bar in module a is a cycle of size 2
        assert_eq!(analysis.cycle_count, 1);
        let cycle = &analysis.cycles[0];
        assert!(cycle.contains(&fid("a", "foo")));
        assert!(cycle.contains(&fid("a", "bar")));
    }

    #[test]
    fn optimize_removes_cycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let optimization = optimize_ir(&ir).expect("optimize");
        assert_eq!(optimization.cuts.len(), 1);
        let next = apply_optimization(&ir, &optimization).expect("apply");
        let proof = prove_ir(&next).expect("prove");
        assert!(proof.valid);
        assert_eq!(proof.remaining_cycles, 0);
        assert!(proof.edge_consistency);
    }

    #[test]
    fn dag_optimize_has_zero_cuts() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"verify_cli_dag\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod a;\npub mod b;\n").expect("lib");
        // a.rs has one function with no calls
        fs::write(dir.path().join("src/a.rs"), "pub fn a() {}\n").expect("a");
        // b.rs has one function with no calls
        fs::write(dir.path().join("src/b.rs"), "pub fn b() {}\n").expect("b");

        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");
        let optimization = optimize_ir(&ir).expect("optimize");
        assert_eq!(analysis.cycle_count, 0);
        assert!(optimization.cuts.is_empty());
    }

    #[test]
    fn snapshot_replay_is_deterministic() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");
        let snapshot = Snapshot {
            id: format!("{:016x}", ir_hash(&ir)),
            ir: ir.clone(),
            analysis: analysis.clone(),
            optimization: None,
            metrics_stage: Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: analysis.cycle_count,
                    edge_count_before: ir.edges.len(),
                    max_scc_size: 0,
                    avg_scc_size: 0.0,
                },
                optimize: None,
                prove: Some(ProveMetrics {
                    cycles_after: analysis.cycle_count,
                    valid: false,
                    deterministic: true,
                    hash: ir_hash(&ir),
                }),
            }),
            hash: ir_hash(&ir),
        };
        let replay = analyze_ir(&snapshot.ir).expect("replay");
        assert_eq!(snapshot.hash, ir_hash(&snapshot.ir));
        assert_eq!(snapshot.analysis.cycle_count, replay.cycle_count);
    }

    #[test]
    fn normalized_cut_score_matches_expected() {
        assert_eq!(compute_normalized_cut_score(2, 4), 0.5);
        assert_eq!(compute_normalized_cut_score(1, 0), 0.0);
    }

    #[test]
    fn baseline_cut_is_deterministic() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");
        let lhs = baseline_cut(&ir, &analysis.cycles);
        let rhs = baseline_cut(&ir, &analysis.cycles);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn baseline_v2_is_deterministic() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let extracted = extract_graph(dir.path()).expect("extract");
        let ir = build_ir(&extracted);
        let analysis = analyze_ir(&ir).expect("analyze");
        let lhs = baseline_v2_cut(&ir, &analysis.cycles);
        let rhs = baseline_v2_cut(&ir, &analysis.cycles);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn optimality_ratio_matches_expected() {
        assert_eq!(compute_optimality_ratio(2, 4), 0.5);
        assert_eq!(compute_optimality_ratio(0, 0), 1.0);
    }

    #[test]
    fn scc_metrics_match_expected() {
        let sccs = vec![
            vec![fid("a", "x"), fid("a", "y"), fid("a", "z")],
            vec![fid("b", "p"), fid("b", "q")],
            vec![fid("c", "r"), fid("c", "s"), fid("c", "t"), fid("c", "u")],
        ];
        let (max, avg) = compute_scc_metrics(&sccs);
        assert_eq!(max, 4);
        assert_eq!(avg, 3.0);
    }

    #[test]
    fn baseline_v2_tie_break_selects_lex_smallest_edge() {
        let ir = IR {
            functions: vec![fid("m", "a"), fid("m", "b"), fid("m", "c")],
            edges: vec![
                (fid("m", "a"), fid("m", "b")),
                (fid("m", "a"), fid("m", "c")),
                (fid("m", "b"), fid("m", "a")),
                (fid("m", "c"), fid("m", "a")),
            ],
        };
        let sccs = vec![vec![fid("m", "a"), fid("m", "b"), fid("m", "c")]];
        let cuts = baseline_v2_cut(&ir, &sccs);
        assert_eq!(cuts, vec![(fid("m", "a"), fid("m", "b"))]);
    }

    #[test]
    fn compare_reports_improved() {
        let previous = Snapshot {
            id: "prev".to_string(),
            ir: IR::default(),
            analysis: AnalysisReport::default(),
            optimization: None,
            metrics_stage: Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: 3,
                    edge_count_before: 6,
                    max_scc_size: 0,
                    avg_scc_size: 0.0,
                },
                optimize: Some(OptimizeMetrics {
                    cuts: 3,
                    edge_count_after: 3,
                    cut_efficiency: 1.0,
                    reduction_ratio: 0.5,
                    normalized_cut_score: 0.5,
                    baseline_v1_cuts: 3,
                    baseline_v2_cuts: 3,
                    optimality_v1: 1.0,
                    optimality_v2: 1.0,
                    max_scc_size: 4,
                    avg_scc_size: 3.0,
                }),
                prove: Some(ProveMetrics {
                    cycles_after: 0,
                    valid: true,
                    deterministic: true,
                    hash: 1,
                }),
            }),
            hash: 1,
        };
        let current = Snapshot {
            id: "curr".to_string(),
            ir: IR::default(),
            analysis: AnalysisReport::default(),
            optimization: None,
            metrics_stage: Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: 3,
                    edge_count_before: 6,
                    max_scc_size: 0,
                    avg_scc_size: 0.0,
                },
                optimize: Some(OptimizeMetrics {
                    cuts: 2,
                    edge_count_after: 4,
                    cut_efficiency: 1.5,
                    reduction_ratio: 0.333333,
                    normalized_cut_score: 0.4,
                    baseline_v1_cuts: 3,
                    baseline_v2_cuts: 2,
                    optimality_v1: 0.666666,
                    optimality_v2: 1.0,
                    max_scc_size: 4,
                    avg_scc_size: 3.0,
                }),
                prove: Some(ProveMetrics {
                    cycles_after: 0,
                    valid: true,
                    deterministic: true,
                    hash: 2,
                }),
            }),
            hash: 2,
        };
        let report = compare_snapshots(&current, &previous);
        assert!(report.improved);
        assert!(!report.regressed);
    }

    #[test]
    fn compare_reports_regressed() {
        let previous = Snapshot {
            id: "prev".to_string(),
            ir: IR::default(),
            analysis: AnalysisReport::default(),
            optimization: None,
            metrics_stage: Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: 2,
                    edge_count_before: 5,
                    max_scc_size: 0,
                    avg_scc_size: 0.0,
                },
                optimize: Some(OptimizeMetrics {
                    cuts: 2,
                    edge_count_after: 3,
                    cut_efficiency: 1.0,
                    reduction_ratio: 0.4,
                    normalized_cut_score: 0.5,
                    baseline_v1_cuts: 2,
                    baseline_v2_cuts: 2,
                    optimality_v1: 1.0,
                    optimality_v2: 1.0,
                    max_scc_size: 3,
                    avg_scc_size: 2.5,
                }),
                prove: Some(ProveMetrics {
                    cycles_after: 0,
                    valid: true,
                    deterministic: true,
                    hash: 1,
                }),
            }),
            hash: 1,
        };
        let current = Snapshot {
            id: "curr".to_string(),
            ir: IR::default(),
            analysis: AnalysisReport::default(),
            optimization: None,
            metrics_stage: Some(MetricsStage {
                analyze: AnalyzeMetrics {
                    cycles_before: 2,
                    edge_count_before: 5,
                    max_scc_size: 0,
                    avg_scc_size: 0.0,
                },
                optimize: Some(OptimizeMetrics {
                    cuts: 3,
                    edge_count_after: 2,
                    cut_efficiency: 0.666666,
                    reduction_ratio: 0.6,
                    normalized_cut_score: 0.75,
                    baseline_v1_cuts: 2,
                    baseline_v2_cuts: 2,
                    optimality_v1: 1.5,
                    optimality_v2: 1.5,
                    max_scc_size: 3,
                    avg_scc_size: 2.5,
                }),
                prove: Some(ProveMetrics {
                    cycles_after: 1,
                    valid: false,
                    deterministic: true,
                    hash: 2,
                }),
            }),
            hash: 2,
        };
        let report = compare_snapshots(&current, &previous);
        assert!(report.regressed);
    }

    #[test]
    fn metrics_stage_is_separated() {
        let stage = MetricsStage {
            analyze: AnalyzeMetrics {
                cycles_before: 1,
                edge_count_before: 2,
                max_scc_size: 0,
                avg_scc_size: 0.0,
            },
            optimize: None,
            prove: None,
        };
        assert!(stage.optimize.is_none());
        assert!(stage.prove.is_none());
    }

    // -----------------------------------------------------------------------
    // Unit tests for extraction helpers
    // -----------------------------------------------------------------------

    #[test]
    fn collect_top_level_item_fns_handles_visibility_and_async() {
        let content = r#"
            fn foo() {}
            pub fn bar() {}
            async fn baz() {}
            pub async fn qux() {}
            pub(crate) fn priv_fn() {}
        "#;
        let names: Vec<_> = collect_top_level_item_fns(content)
            .into_iter()
            .map(|item| item.sig.ident.to_string())
            .collect();
        assert_eq!(names, vec!["foo", "bar", "baz", "qux", "priv_fn"]);
    }

    #[test]
    fn extract_call_name_detects_simple_calls() {
        let expr: ExprCall = syn::parse_str("foo()").expect("parse call");
        assert_eq!(extract_callee_path(&expr), Some(vec!["foo".to_string()]));

        let expr: ExprCall = syn::parse_str("crate::foo()").expect("parse qualified call");
        assert_eq!(
            extract_callee_path(&expr),
            Some(vec!["crate".to_string(), "foo".to_string()])
        );
    }

    #[test]
    fn extract_call_name_rejects_non_path_calls() {
        let expr: ExprCall = syn::parse_str("(factory())()").expect("parse nested call");
        assert_eq!(extract_callee_path(&expr), None);
    }

    #[test]
    fn collect_functions_extracts_all_fn_defs() {
        let content = "pub fn a() {}\nfn b() {}\nasync fn c() {}\n";
        let module = "m".to_string();
        let fns = collect_functions(content, &module);
        assert_eq!(fns.len(), 3);
        assert!(fns.contains(&fid("m", "a")));
        assert!(fns.contains(&fid("m", "b")));
        assert!(fns.contains(&fid("m", "c")));
    }

    #[test]
    fn collect_callsites_detects_mutual_recursion() {
        let module = "m".to_string();
        let fns = vec![fid("m", "a"), fid("m", "b")];
        let content = "pub fn a() { b(); }\npub fn b() { a(); }\n";
        let sites = collect_callsites(content, &fns);
        assert!(sites.contains(&CallSite {
            caller: fid(&module, "a"),
            callee_name: "b".to_string(),
            callee_path: Some(vec!["b".to_string()]),
        }));
        assert!(sites.contains(&CallSite {
            caller: fid(&module, "b"),
            callee_name: "a".to_string(),
            callee_path: Some(vec!["a".to_string()]),
        }));
    }

    #[test]
    fn collect_callsites_detects_self_recursion() {
        let module = "m".to_string();
        let fns = vec![fid("m", "a")];
        let content = "pub fn a() { a(); }\n";
        let sites = collect_callsites(content, &fns);
        assert!(sites.contains(&CallSite {
            caller: fid(&module, "a"),
            callee_name: "a".to_string(),
            callee_path: Some(vec!["a".to_string()]),
        }));
    }

    #[test]
    fn resolve_calls_matches_by_name() {
        let functions = vec![fid("m", "alpha"), fid("m", "beta")];
        let callsites = vec![CallSite {
            caller: fid("m", "alpha"),
            callee_name: "beta".to_string(),
            callee_path: Some(vec!["beta".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert_eq!(edges, vec![(fid("m", "alpha"), fid("m", "beta"))]);
    }

    #[test]
    fn resolve_calls_skips_unknown_callee() {
        let functions = vec![fid("m", "alpha")];
        let callsites = vec![CallSite {
            caller: fid("m", "alpha"),
            callee_name: "unknown".to_string(),
            callee_path: Some(vec!["unknown".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert!(edges.is_empty());
    }

    // -----------------------------------------------------------------------
    // Validation Checks (spec requirement §6)
    // -----------------------------------------------------------------------

    /// Check 1: cycles_before > 0 → max_scc_size ≥ 1
    #[test]
    fn check1_cycles_imply_nonzero_max_scc_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"c1\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn a() { b(); }\npub fn b() { a(); }\n",
        )
        .expect("m");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");

        assert!(analysis.cycle_count > 0, "precondition: must have cycles");
        let (max_scc, _) = compute_scc_metrics_from_sizes(&analysis.scc_sizes);
        assert!(
            max_scc >= 1,
            "max_scc_size must be ≥ 1 when cycles exist (got {max_scc})"
        );
    }

    /// Check 2: single-node without self-loop MUST NOT be a cycle
    #[test]
    fn check2_no_false_positive_single_node() {
        // Build a graph where function `a` has no self-loop
        let ir = IR {
            functions: vec![fid("m", "a")],
            edges: vec![],
        };
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(analysis.cycle_count, 0, "isolated node must not be a cycle");
    }

    /// Check 3: scc_sizes must be populated (non-empty when graph has functions)
    #[test]
    fn check3_scc_sizes_populated() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");
        assert!(
            !analysis.scc_sizes.is_empty(),
            "scc_sizes must not be empty"
        );
        assert_eq!(
            analysis.scc_sizes.iter().sum::<usize>(),
            ir.functions.len(),
            "scc_sizes must account for every function exactly once"
        );
    }

    /// Check 4: extract_cycles strict unit test
    #[test]
    fn extract_cycles_rejects_isolated_nodes() {
        let sccs = vec![vec![fid("m", "a")], vec![fid("m", "b")]];
        let edges: Vec<(FunctionId, FunctionId)> = vec![];
        let cycles = extract_cycles(&sccs, &edges);
        assert!(
            cycles.is_empty(),
            "isolated nodes must not appear as cycles"
        );
    }

    #[test]
    fn extract_cycles_accepts_self_loop() {
        let sccs = vec![vec![fid("m", "a")]];
        let edges = vec![(fid("m", "a"), fid("m", "a"))];
        let cycles = extract_cycles(&sccs, &edges);
        assert_eq!(cycles.len(), 1);
    }

    #[test]
    fn extract_cycles_accepts_mutual_recursion() {
        let sccs = vec![vec![fid("m", "a"), fid("m", "b")]];
        let edges = vec![
            (fid("m", "a"), fid("m", "b")),
            (fid("m", "b"), fid("m", "a")),
        ];
        let cycles = extract_cycles(&sccs, &edges);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 2);
    }

    // -----------------------------------------------------------------------
    // Phase 1.1 Required Tests
    // -----------------------------------------------------------------------

    /// Phase 1.1 Test 2: Mutual recursion between distinct functions is valid.
    #[test]
    fn phase11_mutual_recursion_valid() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"p11t2\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn alpha() { beta(); }\npub fn beta() { alpha(); }\n",
        )
        .expect("m");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");

        assert_eq!(
            analysis.cycle_count, 1,
            "mutual recursion must yield 1 cycle"
        );
        let (max_scc, _) = compute_scc_metrics_from_sizes(&analysis.scc_sizes);
        assert_eq!(max_scc, 2, "max_scc_size must be 2 for a 2-function cycle");
    }

    /// Phase 1.1 Test 3: Same-name functions in different modules must not
    /// produce cross-module edges (already enforced via per-file processing).
    #[test]
    fn phase11_same_name_different_module_no_cross_edge() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"p11t3\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod a;\npub mod b;\n").expect("lib");
        // Both modules define `setup` — no call between them
        fs::write(dir.path().join("src/a.rs"), "pub fn setup() {}\n").expect("a");
        fs::write(dir.path().join("src/b.rs"), "pub fn setup() {}\n").expect("b");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));

        // No edges should exist across modules
        for (from, to) in &ir.edges {
            assert_eq!(
                from.module, to.module,
                "cross-module edge must not exist: {from} -> {to}"
            );
        }
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(
            analysis.cycle_count, 0,
            "same-name different-module must produce no cycle"
        );
    }

    /// Phase 1.1 Test 4: Determinism — hash is identical across repeated runs.
    #[test]
    fn phase11_deterministic_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let hash =
            |path: &Path| -> u64 { ir_hash(&build_ir(&extract_graph(path).expect("extract"))) };
        let h1 = hash(dir.path());
        let h2 = hash(dir.path());
        let h3 = hash(dir.path());
        assert_eq!(h1, h2, "hash mismatch between run 1 and 2");
        assert_eq!(h2, h3, "hash mismatch between run 2 and 3");
    }

    // -----------------------------------------------------------------------
    // Phase 1.2 Required Tests
    // -----------------------------------------------------------------------

    /// Phase 1.2 Test 1: Self-recursion is detected (genuine recursive call).
    #[test]
    fn phase12_self_recursion_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"p12t1\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn recurse() { recurse(); }\n",
        )
        .expect("m");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(
            analysis.cycle_count, 1,
            "self-recursion must yield exactly 1 cycle"
        );
    }

    /// Phase 1.2 Test 2: Mutual recursion detected without any blocklist.
    #[test]
    fn phase12_mutual_recursion_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"p12t2\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn ping() { pong(); }\npub fn pong() { ping(); }\n",
        )
        .expect("m");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(
            analysis.cycle_count, 1,
            "mutual recursion must yield exactly 1 cycle"
        );
    }

    /// Phase 1.2 Test 3: Pure DAG produces zero cycles.
    #[test]
    fn phase12_dag_no_cycles() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"p12t3\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .expect("cargo");
        fs::write(dir.path().join("src/lib.rs"), "pub mod m;\n").expect("lib");
        fs::write(
            dir.path().join("src/m.rs"),
            "pub fn top() { mid(); }\npub fn mid() { leaf(); }\npub fn leaf() {}\n",
        )
        .expect("m");

        let ir = build_ir(&extract_graph(dir.path()).expect("extract"));
        let analysis = analyze_ir(&ir).expect("analyze");
        assert_eq!(analysis.cycle_count, 0, "DAG must produce zero cycles");
    }

    /// Phase 1.2 Test 4: resolve_calls allows self-loops (genuine recursion).
    #[test]
    fn phase12_resolve_calls_allows_self_loop() {
        let functions = vec![fid("m", "run")];
        let callsites = vec![CallSite {
            caller: fid("m", "run"),
            callee_name: "run".to_string(),
            callee_path: Some(vec!["run".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert_eq!(
            edges,
            vec![(fid("m", "run"), fid("m", "run"))],
            "self-loop must be preserved"
        );
    }

    /// Phase 1.2 Test 5: resolve_calls restricts to same module only.
    #[test]
    fn phase12_resolve_calls_same_module_only() {
        // mod_a::caller calls "helper" — only mod_a::helper should be resolved, not mod_b::helper.
        let functions = vec![
            fid("mod_a", "caller"),
            fid("mod_a", "helper"),
            fid("mod_b", "helper"),
        ];
        let callsites = vec![CallSite {
            caller: fid("mod_a", "caller"),
            callee_name: "helper".to_string(),
            callee_path: Some(vec!["helper".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert_eq!(
            edges,
            vec![(fid("mod_a", "caller"), fid("mod_a", "helper"))]
        );
    }

    /// Phase 1.2 Test 6: Determinism — IR hash is identical across repeated runs.
    #[test]
    fn phase12_deterministic_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let hash =
            |path: &Path| -> u64 { ir_hash(&build_ir(&extract_graph(path).expect("extract"))) };
        let h1 = hash(dir.path());
        let h2 = hash(dir.path());
        let h3 = hash(dir.path());
        assert_eq!(h1, h2, "hash mismatch between run 1 and 2");
        assert_eq!(h2, h3, "hash mismatch between run 2 and 3");
    }

    // -----------------------------------------------------------------------
    // Phase 1.3 Required Tests
    // -----------------------------------------------------------------------

    #[test]
    fn phase13_type_method_ignored() {
        let functions = vec![fid("m", "a"), fid("m", "new")];
        let callsites = vec![CallSite {
            caller: fid("m", "a"),
            callee_name: "new".to_string(),
            callee_path: Some(vec!["HashMap".to_string(), "new".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert!(edges.is_empty(), "Type::method call must be ignored");
    }

    #[test]
    fn phase13_crate_path_ignored() {
        let functions = vec![fid("m", "a"), fid("m", "foo")];
        let callsites = vec![CallSite {
            caller: fid("m", "a"),
            callee_name: "foo".to_string(),
            callee_path: Some(vec!["crate".to_string(), "foo".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert!(edges.is_empty(), "crate::function call must be ignored");
    }

    #[test]
    fn phase13_local_function_resolved() {
        let functions = vec![fid("m", "a"), fid("m", "b")];
        let callsites = vec![CallSite {
            caller: fid("m", "a"),
            callee_name: "b".to_string(),
            callee_path: Some(vec!["b".to_string()]),
        }];
        let edges = resolve_calls(&functions, &callsites);
        assert_eq!(edges, vec![(fid("m", "a"), fid("m", "b"))]);
    }

    #[test]
    fn phase13_collect_callsites_keeps_full_path() {
        let fns = vec![fid("m", "a"), fid("m", "b")];
        let content = "pub fn a() { HashMap::new(); crate::b(); b(); }\npub fn b() {}\n";
        let sites = collect_callsites(content, &fns);
        assert!(sites.contains(&CallSite {
            caller: fid("m", "a"),
            callee_name: "new".to_string(),
            callee_path: Some(vec!["HashMap".to_string(), "new".to_string()]),
        }));
        assert!(sites.contains(&CallSite {
            caller: fid("m", "a"),
            callee_name: "b".to_string(),
            callee_path: Some(vec!["crate".to_string(), "b".to_string()]),
        }));
        assert!(sites.contains(&CallSite {
            caller: fid("m", "a"),
            callee_name: "b".to_string(),
            callee_path: Some(vec!["b".to_string()]),
        }));
    }
}
