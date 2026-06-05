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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzeEngineOutput {
    pub root: PathBuf,
    pub repository: RepositorySnapshot,
    pub files: Vec<PathBuf>,
    pub ast_modules: Vec<AstModule>,
    pub graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
    pub result: AnalyzeResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticMemoryEntry {
    pub id: String,
    pub kind: String,
    pub project_name: String,
    pub summary: AnalyzeResult,
    pub repository_snapshot: RepositorySnapshot,
    pub structure_graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
}

pub fn execute(command: AnalyzeCommand) -> Result<AnalyzeEngineOutput, String> {
    let root = resolve_repository_path(&command.path);
    let scanner = RepositoryScanner;
    let repository = scanner.scan(&root)?;
    let files = repository.rust_files.clone();
    let ast_modules = extract_rust_ast_modules(&root, &files)?;
    let graph = build_structure_graph(&ast_modules);
    let semantic_structure = analyze_semantic_structure(&graph, &ast_modules);
    let result = build_analyze_result(&root, &ast_modules, &graph);
    persist_to_holographic_memory(&root, &result, &repository, &graph, &semantic_structure)?;
    Ok(AnalyzeEngineOutput {
        root,
        repository,
        files,
        ast_modules,
        graph,
        semantic_structure,
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
    out.push_str("Top Components\n");
    if result.top_modules.is_empty() {
        out.push_str("(none)\n");
    } else {
        for module in &result.top_modules {
            out.push_str(module);
            out.push('\n');
        }
    }
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

fn build_analyze_result(
    root: &Path,
    modules: &[AstModule],
    graph: &StructureGraph,
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
    }
}

fn persist_to_holographic_memory(
    root: &Path,
    result: &AnalyzeResult,
    repository: &RepositorySnapshot,
    graph: &StructureGraph,
    semantic_structure: &SemanticStructure,
) -> Result<(), String> {
    let entry = SemanticMemoryEntry {
        id: semantic_memory_id(result),
        kind: "structure_analysis".to_string(),
        project_name: result.project_name.clone(),
        summary: result.clone(),
        repository_snapshot: repository.clone(),
        structure_graph: graph.clone(),
        semantic_structure: semantic_structure.clone(),
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
}
