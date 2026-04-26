use std::collections::{BTreeMap, BTreeSet};

use code_ir::{
    CodeIr, CodeMetadata, CodeModule, ControlFlowEdge, DataFlowEdge, Dependency, DependencyType,
    ModuleId, ModuleType,
};
use design_domain::{DependencyKind, Layer};

use crate::dbm::analyzer::{DependencyEdgeType, ProjectAnalysisResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureView {
    pub nodes: Vec<NodeView>,
    pub edges: Vec<EdgeView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyView {
    pub graph: DirectedGraph<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowView {
    pub control_flow: Vec<FlowEdge>,
    pub data_flow: Vec<FlowEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectedGraph<T> {
    pub nodes: Vec<T>,
    pub edges: Vec<DirectedEdge<T>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirectedEdge<T> {
    pub edge_id: String,
    pub from: T,
    pub to: T,
    pub kind: String,
    pub cycle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeView {
    pub node_id: String,
    pub label: String,
    pub layer: usize,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeView {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub cycle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FlowEdge {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub label: String,
}

pub fn project_analysis_to_code_ir(project: &ProjectAnalysisResult) -> CodeIr {
    let module_names = project_module_names(project);
    let ids = module_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), ModuleId((index + 1) as u64)))
        .collect::<BTreeMap<_, _>>();

    let dependencies = project
        .dependencies
        .iter()
        .filter_map(|edge| {
            Some(Dependency {
                source: *ids.get(&edge.from)?,
                target: *ids.get(&edge.to)?,
                dependency_type: map_dependency_type(edge.edge_type),
                kind: map_dependency_kind(edge.edge_type),
            })
        })
        .collect::<Vec<_>>();

    let mut modules = module_names
        .iter()
        .map(|name| {
            let module_id = ids[name];
            let mut outgoing = dependencies
                .iter()
                .filter(|dependency| dependency.source == module_id)
                .map(|dependency| dependency.target)
                .collect::<Vec<_>>();
            outgoing.sort();
            outgoing.dedup();

            CodeModule {
                id: module_id,
                name: name.clone(),
                module_type: infer_module_type(name),
                interfaces: Vec::new(),
                functions: Vec::new(),
                dependencies: outgoing,
                layer: Layer::infer_from_name(name),
                responsibilities: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));

    let mut dependencies = dependencies;
    dependencies.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
            .then(format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });

    let control_flow = dependencies
        .iter()
        .map(|dependency| ControlFlowEdge {
            from: dependency.source.0,
            to: dependency.target.0,
            label: dependency_label(dependency),
        })
        .collect::<Vec<_>>();
    let data_flow = dependencies
        .iter()
        .map(|dependency| DataFlowEdge {
            from: dependency.source.0,
            to: dependency.target.0,
            payload: dependency_label(dependency),
        })
        .collect::<Vec<_>>();

    CodeIr {
        modules,
        interfaces: Vec::new(),
        datatypes: Vec::new(),
        functions: Vec::new(),
        dependencies,
        metadata: CodeMetadata::default(),
        control_flow,
        data_flow,
    }
}

pub fn analyze_structure(ir: &CodeIr) -> StructureView {
    let dependency_view = analyze_dependencies(ir);
    let cycle_pairs = dependency_view
        .graph
        .edges
        .iter()
        .filter(|edge| edge.cycle)
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect::<BTreeSet<_>>();

    let mut nodes = ir
        .modules
        .iter()
        .map(|module| NodeView {
            node_id: module.name.clone(),
            label: module.name.clone(),
            layer: module.layer.order(),
            role: module_role(module),
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));

    let id_to_name = module_name_by_id(ir);
    let mut edges = ir
        .dependencies
        .iter()
        .filter_map(|dependency| {
            let from = id_to_name.get(&dependency.source)?.clone();
            let to = id_to_name.get(&dependency.target)?.clone();
            let kind = dependency_label(dependency);
            Some(EdgeView {
                edge_id: format!("{from}->{to}:{kind}"),
                from: from.clone(),
                to: to.clone(),
                kind,
                cycle: cycle_pairs.contains(&(from, to)),
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    StructureView { nodes, edges }
}

pub fn analyze_dependencies(ir: &CodeIr) -> DependencyView {
    let id_to_name = module_name_by_id(ir);
    let adjacency = adjacency_by_name(ir, &id_to_name);

    let mut nodes = ir
        .modules
        .iter()
        .map(|module| module.name.clone())
        .collect::<Vec<_>>();
    nodes.sort();
    nodes.dedup();

    let mut edges = ir
        .dependencies
        .iter()
        .filter_map(|dependency| {
            let from = id_to_name.get(&dependency.source)?.clone();
            let to = id_to_name.get(&dependency.target)?.clone();
            let kind = dependency_label(dependency);
            let cycle = reachable(&adjacency, &to, &from);
            Some(DirectedEdge {
                edge_id: format!("{from}->{to}:{kind}"),
                from,
                to,
                kind,
                cycle,
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    DependencyView {
        graph: DirectedGraph { nodes, edges },
    }
}

pub fn analyze_flow(ir: &CodeIr) -> FlowView {
    let id_to_name = module_name_by_id(ir);
    let mut control_flow = ir
        .control_flow
        .iter()
        .filter_map(|edge| {
            Some(FlowEdge {
                edge_id: format!(
                    "{}->{}:{}",
                    id_to_name.get(&ModuleId(edge.from))?,
                    id_to_name.get(&ModuleId(edge.to))?,
                    edge.label
                ),
                from: id_to_name.get(&ModuleId(edge.from))?.clone(),
                to: id_to_name.get(&ModuleId(edge.to))?.clone(),
                label: edge.label.clone(),
            })
        })
        .collect::<Vec<_>>();
    control_flow.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    let mut data_flow = ir
        .data_flow
        .iter()
        .filter_map(|edge| {
            Some(FlowEdge {
                edge_id: format!(
                    "{}->{}:{}",
                    id_to_name.get(&ModuleId(edge.from))?,
                    id_to_name.get(&ModuleId(edge.to))?,
                    edge.payload
                ),
                from: id_to_name.get(&ModuleId(edge.from))?.clone(),
                to: id_to_name.get(&ModuleId(edge.to))?.clone(),
                label: edge.payload.clone(),
            })
        })
        .collect::<Vec<_>>();
    data_flow.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    FlowView {
        control_flow,
        data_flow,
    }
}

fn project_module_names(project: &ProjectAnalysisResult) -> Vec<String> {
    let mut names = project
        .modules
        .iter()
        .map(|module| module.name.clone())
        .collect::<BTreeSet<_>>();
    for dependency in &project.dependencies {
        names.insert(dependency.from.clone());
        names.insert(dependency.to.clone());
    }
    names.into_iter().collect()
}

fn module_name_by_id(ir: &CodeIr) -> BTreeMap<ModuleId, String> {
    ir.modules
        .iter()
        .map(|module| (module.id, module.name.clone()))
        .collect()
}

fn adjacency_by_name(
    ir: &CodeIr,
    id_to_name: &BTreeMap<ModuleId, String>,
) -> BTreeMap<String, Vec<String>> {
    let mut adjacency = BTreeMap::<String, Vec<String>>::new();
    for dependency in &ir.dependencies {
        let Some(from) = id_to_name.get(&dependency.source) else {
            continue;
        };
        let Some(to) = id_to_name.get(&dependency.target) else {
            continue;
        };
        adjacency.entry(from.clone()).or_default().push(to.clone());
    }
    for targets in adjacency.values_mut() {
        targets.sort();
        targets.dedup();
    }
    adjacency
}

fn reachable(adjacency: &BTreeMap<String, Vec<String>>, from: &str, goal: &str) -> bool {
    let mut stack = vec![from.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(node) = stack.pop() {
        if node == goal {
            return true;
        }
        if !seen.insert(node.clone()) {
            continue;
        }
        if let Some(next) = adjacency.get(&node) {
            for candidate in next.iter().rev() {
                stack.push(candidate.clone());
            }
        }
    }
    false
}

fn dependency_label(dependency: &Dependency) -> String {
    format!("{:?}", dependency.kind).to_ascii_lowercase()
}

fn module_role(module: &CodeModule) -> String {
    match module.module_type {
        ModuleType::API => "Interface",
        ModuleType::Adapter => "Adapter",
        ModuleType::DatabaseAdapter | ModuleType::QueueAdapter => "Infrastructure",
        ModuleType::Worker => "Worker",
        ModuleType::Service | ModuleType::Library => {
            if module.layer.order() == 0 {
                "Core"
            } else {
                "Application"
            }
        }
    }
    .to_string()
}

fn infer_module_type(name: &str) -> ModuleType {
    let lower = name.to_ascii_lowercase();
    if lower.contains("api") || lower.contains("controller") || lower.contains("ui") {
        ModuleType::API
    } else if lower.contains("db") || lower.contains("database") || lower.contains("store") {
        ModuleType::DatabaseAdapter
    } else if lower.contains("queue") {
        ModuleType::QueueAdapter
    } else if lower.contains("adapter") {
        ModuleType::Adapter
    } else if lower.contains("worker") || lower.contains("job") {
        ModuleType::Worker
    } else {
        ModuleType::Service
    }
}

fn map_dependency_kind(edge_type: DependencyEdgeType) -> DependencyKind {
    match edge_type {
        DependencyEdgeType::Direct => DependencyKind::Calls,
        DependencyEdgeType::Mediated => DependencyKind::Emits,
    }
}

fn map_dependency_type(edge_type: DependencyEdgeType) -> DependencyType {
    match edge_type {
        DependencyEdgeType::Direct => DependencyType::Call,
        DependencyEdgeType::Mediated => DependencyType::Publish,
    }
}

#[cfg(test)]
mod tests {
    use code_ir::{
        CodeIr, CodeMetadata, CodeModule, ControlFlowEdge, DataFlowEdge, Dependency,
        DependencyType, ModuleId, ModuleType,
    };
    use design_domain::{DependencyKind, Layer};

    use super::{analyze_dependencies, analyze_flow, analyze_structure};

    fn sample_ir() -> CodeIr {
        CodeIr {
            modules: vec![
                CodeModule {
                    id: ModuleId(2),
                    name: "debug".to_string(),
                    module_type: ModuleType::Library,
                    interfaces: Vec::new(),
                    functions: Vec::new(),
                    dependencies: vec![ModuleId(1)],
                    layer: Layer::Service,
                    responsibilities: Vec::new(),
                },
                CodeModule {
                    id: ModuleId(1),
                    name: "renderer".to_string(),
                    module_type: ModuleType::API,
                    interfaces: Vec::new(),
                    functions: Vec::new(),
                    dependencies: vec![ModuleId(2)],
                    layer: Layer::Ui,
                    responsibilities: Vec::new(),
                },
            ],
            interfaces: Vec::new(),
            datatypes: Vec::new(),
            functions: Vec::new(),
            dependencies: vec![
                Dependency {
                    source: ModuleId(1),
                    target: ModuleId(2),
                    dependency_type: DependencyType::Call,
                    kind: DependencyKind::Calls,
                },
                Dependency {
                    source: ModuleId(2),
                    target: ModuleId(1),
                    dependency_type: DependencyType::Call,
                    kind: DependencyKind::Calls,
                },
            ],
            metadata: CodeMetadata::default(),
            control_flow: vec![
                ControlFlowEdge {
                    from: 1,
                    to: 2,
                    label: "calls".to_string(),
                },
                ControlFlowEdge {
                    from: 2,
                    to: 1,
                    label: "calls".to_string(),
                },
            ],
            data_flow: vec![DataFlowEdge {
                from: 1,
                to: 2,
                payload: "calls".to_string(),
            }],
        }
    }

    #[test]
    fn deterministic_under_shuffle() {
        let baseline = sample_ir();
        let mut shuffled = sample_ir();
        shuffled.modules.reverse();
        shuffled.dependencies.reverse();
        shuffled.control_flow.reverse();

        assert_eq!(analyze_structure(&baseline), analyze_structure(&shuffled));
        assert_eq!(
            analyze_dependencies(&baseline),
            analyze_dependencies(&shuffled)
        );
        assert_eq!(analyze_flow(&baseline), analyze_flow(&shuffled));
    }

    #[test]
    fn marks_cycle_edges_deterministically() {
        let dependencies = analyze_dependencies(&sample_ir());
        assert!(dependencies.graph.edges.iter().all(|edge| edge.cycle));
    }
}
