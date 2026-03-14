use std::collections::BTreeMap;
use std::thread::{self, JoinHandle};

use code_ir::{
    ArchitectureToCodeIR, CodeIR, CodeState as GeneratedCodeState,
    DeterministicArchitectureToCodeIR,
};

use crate::domain::state::{AppState, DesignScoreVector, UnifiedDesignState};
use crate::domain::transaction::{ProposedDiff, TxError};

pub type NodeId = String;
pub type EdgeId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Specification {
    pub title: String,
    pub nodes: Vec<(NodeId, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureState {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryState {
    pub node_positions: BTreeMap<NodeId, Position>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeState {
    pub code_ir: CodeIR,
    pub modules: Vec<code_ir::CodeModule>,
    pub metrics: code_ir::CodeMetrics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldState {
    pub architecture: ArchitectureState,
    pub geometry: GeometryState,
    pub code: CodeState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeType {
    Service,
    Worker,
    Api,
    Adapter,
    Library,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchitectureAction {
    AddNode { node_type: NodeType },
    RemoveNode { node_id: NodeId },
    AddEdge { from: NodeId, to: NodeId },
    RemoveEdge { edge_id: EdgeId },
    UpdateProperty { node_id: NodeId, property: Property },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionHistory {
    pub undo_stack: Vec<ArchitectureAction>,
    pub redo_stack: Vec<ArchitectureAction>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReasoningEvent {
    #[default]
    SearchStarted,
    CandidateGenerated,
    CandidateEvaluated,
    ParetoUpdated,
    SearchCompleted,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationMetrics {
    pub consistency: u32,
    pub structural_integrity: u32,
    pub dependency_soundness: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReasoningTrace {
    pub decision: String,
    pub metrics: EvaluationMetrics,
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    SpecSubmit,
    NodeAdd,
    NodeRemove,
    EdgeAdd,
    EdgeRemove,
    GenerateCode,
    EvaluateArchitecture,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionResult {
    pub world_state: WorldState,
    pub reasoning_trace: Option<ReasoningTrace>,
    pub reasoning_events: Vec<ReasoningEvent>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionLayer;

impl InteractionLayer {
    pub fn generate_architecture(spec: Specification) -> WorldState {
        let mut uds = UnifiedDesignState::default();
        for (key, value) in spec.nodes {
            uds.nodes.insert(key, value);
        }
        Self::world_state_from_uds(&uds)
    }

    pub fn update_architecture(
        app_state: &mut AppState,
        action: ArchitectureAction,
        history: &mut ActionHistory,
    ) -> Result<InteractionResult, TxError> {
        let diff = Self::diff_from_action(app_state, &action)?;
        app_state.begin_tx()?;
        if let Err(err) = app_state.apply_diff(diff.clone()) {
            let _ = app_state.abort_tx();
            return Err(err);
        }
        if let Err(err) = app_state.commit_tx() {
            let _ = app_state.abort_tx();
            return Err(err);
        }

        history.undo_stack.push(action);
        history.redo_stack.clear();
        let world_state = Self::world_state_from_app_state(app_state);
        let metrics = Self::evaluate_architecture(&world_state);
        let reasoning_trace = Some(Self::reasoning_trace_for_diff(&diff, metrics.clone()));

        Ok(InteractionResult {
            world_state,
            reasoning_trace,
            reasoning_events: vec![
                ReasoningEvent::SearchStarted,
                ReasoningEvent::CandidateGenerated,
                ReasoningEvent::CandidateEvaluated,
                ReasoningEvent::ParetoUpdated,
                ReasoningEvent::SearchCompleted,
            ],
        })
    }

    pub fn evaluate_architecture(state: &WorldState) -> EvaluationMetrics {
        let module_count = state.architecture.nodes.len() as u32;
        let edge_count = state.architecture.edges.len() as u32;
        let orphan_count = state
            .architecture
            .nodes
            .keys()
            .filter(|node_id| {
                !state
                    .architecture
                    .edges
                    .iter()
                    .any(|edge| &edge.from == *node_id || &edge.to == *node_id)
            })
            .count() as u32;

        EvaluationMetrics {
            consistency: (100_u32.saturating_sub(orphan_count.saturating_mul(10))).min(100),
            structural_integrity: (100_u32
                .saturating_sub(edge_count.saturating_sub(module_count.saturating_sub(1)) * 5))
                .min(100),
            dependency_soundness: (100_u32
                .saturating_sub(Self::cycle_count(&state.architecture) as u32 * 20))
                .min(100),
        }
    }

    pub fn generate_code(state: &WorldState) -> CodeState {
        let mut architecture = design_domain::Architecture::seeded();
        architecture.classes.clear();
        architecture
            .classes
            .push(design_domain::ClassUnit::new(1, "InteractionArchitecture"));
        architecture.classes[0]
            .structures
            .push(design_domain::StructureUnit::new(1, "interaction_graph"));
        for node in state.architecture.nodes.values() {
            architecture.add_design_unit(design_domain::DesignUnit::new(
                node.id.parse::<u64>().unwrap_or(0),
                node.properties
                    .get("label")
                    .cloned()
                    .unwrap_or_else(|| node.id.clone()),
            ));
        }
        for edge in &state.architecture.edges {
            let from = edge.from.parse::<u64>().unwrap_or(0);
            let to = edge.to.parse::<u64>().unwrap_or(0);
            architecture.dependencies.push(design_domain::Dependency {
                from: design_domain::DesignUnitId(from),
                to: design_domain::DesignUnitId(to),
                kind: design_domain::DependencyKind::Calls,
            });
            architecture.graph.edges.push((from, to));
        }

        let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);
        let GeneratedCodeState { metrics, .. } = code_ir.code_state();

        CodeState {
            modules: code_ir.modules.clone(),
            metrics,
            code_ir,
        }
    }

    pub fn world_state_from_app_state(app_state: &AppState) -> WorldState {
        Self::world_state_from_uds(&app_state.uds)
    }

    pub fn handle_ui_event(
        app_state: &mut AppState,
        history: &mut ActionHistory,
        event: UiEvent,
    ) -> Result<InteractionResult, TxError> {
        match event {
            UiEvent::GenerateCode | UiEvent::EvaluateArchitecture | UiEvent::SpecSubmit => {
                let world_state = Self::world_state_from_app_state(app_state);
                let metrics = Self::evaluate_architecture(&world_state);
                Ok(InteractionResult {
                    world_state,
                    reasoning_trace: Some(ReasoningTrace {
                        decision: format!("{event:?}"),
                        metrics,
                        constraints: vec!["single_source_of_truth".into()],
                    }),
                    reasoning_events: vec![ReasoningEvent::SearchCompleted],
                })
            }
            UiEvent::NodeAdd => Self::update_architecture(
                app_state,
                ArchitectureAction::AddNode {
                    node_type: NodeType::Service,
                },
                history,
            ),
            UiEvent::NodeRemove => {
                let node_id = app_state
                    .uds
                    .nodes
                    .keys()
                    .next_back()
                    .cloned()
                    .unwrap_or_else(|| "1".into());
                Self::update_architecture(
                    app_state,
                    ArchitectureAction::RemoveNode { node_id },
                    history,
                )
            }
            UiEvent::EdgeAdd => {
                let mut keys = app_state.uds.nodes.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                let from = keys.first().cloned().unwrap_or_else(|| "1".into());
                let to = keys.get(1).cloned().unwrap_or_else(|| from.clone());
                Self::update_architecture(app_state, ArchitectureAction::AddEdge { from, to }, history)
            }
            UiEvent::EdgeRemove => {
                let world = Self::world_state_from_app_state(app_state);
                let edge_id = world
                    .architecture
                    .edges
                    .first()
                    .map(|edge| edge.id.clone())
                    .unwrap_or_else(|| "1->1".into());
                Self::update_architecture(
                    app_state,
                    ArchitectureAction::RemoveEdge { edge_id },
                    history,
                )
            }
        }
    }

    pub fn spawn_reasoning_task(app_state: AppState) -> JoinHandle<Vec<ReasoningEvent>> {
        thread::spawn(move || {
            let world = Self::world_state_from_app_state(&app_state);
            let _ = Self::evaluate_architecture(&world);
            vec![
                ReasoningEvent::SearchStarted,
                ReasoningEvent::CandidateGenerated,
                ReasoningEvent::CandidateEvaluated,
                ReasoningEvent::ParetoUpdated,
                ReasoningEvent::SearchCompleted,
            ]
        })
    }

    fn world_state_from_uds(uds: &UnifiedDesignState) -> WorldState {
        let architecture = Self::architecture_from_uds(uds);
        let geometry = Self::compute_layout(&architecture);
        let code = Self::generate_code(&WorldState {
            architecture: architecture.clone(),
            geometry: geometry.clone(),
            code: CodeState::default(),
        });

        WorldState {
            architecture,
            geometry,
            code,
        }
    }

    fn architecture_from_uds(uds: &UnifiedDesignState) -> ArchitectureState {
        let mut nodes = BTreeMap::new();
        for (key, value) in &uds.nodes {
            nodes.insert(
                key.clone(),
                Node {
                    id: key.clone(),
                    node_type: infer_node_type(value),
                    properties: BTreeMap::from([
                        ("label".into(), value.clone()),
                        ("value".into(), value.clone()),
                    ]),
                },
            );
        }

        let mut edges = Vec::new();
        for (from, tos) in &uds.dependencies {
            for to in tos {
                edges.push(Edge {
                    id: format!("{from}->{to}"),
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        edges.sort_by(|left, right| left.id.cmp(&right.id));

        ArchitectureState { nodes, edges }
    }

    fn compute_layout(architecture: &ArchitectureState) -> GeometryState {
        let mut node_positions = BTreeMap::new();
        for (index, node_id) in architecture.nodes.keys().enumerate() {
            node_positions.insert(
                node_id.clone(),
                Position {
                    x: (index % 4) as f32 * 160.0,
                    y: (index / 4) as f32 * 120.0,
                },
            );
        }
        GeometryState { node_positions }
    }

    fn diff_from_action(app_state: &AppState, action: &ArchitectureAction) -> Result<ProposedDiff, TxError> {
        match action {
            ArchitectureAction::AddNode { node_type } => {
                let next_id = app_state
                    .uds
                    .nodes
                    .keys()
                    .filter_map(|key| key.parse::<u64>().ok())
                    .max()
                    .unwrap_or(0)
                    + 1;
                Ok(ProposedDiff::UpsertNode {
                    key: next_id.to_string(),
                    value: format!("{node_type:?}"),
                })
            }
            ArchitectureAction::RemoveNode { node_id } => Ok(ProposedDiff::RemoveNode {
                key: node_id.clone(),
            }),
            ArchitectureAction::AddEdge { from, to } => {
                let mut dependencies = app_state.uds.dependencies.get(from).cloned().unwrap_or_default();
                dependencies.push(to.clone());
                dependencies.sort();
                dependencies.dedup();
                Ok(ProposedDiff::SetDependencies {
                    key: from.clone(),
                    dependencies,
                })
            }
            ArchitectureAction::RemoveEdge { edge_id } => {
                let (from, to) = edge_id
                    .split_once("->")
                    .ok_or_else(|| TxError::MissingDependency(edge_id.clone()))?;
                let dependencies = app_state
                    .uds
                    .dependencies
                    .get(from)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|dep| dep != to)
                    .collect::<Vec<_>>();
                Ok(ProposedDiff::SetDependencies {
                    key: from.to_string(),
                    dependencies,
                })
            }
            ArchitectureAction::UpdateProperty { node_id, property } => Ok(ProposedDiff::UpsertNode {
                key: node_id.clone(),
                value: format!("{}={}", property.key, property.value),
            }),
        }
    }

    fn reasoning_trace_for_diff(diff: &ProposedDiff, metrics: EvaluationMetrics) -> ReasoningTrace {
        ReasoningTrace {
            decision: format!("applied {diff:?}"),
            metrics,
            constraints: vec![
                "single_source_of_truth".into(),
                "incremental_update".into(),
                "human_in_the_loop".into(),
            ],
        }
    }

    fn cycle_count(architecture: &ArchitectureState) -> usize {
        let mut visited = Vec::new();
        let mut stack = Vec::new();
        let mut cycles = 0;
        for node_id in architecture.nodes.keys() {
            if Self::dfs_cycle(node_id, architecture, &mut visited, &mut stack, &mut cycles) {
                stack.clear();
            }
        }
        cycles
    }

    fn dfs_cycle(
        node_id: &str,
        architecture: &ArchitectureState,
        visited: &mut Vec<String>,
        stack: &mut Vec<String>,
        cycles: &mut usize,
    ) -> bool {
        if stack.iter().any(|entry| entry == node_id) {
            *cycles += 1;
            return true;
        }
        if visited.iter().any(|entry| entry == node_id) {
            return false;
        }
        visited.push(node_id.to_string());
        stack.push(node_id.to_string());
        let mut found = false;
        for edge in architecture.edges.iter().filter(|edge| edge.from == node_id) {
            found |= Self::dfs_cycle(&edge.to, architecture, visited, stack, cycles);
        }
        stack.pop();
        found
    }
}

fn infer_node_type(value: &str) -> NodeType {
    let lower = value.to_ascii_lowercase();
    if lower.contains("worker") {
        NodeType::Worker
    } else if lower.contains("api") || lower.contains("controller") {
        NodeType::Api
    } else if lower.contains("adapter") || lower.contains("repository") {
        NodeType::Adapter
    } else if lower.contains("library") {
        NodeType::Library
    } else {
        NodeType::Service
    }
}

impl From<DesignScoreVector> for EvaluationMetrics {
    fn from(value: DesignScoreVector) -> Self {
        Self {
            consistency: value.consistency,
            structural_integrity: value.structural_integrity,
            dependency_soundness: value.dependency_soundness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_app_state() -> AppState {
        let mut uds = UnifiedDesignState::default();
        uds.nodes.insert("1".into(), "ApiService".into());
        uds.nodes.insert("2".into(), "UserRepository".into());
        uds.dependencies.insert("1".into(), vec!["2".into()]);
        AppState::new(uds)
    }

    #[test]
    fn architecture_action_updates_world_state_and_history() {
        let mut app = sample_app_state();
        let mut history = ActionHistory::default();

        let result = InteractionLayer::update_architecture(
            &mut app,
            ArchitectureAction::AddNode {
                node_type: NodeType::Worker,
            },
            &mut history,
        )
        .expect("action should apply");

        assert_eq!(history.undo_stack.len(), 1);
        assert!(result.world_state.architecture.nodes.len() >= 3);
        assert_eq!(
            result.reasoning_events.first(),
            Some(&ReasoningEvent::SearchStarted)
        );
        assert_eq!(
            result.reasoning_events.last(),
            Some(&ReasoningEvent::SearchCompleted)
        );
    }

    #[test]
    fn generated_code_reflects_world_state_modules() {
        let world = InteractionLayer::world_state_from_app_state(&sample_app_state());
        let code = InteractionLayer::generate_code(&world);

        assert_eq!(code.modules.len(), 2);
        assert_eq!(code.metrics.module_count, 2);
    }

    #[test]
    fn reasoning_task_event_order_is_deterministic() {
        let app = sample_app_state();
        let left = InteractionLayer::spawn_reasoning_task(app.clone())
            .join()
            .expect("thread should finish");
        let right = InteractionLayer::spawn_reasoning_task(app)
            .join()
            .expect("thread should finish");

        assert_eq!(left, right);
        assert_eq!(left.len(), 5);
    }
}
