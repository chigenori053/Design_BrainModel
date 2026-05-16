use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkspaceRole {
    Runtime,
    Governance,
    Memory,
    Projection,
    Rollback,
    Planner,
    Execution,
    Persistence,
    Interface,
    Test,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleOwnership {
    RuntimeCore,
    Governance,
    Rollback,
    Memory,
    Projection,
    Execution,
    Planner,
    Persistence,
    Interface,
    Test,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticModuleRole {
    RuntimeAuthority,
    GovernanceAuthority,
    RollbackAuthority,
    MemoryAuthority,
    ProjectionObserver,
    ExecutionProposal,
    PlanningProposal,
    PersistenceAuthority,
    UserInterface,
    TestSupport,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MutationRiskLevel {
    Safe,
    Moderate,
    Protected,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArchitectureDomain {
    Runtime,
    Governance,
    Memory,
    Projection,
    Rollback,
    Planner,
    Execution,
    Persistence,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCrate {
    pub crate_name: String,
    pub crate_path: PathBuf,
    pub crate_role: WorkspaceRole,
    pub dependencies: Vec<String>,
    pub owned_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceModule {
    pub module_name: String,
    pub module_path: PathBuf,
    pub ownership: ModuleOwnership,
    pub semantic_role: SemanticModuleRole,
    pub mutation_risk: MutationRiskLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceDependencyGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<WorkspaceDependencyEdge>,
    pub cyclic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspaceDependencyEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTopologySnapshot {
    pub workspace_id: u64,
    pub crates: Vec<WorkspaceCrate>,
    pub modules: Vec<WorkspaceModule>,
    pub dependency_graph: WorkspaceDependencyGraph,
    pub deterministic_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBoundaryMap {
    pub runtime_core_modules: Vec<String>,
    pub governance_modules: Vec<String>,
    pub rollback_modules: Vec<String>,
    pub mutable_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticConnection {
    pub source: String,
    pub target: String,
    pub domain: ArchitectureDomain,
    pub propagation_risk: MutationRiskLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLineage {
    pub lineage_id: u64,
    pub topology_revisions: Vec<u64>,
    pub deterministic_hash_chain: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSemanticMap {
    pub architecture_domains: Vec<ArchitectureDomain>,
    pub semantic_connections: Vec<SemanticConnection>,
    pub persistent_lineage: WorkspaceLineage,
}

pub fn workspace_topology_snapshot(workspace_root: &Path) -> WorkspaceTopologySnapshot {
    let root = workspace_root.to_path_buf();
    let member_paths = discover_workspace_members(&root);
    let mut crates = member_paths
        .iter()
        .map(|member| workspace_crate(&root, member))
        .collect::<Vec<_>>();
    crates.sort_by(|left, right| left.crate_name.cmp(&right.crate_name));

    let mut modules = crates
        .iter()
        .flat_map(|krate| discover_crate_modules(&root, krate))
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| {
        left.module_path
            .cmp(&right.module_path)
            .then_with(|| left.module_name.cmp(&right.module_name))
    });

    let dependency_graph = workspace_dependency_graph_from_crates(&crates);
    let mut snapshot = WorkspaceTopologySnapshot {
        workspace_id: stable_hash_strs(
            crates
                .iter()
                .map(|krate| krate.crate_name.as_str())
                .chain(modules.iter().map(|module| module.module_name.as_str())),
        ),
        crates,
        modules,
        dependency_graph,
        deterministic_checksum: 0,
    };
    snapshot.deterministic_checksum = deterministic_workspace_checksum(&snapshot);
    snapshot
}

pub fn workspace_dependency_graph(
    snapshot: &WorkspaceTopologySnapshot,
) -> WorkspaceDependencyGraph {
    workspace_dependency_graph_from_crates(&snapshot.crates)
}

pub fn runtime_boundary_map(snapshot: &WorkspaceTopologySnapshot) -> RuntimeBoundaryMap {
    let mut runtime_core_modules = Vec::new();
    let mut governance_modules = Vec::new();
    let mut rollback_modules = Vec::new();
    let mut mutable_modules = Vec::new();

    for module in &snapshot.modules {
        let name = module.module_name.clone();
        match module.ownership {
            ModuleOwnership::RuntimeCore => runtime_core_modules.push(name),
            ModuleOwnership::Governance => governance_modules.push(name),
            ModuleOwnership::Rollback => rollback_modules.push(name),
            _ if module.mutation_risk <= MutationRiskLevel::Moderate => mutable_modules.push(name),
            _ => {}
        }
    }

    runtime_core_modules.sort();
    runtime_core_modules.dedup();
    governance_modules.sort();
    governance_modules.dedup();
    rollback_modules.sort();
    rollback_modules.dedup();
    mutable_modules.sort();
    mutable_modules.dedup();

    RuntimeBoundaryMap {
        runtime_core_modules,
        governance_modules,
        rollback_modules,
        mutable_modules,
    }
}

pub fn workspace_semantic_map(snapshot: &WorkspaceTopologySnapshot) -> WorkspaceSemanticMap {
    let mut domains = BTreeSet::new();
    for krate in &snapshot.crates {
        domains.insert(domain_for_role(krate.crate_role));
    }
    for module in &snapshot.modules {
        domains.insert(domain_for_ownership(module.ownership));
    }

    let module_by_name = snapshot
        .modules
        .iter()
        .map(|module| (module.module_name.clone(), module))
        .collect::<BTreeMap<_, _>>();
    let mut semantic_connections = snapshot
        .dependency_graph
        .edges
        .iter()
        .map(|edge| {
            let source_module = module_by_name.get(&edge.from);
            let risk = source_module
                .map(|module| module.mutation_risk)
                .unwrap_or(MutationRiskLevel::Moderate);
            SemanticConnection {
                source: edge.from.clone(),
                target: edge.to.clone(),
                domain: source_module
                    .map(|module| domain_for_ownership(module.ownership))
                    .unwrap_or(ArchitectureDomain::Execution),
                propagation_risk: risk,
            }
        })
        .collect::<Vec<_>>();
    semantic_connections.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.target.cmp(&right.target))
    });

    WorkspaceSemanticMap {
        architecture_domains: domains.into_iter().collect(),
        semantic_connections,
        persistent_lineage: WorkspaceLineage {
            lineage_id: stable_hash_u64s([
                snapshot.workspace_id,
                snapshot.deterministic_checksum,
                snapshot.crates.len() as u64,
                snapshot.modules.len() as u64,
            ]),
            topology_revisions: vec![snapshot.deterministic_checksum],
            deterministic_hash_chain: vec![stable_hash_u64s([
                snapshot.workspace_id,
                snapshot.deterministic_checksum,
            ])],
        },
    }
}

pub fn validate_workspace_topology(snapshot: &WorkspaceTopologySnapshot) -> bool {
    deterministic_workspace_checksum(snapshot) == snapshot.deterministic_checksum
        && snapshot
            .crates
            .windows(2)
            .all(|pair| pair[0].crate_name <= pair[1].crate_name)
        && snapshot
            .modules
            .windows(2)
            .all(|pair| pair[0].module_path <= pair[1].module_path)
        && snapshot
            .dependency_graph
            .edges
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
}

pub fn render_workspace_snapshot(snapshot: &WorkspaceTopologySnapshot) -> String {
    format!(
        "workspace_id: {}\ncrates: {}\nmodules: {}\ndependency_edges: {}\ncyclic: {}\nchecksum: {}\nvalid: {}",
        snapshot.workspace_id,
        snapshot.crates.len(),
        snapshot.modules.len(),
        snapshot.dependency_graph.edges.len(),
        snapshot.dependency_graph.cyclic,
        snapshot.deterministic_checksum,
        validate_workspace_topology(snapshot),
    )
}

pub fn render_dependency_graph(graph: &WorkspaceDependencyGraph) -> String {
    let mut lines = vec![
        format!("nodes: {}", graph.nodes.len()),
        format!("edges: {}", graph.edges.len()),
        format!("cyclic: {}", graph.cyclic),
    ];
    for edge in &graph.edges {
        lines.push(format!("edge: {} -> {}", edge.from, edge.to));
    }
    lines.join("\n")
}

pub fn render_runtime_boundaries(boundaries: &RuntimeBoundaryMap) -> String {
    format!(
        "runtime_core: {:?}\ngovernance: {:?}\nrollback: {:?}\nmutable: {:?}",
        boundaries.runtime_core_modules,
        boundaries.governance_modules,
        boundaries.rollback_modules,
        boundaries.mutable_modules,
    )
}

pub fn render_workspace_architecture(map: &WorkspaceSemanticMap) -> String {
    let mut lines = vec![
        format!("architecture_domains: {:?}", map.architecture_domains),
        format!("semantic_connections: {}", map.semantic_connections.len()),
        format!("lineage_id: {}", map.persistent_lineage.lineage_id),
        format!(
            "topology_revisions: {:?}",
            map.persistent_lineage.topology_revisions
        ),
    ];
    for connection in &map.semantic_connections {
        lines.push(format!(
            "connection: {} -> {} domain={:?} risk={:?}",
            connection.source, connection.target, connection.domain, connection.propagation_risk
        ));
    }
    lines.join("\n")
}

pub fn render_mutation_risks(snapshot: &WorkspaceTopologySnapshot) -> String {
    let mut lines = vec![format!("modules: {}", snapshot.modules.len())];
    for module in &snapshot.modules {
        lines.push(format!(
            "risk: {:?} module={} ownership={:?} semantic_role={:?}",
            module.mutation_risk, module.module_name, module.ownership, module.semantic_role
        ));
    }
    lines.join("\n")
}

fn discover_workspace_members(root: &Path) -> Vec<PathBuf> {
    let manifest = root.join("Cargo.toml");
    let Ok(contents) = fs::read_to_string(&manifest) else {
        return Vec::new();
    };
    let members = parse_workspace_members(&contents);
    if members.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        members
    }
}

fn parse_workspace_members(contents: &str) -> Vec<PathBuf> {
    let mut members = Vec::new();
    let mut in_members = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.starts_with("members") && line.contains('[') {
            in_members = true;
            continue;
        }
        if in_members && line.starts_with(']') {
            break;
        }
        if in_members && let Some(value) = quoted_value(line) {
            members.push(PathBuf::from(value));
        }
    }
    members.sort();
    members.dedup();
    members
}

fn workspace_crate(root: &Path, member: &Path) -> WorkspaceCrate {
    let manifest_path = root.join(member).join("Cargo.toml");
    let contents = fs::read_to_string(&manifest_path).unwrap_or_default();
    let crate_name = parse_package_name(&contents).unwrap_or_else(|| {
        member
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
            .to_string()
    });
    let dependencies = parse_dependencies(&contents);
    let crate_path = normalize_relative(member);
    let crate_role = classify_workspace_role(&crate_name, &crate_path);
    let owned_modules = discover_module_paths(root, &crate_path)
        .into_iter()
        .map(|path| module_name_from_path(&path))
        .collect::<Vec<_>>();
    WorkspaceCrate {
        crate_name,
        crate_path,
        crate_role,
        dependencies,
        owned_modules,
    }
}

fn parse_package_name(contents: &str) -> Option<String> {
    let mut in_package = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            return None;
        }
        if in_package && line.starts_with("name") {
            return line
                .split_once('=')
                .map(|(_, value)| value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn parse_dependencies(contents: &str) -> Vec<String> {
    let mut dependencies = Vec::new();
    let mut in_dependencies = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.starts_with("[dependencies]")
            || line.starts_with("[dev-dependencies]")
            || line.starts_with("[build-dependencies]")
        {
            in_dependencies = true;
            continue;
        }
        if in_dependencies && line.starts_with('[') {
            in_dependencies = false;
        }
        if in_dependencies
            && !line.is_empty()
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            dependencies.push(name.trim().trim_matches('"').to_string());
        }
    }
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn discover_crate_modules(root: &Path, krate: &WorkspaceCrate) -> Vec<WorkspaceModule> {
    discover_module_paths(root, &krate.crate_path)
        .into_iter()
        .map(|module_path| {
            let module_name = module_name_from_path(&module_path);
            let ownership = classify_module_ownership(&module_name, &module_path);
            WorkspaceModule {
                module_name,
                module_path,
                ownership,
                semantic_role: semantic_role_for_ownership(ownership),
                mutation_risk: mutation_risk_for_ownership(ownership),
            }
        })
        .collect()
}

fn discover_module_paths(root: &Path, crate_path: &Path) -> Vec<PathBuf> {
    let src = root.join(crate_path).join("src");
    let mut modules = Vec::new();
    visit_rust_modules(root, &src, &mut modules);
    modules.sort();
    modules.dedup();
    modules
}

fn visit_rust_modules(root: &Path, dir: &Path, modules: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            visit_rust_modules(root, &path, modules);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            modules.push(relative_path(root, &path));
        }
    }
}

fn workspace_dependency_graph_from_crates(crates: &[WorkspaceCrate]) -> WorkspaceDependencyGraph {
    let crate_names = crates
        .iter()
        .map(|krate| krate.crate_name.clone())
        .collect::<BTreeSet<_>>();
    let mut nodes = crate_names.iter().cloned().collect::<Vec<_>>();
    let mut edges = Vec::new();
    for krate in crates {
        for dependency in &krate.dependencies {
            if crate_names.contains(dependency) {
                edges.push(WorkspaceDependencyEdge {
                    from: krate.crate_name.clone(),
                    to: dependency.clone(),
                });
            }
        }
    }
    nodes.sort();
    edges.sort();
    edges.dedup();
    let cyclic = dependency_graph_has_cycle(&nodes, &edges);
    WorkspaceDependencyGraph {
        nodes,
        edges,
        cyclic,
    }
}

fn dependency_graph_has_cycle(nodes: &[String], edges: &[WorkspaceDependencyEdge]) -> bool {
    let adjacency = edges.iter().fold(
        BTreeMap::<String, Vec<String>>::new(),
        |mut adjacency, edge| {
            adjacency
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            adjacency
        },
    );
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    nodes
        .iter()
        .any(|node| has_cycle_from(node, &adjacency, &mut visiting, &mut visited))
}

fn has_cycle_from(
    node: &str,
    adjacency: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_string()) {
        return true;
    }
    for next in adjacency.get(node).into_iter().flatten() {
        if has_cycle_from(next, adjacency, visiting, visited) {
            return true;
        }
    }
    visiting.remove(node);
    visited.insert(node.to_string());
    false
}

fn classify_workspace_role(crate_name: &str, path: &Path) -> WorkspaceRole {
    let text = format!("{} {}", crate_name, path.display()).to_ascii_lowercase();
    if text.contains("runtime") || crate_name == "design_cli" {
        WorkspaceRole::Runtime
    } else if text.contains("governance") || text.contains("policy") {
        WorkspaceRole::Governance
    } else if text.contains("rollback") {
        WorkspaceRole::Rollback
    } else if text.contains("memory") || text.contains("semantic") {
        WorkspaceRole::Memory
    } else if text.contains("projection") || text.contains("viewer") {
        WorkspaceRole::Projection
    } else if text.contains("planner") || text.contains("reasoning") {
        WorkspaceRole::Planner
    } else if text.contains("execution") || text.contains("executor") {
        WorkspaceRole::Execution
    } else if text.contains("persistence") || text.contains("store") {
        WorkspaceRole::Persistence
    } else if text.contains("gui") || text.contains("ui") || text.contains("desktop") {
        WorkspaceRole::Interface
    } else if text.contains("test") {
        WorkspaceRole::Test
    } else {
        WorkspaceRole::Unknown
    }
}

fn classify_module_ownership(module_name: &str, path: &Path) -> ModuleOwnership {
    let text = format!("{} {}", module_name, path.display()).to_ascii_lowercase();
    if text.contains("runtime/unified_apply")
        || text.contains("runtime/unified_projection")
        || text.contains("runtime/mod")
        || text.contains("runtime_state")
        || text.contains("runtime/runtime")
    {
        ModuleOwnership::RuntimeCore
    } else if text.contains("governance") || text.contains("policy") {
        ModuleOwnership::Governance
    } else if text.contains("rollback") {
        ModuleOwnership::Rollback
    } else if text.contains("memory") || text.contains("semantic") {
        ModuleOwnership::Memory
    } else if text.contains("projection")
        || text.contains("render")
        || text.contains("viewer")
        || text.contains("observability")
    {
        ModuleOwnership::Projection
    } else if text.contains("executor")
        || text.contains("execution")
        || text.contains("apply")
        || text.contains("coding")
    {
        ModuleOwnership::Execution
    } else if text.contains("planner") || text.contains("plan") {
        ModuleOwnership::Planner
    } else if text.contains("persistence")
        || text.contains("checkpoint")
        || text.contains("lineage")
        || text.contains("store")
    {
        ModuleOwnership::Persistence
    } else if text.contains("tui") || text.contains("ui") || text.contains("gui") {
        ModuleOwnership::Interface
    } else if text.contains("test") {
        ModuleOwnership::Test
    } else {
        ModuleOwnership::Unknown
    }
}

fn semantic_role_for_ownership(ownership: ModuleOwnership) -> SemanticModuleRole {
    match ownership {
        ModuleOwnership::RuntimeCore => SemanticModuleRole::RuntimeAuthority,
        ModuleOwnership::Governance => SemanticModuleRole::GovernanceAuthority,
        ModuleOwnership::Rollback => SemanticModuleRole::RollbackAuthority,
        ModuleOwnership::Memory => SemanticModuleRole::MemoryAuthority,
        ModuleOwnership::Projection => SemanticModuleRole::ProjectionObserver,
        ModuleOwnership::Execution => SemanticModuleRole::ExecutionProposal,
        ModuleOwnership::Planner => SemanticModuleRole::PlanningProposal,
        ModuleOwnership::Persistence => SemanticModuleRole::PersistenceAuthority,
        ModuleOwnership::Interface => SemanticModuleRole::UserInterface,
        ModuleOwnership::Test => SemanticModuleRole::TestSupport,
        ModuleOwnership::Unknown => SemanticModuleRole::Unknown,
    }
}

fn mutation_risk_for_ownership(ownership: ModuleOwnership) -> MutationRiskLevel {
    match ownership {
        ModuleOwnership::RuntimeCore => MutationRiskLevel::Critical,
        ModuleOwnership::Governance | ModuleOwnership::Rollback | ModuleOwnership::Persistence => {
            MutationRiskLevel::Protected
        }
        ModuleOwnership::Memory | ModuleOwnership::Execution | ModuleOwnership::Planner => {
            MutationRiskLevel::Moderate
        }
        ModuleOwnership::Projection
        | ModuleOwnership::Interface
        | ModuleOwnership::Test
        | ModuleOwnership::Unknown => MutationRiskLevel::Safe,
    }
}

fn domain_for_role(role: WorkspaceRole) -> ArchitectureDomain {
    match role {
        WorkspaceRole::Runtime => ArchitectureDomain::Runtime,
        WorkspaceRole::Governance => ArchitectureDomain::Governance,
        WorkspaceRole::Memory => ArchitectureDomain::Memory,
        WorkspaceRole::Projection => ArchitectureDomain::Projection,
        WorkspaceRole::Rollback => ArchitectureDomain::Rollback,
        WorkspaceRole::Planner => ArchitectureDomain::Planner,
        WorkspaceRole::Execution => ArchitectureDomain::Execution,
        WorkspaceRole::Persistence => ArchitectureDomain::Persistence,
        WorkspaceRole::Interface | WorkspaceRole::Test | WorkspaceRole::Unknown => {
            ArchitectureDomain::Interface
        }
    }
}

fn domain_for_ownership(ownership: ModuleOwnership) -> ArchitectureDomain {
    match ownership {
        ModuleOwnership::RuntimeCore => ArchitectureDomain::Runtime,
        ModuleOwnership::Governance => ArchitectureDomain::Governance,
        ModuleOwnership::Rollback => ArchitectureDomain::Rollback,
        ModuleOwnership::Memory => ArchitectureDomain::Memory,
        ModuleOwnership::Projection => ArchitectureDomain::Projection,
        ModuleOwnership::Execution => ArchitectureDomain::Execution,
        ModuleOwnership::Planner => ArchitectureDomain::Planner,
        ModuleOwnership::Persistence => ArchitectureDomain::Persistence,
        ModuleOwnership::Interface | ModuleOwnership::Test | ModuleOwnership::Unknown => {
            ArchitectureDomain::Interface
        }
    }
}

fn deterministic_workspace_checksum(snapshot: &WorkspaceTopologySnapshot) -> u64 {
    let mut values = vec![snapshot.workspace_id];
    for krate in &snapshot.crates {
        values.extend([
            stable_hash_strs([krate.crate_name.as_str()]),
            stable_hash_strs([krate.crate_path.display().to_string().as_str()]),
            krate.crate_role as u64,
            stable_hash_strs(krate.dependencies.iter().map(String::as_str)),
            stable_hash_strs(krate.owned_modules.iter().map(String::as_str)),
        ]);
    }
    for module in &snapshot.modules {
        values.extend([
            stable_hash_strs([module.module_name.as_str()]),
            stable_hash_strs([module.module_path.display().to_string().as_str()]),
            module.ownership as u64,
            module.semantic_role as u64,
            module.mutation_risk as u64,
        ]);
    }
    for edge in &snapshot.dependency_graph.edges {
        values.extend([
            stable_hash_strs([edge.from.as_str()]),
            stable_hash_strs([edge.to.as_str()]),
        ]);
    }
    stable_hash_u64s(values)
}

fn quoted_value(line: &str) -> Option<String> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn normalize_relative(path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path.to_path_buf()
    }
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn module_name_from_path(path: &Path) -> String {
    path.with_extension("")
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn stable_hash_strs<'a>(values: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_hash_u64s(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture_workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(tmp.path().join("crates/runtime_core/src/runtime"))
            .expect("runtime dirs");
        fs::create_dir_all(tmp.path().join("crates/memory_core/src")).expect("memory dirs");
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"[workspace]
members = [
    "crates/memory_core",
    "crates/runtime_core",
]
"#,
        )
        .expect("workspace manifest");
        fs::write(
            tmp.path().join("crates/runtime_core/Cargo.toml"),
            r#"[package]
name = "runtime_core"

[dependencies]
memory_core = { path = "../memory_core" }
"#,
        )
        .expect("runtime manifest");
        fs::write(
            tmp.path().join("crates/memory_core/Cargo.toml"),
            r#"[package]
name = "memory_core"
"#,
        )
        .expect("memory manifest");
        fs::write(
            tmp.path().join("crates/runtime_core/src/lib.rs"),
            "pub mod runtime;",
        )
        .expect("runtime lib");
        fs::write(
            tmp.path()
                .join("crates/runtime_core/src/runtime/unified_apply.rs"),
            "pub fn apply() {}",
        )
        .expect("runtime module");
        fs::write(
            tmp.path().join("crates/memory_core/src/lib.rs"),
            "pub fn memory() {}",
        )
        .expect("memory lib");
        tmp
    }

    #[test]
    fn workspace_snapshot_is_deterministic() {
        let tmp = fixture_workspace();
        let first = workspace_topology_snapshot(tmp.path());
        let second = workspace_topology_snapshot(tmp.path());

        assert_eq!(first, second);
        assert!(validate_workspace_topology(&first));
        assert_eq!(first.crates.len(), 2);
    }

    #[test]
    fn dependency_graph_detects_workspace_edges() {
        let tmp = fixture_workspace();
        let snapshot = workspace_topology_snapshot(tmp.path());
        let graph = workspace_dependency_graph(&snapshot);

        assert!(
            graph
                .edges
                .iter()
                .any(|edge| { edge.from == "runtime_core" && edge.to == "memory_core" })
        );
        assert!(!graph.cyclic);
    }

    #[test]
    fn runtime_boundary_is_protected() {
        let tmp = fixture_workspace();
        let snapshot = workspace_topology_snapshot(tmp.path());
        let boundaries = runtime_boundary_map(&snapshot);

        assert!(
            boundaries
                .runtime_core_modules
                .iter()
                .any(|module| module.contains("unified_apply"))
        );
        assert!(snapshot.modules.iter().any(|module| {
            module.module_name.contains("unified_apply")
                && module.mutation_risk == MutationRiskLevel::Critical
        }));
    }

    #[test]
    fn semantic_mapping_has_append_only_lineage() {
        let tmp = fixture_workspace();
        let snapshot = workspace_topology_snapshot(tmp.path());
        let semantic = workspace_semantic_map(&snapshot);

        assert!(
            semantic
                .architecture_domains
                .contains(&ArchitectureDomain::Runtime)
        );
        assert_eq!(semantic.persistent_lineage.topology_revisions.len(), 1);
        assert_eq!(
            semantic.persistent_lineage.deterministic_hash_chain.len(),
            1
        );
    }
}
