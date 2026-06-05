use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::visit::Visit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyzeCommand {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct RepositoryScanner;

impl RepositoryScanner {
    pub fn scan(&self, root: &Path) -> Result<RepositorySnapshot, String> {
        let root = resolve_repository_path(root);
        let roots = scan_roots(&root);
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut cargo_tomls = Vec::new();
        for scan_root in roots {
            if scan_root.exists() {
                collect_repository_entries(
                    &scan_root,
                    &mut files,
                    &mut directories,
                    &mut cargo_tomls,
                )?;
            }
        }
        let root_cargo = root.join("Cargo.toml");
        if root_cargo.exists() {
            cargo_tomls.push(root_cargo);
        }
        files.sort();
        files.dedup();
        directories.sort();
        directories.dedup();
        cargo_tomls.sort();
        cargo_tomls.dedup();
        let workspace_members = parse_workspace_members(&root.join("Cargo.toml"))?;
        Ok(RepositorySnapshot {
            root,
            directories,
            rust_files: files,
            cargo_tomls,
            workspace_members,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub root: PathBuf,
    pub directories: Vec<PathBuf>,
    pub rust_files: Vec<PathBuf>,
    pub cargo_tomls: Vec<PathBuf>,
    pub workspace_members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstModule {
    pub file_path: PathBuf,
    pub module_path: String,
    pub modules: Vec<String>,
    pub structs: Vec<String>,
    pub enums: Vec<String>,
    pub traits: Vec<String>,
    pub functions: Vec<String>,
    pub uses: Vec<String>,
    pub impls: Vec<TraitImplementation>,
    pub calls: Vec<FunctionCall>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitImplementation {
    pub trait_name: String,
    pub for_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub caller: String,
    pub callee: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureGraph {
    pub nodes: Vec<StructureNode>,
    pub edges: Vec<StructureEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureNode {
    pub id: String,
    pub label: String,
    pub kind: StructureNodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureNodeKind {
    File,
    Module,
    Struct,
    Enum,
    Trait,
    Function,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureEdge {
    pub from: String,
    pub to: String,
    pub kind: StructureEdgeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureEdgeKind {
    Contains,
    DependsOn,
    Implements,
    Uses,
    Calls,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub project_name: String,
    pub modules: usize,
    pub structs: usize,
    pub enums: usize,
    pub traits: usize,
    pub functions: usize,
    pub module_count: usize,
    pub struct_count: usize,
    pub enum_count: usize,
    pub trait_count: usize,
    pub function_count: usize,
    pub top_components: Vec<String>,
    pub top_modules: Vec<String>,
    pub dependencies: Vec<String>,
    pub circular_dependencies: usize,
    pub layer_violations: usize,
    pub boundary_violations: usize,
    pub god_objects: usize,
    pub architecture_health: f32,
    pub status: ArchitectureStatus,
    pub convergence_score: f32,
    pub direction: Direction,
    pub top_proposal: Option<String>,
    pub predicted_health_delta: f32,
    pub convergence_confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticStructure {
    pub components: Vec<SemanticComponent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticComponent {
    pub name: String,
    pub category: SemanticCategory,
    pub responsibility: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticCategory {
    Controller,
    Service,
    Repository,
    Engine,
    Memory,
    Policy,
    Runtime,
    UI,
    Storage,
    Module,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircularDependency {
    pub cycle_nodes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiddenDependency {
    pub source: String,
    pub via: String,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DependencyHotspot {
    pub component: String,
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
    pub calls: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub circular_dependencies: Vec<CircularDependency>,
    pub hidden_dependencies: Vec<HiddenDependency>,
    pub dependency_density: f32,
    pub hotspots: Vec<DependencyHotspot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GodObject {
    pub component: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MassiveModule {
    pub component: String,
    pub function_count: usize,
    pub dependency_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsibilityLeakage {
    pub source: String,
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsibilityAnalysis {
    pub god_objects: Vec<GodObject>,
    pub massive_modules: Vec<MassiveModule>,
    pub responsibility_leakage: Vec<ResponsibilityLeakage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureRule {
    pub source: String,
    pub target: String,
    pub allow: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerViolation {
    pub source: String,
    pub target: String,
    pub source_layer: SemanticCategory,
    pub target_layer: SemanticCategory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryViolation {
    pub source: String,
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitectureStatus {
    Stable,
    Healthy,
    Warning,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureHealth {
    pub score: f32,
    pub status: ArchitectureStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureAnalysis {
    pub rules: Vec<ArchitectureRule>,
    pub layer_violations: Vec<LayerViolation>,
    pub boundary_violations: Vec<BoundaryViolation>,
    pub health: ArchitectureHealth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentDrift {
    pub component: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDrift {
    pub design_source: Option<PathBuf>,
    pub intent_drifts: Vec<IntentDrift>,
    pub missing_design_reference: bool,
}

pub struct DependencyAnalyzer;
pub struct RefactoringPlanner;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationPlan {
    pub kind: String,
    pub target: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceProposal {
    pub title: String,
    pub reason: String,
    pub impact_score: f32,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefactoringProposal {
    pub proposal: ConvergenceProposal,
    pub mutation_plan: MutationPlan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub proposal_title: String,
    pub dependency_reduction: f32,
    pub violation_reduction: f32,
    pub health_improvement: f32,
    pub risk: f32,
    pub predicted_health: f32,
    pub design_drift_impact: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureSimulation {
    pub proposal_title: String,
    pub current_health: f32,
    pub predicted_health: f32,
    pub health_delta: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesignAlignmentStatus {
    Aligned,
    PartiallyAligned,
    Misaligned,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignAlignment {
    pub status: DesignAlignmentStatus,
    pub score: f32,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceState {
    Converging,
    Unstable,
    Diverging,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceScore {
    pub score: f32,
    pub state: ConvergenceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Improving,
    Stable,
    Degrading,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationImpactModel {
    pub proposal_title: String,
    pub architecture_risk: f32,
    pub dependency_impact: f32,
    pub design_drift_impact: f32,
    pub predicted_health_delta: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceReport {
    pub architecture_health: f32,
    pub convergence_score: ConvergenceScore,
    pub direction: Direction,
    pub top_proposal: Option<ConvergenceProposal>,
    pub predicted_health_delta: f32,
    pub confidence: f32,
    pub refactoring_proposals: Vec<RefactoringProposal>,
    pub impact_analysis: Vec<ImpactAnalysis>,
    pub architecture_simulations: Vec<ArchitectureSimulation>,
    pub design_alignment: DesignAlignment,
    pub mutation_impact_models: Vec<MutationImpactModel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeEngineOutput {
    pub root: PathBuf,
    pub repository: RepositorySnapshot,
    pub files: Vec<PathBuf>,
    pub ast_modules: Vec<AstModule>,
    pub graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
    pub dependency_analysis: DependencyAnalysis,
    pub responsibility_analysis: ResponsibilityAnalysis,
    pub architecture_analysis: ArchitectureAnalysis,
    pub design_drift: DesignDrift,
    pub convergence_report: ConvergenceReport,
    pub result: AnalyzeResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryEntry {
    pub id: String,
    pub kind: String,
    pub project_name: String,
    pub summary: AnalyzeResult,
    pub repository_snapshot: RepositorySnapshot,
    pub structure_graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
    pub dependency_analysis: DependencyAnalysis,
    pub responsibility_analysis: ResponsibilityAnalysis,
    pub architecture_analysis: ArchitectureAnalysis,
    pub design_drift: DesignDrift,
    pub convergence_report: ConvergenceReport,
}

pub fn execute(command: AnalyzeCommand) -> Result<AnalyzeEngineOutput, String> {
    let root = resolve_repository_path(&command.path);
    let scanner = RepositoryScanner;
    let repository = scanner.scan(&root)?;
    let files = repository.rust_files.clone();
    let ast_modules = extract_rust_ast_modules(&root, &files)?;
    let graph = build_structure_graph(&ast_modules);
    let semantic_structure = analyze_semantic_structure(&graph, &ast_modules);
    let dependency_analysis =
        DependencyAnalyzer::analyze(&graph, &semantic_structure, &ast_modules);
    let responsibility_analysis =
        analyze_responsibilities(&graph, &semantic_structure, &ast_modules);
    let architecture_analysis = analyze_architecture(
        &semantic_structure,
        &dependency_analysis,
        &responsibility_analysis,
    );
    let design_drift = analyze_design_drift(&root, &semantic_structure);
    let convergence_report = analyze_convergence(
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &design_drift,
    );
    let result = build_analyze_result(
        &root,
        &ast_modules,
        &graph,
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &convergence_report,
    );
    persist_to_holographic_memory(
        &root,
        &result,
        &repository,
        &graph,
        &semantic_structure,
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &design_drift,
        &convergence_report,
    )?;
    Ok(AnalyzeEngineOutput {
        root,
        repository,
        files,
        ast_modules,
        graph,
        semantic_structure,
        dependency_analysis,
        responsibility_analysis,
        architecture_analysis,
        design_drift,
        convergence_report,
        result,
    })
}

pub fn render_analyze_result(result: &AnalyzeResult) -> String {
    let mut out = String::new();
    out.push_str("=== Analyze Result ===\n\n");
    out.push_str("Project\n");
    out.push_str(&format!("{}\n\n", result.project_name));
    out.push_str("Modules\n");
    out.push_str(&format!("{}\n\n", result.module_count));
    out.push_str("Structs\n");
    out.push_str(&format!("{}\n\n", result.struct_count));
    out.push_str("Enums\n");
    out.push_str(&format!("{}\n\n", result.enum_count));
    out.push_str("Traits\n");
    out.push_str(&format!("{}\n\n", result.trait_count));
    out.push_str("Functions\n");
    out.push_str(&format!("{}\n\n", result.function_count));
    out.push_str("Circular Dependencies\n");
    out.push_str(&format!("{}\n\n", result.circular_dependencies));
    out.push_str("Layer Violations\n");
    out.push_str(&format!("{}\n\n", result.layer_violations));
    out.push_str("Boundary Violations\n");
    out.push_str(&format!("{}\n\n", result.boundary_violations));
    out.push_str("God Objects\n");
    out.push_str(&format!("{}\n\n", result.god_objects));
    out.push_str("Architecture Health\n");
    out.push_str(&format!("{:.1}\n\n", result.architecture_health));
    out.push_str("Status\n");
    out.push_str(&format!("{:?}\n\n", result.status));
    out.push_str("Top Components\n");
    if result.top_modules.is_empty() {
        out.push_str("(none)\n");
    } else {
        for module in &result.top_modules {
            out.push_str(module);
            out.push('\n');
        }
    }
    out.push_str("\n=== Convergence Analysis ===\n\n");
    out.push_str("Architecture Health\n");
    out.push_str(&format!("{:.1}\n\n", result.architecture_health));
    out.push_str("Convergence Score\n");
    out.push_str(&format!("{:.1}\n\n", result.convergence_score));
    out.push_str("Direction\n");
    out.push_str(&format!("{:?}\n\n", result.direction));
    out.push_str("Top Proposal\n");
    out.push_str(
        result
            .top_proposal
            .as_deref()
            .unwrap_or("No convergence proposal"),
    );
    out.push_str("\n\nPredicted Health\n");
    out.push_str(&format!("{:+.1}\n\n", result.predicted_health_delta));
    out.push_str("Confidence\n");
    out.push_str(&format!("{:.2}\n", result.convergence_confidence));
    out
}

fn extract_rust_ast_modules(root: &Path, files: &[PathBuf]) -> Result<Vec<AstModule>, String> {
    let mut modules = Vec::new();
    for file in files {
        let source = fs::read_to_string(file)
            .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
        let parsed = syn::parse_file(&source)
            .map_err(|err| format!("failed to parse {}: {err}", file.display()))?;
        let rel = file.strip_prefix(root).unwrap_or(file).to_path_buf();
        let module_path = module_path_from_relative_path(&rel);
        let mut visitor = RustAstVisitor::default();
        visitor.visit_file(&parsed);
        modules.push(AstModule {
            file_path: rel,
            module_path,
            modules: visitor.modules,
            structs: visitor.structs,
            enums: visitor.enums,
            traits: visitor.traits,
            functions: visitor.functions,
            uses: visitor.uses,
            impls: visitor.impls,
            calls: visitor.calls,
        });
    }
    Ok(modules)
}

#[derive(Default)]
struct RustAstVisitor {
    modules: Vec<String>,
    structs: Vec<String>,
    enums: Vec<String>,
    traits: Vec<String>,
    functions: Vec<String>,
    uses: Vec<String>,
    impls: Vec<TraitImplementation>,
    calls: Vec<FunctionCall>,
    current_function: Option<String>,
}

impl<'ast> Visit<'ast> for RustAstVisitor {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.modules.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.structs.push(node.ident.to_string());
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.enums.push(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.traits.push(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let previous = self.current_function.replace(node.sig.ident.to_string());
        self.functions.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.current_function = previous;
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        collect_use_tree(&node.tree, String::new(), &mut self.uses);
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let Some((_, path, _)) = &node.trait_ {
            self.impls.push(TraitImplementation {
                trait_name: path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                    .unwrap_or_else(|| path_to_string(path)),
                for_type: type_to_string(&node.self_ty),
            });
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Some(caller) = &self.current_function {
            self.calls.push(FunctionCall {
                caller: caller.clone(),
                callee: expr_to_call_name(&node.func),
            });
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let Some(caller) = &self.current_function {
            self.calls.push(FunctionCall {
                caller: caller.clone(),
                callee: node.method.to_string(),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn collect_use_tree(tree: &syn::UseTree, prefix: String, output: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(path) => {
            let next = append_path_segment(&prefix, &path.ident.to_string());
            collect_use_tree(&path.tree, next, output);
        }
        syn::UseTree::Name(name) => {
            output.push(append_path_segment(&prefix, &name.ident.to_string()))
        }
        syn::UseTree::Rename(rename) => {
            output.push(append_path_segment(&prefix, &rename.ident.to_string()))
        }
        syn::UseTree::Glob(_) => output.push(append_path_segment(&prefix, "*")),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix.clone(), output);
            }
        }
    }
}

fn build_structure_graph(ast_modules: &[AstModule]) -> StructureGraph {
    let mut nodes = BTreeMap::<String, StructureNode>::new();
    let mut edges = BTreeSet::<(String, String, String)>::new();
    for module in ast_modules {
        let file_id = format!("file:{}", module.file_path.display());
        let module_id = format!("module:{}", module.module_path);
        insert_node(
            &mut nodes,
            file_id.clone(),
            module.file_path.display().to_string(),
            StructureNodeKind::File,
        );
        insert_node(
            &mut nodes,
            module_id.clone(),
            module.module_path.clone(),
            StructureNodeKind::Module,
        );
        edges.insert((file_id.clone(), module_id.clone(), "contains".to_string()));

        for name in &module.modules {
            let id = format!("module:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Module,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.structs {
            let id = format!("struct:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Struct,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.enums {
            let id = format!("enum:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Enum,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.traits {
            let id = format!("trait:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Trait,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.functions {
            let id = format!("fn:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Function,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for dependency in &module.uses {
            let dep_id = format!("dep:{dependency}");
            insert_node(
                &mut nodes,
                dep_id.clone(),
                dependency.clone(),
                StructureNodeKind::Module,
            );
            edges.insert((module_id.clone(), dep_id, "uses".to_string()));
        }
        for call in &module.calls {
            let caller_id = format!("fn:{}::{}", module.module_path, call.caller);
            let callee_id = format!("call:{}", call.callee);
            insert_node(
                &mut nodes,
                caller_id.clone(),
                call.caller.clone(),
                StructureNodeKind::Function,
            );
            insert_node(
                &mut nodes,
                callee_id.clone(),
                call.callee.clone(),
                StructureNodeKind::Function,
            );
            edges.insert((caller_id, callee_id, "calls".to_string()));
        }
        for implementation in &module.impls {
            let type_id = format!("struct:{}::{}", module.module_path, implementation.for_type);
            let trait_id = format!("trait:{}", implementation.trait_name);
            insert_node(
                &mut nodes,
                type_id.clone(),
                implementation.for_type.clone(),
                StructureNodeKind::Struct,
            );
            insert_node(
                &mut nodes,
                trait_id.clone(),
                implementation.trait_name.clone(),
                StructureNodeKind::Trait,
            );
            edges.insert((type_id, trait_id, "implements".to_string()));
        }
    }

    StructureGraph {
        nodes: nodes.into_values().collect(),
        edges: edges
            .into_iter()
            .map(|(from, to, kind)| StructureEdge {
                from,
                to,
                kind: match kind.as_str() {
                    "contains" => StructureEdgeKind::Contains,
                    "implements" => StructureEdgeKind::Implements,
                    "uses" => StructureEdgeKind::Uses,
                    "calls" => StructureEdgeKind::Calls,
                    _ => StructureEdgeKind::DependsOn,
                },
            })
            .collect(),
    }
}

fn analyze_semantic_structure(graph: &StructureGraph, modules: &[AstModule]) -> SemanticStructure {
    let module_lookup = modules
        .iter()
        .map(|module| (module.module_path.as_str(), module))
        .collect::<BTreeMap<_, _>>();
    let mut components = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::Module)
        .map(|node| {
            let module = module_lookup.get(node.label.as_str()).copied();
            let evidence = semantic_evidence(&node.label, node, module);
            SemanticComponent {
                name: node.label.clone(),
                category: classify_semantic_category(&node.label, node, module),
                responsibility: infer_responsibility(&node.label, node, module),
                evidence,
            }
        })
        .collect::<Vec<_>>();
    components.sort_by(|left, right| left.name.cmp(&right.name));
    components.dedup_by(|left, right| left.name == right.name);
    SemanticStructure { components }
}

impl DependencyAnalyzer {
    pub fn analyze(
        graph: &StructureGraph,
        semantic: &SemanticStructure,
        modules: &[AstModule],
    ) -> DependencyAnalysis {
        let module_edges = module_dependency_edges(modules);
        let circular_dependencies = detect_circular_dependencies(&module_edges);
        let hidden_dependencies = detect_hidden_dependencies(&module_edges, semantic);
        let dependency_density = if graph.nodes.is_empty() {
            0.0
        } else {
            graph.edges.len() as f32 / graph.nodes.len() as f32
        };
        let hotspots = detect_dependency_hotspots(graph, modules);
        DependencyAnalysis {
            circular_dependencies,
            hidden_dependencies,
            dependency_density,
            hotspots,
        }
    }
}

fn analyze_responsibilities(
    graph: &StructureGraph,
    semantic: &SemanticStructure,
    modules: &[AstModule],
) -> ResponsibilityAnalysis {
    let mut incoming = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, usize>::new();
    for edge in &graph.edges {
        *incoming.entry(edge.to.clone()).or_default() += 1;
        *outgoing.entry(edge.from.clone()).or_default() += 1;
    }

    let mut god_objects = Vec::new();
    let mut massive_modules = Vec::new();
    for module in modules {
        let module_id = format!("module:{}", module.module_path);
        let in_count = incoming.get(&module_id).copied().unwrap_or(0);
        let dependency_count = module.uses.len();
        if module.functions.len() > 100 || in_count > 80 {
            god_objects.push(GodObject {
                component: module.module_path.clone(),
                score: (module.functions.len() as f32 / 100.0) + (in_count as f32 / 80.0),
            });
        }
        if module.functions.len() > 80 || dependency_count > 40 {
            massive_modules.push(MassiveModule {
                component: module.module_path.clone(),
                function_count: module.functions.len(),
                dependency_count,
            });
        }
    }

    let component_categories = semantic_category_map(semantic);
    let mut responsibility_leakage = Vec::new();
    for edge in &graph.edges {
        if !matches!(
            edge.kind,
            StructureEdgeKind::Uses | StructureEdgeKind::DependsOn
        ) {
            continue;
        }
        let source = component_name_from_node_id(&edge.from);
        let target = component_name_from_node_id(&edge.to);
        let source_category = component_categories.get(&source);
        let target_category = component_categories.get(&target);
        if matches!(source_category, Some(SemanticCategory::UI))
            && matches!(target_category, Some(SemanticCategory::Storage))
        {
            responsibility_leakage.push(ResponsibilityLeakage {
                source,
                target,
                reason: "UI accesses Storage directly".to_string(),
            });
        } else if matches!(source_category, Some(SemanticCategory::Policy))
            && matches!(target_category, Some(SemanticCategory::Repository))
        {
            responsibility_leakage.push(ResponsibilityLeakage {
                source,
                target,
                reason: "Policy writes through Repository boundary".to_string(),
            });
        }
    }
    responsibility_leakage.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
    });
    responsibility_leakage
        .dedup_by(|left, right| left.source == right.source && left.target == right.target);

    ResponsibilityAnalysis {
        god_objects,
        massive_modules,
        responsibility_leakage,
    }
}

fn analyze_architecture(
    semantic: &SemanticStructure,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
) -> ArchitectureAnalysis {
    let rules = default_architecture_rules();
    let mut layer_violations = Vec::new();
    for hidden in &dependency.hidden_dependencies {
        let source_layer = semantic_category_for(semantic, &hidden.source);
        let target_layer = semantic_category_for(semantic, &hidden.target);
        if !architecture_allows(&rules, &source_layer, &target_layer) {
            layer_violations.push(LayerViolation {
                source: hidden.source.clone(),
                target: hidden.target.clone(),
                source_layer,
                target_layer,
            });
        }
    }
    for leakage in &responsibility.responsibility_leakage {
        layer_violations.push(LayerViolation {
            source: leakage.source.clone(),
            target: leakage.target.clone(),
            source_layer: semantic_category_for(semantic, &leakage.source),
            target_layer: semantic_category_for(semantic, &leakage.target),
        });
    }
    layer_violations.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
    });
    layer_violations
        .dedup_by(|left, right| left.source == right.source && left.target == right.target);

    let boundary_violations = dependency
        .hotspots
        .iter()
        .filter(|hotspot| hotspot.incoming_edges + hotspot.outgoing_edges > 300)
        .map(|hotspot| BoundaryViolation {
            source: hotspot.component.clone(),
            target: "module-boundary".to_string(),
            reason: "high cross-boundary dependency concentration".to_string(),
        })
        .collect::<Vec<_>>();

    let health = compute_architecture_health(
        dependency.circular_dependencies.len(),
        layer_violations.len(),
        boundary_violations.len(),
        responsibility.god_objects.len(),
        dependency.dependency_density,
    );
    ArchitectureAnalysis {
        rules,
        layer_violations,
        boundary_violations,
        health,
    }
}

fn analyze_design_drift(root: &Path, semantic: &SemanticStructure) -> DesignDrift {
    let design_path = root.join("design.md");
    if !design_path.exists() {
        return DesignDrift {
            design_source: None,
            intent_drifts: Vec::new(),
            missing_design_reference: true,
        };
    }
    let design = fs::read_to_string(&design_path)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut intent_drifts = Vec::new();
    for component in &semantic.components {
        if design.contains("memory")
            && matches!(component.category, SemanticCategory::Memory)
            && component.responsibility.contains("Runtime")
        {
            intent_drifts.push(IntentDrift {
                component: component.name.clone(),
                expected: "Memory Layer".to_string(),
                actual: component.responsibility.clone(),
            });
        }
        if design.contains("ui")
            && component.name.to_ascii_lowercase().contains("storage")
            && matches!(component.category, SemanticCategory::UI)
        {
            intent_drifts.push(IntentDrift {
                component: component.name.clone(),
                expected: "Storage boundary".to_string(),
                actual: "UI responsibility".to_string(),
            });
        }
    }
    DesignDrift {
        design_source: Some(design_path),
        intent_drifts,
        missing_design_reference: false,
    }
}

fn analyze_convergence(
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
) -> ConvergenceReport {
    let refactoring_proposals = RefactoringPlanner::plan(dependency, responsibility, architecture);
    let design_alignment = match_design_alignment(design_drift);
    let impact_analysis = analyze_impacts(
        &refactoring_proposals,
        architecture.health.score,
        dependency,
        responsibility,
        architecture,
        design_drift,
    );
    let architecture_simulations = simulate_architecture(
        &refactoring_proposals,
        architecture.health.score,
        &impact_analysis,
    );
    let mutation_impact_models = model_mutation_impacts(&impact_analysis);
    let top_proposal = refactoring_proposals
        .first()
        .map(|proposal| proposal.proposal.clone());
    let predicted_health_delta = architecture_simulations
        .first()
        .map(|simulation| simulation.health_delta)
        .unwrap_or(0.0);
    let confidence = top_proposal
        .as_ref()
        .map(|proposal| proposal.confidence)
        .unwrap_or(0.70);
    let convergence_score = compute_convergence_score(
        architecture.health.score,
        dependency,
        responsibility,
        &design_alignment,
    );
    let direction = infer_direction(predicted_health_delta, convergence_score.score);

    ConvergenceReport {
        architecture_health: architecture.health.score,
        convergence_score,
        direction,
        top_proposal,
        predicted_health_delta,
        confidence,
        refactoring_proposals,
        impact_analysis,
        architecture_simulations,
        design_alignment,
        mutation_impact_models,
    }
}

impl RefactoringPlanner {
    pub fn plan(
        dependency: &DependencyAnalysis,
        responsibility: &ResponsibilityAnalysis,
        architecture: &ArchitectureAnalysis,
    ) -> Vec<RefactoringProposal> {
        let mut proposals = Vec::new();
        for god_object in &responsibility.god_objects {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!("Split {}", god_object.component),
                    reason: format!("god object score {:.2}", god_object.score),
                    impact_score: (0.70 + god_object.score / 5.0).min(0.98),
                    confidence: (0.82 + god_object.score / 20.0).min(0.96),
                },
                mutation_plan: MutationPlan {
                    kind: "ModuleSplit".to_string(),
                    target: god_object.component.clone(),
                    actions: vec![
                        format!("Extract {}Execution", pascal_tail(&god_object.component)),
                        format!("Extract {}Policy", pascal_tail(&god_object.component)),
                        format!("Extract {}Memory", pascal_tail(&god_object.component)),
                    ],
                },
            });
        }
        for cycle in &dependency.circular_dependencies {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!("Break dependency cycle {}", cycle.cycle_nodes.join(" -> ")),
                    reason: "circular dependency detected".to_string(),
                    impact_score: 0.86,
                    confidence: 0.88,
                },
                mutation_plan: MutationPlan {
                    kind: "DependencyInversion".to_string(),
                    target: cycle.cycle_nodes.join(" -> "),
                    actions: vec![
                        "Introduce boundary trait".to_string(),
                        "Invert dependency".to_string(),
                    ],
                },
            });
        }
        for violation in &architecture.layer_violations {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!(
                        "Insert boundary between {} and {}",
                        violation.source, violation.target
                    ),
                    reason: format!(
                        "{:?} directly reaches {:?}",
                        violation.source_layer, violation.target_layer
                    ),
                    impact_score: 0.74,
                    confidence: 0.84,
                },
                mutation_plan: MutationPlan {
                    kind: "BoundaryAdapter".to_string(),
                    target: format!("{} -> {}", violation.source, violation.target),
                    actions: vec![
                        "Introduce interface boundary".to_string(),
                        "Route dependency through service layer".to_string(),
                    ],
                },
            });
        }
        proposals.sort_by(|left, right| {
            right
                .proposal
                .impact_score
                .total_cmp(&left.proposal.impact_score)
                .then(left.proposal.title.cmp(&right.proposal.title))
        });
        proposals.truncate(20);
        proposals
    }
}

fn analyze_impacts(
    proposals: &[RefactoringProposal],
    current_health: f32,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
) -> Vec<ImpactAnalysis> {
    proposals
        .iter()
        .map(|proposal| {
            let dependency_reduction = match proposal.mutation_plan.kind.as_str() {
                "DependencyInversion" => 0.35,
                "BoundaryAdapter" => 0.20,
                "ModuleSplit" => 0.12,
                _ => 0.08,
            };
            let violation_reduction = if proposal.mutation_plan.kind == "BoundaryAdapter" {
                0.40
            } else if proposal.mutation_plan.kind == "DependencyInversion" {
                0.25
            } else {
                0.10
            };
            let health_improvement =
                predicted_health_improvement(proposal, dependency, responsibility, architecture);
            let risk = mutation_risk(proposal, design_drift);
            ImpactAnalysis {
                proposal_title: proposal.proposal.title.clone(),
                dependency_reduction,
                violation_reduction,
                health_improvement,
                risk,
                predicted_health: (current_health + health_improvement).clamp(0.0, 100.0),
                design_drift_impact: design_drift_impact(proposal, design_drift),
            }
        })
        .collect()
}

fn simulate_architecture(
    proposals: &[RefactoringProposal],
    current_health: f32,
    impacts: &[ImpactAnalysis],
) -> Vec<ArchitectureSimulation> {
    proposals
        .iter()
        .zip(impacts.iter())
        .map(|(proposal, impact)| ArchitectureSimulation {
            proposal_title: proposal.proposal.title.clone(),
            current_health,
            predicted_health: impact.predicted_health,
            health_delta: impact.predicted_health - current_health,
        })
        .collect()
}

fn match_design_alignment(design_drift: &DesignDrift) -> DesignAlignment {
    if design_drift.missing_design_reference {
        return DesignAlignment {
            status: DesignAlignmentStatus::PartiallyAligned,
            score: 65.0,
            evidence: vec!["design.md not found".to_string()],
        };
    }
    let drift_count = design_drift.intent_drifts.len();
    let score = (100.0 - drift_count as f32 * 12.0).clamp(0.0, 100.0);
    let status = if score >= 85.0 {
        DesignAlignmentStatus::Aligned
    } else if score >= 55.0 {
        DesignAlignmentStatus::PartiallyAligned
    } else {
        DesignAlignmentStatus::Misaligned
    };
    DesignAlignment {
        status,
        score,
        evidence: if drift_count == 0 {
            vec!["no intent drift detected".to_string()]
        } else {
            design_drift
                .intent_drifts
                .iter()
                .map(|drift| {
                    format!(
                        "{}: {} -> {}",
                        drift.component, drift.expected, drift.actual
                    )
                })
                .collect()
        },
    }
}

fn model_mutation_impacts(impacts: &[ImpactAnalysis]) -> Vec<MutationImpactModel> {
    impacts
        .iter()
        .map(|impact| MutationImpactModel {
            proposal_title: impact.proposal_title.clone(),
            architecture_risk: impact.risk,
            dependency_impact: impact.dependency_reduction,
            design_drift_impact: impact.design_drift_impact,
            predicted_health_delta: impact.health_improvement,
        })
        .collect()
}

fn compute_convergence_score(
    architecture_health: f32,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    design_alignment: &DesignAlignment,
) -> ConvergenceScore {
    let dependency_stability = (100.0
        - dependency.circular_dependencies.len() as f32 * 8.0
        - dependency.dependency_density * 6.0)
        .clamp(0.0, 100.0);
    let responsibility_quality = (100.0
        - responsibility.god_objects.len() as f32 * 9.0
        - responsibility.massive_modules.len() as f32 * 4.0)
        .clamp(0.0, 100.0);
    let score = (architecture_health * 0.35
        + dependency_stability * 0.25
        + responsibility_quality * 0.25
        + design_alignment.score * 0.15)
        .clamp(0.0, 100.0);
    let state = if score >= 80.0 {
        ConvergenceState::Converging
    } else if score >= 50.0 {
        ConvergenceState::Unstable
    } else {
        ConvergenceState::Diverging
    };
    ConvergenceScore { score, state }
}

fn infer_direction(predicted_health_delta: f32, convergence_score: f32) -> Direction {
    if predicted_health_delta > 2.0 {
        Direction::Improving
    } else if predicted_health_delta < -1.0 || convergence_score < 25.0 {
        Direction::Degrading
    } else {
        Direction::Stable
    }
}

fn predicted_health_improvement(
    proposal: &RefactoringProposal,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
) -> f32 {
    let base = match proposal.mutation_plan.kind.as_str() {
        "ModuleSplit" => 7.0 + responsibility.god_objects.len() as f32 * 0.4,
        "DependencyInversion" => 6.0 + dependency.circular_dependencies.len() as f32 * 0.8,
        "BoundaryAdapter" => 4.5 + architecture.layer_violations.len() as f32 * 0.5,
        _ => 2.0,
    };
    base.min(18.0)
}

fn mutation_risk(proposal: &RefactoringProposal, design_drift: &DesignDrift) -> f32 {
    let base = match proposal.mutation_plan.kind.as_str() {
        "ModuleSplit" => 0.28,
        "DependencyInversion" => 0.22,
        "BoundaryAdapter" => 0.16,
        _ => 0.20,
    };
    (base + design_drift.intent_drifts.len() as f32 * 0.03).min(0.95)
}

fn design_drift_impact(proposal: &RefactoringProposal, design_drift: &DesignDrift) -> f32 {
    if design_drift.missing_design_reference {
        return -1.0;
    }
    if proposal.mutation_plan.kind == "BoundaryAdapter" {
        1.5
    } else {
        -(design_drift.intent_drifts.len() as f32 * 0.2)
    }
}

fn pascal_tail(value: &str) -> String {
    value
        .split("::")
        .last()
        .unwrap_or(value)
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn module_dependency_edges(modules: &[AstModule]) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    for module in modules {
        let source = module.module_path.clone();
        for dependency in &module.uses {
            let target = normalize_dependency_name(dependency);
            if !target.is_empty() && target != source {
                edges.entry(source.clone()).or_default().insert(target);
            }
        }
        for declared_module in &module.modules {
            edges
                .entry(source.clone())
                .or_default()
                .insert(declared_module.clone());
        }
    }
    edges
}

fn detect_circular_dependencies(
    edges: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<CircularDependency> {
    let mut cycles = BTreeSet::<Vec<String>>::new();
    for start in edges.keys() {
        let mut path = Vec::new();
        detect_cycles_from(start, start, edges, &mut path, &mut cycles);
    }
    cycles
        .into_iter()
        .map(|cycle_nodes| CircularDependency { cycle_nodes })
        .collect()
}

fn detect_cycles_from(
    start: &str,
    current: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    path: &mut Vec<String>,
    cycles: &mut BTreeSet<Vec<String>>,
) {
    if path.len() > 12 {
        return;
    }
    path.push(current.to_string());
    if let Some(next_nodes) = edges.get(current) {
        for next in next_nodes {
            if next == start && path.len() > 1 {
                let mut cycle = path.clone();
                canonicalize_cycle(&mut cycle);
                cycles.insert(cycle);
            } else if !path.contains(next) {
                detect_cycles_from(start, next, edges, path, cycles);
            }
        }
    }
    path.pop();
}

fn canonicalize_cycle(cycle: &mut [String]) {
    if cycle.is_empty() {
        return;
    }
    let min_index = cycle
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.cmp(right.1))
        .map(|(index, _)| index)
        .unwrap_or(0);
    cycle.rotate_left(min_index);
}

fn detect_hidden_dependencies(
    edges: &BTreeMap<String, BTreeSet<String>>,
    semantic: &SemanticStructure,
) -> Vec<HiddenDependency> {
    let categories = semantic_category_map(semantic);
    let mut hidden = BTreeSet::<(String, String, String)>::new();
    for (source, targets) in edges {
        for via in targets {
            if !matches!(
                categories.get(via),
                Some(SemanticCategory::Runtime | SemanticCategory::Engine)
            ) {
                continue;
            }
            if let Some(second_targets) = edges.get(via) {
                for target in second_targets {
                    if target != source {
                        hidden.insert((source.clone(), via.clone(), target.clone()));
                    }
                }
            }
        }
    }
    hidden
        .into_iter()
        .map(|(source, via, target)| HiddenDependency {
            source,
            via,
            target,
        })
        .collect()
}

fn detect_dependency_hotspots(
    graph: &StructureGraph,
    modules: &[AstModule],
) -> Vec<DependencyHotspot> {
    let mut incoming = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, usize>::new();
    let mut calls = BTreeMap::<String, usize>::new();
    for edge in &graph.edges {
        *incoming.entry(edge.to.clone()).or_default() += 1;
        *outgoing.entry(edge.from.clone()).or_default() += 1;
        if edge.kind == StructureEdgeKind::Calls {
            *calls.entry(edge.from.clone()).or_default() += 1;
        }
    }
    let mut hotspots = modules
        .iter()
        .filter_map(|module| {
            let module_id = format!("module:{}", module.module_path);
            let incoming_edges = incoming.get(&module_id).copied().unwrap_or(0);
            let outgoing_edges = outgoing.get(&module_id).copied().unwrap_or(0);
            let call_count = module.calls.len()
                + module
                    .functions
                    .iter()
                    .map(|function| {
                        calls
                            .get(&format!("fn:{}::{function}", module.module_path))
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum::<usize>();
            let score = incoming_edges + outgoing_edges + call_count;
            (score > 50).then(|| DependencyHotspot {
                component: module.module_path.clone(),
                incoming_edges,
                outgoing_edges,
                calls: call_count,
            })
        })
        .collect::<Vec<_>>();
    hotspots.sort_by(|left, right| {
        let left_score = left.incoming_edges + left.outgoing_edges + left.calls;
        let right_score = right.incoming_edges + right.outgoing_edges + right.calls;
        right_score
            .cmp(&left_score)
            .then(left.component.cmp(&right.component))
    });
    hotspots.truncate(20);
    hotspots
}

fn default_architecture_rules() -> Vec<ArchitectureRule> {
    vec![
        ArchitectureRule {
            source: "UI".to_string(),
            target: "Storage".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "Controller".to_string(),
            target: "Storage".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "Storage".to_string(),
            target: "UI".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "UI".to_string(),
            target: "Engine".to_string(),
            allow: true,
        },
    ]
}

fn architecture_allows(
    rules: &[ArchitectureRule],
    source: &SemanticCategory,
    target: &SemanticCategory,
) -> bool {
    let source = semantic_category_name(source);
    let target = semantic_category_name(target);
    rules
        .iter()
        .find(|rule| rule.source == source && rule.target == target)
        .map(|rule| rule.allow)
        .unwrap_or(true)
}

fn compute_architecture_health(
    circular_dependencies: usize,
    layer_violations: usize,
    boundary_violations: usize,
    god_objects: usize,
    dependency_density: f32,
) -> ArchitectureHealth {
    let density_penalty = (dependency_density * 3.0).min(20.0);
    let mut score = 100.0
        - circular_dependencies as f32 * 3.0
        - layer_violations as f32 * 5.0
        - boundary_violations as f32 * 4.0
        - god_objects as f32 * 4.0
        - density_penalty;
    score = score.clamp(0.0, 100.0);
    let status = if circular_dependencies == 0
        && layer_violations == 0
        && boundary_violations == 0
        && god_objects == 0
        && score >= 90.0
    {
        ArchitectureStatus::Stable
    } else if score >= 80.0 {
        ArchitectureStatus::Healthy
    } else if score >= 50.0 {
        ArchitectureStatus::Warning
    } else {
        ArchitectureStatus::Critical
    };
    ArchitectureHealth { score, status }
}

fn semantic_category_map(semantic: &SemanticStructure) -> BTreeMap<String, SemanticCategory> {
    semantic
        .components
        .iter()
        .map(|component| (component.name.clone(), component.category.clone()))
        .collect()
}

fn semantic_category_for(semantic: &SemanticStructure, component: &str) -> SemanticCategory {
    semantic
        .components
        .iter()
        .find(|entry| entry.name == component)
        .map(|entry| entry.category.clone())
        .unwrap_or(SemanticCategory::Module)
}

fn semantic_category_name(category: &SemanticCategory) -> &'static str {
    match category {
        SemanticCategory::Controller => "Controller",
        SemanticCategory::Service => "Service",
        SemanticCategory::Repository => "Repository",
        SemanticCategory::Engine => "Engine",
        SemanticCategory::Memory => "Memory",
        SemanticCategory::Policy => "Policy",
        SemanticCategory::Runtime => "Runtime",
        SemanticCategory::UI => "UI",
        SemanticCategory::Storage => "Storage",
        SemanticCategory::Module => "Module",
    }
}

fn component_name_from_node_id(node_id: &str) -> String {
    node_id
        .split_once(':')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| node_id.to_string())
}

fn normalize_dependency_name(dependency: &str) -> String {
    let without_prefix = dependency
        .trim_start_matches("crate::")
        .trim_start_matches("super::")
        .trim_start_matches("self::");
    without_prefix
        .split("::")
        .next()
        .unwrap_or(without_prefix)
        .to_string()
}

fn build_analyze_result(
    root: &Path,
    modules: &[AstModule],
    graph: &StructureGraph,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    convergence: &ConvergenceReport,
) -> AnalyzeResult {
    let mut top_counts = BTreeMap::<String, usize>::new();
    for module in modules {
        let top = module
            .module_path
            .split("::")
            .next()
            .unwrap_or("root")
            .to_string();
        *top_counts.entry(top).or_default() += module.structs.len()
            + module.enums.len()
            + module.traits.len()
            + module.functions.len()
            + module.modules.len();
    }
    let mut top_modules = top_counts.into_iter().collect::<Vec<_>>();
    top_modules.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));

    let mut dependencies = modules
        .iter()
        .flat_map(|module| module.uses.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dependencies.truncate(50);

    AnalyzeResult {
        project_name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Design_BrainModel")
            .to_string(),
        modules: graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::Module)
            .count(),
        structs: modules.iter().map(|module| module.structs.len()).sum(),
        enums: modules.iter().map(|module| module.enums.len()).sum(),
        traits: modules.iter().map(|module| module.traits.len()).sum(),
        functions: modules.iter().map(|module| module.functions.len()).sum(),
        module_count: graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::Module)
            .count(),
        struct_count: modules.iter().map(|module| module.structs.len()).sum(),
        enum_count: modules.iter().map(|module| module.enums.len()).sum(),
        trait_count: modules.iter().map(|module| module.traits.len()).sum(),
        function_count: modules.iter().map(|module| module.functions.len()).sum(),
        top_components: top_modules
            .iter()
            .take(5)
            .map(|(name, _)| name.clone())
            .collect(),
        top_modules: top_modules
            .into_iter()
            .take(5)
            .map(|(name, _)| name)
            .collect(),
        dependencies,
        circular_dependencies: dependency.circular_dependencies.len(),
        layer_violations: architecture.layer_violations.len(),
        boundary_violations: architecture.boundary_violations.len(),
        god_objects: responsibility.god_objects.len(),
        architecture_health: architecture.health.score,
        status: architecture.health.status,
        convergence_score: convergence.convergence_score.score,
        direction: convergence.direction,
        top_proposal: convergence
            .top_proposal
            .as_ref()
            .map(|proposal| proposal.title.clone()),
        predicted_health_delta: convergence.predicted_health_delta,
        convergence_confidence: convergence.confidence,
    }
}

fn persist_to_holographic_memory(
    root: &Path,
    result: &AnalyzeResult,
    repository: &RepositorySnapshot,
    graph: &StructureGraph,
    semantic_structure: &SemanticStructure,
    dependency_analysis: &DependencyAnalysis,
    responsibility_analysis: &ResponsibilityAnalysis,
    architecture_analysis: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
    convergence_report: &ConvergenceReport,
) -> Result<(), String> {
    let entry = SemanticMemoryEntry {
        id: semantic_memory_id(result),
        kind: "structure_analysis".to_string(),
        project_name: result.project_name.clone(),
        summary: result.clone(),
        repository_snapshot: repository.clone(),
        structure_graph: graph.clone(),
        semantic_structure: semantic_structure.clone(),
        dependency_analysis: dependency_analysis.clone(),
        responsibility_analysis: responsibility_analysis.clone(),
        architecture_analysis: architecture_analysis.clone(),
        design_drift: design_drift.clone(),
        convergence_report: convergence_report.clone(),
    };
    let dir = root.join(".dbm/analyze");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    fs::write(
        dir.join("repository_snapshot.json"),
        serde_json::to_string_pretty(repository).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("structure_graph.json"),
        serde_json::to_string_pretty(graph).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("analyze_result.json"),
        serde_json::to_string_pretty(result).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("semantic_structure.json"),
        serde_json::to_string_pretty(semantic_structure).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("dependency_analysis.json"),
        serde_json::to_string_pretty(dependency_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("responsibility_analysis.json"),
        serde_json::to_string_pretty(responsibility_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("architecture_analysis.json"),
        serde_json::to_string_pretty(architecture_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("design_drift.json"),
        serde_json::to_string_pretty(design_drift).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("health_report.json"),
        serde_json::to_string_pretty(&architecture_analysis.health)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("convergence_report.json"),
        serde_json::to_string_pretty(convergence_report).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("refactoring_proposals.json"),
        serde_json::to_string_pretty(&convergence_report.refactoring_proposals)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("impact_analysis.json"),
        serde_json::to_string_pretty(&convergence_report.impact_analysis)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("convergence_score.json"),
        serde_json::to_string_pretty(&convergence_report.convergence_score)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("semantic_memory.jsonl"))
        .map_err(|err| err.to_string())?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&entry).map_err(|err| err.to_string())?
    )
    .map_err(|err| err.to_string())
}

fn collect_repository_entries(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    directories: &mut Vec<PathBuf>,
    cargo_tomls: &mut Vec<PathBuf>,
) -> Result<(), String> {
    directories.push(dir.to_path_buf());
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("cannot read {}: {err}", dir.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_excluded_dir_name(&name) {
            continue;
        }
        if path.is_dir() {
            collect_repository_entries(&path, files, directories, cargo_tomls)?;
        } else if name == "Cargo.toml" {
            cargo_tomls.push(path);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn scan_roots(root: &Path) -> Vec<PathBuf> {
    let apps = root.join("apps");
    let crates = root.join("crates");
    if apps.exists() && crates.exists() {
        vec![apps, crates]
    } else {
        vec![root.to_path_buf()]
    }
}

fn is_excluded_dir_name(name: &str) -> bool {
    matches!(
        name,
        "target" | ".git" | "node_modules" | "dist" | "build" | ".dbm"
    )
}

fn resolve_repository_path(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    if let Ok(current) = std::env::current_dir()
        && current.file_name() == path.file_name()
    {
        return current;
    }
    path.to_path_buf()
}

fn module_path_from_relative_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            let value = value.to_string_lossy();
            if value == "src" {
                continue;
            }
            let clean = value.trim_end_matches(".rs");
            if matches!(clean, "lib" | "main" | "mod") {
                continue;
            }
            parts.push(clean.to_string());
        }
    }
    if parts.is_empty() {
        "root".to_string()
    } else {
        parts.join("::")
    }
}

fn insert_node(
    nodes: &mut BTreeMap<String, StructureNode>,
    id: String,
    label: String,
    kind: StructureNodeKind,
) {
    nodes
        .entry(id.clone())
        .or_insert(StructureNode { id, label, kind });
}

fn append_path_segment(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}::{segment}")
    }
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn type_to_string(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "Self".to_string()),
        _ => "Self".to_string(),
    }
}

fn expr_to_call_name(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Path(path) => path_to_string(&path.path),
        _ => "call".to_string(),
    }
}

fn parse_workspace_members(cargo_toml: &Path) -> Result<Vec<String>, String> {
    if !cargo_toml.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(cargo_toml)
        .map_err(|err| format!("failed to read {}: {err}", cargo_toml.display()))?;
    let mut members = Vec::new();
    let mut in_workspace = false;
    let mut in_members = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_workspace = line == "[workspace]";
            in_members = false;
            continue;
        }
        if !in_workspace {
            continue;
        }
        if line.starts_with("members") {
            in_members = true;
        }
        if in_members {
            for value in quoted_values(line) {
                members.push(value);
            }
            if line.contains(']') {
                in_members = false;
            }
        }
    }
    members.sort();
    members.dedup();
    Ok(members)
}

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        values.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    values
}

fn classify_semantic_category(
    name: &str,
    node: &StructureNode,
    module: Option<&AstModule>,
) -> SemanticCategory {
    let value = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        module
            .map(|module| module.file_path.display().to_string())
            .unwrap_or_default()
            .to_ascii_lowercase()
    );
    if value.contains("controller") || value.contains("command") || value.contains("handler") {
        SemanticCategory::Controller
    } else if value.contains("service") {
        SemanticCategory::Service
    } else if value.contains("repository") || value.contains("repo") {
        SemanticCategory::Repository
    } else if value.contains("engine") || value.contains("analyze") || value.contains("parser") {
        SemanticCategory::Engine
    } else if value.contains("memory") || value.contains("holographic") {
        SemanticCategory::Memory
    } else if value.contains("policy") || value.contains("guard") {
        SemanticCategory::Policy
    } else if value.contains("runtime") || value.contains("executor") {
        SemanticCategory::Runtime
    } else if value.contains("ui") || value.contains("tui") || value.contains("renderer") {
        SemanticCategory::UI
    } else if value.contains("storage") || value.contains("store") || value.contains("persist") {
        SemanticCategory::Storage
    } else if node.kind == StructureNodeKind::Module {
        SemanticCategory::Module
    } else {
        SemanticCategory::Engine
    }
}

fn infer_responsibility(name: &str, node: &StructureNode, module: Option<&AstModule>) -> String {
    match classify_semantic_category(name, node, module) {
        SemanticCategory::Controller => "Command and request routing".to_string(),
        SemanticCategory::Service => "Application service orchestration".to_string(),
        SemanticCategory::Repository => "Repository access boundary".to_string(),
        SemanticCategory::Engine => {
            if name.to_ascii_lowercase().contains("analyze") {
                "Structure Analysis".to_string()
            } else {
                "Core engine computation".to_string()
            }
        }
        SemanticCategory::Memory => "Persistent structure memory".to_string(),
        SemanticCategory::Policy => "Execution policy and safety decisions".to_string(),
        SemanticCategory::Runtime => "Runtime execution and lifecycle".to_string(),
        SemanticCategory::UI => "User interface rendering and interaction".to_string(),
        SemanticCategory::Storage => "Persistence and storage management".to_string(),
        SemanticCategory::Module => "Structural module boundary".to_string(),
    }
}

fn semantic_evidence(name: &str, node: &StructureNode, module: Option<&AstModule>) -> Vec<String> {
    let mut evidence = vec![format!("name:{name}")];
    if let Some(module) = module {
        evidence.push(format!("path:{}", module.file_path.display()));
        if !module.uses.is_empty() {
            evidence.push(format!("uses:{}", module.uses.len()));
        }
        if !module.functions.is_empty() {
            evidence.push(format!("functions:{}", module.functions.len()));
        }
    }
    evidence.push(format!("node_kind:{:?}", node.kind));
    evidence
}

fn semantic_memory_id(result: &AnalyzeResult) -> String {
    let mut hasher = Sha256::new();
    hasher.update(result.project_name.as_bytes());
    hasher.update(result.module_count.to_le_bytes());
    hasher.update(result.struct_count.to_le_bytes());
    hasher.update(result.enum_count.to_le_bytes());
    hasher.update(result.trait_count.to_le_bytes());
    hasher.update(result.function_count.to_le_bytes());
    format!("semantic-memory:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_excludes_target_and_collects_rust_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/a/src")).unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::write(dir.path().join("crates/a/src/lib.rs"), "pub struct A;").unwrap();
        fs::write(
            dir.path().join("target/debug/generated.rs"),
            "pub struct Generated;",
        )
        .unwrap();

        let snapshot = RepositoryScanner.scan(dir.path()).unwrap();
        assert_eq!(snapshot.rust_files.len(), 1);
        assert!(snapshot.rust_files[0].ends_with("crates/a/src/lib.rs"));
        assert!(snapshot.cargo_tomls.is_empty());
    }

    #[test]
    fn ast_extractor_collects_rust_items() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "use crate::runtime::RuntimeCore;\nmod runtime;\npub struct S;\npub enum E { A }\npub trait T {}\nimpl T for S {}\npub fn f() {}\n",
        )
        .unwrap();

        let ast = extract_rust_ast_modules(dir.path(), &[file]).unwrap();
        assert_eq!(ast[0].modules, vec!["runtime"]);
        assert_eq!(ast[0].structs, vec!["S"]);
        assert_eq!(ast[0].enums, vec!["E"]);
        assert_eq!(ast[0].traits, vec!["T"]);
        assert_eq!(ast[0].functions, vec!["f"]);
        assert_eq!(ast[0].uses, vec!["crate::runtime::RuntimeCore"]);
        assert_eq!(ast[0].impls[0].trait_name, "T");
    }

    #[test]
    fn semantic_analysis_classifies_engine_and_memory_components() {
        let graph = StructureGraph {
            nodes: vec![
                StructureNode {
                    id: "module:AnalyzeEngine".to_string(),
                    label: "AnalyzeEngine".to_string(),
                    kind: StructureNodeKind::Module,
                },
                StructureNode {
                    id: "module:HolographicMemory".to_string(),
                    label: "HolographicMemory".to_string(),
                    kind: StructureNodeKind::Module,
                },
            ],
            edges: vec![],
        };

        let semantic = analyze_semantic_structure(&graph, &[]);
        assert!(semantic.components.iter().any(|component| {
            component.name == "AnalyzeEngine"
                && component.category == SemanticCategory::Engine
                && component.responsibility == "Structure Analysis"
        }));
        assert!(semantic.components.iter().any(|component| {
            component.name == "HolographicMemory" && component.category == SemanticCategory::Memory
        }));
    }

    #[test]
    fn workspace_members_are_observed_from_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\", \"crates/core\"]\n",
        )
        .unwrap();

        let snapshot = RepositoryScanner.scan(dir.path()).unwrap();
        assert_eq!(
            snapshot.workspace_members,
            vec!["apps/cli".to_string(), "crates/core".to_string()]
        );
    }

    #[test]
    fn structure_graph_includes_call_edges() {
        let ast = vec![AstModule {
            file_path: PathBuf::from("src/lib.rs"),
            module_path: "root".to_string(),
            modules: vec![],
            structs: vec![],
            enums: vec![],
            traits: vec![],
            functions: vec!["caller".to_string()],
            uses: vec![],
            impls: vec![],
            calls: vec![FunctionCall {
                caller: "caller".to_string(),
                callee: "callee".to_string(),
            }],
        }];

        let graph = build_structure_graph(&ast);
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "fn:root::caller"
                && edge.to == "call:callee"
                && edge.kind == StructureEdgeKind::Calls
        }));
    }

    #[test]
    fn dependency_analyzer_detects_cycles_density_and_hotspots() {
        let ast = vec![
            test_ast_module("a", vec!["crate::b::B"], 120, 60),
            test_ast_module("b", vec!["crate::c::C"], 1, 0),
            test_ast_module("c", vec!["crate::a::A"], 1, 0),
        ];
        let graph = build_structure_graph(&ast);
        let semantic = analyze_semantic_structure(&graph, &ast);

        let analysis = DependencyAnalyzer::analyze(&graph, &semantic, &ast);
        assert_eq!(analysis.circular_dependencies.len(), 1);
        assert!(analysis.dependency_density > 0.0);
        assert!(
            analysis
                .hotspots
                .iter()
                .any(|hotspot| hotspot.component == "a")
        );
    }

    #[test]
    fn responsibility_analyzer_detects_god_objects() {
        let ast = vec![test_ast_module("runtime_core", vec![], 101, 0)];
        let graph = build_structure_graph(&ast);
        let semantic = analyze_semantic_structure(&graph, &ast);

        let analysis = analyze_responsibilities(&graph, &semantic, &ast);
        assert_eq!(analysis.god_objects.len(), 1);
        assert_eq!(analysis.god_objects[0].component, "runtime_core");
    }

    #[test]
    fn architecture_analyzer_reports_layer_violation_and_health() {
        let semantic = SemanticStructure {
            components: vec![
                SemanticComponent {
                    name: "ui".to_string(),
                    category: SemanticCategory::UI,
                    responsibility: "User interface rendering and interaction".to_string(),
                    evidence: vec![],
                },
                SemanticComponent {
                    name: "storage".to_string(),
                    category: SemanticCategory::Storage,
                    responsibility: "Persistence and storage management".to_string(),
                    evidence: vec![],
                },
            ],
        };
        let dependency = DependencyAnalysis {
            circular_dependencies: vec![CircularDependency {
                cycle_nodes: vec!["a".to_string(), "b".to_string()],
            }],
            hidden_dependencies: vec![HiddenDependency {
                source: "ui".to_string(),
                via: "runtime".to_string(),
                target: "storage".to_string(),
            }],
            dependency_density: 1.0,
            hotspots: vec![],
        };
        let responsibility = ResponsibilityAnalysis {
            god_objects: vec![],
            massive_modules: vec![],
            responsibility_leakage: vec![ResponsibilityLeakage {
                source: "ui".to_string(),
                target: "storage".to_string(),
                reason: "UI accesses Storage directly".to_string(),
            }],
        };

        let analysis = analyze_architecture(&semantic, &dependency, &responsibility);
        assert_eq!(analysis.layer_violations.len(), 1);
        assert!(analysis.health.score < 100.0);
        assert!(matches!(
            analysis.health.status,
            ArchitectureStatus::Healthy
                | ArchitectureStatus::Warning
                | ArchitectureStatus::Critical
        ));
    }

    #[test]
    fn convergence_engine_generates_split_proposal_and_impact_prediction() {
        let dependency = DependencyAnalysis {
            circular_dependencies: vec![],
            hidden_dependencies: vec![],
            dependency_density: 1.0,
            hotspots: vec![],
        };
        let responsibility = ResponsibilityAnalysis {
            god_objects: vec![GodObject {
                component: "RuntimeCore".to_string(),
                score: 1.32,
            }],
            massive_modules: vec![],
            responsibility_leakage: vec![],
        };
        let architecture = ArchitectureAnalysis {
            rules: default_architecture_rules(),
            layer_violations: vec![],
            boundary_violations: vec![],
            health: ArchitectureHealth {
                score: 72.3,
                status: ArchitectureStatus::Warning,
            },
        };
        let design_drift = DesignDrift {
            design_source: Some(PathBuf::from("design.md")),
            intent_drifts: vec![],
            missing_design_reference: false,
        };

        let report =
            analyze_convergence(&dependency, &responsibility, &architecture, &design_drift);
        let top = report.top_proposal.as_ref().expect("top proposal");
        assert_eq!(top.title, "Split RuntimeCore");
        assert!(report.predicted_health_delta > 0.0);
        assert!(report.confidence >= 0.80);
        assert!(matches!(
            report.direction,
            Direction::Improving | Direction::Stable
        ));
        assert!(!report.impact_analysis.is_empty());
        assert!(!report.architecture_simulations.is_empty());
        assert!(!report.mutation_impact_models.is_empty());
    }

    #[test]
    fn design_matcher_scores_missing_design_as_partial_alignment() {
        let drift = DesignDrift {
            design_source: None,
            intent_drifts: vec![],
            missing_design_reference: true,
        };

        let alignment = match_design_alignment(&drift);
        assert_eq!(alignment.status, DesignAlignmentStatus::PartiallyAligned);
        assert_eq!(alignment.score, 65.0);
    }

    fn test_ast_module(
        module_path: &str,
        uses: Vec<&str>,
        function_count: usize,
        call_count: usize,
    ) -> AstModule {
        let functions = (0..function_count)
            .map(|index| format!("f_{index}"))
            .collect::<Vec<_>>();
        let calls = (0..call_count)
            .map(|index| FunctionCall {
                caller: "f_0".to_string(),
                callee: format!("callee_{index}"),
            })
            .collect::<Vec<_>>();
        AstModule {
            file_path: PathBuf::from(format!("{module_path}.rs")),
            module_path: module_path.to_string(),
            modules: vec![],
            structs: vec![],
            enums: vec![],
            traits: vec![],
            functions,
            uses: uses.into_iter().map(ToString::to_string).collect(),
            impls: vec![],
            calls,
        }
    }
}
