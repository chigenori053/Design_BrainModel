use std::collections::BTreeMap;

use design_domain::{Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId};
use world_model_core::{SimulationResult, WorldState};

use crate::{DefaultSimulationEngine, SimulationEngine};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignParameters {
    pub values: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExplorationMetadata {
    pub labels: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignState {
    pub active_design: Architecture,
    pub constraints: Vec<Constraint>,
    pub parameters: DesignParameters,
    pub exploration_step: u64,
    pub exploration_metadata: ExplorationMetadata,
}

impl DesignState {
    pub fn from_architecture(architecture: Architecture, constraints: Vec<Constraint>) -> Self {
        Self {
            active_design: architecture,
            constraints,
            parameters: DesignParameters::default(),
            exploration_step: 0,
            exploration_metadata: ExplorationMetadata::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureAttributes {
    pub graph: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyEdge {
    pub from: DesignUnitId,
    pub to: DesignUnitId,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureGraph {
    pub nodes: Vec<DesignUnit>,
    pub edges: Vec<DependencyEdge>,
    pub attributes: ArchitectureAttributes,
}

impl ArchitectureGraph {
    pub fn from_architecture(architecture: &Architecture) -> Self {
        let nodes = architecture
            .classes
            .iter()
            .flat_map(|class_unit| class_unit.structures.iter())
            .flat_map(|structure| structure.design_units.iter().cloned())
            .collect();
        let edges = architecture
            .dependencies
            .iter()
            .map(|dependency| DependencyEdge {
                from: dependency.from,
                to: dependency.to,
                kind: dependency.kind,
            })
            .collect();

        Self {
            nodes,
            edges,
            attributes: ArchitectureAttributes::default(),
        }
    }

    pub fn remove_edge(&mut self, from: DesignUnitId, to: DesignUnitId) {
        self.edges.retain(|edge| !(edge.from == from && edge.to == to));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeUnit {
    pub id: u64,
    pub label: String,
    pub details: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeRelation {
    pub source: u64,
    pub target: u64,
    pub relation_type: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeGraph {
    pub knowledge_units: Vec<KnowledgeUnit>,
    pub relations: Vec<KnowledgeRelation>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub id: u64,
    pub action: String,
    pub state_id: SnapshotStateId,
    pub evaluation: EvaluationScore,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryGraph {
    pub episodes: Vec<Episode>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EvaluationScore {
    pub performance: f64,
    pub complexity: f64,
    pub maintainability: f64,
    pub correctness: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationState {
    pub scores: EvaluationScore,
    pub confidence: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlgorithmType {
    RuleBased,
    Heuristic,
    SearchOptimized,
    Custom(String),
}

impl Default for AlgorithmType {
    fn default() -> Self {
        Self::RuleBased
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryWorld {
    pub enabled: bool,
    pub positions: BTreeMap<DesignUnitId, Position>,
    pub sizes: BTreeMap<DesignUnitId, Size>,
    pub clusters: Vec<Vec<DesignUnitId>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MathWorld {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SnapshotStateId(pub u64);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionSequence {
    pub actions: Vec<DesignAction>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionTrace {
    pub actions: Vec<DesignAction>,
    pub states: Vec<SnapshotStateId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldTrace {
    pub action_trace: ActionTrace,
    pub evaluations: Vec<EvaluationState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldModelSnapshot {
    pub state_id: SnapshotStateId,
    pub model: Box<WorldModel>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArchitectureAction {
    AddComponent { component: DesignUnit },
    RemoveComponent { id: DesignUnitId },
    AddDependency {
        from: DesignUnitId,
        to: DesignUnitId,
        kind: DependencyKind,
    },
    RemoveDependency { from: DesignUnitId, to: DesignUnitId },
    SplitModule { id: DesignUnitId },
    MergeModule { target: DesignUnitId, source: DesignUnitId },
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodeAction {
    AddFunction { target: DesignUnitId, function_name: String },
    RemoveFunction { target: DesignUnitId, function_name: String },
    ModifyInterface { target: DesignUnitId, interface_name: String },
    RefactorModule { target: DesignUnitId, pattern: String },
    ReplaceAlgorithm {
        target: DesignUnitId,
        algorithm: AlgorithmType,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryAction {
    MoveNode { id: DesignUnitId, position: Position },
    ResizeNode { id: DesignUnitId, size: Size },
    AlignNodes { nodes: Vec<DesignUnitId> },
    ClusterNodes { nodes: Vec<DesignUnitId> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AlgorithmAction {
    ChangeAlgorithm {
        target: DesignUnitId,
        algorithm: AlgorithmType,
    },
    AdjustParameter {
        target: DesignUnitId,
        parameter: String,
        value: f64,
    },
    OptimizeStructure { target: DesignUnitId },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DesignAction {
    Architecture(ArchitectureAction),
    Code(CodeAction),
    Geometry(GeometryAction),
    Algorithm(AlgorithmAction),
}

impl Default for DesignAction {
    fn default() -> Self {
        Self::Algorithm(AlgorithmAction::OptimizeStructure {
            target: DesignUnitId(0),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldModel {
    pub design_state: DesignState,
    pub architecture_graph: ArchitectureGraph,
    pub knowledge_graph: KnowledgeGraph,
    pub memory_graph: MemoryGraph,
    pub evaluation_state: EvaluationState,
    pub geometry_world: Option<GeometryWorld>,
    pub math_world: Option<MathWorld>,
    pub trace: WorldTrace,
    pub snapshots: Vec<WorldModelSnapshot>,
}

impl WorldModel {
    pub fn new(
        design_state: DesignState,
        knowledge_graph: KnowledgeGraph,
        memory_graph: MemoryGraph,
        evaluation_state: EvaluationState,
    ) -> Self {
        let architecture_graph = ArchitectureGraph::from_architecture(&design_state.active_design);
        let mut world = Self {
            design_state,
            architecture_graph,
            knowledge_graph,
            memory_graph,
            evaluation_state,
            geometry_world: Some(GeometryWorld::default()),
            math_world: Some(MathWorld::default()),
            trace: WorldTrace::default(),
            snapshots: Vec::new(),
        };
        world.record_snapshot();
        world
    }

    pub fn from_architecture(architecture: Architecture, constraints: Vec<Constraint>) -> Self {
        Self::new(
            DesignState::from_architecture(architecture, constraints),
            KnowledgeGraph::default(),
            MemoryGraph::default(),
            EvaluationState::default(),
        )
    }

    pub fn current_state_id(&self) -> SnapshotStateId {
        SnapshotStateId(self.design_state.exploration_step)
    }

    pub fn generate_actions(&self) -> Vec<DesignAction> {
        let mut actions = Vec::new();
        let mut nodes = self.architecture_graph.nodes.clone();
        nodes.sort_by_key(|node| node.id.0);

        if let Some(first) = nodes.first() {
            actions.push(DesignAction::Architecture(
                ArchitectureAction::AddComponent {
                    component: DesignUnit::new(
                        nodes.last().map(|node| node.id.0 + 1).unwrap_or(1),
                        format!("Component{}", nodes.len() + 1),
                    ),
                },
            ));
            actions.push(DesignAction::Geometry(GeometryAction::MoveNode {
                id: first.id,
                position: Position {
                    x: self.design_state.exploration_step as f64 + 1.0,
                    y: first.id.0 as f64,
                },
            }));
            actions.push(DesignAction::Algorithm(AlgorithmAction::AdjustParameter {
                target: first.id,
                parameter: "search_weight".into(),
                value: self
                    .design_state
                    .parameters
                    .values
                    .get("search_weight")
                    .copied()
                    .unwrap_or(0.5)
                    + 0.1,
            }));
            if let Some(second) = nodes.get(1) {
                let edge_exists = self
                    .architecture_graph
                    .edges
                    .iter()
                    .any(|edge| edge.from == first.id && edge.to == second.id);
                if !edge_exists {
                    actions.push(DesignAction::Architecture(
                        ArchitectureAction::AddDependency {
                            from: first.id,
                            to: second.id,
                            kind: DependencyKind::Calls,
                        },
                    ));
                }
            }
        }

        actions
    }

    pub fn transition(&self, action: &DesignAction) -> Self {
        let mut next = self.clone();
        next.apply_action(action);
        next
    }

    pub fn apply(&mut self, action: &DesignAction) {
        self.apply_action(action);
    }

    pub fn apply_action(&mut self, action: &DesignAction) {
        match action {
            DesignAction::Architecture(action) => self.apply_architecture_action(action),
            DesignAction::Code(action) => self.apply_code_action(action),
            DesignAction::Geometry(action) => self.apply_geometry_action(action),
            DesignAction::Algorithm(action) => self.apply_algorithm_action(action),
        }

        self.design_state.exploration_step = self.design_state.exploration_step.saturating_add(1);
        self.architecture_graph = ArchitectureGraph::from_architecture(&self.design_state.active_design);
        self.trace.action_trace.actions.push(action.clone());
        self.record_snapshot();
    }

    pub fn apply_sequence(&self, sequence: &ActionSequence) -> Self {
        let mut current = self.clone();
        for action in &sequence.actions {
            current.apply_action(action);
        }
        current
    }

    pub fn simulate(&self, action: &DesignAction) -> WorldModel {
        self.simulate_action(action)
    }

    pub fn simulate_action(&self, action: &DesignAction) -> WorldModel {
        let mut candidate = self.transition(action);
        let world_state = WorldState::from_architecture(
            candidate.current_state_id().0,
            candidate.design_state.active_design.clone(),
            candidate.design_state.constraints.clone(),
        );
        let result = DefaultSimulationEngine.simulate(&world_state, None);
        candidate.update_evaluation_from_simulation(action, &result);
        candidate
    }

    pub fn simulate_sequence(&self, sequence: &ActionSequence) -> WorldModel {
        let mut current = self.clone();
        for action in &sequence.actions {
            current = current.simulate_action(action);
        }
        current
    }

    pub fn snapshot(&self) -> WorldModelSnapshot {
        WorldModelSnapshot {
            state_id: self.current_state_id(),
            model: Box::new(self.clone()),
        }
    }

    pub fn rollback(&self, state_id: SnapshotStateId) -> Option<Self> {
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.state_id == state_id)
            .map(|snapshot| snapshot.model.as_ref().clone())
    }

    fn apply_architecture_action(&mut self, action: &ArchitectureAction) {
        match action {
            ArchitectureAction::AddComponent { component } => {
                if !self
                    .design_state
                    .active_design
                    .all_design_unit_ids()
                    .contains(&component.id.0)
                {
                    self.design_state
                        .active_design
                        .add_design_unit(component.clone());
                }
            }
            ArchitectureAction::RemoveComponent { id } => {
                remove_component(&mut self.design_state.active_design, *id);
                if let Some(geometry_world) = &mut self.geometry_world {
                    geometry_world.positions.remove(id);
                    geometry_world.sizes.remove(id);
                }
            }
            ArchitectureAction::AddDependency { from, to, kind } => {
                let dependency = Dependency {
                    from: *from,
                    to: *to,
                    kind: *kind,
                };
                if !self.design_state.active_design.dependencies.contains(&dependency) {
                    self.design_state.active_design.dependencies.push(dependency);
                }
                let pair = (from.0, to.0);
                if !self.design_state.active_design.graph.edges.contains(&pair) {
                    self.design_state.active_design.graph.edges.push(pair);
                }
            }
            ArchitectureAction::RemoveDependency { from, to } => {
                self.design_state
                    .active_design
                    .dependencies
                    .retain(|dependency| !(dependency.from == *from && dependency.to == *to));
                self.design_state
                    .active_design
                    .graph
                    .edges
                    .retain(|edge| *edge != (from.0, to.0));
            }
            ArchitectureAction::SplitModule { .. } => {
                self.design_state
                    .exploration_metadata
                    .labels
                    .insert("last_architecture_op".into(), "split_module".into());
            }
            ArchitectureAction::MergeModule { .. } => {
                self.design_state
                    .exploration_metadata
                    .labels
                    .insert("last_architecture_op".into(), "merge_module".into());
            }
        }
    }

    fn apply_code_action(&mut self, action: &CodeAction) {
        let target = match action {
            CodeAction::AddFunction { target, function_name } => (*target, function_name.as_str()),
            CodeAction::RemoveFunction {
                target,
                function_name,
            } => (*target, function_name.as_str()),
            CodeAction::ModifyInterface {
                target,
                interface_name,
            } => (*target, interface_name.as_str()),
            CodeAction::RefactorModule { target, pattern } => (*target, pattern.as_str()),
            CodeAction::ReplaceAlgorithm { target, .. } => (*target, "replace_algorithm"),
        };

        self.design_state.exploration_metadata.labels.insert(
            format!("code_action_{}", target.0 .0),
            target.1.to_string(),
        );
    }

    fn apply_geometry_action(&mut self, action: &GeometryAction) {
        let geometry_world = self.geometry_world.get_or_insert_with(GeometryWorld::default);
        geometry_world.enabled = true;

        match action {
            GeometryAction::MoveNode { id, position } => {
                geometry_world.positions.insert(*id, *position);
            }
            GeometryAction::ResizeNode { id, size } => {
                geometry_world.sizes.insert(*id, *size);
            }
            GeometryAction::AlignNodes { nodes } => {
                if let Some(first) = nodes
                    .first()
                    .and_then(|id| geometry_world.positions.get(id))
                    .copied()
                {
                    for node in nodes {
                        geometry_world.positions.insert(
                            *node,
                            Position {
                                x: first.x,
                                y: first.y,
                            },
                        );
                    }
                }
            }
            GeometryAction::ClusterNodes { nodes } => {
                geometry_world.clusters.push(nodes.clone());
            }
        }
    }

    fn apply_algorithm_action(&mut self, action: &AlgorithmAction) {
        match action {
            AlgorithmAction::ChangeAlgorithm { target, algorithm } => {
                self.design_state.exploration_metadata.labels.insert(
                    format!("algorithm_{}", target.0),
                    format!("{algorithm:?}"),
                );
            }
            AlgorithmAction::AdjustParameter {
                target,
                parameter,
                value,
            } => {
                self.design_state
                    .parameters
                    .values
                    .insert(parameter.clone(), *value);
                self.design_state.exploration_metadata.labels.insert(
                    format!("parameter_target_{}", parameter),
                    target.0.to_string(),
                );
            }
            AlgorithmAction::OptimizeStructure { target } => {
                self.design_state.exploration_metadata.labels.insert(
                    "optimized_target".into(),
                    target.0.to_string(),
                );
            }
        }
    }

    fn update_evaluation_from_simulation(&mut self, action: &DesignAction, result: &SimulationResult) {
        self.evaluation_state = EvaluationState {
            scores: EvaluationScore {
                performance: result.performance_score,
                complexity: self.architecture_graph.edges.len() as f64
                    / self.architecture_graph.nodes.len().max(1) as f64,
                maintainability: (1.0 - result.execution.dependency_cost).clamp(0.0, 1.0),
                correctness: result.correctness_score,
            },
            confidence: result.confidence_score,
        };
        self.trace.evaluations.push(self.evaluation_state.clone());
        self.memory_graph.episodes.push(Episode {
            id: self.memory_graph.episodes.len() as u64 + 1,
            action: format!("{action:?}"),
            state_id: self.current_state_id(),
            evaluation: self.evaluation_state.scores,
        });
    }

    fn record_snapshot(&mut self) {
        let state_id = self.current_state_id();
        self.trace.action_trace.states.push(state_id);
        let snapshot_model = Self {
            design_state: self.design_state.clone(),
            architecture_graph: self.architecture_graph.clone(),
            knowledge_graph: self.knowledge_graph.clone(),
            memory_graph: self.memory_graph.clone(),
            evaluation_state: self.evaluation_state.clone(),
            geometry_world: self.geometry_world.clone(),
            math_world: self.math_world.clone(),
            trace: self.trace.clone(),
            snapshots: Vec::new(),
        };
        self.snapshots.push(WorldModelSnapshot {
            state_id,
            model: Box::new(snapshot_model),
        });
    }
}

fn remove_component(architecture: &mut Architecture, component_id: DesignUnitId) {
    for class_unit in &mut architecture.classes {
        for structure in &mut class_unit.structures {
            structure.design_units.retain(|unit| unit.id != component_id);
        }
    }

    architecture
        .dependencies
        .retain(|dependency| dependency.from != component_id && dependency.to != component_id);
    architecture
        .graph
        .edges
        .retain(|(from, to)| *from != component_id.0 && *to != component_id.0);
}

#[cfg(test)]
mod tests {
    use design_domain::Layer;

    use super::*;

    fn seeded_world() -> WorldModel {
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::with_layer(1, "ApiService", Layer::Service));
        architecture.add_design_unit(DesignUnit::with_layer(2, "UserRepository", Layer::Repository));
        WorldModel::from_architecture(architecture, Vec::new())
    }

    #[test]
    fn action_generation_is_deterministic() {
        let world = seeded_world();

        let left = world.generate_actions();
        let right = world.generate_actions();

        assert_eq!(left, right);
        assert!(!left.is_empty());
    }

    #[test]
    fn transition_keeps_state_and_trace_consistent() {
        let world = seeded_world();
        let action = DesignAction::Architecture(ArchitectureAction::AddDependency {
            from: DesignUnitId(1),
            to: DesignUnitId(2),
            kind: DependencyKind::Calls,
        });

        let next = world.transition(&action);

        assert_eq!(next.current_state_id(), SnapshotStateId(1));
        assert_eq!(next.architecture_graph.edges.len(), 1);
        assert_eq!(next.trace.action_trace.actions, vec![action]);
        assert_eq!(next.trace.action_trace.states, vec![SnapshotStateId(0), SnapshotStateId(1)]);
    }

    #[test]
    fn rollback_restores_prior_snapshot() {
        let world = seeded_world();
        let sequence = ActionSequence {
            actions: vec![
                DesignAction::Architecture(ArchitectureAction::AddComponent {
                    component: DesignUnit::with_layer(3, "AuditStore", Layer::Database),
                }),
                DesignAction::Geometry(GeometryAction::MoveNode {
                    id: DesignUnitId(1),
                    position: Position { x: 10.0, y: 4.0 },
                }),
            ],
        };

        let advanced = world.apply_sequence(&sequence);
        let restored = advanced
            .rollback(SnapshotStateId(0))
            .expect("initial snapshot must exist");

        assert_eq!(restored.current_state_id(), SnapshotStateId(0));
        assert_eq!(restored.architecture_graph.nodes.len(), 2);
        assert!(restored
            .geometry_world
            .as_ref()
            .expect("geometry world")
            .positions
            .is_empty());
    }

    #[test]
    fn search_compatibility_expands_actions_into_next_states() {
        let world = seeded_world();
        let actions = world.generate_actions();

        let next_states = actions
            .iter()
            .map(|action| world.simulate_action(action))
            .collect::<Vec<_>>();

        assert_eq!(next_states.len(), actions.len());
        assert!(next_states.iter().all(|state| state.current_state_id() == SnapshotStateId(1)));
        assert!(next_states
            .iter()
            .all(|state| state.evaluation_state.confidence >= 0.0));
    }
}
